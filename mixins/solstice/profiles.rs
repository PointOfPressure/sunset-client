// SPDX-License-Identifier: MIT
// Generated with OpenAI Codex (GPT-6.1 Sol), for PointOfPressure.
// Settings-only profiles. No host dependency: the caller supplies native_data_dir(ID).
use super::solstice::{Module, Value, norm};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const HEADER: &str = "solstice-settings-profile=1";
pub(super) const BACKUP: &str = "before-load";
pub(super) const MAX_BYTES: usize = 1024 * 1024;
const MAX_TEXT: usize = 4096;
const MAX_PROFILES: usize = 128;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn name(input: &str) -> Result<String, String> {
    let n = input.to_ascii_lowercase();
    if n.is_empty() || n.len() > 32 || !n.as_bytes()[0].is_ascii_alphanumeric()
        || !n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Use 1-32 ASCII letters, digits, - or _; start with a letter or digit.".into());
    }
    if matches!(n.as_str(), "con" | "prn" | "aux" | "nul")
        || (n.len() == 4 && (n.starts_with("com") || n.starts_with("lpt"))
            && matches!(n.as_bytes()[3], b'0'..=b'9'))
    {
        return Err("That name is reserved by Windows. Choose another.".into());
    }
    Ok(n)
}

fn text_ok(value: &str) -> bool {
    // The normal config trims values on startup. Refuse unrepresentable padding
    // rather than silently changing a loaded snapshot after a restart.
    value.len() <= MAX_TEXT && value.trim() == value && !value.chars().any(char::is_control)
}

fn fields(modules: &[Module]) -> Result<BTreeMap<String, (usize, usize)>, String> {
    let mut map = BTreeMap::new();
    for (mi, m) in modules.iter().enumerate() {
        let id = norm(m.name);
        for (si, s) in m.settings.iter().enumerate() {
            let field = norm(s.name);
            let key = format!("{id}.{field}");
            if id.is_empty() || field.is_empty() || map.insert(key.clone(), (mi, si)).is_some() {
                return Err(format!("Ambiguous setting name: {key}."));
            }
        }
    }
    Ok(map)
}

fn stored(value: &Value) -> Result<String, String> {
    Ok(match value {
        Value::Bool(b) => format!("bool:{b}"),
        Value::Num { v, min, max, .. } if v.is_finite() && min.is_finite()
            && max.is_finite() && min <= max && v >= min && v <= max => format!("num:{v}"),
        Value::Pick { i, of } if of.get(*i).is_some_and(|s| text_ok(s)) => format!("pick:{}", of[*i]),
        Value::Text(t) if text_ok(t) => format!("text:{t}"),
        _ => return Err("Invalid setting value; text must be at most 4096 bytes, without controls or leading/trailing whitespace.".into()),
    })
}

pub(super) fn encode(modules: &[Module]) -> Result<String, String> {
    let mut out = format!("{HEADER}\n");
    for (key, (mi, si)) in fields(modules)? {
        out.push_str(&format!("{key}={}\n", stored(&modules[mi].settings[si].value)?));
    }
    if out.len() > MAX_BYTES {
        return Err("Profile exceeds the 1 MiB limit.".into());
    }
    Ok(out)
}

// Plugins may register after startup. Carry their unregistered config lines forward,
// and let runtime.raw point to this committed config so registration cannot undo a load.
pub(super) fn preserve_pending(raw: &str, config: &str, modules: &[Module]) -> String {
    let ids: BTreeSet<String> = modules.iter().map(|m| norm(m.name)).collect();
    let mut out = config.to_owned();
    for line in raw.lines() {
        if let Some((key, _)) = line.split_once('=')
            && let Some((id, _)) = key.split_once('.')
            && id != "panel" && !ids.contains(id)
        {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

// Parse and type-check the entire file into a clone before the caller changes anything.
// Missing settings are deliberately preserved; unknown settings are errors, not silently lost.
pub(super) fn stage(modules: &[Module], input: &str) -> Result<Vec<Module>, String> {
    if input.len() > MAX_BYTES {
        return Err("Profile exceeds the 1 MiB limit.".into());
    }
    let mut lines = input.lines();
    if lines.next() != Some(HEADER) {
        return Err("Unsupported or missing settings-profile version.".into());
    }
    let fields = fields(modules)?;
    let mut seen = BTreeSet::new();
    let mut result = modules.to_vec();
    for (line, entry) in lines.enumerate() {
        let bad = || format!("Invalid profile entry on line {}.", line + 2);
        let (key, value) = entry.split_once('=').ok_or_else(bad)?;
        let &(mi, si) = fields.get(key).ok_or_else(|| {
            let shown: String = key.chars().take(80).collect();
            format!("Unknown setting: {shown}{}.", if key.len() > shown.len() { "..." } else { "" })
        })?;
        if !seen.insert(key) {
            return Err(format!("Duplicate setting: {key}."));
        }
        let target = &mut result[mi].settings[si].value;
        match target {
            Value::Bool(b) => *b = match value {
                "bool:true" => true,
                "bool:false" => false,
                _ => return Err(bad()),
            },
            Value::Num { v, min, max, .. } => {
                let number = value.strip_prefix("num:").and_then(|v| v.parse::<f64>().ok()).ok_or_else(bad)?;
                if !number.is_finite() || !min.is_finite() || !max.is_finite()
                    || min > max || number < *min || number > *max
                {
                    return Err(format!("{key} must be a finite number from {min} to {max}."));
                }
                *v = number;
            }
            Value::Pick { i, of } => {
                *i = value.strip_prefix("pick:").filter(|v| text_ok(v))
                    .and_then(|v| of.iter().position(|o| *o == v)).ok_or_else(bad)?;
            }
            Value::Text(t) => {
                let value = value.strip_prefix("text:").filter(|v| text_ok(v)).ok_or_else(bad)?;
                *t = value.to_owned();
            }
        }
    }
    let active: Vec<&str> = modules.iter().zip(&result)
        .filter(|(old, new)| old.on && old.settings != new.settings)
        .map(|(old, _)| old.name).collect();
    if !active.is_empty() {
        return Err(format!("Turn off these modules before changing their settings: {}.", active.join(", ")));
    }
    Ok(result)
}

fn io_error(action: &str, error: std::io::Error) -> String {
    format!("Could not {action}: {error}")
}

// Reparse points include Windows directory junctions, not just symbolic links.
fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    { metadata.file_type().is_symlink() }
}

fn check(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| io_error("inspect profile storage", e))?;
    if linked(&m) || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("Profile storage must use regular files and directories, without links or junctions.".into());
    }
    Ok(())
}

