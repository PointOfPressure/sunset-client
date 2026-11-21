//! Plugins: native mod packages (`.hmod` files or folders) built into Sunset on this computer.
//!
//! Sunset ships as a signed prebuilt version folder. A plugin compiles into that same game, so turning plugins on
//! builds a second version folder on this computer: Sunset's own `.hmod` package (the release's signed "source"
//! artifact) and every installed plugin, staged side by side and compiled by the stock horizon-launcher's
//! `native-build` against the pinned Horizon Nightly (with the signed "sdk" artifact when Sunset's hooks need the
//! Horizon fork's SDK). The instance then plays that build; turning plugins off, or removing the last one, points it
//! back at the signed version, which is never touched.
//!
//! Files, all under the launcher's data folder:
//! - `plugins/packages/<id>/`: each installed plugin, unpacked (the package's flat layout).
//! - `plugins/build.json`: the build the instance plays when plugins are on ([`Build`]); absent when it plays clean.
//! - `plugins/stage/`: the last build's input; `plugins/build.log`: the whole output of the last build.
//! - `kit/`: Sunset's package source and the fork SDK, unpacked from their signed artifacts, kept for rebuilds.

use crate::core::{self, Manifest, Paths, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The largest plugin package accepted, unpacked.
const MAX_PACKAGE: u64 = 64 << 20;
/// Folders of a plugin project that are never part of its package.
const SKIP: [&str; 4] = ["target", ".horizon", ".git", ".horizon-mixins-stage"];

// ---------------------------------------------------------------- installed plugins

/// An installed plugin, as its `horizon-mod.toml` names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Its `[native] horizon` pin: the Horizon build it is made for.
    pub horizon: String,
    /// SHA-256 over the package's files: a changed package needs a new build.
    pub hash: String,
    pub dir: PathBuf,
}

#[derive(Deserialize)]
struct ModToml {
    id: String,
    #[serde(default)]
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    native: Option<NativeToml>,
}

#[derive(Deserialize)]
struct NativeToml {
    horizon: String,
    #[serde(default)]
    game: String,
}

pub fn dir(p: &Paths) -> PathBuf {
    p.root.join("plugins")
}

fn packages(p: &Paths) -> PathBuf {
    dir(p).join("packages")
}

