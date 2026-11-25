//! The launcher's own update. A release's signed manifest names a build of the launcher for its platform
//! (`"launcher": {version, file, size, sha256, signature}`, written by client/scripts/build-prebuilt.sh). When that
//! version is newer than the running one, the launcher downloads it, checks its SHA-256 and minisign signature with
//! the same built-in key as the game packages, runs it once (`--version` must name that version), puts it in place
//! of its own program and starts it.
//!
//! The entry is read on its own ([`offered`]), apart from the rest of the manifest: a manifest this launcher can't
//! install (a newer schema, a CLI address it doesn't accept) still updates it, so a launcher from 2.1.0 on never
//! needs reinstalling by hand.
//!
//! Linux: the release's `sunset-launcher-linux-x86_64.tar.gz` (one `sunset-launcher/` folder: the program, LICENSE,
//! THIRD-PARTY-LICENSES.txt). The new files are written beside the running program and renamed over it; the running
//! process keeps the file it started from. Windows: the release's `.exe`. A running program can't be overwritten
//! there but can be renamed: the old one becomes `<name>.old` (the next start removes it) and the new one takes its
//! name, so shortcuts keep working.

use crate::core::{self, Paths, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

/// The manifest's `launcher` entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LauncherPin {
    pub version: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

/// The two fields the update needs, so nothing else in a newer manifest can stop it.
#[derive(Deserialize)]
struct Offer {
    #[serde(default)]
    platform: String,
    #[serde(default)]
    launcher: Option<LauncherPin>,
}

/// Set (to anything) to keep the launcher as it is; debug builds never replace themselves either.
pub const OFF_ENV: &str = "SUNSET_NO_SELF_UPDATE";

/// Why the launcher doesn't update itself on its own, if it doesn't. `sunset-launcher self-update` still does.
pub fn disabled() -> Option<&'static str> {
    if std::env::var_os(OFF_ENV).is_some_and(|v| !v.is_empty()) {
        return Some("SUNSET_NO_SELF_UPDATE is set");
    }
    cfg!(debug_assertions).then_some("this is a debug build")
}

/// The launcher build a signed manifest (`raw`, already checked against its signature) offers, when it is newer than
/// `running`. Err for an entry that is there but unusable.
pub fn offered(raw: &[u8], running: &str) -> Result<Option<LauncherPin>> {
    let o: Offer = serde_json::from_slice(raw).map_err(|e| format!("unreadable manifest: {e}"))?;
    let Some(pin) = o.launcher else { return Ok(None) };
    if o.platform != core::PLATFORM {
        return Ok(None);
    }
    check(&pin)?;
    Ok(newer(&pin.version, running).then_some(pin))
}

/// The file the release carries for this platform (see the module comment).
pub fn file_name() -> String {
    if cfg!(windows) { format!("sunset-launcher-{}.exe", core::PLATFORM) } else { format!("sunset-launcher-{}.tar.gz", core::PLATFORM) }
}

fn check(pin: &LauncherPin) -> Result<()> {
    let plain = |s: &str| !s.is_empty() && s != "." && s != ".." && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-@+".contains(&b));
    let hex = pin.sha256.len() == 64 && pin.sha256.bytes().all(|b| b.is_ascii_hexdigit());
    if !plain(&pin.file) || !plain(&pin.signature) || !hex || parse(&pin.version).is_none() || pin.file != file_name() {
        return Err(format!("the manifest's launcher entry is unusable ({} {})", pin.version, pin.file));
    }
    Ok(())
}

fn parse(v: &str) -> Option<[u64; 3]> {
    let mut it = v.split('.').map(|n| (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.parse().ok()).flatten());
    let r = [it.next()??, it.next()??, it.next()??];
    it.next().is_none().then_some(r)
}

/// Whether version `a` is newer than `b` (both x.y.z; anything else is never newer).
pub fn newer(a: &str, b: &str) -> bool {
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a > b)
}

/// The running program, as the file it was started from.
fn own_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| format!("can't find the launcher's own program: {e}"))?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
}

/// Downloads `pin` from the release at `base`, checks it and puts it in place of the running program, which it
/// returns (to start). The running launcher goes on as it was until it exits.
pub fn apply(p: &Paths, base: &str, pin: &LauncherPin, cancel: &AtomicBool, progress: core::Progress) -> Result<PathBuf> {
    apply_to(p, base, pin, &own_exe()?, cancel, progress)
}

