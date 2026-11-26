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

// ---------------------------------------------------------------- licences

/// Sunset's licence (the repository's LICENSE), which the launcher is under.
const SUNSET_LICENCE: &str = include_str!("../../LICENSE");
/// launcher/THIRD-PARTY-LICENSES.txt, compressed by build.rs.
const THIRD_PARTY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/third-party.zst"));
pub const HORIZON_NOTICE: &str = "Horizon (https://github.com/horizon-rust/horizon) is licensed under the GNU Lesser General Public License, version 3 or later. The launcher runs the stock horizon-launcher, downloaded from Horizon's own releases, as a separate program and links nothing from it. The prebuilt Sunset client it installs contains Horizon, modified by Sunset: its game folder's licences/ has the Horizon notice (copyright, the exact source and how to rebuild or relink it), the LGPL and GPL texts, PREBUILT-TERMS.md (your right to modify the Horizon part and to reverse engineer the build to debug that) and its third-party licences, and each release carries the same files with Horizon's source.";

/// Everything `sunset-launcher licences` prints and About opens: Sunset's licence, the Horizon notice, and the
/// launcher's third-party licences.
pub fn licences() -> String {
    let mut third = String::new();
    if let Err(e) = ruzstd::decoding::StreamingDecoder::new(THIRD_PARTY).map_err(|e| e.to_string()).and_then(|mut d| std::io::Read::read_to_string(&mut d, &mut third).map_err(|e| e.to_string())) {
        third = format!("(the embedded third-party licences could not be read: {e}; they are THIRD-PARTY-LICENSES.txt in the launcher's source)\n");
    }
    let rule = "=".repeat(100);
    format!("Sunset launcher {VERSION}: licences\n\nThe Sunset launcher is licensed under the PolyForm Strict License 1.0.0 (Sunset's licence, below).\n\n{HORIZON_NOTICE}\n\n{rule}\nSunset's licence\n{rule}\n\n{SUNSET_LICENCE}\n\n{third}")
}

/// `licences()` written to `<data>/licences/LICENCES.txt`, for the system's text viewer.
pub fn write_licences(p: &Paths) -> Result<PathBuf> {
    let dir = p.root.join("licences");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("LICENCES.txt");
    fs::write(&path, licences()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------- paths and settings

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

/// The per-user data folder (Linux `$XDG_DATA_HOME` or `~/.local/share`, Windows `%APPDATA%`): the stock launcher's
/// `dirs::data_dir()` too.
fn data_base() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }
}

impl Paths {
    /// `SUNSET_LAUNCHER_HOME`, else `<data dir>/sunset-launcher` (Linux `~/.local/share`, Windows `%APPDATA%`).
    pub fn new() -> Self {
        if let Some(home) = std::env::var_os("SUNSET_LAUNCHER_HOME").filter(|h| !h.is_empty()) {
            return Paths { root: PathBuf::from(home) };
        }
        Paths { root: data_base().unwrap_or_else(std::env::temp_dir).join("sunset-launcher") }
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
    /// When the launcher last started the game (Unix seconds); None until it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
    /// Seconds the game ran when started from this launcher, counted when it exits.
    #[serde(skip_serializing_if = "is_zero")]
    pub playtime: u64,
    /// Plugins are built into Sunset (Plugins tab). Off until the player turns it on: until then the instance plays the
    /// signed build exactly as released.
    pub plugins_enabled: bool,
    /// Saved offline (cracked) nicknames. Offline play needs a Microsoft account that owns Minecraft
    /// to have signed in at least once (`owner_ever`); the game then joins offline-mode servers under
    /// the chosen name, with that name's offline UUID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offline_names: Vec<String>,
    /// The offline name Play starts the game as; `None` plays with the Microsoft account instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offline_active: Option<String>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_update: true,
            window: "1280x720".into(),
            import_decided: false,
            manifest_url: None,
            last_played: None,
            playtime: 0,
            plugins_enabled: false,
            offline_names: vec![],
            offline_active: None,
        }
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

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
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
    /// The Horizon build as people say it: its release version when the pin is a release ("v0.4.1" -> "0.4.1"), else
    /// "Nightly". Never the bare commit.
    pub fn horizon_build(&self) -> String {
        horizon_build(&self.horizon.tag)
    }
    fn game(&self) -> Result<&Artifact> {
        self.artifact("game").ok_or_else(|| "the manifest names no game package".to_string())
    }
    /// The artifact of `kind`: "game" (the version folder), and for plugin builds "source" (Sunset's .hmod package)
    /// and "sdk" (the mod SDK of Sunset's Horizon fork, when its hooks need one).
    pub fn artifact(&self, kind: &str) -> Option<&Artifact> {
        self.artifacts.iter().find(|a| a.kind == kind)
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
        // The stock CLI comes from upstream's releases, or from Sunset's own (a mirror of the same archive with its
        // source: upstream's repository has been unreachable). Either way the signed manifest's hash decides.
        let from = |base: &str| self.cli.url.starts_with(&format!("https://github.com/{base}/releases/download/"));
        if !hex(&self.cli.sha256) || !(from("horizon-rust/horizon") || from(REPO)) {
            return Err("the manifest's horizon-launcher entry is not a signed upstream or Sunset download".into());
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
            if let Some(a) = assets.iter().find(|a| a["name"].as_str() == Some(name))
                && let Some(u) = a["browser_download_url"].as_str() {
                    return Ok(u.to_string());
                }
        }
    }
    Err(format!("no Sunset client release for {PLATFORM} yet (github.com/{REPO}/releases)"))
}

/// A manifest that passed its signature check, before its contents are checked: the launcher's own update
/// (selfupdate.rs) reads its entry from it even when this launcher can't install the rest.
#[derive(Clone, Debug)]
pub struct Signed {
    pub raw: Vec<u8>,
    pub base: String,
}

/// Fetches the manifest and its `.minisig` and checks the signature with the built-in key.
pub fn fetch_signed(settings: &Settings) -> Result<Signed> {
    let url = manifest_url(settings)?;
    let raw = fetch(&url)?;
    let sig = fetch(&format!("{url}.minisig"))?;
    verify_signature(&raw, &String::from_utf8_lossy(&sig)).map_err(|_| "the release failed its signature check, so it was refused".to_string())?;
    let base = url.rsplit_once('/').map(|(b, _)| b.to_string()).unwrap_or_default();
    Ok(Signed { raw, base })
}

impl Release {
    /// The signed manifest's contents, checked.
    pub fn from_signed(s: Signed) -> Result<Release> {
        let manifest: Manifest = serde_json::from_slice(&s.raw).map_err(|e| format!("unreadable manifest: {e}"))?;
        manifest.check()?;
        Ok(Release { manifest, raw: s.raw, base: s.base })
    }
}

/// Fetches the manifest and its `.minisig`, checks the signature with the built-in key, then the contents.
pub fn fetch_release(settings: &Settings) -> Result<Release> {
    Release::from_signed(fetch_signed(settings)?)
}

/// Where the installed release's files were downloaded from (written by `install`), for the plugin build's downloads.
pub fn installed_base(p: &Paths) -> Option<String> {
    fs::read_to_string(p.file("installed-base")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// The manifest of what is installed, or None.
pub fn installed(p: &Paths) -> Option<Manifest> {
    let m: Manifest = serde_json::from_slice(&fs::read(p.file("installed.json")).ok()?).ok()?;
    p.versions().join(&m.version_dir).is_dir().then_some(m)
}

// ---------------------------------------------------------------- network

const READ_TIMEOUT: u64 = 30;

/// The host of an address, for the "can't reach" error.
fn host(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, r)| r).split('/').next().unwrap_or(url)
}

fn agent() -> ureq::Agent {
    // A read that gets nothing for READ_TIMEOUT fails, so a stalled download ends instead of hanging (Cancel is only
    // seen between reads).
    ureq::AgentBuilder::new()
        .user_agent(&format!("sunset-launcher/{VERSION}"))
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(READ_TIMEOUT))
        .build()
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
        // ureq's own text already starts with the address and goes on about DNS lookups.
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => format!("can't reach {}. Check the internet connection", host(url)),
            k => format!("{url}: {}", t.message().map(str::to_string).unwrap_or_else(|| k.to_string())),
        },
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

/// A signed artifact of the release at `base`, in `p.downloads()`: kept when its SHA-256 still matches, else downloaded and
/// checked against its signature. Returns the file.
pub(crate) fn fetch_artifact(p: &Paths, base: &str, a: &Artifact, what: &str, cancel: &AtomicBool, progress: Progress) -> Result<PathBuf> {
    let cached = p.downloads().join(&a.file);
    if cached.is_file() && sha256_file(&cached).is_ok_and(|h| h.eq_ignore_ascii_case(&a.sha256)) {
        return Ok(cached);
    }
    let sig = fetch(&format!("{base}/{}", a.signature))?;
    download(&format!("{base}/{}", a.file), &cached, &a.sha256, Some(&String::from_utf8_lossy(&sig)), what, cancel, progress)?;
    Ok(cached)
}

/// Downloads `url` to `dest`, refusing it unless its SHA-256 is `sha256` and (with `sig`) its minisign signature holds.
pub(crate) fn download(url: &str, dest: &Path, sha256: &str, sig: Option<&str>, what: &str, cancel: &AtomicBool, progress: Progress) -> Result<()> {
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
        let n = match r.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                // Nothing resumes a partial file, so it goes.
                drop(out);
                let _ = fs::remove_file(&part);
                return Err(match e.kind() {
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => format!("{what}: the download stalled (nothing arrived for {READ_TIMEOUT} seconds)"),
                    _ => format!("{what}: the download broke off ({e})"),
                });
            }
        };
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
    cli_result(args, out.status, &out.stdout, &out.stderr)
}

fn cli_result(args: &[&str], status: std::process::ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<String> {
    if status.success() {
        return Ok(String::from_utf8_lossy(stdout).into_owned());
    }
    let tail = error_tail(&String::from_utf8_lossy(stderr));
    Err(format!("horizon-launcher {} failed: {}", args.first().unwrap_or(&""), if tail.is_empty() { status.to_string() } else { tail }))
}

/// The CLI's last error lines, for a message. Its download counter ("67 of 76 files", redrawn in place with \r) is
/// left out, and so is a line said twice in a row; when a line starts with "error", the message starts there.
fn error_tail(stderr: &str) -> String {
    fn counter(l: &str) -> bool {
        let mut w = l.split_whitespace();
        let num = |s: Option<&str>| s.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
        num(w.next()) && w.next() == Some("of") && num(w.next())
    }
    let mut lines: Vec<&str> = Vec::new();
    for l in stderr.split(['\n', '\r']).map(str::trim) {
        // A counter the error was printed right after, on the same line.
        let l = match l.find("error:") {
            Some(i) if i > 0 && counter(&l[..i]) => &l[i..],
            _ => l,
        };
        if !l.is_empty() && !(counter(l) && !l.contains("error")) && lines.last() != Some(&l) {
            lines.push(l);
        }
    }
    let start = lines
        .iter()
        .rposition(|l| l.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("error")))
        .unwrap_or(lines.len().saturating_sub(3));
    lines[start..].iter().take(3).copied().collect::<Vec<_>>().join(" ")
}