pub fn log_path(p: &Paths) -> PathBuf {
    dir(p).join("build.log")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Reads and checks a package folder: a native `.hmod` layout (horizon-mod.toml with `[native]`, Cargo.toml, src/).
fn read_package(dir: &Path) -> Result<Plugin> {
    let text = fs::read_to_string(dir.join("horizon-mod.toml")).map_err(|_| "it has no horizon-mod.toml: pick a plugin's folder or its .hmod file".to_string())?;
    let m: ModToml = toml::from_str(&text).map_err(|e| format!("its horizon-mod.toml can't be read: {}", e.message()))?;
    if !valid_id(&m.id) {
        return Err(format!("its id \"{}\" isn't a mod id (lower case letters, digits and _)", m.id));
    }
    if m.id == "sunset" {
        return Err("that is Sunset itself, not a plugin".into());
    }
    let native = m.native.ok_or("it isn't a native mod (no [native] in horizon-mod.toml), and plugins are built into Sunset")?;
    if !dir.join("Cargo.toml").is_file() || !dir.join("src").is_dir() {
        return Err("it has no Cargo.toml or src/ folder: a native plugin is a Rust package".into());
    }
    let _ = native.game;
    Ok(Plugin {
        name: if m.name.trim().is_empty() { m.id.clone() } else { m.name.trim().to_string() },
        id: m.id,
        version: m.version,
        description: m.description.trim().to_string(),
        horizon: native.horizon,
        hash: tree_hash(dir)?,
        dir: dir.to_path_buf(),
    })
}

/// SHA-256 over every file's relative path and contents, in path order, skipping build output.
fn tree_hash(root: &Path) -> Result<String> {
    let mut files = vec![];
    walk(root, Path::new(""), &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    files.sort();
    let mut h = Sha256::new();
    for rel in files {
        let key = rel.to_string_lossy().replace('\\', "/");
        h.update((key.len() as u64).to_le_bytes());
        h.update(key.as_bytes());
        let bytes = fs::read(root.join(&rel)).map_err(|e| format!("{}: {e}", rel.display()))?;
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn walk(root: &Path, rel: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in fs::read_dir(root.join(rel))? {
        let e = e?;
        let name = e.file_name();
        let t = e.file_type()?;
        let r = rel.join(&name);
        if t.is_dir() {
            if rel.as_os_str().is_empty() && SKIP.iter().any(|s| name == *s) {
                continue;
            }
            walk(root, &r, out)?;
        } else if t.is_file() {
            out.push(r);
        }
    }
    Ok(())
}

/// The installed plugins, by id. A folder that no longer reads as a package is left out.
pub fn list(p: &Paths) -> Vec<Plugin> {
    let mut v: Vec<Plugin> = fs::read_dir(packages(p)).into_iter().flatten().flatten().filter_map(|e| read_package(&e.path()).ok()).collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Installs a plugin from a package folder or an `.hmod` file, replacing an installed one with the same id. It must be
/// made for the Horizon build the installed Sunset runs on.
pub fn add(p: &Paths, from: &Path, sunset: &Manifest) -> Result<Plugin> {
    let store = packages(p);
    fs::create_dir_all(&store).map_err(|e| e.to_string())?;
    let tmp = store.join(".incoming");
    let _ = fs::remove_dir_all(&tmp);
    let staged = if from.is_dir() {
        copy_package(from, &tmp).map(|_| ())
    } else if from.extension().is_some_and(|e| e.eq_ignore_ascii_case("hmod") || e.eq_ignore_ascii_case("zip")) {
        unzip_package(from, &tmp)
    } else {
        Err("pick a plugin's folder or its .hmod file".to_string())
    };
    let checked = staged.and_then(|_| read_package(&tmp)).and_then(|pl| {
        if pl.horizon != sunset.horizon.revision {
            let short = |s: &str| s.chars().take(7).collect::<String>();
            Err(format!(
                "it is made for Horizon {} and this Sunset runs on {}: ask its author for a version for Sunset {}",
                short(&pl.horizon),
                short(&sunset.horizon.revision),
                sunset.version
            ))
        } else {
            Ok(pl)
        }
    });
    let pl = match checked {
        Ok(pl) => pl,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            return Err(format!("{} can't be added: {e}", from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        }
    };
    let dest = store.join(&pl.id);
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&tmp, &dest).map_err(|e| format!("can't install {}: {e}", pl.id))?;
    read_package(&dest)
}

pub fn remove(p: &Paths, id: &str) -> Result<()> {
    if !valid_id(id) || !packages(p).join(id).is_dir() {
        return Err(format!("no plugin {id} is installed"));
    }
    fs::remove_dir_all(packages(p).join(id)).map_err(|e| format!("can't remove {id}: {e}"))
}

/// Copies a package folder without its build output; returns the bytes copied.
fn copy_package(from: &Path, to: &Path) -> Result<u64> {
    let mut files = vec![];
    walk(from, Path::new(""), &mut files).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut total = 0;
    for rel in files {
        total += fs::metadata(from.join(&rel)).map(|m| m.len()).unwrap_or(0);
        if total > MAX_PACKAGE {
            return Err(format!("it is larger than {} MB", MAX_PACKAGE >> 20));
        }
        let dest = to.join(&rel);
        fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(from.join(&rel), &dest).map_err(|e| format!("{}: {e}", rel.display()))?;
    }
    Ok(total)
}

/// An `.hmod` is a zip of the package's flat layout (no top folder). Names outside it are refused.
fn unzip_package(file: &Path, to: &Path) -> Result<()> {
    let f = fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("it isn't an .hmod archive ({e})"))?;
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { return Err(format!("it holds an unsafe path {:?}", entry.name())) };
        let dest = to.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            continue;
        }
        total += entry.size();
        if total > MAX_PACKAGE {
            return Err(format!("it is larger than {} MB unpacked", MAX_PACKAGE >> 20));
        }
        fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut (&mut entry).take(MAX_PACKAGE), &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------- the build the instance plays

/// A plugin build of Sunset, as `plugins/build.json` records it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Build {
    /// The version folder native-build made, which the instance plays.
    pub version_dir: String,
    /// The signed Sunset version folder it was built for.
    pub base: String,
    pub sunset_version: String,
    pub plugins: Vec<Built>,
    /// Unix seconds.
    pub built: u64,
    /// How long the compile took, for the next build's progress estimate.
    #[serde(default)]
    pub compile_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Built {
    pub id: String,
    pub version: String,
    pub hash: String,
}

fn record_path(p: &Paths) -> PathBuf {
    dir(p).join("build.json")
}

pub fn record(p: &Paths) -> Option<Build> {
    serde_json::from_slice(&fs::read(record_path(p)).ok()?).ok()
}

/// The version folder the instance plays, from the stock CLI's instances.json.
pub fn instance_version(p: &Paths) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(p.horizon().join("instances.json")).ok()?).ok()?;
    v["instances"][core::INSTANCE]["version"].as_str().map(str::to_string)
}

/// What the instance plays, against what the settings and the installed plugins ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Plugins are off (or none is installed) and the instance plays the signed build.
    Clean,
    /// The instance plays a build with exactly the installed plugins.
    Built(Build),
    /// Plugins are on, and the instance doesn't play a build of exactly these plugins yet.
    NeedsBuild,
    /// Plugins are off or all removed, and the instance still plays a plugin build.
    NeedsClean,
}

pub fn status(p: &Paths, m: &Manifest, enabled: bool, installed: &[Plugin]) -> Status {
    let playing = instance_version(p);
    let want = enabled && !installed.is_empty();
    if !want {
        return if playing.as_deref() == Some(m.version_dir.as_str()) { Status::Clean } else { Status::NeedsClean };
    }
    match record(p) {
        Some(b) if b.base == m.version_dir
            && b.sunset_version == m.version
            && playing.as_deref() == Some(b.version_dir.as_str())
            && p.versions().join(&b.version_dir).is_dir()
            && same_plugins(&b.plugins, installed) =>
        {
            Status::Built(b)
        }
        _ => Status::NeedsBuild,
    }
}

fn same_plugins(built: &[Built], installed: &[Plugin]) -> bool {
    built.len() == installed.len() && installed.iter().all(|i| built.iter().any(|b| b.id == i.id && b.hash == i.hash))
}

/// Points the instance back at the signed build and removes the plugin build.
pub fn clean(p: &Paths, m: &Manifest) -> Result<()> {
    core::run_cli(p, &["instance", "version", core::INSTANCE, &m.version_dir])?;
    if let Some(b) = record(p)
        && b.version_dir != m.version_dir {
            let _ = fs::remove_dir_all(p.versions().join(&b.version_dir));
        }
    match fs::remove_file(record_path(p)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("can't remove {}: {e}", record_path(p).display())),
        _ => Ok(()),
    }
}

/// What `sync` did.
#[derive(Debug)]
pub enum Synced {
    /// The instance already played what was asked for.
    Unchanged,
    /// It plays the signed build again.
    Cleaned,
    Built(Build),
}