pub(crate) fn apply_to(p: &Paths, base: &str, pin: &LauncherPin, exe: &Path, cancel: &AtomicBool, progress: core::Progress) -> Result<PathBuf> {
    check(pin)?;
    let a = core::Artifact { file: pin.file.clone(), kind: "launcher".into(), size: pin.size, sha256: pin.sha256.clone(), signature: pin.signature.clone() };
    let got = core::fetch_artifact(p, base, &a, &format!("Sunset launcher {}", pin.version), cancel, progress)?;
    let dir = exe.parent().ok_or("the launcher's program has no folder")?;
    let fail = |e: String| format!("{e}. Download the new launcher from github.com/{}/releases instead", core::REPO);
    replace(&got, exe, dir, &pin.version).map_err(fail)?;
    Ok(exe.to_path_buf())
}

/// Runs the new program once: it has to start on this computer and be the version the manifest says.
fn try_run(new: &Path, version: &str) -> Result<()> {
    let out = Command::new(new)
        .arg("--version")
        .env(OFF_ENV, "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("the new launcher doesn't start: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || said.trim() != format!("sunset-launcher {version}") {
        return Err(format!("the new launcher doesn't start as {version} (it said {:?})", said.trim()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(archive: &Path, exe: &Path, dir: &Path, version: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let io = |e: std::io::Error| format!("{}: {e}", dir.display());
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let unpacked = (|| -> Result<()> {
        let f = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(f));
        for entry in tar.entries().map_err(|e| format!("the launcher archive: {e}"))? {
            let mut e = entry.map_err(|e| format!("the launcher archive: {e}"))?;
            if !e.header().entry_type().is_file() {
                continue;
            }
            let path = e.path().map_err(|e| format!("the launcher archive: {e}"))?.into_owned();
            let mut parts = path.iter().map(|c| c.to_string_lossy().into_owned());
            let (Some(top), Some(name), None) = (parts.next(), parts.next(), parts.next()) else { continue };
            let dest = match (top.as_str(), name.as_str()) {
                ("sunset-launcher", "sunset-launcher") => exe.to_path_buf(),
                // The licence texts beside the program, where it was unpacked with them.
                ("sunset-launcher", "LICENSE" | "THIRD-PARTY-LICENSES.txt") if dir.join(&name).is_file() => dir.join(&name),
                _ => continue,
            };
            let tmp = dir.join(format!(".{}.update", dest.file_name().unwrap().to_string_lossy()));
            let mut out = fs::File::create(&tmp).map_err(io)?;
            staged.push((tmp, dest));
            std::io::copy(&mut e, &mut out).map_err(io)?;
            out.sync_all().map_err(io)?;
        }
        let (new, _) = staged.iter().find(|(_, d)| d == exe).ok_or("the launcher archive has no sunset-launcher program")?;
        fs::set_permissions(new, fs::Permissions::from_mode(0o755)).map_err(io)?;
        try_run(new, version)
    })();
    // The program goes last, so a failure part way leaves the old one, which works.
    staged.sort_by_key(|(_, d)| d == exe);
    let swapped = unpacked.and_then(|_| staged.iter().try_for_each(|(tmp, dest)| fs::rename(tmp, dest).map_err(io)));
    for (tmp, _) in &staged {
        let _ = fs::remove_file(tmp);
    }
    swapped
}

#[cfg(windows)]
fn replace(new_exe: &Path, exe: &Path, dir: &Path, version: &str) -> Result<()> {
    let io = |e: std::io::Error| format!("{}: {e}", dir.display());
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "sunset-launcher".into());
    let tmp = dir.join(format!("{stem}.update.exe"));
    let result = (|| {
        fs::copy(new_exe, &tmp).map_err(io)?;
        try_run(&tmp, version)?;
        let old = old_path(exe);
        let _ = fs::remove_file(&old);
        fs::rename(exe, &old).map_err(io)?;
        if let Err(e) = fs::rename(&tmp, exe) {
            let _ = fs::rename(&old, exe);
            return Err(io(e));
        }
        Ok(())
    })();
    let _ = fs::remove_file(&tmp);
    result
}

/// Where Windows' replaced program waits until it can be removed.
#[cfg_attr(not(windows), allow(dead_code))]
fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Removes the program an update replaced (Windows), once the launcher that ran it has exited: a few tries for a
/// minute, in the background.
pub fn remove_replaced() {
    if !cfg!(windows) {
        return;
    }
    let Ok(old) = own_exe().map(|e| old_path(&e)) else { return };
    if !old.exists() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..30 {
            if fs::remove_file(&old).is_ok() || !old.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

/// Starts `exe` with this launcher's arguments (the caller then exits).
pub fn restart(exe: &Path) -> Result<()> {
    Command::new(exe).args(std::env::args_os().skip(1)).spawn().map(|_| ()).map_err(|e| format!("couldn't start the new launcher: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(launcher: &str) -> Vec<u8> {
        format!(r#"{{"schema":9,"platform":"{}","something_new":[1,2],{launcher}"artifacts":"changed"}}"#, core::PLATFORM).into_bytes()
    }
    fn pin(version: &str) -> String {
        format!(
            r#""launcher":{{"version":"{version}","file":"{}","size":5,"sha256":"{}","signature":"{}.minisig"}},"#,
            file_name(),
            "ab".repeat(32),
            file_name()
        )
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("2.1.0", "2.0.9"));
        assert!(newer("2.10.0", "2.9.0"));
        assert!(newer("3.0.0", "2.99.99"));
        assert!(!newer("2.1.0", "2.1.0"));
        assert!(!newer("2.0.0", "2.1.0"));
        for bad in ["2.1", "2.1.0.1", "2.1.0-rc1", "v2.1.0", "", "2..0", "2.1.x", " 2.1.0", "+2.1.0"] {
            assert!(!newer(bad, "1.0.0"), "{bad}");
            assert!(!newer("9.0.0", bad), "{bad}");
        }
    }

    #[test]
    fn offer_is_read_from_any_manifest_shape() {
        // A manifest this launcher couldn't install (schema 9, other fields) still offers the update.
        assert_eq!(offered(&manifest(&pin("9.0.0")), "2.1.0").unwrap().unwrap().version, "9.0.0");
        assert_eq!(offered(&manifest(&pin("2.1.0")), "2.1.0").unwrap(), None, "same version");
        assert_eq!(offered(&manifest(&pin("2.0.0")), "2.1.0").unwrap(), None, "older");
        assert_eq!(offered(&manifest(""), "2.1.0").unwrap(), None, "no entry");
        let other = manifest(&pin("9.0.0")).iter().map(|&b| b as char).collect::<String>().replace(core::PLATFORM, "macos-arm64");
        assert_eq!(offered(other.as_bytes(), "2.1.0").unwrap(), None, "another platform's manifest");
        assert!(offered(b"not json", "2.1.0").is_err());
    }

    #[test]
    fn unusable_entries_are_refused() {
        let good = pin("9.0.0");
        for (from, to) in [
            (file_name(), "../sunset-launcher".to_string()),
            (file_name(), "sunset-launcher-other.tar.gz".to_string()),
            ("ab".repeat(32), "zz".repeat(32)),
            ("ab".repeat(32), "ab".repeat(31)),
            ("\"9.0.0\"".to_string(), "\"9.0\"".to_string()),
        ] {
            let bad = good.replacen(&from, &to, 1);
            assert!(offered(&manifest(&bad), "2.1.0").is_err(), "{bad}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn replace_swaps_program_and_licences_only() {
        use std::io::Write;
        let t = std::env::temp_dir().join(format!("sunset-selfupdate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let dir = t.join("app");
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("sunset-launcher");
        fs::write(&exe, "old").unwrap();
        fs::write(dir.join("LICENSE"), "old licence").unwrap();
        // The new program: a script that answers --version as the launcher does.
        let tarball = |version_said: &str, with_program: bool| {
            let path = t.join("new.tar.gz");
            let gz = flate2::write::GzEncoder::new(fs::File::create(&path).unwrap(), flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            let mut add = |name: &str, body: &[u8], mode: u32| {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(mode);
                h.set_cksum();
                b.append_data(&mut h, name, body).unwrap();
            };
            if with_program {
                add("sunset-launcher/sunset-launcher", format!("#!/bin/sh\necho sunset-launcher {version_said}\n").as_bytes(), 0o644);
            }
            add("sunset-launcher/LICENSE", b"new licence", 0o644);
            add("sunset-launcher/THIRD-PARTY-LICENSES.txt", b"not beside the old one", 0o644);
            add("sunset-launcher/evil", b"x", 0o644);
            b.into_inner().unwrap().finish().unwrap().flush().unwrap();
            path
        };
        let leftovers = || fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".update")).count();
        // No program, or a program that says another version: nothing changes.
        assert!(replace(&tarball("9.0.0", false), &exe, &dir, "9.0.0").is_err());
        assert!(replace(&tarball("8.0.0", true), &exe, &dir, "9.0.0").is_err());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(fs::read_to_string(dir.join("LICENSE")).unwrap(), "old licence");
        assert_eq!(leftovers(), 0);
        replace(&tarball("9.0.0", true), &exe, &dir, "9.0.0").unwrap();
        assert!(fs::read_to_string(&exe).unwrap().contains("9.0.0"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read_to_string(dir.join("LICENSE")).unwrap(), "new licence");
        assert!(!dir.join("THIRD-PARTY-LICENSES.txt").exists(), "only files that were there are replaced");
        assert!(!dir.join("evil").exists());
        assert_eq!(leftovers(), 0);
        let _ = fs::remove_dir_all(&t);
    }
}
