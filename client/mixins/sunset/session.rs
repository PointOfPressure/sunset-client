// Tells the launcher that a native mod is hidden for as long as this game process runs, without
// touching the installation: `<game folder>/horizon-session/hidden/<mod id>.<pid>.lock`, a file
// this process keeps open under an exclusive lock until it exits. The operating system drops the
// lock when the process ends, crash or kill included, so a file whose lock can be taken is stale
// whoever finds it, and is removed. The launcher's side is docs/hard-panic.md's patch.
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(super) const DIR: &str = "horizon-session/hidden";

// The open, locked markers: kept until the process exits.
static HELD: Mutex<Vec<File>> = Mutex::new(Vec::new());

fn root() -> PathBuf {
    // The game folder (`Game::game_dir`): the launcher starts the game in the instance's folder.
    std::env::current_dir().unwrap_or_default()
}

// Hides `id` until this process exits. A marker that can't be made leaves the launcher's list as
// it is; the game's own Options > Mods doesn't depend on it.
pub(super) fn hide(id: &str) {
    if let Ok(file) = hold(&root(), id, std::process::id()) {
        HELD.lock().unwrap_or_else(|e| e.into_inner()).push(file);
    }
}

fn hold(root: &Path, id: &str, pid: u32) -> std::io::Result<File> {
    let dir = root.join(DIR);
    fs::create_dir_all(&dir)?;
    // Locked under another name first: a sweep never finds the marker unlocked.
    let new = dir.join(format!("{id}.{pid}.tmp"));
    let file = File::create(&new)?;
    file.try_lock().map_err(std::io::Error::other)?;
    fs::rename(&new, dir.join(format!("{id}.{pid}.lock")))?;
    Ok(file)
}

// Removes the markers of game processes that have ended (their lock is free), so a crashed
// session leaves nothing behind once the next game starts. Markers still held are left alone.
// Once a process, as Sunset starts.
pub(super) fn sweep() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| sweep_in(&root()));
}

fn sweep_in(root: &Path) {
    let dir = root.join(DIR);
    let Ok(entries) = fs::read_dir(&dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "lock" && e != "tmp") {
            continue;
        }
        let stale = File::open(&path).is_ok_and(|f| f.try_lock_shared().is_ok());
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
    // Only when empty: `hidden`, then `horizon-session`.
    if fs::remove_dir(&dir).is_ok() {
        let _ = fs::remove_dir(root.join(DIR).parent().unwrap_or(root));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_marker_survives_a_sweep_and_a_released_one_does_not() {
        let root = std::env::temp_dir().join(format!("sunset-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join(DIR);
        let held = hold(&root, "sunset", 41).unwrap();
        let ended = hold(&root, "sunset", 42).unwrap();
        drop(ended);
        sweep_in(&root);
        assert!(dir.join("sunset.41.lock").exists());
        assert!(!dir.join("sunset.42.lock").exists());
        // Another handle (another process's view) can't share the lock while it is held.
        let other = File::open(dir.join("sunset.41.lock")).unwrap();
        assert!(matches!(other.try_lock_shared(), Err(fs::TryLockError::WouldBlock)));
        drop((other, held));
        sweep_in(&root);
        assert!(!root.join("horizon-session").exists());
        let _ = fs::remove_dir(&root);
    }
}