/// Brings the instance in line with the settings and the installed plugins: builds Sunset with them, or goes back to
/// the signed build when plugins are off or none is left.
pub fn sync(p: &Paths, m: &Manifest, enabled: bool, cancel: &AtomicBool, report: StepReport) -> std::result::Result<Synced, Failure> {
    let installed = list(p);
    match status(p, m, enabled, &installed) {
        Status::Clean | Status::Built(_) => Ok(Synced::Unchanged),
        Status::NeedsClean => {
            report(Phase::Installing, "Going back to Sunset as released", Some(0.4));
            clean(p, m).map_err(|e| Failure::new(Phase::Installing, e))?;
            report(Phase::Installing, "Done", Some(1.0));
            Ok(Synced::Cleaned)
        }
        Status::NeedsBuild => rebuild(p, m, &installed, cancel, report).map(Synced::Built),
    }
}

// ---------------------------------------------------------------- rebuild

/// The steps a plugin build goes through, in order, as the Plugins tab lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Sunset's package and the build tools fetched, the packages copied side by side.
    Staging,
    /// native-build compiles Sunset and the plugins against the pinned Horizon.
    Compiling,
    /// native-build links the game and writes its version folder; its contents are checked.
    Packaging,
    /// The instance switched to the new build, the old one removed.
    Installing,
}

pub const PHASES: [Phase; 4] = [Phase::Staging, Phase::Compiling, Phase::Packaging, Phase::Installing];

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Staging => "Staging",
            Phase::Compiling => "Compiling",
            Phase::Packaging => "Packaging",
            Phase::Installing => "Installing",
        }
    }
    /// The overall bar's share before this step, and this step's own share of it.
    fn span(self) -> (f32, f32) {
        match self {
            Phase::Staging => (0.0, 0.12),
            Phase::Compiling => (0.12, 0.76),
            Phase::Packaging => (0.88, 0.07),
            Phase::Installing => (0.95, 0.05),
        }
    }
    /// How far the whole build is when this step is `f` through.
    pub fn overall(self, f: f32) -> f32 {
        let (start, len) = self.span();
        start + len * f.clamp(0., 1.)
    }
}

/// A build step's report: which step, what it is doing now, and how far through that step (None: can't tell).
pub type StepReport<'a> = &'a dyn Fn(Phase, &str, Option<f32>);

/// A failed build: the step it stopped in and why.
#[derive(Clone, Debug)]
pub struct Failure {
    pub phase: Phase,
    pub error: String,
}

impl Failure {
    fn new(phase: Phase, error: impl Into<String>) -> Self {
        Failure { phase, error: error.into() }
    }
}

/// Builds Sunset with `plugins` into a new version folder and makes the instance play it. On failure or cancel the
/// instance is left on what it played before, and whatever version folder the failed build wrote is removed.
pub fn rebuild(p: &Paths, m: &Manifest, plugins: &[Plugin], cancel: &AtomicBool, report: StepReport) -> std::result::Result<Build, Failure> {
    let before = instance_version(p);
    let had = native_dirs(p);
    let r = build(p, m, plugins, cancel, report);
    if r.is_err() {
        // native-build selects its build as it finishes; a failure after that (or a check of ours) puts the instance back.
        if let Some(v) = before.filter(|v| instance_version(p).as_deref() != Some(v.as_str())) {
            let _ = core::run_cli(p, &["instance", "version", core::INSTANCE, &v]);
        }
        remove_new_builds(p, m, &had);
    }
    r
}

/// The `native-*` version folders in the game's data.
fn native_dirs(p: &Paths) -> std::collections::BTreeSet<String> {
    fs::read_dir(p.versions())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("native-"))
        .collect()
}

/// Removes the version folders a failed build added (a whole build, or what a cancel left of one), leaving every
/// folder that was there before it, the signed build, the recorded build and whatever the instance plays now.
fn remove_new_builds(p: &Paths, m: &Manifest, had: &std::collections::BTreeSet<String>) {
    let keep = [Some(m.version_dir.clone()), record(p).map(|b| b.version_dir), instance_version(p)];
    for d in native_dirs(p).difference(had) {
        if !keep.iter().flatten().any(|k| k == d) {
            let _ = fs::remove_dir_all(p.versions().join(d));
        }
    }
}