pub(super) struct Store {
    root: PathBuf,
    profiles: PathBuf,
}

impl Store {
    pub(super) fn new(native_data_dir: PathBuf) -> Result<Self, String> {
        // Only the host supplies this root; never accept it from a command or profile.
        fs::create_dir_all(&native_data_dir).map_err(|e| io_error("create native data directory", e))?;
        check(&native_data_dir, true)?;
        let profiles = native_data_dir.join("profiles");
        match fs::create_dir(&profiles) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(io_error("create profiles directory", e)),
        }
        check(&profiles, true)?;
        Ok(Self { root: native_data_dir, profiles })
    }

    fn path(&self, input: &str) -> Result<PathBuf, String> {
        check(&self.root, true)?;
        check(&self.profiles, true)?;
        Ok(self.profiles.join(format!("{}.profile", name(input)?)))
    }

    pub(super) fn read(&self, input: &str) -> Result<String, String> {
        let path = self.path(input)?;
        check(&path, false)?;
        let mut bytes = Vec::new();
        File::open(path).and_then(|f| f.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
            .map_err(|e| io_error("read profile", e))?;
        if bytes.len() > MAX_BYTES {
            return Err("Profile exceeds the 1 MiB limit.".into());
        }
        String::from_utf8(bytes).map_err(|_| "Profile must be UTF-8 text.".into())
    }

    pub(super) fn list(&self) -> Result<Vec<String>, String> {
        self.path(BACKUP)?;
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.profiles).map_err(|e| io_error("list profiles", e))? {
            let entry = entry.map_err(|e| io_error("list profiles", e))?;
            let file = entry.file_name();
            let Some(n) = file.to_str().and_then(|s| s.strip_suffix(".profile")) else { continue };
            if name(n).as_deref() != Ok(n) || n == BACKUP { continue; }
            check(&entry.path(), false)?;
            names.push(n.to_owned());
            if names.len() > MAX_PROFILES { return Err("More than 128 profiles; remove excess files first.".into()); }
        }
        names.sort();
        Ok(names)
    }

    pub(super) fn write(&self, input: &str, text: &str) -> Result<(), String> {
        let path = self.path(input)?;
        if input != BACKUP && !path.exists() && self.list()?.len() >= MAX_PROFILES {
            return Err("At most 128 profiles can be saved. Delete one first.".into());
        }
        atomic_write(&path, text)
    }

    pub(super) fn delete(&self, input: &str) -> Result<(), String> {
        let path = self.path(input)?;
        if name(input)? == BACKUP { return Err("before-load is the automatic backup; it cannot be deleted here.".into()); }
        check(&path, false)?;
        fs::remove_file(path).map_err(|e| io_error("delete profile", e))
    }

    // Checked native-data writes: the pinned host's native_set_setting returns no I/O result.
    // Keep a full config copy as well as a loadable settings snapshot before committing config.
    pub(super) fn commit(&self, before_config: &str, before_profile: &str, config: &str) -> Result<(), String> {
        self.write(BACKUP, before_profile)?;
        atomic_write(&self.profiles.join("before-load.config"), before_config)?;
        atomic_write(&self.root.join("config"), config)
    }
}

// Same-directory create_new + sync + rename: a failed write leaves the old destination intact.
// Rust's rename replaces existing files on Windows too. No remove-then-rename gap.
fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES { return Err("Profile/config exceeds the 1 MiB limit.".into()); }
    match fs::symlink_metadata(path) {
        Ok(_) => check(path, false)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(io_error("inspect destination", e)),
    }
    let parent = path.parent().ok_or("Missing storage directory.")?;
    check(parent, true)?;
    let mut scratch = None;
    for _ in 0..16 {
        let tmp = parent.join(format!(".solstice-profile-{}-{}.tmp", std::process::id(), TEMP_ID.fetch_add(1, Ordering::Relaxed)));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => { scratch = Some((tmp, f)); break; }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_error("create profile scratch file", e)),
        }
    }
    let (tmp, mut file) = scratch.ok_or("Could not reserve profile scratch file.")?;
    let result = file.write_all(text.as_bytes()).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&tmp, path));
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result.map_err(|e| io_error("save profile/config", e))
}
