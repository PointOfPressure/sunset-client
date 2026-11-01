// Real filesystem regression tests; the production module is included unchanged.
mod store {
    include!("../mixins/sunset/rival_store.rs");
    pub fn next_temp() -> u64 {
        NEXT_TEMP.load(Ordering::Relaxed)
    }
    #[cfg(windows)]
    pub fn while_locked(data: &Path, f: impl FnOnce()) {
        let _guard = directory(data, true).unwrap();
        f();
    }
}
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "sunset-rival-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn data(&self) -> PathBuf {
        self.0.join("data")
    }
    fn record(&self, id: u128) -> PathBuf {
        self.data().join("rivals").join(format!("{id:032x}.json"))
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sunset-rival-test-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn save(s: &Sandbox, id: u128, text: &str) -> std::io::Result<()> {
    store::save(&s.data(), &[(id, text.to_owned())])
}
fn names(s: &Sandbox) -> Vec<String> {
    fs::read_dir(s.data().join("rivals"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}
#[test]
fn round_trip_and_atomic_replacement() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    save(&s, 7, "new").unwrap();
    assert_eq!(store::read(&s.data(), 7).unwrap(), "new");
    assert_eq!(names(&s), [format!("{:032x}.json", 7)]);
}
#[test]
fn empty_snapshot_does_not_create_folders() {
    let s = Sandbox::new();
    store::save(&s.data(), &[]).unwrap();
    assert!(!s.data().exists());
}
#[test]
fn missing_read_does_not_create_folders() {
    let s = Sandbox::new();
    assert!(store::read(&s.data(), 7).is_err());
    assert!(!s.data().exists());
}
#[test]
fn batch_and_payload_limits_apply_before_disk_changes() {
    let s = Sandbox::new();
    assert!(
        store::save(
            &s.data(),
            &(0..65).map(|i| (i, "x".into())).collect::<Vec<_>>()
        )
        .is_err()
    );
    assert!(save(&s, 7, &"x".repeat(16_385)).is_err());
    assert!(!s.data().exists());
    save(&s, 7, &"x".repeat(16_384)).unwrap();
    assert_eq!(store::read(&s.data(), 7).unwrap().len(), 16_384);
}
#[test]
fn oversized_or_invalid_utf8_read_is_rejected() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    fs::write(s.record(7), vec![b'x'; 16_385]).unwrap();
    assert!(store::read(&s.data(), 7).is_err());
    fs::write(s.record(7), [0xff]).unwrap();
    assert!(store::read(&s.data(), 7).is_err());
}
#[test]
fn file_in_directory_position_is_rejected() {
    let s = Sandbox::new();
    fs::write(s.data(), "keep").unwrap();
    assert!(save(&s, 7, "new").is_err());
    assert_eq!(fs::read(s.data()).unwrap(), b"keep");
}
#[test]
fn directory_in_file_position_is_rejected() {
    let s = Sandbox::new();
    fs::create_dir_all(s.record(7)).unwrap();
    assert!(save(&s, 7, "new").is_err());
    assert!(store::read(&s.data(), 7).is_err());
    assert!(s.record(7).is_dir());
    assert_eq!(names(&s).len(), 1);
}
#[test]
fn parent_traversal_is_rejected() {
    let s = Sandbox::new();
    fs::create_dir(s.data()).unwrap();
    assert!(store::save(&s.data().join("..").join("outside"), &[(7, "new".into())]).is_err());
    assert!(!s.0.join("outside").exists());
}
#[test]
fn hard_link_target_is_replaced_without_modifying_external_file() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    let outside = s.0.join("outside");
    fs::write(&outside, "keep").unwrap();
    fs::remove_file(s.record(7)).unwrap();
    fs::hard_link(&outside, s.record(7)).unwrap();
    save(&s, 7, "new").unwrap();
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    assert_eq!(store::read(&s.data(), 7).unwrap(), "new");
}
#[test]
fn occupied_temporary_names_are_not_truncated_or_removed() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    let outside = s.0.join("outside");
    fs::write(&outside, "keep").unwrap();
    let first = store::next_temp();
    let collisions: Vec<_> = (first..first + 16)
        .map(|i| {
            s.data()
                .join("rivals")
                .join(format!(".{:032x}.{}.{i}.tmp", 7, std::process::id()))
        })
        .collect();
    for p in &collisions {
        fs::hard_link(&outside, p).unwrap();
    }
    assert!(save(&s, 7, "new").is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    assert_eq!(store::read(&s.data(), 7).unwrap(), "old");
    for p in &collisions {
        assert_eq!(fs::read(p).unwrap(), b"keep");
    }
}
#[test]
fn retention_only_removes_plain_uuid_json_files() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    for i in 10..80 {
        fs::write(s.record(i), "old").unwrap();
    }
    let notes = s.data().join("rivals/notes.json");
    fs::write(&notes, "keep").unwrap();
    let directory = s.record(99);
    fs::create_dir(&directory).unwrap();
    save(&s, 7, "new").unwrap();
    assert_eq!(fs::read(notes).unwrap(), b"keep");
    assert!(directory.is_dir());
    assert_eq!(
        fs::read_dir(s.data().join("rivals"))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
            .count(),
        65
    );
}
#[test]
fn worker_checks_paths_when_it_runs_not_when_snapshot_is_taken() {
    let s = Sandbox::new();
    let data = s.data();
    let (go, wait) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        wait.recv().unwrap();
        store::save(&data, &[(7, "new".into())])
    });
    fs::write(s.data(), "keep").unwrap();
    go.send(()).unwrap();
    assert!(thread.join().unwrap().is_err());
    assert_eq!(fs::read(s.data()).unwrap(), b"keep");
}

