//! Everything the window does, without the window: the data folder, the signed manifest, verified downloads, the
//! stock horizon-launcher CLI (run as a separate process with its homes inside ours) and the install itself.
//! Blocking: the window runs these on worker threads.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INSTANCE: &str = "sunset";
/// GitHub repository whose `client-v*` releases carry the manifest and the prebuilt client.
pub const REPO: &str = "PointOfPressure/sunset-client";
const PUBLIC_KEY: &str = include_str!("../sunset.pub");

#[cfg(target_os = "linux")]
pub const PLATFORM: &str = "linux-x86_64";
#[cfg(windows)]
pub const PLATFORM: &str = "windows-x86_64";
#[cfg(not(any(target_os = "linux", windows)))]
pub const PLATFORM: &str = "unsupported";

pub type Result<T> = std::result::Result<T, String>;

// ---------------------------------------------------------------- paths and settings

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    /// `SUNSET_LAUNCHER_HOME`, else `<data dir>/sunset-launcher` (Linux `~/.local/share`, Windows `%APPDATA%`).
    pub fn new() -> Self {
        if let Some(home) = std::env::var_os("SUNSET_LAUNCHER_HOME").filter(|h| !h.is_empty()) {
            return Paths { root: PathBuf::from(home) };
        }
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        };
        Paths { root: base.unwrap_or_else(std::env::temp_dir).join("sunset-launcher") }
    }
    /// The stock CLI's own data (`HORIZON_LAUNCHER_HOME`): versions, accounts, settings.
    pub fn horizon(&self) -> PathBuf {
        self.root.join("horizon")
    }
    /// The game folders (`HORIZON_HOME`); instances live in `instances/`.
    pub fn game(&self) -> PathBuf {
        self.root.join("game")
    }
    pub fn instance(&self) -> PathBuf {
        self.game().join("instances").join(INSTANCE)
    }
    pub fn versions(&self) -> PathBuf {
        self.horizon().join("versions")
    }
    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }
    pub fn logs(&self) -> PathBuf {
        self.instance().join("logs")
    }
    /// Sunset's own data inside the instance (config, profiles), as the game names it.
    pub fn sunset_data(&self) -> PathBuf {
        sunset_data_in(&self.instance())
    }
    fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

pub fn sunset_data_in(game_dir: &Path) -> PathBuf {
    game_dir.join("horizon-mods").join(".horizon-mod-data").join("sunset")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_update: bool,
    /// "1280x720", "1600x900", "1920x1080" or "fullscreen".
    pub window: String,
    /// The one-time import from a Horizon instance was done or turned down.
    pub import_decided: bool,
    /// Where the manifest comes from instead of GitHub Releases (`SUNSET_MANIFEST` wins over it).
    pub manifest_url: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { auto_update: true, window: "1280x720".into(), import_decided: false, manifest_url: None }
    }
}

pub const WINDOW_SIZES: [&str; 4] = ["1280x720", "1600x900", "1920x1080", "fullscreen"];

