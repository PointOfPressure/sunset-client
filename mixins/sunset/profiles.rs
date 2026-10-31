// Profiles as files: `<mod data>/profiles/<name>.profile`, with the checks from Rust's pull request #2 (names that
// are safe file names on every system, size limits, no links or junctions in the way, writes that leave the old file
// whole when they fail), a backup of the settings before every load, and export and import folders beside them. Only
// the mod's own data folder is ever written; no command names a path.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) const HEADER: &str = "sunset-profile=1";
// The settings as they were before the last load.
pub(super) const BACKUP: &str = "before-load";
const MAX_BYTES: usize = 1024 * 1024;
const MAX_PROFILES: usize = 128;

// A profile's name as its file is called: 1 to 32 letters, digits, - or _, starting with a letter or digit, and not
// a name Windows keeps for itself.
pub(super) fn name(input: &str) -> Result<String, String> {
    let n = input.trim().to_ascii_lowercase().replace(' ', "-");
    let ok = !n.is_empty()
        && n.len() <= 32
        && n.as_bytes()[0].is_ascii_alphanumeric()
        && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !ok {
        return Err("a name is 1 to 32 letters, digits, - or _, starting with a letter or digit".into());
    }
    let device = |p: &str| n.len() == 4 && n.starts_with(p) && n.as_bytes()[3].is_ascii_digit();
    if matches!(n.as_str(), "con" | "prn" | "aux" | "nul") || device("com") || device("lpt") {
        return Err(format!("{n} is a name Windows keeps for itself"));
    }
    Ok(n)
}

// A link or a Windows junction (a reparse point).
fn linked(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("can't look at {}: {e}", path.display()))?;
    if linked(&m) || m.is_dir() != directory {
        return Err(format!("{} must be a plain {} (no links)", path.display(), if directory { "folder" } else { "file" }));
    }
    Ok(())
}

fn folder(path: PathBuf) -> Result<PathBuf, String> {
    match fs::create_dir_all(&path) {
        Ok(()) => plain(&path, true).map(|()| path),
        Err(e) => Err(format!("can't make {}: {e}", path.display())),
    }
}

pub(super) struct Store {
    profiles: PathBuf,
    exports: PathBuf,
    imports: PathBuf,
}

impl Store {
    // The mod's data folder, from the game.
    pub(super) fn new(data: PathBuf) -> Result<Self, String> {
        let data = folder(data)?;
        Ok(Self { profiles: folder(data.join("profiles"))?, exports: folder(data.join("exports"))?, imports: folder(data.join("imports"))? })
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.profiles.join(format!("{name}.profile"))
    }

    pub(super) fn exists(&self, name: &str) -> bool {
        self.path(name).is_file()
    }

    pub(super) fn read(&self, name: &str) -> Result<String, String> {
        read(&self.path(name))
    }

    // The saved profiles, less the backup.
    pub(super) fn list(&self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.profiles).map_err(|e| format!("can't list the profiles: {e}"))?.flatten() {
            let file = entry.file_name();
            if let Some(n) = file.to_str().and_then(|f| f.strip_suffix(".profile"))
                && name(n).as_deref() == Ok(n)
                && n != BACKUP
            {
                names.push(n.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    pub(super) fn write(&self, name: &str, text: &str) -> Result<(), String> {
        if name != BACKUP && !self.exists(name) && self.list()?.len() >= MAX_PROFILES {
            return Err(format!("{MAX_PROFILES} profiles are the most; delete one first"));
        }
        write(&self.path(name), text)
    }

    pub(super) fn delete(&self, name: &str) -> Result<(), String> {
        let path = self.path(name);
        plain(&path, false)?;
        fs::remove_file(&path).map_err(|e| format!("can't delete {name}: {e}"))
    }

    // A copy in the exports folder, to share: where it went.
    pub(super) fn export(&self, name: &str) -> Result<PathBuf, String> {
        let to = self.exports.join(format!("{name}.profile"));
        write(&to, &self.read(name)?)?;
        Ok(to)
    }

    // A profile file someone put in the imports folder.
    pub(super) fn import(&self, name: &str) -> Result<String, String> {
        read(&self.imports.join(format!("{name}.profile")))
    }

    pub(super) fn imports(&self) -> &Path {
        &self.imports
    }
}

fn read(path: &Path) -> Result<String, String> {
    plain(path, false)?;
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("can't read {}: {e}", path.display()))?;
    if bytes.len() > MAX_BYTES {
        return Err("a profile is 1 MiB at most".into());
    }
    String::from_utf8(bytes).map_err(|_| "a profile is UTF-8 text".into())
}

// Written beside the old file and renamed over it, so a failed write leaves the old one whole.
fn write(path: &Path, text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err("a profile is 1 MiB at most".into());
    }
    if fs::symlink_metadata(path).is_ok() {
        plain(path, false)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let done = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|()| f.sync_all()))
        .and_then(|()| fs::rename(&tmp, path));
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done.map_err(|e| format!("can't write {}: {e}", path.display()))
}
