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

// Selection belongs to the config, never to a portable profile. Hashing is only
// change detection, not a trust check; the full module snapshot is canonical.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Selection {
    current: Option<(String, u64)>,
    before: Option<(String, u64)>,
    modified: bool,
}

pub(super) fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

impl Selection {
    pub(super) fn refresh(&mut self, snapshot: &str) {
        self.modified = self.current.as_ref().is_some_and(|(_, hash)| *hash != fingerprint(snapshot));
    }
    pub(super) fn selected(&self) -> Option<&str> {
        self.current.as_ref().map(|(name, _)| name.as_str())
    }
    pub(super) fn label(&self) -> String {
        match self.selected() {
            Some(name) => format!("{name}{}", if self.modified { " (modified)" } else { "" }),
            None => "Custom".into(),
        }
    }
    pub(super) fn choose(&mut self, name: &str, snapshot: &str) {
        self.current = Some((name.to_owned(), fingerprint(snapshot)));
        self.modified = false;
    }
    pub(super) fn loaded(&mut self, name: &str, snapshot: &str) {
        let previous = self.current.clone();
        if name == BACKUP {
            self.current = self.before.take();
        } else {
            self.choose(name, snapshot);
        }
        self.before = previous;
        self.refresh(snapshot);
    }
    pub(super) fn deleted(&mut self, name: &str) {
        if self.selected() == Some(name) {
            self.current = None;
            self.modified = false;
        }
        if self.before.as_ref().is_some_and(|(n, _)| n == name) { self.before = None; }
    }
    pub(super) fn stored(&self) -> String {
        let encode = |value: &Option<(String, u64)>| value.as_ref().map_or(String::new(), |(n, h)| format!("{n}|{h:016x}"));
        format!("{};{}", encode(&self.current), encode(&self.before))
    }
    pub(super) fn restore(&mut self, text: &str) {
        let decode = |value: &str| -> Option<(String, u64)> {
            let (n, hash) = value.split_once('|')?;
            (name(n).ok()?.as_str() == n && n != BACKUP && hash.len() == 16)
                .then(|| u64::from_str_radix(hash, 16).ok().map(|h| (n.to_owned(), h))).flatten()
        };
        let (current, before) = text.split_once(';').unwrap_or(("", ""));
        self.current = decode(current);
        self.before = decode(before);
        self.modified = false;
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn edits_and_restores_are_tracked() {
        let mut s = Selection::default();
        s.choose("bedwars", "hud.on=true");
        s.refresh("hud.on=false");
        assert_eq!(s.label(), "bedwars (modified)");
        s.refresh("hud.on=true");
        assert_eq!(s.label(), "bedwars");
    }
    #[test]
    fn saved_selection_survives_restart_with_dirty_state() {
        let mut s = Selection::default();
        s.choose("smp", "baseline");
        let mut restored = Selection::default();
        restored.restore(&s.stored());
        restored.refresh("changed");
        assert_eq!(restored.label(), "smp (modified)");
    }
    #[test]
    fn undo_preserves_previous_name_and_dirty_baseline() {
        let mut s = Selection::default();
        s.choose("smp", "baseline");
        s.loaded("bedwars", "new");
        let mut restored = Selection::default();
        restored.restore(&s.stored());
        restored.loaded(BACKUP, "changed");
        assert_eq!(restored.label(), "smp (modified)");
        restored.loaded(BACKUP, "new");
        assert_eq!(restored.label(), "bedwars");
    }
    #[test]
    fn deleting_selected_profile_and_invalid_metadata_show_custom() {
        let mut s = Selection::default();
        s.choose("bedwars", "baseline");
        s.deleted("bedwars");
        assert_eq!(s.label(), "Custom");
        for text in ["../secret|0000000000000000;", "before-load|0000000000000000;", "bedwars|bad;", "broken"] {
            s.restore(text);
            assert_eq!(s.label(), "Custom");
        }
    }
}

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

    // Another kind of file (a loadout) to share: written to the exports folder as `file`, where it went.
    pub(super) fn share(&self, file: &str, text: &str) -> Result<PathBuf, String> {
        let to = self.exports.join(file);
        write(&to, text)?;
        Ok(to)
    }

    // Such a file someone put in the imports folder.
    pub(super) fn shared(&self, file: &str) -> Result<String, String> {
        read(&self.imports.join(file))
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

// The config: the game's own setting writer truncates the file and then writes it, so a crash or a full disk in
// between left an empty config (every setting gone). Sunset writes it as it writes profiles instead.
pub(super) fn write_config(data: &Path, key: &str, text: &str) -> Result<(), String> {
    let data = folder(data.to_path_buf())?;
    write_limited(&data.join(key), text, 16 * MAX_BYTES)
}

// A config that holds settings: anything else (empty, cut short before its first module) is not one to load.
pub(super) fn whole_config(text: &str) -> bool {
    text.lines().any(|l| l.contains(".on="))
}

// Files a write left behind when the game was closed in the middle of it (`config.tmp1234`, `pvp.tmp1234`).
pub(super) fn sweep(data: &Path) {
    for dir in [data.to_path_buf(), data.join("profiles")] {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let stray = name.rsplit_once(".tmp").is_some_and(|(_, pid)| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
            if stray && entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

// Written beside the old file and renamed over it, so a failed write leaves the old one whole.
fn write(path: &Path, text: &str) -> Result<(), String> {
    write_limited(path, text, MAX_BYTES)
}

fn write_limited(path: &Path, text: &str, limit: usize) -> Result<(), String> {
    if text.len() > limit {
        return Err(format!("{} is {} KiB at most", path.display(), limit / 1024));
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

#[cfg(test)]
mod store_tests {
    use super::*;

    // The config is written whole or not at all, a cut-short one is told apart, and a crashed write's leftovers go.
    #[test]
    fn config_writes_replace_whole_and_leftovers_are_swept() {
        let dir = std::env::temp_dir().join(format!("sunset-store-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_config(&dir, "config", "sprint.on=true\n").unwrap();
        write_config(&dir, "config", "sprint.on=false\nhud.on=true\n").unwrap();
        assert_eq!(fs::read_to_string(dir.join("config")).unwrap(), "sprint.on=false\nhud.on=true\n");
        fs::write(dir.join("config.tmp4242"), "half").unwrap();
        fs::create_dir_all(dir.join("profiles")).unwrap();
        fs::write(dir.join("profiles").join("pvp.tmp99"), "half").unwrap();
        fs::write(dir.join("profiles").join("pvp.profile"), "kept").unwrap();
        sweep(&dir);
        assert!(!dir.join("config.tmp4242").exists() && !dir.join("profiles").join("pvp.tmp99").exists());
        assert!(dir.join("profiles").join("pvp.profile").exists() && dir.join("config").exists());
        assert!(whole_config("sprint.on=true") && !whole_config("") && !whole_config("friends=a"));
        let _ = fs::remove_dir_all(&dir);
    }
}