impl Settings {
    pub fn load(p: &Paths) -> Self {
        fs::read(p.file("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, p: &Paths) -> Result<()> {
        write_atomic(&p.file("settings.json"), &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
    }
    /// The game's own window arguments.
    pub fn game_args(&self) -> Vec<String> {
        if self.window == "fullscreen" {
            return vec!["--fullscreen".into()];
        }
        match self.window.split_once('x').map(|(w, h)| (w.parse::<u32>(), h.parse::<u32>())) {
            Some((Ok(w), Ok(h))) => vec!["--width".into(), w.to_string(), "--height".into(), h.to_string()],
            _ => vec![],
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, path)).map_err(|e| format!("can't write {}: {e}", path.display()))
}

// ---------------------------------------------------------------- manifest

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub platform: String,
    pub version_dir: String,
    pub horizon: HorizonPin,
    pub cli: CliPin,
    pub artifacts: Vec<Artifact>,
    /// What changed in this version, shown on Overview. Optional: manifests written before it have none, and older
    /// launchers ignore it. It is part of the signed manifest, so it is as trusted as the rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changelog: Vec<Change>,
}

/// One line of a version's changelog (client/CHANGELOG.md, written into the manifest by the release scripts).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    /// The section it is listed under: New, Changed, Fixed, Removed.
    #[serde(default)]
    pub kind: String,
    pub text: String,
    /// Written in bold in the changelog: shown highlighted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub important: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HorizonPin {
    pub revision: String,
    pub tag: String,
    pub game: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CliPin {
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub file: String,
    pub kind: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

impl Manifest {
    pub fn short_pin(&self) -> &str {
        &self.horizon.revision[..self.horizon.revision.len().min(7)]
    }
    fn game(&self) -> Result<&Artifact> {
        self.artifacts.iter().find(|a| a.kind == "game").ok_or_else(|| "the manifest names no game package".to_string())
    }
    /// Names from the manifest become paths: one plain component each, nothing else.
    fn check(&self) -> Result<()> {
        if self.schema != 1 || self.id != "sunset" {
            return Err(format!("unsupported manifest (schema {}, id {})", self.schema, self.id));
        }
        if self.platform != PLATFORM {
            return Err(format!("the manifest is for {}, this computer needs {PLATFORM}", self.platform));
        }
        let plain = |s: &str| {
            !s.is_empty() && s != "." && s != ".." && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-@+".contains(&b))
        };
        let hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        let names = [&self.version_dir, &self.horizon.tag, &self.cli.version, &self.cli.file];
        if let Some(bad) = names.into_iter().find(|n| !plain(n)) {
            return Err(format!("the manifest has an unsafe name: {bad:?}"));
        }
        if !hex(&self.cli.sha256) || !self.cli.url.starts_with("https://github.com/horizon-rust/horizon/releases/download/") {
            return Err("the manifest's horizon-launcher entry is not a signed upstream download".into());
        }
        for a in &self.artifacts {
            if !plain(&a.file) || !plain(&a.signature) || !hex(&a.sha256) {
                return Err(format!("the manifest has an unsafe artifact entry: {:?}", a.file));
            }
        }
        self.game().map(|_| ())
    }
}

/// A manifest that passed its signature, and where its files are.
#[derive(Clone, Debug)]
pub struct Release {
    pub manifest: Manifest,
    pub raw: Vec<u8>,
    pub base: String,
}

fn public_key() -> minisign_verify::PublicKey {
    // Baked in at build time from launcher/sunset.pub: a bad key is a build error, caught by the tests.
    minisign_verify::PublicKey::decode(PUBLIC_KEY.trim()).expect("launcher/sunset.pub is a minisign public key")
}

pub fn verify_signature(bytes: &[u8], sig_text: &str) -> Result<()> {
    let sig = minisign_verify::Signature::decode(sig_text).map_err(|e| format!("unreadable signature: {e}"))?;
    public_key().verify(bytes, &sig, false).map_err(|e| format!("bad signature: {e}"))
}

/// The manifest's address: `SUNSET_MANIFEST`, the setting, or the newest `client-v*` release of [`REPO`] that has one
/// for this platform. The launcher only installs what that manifest pins: it never follows Horizon's newest Nightly.
pub fn manifest_url(settings: &Settings) -> Result<String> {
    if let Some(u) = std::env::var("SUNSET_MANIFEST").ok().filter(|u| !u.is_empty()).or(settings.manifest_url.clone()) {
        return Ok(u);
    }
    let list = fetch(&format!("https://api.github.com/repos/{REPO}/releases?per_page=30"))?;
    let releases: Vec<serde_json::Value> = serde_json::from_slice(&list).map_err(|e| format!("GitHub's release list: {e}"))?;
    let wanted = [format!("manifest-{PLATFORM}.json"), "manifest.json".to_string()];
    for r in &releases {
        let tag = r["tag_name"].as_str().unwrap_or("");
        if !tag.starts_with("client-v") || r["draft"].as_bool() == Some(true) || r["prerelease"].as_bool() == Some(true) {
            continue;
        }
        let assets = r["assets"].as_array().cloned().unwrap_or_default();
        for name in &wanted {
            if let Some(a) = assets.iter().find(|a| a["name"].as_str() == Some(name)) {
                if let Some(u) = a["browser_download_url"].as_str() {
                    return Ok(u.to_string());
                }
            }
        }
    }
    Err(format!("no Sunset client release for {PLATFORM} yet (github.com/{REPO}/releases)"))
}

/// Fetches the manifest and its `.minisig`, checks the signature with the built-in key, then the contents.
pub fn fetch_release(settings: &Settings) -> Result<Release> {
    let url = manifest_url(settings)?;
    let raw = fetch(&url)?;
    let sig = fetch(&format!("{url}.minisig"))?;
    verify_signature(&raw, &String::from_utf8_lossy(&sig)).map_err(|e| format!("manifest refused, {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|e| format!("unreadable manifest: {e}"))?;
    manifest.check()?;
    let base = url.rsplit_once('/').map(|(b, _)| b.to_string()).unwrap_or_default();
    Ok(Release { manifest, raw, base })
}

/// The manifest of what is installed, or None.
pub fn installed(p: &Paths) -> Option<Manifest> {
    let m: Manifest = serde_json::from_slice(&fs::read(p.file("installed.json")).ok()?).ok()?;
    p.versions().join(&m.version_dir).is_dir().then_some(m)
}

// ---------------------------------------------------------------- network

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().user_agent(&format!("sunset-launcher/{VERSION}")).timeout_connect(std::time::Duration::from_secs(20)).build()
}

fn open(url: &str) -> Result<(Box<dyn Read + Send>, Option<u64>)> {
    if let Some(path) = url.strip_prefix("file://") {
        let f = fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let len = f.metadata().ok().map(|m| m.len());
        return Ok((Box::new(f), len));
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("unsupported address {url}"));
    }
    let resp = agent().get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("{url}: HTTP {code}"),
        e => format!("{url}: {e}"),
    })?;
    let len = resp.header("Content-Length").and_then(|l| l.parse().ok());
    Ok((Box::new(resp.into_reader()), len))
}

pub fn fetch(url: &str) -> Result<Vec<u8>> {
    let (r, _) = open(url)?;
    let mut out = Vec::new();
    r.take(64 << 20).read_to_end(&mut out).map_err(|e| format!("{url}: {e}"))?;
    Ok(out)
}

pub type Progress<'a> = &'a dyn Fn(&str, Option<f32>);

/// Downloads `url` to `dest`, refusing it unless its SHA-256 is `sha256` and (with `sig`) its minisign signature holds.
fn download(url: &str, dest: &Path, sha256: &str, sig: Option<&str>, what: &str, cancel: &AtomicBool, progress: Progress) -> Result<()> {
    let sig = sig.map(minisign_verify::Signature::decode).transpose().map_err(|e| format!("{what}: unreadable signature: {e}"))?;
    let key = public_key();
    let mut verifier = sig.as_ref().map(|s| key.verify_stream(s)).transpose().map_err(|e| format!("{what}: {e}"))?;
    let (mut r, len) = open(url)?;
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let part = dest.with_extension("part");
    let mut out = io::BufWriter::new(fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?);
    let mut hash = Sha256::new();
    let (mut done, mut buf, mut last) = (0u64, vec![0u8; 256 << 10], 0u64);
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = fs::remove_file(&part);
            return Err("cancelled".into());
        }
        let n = r.read(&mut buf).map_err(|e| format!("{what}: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        if let Some(v) = verifier.as_mut() {
            v.update(&buf[..n]);
        }
        out.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
        done += n as u64;
        if done - last >= 1 << 20 {
            last = done;
            progress(&format!("Downloading {what}: {} of {}", mib(done), len.map(mib).unwrap_or_else(|| "?".into())), len.map(|l| done as f32 / l as f32));
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    let got = hex(&hash.finalize());
    let check = if !got.eq_ignore_ascii_case(sha256) {
        Err(format!("{what}: SHA-256 mismatch (got {got}, the signed manifest says {sha256})"))
    } else if let Some(v) = verifier.as_mut() {
        v.finalize().map_err(|e| format!("{what}: bad signature: {e}"))
    } else {
        Ok(())
    };
    if let Err(e) = check {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())
}

fn mib(n: u64) -> String {
    format!("{:.0} MB", n as f64 / 1e6)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    io::copy(&mut f, &mut h).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(hex(&h.finalize()))
}

// ---------------------------------------------------------------- the stock CLI

fn cli_name() -> &'static str {
    if cfg!(windows) { "horizon-launcher.exe" } else { "horizon-launcher" }
}

/// The installed stock CLI, if any (`cli/current` names its version).
pub fn cli_path(p: &Paths) -> Option<PathBuf> {
    let v = fs::read_to_string(p.root.join("cli").join("current")).ok()?;
    let exe = p.root.join("cli").join(v.trim()).join(cli_name());
    exe.is_file().then_some(exe)
}

pub fn cli(p: &Paths) -> Result<Command> {
    let exe = cli_path(p).ok_or("horizon-launcher is not installed yet")?;
    let mut c = Command::new(exe);
    c.env("HORIZON_LAUNCHER_HOME", p.horizon())
        .env("HORIZON_HOME", p.game())
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(c)
}

/// Runs the CLI to the end: its stdout, or its last error lines.
pub fn run_cli(p: &Paths, args: &[&str]) -> Result<String> {
    let out = cli(p)?.args(args).output().map_err(|e| format!("can't start horizon-launcher: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = tail[tail.len().saturating_sub(3)..].join(" ");
    Err(format!("horizon-launcher {} failed: {}", args.first().unwrap_or(&""), if tail.is_empty() { out.status.to_string() } else { tail }))
}

/// Downloads the stock CLI the manifest pins (its SHA-256 is in the signed manifest) unless it is already there.
pub fn ensure_cli(p: &Paths, pin: &CliPin, cancel: &AtomicBool, progress: Progress) -> Result<PathBuf> {
    let dir = p.root.join("cli").join(&pin.version);
    let exe = dir.join(cli_name());
    let stamp = dir.join(".sha256");
    if exe.is_file() && fs::read_to_string(&stamp).is_ok_and(|s| s.trim() == pin.sha256) {
        write_atomic(&p.root.join("cli").join("current"), pin.version.as_bytes())?;
        return Ok(exe);
    }
    let archive = p.downloads().join(&pin.file);
    download(&pin.url, &archive, &pin.sha256, None, &format!("horizon-launcher {}", pin.version), cancel, progress)?;
    progress("Unpacking horizon-launcher", None);
    let tmp = p.root.join("cli").join(format!(".{}.tmp", pin.version));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let f = fs::File::open(&archive).map_err(|e| e.to_string())?;
    if pin.file.ends_with(".zip") {
        unzip(f, &tmp)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(f)).unpack(&tmp).map_err(|e| format!("horizon-launcher archive: {e}"))?;
    }
    if !tmp.join(cli_name()).is_file() {
        return Err(format!("the horizon-launcher archive has no {}", cli_name()));
    }
    fs::write(tmp.join(".sha256"), &pin.sha256).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&tmp, &dir).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&archive);
    write_atomic(&p.root.join("cli").join("current"), pin.version.as_bytes())?;
    Ok(exe)
}