/// Whether a failed step failed on the network (a dropped, reset or refused connection, a timeout, a name that did
/// not resolve), so trying it again can help. KRAMMC's download of Minecraft stopped on Windows' 10053.
pub fn network_error(e: &str) -> bool {
    let e = e.to_ascii_lowercase();
    [
        "os error 10053", "os error 10054", "os error 10060", "os error 10061", "os error 11001", "os error 104", "os error 110",
        "os error 111", "connection reset", "connection aborted", "connection refused", "connection closed", "timed out",
        "broken pipe", "unexpected eof", "dns error", "failed to lookup address", "error sending request", "temporary failure in name resolution",
    ]
    .iter()
    .any(|k| e.contains(k))
}

/// `run_cli` for the long steps (prepare and its downloads): the CLI runs as its own process tree, and setting
/// `cancel` ends it and everything it started (horizon-prepare, its exporters) and returns "cancelled".
pub fn run_cli_cancellable(p: &Paths, args: &[&str], cancel: &AtomicBool) -> Result<String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let mut c = cli(p)?;
    c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut tree = crate::plugins::proc_tree::Tree::spawn(&mut c).map_err(|e| format!("can't start horizon-launcher: {e}"))?;
    // Both pipes are drained as the CLI writes, so a chatty step can't fill one and stall.
    fn drain(r: Option<impl Read + Send + 'static>) -> Option<std::thread::JoinHandle<Vec<u8>>> {
        r.map(|mut r| {
            std::thread::spawn(move || {
                let mut b = Vec::new();
                let _ = r.read_to_end(&mut b);
                b
            })
        })
    }
    let (out, err) = (drain(tree.child.stdout.take()), drain(tree.child.stderr.take()));
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            tree.kill();
            let _ = tree.child.wait();
            return Err("cancelled".into());
        }
        match tree.child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => {
                tree.kill();
                return Err(format!("horizon-launcher {}: {e}", args.first().unwrap_or(&"")));
            }
        }
    };
    // Anything the CLI left running goes with the tree, so the pipes close and the readers end.
    drop(tree);
    let read = |h: Option<std::thread::JoinHandle<Vec<u8>>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
    let (out, err) = (read(out), read(err));
    cli_result(args, status, &out, &err)
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

// ---------------------------------------------------------------- offline (cracked) play

fn client_name() -> &'static str {
    if cfg!(windows) { "horizon-client.exe" } else { "horizon-client" }
}

/// A Minecraft name: 3 to 16 of letters, digits and underscore. An offline-mode server derives the
/// player's UUID from the name (vanilla's `OfflinePlayer:<name>`, which Horizon calls `offline_uuid`),
/// so the launcher keeps to the same rule a server will accept.
pub fn valid_offline_name(name: &str) -> bool {
    let n = name.chars().count();
    (3..=16).contains(&n) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether a Microsoft account that owns Minecraft: Java Edition has signed in here at least once.
/// Offline play is only for owners: Horizon's Minecraft files are prepared from the owner's own copy,
/// so a chosen nick is never a way to play without owning the game. The answer is sticky — a `.owned`
/// marker is written the first time an owner is seen — so signing the account out later does not take
/// offline play away ("logged in at least once").
pub fn owner_ever(p: &Paths) -> bool {
    if p.file(".owned").is_file() {
        return true;
    }
    if cli_owns_java(p) {
        let _ = write_atomic(&p.file(".owned"), b"");
        return true;
    }
    false
}

/// Asks the stock CLI whether a signed-in account owns Java Edition (`account list --json` reports
/// `owner`). Any trouble reaching it reads as "not an owner yet", never a false yes.
fn cli_owns_java(p: &Paths) -> bool {
    #[derive(Deserialize)]
    struct List {
        #[serde(default)]
        owner: bool,
    }
    cli_path(p).is_some()
        && run_cli(p, &["account", "list", "--json"]).ok().and_then(|out| serde_json::from_str::<List>(&out).ok()).is_some_and(|l| l.owner)
}

/// Starts the game offline under `name`, by running the installed `horizon-client` directly: the stock
/// CLI's `play` only starts a signed-in Microsoft account, so an offline launch cannot go through it.
/// Same instance, resources and window as a normal start, but no account session, so the game joins
/// offline-mode servers as `name`. Refused until an owning account has signed in ([`owner_ever`]).
///
/// What the stock CLI adds on its own and this start cannot: on Windows it runs the game from a
/// fixed runtime slot and holds run leases, so here the game runs from its version folder, whose
/// files stay in use until it exits (the window refuses Install and Repair while it plays).
pub fn play_offline(p: &Paths, s: &Settings, name: &str) -> Result<Child> {
    if !valid_offline_name(name) {
        return Err("An offline name is 3 to 16 letters, digits or underscores.".into());
    }
    if !owner_ever(p) {
        return Err(OWNER_NEEDED.into());
    }
    let m = installed(p).ok_or("Sunset isn't installed yet.")?;
    let version = p.versions().join(offline_version_dir(p, &m));
    let exe = version.join(client_name());
    if !exe.is_file() {
        return Err("The game isn't installed yet. Install or repair Sunset first.".into());
    }
    let resources = resources(p, &m);
    if !is_prepared(&resources) {
        return Err(NOT_PREPARED.into());
    }
    drop_stale_data_file(&resources);
    // Best effort: when it fails, the game says what it could not hold.
    let _ = register_leases(p, &[&version, &resources]);
    let instance = p.instance();
    fs::create_dir_all(&instance).map_err(|e| format!("can't make {}: {e}", instance.display()))?;
    let mut c = Command::new(&exe);
    // As the stock CLI's client_command: the launcher's own renderer workarounds and account
    // handover stay out of the game, and the "I'm playing" ping follows Horizon's setting.
    for key in ["LIBGL_ALWAYS_SOFTWARE", "GALLIUM_DRIVER", "WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE", "HORIZON_ACCOUNT_SESSIONS", "HORIZON_PLAYER_NAME"] {
        c.env_remove(key);
    }
    if horizon_ping_off(p) {
        c.env("HORIZON_NO_HEARTBEAT", "1");
    }
    if cfg!(target_os = "linux") && std::env::var_os("PIPEWIRE_PROPS").is_none() {
        c.env("PIPEWIRE_PROPS", "{ application.name = \"Horizon\" media.role = \"Game\" }");
    }
    c.current_dir(&instance)
        .env("HORIZON_HOME", &instance)
        // The launcher-owned marker for the version folder the running game's files come from, so a
        // Repair that would replace them can tell the game is using them (run_leases.rs `SOURCE`).
        .env("HORIZON_RUNTIME_SOURCE", &version)
        .arg("--resources").arg(&resources)
        .arg("--game-dir").arg(&instance)
        .arg("--home").arg(&instance)
        .arg("--name").arg(name);
    for a in s.game_args() {
        c.arg(a);
    }
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: the window has no console to share
    }
    c.spawn().map_err(|e| format!("can't start the game: {e}"))
}