fn build(p: &Paths, m: &Manifest, plugins: &[Plugin], cancel: &AtomicBool, report: StepReport) -> std::result::Result<Build, Failure> {
    let stop = |phase| if cancel.load(Ordering::Relaxed) { Err(Failure::new(phase, "cancelled")) } else { Ok(()) };
    let staging = |msg: &str, f: Option<f32>| report(Phase::Staging, msg, f);
    staging("Checking the plugins", Some(0.0));
    if plugins.is_empty() {
        return Err(Failure::new(Phase::Staging, "no plugin is installed"));
    }
    for pl in plugins {
        if pl.horizon != m.horizon.revision {
            return Err(Failure::new(Phase::Staging, format!("{} is made for another Horizon build than Sunset {}: remove it or install a version for this Sunset", pl.name, m.version)));
        }
    }
    core::cli_path(p).ok_or_else(|| Failure::new(Phase::Staging, "horizon-launcher is not installed yet: install Sunset first"))?;
    let kit = kit(p, m, cancel, &staging).map_err(|e| Failure::new(Phase::Staging, e))?;
    stop(Phase::Staging)?;

    staging("Copying Sunset and the plugins into the build folder", Some(0.9));
    let stage = dir(p).join("stage");
    let _ = fs::remove_dir_all(&stage);
    let copy = |from: &Path, id: &str| copy_package(from, &stage.join(id)).map_err(|e| Failure::new(Phase::Staging, format!("staging {id}: {e}")));
    copy(&kit.sunset, "sunset")?;
    for pl in plugins {
        copy(&pl.dir, &pl.id)?;
    }
    stop(Phase::Staging)?;

    let estimate = record(p).map(|b| b.compile_secs).filter(|s| *s > 0).unwrap_or(240);
    let out = native_build(p, &stage, &m.horizon.tag, kit.sdk_base.as_deref(), estimate, cancel, report)?;

    report(Phase::Packaging, "Checking the new build", Some(0.7));
    let vdir = out.version_dir;
    let built = core::built_in_mods(p, &vdir);
    let left_out = left_out(p, &vdir);
    if let Some(why) = left_out.first() {
        return Err(Failure::new(Phase::Packaging, why.clone()));
    }
    let mut missing: Vec<&str> = plugins.iter().filter(|pl| !built.iter().any(|(id, _)| *id == pl.id)).map(|pl| pl.name.as_str()).collect();
    if !built.iter().any(|(id, _)| id == "sunset") {
        missing.insert(0, "Sunset");
    }
    if !missing.is_empty() {
        return Err(Failure::new(Phase::Packaging, format!("the build doesn't hold {}", missing.join(", "))));
    }
    stop(Phase::Packaging)?;

    report(Phase::Installing, "Switching the instance to the new build", Some(0.3));
    core::run_cli(p, &["instance", "version", core::INSTANCE, &vdir]).map_err(|e| Failure::new(Phase::Installing, e))?;
    let rec = Build {
        version_dir: vdir.clone(),
        base: m.version_dir.clone(),
        sunset_version: m.version.clone(),
        plugins: plugins.iter().map(|pl| Built { id: pl.id.clone(), version: pl.version.clone(), hash: pl.hash.clone() }).collect(),
        built: core::now_secs(),
        compile_secs: out.compile_secs,
    };
    let old = record(p);
    let json = serde_json::to_vec_pretty(&rec).map_err(|e| Failure::new(Phase::Installing, e.to_string()))?;
    core::write_atomic(&record_path(p), &json).map_err(|e| Failure::new(Phase::Installing, e))?;
    report(Phase::Installing, "Removing the previous build", Some(0.8));
    if let Some(old) = old.filter(|o| o.version_dir != vdir && o.version_dir != m.version_dir) {
        let _ = fs::remove_dir_all(p.versions().join(&old.version_dir));
    }
    let _ = fs::remove_dir_all(&stage);
    report(Phase::Installing, "Done", Some(1.0));
    Ok(rec)
}

/// What native-build says it left out of the build (native-mods.json's `left_out`), as sentences.
fn left_out(p: &Paths, vdir: &str) -> Vec<String> {
    let Ok(b) = fs::read(p.versions().join(vdir).join("native-mods.json")) else { return vec![] };
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap_or_default();
    v["left_out"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| match (e["id"].as_str(), e.as_str()) {
            (Some(id), _) => format!("{id} was left out of the build ({})", e["why"].to_string().trim_matches('"')),
            (None, Some(s)) => s.to_string(),
            _ => format!("a mod was left out of the build ({e})"),
        })
        .collect()
}

// ---------------------------------------------------------------- the build kit

struct Kit {
    /// Sunset's own .hmod package, for this exact release.
    sunset: PathBuf,
    /// HORIZON_SDK_BASE for native-build when the release builds against the Horizon fork's SDK.
    sdk_base: Option<PathBuf>,
}

/// Sunset's package (the release's "source" artifact) and, when the release has one, the fork SDK ("sdk"),
/// downloaded and checked against their signatures once, unpacked under `kit/`, and the fork SDK seeded into
/// horizon-launcher's own data so native-build uses it.
fn kit(p: &Paths, m: &Manifest, cancel: &AtomicBool, staging: &dyn Fn(&str, Option<f32>)) -> Result<Kit> {
    let base = || -> Result<String> {
        core::installed_base(p).ok_or_else(|| "this Sunset was installed by an older launcher: repair it from Overview once, then build again".to_string())
    };
    let root = p.root.join("kit");
    let src = m.artifact("source").ok_or_else(|| format!("Sunset {} was released without its plugin kit, so plugins can't be built for it: wait for the next Sunset release", m.version))?;
    let sunset_dir = root.join(format!("sunset-{}", &src.sha256[..16]));
    if !sunset_dir.join("sunset").join("horizon-mod.toml").is_file() {
        let progress = |msg: &str, f: Option<f32>| staging(msg, f.map(|f| 0.05 + 0.25 * f));
        let file = core::fetch_artifact(p, &base()?, src, &format!("Sunset {} for plugins", m.version), cancel, &progress)?;
        staging("Unpacking Sunset's package", Some(0.3));
        fs::create_dir_all(&sunset_dir).map_err(|e| e.to_string())?;
        core::unpack_tar_zst(&file, &sunset_dir, "sunset", "Sunset package", cancel)?;
        let _ = fs::remove_file(&file);
    }
    for old in fs::read_dir(&root).into_iter().flatten().flatten() {
        let n = old.file_name().to_string_lossy().into_owned();
        if n.starts_with("sunset-") && old.path() != sunset_dir {
            let _ = fs::remove_dir_all(old.path());
        }
    }
    let sdk_base = match m.artifact("sdk") {
        None => None,
        Some(a) => {
            let sdk_dir = root.join(format!("sdk-{}", &a.sha256[..16]));
            let rev = &m.horizon.revision;
            if !sdk_dir.join("fork-sdk").join(format!("sdk-{rev}")).join("sdk.json").is_file() {
                let progress = |msg: &str, f: Option<f32>| staging(msg, f.map(|f| 0.35 + 0.4 * f));
                let file = core::fetch_artifact(p, &base()?, a, "the Horizon fork's mod SDK (once per Sunset build)", cancel, &progress)?;
                staging("Unpacking the mod SDK", Some(0.75));
                fs::create_dir_all(&sdk_dir).map_err(|e| e.to_string())?;
                core::unpack_tar_zst(&file, &sdk_dir, "fork-sdk", "mod SDK", cancel)?;
                let _ = fs::remove_file(&file);
            }
            let sdk = sdk_dir.join("fork-sdk");
            staging("Setting up the mod SDK", Some(0.85));
            seed(p, &sdk, rev, &m.horizon.tag)?;
            for old in fs::read_dir(&root).into_iter().flatten().flatten() {
                let n = old.file_name().to_string_lossy().into_owned();
                if n.starts_with("sdk-") && old.path() != sdk_dir {
                    let _ = fs::remove_dir_all(old.path());
                }
            }
            Some(sdk)
        }
    };
    Ok(Kit { sunset: sunset_dir.join("sunset"), sdk_base })
}