#[cfg(windows)]
fn unzip(f: fs::File, to: &Path) -> Result<()> {
    zip::ZipArchive::new(f).and_then(|mut z| z.extract(to)).map_err(|e| format!("horizon-launcher archive: {e}"))
}

#[cfg(not(windows))]
fn unzip(_: fs::File, _: &Path) -> Result<()> {
    Err("zip archives are only used on Windows".into())
}

/// Linux: the shared libraries the CLI can't find, with the packages that hold them. Empty when it can run.
pub fn missing_libs(exe: &Path) -> Vec<String> {
    if !cfg!(target_os = "linux") {
        return vec![];
    }
    match Command::new("ldd").arg(exe).stdin(Stdio::null()).output() {
        Ok(o) => parse_ldd(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => vec![],
    }
}

pub fn parse_ldd(out: &str) -> Vec<String> {
    let mut libs: Vec<String> = out
        .lines()
        .filter(|l| l.contains("not found"))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    libs.dedup();
    libs
}

/// The install command for `libs` on this distribution (from /etc/os-release), or a plain list.
pub fn install_hint(libs: &[String]) -> String {
    let os = fs::read_to_string("/etc/os-release").unwrap_or_default();
    install_hint_for(&os, libs)
}

pub fn install_hint_for(os_release: &str, libs: &[String]) -> String {
    let ids: String = os_release
        .lines()
        .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "");
    let fam = if ids.contains("debian") || ids.contains("ubuntu") {
        "apt"
    } else if ids.contains("fedora") || ids.contains("rhel") {
        "dnf"
    } else if ids.contains("arch") {
        "pacman"
    } else if ids.contains("suse") {
        "zypper"
    } else {
        ""
    };
    let pkg = |lib: &str| -> String {
        let stem = lib.split(".so").next().unwrap_or(lib);
        let ver = lib.rsplit(".so.").next().filter(|v| v.chars().all(|c| c.is_ascii_digit())).unwrap_or("");
        match fam {
            "apt" => format!("{stem}{}{ver}", if stem.ends_with(char::is_numeric) { "-" } else { "" }),
            "zypper" => format!("{stem}-{ver}"),
            _ => stem.to_string(),
        }
    };
    let mut pkgs: Vec<String> = libs.iter().map(|l| pkg(l)).collect();
    pkgs.dedup();
    match fam {
        "apt" => format!("sudo apt install {}", pkgs.join(" ")),
        "dnf" => format!("sudo dnf install {}", pkgs.join(" ")),
        "pacman" => format!("sudo pacman -S {}", pkgs.join(" ")),
        "zypper" => format!("sudo zypper install {}", pkgs.join(" ")),
        _ => format!("install the packages that provide {}", libs.join(", ")),
    }
}