/// Registers `dirs` with Horizon's run-lease registry (`<horizon>/run-leases`) the way the stock CLI's Play does
/// before it starts the game. A game started directly holds a lease on its version folder and on its resources when
/// the registry exists, and refuses to start ("unregistered cache directories") when the registry has no entry for one
/// of them: offline play before any signed-in Play (Hirox's report). An entry is an empty file named after the SHA-256
/// of the folder's "device:file id" (Horizon's runtime_leases.rs `Registry::lock`, with cap-fs-ext's `dev`/`ino`:
/// the volume serial number and file index on Windows). Without a registry the game holds no leases: nothing to do.
fn register_leases(p: &Paths, dirs: &[&Path]) -> io::Result<()> {
    let registry = p.horizon().join("run-leases");
    if !registry.is_dir() {
        return Ok(());
    }
    // Each folder and the ones above it inside the CLI's data, as the game holds them (horizon-mod-package
    // game_entry.rs lease_paths, from 2e7d2c6 on: <base>/.local and <base>/.local/horizon-client too).
    let data = fs::canonicalize(p.horizon())?;
    let mut held: Vec<PathBuf> = Vec::new();
    for d in dirs {
        let d = fs::canonicalize(d)?;
        for a in d.ancestors().take_while(|a| a.starts_with(&data) && *a != data) {
            if !held.iter().any(|h| h == a) {
                held.push(a.to_path_buf());
            }
        }
    }
    for d in &held {
        let (dev, ino) = file_id(d)?;
        let key = hex(&Sha256::digest(format!("{dev}:{ino}").as_bytes()));
        match fs::OpenOptions::new().write(true).create_new(true).open(registry.join(key)) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// A folder's (device, file id), as Horizon's lease registry names it.
#[cfg(unix)]
fn file_id(dir: &Path) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(dir)?;
    Ok((m.dev(), m.ino()))
}

#[cfg(windows)]
fn file_id(dir: &Path) -> io::Result<(u64, u64)> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    #[repr(C)]
    #[derive(Default)]
    struct Info {
        attributes: u32,
        times: [u32; 6],
        volume_serial: u32,
        size: [u32; 2],
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(file: *mut core::ffi::c_void, info: *mut Info) -> i32;
    }
    // FILE_FLAG_BACKUP_SEMANTICS opens a folder.
    let f = fs::OpenOptions::new().read(true).custom_flags(0x0200_0000).open(dir)?;
    let mut info = Info::default();
    // SAFETY: a valid handle that `f` owns, and a BY_HANDLE_FILE_INFORMATION-shaped buffer.
    if unsafe { GetFileInformationByHandle(f.as_raw_handle().cast(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((info.volume_serial as u64, (info.index_high as u64) << 32 | info.index_low as u64))
}

#[cfg(not(any(unix, windows)))]
fn file_id(_: &Path) -> io::Result<(u64, u64)> {
    Err(io::ErrorKind::Unsupported.into())
}

/// Said at Play when the prepared files have no sounds: the game then plays silent.
pub const NO_SOUNDS: &str = "Minecraft's sounds aren't prepared, so the game is silent. Signed in, Repair downloads them from Mojang.";

/// Whether the installed Sunset's prepared files lack the sounds.
pub fn sounds_missing(p: &Paths) -> bool {
    installed(p).is_some_and(|m| resources(p, &m).is_dir() && !has_sounds(&resources(p, &m)))
}

/// What to say when the game stopped with an error. `last` is the last line the game (or the CLI's Play) wrote to
/// stderr: often only Play's step, as "Horizon: Play: checking the game's assets…" was for lexii, whose game had
/// stopped on a graphics device error that only latest.log held. So the log of this run (written since `since`) is
/// read for its last ERROR line, and the message names the log.
pub fn game_failure(p: &Paths, since: std::time::SystemTime, last: &str) -> String {
    if let Some(why) = sign_in_refusal(last) {
        return why;
    }
    if last.contains("is not prepared yet") {
        return NOT_PREPARED.into();
    }
    if last.contains("cannot acquire launcher cache leases") {
        return format!("{last}. Play once with the Microsoft account, then offline play starts too.");
    }
    let log = p.logs().join("latest.log");
    let fresh = fs::metadata(&log).and_then(|m| m.modified()).is_ok_and(|t| t + std::time::Duration::from_secs(2) >= since);
    let error = fresh.then(|| fs::read_to_string(&log).ok()).flatten().and_then(|text| log_error(&text));
    match error {
        Some(e) => {
            let gpu = e.contains("graphics device") || e.contains("graphics driver") || e.contains("GPU");
            format!(
                "The game crashed: {e}{}\nIts log: {}",
                if gpu { "\nUpdating the graphics driver usually fixes this. Settings → Import from Horizon also brings over the graphics settings of a Horizon install that works." } else { "" },
                log.display()
            )
        }
        None => match last.strip_prefix("Horizon: Play: ") {
            Some(step) => format!("The game stopped while {}. Its log: {}", step.trim_end_matches(['…', '.']), log.display()),
            None if last.is_empty() => "The game stopped with an error".into(),
            None => last.into(),
        },
    }
}

/// The stock CLI's own sign-in command, which its refusals end on (the words around it follow the system's language).
const CLI_SIGN_IN: &str = "`horizon-launcher account sign-in`";

/// Whether a message asks for a sign-in, so the window puts Sign in beside it.
pub fn needs_sign_in(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("sign in again") || m.contains("sign in with")
}

/// Why Play was refused for want of a sign-in, from the CLI's last line, without its "error: " and the sentence naming
/// its own command: the window has a button for that, and the terminal its own command. None for any other error.
pub fn sign_in_refusal(last: &str) -> Option<String> {
    let e = last.trim().strip_prefix("error: ")?;
    let why = match e.find(CLI_SIGN_IN) {
        Some(at) => e[..at].rfind(". ").map_or("", |end| &e[..=end]),
        None if needs_sign_in(e) => e,
        None => return None,
    };
    Some(if needs_sign_in(why) { why.into() } else { format!("{why} Sign in again.").trim_start().into() })
}

/// Why the CLI started the game without a session (an expired sign-in of an account that owns it, no network).
pub fn offline_reason(line: &str) -> Option<&str> {
    line.strip_prefix("Playing offline: ")
}

/// Whether a line of Play's stderr shows the game itself running: the client first logs "Horizon <version>", while the
/// CLI's own lines start "Horizon: " (or "Horizon launcher: "). Spawning the CLI alone is not a game played.
pub fn game_started(line: &str) -> bool {
    line.strip_prefix("Horizon ").is_some_and(|rest| !rest.starts_with("launcher"))
}

/// The last ERROR (or FATAL) line of a game log (`[time] [thread/ERROR]: message`), or else its last panic line,
/// shortened for the window.
fn log_error(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let error = lines
        .iter()
        .rev()
        .find_map(|l| ["/ERROR]: ", "/FATAL]: "].iter().find_map(|k| l.find(k).map(|i| l[i + k.len()..].trim())))
        .or_else(|| lines.iter().rev().find(|l| l.contains("panicked at")).map(|l| l.trim()))?;
    let mut e: String = error.chars().take(300).collect();
    if e.len() < error.len() {
        e.push('…');
    }
    Some(e)
}

pub const NOT_PREPARED: &str = "Minecraft's files aren't fully prepared (an earlier prepare stopped half way). Repair prepares them again.";

pub const OWNER_NEEDED: &str = "Offline play needs a Microsoft account that owns Minecraft: Java Edition to have signed in at least once.";

/// The version folder the instance plays: a plugin build when plugins are built in (plugins.rs selects
/// it with `instance version`), else the signed release. Only a plain folder name that holds the game counts.
fn offline_version_dir(p: &Paths, m: &Manifest) -> String {
    crate::plugins::instance_version(p)
        .filter(|v| plain_folder_name(v) && p.versions().join(v).join(client_name()).is_file())
        .unwrap_or_else(|| m.version_dir.clone())
}

fn plain_folder_name(v: &str) -> bool {
    !v.is_empty() && v != "." && v != ".." && !v.contains(['/', '\\', ':'])
}

/// Horizon's own "I'm playing" ping switch (`ping_off` in the stock CLI's settings.json).
fn horizon_ping_off(p: &Paths) -> bool {
    fs::read(p.horizon().join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["ping_off"].as_bool() == Some(true))
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
    // The package comes first: it can stand in for the pinned Nightly when that is gone from GitHub (below).
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
    let base = p.versions().join(&m.horizon.tag);
    if !base.join(".installed").exists() {
        // The base Nightly comes from the signed package, never from Horizon's GitHub: Horizon replaces a day's
        // Nightly when it rebuilds it and deletes the earlier build's release, which broke fresh 2.0.0 installs. The
        // package holds that same revision's preparer, exporters and server. A base the stock CLI installed earlier
        // (it has .installed) stays as it is.
        progress(&format!("Setting up Horizon {} from the Sunset package", m.horizon.tag), None);
        base_from_package(&p.versions(), &m.version_dir, &m.horizon.tag)?;
    }
    stop()?;
    let prepared = base.join(".local").join("horizon-client");
    let res = resources(p, m);
    // A prepare that stopped half way (Cancel, an error) leaves nothing behind, so the next install prepares again.
    // One that failed on top of complete files (the sounds' download below) leaves them as they are.
    let prepare = |args: &[&str]| {
        run_cli_cancellable(p, args, cancel).inspect_err(|_| {
            if !is_prepared(&res) {
                let _ = fs::remove_dir_all(&prepared);
            }
        })
    };
    // Mojang's download can drop on a flaky connection (os error 10053 on Windows): it is tried again twice.
    let download = |what: &str| -> Result<String> {
        let mut tries = 1;
        loop {
            match prepare(&["prepare", "--download", "--version", &m.horizon.tag]) {
                Err(e) if tries < 3 && network_error(&e) => {
                    progress(&format!("{what}: the connection dropped, trying again ({} of 3)", tries + 1), None);
                    for _ in 0..tries * 30 {
                        stop()?;
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    tries += 1;
                }
                Err(e) if network_error(&e) => {
                    return Err(format!("{e}\nThe connection to Mojang dropped 3 times. Check the internet connection (a VPN or firewall can cut long downloads), then Repair."));
                }
                r => return r,
            }
        }
    };
    // Only complete files count as prepared, as Horizon's Play checks them. A prepare that stopped before 2.0.1 (or
    // with the launcher killed) left its folder behind, and Install and Repair took that as prepared (KRAMMC's
    // "is not prepared yet" at Play); what is left of it goes before preparing again.
    if !is_prepared(&res) {
        let _ = fs::remove_dir_all(&prepared);
        progress(&format!("Preparing Minecraft {} files", m.horizon.game), None);
        // Minecraft's own files come from a launcher that has them, else from Mojang for the signed-in owner.
        if prepare(&["prepare", "--version", &m.horizon.tag]).is_err() {
            stop()?;
            if accounts(p).unwrap_or_default().is_empty() {
                return Err(format!(
                    "Minecraft {0} isn't on this computer yet. Sign in with the account that owns Minecraft: Java Edition \
                     and install again (Sunset then downloads it from Mojang), or play {0} once in the official launcher",
                    m.horizon.game
                ));
            }
            let what = format!("Downloading Minecraft {} for your account", m.horizon.game);
            progress(&what, None);
            download(&what)?;
        }
    }
    stop()?;
    if !has_sounds(&res) {
        // horizon-prepare copies the sounds from the Minecraft folder's asset store and leaves them all out when one
        // file is missing there, which leaves the game silent. A stock Horizon install of the same game often has
        // them; else Mojang's store comes with the owner's download.
        progress("Looking for Minecraft's sounds", None);
        if !sounds_from_stock(&res, &m.horizon.game) && !accounts(p).unwrap_or_default().is_empty() {
            let what = format!("Downloading Minecraft {}'s sounds for your account", m.horizon.game);
            progress(&what, None);
            if let Err(e) = download(&what) {
                stop()?;
                progress(&format!("Sounds not downloaded ({e}); Repair tries again"), None);
            }
        }
    }
    drop_stale_data_file(&res);
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
    write_atomic(&p.file("installed-base"), rel.base.as_bytes())?;
    progress("Ready to play", Some(1.0));
    Ok(())
}

/// The prepared Minecraft files the game plays with: the prebuilt native client reads the base Nightly's (the one it
/// was built on), not its own version folder's.
pub fn resources(p: &Paths, m: &Manifest) -> PathBuf {
    p.versions().join(&m.horizon.tag).join(".local").join("horizon-client").join(&m.horizon.game)
}

/// Whether `res` holds complete prepared files, by the stock CLI's own test before Play (backend.rs `prepared_at`):
/// a readable horizon-resources.json, and no empty .json beside it.
pub fn is_prepared(res: &Path) -> bool {
    let manifest = fs::read(res.join("horizon-resources.json")).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    manifest.is_some()
        && fs::read_dir(res).is_ok_and(|entries| {
            entries.flatten().all(|e| {
                let path = e.path();
                path.extension() != Some(std::ffi::OsStr::new("json")) || fs::metadata(&path).is_ok_and(|m| m.len() > 0)
            })
        })
}

const SOUNDS_JSON: &str = "assets/minecraft/sounds.json";

/// Whether prepared resources hold Minecraft's sounds (`horizon-prepare sounds` puts them there).
pub fn has_sounds(res: &Path) -> bool {
    res.join(SOUNDS_JSON).is_file() && res.join("assets/minecraft/sounds").is_dir()
}

/// The asset index entries of the sounds (`minecraft/sounds.json` and `minecraft/sounds/…`), from a prepared
/// folder's `asset-index.json`: two folders with the same entries have the same sound files.
fn sound_entries(res: &Path) -> Option<Vec<(String, String)>> {
    let index: serde_json::Value = serde_json::from_slice(&fs::read(res.join("asset-index.json")).ok()?).ok()?;
    let mut v: Vec<(String, String)> = index["objects"]
        .as_object()?
        .iter()
        .filter(|(k, _)| k.starts_with("minecraft/sounds"))
        .filter_map(|(k, o)| Some((k.clone(), o["hash"].as_str()?.to_string())))
        .collect();
    v.sort();
    (!v.is_empty()).then_some(v)
}

/// Fills `res` with the sounds of a stock Horizon version prepared for the same Minecraft `game` with the same
/// sound assets, as hard links where the disk allows. Whether it did.
fn sounds_from_stock(res: &Path, game: &str) -> bool {
    let Some(versions) = data_base().map(|d| d.join("horizon-launcher").join("versions")) else { return false };
    sounds_from(res, &versions, game)
}

fn sounds_from(res: &Path, versions: &Path, game: &str) -> bool {
    let Some(want) = sound_entries(res) else { return false };
    let Ok(dirs) = fs::read_dir(versions) else { return false };
    for d in dirs.flatten() {
        let from = d.path().join(".local").join("horizon-client").join(game);
        if from == res || !has_sounds(&from) || sound_entries(&from).as_ref() != Some(&want) {
            continue;
        }
        let assets = res.join("assets").join("minecraft");
        let tmp = assets.join(".sounds.tmp");
        let _ = fs::remove_dir_all(&tmp);
        let copied = (|| -> io::Result<()> {
            link_tree(&from.join("assets/minecraft/sounds"), &tmp)?;
            let _ = fs::remove_dir_all(assets.join("sounds"));
            fs::rename(&tmp, assets.join("sounds"))?;
            // sounds.json last: until it is there, the folder reads as without sounds and the next install tries again.
            fs::copy(from.join(SOUNDS_JSON), assets.join(".sounds.json.tmp"))?;
            fs::rename(assets.join(".sounds.json.tmp"), res.join(SOUNDS_JSON))
        })();
        if copied.is_ok() {
            return true;
        }
        let _ = fs::remove_dir_all(&tmp);
    }
    false
}

/// Horizon (0b14f8a and later) answers reads under `assets/` from the prepared folder's `horizon-data.bin`, the copy
/// the game made of it at a first start, and keeps using that copy while `horizon-resources.json` is unchanged. Sounds
/// prepared after it was written are then never read: the game says "no sounds in the active resources" and is
/// silent. Such a file goes here; the game writes a new one.
pub fn drop_stale_data_file(res: &Path) {
    let bin = res.join("horizon-data.bin");
    if has_sounds(res) && bin.is_file() && data_file_holds(&bin, SOUNDS_JSON).is_ok_and(|held| !held) {
        let _ = fs::remove_file(&bin);
    }
}

/// Whether `horizon-data.bin` (horizon-core prepared_files.rs, layout 2) lists `name`. A file of another layout reads
/// as holding it, so it is left alone.
fn data_file_holds(bin: &Path, name: &str) -> io::Result<bool> {
    use std::io::{Seek, SeekFrom};
    let mut f = fs::File::open(bin)?;
    let mut h = [0u8; 40];
    f.read_exact(&mut h)?;
    let u32_at = |i: usize| u32::from_le_bytes([h[i], h[i + 1], h[i + 2], h[i + 3]]);
    let u64_at = |i: usize| u64::from_le_bytes(h[i..i + 8].try_into().unwrap_or([0; 8]));
    if &h[..8] != b"HZDATA\0\0" || u32_at(8) != 2 {
        return Ok(true);
    }
    let pad = |n: u64| n.div_ceil(8) * 8;
    let (count, root, source, pool) = (u64::from(u32_at(12)), u64_at(16), u64_at(24), u64_at(32));
    if pool > 1 << 28 {
        return Ok(true);
    }
    f.seek(SeekFrom::Start(40 + pad(root) + pad(source) + count * 24))?;
    let mut names = vec![0; pool as usize];
    f.read_exact(&mut names)?;
    Ok(names.windows(name.len()).any(|w| w == name.as_bytes()))
}

/// Unpacks the package (one `<version_dir>/` folder) beside the old copy, then swaps it in.
fn unpack_game(archive: &Path, versions: &Path, vdir: &str, cancel: &AtomicBool) -> Result<()> {
    unpack_tar_zst(archive, versions, vdir, "Sunset package", cancel)?;
    if !versions.join(vdir).join(".installed").exists() {
        return Err("the Sunset package is incomplete (no .installed)".into());
    }
    Ok(())
}

/// What a base set up from the package leaves out: Sunset's own game and libraries, so the base never plays Sunset
/// under Horizon's name, and the record that makes the package a native build.
fn sunset_only(name: &str) -> bool {
    matches!(name, "native-mods.json" | "files.sha256" | "horizon-client" | "horizon-client.exe" | "horizon-native-mods" | ".installed")
        || name.ends_with(".so")
        || name.ends_with(".dll")
}

/// `from`'s tree at `to`, files as hard links where the disk allows, else copies.
fn link_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            link_tree(&entry.path(), &dest)?;
        } else if fs::hard_link(entry.path(), &dest).is_err() {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Sets up the Horizon version `base` (the Nightly the package was built on) from the unpacked package `vdir`, so an
/// install never depends on Horizon's GitHub still having that Nightly. The package carries the same revision's preparer, exporters and
/// server; the client takes only the base's prepared resources from it. Files are hard links where the disk allows.
pub(crate) fn base_from_package(versions: &Path, vdir: &str, base: &str) -> Result<()> {
    let src = versions.join(vdir);
    if !src.join(".installed").exists() {
        return Err("the Sunset package isn't unpacked".into());
    }
    let tmp = versions.join(format!(".{base}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    let made = (|| -> io::Result<()> {
        fs::create_dir_all(&tmp)?;
        for entry in fs::read_dir(&src)? {
            let entry = entry?;
            if sunset_only(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let dest = tmp.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                link_tree(&entry.path(), &dest)?;
            } else if fs::hard_link(entry.path(), &dest).is_err() {
                fs::copy(entry.path(), &dest)?;
            }
        }
        fs::write(
            tmp.join("SUNSET-BASE.txt"),
            format!(
                "Set up by the Sunset launcher from the signed Sunset package {vdir}, so Sunset never needs Horizon's \
                 GitHub release {base} (Horizon replaces a Nightly when it rebuilds it). It holds that Horizon revision's preparer and, once prepared, the Minecraft \
                 resources Sunset plays with. It has no game of its own: play from the sunset instance.\n"
            ),
        )?;
        fs::write(tmp.join(".installed"), "")?;
        let dest = versions.join(base);
        let _ = fs::remove_dir_all(&dest);
        fs::rename(&tmp, &dest)
    })();
    if let Err(e) = made {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("couldn't set up Horizon {base} from the Sunset package: {e}"));
    }
    Ok(())
}

/// Unpacks a `.tar.zst` whose entries all sit under one `top/` folder into `parent/top`, beside the old copy, then
/// swaps it in. Anything outside `top/` (absolute paths, `..`, another folder) refuses the whole archive.
pub(crate) fn unpack_tar_zst(archive: &Path, parent: &Path, top: &str, what: &str, cancel: &AtomicBool) -> Result<()> {
    let tmp = parent.join(format!(".{top}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let f = io::BufReader::new(fs::File::open(archive).map_err(|e| e.to_string())?);
    let dec = ruzstd::decoding::StreamingDecoder::new(f).map_err(|e| format!("{what}: {e}"))?;
    let mut ar = tar::Archive::new(dec);
    ar.set_preserve_permissions(true);
    let fail = |tmp: &Path, e: String| {
        let _ = fs::remove_dir_all(tmp);
        Err(e)
    };
    for entry in ar.entries().map_err(|e| format!("{what}: {e}"))? {
        if cancel.load(Ordering::Relaxed) {
            return fail(&tmp, "cancelled".into());
        }
        let mut entry = match entry {
            Ok(e) => e,
            Err(e) => return fail(&tmp, format!("{what}: {e}")),
        };
        // unpack_in refuses absolute paths and `..`; the top folder must be the one named.
        let first = entry.path().ok().and_then(|p| p.components().next().map(|c| c.as_os_str().to_owned()));
        if first.as_deref() != Some(std::ffi::OsStr::new(top)) {
            return fail(&tmp, format!("the {what} holds files outside {top}/"));
        }
        if let Err(e) = entry.unpack_in(&tmp) {
            return fail(&tmp, format!("{what}: {e}"));
        }
    }
    let dest = parent.join(top);
    let _ = fs::remove_dir_all(&dest);
    let moved = fs::rename(tmp.join(top), &dest).map_err(|e| e.to_string());
    let _ = fs::remove_dir_all(&tmp);
    moved
}

// ---------------------------------------------------------------- play

/// See `Manifest::horizon_build`.
pub fn horizon_build(tag: &str) -> String {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let release = t.split(['.', '-', '+']).next().is_some_and(|major| !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit())) && t.contains('.');
    if release { t.to_string() } else { "Nightly".into() }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// How long ago `then` was, from `now` (both Unix seconds), in plain words.
pub fn ago(then: u64, now: u64) -> String {
    let s = now.saturating_sub(then);
    let plural = |n: u64, unit: &str| if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") };
    match s {
        0..60 => "Just now".into(),
        60..3600 => plural(s / 60, "minute"),
        3600..86400 => plural(s / 3600, "hour"),
        86400..172800 => "Yesterday".into(),
        172800..2592000 => plural(s / 86400, "day"),
        _ => plural(s / 2592000, "month"),
    }
}

/// A play time total: "Under a minute", "45 min", "3 h 20 min".
pub fn play_time(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, 0) => "Under a minute".into(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Starts the game through the CLI (it adds the signed-in account), with the window settings.
pub fn play(p: &Paths, s: &Settings) -> Result<Child> {
    let mut c = cli(p)?;
    if let Some(m) = installed(p) {
        drop_stale_data_file(&resources(p, &m));
    }
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

/// A profile file's first line, the name the game keeps for the settings before the last load, and the largest
/// profile it reads (client/sunset/src/game/profiles.rs has the same three).
const PROFILE_HEADER: &str = "sunset-profile=1";
const PROFILE_BACKUP: &str = "before-load";
const PROFILE_MAX: u64 = 1024 * 1024;

/// What an import does when a profile of that name is saved already.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clash {
    Ask,
    KeepBoth,
    Replace,
}

/// How an import ended: saved under a name, or its name is taken and nobody has said what to do.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileImport {
    Saved(String),
    Taken(String),
}

/// A profile's name from its file's, as the game names profiles (1 to 32 letters, digits, - or _, lower case,
/// starting with a letter or digit): what a download adds to a name ("PvP config (1).profile") becomes dashes.
pub fn profile_name(file: &Path) -> Result<String> {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    let mut n = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            n.push(c);
        } else if !n.ends_with('-') {
            n.push('-');
        }
    }
    let n: String = n.trim_matches(|c| c == '-' || c == '_').chars().take(32).collect();
    let n = n.trim_end_matches(|c| c == '-' || c == '_').to_string();
    if n.is_empty() {
        return Err(format!("{} has no letters or digits in its name: rename it first", file.display()));
    }
    let device = |p: &str| n.len() == 4 && n.starts_with(p) && n.as_bytes()[3].is_ascii_digit();
    if matches!(n.as_str(), "con" | "prn" | "aux" | "nul") || device("com") || device("lpt") {
        return Err(format!("{n} is a name Windows keeps for itself: rename the file first"));
    }
    if n == PROFILE_BACKUP {
        return Err(format!("{PROFILE_BACKUP} is the name of the settings before the last load: rename the file first"));
    }
    Ok(n)
}

/// Takes a `.profile` file into the instance's saved profiles (the game lists and loads them from its Profiles menu).
/// The file has to look like a profile: text of a megabyte at most with `module.setting=value` lines; the profile's
/// first line is added when it is missing (a profile pasted into a file from a chat). Nothing is replaced unless
/// `clash` says so.
pub fn import_profile(p: &Paths, file: &Path, clash: Clash) -> Result<ProfileImport> {
    let name = profile_name(file)?;
    let meta = fs::metadata(file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", file.display()));
    }
    if meta.len() > PROFILE_MAX {
        return Err(format!("{} is over 1 MiB: not a profile", file.display()));
    }
    let text = fs::read_to_string(file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    let body = text.trim();
    let setting = |l: &str| l.split_once('=').is_some_and(|(k, _)| k.trim().contains('.') && !k.trim().contains(' '));
    if !body.lines().any(setting) {
        return Err(format!("{} is not a Sunset profile (it has no module.setting=value lines)", file.display()));
    }
    let text = if body.lines().next().is_some_and(|l| l.trim() == PROFILE_HEADER) { format!("{body}\n") } else { format!("{PROFILE_HEADER}\n{body}\n") };
    let dir = p.sunset_data().join("profiles");
    let saved = |n: &str| dir.join(format!("{n}.profile")).exists();
    let to = if !saved(&name) || clash == Clash::Replace {
        name
    } else if clash == Clash::KeepBoth {
        (2..100).map(|n| format!("{name}-{n}")).find(|n| !saved(n)).ok_or_else(|| format!("too many profiles named {name}"))?
    } else {
        return Ok(ProfileImport::Taken(name));
    };
    fs::create_dir_all(&dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    // Whole or not at all: written beside it, then moved over.
    let (tmp, end) = (dir.join(format!(".{to}.profile.tmp")), dir.join(format!("{to}.profile")));
    fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &end)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("can't save {}: {e}", end.display())
    })?;
    Ok(ProfileImport::Saved(to))
}

/// The native mods built into the version folder `vdir` (`native-mods.json`): Sunset and the plugins compiled into it.
pub fn built_in_mods(p: &Paths, vdir: &str) -> Vec<(String, String)> {
    let Ok(b) = fs::read(p.versions().join(vdir).join("native-mods.json")) else { return vec![] };
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap_or_default();
    v["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["version"].as_str().unwrap_or("").to_string())))
        .collect()
}

// ---------------------------------------------------------------- import from Horizon

/// The stock launcher's instances (its own homes) to import from: (name, game folder). Any of them is worth
/// importing, Sunset data or not: its settings and keybinds alone are.
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
        .filter(|(_, home)| home != &p.instance() && home.is_dir())
        .collect()
}

/// Files at the top of a game folder that an import brings over: Horizon's settings (options.txt holds the
/// keybinds, video and sound settings), the server list, and the rest the pinned game reads back as
/// settings. Never accounts, sessions, logs, caches, assets or the game's binaries.
const IMPORT_FILES: &[&str] = &[
    "options.txt",
    "horizon-graphics.txt",
    "horizon-extras.txt",
    "horizon-pad.txt",
    "horizon-controller.txt",
    "horizon-player.txt",
    "horizon-bluemap.txt",
    "horizon-mod-content-warnings.txt",
    "servers.dat",
    "command_history.txt",
    "cosmetics-owned.txt",
    "cosmetics-visible.txt",
    "realms_persistence.json",
];
/// Folders whose entries (a world, a pack) are each replaced whole: half of one world over another is no world.
const IMPORT_WHOLE: &[&str] = &["saves", "resourcepacks", "shaderpacks"];
/// Folders merged file by file.
const IMPORT_MERGED: &[&str] = &["config"];
/// Left out of Sunset's own data: its debug dumps.
const SUNSET_SKIP: &[&str] = &["debug"];

pub const GAME_RUNNING: &str = "Close Sunset's game first: it writes its settings back when it exits, over the imported ones.";

/// What an import did: files copied, and the folder the files it replaced were moved to (none if it replaced nothing).
#[derive(Debug)]
pub struct Imported {
    pub files: usize,
    pub backup: Option<PathBuf>,
}

impl Imported {
    fn count(&self) -> String {
        if self.files == 1 { "1 file".into() } else { format!("{} files", self.files) }
    }

    pub fn note(&self, from: &str) -> String {
        match &self.backup {
            Some(b) => format!("Imported {} from {from}. The files they replaced were moved to {}", self.count(), b.display()),
            None => format!("Imported {} from {from}", self.count()),
        }
    }
}

/// Copies another game folder's settings, keybinds, servers, worlds, packs and Sunset data (never accounts)
/// into the instance. It overwrites, as the explicit ask it is, but first moves each file or world it would
/// replace into `backup-import-<time>` in the instance; files already the same are left alone. Refused
/// while the game runs.
pub fn import_from(p: &Paths, game_dir: &Path) -> Result<Imported> {
    if game_running(p) {
        return Err(GAME_RUNNING.into());
    }
    let to = p.instance();
    if !game_dir.is_dir() {
        return Err(format!("no game folder at {}", game_dir.display()));
    }
    if fs::canonicalize(game_dir).ok() == fs::canonicalize(&to).ok() {
        return Err("that is Sunset's own instance".into());
    }
    let mut backup = to.join(format!("backup-import-{}", stamp(now_secs())));
    for n in 2.. {
        if !backup.exists() {
            break;
        }
        backup.set_file_name(format!("backup-import-{}-{n}", stamp(now_secs())));
    }
    let mut job = Import { root: to.clone(), backup, files: 0, moved: false };
    let run = |job: &mut Import| -> io::Result<()> {
        for f in IMPORT_FILES {
            job.unit(&game_dir.join(f), &to.join(f))?;
        }
        for d in IMPORT_WHOLE {
            job.entries(&game_dir.join(d), &to.join(d), false, &[])?;
        }
        for d in IMPORT_MERGED {
            job.entries(&game_dir.join(d), &to.join(d), true, &[])?;
        }
        job.entries(&sunset_data_in(game_dir), &p.sunset_data(), true, SUNSET_SKIP)
    };
    let r = run(&mut job);
    let backup = job.moved.then_some(job.backup);
    match r {
        Ok(()) => Ok(Imported { files: job.files, backup }),
        Err(e) => Err(match backup {
            Some(b) => format!("import stopped: {e} (the files it replaced so far are in {})", b.display()),
            None => format!("import stopped: {e}"),
        }),
    }
}

struct Import {
    root: PathBuf,
    backup: PathBuf,
    files: usize,
    moved: bool,
}

impl Import {
    /// Copies `from` (a file, or a folder whole) to `to`, moving what is at `to` into the backup first.
    fn unit(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let Ok(meta) = fs::symlink_metadata(from) else { return Ok(()) };
        if !meta.is_file() && !meta.is_dir() {
            return Ok(()); // symlinks and the like stay behind
        }
        if same(from, to) {
            return Ok(());
        }
        self.back_up(to)?;
        self.files += copy_all(from, to)?;
        Ok(())
    }

    /// Each entry of the folder `from` as a unit; with `merge`, sub-folders are walked down to their files.
    fn entries(&mut self, from: &Path, to: &Path, merge: bool, skip: &[&str]) -> io::Result<()> {
        if !fs::symlink_metadata(from).is_ok_and(|m| m.is_dir()) {
            return Ok(());
        }
        for e in fs::read_dir(from)? {
            let e = e?;
            let name = e.file_name();
            if skip.iter().any(|s| name == *s) {
                continue;
            }
            let (f, t) = (e.path(), to.join(&name));
            if merge && e.file_type()?.is_dir() {
                if fs::symlink_metadata(&t).is_ok_and(|m| !m.is_dir()) {
                    self.back_up(&t)?;
                }
                self.entries(&f, &t, true, &[])?;
            } else {
                self.unit(&f, &t)?;
            }
        }
        Ok(())
    }

    /// Moves `path` (in the instance) to the same place in the backup folder, if there is anything there.
    fn back_up(&mut self, path: &Path) -> io::Result<()> {
        if fs::symlink_metadata(path).is_err() {
            return Ok(());
        }
        let rel = path.strip_prefix(&self.root).map_err(|_| io::Error::other(format!("{} is outside the instance", path.display())))?;
        let dest = self.backup.join(rel);
        fs::create_dir_all(dest.parent().unwrap_or(&self.backup))?;
        fs::rename(path, &dest)?;
        self.moved = true;
        Ok(())
    }
}

/// Whether `b` already holds what `a` does: the same bytes, or a folder of the same (a world's session.lock aside).
fn same(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::symlink_metadata(a), fs::symlink_metadata(b)) else { return false };
    if ma.is_file() {
        return mb.is_file() && ma.len() == mb.len() && fs::read(a).ok() == fs::read(b).ok();
    }
    if !ma.is_dir() || !mb.is_dir() {
        return false;
    }
    let names = |d: &Path| -> Option<Vec<std::ffi::OsString>> {
        let mut v: Vec<_> = fs::read_dir(d).ok()?.map(|e| e.map(|e| e.file_name())).collect::<io::Result<_>>().ok()?;
        v.retain(|n| n != "session.lock" && fs::symlink_metadata(d.join(n)).is_ok_and(|m| m.is_file() || m.is_dir()));
        v.sort();
        Some(v)
    };
    match (names(a), names(b)) {
        (Some(x), Some(y)) => x == y && x.iter().all(|n| same(&a.join(n), &b.join(n))),
        _ => false,
    }
}

/// Copies a file or a folder tree (no symlinks, no worlds' session.lock); returns how many files.
fn copy_all(from: &Path, to: &Path) -> io::Result<usize> {
    let meta = fs::symlink_metadata(from)?;
    if meta.is_file() {
        if from.file_name().is_some_and(|n| n == "session.lock") {
            return Ok(0);
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        return Ok(1);
    }
    if !meta.is_dir() {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    let mut n = 0;
    for e in fs::read_dir(from)? {
        let e = e?;
        n += copy_all(&e.path(), &to.join(e.file_name()))?;
    }
    Ok(n)
}

/// Unix seconds as UTC `YYYYMMDD-HHMMSS`.
fn stamp(secs: u64) -> String {
    let (days, s) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Whether a game from the launcher's version folders runs (it writes options.txt back when it exits).
pub fn game_running(p: &Paths) -> bool {
    let versions = fs::canonicalize(p.versions()).unwrap_or_else(|_| p.versions());
    #[cfg(target_os = "linux")]
    {
        fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| fs::read_link(e.path().join("exe")).is_ok_and(|x| x.starts_with(&versions)))
    }
    #[cfg(windows)]
    {
        // A running .exe can't be opened for writing (a sharing violation).
        fs::read_dir(&versions).into_iter().flatten().flatten().any(|e| {
            let exe = e.path().join(client_name());
            exe.is_file() && fs::OpenOptions::new().write(true).open(&exe).is_err_and(|err| err.raw_os_error() == Some(32))
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Command::new("ps")
            .args(["-axo", "comm="])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| Path::new(l.trim()).starts_with(&versions)))
    }
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

    /// A `horizon-data.bin` of layout 2 listing `names` (prepared_files.rs; file bytes left out, which the check never reads).
    fn data_file(path: &Path, names: &[&str]) {
        let root = b"/some/root";
        let source = b"{}";
        let pool: Vec<u8> = names.concat().into_bytes();
        let pad = |v: &mut Vec<u8>| v.resize(v.len().div_ceil(8) * 8, 0);
        let mut b = b"HZDATA\0\0".to_vec();
        b.extend(2u32.to_le_bytes());
        b.extend((names.len() as u32).to_le_bytes());
        for n in [root.len(), source.len(), pool.len()] {
            b.extend((n as u64).to_le_bytes());
        }
        b.extend(root);
        pad(&mut b);
        b.extend(source);
        pad(&mut b);
        b.extend(vec![0u8; names.len() * 24]);
        b.extend(&pool);
        pad(&mut b);
        fs::write(path, b).unwrap();
    }

    fn prepared(dir: &Path, sounds: bool, sound_hash: &str) {
        fs::create_dir_all(dir.join("assets/minecraft/lang")).unwrap();
        fs::write(dir.join("assets/minecraft/lang/en_us.json"), "{}").unwrap();
        let index = format!(
            r#"{{"objects": {{"icons/a.png": {{"hash": "aa"}}, "minecraft/sounds.json": {{"hash": "{sound_hash}"}}, "minecraft/sounds/x/y.ogg": {{"hash": "bb"}}}}}}"#
        );
        fs::write(dir.join("asset-index.json"), index).unwrap();
        if sounds {
            fs::create_dir_all(dir.join("assets/minecraft/sounds/x")).unwrap();
            fs::write(dir.join("assets/minecraft/sounds/x/y.ogg"), "ogg").unwrap();
            fs::write(dir.join(SOUNDS_JSON), "{\"y\": {}}").unwrap();
        }
    }

    #[test]
    fn a_data_file_from_before_the_sounds_goes() {
        let t = std::env::temp_dir().join(format!("sunset-datafile-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let bin = t.join("horizon-data.bin");
        // No sounds yet: the file stays (a later prepare that adds them is what makes it stale).
        prepared(&t, false, "cc");
        data_file(&bin, &["assets/minecraft/lang/en_us.json"]);
        assert!(!data_file_holds(&bin, SOUNDS_JSON).unwrap());
        drop_stale_data_file(&t);
        assert!(bin.is_file());
        // Sounds prepared after it: it goes.
        prepared(&t, true, "cc");
        drop_stale_data_file(&t);
        assert!(!bin.exists());
        // Written with the sounds there: it stays.
        data_file(&bin, &["assets/minecraft/lang/en_us.json", SOUNDS_JSON]);
        assert!(data_file_holds(&bin, SOUNDS_JSON).unwrap());
        drop_stale_data_file(&t);
        assert!(bin.is_file());
        // Another layout, or a short file: left alone.
        let mut other = fs::read(&bin).unwrap();
        other[8] = 3;
        fs::write(&bin, &other).unwrap();
        data_file(&t.join("unused.bin"), &[]);
        drop_stale_data_file(&t);
        assert!(bin.is_file());
        fs::write(&bin, b"HZDATA").unwrap();
        drop_stale_data_file(&t);
        assert!(bin.is_file());
        // No data file at all: nothing to do.
        fs::remove_file(&bin).unwrap();
        drop_stale_data_file(&t);
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn sounds_come_from_a_stock_version_with_the_same_assets() {
        let t = std::env::temp_dir().join(format!("sunset-sounds-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let res = t.join("sunset/nightly/.local/horizon-client/9.9");
        let stock = t.join("stock");
        prepared(&res, false, "cc");
        // A stock version of another game, one without sounds and one with other sound assets: none of them.
        prepared(&stock.join("a/.local/horizon-client/9.8"), true, "cc");
        prepared(&stock.join("b/.local/horizon-client/9.9"), false, "cc");
        prepared(&stock.join("c/.local/horizon-client/9.9"), true, "dd");
        assert!(!sounds_from(&res, &stock, "9.9"));
        assert!(!has_sounds(&res));
        // Nothing to compare with: no index here.
        assert!(!sounds_from(&t.join("nowhere"), &stock, "9.9"));
        // The same sound assets: copied, sounds.json and the files, and a half-copied folder is replaced.
        prepared(&stock.join("d/.local/horizon-client/9.9"), true, "cc");
        fs::create_dir_all(res.join("assets/minecraft/sounds/partial")).unwrap();
        assert!(sounds_from(&res, &stock, "9.9"));
        assert!(has_sounds(&res));
        assert_eq!(fs::read_to_string(res.join("assets/minecraft/sounds/x/y.ogg")).unwrap(), "ogg");
        assert!(!res.join("assets/minecraft/sounds/partial").exists());
        assert!(!res.join("assets/minecraft/.sounds.tmp").exists());
        // The stock copy is untouched.
        assert!(has_sounds(&stock.join("d/.local/horizon-client/9.9")));
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn offline_names_and_the_owner_gate() {
        for ok in ["Steve", "abc", "Player_1234567890"[..16].as_ref(), "___"] {
            assert!(valid_offline_name(ok), "{ok}");
        }
        for bad in ["", "ab", "Player_12345678901", "two words", "émile", "a-b-c", "../x"] {
            assert!(!valid_offline_name(bad), "{bad}");
        }
        assert!(plain_folder_name("native-0b14f8a-98af49efc335"));
        for bad in ["", ".", "..", "../evil", "a/b", r"a\b", "C:x"] {
            assert!(!plain_folder_name(bad), "{bad}");
        }
        // No owner seen here (no marker, no CLI): offline play is refused before anything starts.
        let t = std::env::temp_dir().join(format!("sunset-offline-test-{}", std::process::id()));
        let p = Paths { root: t.clone() };
        assert_eq!(play_offline(&p, &Settings::default(), "Steve").err().as_deref(), Some(OWNER_NEEDED));
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn horizon_build_is_never_a_hash() {
        assert_eq!(horizon_build("nightly-0b14f8a"), "Nightly");
        assert_eq!(horizon_build("0b14f8a"), "Nightly");
        assert_eq!(horizon_build("1234567"), "Nightly");
        assert_eq!(horizon_build(""), "Nightly");
        assert_eq!(horizon_build("v0.4.1"), "0.4.1");
        assert_eq!(horizon_build("0.5.0-rc1"), "0.5.0-rc1");
        assert_eq!(horizon_build("v.1"), "Nightly");
    }

    #[test]
    fn ago_and_play_time() {
        let now = 10_000_000;
        assert_eq!(ago(now, now), "Just now");
        assert_eq!(ago(now + 50, now), "Just now"); // a clock that went back
        assert_eq!(ago(now - 60, now), "1 minute ago");
        assert_eq!(ago(now - 7200, now), "2 hours ago");
        assert_eq!(ago(now - 90000, now), "Yesterday");
        assert_eq!(ago(now - 3 * 86400, now), "3 days ago");
        assert_eq!(ago(0, now), "3 months ago");
        assert_eq!(play_time(0), "Under a minute");
        assert_eq!(play_time(59), "Under a minute");
        assert_eq!(play_time(45 * 60), "45 min");
        assert_eq!(play_time(3600), "1 h");
        assert_eq!(play_time(3 * 3600 + 20 * 60 + 5), "3 h 20 min");
    }

    #[test]
    fn settings_without_play_records_load() {
        let s: Settings = serde_json::from_str(r#"{"auto_update":true,"window":"1280x720","import_decided":true,"manifest_url":null}"#).unwrap();
        assert_eq!((s.last_played, s.playtime), (None, 0));
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("last_played") && !j.contains("playtime"));
    }

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
        // Sunset's own release mirrors the stock CLI; another repository of the same owner, or a look-alike, does not.
        let mut m = manifest();
        m.cli.url = format!("https://github.com/{REPO}/releases/download/client-v2.1.0/horizon-launcher-linux-x86_64.tar.gz");
        assert!(m.check().is_ok());
        for bad in [
            "https://github.com/PointOfPressure/sunset-client-evil/releases/download/x/h.tar.gz",
            "https://github.com/PointOfPressure/other/releases/download/x/h.tar.gz",
            "https://github.com.evil.example/PointOfPressure/sunset-client/releases/download/x/h.tar.gz",
            "http://github.com/PointOfPressure/sunset-client/releases/download/x/h.tar.gz",
        ] {
            let mut m = manifest();
            m.cli.url = bad.into();
            assert!(m.check().is_err(), "{bad}");
        }
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

    /// A download the server cuts off fails plainly and leaves no .part behind; an unreachable host says so once.
    #[test]
    fn cut_download_and_unreachable_host() {
        use std::io::Write as _;
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut s, _) = l.accept().unwrap();
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\npartial");
            }
        });
        let dir = std::env::temp_dir().join(format!("sunset-cut-{}", std::process::id()));
        let dest = dir.join("pkg.tar.zst");
        let none = AtomicBool::new(false);
        let e = download(&format!("http://127.0.0.1:{port}/pkg"), &dest, "00", None, "Sunset", &none, &|_, _| {}).unwrap_err();
        assert!(e.starts_with("Sunset: the download broke off"), "{e}");
        assert!(!dest.with_extension("part").exists() && !dest.exists());
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(host("https://api.github.com/repos/x"), "api.github.com");
        assert_eq!(host("http://127.0.0.1:9/m.json"), "127.0.0.1:9");
        let e = fetch("http://127.0.0.1:9/m.json").unwrap_err();
        assert_eq!(e, "can't reach 127.0.0.1:9. Check the internet connection");
    }

    /// A stand-in horizon-launcher for run_cli_cancellable: its output, errors, a lot of stderr, and a slow step that
    /// starts a child of its own.
    #[cfg(unix)]
    fn fake_cli(t: &Path) -> Paths {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::remove_dir_all(t);
        let dir = t.join("cli").join("0.0.0");
        fs::create_dir_all(&dir).unwrap();
        fs::write(t.join("cli").join("current"), "0.0.0").unwrap();
        let exe = dir.join(cli_name());
        fs::write(
            &exe,
            "#!/bin/sh\ncase \"$1\" in\n ok) echo hello; echo noise >&2 ;;\n fail) for l in one two three four; do echo $l >&2; done; exit 3 ;;\n \
             chatty) head -c 300000 /dev/zero | tr '\\0' x >&2; echo done ;;\n slow) (sleep 2; touch \"$2\") & sleep 30 ;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        Paths { root: t.to_path_buf() }
    }

    #[cfg(unix)]
    #[test]
    fn cancellable_cli_reads_like_run_cli() {
        let t = std::env::temp_dir().join(format!("sunset-cli-test-{}", std::process::id()));
        let p = fake_cli(&t);
        let no = AtomicBool::new(false);
        assert_eq!(run_cli_cancellable(&p, &["ok"], &no).unwrap(), "hello\n");
        assert_eq!(run_cli(&p, &["ok"]).unwrap(), "hello\n");
        let e = run_cli_cancellable(&p, &["fail"], &no).unwrap_err();
        assert_eq!(e, "horizon-launcher fail failed: two three four");
        assert_eq!(run_cli(&p, &["fail"]).unwrap_err(), e);
        // 300 kB on stderr: more than a pipe holds, so it is read while the CLI runs.
        assert_eq!(run_cli_cancellable(&p, &["chatty"], &no).unwrap(), "done\n");
        // Cancelled before it starts: nothing runs.
        assert_eq!(run_cli_cancellable(&p, &["ok"], &AtomicBool::new(true)).unwrap_err(), "cancelled");
        fs::remove_dir_all(&t).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cancel_ends_the_cli_and_what_it_started() {
        let t = std::env::temp_dir().join(format!("sunset-cli-cancel-test-{}", std::process::id()));
        let p = fake_cli(&t);
        let mark = t.join("child-finished");
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let flip = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            c.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        let e = run_cli_cancellable(&p, &["slow", &mark.to_string_lossy()], &cancel).unwrap_err();
        flip.join().unwrap();
        assert_eq!(e, "cancelled");
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "Cancel took {:?}", started.elapsed());
        // The CLI's own child went with it: it never gets to write its file.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        assert!(!mark.exists(), "the CLI's child outlived Cancel");
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn cli_errors_leave_the_download_counter_out() {
        // KRAMMC's prepare: the counter redrawn in place, then the error.
        let krammc = "67 of 76 files\r67 of 76 files\nerror: lwjgl-shaderc-3.4.3-natives-windows.jar: An established connection was aborted by the software in your host machine. (os error 10053)\n";
        let tail = error_tail(krammc);
        assert_eq!(tail, "error: lwjgl-shaderc-3.4.3-natives-windows.jar: An established connection was aborted by the software in your host machine. (os error 10053)");
        assert!(network_error(&tail));
        assert_eq!(error_tail("67 of 76 files error: boom\n"), "error: boom");
        assert_eq!(error_tail("one\ntwo\ntwo\nthree\nfour\n"), "two three four");
        assert_eq!(error_tail("Error: no such version\n  hint: run versions\n"), "Error: no such version hint: run versions");
        assert!(!network_error("horizon-launcher prepare failed: error: no Minecraft 26.3 found"));
    }

    #[test]
    fn crash_reasons_come_from_the_log() {
        let log = "[01:16:14] [Render thread/INFO]: Horizon 0.1.0\n[01:16:45] [horizon-worker-gpu-watchdog/ERROR]: Horizon could not get a graphics device: the graphics driver stopped answering while the game was picking a GPU (30s)\nthread 'x' panicked at device.rs:293:5\n";
        assert_eq!(log_error(log).unwrap(), "Horizon could not get a graphics device: the graphics driver stopped answering while the game was picking a GPU (30s)");
        assert_eq!(log_error("a\nthread 'main' panicked at src/x.rs:1:1:\n").unwrap(), "thread 'main' panicked at src/x.rs:1:1:");
        assert!(log_error("[t] [main/INFO]: fine\n").is_none());
        let t = std::env::temp_dir().join(format!("sunset-crash-test-{}", std::process::id()));
        let p = Paths { root: t.clone() };
        let _ = fs::remove_dir_all(&t);
        let since = std::time::SystemTime::now();
        // No log of this run: the step Play stopped at.
        let e = game_failure(&p, since, "Horizon: Play: checking the game's assets…");
        assert!(e.starts_with("The game stopped while checking the game's assets. Its log: "), "{e}");
        fs::create_dir_all(p.logs()).unwrap();
        fs::write(p.logs().join("latest.log"), log).unwrap();
        let e = game_failure(&p, since, "Horizon: Play: checking the game's assets…");
        assert!(e.starts_with("The game crashed: Horizon could not get a graphics device"), "{e}");
        assert!(e.contains("graphics driver usually fixes"), "{e}");
        assert_eq!(game_failure(&p, since, "Error: x is not prepared yet. Prepare it first"), NOT_PREPARED);
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn sign_in_refusals_read_plainly() {
        let owner = "error: Sign in with a Microsoft account that owns Minecraft: Java Edition to play. Sign in with `horizon-launcher account sign-in`.";
        let want = "Sign in with a Microsoft account that owns Minecraft: Java Edition to play.";
        assert_eq!(sign_in_refusal(owner).as_deref(), Some(want));
        // The CLI translates the sentence around its command.
        let de = "error: Konto gesperrt. Melde dich mit `horizon-launcher account sign-in` an.";
        assert_eq!(sign_in_refusal(de).as_deref(), Some("Konto gesperrt. Sign in again."));
        assert_eq!(sign_in_refusal("error: `horizon-launcher account sign-in`").as_deref(), Some("Sign in again."));
        let expired = "Your Microsoft sign-in expired. Sign in again.";
        assert_eq!(sign_in_refusal(&format!("error: {expired}")).as_deref(), Some(expired));
        assert!(needs_sign_in(want) && needs_sign_in(expired));
        assert_eq!(sign_in_refusal("error: nightly-x is not prepared yet."), None);
        // A game's own output is not the CLI refusing.
        assert_eq!(sign_in_refusal(expired), None);
        assert_eq!(offline_reason(&format!("Playing offline: {expired}")), Some(expired));
        let p = Paths { root: std::env::temp_dir().join("sunset-sign-in-test") };
        assert_eq!(game_failure(&p, std::time::SystemTime::now(), owner), want);
    }

    #[test]
    fn only_the_games_own_lines_count_as_played() {
        for game in ["Horizon 0.1.0", "Horizon Nightly 2026-11-18 0b14f8a", "Horizon renderer: creating the pipelines"] {
            assert!(game_started(game), "{game}");
        }
        for cli in ["Horizon: Play: reading the sign-in…", "Horizon launcher: could not switch", "error: Sign in again.", "Playing offline: no network"] {
            assert!(!game_started(cli), "{cli}");
        }
    }

    #[test]
    fn prepared_means_complete() {
        let t = std::env::temp_dir().join(format!("sunset-prepared-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(&t).unwrap();
        assert!(!is_prepared(&t), "an empty folder (a prepare that stopped) is not prepared");
        assert!(!is_prepared(&t.join("missing")));
        fs::write(t.join("horizon-resources.json"), "{\"a\":1}").unwrap();
        assert!(is_prepared(&t));
        fs::write(t.join("blocks.json"), "").unwrap();
        assert!(!is_prepared(&t), "an empty table");
        fs::write(t.join("blocks.json"), "[]").unwrap();
        fs::write(t.join("horizon-resources.json"), "{\"a\":").unwrap();
        assert!(!is_prepared(&t), "a cut manifest");
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn leases_are_registered_like_the_cli_does() {
        let t = std::env::temp_dir().join(format!("sunset-lease-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let p = Paths { root: t.clone() };
        let (v, r) = (p.versions().join("native-x"), p.versions().join("nightly-x/.local/horizon-client/9.9"));
        fs::create_dir_all(&v).unwrap();
        fs::create_dir_all(&r).unwrap();
        // No registry: the game holds no leases, and none is made.
        register_leases(&p, &[&v, &r]).unwrap();
        assert!(!p.horizon().join("run-leases").exists());
        fs::create_dir_all(p.horizon().join("run-leases")).unwrap();
        register_leases(&p, &[&v, &r]).unwrap();
        register_leases(&p, &[&v, &r]).unwrap();
        let mut names: Vec<String> = fs::read_dir(p.horizon().join("run-leases")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        // Each folder and every one above it inside the CLI's data, once.
        let vs = p.versions();
        let nx = vs.join("nightly-x");
        let held = [v.clone(), vs.clone(), r.clone(), nx.join(".local/horizon-client"), nx.join(".local"), nx];
        let mut want: Vec<String> = held
            .iter()
            .map(|d| {
                let (dev, ino) = file_id(&fs::canonicalize(d).unwrap()).unwrap();
                hex(&Sha256::digest(format!("{dev}:{ino}").as_bytes()))
            })
            .collect();
        want.sort();
        assert_eq!(names, want);
        want.dedup();
        assert_eq!(want.len(), 6);
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn base_from_package_leaves_sunset_out() {
        let t = std::env::temp_dir().join(format!("sunset-base-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let v = t.join("versions");
        assert!(base_from_package(&v, "native-x", "nightly-x").is_err(), "nothing unpacked");
        let pkg = v.join("native-x");
        fs::create_dir_all(pkg.join("crates/horizon-prepare/src")).unwrap();
        fs::create_dir_all(pkg.join("horizon-native-mods/sunset")).unwrap();
        for f in ["horizon-prepare", "horizon-server", "PLAY.txt", "crates/horizon-prepare/src/lib.rs", "native-mods.json",
            "files.sha256", "horizon-client", "libsunset_core.so", "libhorizon_patch.so", "horizon-native-mods/sunset/Cargo.toml"] {
            fs::write(pkg.join(f), f).unwrap();
        }
        assert!(base_from_package(&v, "native-x", "nightly-x").is_err(), "no .installed: not unpacked");
        fs::write(pkg.join(".installed"), "").unwrap();
        base_from_package(&v, "native-x", "nightly-x").unwrap();
        let b = v.join("nightly-x");
        for kept in [".installed", "SUNSET-BASE.txt", "horizon-prepare", "horizon-server", "PLAY.txt", "crates/horizon-prepare/src/lib.rs"] {
            assert!(b.join(kept).is_file(), "{kept} kept");
        }
        for out in ["native-mods.json", "files.sha256", "horizon-client", "libsunset_core.so", "libhorizon_patch.so", "horizon-native-mods"] {
            assert!(!b.join(out).exists(), "{out} left out");
        }
        assert_eq!(fs::read_to_string(b.join("crates/horizon-prepare/src/lib.rs")).unwrap(), "crates/horizon-prepare/src/lib.rs");
        // Again over a half-made base: replaced whole, no temporary folder left.
        fs::remove_file(b.join(".installed")).unwrap();
        fs::write(b.join("stale"), "").unwrap();
        base_from_package(&v, "native-x", "nightly-x").unwrap();
        assert!(b.join(".installed").is_file() && !b.join("stale").exists());
        assert!(!v.join(".nightly-x.tmp").exists());
        // The package itself is untouched.
        assert!(pkg.join("horizon-client").is_file() && pkg.join("native-mods.json").is_file());
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn profile_files_import_by_name_and_ask_before_replacing() {
        let t = std::env::temp_dir().join(format!("sunset-profile-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(&t).unwrap();
        let p = Paths { root: t.join("home") };
        let saved = |n: &str| fs::read_to_string(p.sunset_data().join(format!("profiles/{n}.profile"))).ok();
        // A download's name, and a profile's text without its first line (pasted into a file from a chat).
        let file = t.join("My PvP (1).profile");
        fs::write(&file, "killaura.range=3\n").unwrap();
        assert_eq!(profile_name(&file).unwrap(), "my-pvp-1");
        assert_eq!(import_profile(&p, &file, Clash::Ask).unwrap(), ProfileImport::Saved("my-pvp-1".into()));
        assert_eq!(saved("my-pvp-1").unwrap(), "sunset-profile=1\nkillaura.range=3\n");
        assert_eq!(profiles(&p), ["my-pvp-1"]);
        // The name is taken now: asked first, nothing written; then both kept, then replaced.
        fs::write(&file, "sunset-profile=1\nkillaura.range=4\n").unwrap();
        assert_eq!(import_profile(&p, &file, Clash::Ask).unwrap(), ProfileImport::Taken("my-pvp-1".into()));
        assert_eq!(saved("my-pvp-1").unwrap(), "sunset-profile=1\nkillaura.range=3\n");
        assert_eq!(import_profile(&p, &file, Clash::KeepBoth).unwrap(), ProfileImport::Saved("my-pvp-1-2".into()));
        assert_eq!(saved("my-pvp-1").unwrap(), "sunset-profile=1\nkillaura.range=3\n");
        assert_eq!(import_profile(&p, &file, Clash::Replace).unwrap(), ProfileImport::Saved("my-pvp-1".into()));
        assert_eq!(saved("my-pvp-1").unwrap(), "sunset-profile=1\nkillaura.range=4\n");
        assert_eq!(profiles(&p), ["my-pvp-1", "my-pvp-1-2"]);
        // No temporary file is left beside them.
        assert_eq!(fs::read_dir(p.sunset_data().join("profiles")).unwrap().count(), 2);
        // Not profiles: other text, a file too big, a folder, a missing file; and names that can't be kept.
        let other = t.join("notes.profile");
        fs::write(&other, "just some text\nwith = signs but no setting\n").unwrap();
        assert!(import_profile(&p, &other, Clash::Ask).unwrap_err().contains("not a Sunset profile"));
        fs::write(&other, format!("a.b=1\n{}", "x".repeat(1024 * 1024))).unwrap();
        assert!(import_profile(&p, &other, Clash::Ask).unwrap_err().contains("1 MiB"));
        fs::create_dir_all(t.join("folder.profile")).unwrap();
        assert!(import_profile(&p, &t.join("folder.profile"), Clash::Ask).is_err());
        assert!(import_profile(&p, &t.join("gone.profile"), Clash::Ask).is_err());
        for bad in ["before-load.profile", "CON.profile", "---.profile", "lpt1.profile"] {
            assert!(profile_name(Path::new(bad)).is_err(), "{bad}");
        }
        assert_eq!(profile_name(Path::new("__A  very long name that goes on and on and on.profile")).unwrap(), "a-very-long-name-that-goes-on-an");
        assert_eq!(profiles(&p), ["my-pvp-1", "my-pvp-1-2"]);
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn import_overwrites_with_backup() {
        let t = std::env::temp_dir().join(format!("sunset-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let w = |path: &Path, s: &str| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, s).unwrap();
        };
        let src = t.join("src");
        let d = sunset_data_in(&src);
        w(&d.join("config"), "a=1");
        w(&d.join("profiles/pvp.profile"), "p");
        w(&d.join("newchunks/x.bin"), "n");
        w(&d.join("debug/dump.txt"), "debug");
        w(&src.join("options.txt"), "key_key.jump:key.keyboard.k");
        w(&src.join("horizon-graphics.txt"), "g");
        w(&src.join("servers.dat"), "s");
        w(&src.join("config/mod.toml"), "c");
        w(&src.join("saves/World/level.dat"), "new world");
        w(&src.join("saves/World/session.lock"), "lock");
        // Never copied: accounts and sessions, logs, caches, assets, the game's binaries and its mod switches.
        for f in ["sessions/s.json", "logs/latest.log", ".horizon-cache/c", "assets/a", "horizon-mods.txt", "horizon-native-mods/sunset.hnmod", "horizon-boot/b", ".last-used", "backup-fps/options.txt", "telemetry/t"] {
            w(&src.join(f), "no");
        }
        let p = Paths { root: t.join("home") };
        let inst = p.instance();
        w(&p.sunset_data().join("config"), "mine");
        w(&p.sunset_data().join("profiles/own.profile"), "o");
        w(&inst.join("options.txt"), "key_key.jump:key.keyboard.space");
        w(&inst.join("saves/World/level.dat"), "old world");
        w(&inst.join("saves/World/region/r.0.0.mca"), "old region");
        w(&inst.join("saves/Mine/level.dat"), "kept");
        w(&inst.join("servers.dat"), "s"); // the same already: left alone
        let i = import_from(&p, &src).unwrap();
        // config, pvp, x.bin, options, graphics, mod.toml, level.dat (no session.lock, servers.dat the same).
        assert_eq!(i.files, 7, "{i:?}");
        let rd = |path: PathBuf| fs::read_to_string(path).unwrap();
        assert_eq!(rd(p.sunset_data().join("config")), "a=1");
        assert_eq!(rd(inst.join("options.txt")), "key_key.jump:key.keyboard.k");
        assert_eq!(rd(inst.join("saves/World/level.dat")), "new world");
        assert!(!inst.join("saves/World/region").exists() && !inst.join("saves/World/session.lock").exists(), "a world is replaced whole");
        assert_eq!(rd(inst.join("saves/Mine/level.dat")), "kept");
        assert_eq!(profiles(&p), ["own", "pvp"]);
        assert!(p.sunset_data().join("newchunks/x.bin").is_file() && !p.sunset_data().join("debug").exists());
        for f in ["sessions", "logs", ".horizon-cache", "assets", "horizon-mods.txt", "horizon-native-mods", "horizon-boot", ".last-used", "backup-fps", "telemetry"] {
            assert!(!inst.join(f).exists(), "{f} copied");
        }
        // What was replaced is in the backup, at the same place.
        let b = i.backup.clone().unwrap();
        assert!(b.starts_with(&inst) && b.file_name().unwrap().to_str().unwrap().starts_with("backup-import-"));
        assert_eq!(rd(b.join("options.txt")), "key_key.jump:key.keyboard.space");
        assert_eq!(rd(b.join("horizon-mods/.horizon-mod-data/sunset/config")), "mine");
        assert_eq!(rd(b.join("saves/World/region/r.0.0.mca")), "old region");
        assert!(!b.join("servers.dat").exists() && !b.join("saves/Mine").exists());
        assert!(i.note("Horizon").contains(&b.display().to_string()));
        // Again: everything is the same now (the world too), so nothing is copied or backed up.
        let again = import_from(&p, &src).unwrap();
        assert!(again.files == 0 && again.backup.is_none(), "{again:?}");
        // One changed file in the world: the world is replaced whole again, the old one backed up.
        w(&src.join("saves/World/level.dat"), "newer world");
        let world = import_from(&p, &src).unwrap();
        assert_eq!(world.note("Horizon").split(" from").next(), Some("Imported 1 file"));
        assert_eq!(rd(world.backup.unwrap().join("saves/World/level.dat")), "new world");
        // A folder with nothing to import is fine; no folder, or Sunset's own instance, is not.
        fs::create_dir_all(t.join("empty")).unwrap();
        let none = import_from(&p, &t.join("empty")).unwrap();
        assert!(none.files == 0 && none.backup.is_none() && none.note("x") == "Imported 0 files from x");
        assert!(import_from(&p, &t.join("nothing")).is_err());
        assert!(import_from(&p, &inst).is_err());
        fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn import_stamp_is_utc() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_795_229_220), "20261121-024700");
        assert_eq!(stamp(951_782_400), "20000229-000000");
    }
}