// Junctions work on Windows without granting symbolic-link creation privileges.
// On Unix these same regression fixtures use directory symlinks.
fn link_dir(target: &Path, link: &Path) {
    #[cfg(windows)]
    {
        let r = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
}
#[test]
fn linked_data_directory_does_not_create_outside_rivals() {
    let s = Sandbox::new();
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    link_dir(&outside, &s.data());
    assert!(save(&s, 7, "new").is_err());
    assert!(store::read(&s.data(), 7).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
#[test]
fn linked_rivals_directory_cannot_be_written_read_or_pruned() {
    let s = Sandbox::new();
    fs::create_dir(s.data()).unwrap();
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    for i in 0..70 {
        fs::write(outside.join(format!("{i:032x}.json")), "keep").unwrap();
    }
    link_dir(&outside, &s.data().join("rivals"));
    assert!(save(&s, 7, "new").is_err());
    assert!(store::read(&s.data(), 7).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 70);
    for i in 0..70 {
        assert_eq!(
            fs::read(outside.join(format!("{i:032x}.json"))).unwrap(),
            b"keep"
        );
    }
}
#[test]
fn linked_ancestor_is_rejected_before_creating_descendants() {
    let s = Sandbox::new();
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    let parent = s.0.join("parent");
    link_dir(&outside, &parent);
    assert!(store::save(&parent.join("deeper/sunset"), &[(7, "new".into())]).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
#[test]
fn reparse_or_symlink_in_json_position_is_rejected() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    fs::remove_file(s.record(7)).unwrap();
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    link_dir(&outside, &s.record(7));
    assert!(save(&s, 7, "new").is_err());
    assert!(store::read(&s.data(), 7).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
#[test]
fn pruning_skips_link_entries_instead_of_removing_them() {
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    for i in 10..80 {
        fs::write(s.record(i), "old").unwrap();
    }
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "keep").unwrap();
    link_dir(&outside, &s.record(99));
    save(&s, 7, "new").unwrap();
    assert!(fs::symlink_metadata(s.record(99)).is_ok());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
}
#[test]
fn delayed_worker_rejects_directory_link_inserted_after_snapshot() {
    let s = Sandbox::new();
    let outside = s.0.join("outside");
    fs::create_dir(&outside).unwrap();
    let data = s.data();
    let (go, wait) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        wait.recv().unwrap();
        store::save(&data, &[(7, "new".into())])
    });
    link_dir(&outside, &s.data());
    go.send(()).unwrap();
    assert!(thread.join().unwrap().is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
#[cfg(windows)]
#[test]
fn checked_windows_directories_cannot_be_swapped_while_in_use() {
    let s = Sandbox::new();
    store::while_locked(&s.data(), || {
        assert!(fs::rename(s.data(), s.0.join("moved")).is_err());
        assert!(fs::rename(s.data().join("rivals"), s.0.join("moved-rivals")).is_err());
    });
    fs::rename(s.data(), s.0.join("moved")).unwrap();
}

#[cfg(windows)]
#[test]
fn failed_replace_preserves_record_and_removes_only_its_own_temporary() {
    use std::os::windows::fs::OpenOptionsExt;
    let s = Sandbox::new();
    save(&s, 7, "old").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(s.record(7))
        .unwrap();
    assert!(save(&s, 7, "new").is_err());
    assert_eq!(fs::read(s.record(7)).unwrap(), b"old");
    assert_eq!(names(&s), [format!("{:032x}.json", 7)]);
    drop(lock);
    save(&s, 7, "new").unwrap();
    assert_eq!(store::read(&s.data(), 7).unwrap(), "new");
}