// ---------------------------------------------------------------- accounts

#[derive(Clone, Debug, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    #[serde(default)]
    pub selected: bool,
}

pub fn accounts(p: &Paths) -> Result<Vec<Account>> {
    #[derive(Deserialize)]
    struct List {
        accounts: Vec<Account>,
    }
    let out = run_cli(p, &["account", "list", "--json"])?;
    Ok(serde_json::from_str::<List>(&out).map_err(|e| format!("account list: {e}"))?.accounts)
}

/// What the CLI's device-code sign-in printed: `... open <link> in a browser and enter the code <code>`.
pub fn parse_sign_in_line(line: &str) -> Option<(String, String)> {
    let (_, rest) = line.split_once(" open ")?;
    let (link, rest) = rest.split_once(' ')?;
    let code = rest.rsplit_once("enter the code ")?.1.trim();
    (link.starts_with("https://") && !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then(|| (link.to_string(), code.to_string()))
}

pub enum SignIn {
    Code { link: String, code: String },
    Done(Result<String>),
}

/// Starts `account sign-in`. Calls `on` with the code once the CLI prints it, then with the result
/// (the signed-in name). The child is returned so the window can cancel it.
pub fn sign_in(p: &Paths, on: impl Fn(SignIn) + Send + 'static) -> Result<Arc<std::sync::Mutex<Child>>> {
    let mut child = cli(p)?
        .args(["account", "sign-in"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("can't start horizon-launcher: {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let child = Arc::new(std::sync::Mutex::new(child));
    let waiter = child.clone();
    std::thread::spawn(move || {
        let mut last = String::new();
        for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
            if let Some((link, code)) = parse_sign_in_line(&line) {
                on(SignIn::Code { link, code });
            } else if !line.trim().is_empty() {
                last = line;
            }
        }
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let status = loop {
            if let Ok(Some(s)) = waiter.lock().unwrap().try_wait() {
                break s;
            }
            // stderr is closed, so the CLI is exiting: wait for it without holding the lock for long.
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let name = out.lines().find_map(|l| l.strip_prefix("Signed in as ")).map(|n| n.trim_end_matches(" again.").trim_end_matches('.').to_string());
        on(SignIn::Done(match (status.success(), name) {
            (true, Some(n)) => Ok(n),
            (true, None) => Ok(String::new()),
            (false, _) if last.is_empty() => Err(format!("sign-in stopped ({status})")),
            (false, _) => Err(last),
        }));
    });
    Ok(child)
}

// ---------------------------------------------------------------- install

/// Installs (or with `repair`, re-verifies and re-unpacks) the release: the stock CLI, the pinned Horizon Nightly,
/// its resources, the signed Sunset package as a version of its own, and the `sunset` instance on it.
pub fn install(p: &Paths, rel: &Release, repair: bool, cancel: &AtomicBool, progress: Progress) -> Result<()> {
    let m = &rel.manifest;
    let stop = || if cancel.load(Ordering::Relaxed) { Err("cancelled".to_string()) } else { Ok(()) };
    progress("Checking horizon-launcher", None);
    let exe = ensure_cli(p, &m.cli, cancel, progress)?;
    let missing = missing_libs(&exe);
    if !missing.is_empty() {
        return Err(format!("horizon-launcher needs {} first: {}", missing.join(", "), install_hint(&missing)));
    }
    run_cli(p, &["settings", "install-new-versions", "off"])?;
    run_cli(p, &["settings", "prune-old-versions", "off"])?;
    stop()?;
    let base = p.versions().join(&m.horizon.tag);
    if !base.join(".installed").exists() {
        progress(&format!("Installing Horizon {}", m.horizon.tag), None);
        run_cli(p, &["install", &m.horizon.tag])?;
    }
    stop()?;
    if !base.join(".local").join("horizon-client").is_dir() {
        progress(&format!("Preparing Minecraft {} files", m.horizon.game), None);
        // Minecraft's own files come from a launcher that has them, else from Mojang for the signed-in owner.
        if run_cli(p, &["prepare", "--version", &m.horizon.tag]).is_err() {
            if accounts(p).unwrap_or_default().is_empty() {
                return Err(format!(
                    "Minecraft {0} isn't on this computer yet. Sign in with the account that owns Minecraft: Java Edition \
                     and install again (Sunset then downloads it from Mojang), or play {0} once in the official launcher",
                    m.horizon.game
                ));
            }
            progress(&format!("Downloading Minecraft {} for your account", m.horizon.game), None);
            run_cli(p, &["prepare", "--download", "--version", &m.horizon.tag])?;
        }
    }
    stop()?;
    let game = m.game()?;
    let cached = p.downloads().join(&game.file);
    let cache_ok = cached.is_file() && sha256_file(&cached).is_ok_and(|h| h.eq_ignore_ascii_case(&game.sha256));
    if !cache_ok {
        let sig = fetch(&format!("{}/{}", rel.base, game.signature))?;
        download(&format!("{}/{}", rel.base, game.file), &cached, &game.sha256, Some(&String::from_utf8_lossy(&sig)), &format!("Sunset {}", m.version), cancel, progress)?;
    }
    let vdir = p.versions().join(&m.version_dir);
    if repair || !vdir.join(".installed").exists() {
        progress(&format!("Unpacking Sunset {}", m.version), None);
        unpack_game(&cached, &p.versions(), &m.version_dir, cancel)?;
    }
    stop()?;
    progress("Setting up the sunset instance", None);
    if !p.instance().is_dir() {
        run_cli(p, &["instance", "create", INSTANCE])?;
    }
    run_cli(p, &["instance", "version", INSTANCE, &m.version_dir])?;
    run_cli(p, &["instance", "select", INSTANCE])?;
    // The previous Sunset version and its cached package go once the new one is in place.
    if let Some(old) = installed(p).filter(|o| o.version_dir != m.version_dir) {
        let _ = fs::remove_dir_all(p.versions().join(&old.version_dir));
        if let Some(g) = old.game().ok().filter(|g| g.file != game.file) {
            let _ = fs::remove_file(p.downloads().join(&g.file));
        }
    }
    write_atomic(&p.file("installed.json"), &rel.raw)?;
    progress("Ready to play", Some(1.0));
    Ok(())
}

/// Unpacks the package (one `<version_dir>/` folder) beside the old copy, then swaps it in.
fn unpack_game(archive: &Path, versions: &Path, vdir: &str, cancel: &AtomicBool) -> Result<()> {
    let tmp = versions.join(format!(".{vdir}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let f = io::BufReader::new(fs::File::open(archive).map_err(|e| e.to_string())?);
    let dec = ruzstd::decoding::StreamingDecoder::new(f).map_err(|e| format!("Sunset package: {e}"))?;
    let mut ar = tar::Archive::new(dec);
    ar.set_preserve_permissions(true);
    for entry in ar.entries().map_err(|e| format!("Sunset package: {e}"))? {
        if cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_dir_all(&tmp);
            return Err("cancelled".into());
        }
        let mut entry = entry.map_err(|e| format!("Sunset package: {e}"))?;
        // unpack_in refuses absolute paths and `..`; the top folder must be the version's.
        let first = entry.path().ok().and_then(|p| p.components().next().map(|c| c.as_os_str().to_owned()));
        if first.as_deref() != Some(std::ffi::OsStr::new(vdir)) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(format!("the Sunset package holds files outside {vdir}/"));
        }
        entry.unpack_in(&tmp).map_err(|e| format!("Sunset package: {e}"))?;
    }
    let dest = versions.join(vdir);
    let _ = fs::remove_dir_all(&dest);
    fs::rename(tmp.join(vdir), &dest).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&tmp);
    if !dest.join(".installed").exists() {
        return Err("the Sunset package is incomplete (no .installed)".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- play

/// Starts the game through the CLI (it adds the signed-in account), with the window settings.
pub fn play(p: &Paths, s: &Settings) -> Result<Child> {
    let mut c = cli(p)?;
    c.args(["play", "--instance", INSTANCE]);
    for a in s.game_args() {
        c.arg(format!("--arg={a}"));
    }
    c.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("can't start horizon-launcher: {e}"))
}

// ---------------------------------------------------------------- what the instance holds

pub fn profiles(p: &Paths) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(p.sunset_data().join("profiles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".profile").map(str::to_string))
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

/// The native mods built into the installed version (`native-mods.json`): Sunset and the plugins compiled into it.
pub fn built_in_mods(p: &Paths, m: &Manifest) -> Vec<(String, String)> {
    let Ok(b) = fs::read(p.versions().join(&m.version_dir).join("native-mods.json")) else { return vec![] };
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap_or_default();
    v["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["version"].as_str().unwrap_or("").to_string())))
        .collect()
}

// ---------------------------------------------------------------- import from Horizon

/// The stock launcher's instances (its own homes) that have Sunset configs or profiles: (name, game folder).
pub fn import_sources(p: &Paths) -> Vec<(String, PathBuf)> {
    let Some(exe) = cli_path(p) else { return vec![] };
    let mut c = Command::new(exe);
    c.args(["instance", "list", "--json"]).env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1").env_remove("HORIZON_LAUNCHER_HOME").env_remove("HORIZON_HOME").stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let Ok(out) = c.output() else { return vec![] };
    let list: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    list.iter()
        .filter_map(|i| Some((i["name"].as_str()?.to_string(), PathBuf::from(i["home"].as_str()?))))
        .filter(|(_, home)| home != &p.instance() && has_sunset_data(&sunset_data_in(home)))
        .collect()
}

fn has_sunset_data(d: &Path) -> bool {
    d.join("config").is_file() || d.join("profiles").is_dir()
}

/// Copies Sunset's config and profiles (never accounts) from another game folder. Files already here are kept
/// unless `overwrite`; returns how many files were copied.
pub fn import_from(p: &Paths, game_dir: &Path, overwrite: bool) -> Result<usize> {
    let from = sunset_data_in(game_dir);
    let to = p.sunset_data();
    if !has_sunset_data(&from) {
        return Err(format!("no Sunset configs in {}", game_dir.display()));
    }
    let mut n = 0;
    for rel in ["config", "profiles", "exports"] {
        n += copy_tree(&from.join(rel), &to.join(rel), overwrite).map_err(|e| format!("import: {e}"))?;
    }
    Ok(n)
}

fn copy_tree(from: &Path, to: &Path, overwrite: bool) -> io::Result<usize> {
    let meta = match fs::symlink_metadata(from) {
        Ok(m) => m,
        Err(_) => return Ok(0),
    };
    if meta.is_file() {
        if to.exists() && !overwrite {
            return Ok(0);
        }
        fs::create_dir_all(to.parent().unwrap())?;
        fs::copy(from, to)?;
        return Ok(1);
    }
    if !meta.is_dir() {
        return Ok(0); // symlinks and the like stay behind
    }
    let mut n = 0;
    for e in fs::read_dir(from)? {
        let e = e?;
        n += copy_tree(&e.path(), &to.join(e.file_name()), overwrite)?;
    }
    Ok(n)
}

// ---------------------------------------------------------------- skin head

/// The 8x8 face (hat layer over it) of the account's skin as RGBA rows, from Mojang's session server.
pub fn skin_face(uuid: &str) -> Result<Vec<[u8; 4]>> {
    use base64::Engine;
    let id: String = uuid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if id.len() != 32 {
        return Err("bad uuid".into());
    }
    let profile: serde_json::Value = serde_json::from_slice(&fetch(&format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"))?).map_err(|e| e.to_string())?;
    let tex = profile["properties"].as_array().into_iter().flatten().find(|p| p["name"] == "textures").and_then(|p| p["value"].as_str()).ok_or("no textures")?;
    let tex: serde_json::Value = serde_json::from_slice(&base64::engine::general_purpose::STANDARD.decode(tex).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let url = tex["textures"]["SKIN"]["url"].as_str().ok_or("no skin")?;
    let url = url.replacen("http://", "https://", 1);
    if !url.starts_with("https://textures.minecraft.net/") {
        return Err("unexpected skin host".into());
    }
    face_from_png(&fetch(&url)?)
}

pub fn face_from_png(bytes: &[u8]) -> Result<Vec<[u8; 4]>> {
    let mut dec = png::Decoder::new(io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("png too big")?];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let ch = info.color_type.samples();
    let (w, h) = (info.width as usize, info.height as usize);
    if w < 64 || h < 32 || ch < 3 {
        return Err("not a skin".into());
    }
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * ch;
        [buf[i], buf[i + 1], buf[i + 2], if ch == 4 { buf[i + 3] } else { 255 }]
    };
    let mut face = Vec::with_capacity(64);
    for y in 0..8 {
        for x in 0..8 {
            let (b, t) = (px(8 + x, 8 + y), px(40 + x, 8 + y));
            let a = t[3] as u32;
            let mix = |i: usize| ((t[i] as u32 * a + b[i] as u32 * (255 - a)) / 255) as u8;
            face.push([mix(0), mix(1), mix(2), 255]);
        }
    }
    Ok(face)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_key_parses() {
        let _ = public_key();
    }

    #[test]
    fn sign_in_line() {
        let l = "To sign in, open https://www.microsoft.com/link in a browser and enter the code 4CQ98ZTM";
        assert_eq!(parse_sign_in_line(l), Some(("https://www.microsoft.com/link".into(), "4CQ98ZTM".into())));
        let again = "To sign Steve in again, open https://www.microsoft.com/link in a browser and enter the code AB-12";
        assert_eq!(parse_sign_in_line(again).unwrap().1, "AB-12");
        assert_eq!(parse_sign_in_line("Signed in as Steve."), None);
        assert_eq!(parse_sign_in_line("open http://evil in a browser and enter the code X"), None);
        assert_eq!(parse_sign_in_line("To sign in, open https://x in a browser and enter the code "), None);
    }

    #[test]
    fn ldd_and_hints() {
        let out = "\tlinux-vdso.so.1 (0x0)\n\tlibxkbcommon.so.0 => not found\n\tlibxkbcommon-x11.so.0 => not found\n\tlibxcb.so.1 => not found\n\tlibc.so.6 => /lib/libc.so.6\n";
        let libs = parse_ldd(out);
        assert_eq!(libs, ["libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libxcb.so.1"]);
        assert_eq!(install_hint_for("ID=ubuntu\nID_LIKE=debian\n", &libs), "sudo apt install libxkbcommon0 libxkbcommon-x11-0 libxcb1");
        assert_eq!(install_hint_for("ID=fedora\n", &libs), "sudo dnf install libxkbcommon libxkbcommon-x11 libxcb");
        assert_eq!(install_hint_for("ID=\"arch\"\n", &libs), "sudo pacman -S libxkbcommon libxkbcommon-x11 libxcb");
        assert!(install_hint_for("ID=gentoo\n", &libs).contains("libxcb.so.1"));
        assert!(parse_ldd("\tlibc.so.6 => /lib/libc.so.6\n").is_empty());
    }

    #[test]
    fn window_args() {
        let mut s = Settings::default();
        assert_eq!(s.game_args(), ["--width", "1280", "--height", "720"]);
        s.window = "fullscreen".into();
        assert_eq!(s.game_args(), ["--fullscreen"]);
        s.window = "junk".into();
        assert!(s.game_args().is_empty());
    }

    fn manifest() -> Manifest {
        serde_json::from_str(&format!(
            r#"{{"schema":1,"id":"sunset","version":"1.2.1","platform":"{PLATFORM}","version_dir":"native-x",
            "horizon":{{"revision":"0b14f8acfd","tag":"nightly-2026-11-18-0b14f8a","game":"26.3"}},
            "cli":{{"version":"0.2.78","file":"horizon-launcher-linux-x86_64.tar.gz","url":"https://github.com/horizon-rust/horizon/releases/download/launcher@0.2.78/h.tar.gz","sha256":"{0}"}},
            "artifacts":[{{"file":"a.tar.zst","kind":"game","size":1,"sha256":"{0}","signature":"a.tar.zst.minisig"}}]}}"#,
            "ab".repeat(32)
        ))
        .unwrap()
    }

    #[test]
    fn changelog_is_optional_and_read_when_present() {
        assert!(manifest().changelog.is_empty());
        let mut v: serde_json::Value = serde_json::to_value(manifest()).unwrap();
        v["changelog"] = serde_json::json!([{"kind": "Fixed", "text": "Fonts", "important": true}, {"text": "Bare"}]);
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!((m.changelog[0].kind.as_str(), m.changelog[0].important), ("Fixed", true));
        assert_eq!((m.changelog[1].kind.as_str(), m.changelog[1].important), ("", false));
        assert!(m.check().is_ok());
    }

    #[test]
    fn manifest_checks() {
        assert!(manifest().check().is_ok());
        let mut m = manifest();
        m.version_dir = "../../evil".into();
        assert!(m.check().is_err());
        let mut m = manifest();
        m.platform = "macos-arm64".into();
        assert!(m.check().is_err());
        let mut m = manifest();
        m.cli.url = "https://evil.example/h.tar.gz".into();
        assert!(m.check().is_err());
        let mut m = manifest();
        m.artifacts[0].file = "a/b".into();
        assert!(m.check().is_err());
        let mut m = manifest();
        m.artifacts.clear();
        assert!(m.check().is_err());
    }

    #[test]
    fn bad_signature_refused() {
        assert!(verify_signature(b"x", "junk").is_err());
    }

    #[test]
    fn import_keeps_existing() {
        let t = std::env::temp_dir().join(format!("sunset-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let src = t.join("src");
        let d = sunset_data_in(&src);
        fs::create_dir_all(d.join("profiles")).unwrap();
        fs::write(d.join("config"), "a=1").unwrap();
        fs::write(d.join("profiles/pvp.profile"), "p").unwrap();
        let p = Paths { root: t.join("home") };
        fs::create_dir_all(p.sunset_data()).unwrap();
        fs::write(p.sunset_data().join("config"), "mine").unwrap();
        assert_eq!(import_from(&p, &src, false).unwrap(), 1);
        assert_eq!(fs::read_to_string(p.sunset_data().join("config")).unwrap(), "mine");
        assert_eq!(profiles(&p), ["pvp"]);
        assert_eq!(import_from(&p, &src, true).unwrap(), 2);
        assert_eq!(fs::read_to_string(p.sunset_data().join("config")).unwrap(), "a=1");
        assert!(import_from(&p, &t.join("nothing"), false).is_err());
        fs::remove_dir_all(&t).unwrap();
    }
}