/// What client/scripts/fork-sdk.sh `seed` does, for horizon-launcher's data in our folder: its kept `sdk.json` of the
/// build id replaced by the fork SDK's (native-build trusts a kept index again and fetches the slices from
/// HORIZON_SDK_BASE against it), the Nightly mapped to that build id, the fork's source in place of the published
/// one, and the stock SDK's unpacked slices dropped. Done again only when the fork commit changes.
fn seed(p: &Paths, sdk: &Path, rev: &str, tag: &str) -> Result<()> {
    let commit = fs::read_to_string(sdk.join("fork-commit")).map_err(|_| "the mod SDK has no fork-commit".to_string())?;
    let n = p.horizon().join("native");
    let mark = n.join(".fork-seeded");
    if fs::read_to_string(&mark).is_ok_and(|s| s.trim() == commit.trim()) && n.join("source").join(rev).join(".horizon-revision").is_file() {
        return Ok(());
    }
    let io = |e: std::io::Error| format!("setting up the mod SDK: {e}");
    let _ = fs::remove_dir_all(n.join("sdk").join(rev));
    let _ = fs::remove_dir_all(n.join("prebuilt-package"));
    let _ = fs::remove_dir_all(n.join("source").join(rev));
    for e in fs::read_dir(n.join("prebuilt")).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with(&format!("{rev}-")) {
            let _ = fs::remove_dir_all(e.path());
        }
    }
    fs::create_dir_all(n.join("sdk").join(rev)).map_err(io)?;
    fs::create_dir_all(n.join("sdk").join("versions")).map_err(io)?;
    fs::copy(sdk.join(format!("sdk-{rev}")).join("sdk.json"), n.join("sdk").join(rev).join("sdk.json")).map_err(io)?;
    fs::write(n.join("sdk").join("versions").join(tag), rev).map_err(io)?;
    copy_dir(&sdk.join("source"), &n.join("source").join(rev)).map_err(io)?;
    fs::write(&mark, commit.trim()).map_err(io)?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let t = e.file_type()?;
        if t.is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()))?;
        } else if t.is_file() {
            fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- native-build

struct Output {
    version_dir: String,
    compile_secs: u64,
}

/// Runs `native-build` on the staged folder for the sunset instance, reporting its lines as steps. Cancelling kills
/// it and everything it started (cargo, rustc). Its whole output goes to `plugins/build.log`.
#[allow(clippy::too_many_arguments)]
fn native_build(p: &Paths, stage: &Path, tag: &str, sdk_base: Option<&Path>, estimate: u64, cancel: &AtomicBool, report: StepReport) -> std::result::Result<Output, Failure> {
    let fail = |phase, e: String| Failure::new(phase, e);
    report(Phase::Staging, "Starting the build", Some(1.0));
    let mut c = core::cli(p).map_err(|e| fail(Phase::Staging, e))?;
    c.arg("native-build").arg(stage).args(["--instance", core::INSTANCE, "--version", tag, "--trust-native-code"]);
    if let Some(base) = sdk_base {
        c.env("HORIZON_SDK_BASE", base);
    }
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut tree = proc_tree::Tree::spawn(&mut c).map_err(|e| fail(Phase::Staging, format!("can't start horizon-launcher: {e}")))?;
    let mut log = fs::File::create(log_path(p)).ok();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let readers: Vec<_> = [tree.child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), tree.child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
        .map(|r| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(r).lines().map_while(|l| l.ok()) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    drop(tx);

    let mut st = Lines::new(estimate);
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Relaxed) && !cancelled {
            cancelled = true;
            tree.kill();
        }
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                if let Some((phase, msg, frac)) = st.line(&line) {
                    report(phase, &msg, frac);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The compile says nothing for minutes: keep its clock and estimate moving.
                if let Some((phase, msg, frac)) = st.tick() {
                    report(phase, &msg, frac);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let status = tree.child.wait().map_err(|e| fail(st.phase, e.to_string()))?;
    if cancelled {
        return Err(fail(st.phase, "cancelled".into()));
    }
    if !status.success() {
        return Err(fail(st.phase, st.error().unwrap_or_else(|| format!("horizon-launcher native-build stopped ({status})"))));
    }
    let version_dir = st.version_dir.clone().ok_or_else(|| fail(Phase::Packaging, "native-build said nothing about the version it built".into()))?;
    Ok(Output { version_dir, compile_secs: st.compile_secs() })
}

/// Reads native-build's lines (horizon-launcher's `native_mods::Step` labels) into build steps.
struct Lines {
    phase: Phase,
    estimate: u64,
    compiling: Option<Instant>,
    compiled: Option<u64>,
    current: String,
    errors: Vec<String>,
    in_error: bool,
    version_dir: Option<String>,
}

impl Lines {
    fn new(estimate: u64) -> Self {
        Lines { phase: Phase::Staging, estimate, compiling: None, compiled: None, current: String::new(), errors: vec![], in_error: false, version_dir: None }
    }

    fn compile_secs(&self) -> u64 {
        self.compiled.or_else(|| self.compiling.map(|t| t.elapsed().as_secs())).unwrap_or(0)
    }

    /// The compile's own progress: its time against the last build's (the compiler reports nothing in between).
    fn compile_frac(&self) -> Option<f32> {
        let t = self.compiling?.elapsed().as_secs_f32();
        Some((t / self.estimate.max(30) as f32).min(0.97))
    }

    fn line(&mut self, raw: &str) -> Option<(Phase, String, Option<f32>)> {
        let line = raw.trim();
        if line.is_empty() {
            return None;
        }
        if line.starts_with("error") {
            self.in_error = true;
        }
        if self.in_error {
            if self.errors.len() < 40 {
                self.errors.push(line.to_string());
            }
            return None;
        }
        if let Some(v) = line.strip_suffix(" built and selected for its instance.").filter(|v| v.starts_with("native-")) {
            self.version_dir = Some(v.to_string());
            return Some((Phase::Packaging, "Build ready".into(), Some(0.5)));
        }
        if line.starts_with("Compiling the mods") {
            self.phase = Phase::Compiling;
            self.compiling = Some(Instant::now());
            self.current = "Compiling Sunset and the plugins".into();
            return Some((Phase::Compiling, self.current.clone(), Some(0.0)));
        }
        // "Compiling <id> (i of n)": the mods compile side by side, so the last one started says little (Sunset is
        // the long one); the line says how many.
        if let Some(rest) = line.strip_prefix("Compiling ").filter(|_| self.phase == Phase::Compiling) {
            if let Some(n) = rest.rsplit_once(" of ").and_then(|(_, n)| n.trim_end_matches(')').parse::<usize>().ok()) {
                self.current = match n {
                    2 => "Compiling Sunset and 1 plugin".into(),
                    n => format!("Compiling Sunset and {} plugins", n.saturating_sub(1)),
                };
            }
            return self.tick();
        }
        if line.starts_with("Installing the build") {
            self.phase = Phase::Packaging;
            self.compiled = self.compiling.map(|t| t.elapsed().as_secs());
            return Some((Phase::Packaging, "Linking and writing the game folder".into(), Some(0.2)));
        }
        if line.starts_with("Your mods are built") {
            return Some((Phase::Packaging, line.trim_end_matches('.').to_string(), Some(0.4)));
        }
        if self.phase == Phase::Staging {
            // Downloads print their share: "…: 40%" (Rust) or "…: 12 of 300 MB" (the SDK's slices).
            let frac = percent(line).or_else(|| of_total(line));
            return Some((Phase::Staging, line.to_string(), frac.map(|f| 0.9 + 0.1 * f)));
        }
        Some((self.phase, line.to_string(), None))
    }

    fn tick(&self) -> Option<(Phase, String, Option<f32>)> {
        let t = self.compiling.filter(|_| self.phase == Phase::Compiling)?.elapsed().as_secs();
        Some((Phase::Compiling, format!("{} · {}:{:02}", self.current, t / 60, t % 60), self.compile_frac()))
    }

    /// The first error lines native-build printed (the whole output is in the build log).
    fn error(&self) -> Option<String> {
        if self.errors.is_empty() {
            return None;
        }
        // rustc's location arrows, empty gutters and closing remarks say nothing a player can use here.
        let noise = |l: &str| {
            l.starts_with("-->")
                || l.starts_with('|')
                || l.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '|')
                || l.starts_with("error: aborting due to")
                || l.starts_with("For more information about")
        };
        let head: Vec<&str> = self.errors.iter().map(String::as_str).filter(|l| !noise(l)).take(6).collect();
        Some(head.join("\n"))
    }
}

fn percent(line: &str) -> Option<f32> {
    let n: f32 = line.strip_suffix('%')?.rsplit([' ', ':']).next()?.parse().ok()?;
    Some(n / 100.)
}

fn of_total(line: &str) -> Option<f32> {
    let rest = line.strip_suffix(" MB")?;
    let (done, total) = rest.rsplit_once(" of ")?;
    let done: f32 = done.rsplit([' ', ':']).next()?.parse().ok()?;
    let total: f32 = total.trim().parse().ok()?;
    (total > 0.).then(|| (done / total).clamp(0., 1.))
}

/// Ends every plugin build still running, and refuses to start new ones: for when the launcher quits. A build left
/// running would hold cargo's lock and could still switch the instance to its build after the launcher is gone.
pub fn stop_builds() {
    proc_tree::kill_all();
}

/// A child process and everything it starts, killed together: a Job Object on Windows (also killed if the launcher
/// goes away), a process group elsewhere. Every live tree is listed so `kill_all` can end them as the launcher quits;
/// a tree is killed when dropped too.
pub(crate) mod proc_tree {
    use std::process::{Child, Command};
    use std::sync::Mutex;

    /// The live trees (by id) and whether the launcher is quitting.
    static LIVE: Mutex<Live> = Mutex::new(Live { quitting: false, next: 0, trees: Vec::new() });

    struct Live {
        quitting: bool,
        next: u64,
        trees: Vec<(u64, Handle)>,
    }

    /// What kills a tree: its process group (unix) or its job (Windows; the handle is closed only after the tree has
    /// left `LIVE`, under its lock).
    #[derive(Clone, Copy)]
    struct Handle(#[cfg(unix)] i32, #[cfg(windows)] usize);

    impl Handle {
        fn kill(self) {
            #[cfg(unix)]
            unsafe {
                unsafe extern "C" {
                    fn kill(pid: i32, sig: i32) -> i32;
                }
                // SAFETY: a plain syscall; the group is ours (the child leads it).
                kill(-self.0, 9);
            }
            #[cfg(windows)]
            Job::terminate_raw(self.0 as *mut _);
        }
    }

    fn live() -> std::sync::MutexGuard<'static, Live> {
        LIVE.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn kill_all() {
        let mut l = live();
        l.quitting = true;
        for (_, h) in &l.trees {
            h.kill();
        }
    }

    pub struct Tree {
        pub child: Child,
        id: u64,
        handle: Handle,
        /// Owns the job handle `handle` names; closing it ends the tree too.
        #[cfg(windows)]
        _job: Job,
    }

    impl Tree {
        pub fn spawn(c: &mut Command) -> std::io::Result<Tree> {
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                c.process_group(0);
            }
            // Held across the spawn so a quit can't slip in between it and the listing.
            let mut l = live();
            if l.quitting {
                return Err(std::io::Error::other("the launcher is closing"));
            }
            let child = c.spawn()?;
            #[cfg(windows)]
            let job = Job::new();
            #[cfg(windows)]
            job.assign(&child);
            #[cfg(windows)]
            let handle = Handle(job.0 as usize);
            #[cfg(unix)]
            let handle = Handle(child.id() as i32);
            l.next += 1;
            let id = l.next;
            l.trees.push((id, handle));
            Ok(Tree {
                child,
                id,
                handle,
                #[cfg(windows)]
                _job: job,
            })
        }

        pub fn kill(&mut self) {
            self.handle.kill();
            let _ = self.child.kill();
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let mut l = live();
            l.trees.retain(|(id, _)| *id != self.id);
            // Whatever the build left running ends with it (on Windows closing the job does that as well).
            self.handle.kill();
        }
    }

    #[cfg(windows)]
    struct Job(*mut core::ffi::c_void);

    // The handle is only used through the Win32 calls below, which may come from any thread.
    #[cfg(windows)]
    unsafe impl Send for Job {}

    #[cfg(windows)]
    #[repr(C)]
    struct BasicLimits {
        per_process_user_time: i64,
        per_job_user_time: i64,
        flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[cfg(windows)]
    #[repr(C)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[cfg(windows)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut core::ffi::c_void, name: *const u16) -> *mut core::ffi::c_void;
        fn SetInformationJobObject(job: *mut core::ffi::c_void, class: i32, info: *const core::ffi::c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut core::ffi::c_void, process: *mut core::ffi::c_void) -> i32;
        fn TerminateJobObject(job: *mut core::ffi::c_void, code: u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }

    #[cfg(windows)]
    impl Job {
        fn new() -> Job {
            const EXTENDED_LIMIT_INFORMATION: i32 = 9;
            const KILL_ON_JOB_CLOSE: u32 = 0x2000;
            // SAFETY: plain Win32 calls on a handle this struct owns; a null handle makes the others fail harmlessly.
            unsafe {
                let h = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if !h.is_null() {
                    let mut info: ExtendedLimits = std::mem::zeroed();
                    info.basic.flags = KILL_ON_JOB_CLOSE;
                    SetInformationJobObject(h, EXTENDED_LIMIT_INFORMATION, &info as *const _ as *const _, std::mem::size_of::<ExtendedLimits>() as u32);
                }
                Job(h)
            }
        }
        fn assign(&self, child: &std::process::Child) {
            use std::os::windows::io::AsRawHandle;
            if !self.0.is_null() {
                // SAFETY: the child's handle is valid while `child` lives.
                unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as *mut _) };
            }
        }
        fn terminate_raw(h: *mut core::ffi::c_void) {
            if !h.is_null() {
                // SAFETY: a job handle still open: callers hold it through `Tree` or `LIVE`.
                unsafe { TerminateJobObject(h, 1) };
            }
        }
    }

    #[cfg(windows)]
    impl Drop for Job {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: closed once; KILL_ON_JOB_CLOSE ends whatever the build left running.
                unsafe { CloseHandle(self.0) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let t = std::env::temp_dir().join(format!("sunset-plugins-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(&t).unwrap();
        t
    }

    fn package(at: &Path, id: &str, horizon: &str) {
        fs::create_dir_all(at.join("src")).unwrap();
        fs::write(at.join("horizon-mod.toml"), format!("id = \"{id}\"\nname = \"Test\"\nversion = \"1.0.0\"\n\n[native]\ngame = \"26.3\"\nhorizon = \"{horizon}\"\nmixins = \"x\"\n")).unwrap();
        fs::write(at.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        fs::write(at.join("src/lib.rs"), "").unwrap();
        fs::create_dir_all(at.join("target/debug")).unwrap();
        fs::write(at.join("target/debug/junk"), "x").unwrap();
    }

    fn manifest(horizon: &str) -> Manifest {
        serde_json::from_value(serde_json::json!({
            "schema": 1, "id": "sunset", "version": "1.2.1", "platform": core::PLATFORM, "version_dir": "native-x",
            "horizon": {"revision": horizon, "tag": "nightly-x", "game": "26.3"},
            "cli": {"version": "0.2.78", "file": "f", "url": "u", "sha256": "s"},
            "artifacts": []
        }))
        .unwrap()
    }

    #[test]
    fn add_checks_the_pin_and_skips_build_output() {
        let t = temp("add");
        let p = Paths { root: t.join("home") };
        package(&t.join("good"), "good_one", "abc");
        package(&t.join("other"), "other", "def");
        let m = manifest("abc");
        let pl = add(&p, &t.join("good"), &m).unwrap();
        assert_eq!((pl.id.as_str(), pl.name.as_str()), ("good_one", "Test"));
        assert!(!pl.dir.join("target").exists(), "build output is never copied");
        let e = add(&p, &t.join("other"), &m).unwrap_err();
        assert!(e.contains("made for Horizon def"), "{e}");
        assert!(add(&p, &t.join("nothing"), &m).is_err());
        assert_eq!(list(&p).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["good_one"]);
        // A changed package is a changed hash (it needs a new build); the same one is the same hash.
        let again = add(&p, &t.join("good"), &m).unwrap();
        assert_eq!(again.hash, pl.hash);
        fs::write(t.join("good/src/lib.rs"), "// changed").unwrap();
        assert_ne!(add(&p, &t.join("good"), &m).unwrap().hash, pl.hash);
        remove(&p, "good_one").unwrap();
        assert!(list(&p).is_empty());
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn hmod_archives_unpack_flat_and_refuse_escapes() {
        let t = temp("hmod");
        let p = Paths { root: t.join("home") };
        let write = |name: &str, files: &[(&str, &str)]| {
            let path = t.join(name);
            let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (n, body) in files {
                z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
            path
        };
        let toml = "id = \"zipped\"\nversion = \"2.0.0\"\n[native]\nhorizon = \"abc\"\n";
        let ok = write("ok.hmod", &[("horizon-mod.toml", toml), ("Cargo.toml", "[package]"), ("src/lib.rs", "")]);
        let pl = add(&p, &ok, &manifest("abc")).unwrap();
        assert_eq!((pl.id.as_str(), pl.name.as_str(), pl.version.as_str()), ("zipped", "zipped", "2.0.0"));
        let bad = write("bad.hmod", &[("../evil.txt", "x"), ("horizon-mod.toml", toml)]);
        assert!(add(&p, &bad, &manifest("abc")).is_err());
        assert!(!t.join("home/plugins/evil.txt").exists() && !t.join("home/evil.txt").exists());
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn native_build_lines_become_steps() {
        let mut l = Lines::new(100);
        assert_eq!(l.line("Checking 2 mods").map(|s| s.0), Some(Phase::Staging));
        let (ph, _, f) = l.line("Downloading Rust 1.99.1 (once): 40%").unwrap();
        assert_eq!((ph, f), (Phase::Staging, Some(0.94)));
        let (_, _, f) = l.line("Getting the mod tools for the Nov 18 update: 150 of 300 MB").unwrap();
        assert_eq!(f, Some(0.95));
        assert_eq!(l.line("Compiling the mods against the prebuilt game").map(|s| s.0), Some(Phase::Compiling));
        let (ph, msg, _) = l.line("Compiling sunset (1 of 2)").unwrap();
        assert_eq!(ph, Phase::Compiling);
        assert!(msg.starts_with("Compiling Sunset and 1 plugin · 0:0"), "{msg}");
        assert_eq!(l.line("Installing the build").map(|s| s.0), Some(Phase::Packaging));
        l.line("native-0b14f8a-a93f3ccc8155 built and selected for its instance.");
        assert_eq!(l.version_dir.as_deref(), Some("native-0b14f8a-a93f3ccc8155"));

        let mut e = Lines::new(100);
        e.line("Compiling the mods against the prebuilt game");
        e.line("error: Compiling the patched functions failed (exit code: 1). Its first errors:");
        e.line("error[E0425]: cannot find function `x` in module `y`");
        e.line("     --> C:\\x\\lib.rs:1:1");
        e.line("      |");
        e.line("error: aborting due to 1 previous error");
        assert_eq!(e.phase, Phase::Compiling);
        let msg = e.error().unwrap();
        assert!(msg.contains("E0425") && !msg.contains("-->") && !msg.contains("aborting"), "{msg}");
    }

    #[test]
    fn a_failed_build_leaves_no_version_folder_behind() {
        let t = temp("leftover");
        let p = Paths { root: t.clone() };
        let m = manifest("abc");
        let mk = |n: &str| fs::create_dir_all(p.versions().join(n)).unwrap();
        mk("native-x"); // the signed build
        mk("native-old"); // there before the build
        let had = native_dirs(&p);
        mk("native-new"); // the failed build's, finished or cut short
        mk("native-playing"); // native-build selected it and putting the instance back failed
        mk("other");
        fs::create_dir_all(p.horizon()).unwrap();
        fs::write(p.horizon().join("instances.json"), serde_json::json!({"instances": {"sunset": {"version": "native-playing"}}}).to_string()).unwrap();
        remove_new_builds(&p, &m, &had);
        let left: Vec<String> = native_dirs(&p).into_iter().collect();
        assert_eq!(left, ["native-old", "native-playing", "native-x"]);
        assert!(p.versions().join("other").is_dir());
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn status_follows_settings_plugins_and_the_instance() {
        let t = temp("status");
        let p = Paths { root: t.clone() };
        let m = manifest("abc");
        let set_playing = |v: &str| {
            fs::create_dir_all(p.horizon()).unwrap();
            fs::write(p.horizon().join("instances.json"), serde_json::json!({"instances": {"sunset": {"version": v}}}).to_string()).unwrap();
        };
        set_playing("native-x");
        assert_eq!(status(&p, &m, false, &[]), Status::Clean);
        package(&t.join("src-pl"), "pl", "abc");
        let pl = add(&p, &t.join("src-pl"), &m).unwrap();
        assert_eq!(status(&p, &m, false, std::slice::from_ref(&pl)), Status::Clean, "off: the signed build");
        assert_eq!(status(&p, &m, true, std::slice::from_ref(&pl)), Status::NeedsBuild);
        let b = Build { version_dir: "native-y".into(), base: "native-x".into(), sunset_version: "1.2.1".into(), plugins: vec![Built { id: "pl".into(), version: "1.0.0".into(), hash: pl.hash.clone() }], built: 0, compile_secs: 1 };
        core::write_atomic(&record_path(&p), &serde_json::to_vec(&b).unwrap()).unwrap();
        fs::create_dir_all(p.versions().join("native-y")).unwrap();
        set_playing("native-y");
        assert_eq!(status(&p, &m, true, std::slice::from_ref(&pl)), Status::Built(b.clone()));
        assert_eq!(status(&p, &m, false, std::slice::from_ref(&pl)), Status::NeedsClean, "turned off while the plugin build plays");
        assert_eq!(status(&p, &m, true, &[]), Status::NeedsClean, "the last plugin removed");
        let mut changed = pl.clone();
        changed.hash = "other".into();
        assert_eq!(status(&p, &m, true, &[changed]), Status::NeedsBuild, "a changed plugin needs a new build");
        let mut updated = m.clone();
        updated.version_dir = "native-z".into();
        assert_eq!(status(&p, &updated, true, std::slice::from_ref(&pl)), Status::NeedsBuild, "a new Sunset needs a new build");
        fs::remove_dir_all(&t).unwrap();
    }
}
