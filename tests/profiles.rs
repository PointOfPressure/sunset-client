// SPDX-License-Identifier: MIT
// Generated with OpenAI Codex (GPT-6.1 Sol), for PointOfPressure.
use super::*;
use crate::solstice_profiles::{self as profiles, Store};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("solstice-profiles-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> Store { Store::new(self.0.join("native-solstice")).unwrap() }
    fn native(&self) -> PathBuf { self.0.join("native-solstice") }
    fn file(&self, name: &str) -> PathBuf { self.native().join("profiles").join(name) }
}
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

fn modules() -> Vec<Module> {
    let mut m = Module::new("Flight", Category::Movement, "test")
        .num("Speed", 1.123456789123, 0.1, 50.0, 0.1)
        .bool("Anti kick", true).pick("Mode", &["Velocity", "Abilities"])
        .text("List", "stone,ore=value: spaced");
    m.key = Some("key.keyboard.f".into());
    vec![m, Module::new("HUD", Category::Render, "test").on().bool("FPS", true)]
}

#[test]
fn bounded_portable_names() {
    assert_eq!(profiles::name("MiNe_1-a").unwrap(), "mine_1-a");
    assert!(profiles::name(&"a".repeat(32)).is_ok());
    for name in ["", ".", "..", "../escape", "a/b", "a\\b", "a:b", "a b", "a\n", "é", "_a", "-a", "CON", "aux", "COM1", "lpt9", "nul"] {
        assert!(profiles::name(name).is_err(), "accepted {name:?}");
    }
    assert!(profiles::name(&"a".repeat(33)).is_err());
}

#[test]
fn deterministic_lossless_typed_roundtrip() {
    let original = modules();
    let encoded = profiles::encode(&original).unwrap();
    assert!(encoded.contains("speed=num:1.123456789123"));
    assert!(!encoded.contains(".on="));
    assert!(!encoded.contains(".key="));
    let mut reordered = original.clone();
    reordered.reverse();
    for m in &mut reordered { m.settings.reverse(); }
    assert_eq!(encoded, profiles::encode(&reordered).unwrap());
    let mut changed = original.clone();
    changed[0].settings[0].value = Value::Num { v: 50.0, min: 0.1, max: 50.0, step: 0.1 };
    assert_eq!(profiles::stage(&changed, &encoded).unwrap(), original);
}

#[test]
fn malformed_input_is_all_or_nothing() {
    let original = modules();
    for bad in [
        "", "solstice-settings-profile=2", "solstice-settings-profile=1\ninvalid",
        "solstice-settings-profile=1\nflight.antikick=bool:false\nflight.speed=num:NaN",
        "solstice-settings-profile=1\nflight.speed=num:inf", "solstice-settings-profile=1\nflight.speed=num:-inf",
        "solstice-settings-profile=1\nflight.speed=num:51", "solstice-settings-profile=1\nflight.speed=num:0",
        "solstice-settings-profile=1\nflight.speed=bool:true", "solstice-settings-profile=1\nflight.antikick=bool:toggle",
        "solstice-settings-profile=1\nflight.antikick=true", "solstice-settings-profile=1\nflight.mode=pick:unknown",
        "solstice-settings-profile=1\nflight.list=text:tab\tbad", "solstice-settings-profile=1\nflight.list=text:\0",
        "solstice-settings-profile=1\nflight.list=text: leading", "solstice-settings-profile=1\nflight.list=text:trailing ",
        "solstice-settings-profile=1\nflight.on=bool:true", "solstice-settings-profile=1\nflight.key=text:none",
        "solstice-settings-profile=1\nplugin.missing=bool:true",
        "solstice-settings-profile=1\nflight.speed=num:2\nflight.speed=num:3",
    ] {
        assert!(profiles::stage(&original, bad).is_err(), "accepted {bad:?}");
        assert_eq!(original, modules());
    }
    assert!(profiles::stage(&original, &format!("solstice-settings-profile=1\nflight.list=text:{}", "x".repeat(4097))).is_err());
    assert!(profiles::stage(&original, &"x".repeat(profiles::MAX_BYTES + 1)).is_err());
    let huge_key = format!("solstice-settings-profile=1\n{}=bool:true", "x".repeat(100_000));
    assert!(profiles::stage(&original, &huge_key).unwrap_err().len() < 128);
}

#[test]
fn sparse_profile_preserves_other_settings_keys_and_states() {
    let original = modules();
    let loaded = profiles::stage(&original, "solstice-settings-profile=1\nflight.antikick=bool:false\n").unwrap();
    assert!(!loaded[0].b("Anti kick"));
    assert_eq!(loaded[0].key, original[0].key);
    assert_eq!(loaded[0].on, original[0].on);
    assert_eq!(loaded[0].settings[0], original[0].settings[0]);
    assert_eq!(loaded[1], original[1]);
}

#[test]
fn changed_active_modules_are_rejected_but_noop_is_allowed() {
    let original = modules();
    let error = profiles::stage(&original, "solstice-settings-profile=1\nhud.fps=bool:false").unwrap_err();
    assert!(error.contains("HUD"));
    assert!(profiles::stage(&original, &profiles::encode(&original).unwrap()).is_ok());
}

#[test]
fn invalid_current_values_cannot_be_saved() {
    for value in [
        Value::Num { v: f64::NAN, min: 0.0, max: 1.0, step: 0.1 },
        Value::Num { v: 2.0, min: 0.0, max: 1.0, step: 0.1 },
        Value::Pick { i: 1, of: &["one"] }, Value::Text("a\nb".into()),
    ] {
        let mut m = modules();
        m[0].settings[0].value = value;
        assert!(profiles::encode(&m).is_err());
    }
    let duplicate = vec![Module::new("A B", Category::Render, "").bool("X", true), Module::new("AB", Category::Render, "").bool("X", false)];
    assert!(profiles::encode(&duplicate).is_err());
}

#[test]
fn pending_plugin_config_is_preserved_without_stale_known_values() {
    let raw = "flight.speed=999\nlateplugin.speed=4\nlateplugin.on=true\npanel.render=old\nfriends=old\n";
    let result = profiles::preserve_pending(raw, "flight.speed=2\nfriends=current\n", &modules());
    assert_eq!(result, "flight.speed=2\nfriends=current\nlateplugin.speed=4\nlateplugin.on=true\n");
}

#[test]
fn read_write_list_delete_and_overwrite_use_only_native_storage() {
    let temp = Temp::new();
    let store = temp.store();
    store.write("z-last", "old").unwrap();
    store.write("alpha", "one").unwrap();
    store.write("z-last", "new").unwrap();
    assert_eq!(store.read("Z-LAST").unwrap(), "new");
    assert_eq!(store.list().unwrap(), ["alpha", "z-last"]);
    store.delete("alpha").unwrap();
    assert!(store.read("alpha").is_err());
    assert!(store.delete("../escape").is_err());
    assert!(store.delete("before-load").is_err());
    assert!(!temp.0.join("escape.profile").exists());
}

#[test]
fn size_utf8_and_profile_count_limits() {
    let temp = Temp::new();
    let store = temp.store();
    store.write("kept", "old").unwrap();
    assert!(store.write("kept", &"x".repeat(profiles::MAX_BYTES + 1)).is_err());
    assert_eq!(store.read("kept").unwrap(), "old");
    fs::write(temp.file("large.profile"), vec![b'x'; profiles::MAX_BYTES + 1]).unwrap();
    assert!(store.read("large").is_err());
    fs::write(temp.file("invalid.profile"), [0xff, 0xfe]).unwrap();
    assert!(store.read("invalid").is_err());
    store.delete("large").unwrap(); store.delete("invalid").unwrap();
    for i in 0..127 { store.write(&format!("p{i}"), "x").unwrap(); }
    assert!(store.write("overflow", "x").is_err());
    store.write("kept", "still allowed").unwrap();
}

#[test]
fn atomic_replacement_never_exposes_partial_or_missing_destination() {
    let temp = Temp::new();
    let store = temp.store();
    let old = "a".repeat(32768);
    let new = "b".repeat(65536);
    store.write("atomic", &old).unwrap();
    let root = temp.native();
    let thread = std::thread::spawn(move || {
        let reader = Store::new(root).unwrap();
        for _ in 0..100 {
            let text = reader.read("atomic").unwrap();
            assert!(text == "a".repeat(32768) || text == "b".repeat(65536));
        }
    });
    for i in 0..50 { store.write("atomic", if i % 2 == 0 { &new } else { &old }).unwrap(); }
    thread.join().unwrap();
    assert!(fs::read_dir(temp.file("")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

#[cfg(windows)]
#[test]
fn windows_failed_replace_preserves_destination_and_cleans_scratch() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = Temp::new();
    let store = temp.store();
    store.write("locked", "original").unwrap();
    // Allow reads/metadata and writes, but deny FILE_SHARE_DELETE so the final rename fails.
    let locked = fs::OpenOptions::new().read(true).share_mode(3).open(temp.file("locked.profile")).unwrap();
    assert!(store.write("locked", "replacement").is_err());
    drop(locked);
    assert_eq!(store.read("locked").unwrap(), "original");
    assert!(fs::read_dir(temp.file("")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

fn directory_link(link: &Path, target: &Path) {
    #[cfg(unix)] { std::os::unix::fs::symlink(target, link).unwrap(); }
    #[cfg(windows)] {
        // Junctions require no developer mode or administrator token and are reparse points.
        use std::os::windows::process::CommandExt;
        let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let script = format!("$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null", quote(link), quote(target));
        assert!(std::process::Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000).status().unwrap().success());
    }
}

fn remove_directory_link(link: &Path) {
    #[cfg(windows)] { fs::remove_dir(link).unwrap(); }
    #[cfg(unix)] { fs::remove_file(link).unwrap(); }
}

#[test]
fn links_and_windows_junctions_cannot_redirect_storage() {
    let temp = Temp::new();
    let external = temp.0.join("outside");
    fs::create_dir(&external).unwrap();
    let root_link = temp.0.join("native-link");
    directory_link(&root_link, &external);
    assert!(Store::new(root_link.clone()).is_err());
    remove_directory_link(&root_link);
    fs::create_dir(temp.native()).unwrap();
    let profiles_link = temp.native().join("profiles");
    directory_link(&profiles_link, &external);
    assert!(Store::new(temp.native()).is_err());
    remove_directory_link(&profiles_link);
    let store = temp.store();
    directory_link(&temp.file("linked.profile"), &external);
    assert!(store.read("linked").is_err());
    assert!(store.write("linked", "bad").is_err());
    assert!(store.delete("linked").is_err());
    remove_directory_link(&temp.file("linked.profile"));
    assert_eq!(fs::read_dir(&external).unwrap().count(), 0);
}

fn game(temp: &Temp) -> Game<'static> {
    *STATE.lock().unwrap() = Some(State {
        modules: modules(), raw: "lateplugin.speed=4\n".into(), friends: vec!["Friend".into()],
        waypoints: vec![Waypoint { name: "Home".into(), pos: [1.0, 2.0, 3.0], dim: "overworld".into() }],
        gui: Gui { panels: (0..5).map(|_| Panel { x: 20.0, y: 30.0, collapsed: true, scroll: 0.0 }).collect() },
    });
    Game { root: temp.native(), messages: Vec::new(), marker: std::marker::PhantomData }
}

#[test]
fn command_load_saves_backups_preserves_scope_and_can_restore() {
    let temp = Temp::new();
    let mut game = game(&temp);
    profile_command(&mut game, &["save", "mining"]);
    assert!(game.messages.last().unwrap().starts_with("Saved"));
    with(|s| s.modules[0].settings[0].value = Value::Num { v: 8.5, min: 0.1, max: 50.0, step: 0.1 });
    profile_command(&mut game, &["load", "mining"]);
    assert!(game.messages.last().unwrap().starts_with("Loaded"));
    with(|s| {
        assert_eq!(s.modules[0].n("Speed"), 1.123456789123);
        assert_eq!(s.modules[0].key.as_deref(), Some("key.keyboard.f"));
        assert!(s.modules[1].on);
        assert_eq!(s.friends, ["Friend"]);
        assert_eq!(s.waypoints[0].name, "Home");
        assert_eq!(s.gui.panels[0].x, 20.0);
        assert!(s.raw.contains("lateplugin.speed=4"));
    });
    let backup = fs::read_to_string(temp.file("before-load.config")).unwrap();
    assert!(backup.contains("flight.speed=8.5"));
    assert!(backup.contains("friends=Friend"));
    assert!(backup.contains("waypoint=Home|1|2|3|overworld"));
    assert!(backup.contains("panel.combat=20,30,true"));
    assert!(fs::read_to_string(temp.native().join("config")).unwrap().contains("flight.speed=1.123456789123"));
    let persisted = fs::read_to_string(temp.native().join("config")).unwrap();
    with(|s| {
        s.modules[0].settings[0].value = Value::Num { v: 2.0, min: 0.1, max: 50.0, step: 0.1 };
        s.load_text(&persisted);
        assert_eq!(s.modules[0].n("Speed"), 1.123456789123);
        assert_eq!(s.friends, ["Friend"]);
        assert_eq!(s.waypoints[0].name, "Home");
    });
    profile_command(&mut game, &["load", "before-load"]);
    with(|s| assert_eq!(s.modules[0].n("Speed"), 8.5));
}

#[test]
fn command_parse_backup_and_config_failures_leave_runtime_unchanged() {
    let temp = Temp::new();
    let mut game = game(&temp);
    let store = temp.store();
    store.write("broken", "solstice-settings-profile=1\nflight.antikick=bool:false\nflight.speed=num:NaN").unwrap();
    fs::write(temp.native().join("config"), "unchanged").unwrap();
    profile_command(&mut game, &["load", "broken"]);
    with(|s| assert!(s.modules[0].b("Anti kick")));
    assert_eq!(fs::read_to_string(temp.native().join("config")).unwrap(), "unchanged");
    assert!(!temp.file("before-load.profile").exists());
    store.write("good", "solstice-settings-profile=1\nflight.antikick=bool:false").unwrap();
    fs::create_dir(temp.file("before-load.profile")).unwrap();
    profile_command(&mut game, &["load", "good"]);
    with(|s| assert!(s.modules[0].b("Anti kick")));
    assert_eq!(fs::read_to_string(temp.native().join("config")).unwrap(), "unchanged");
    fs::remove_dir(temp.file("before-load.profile")).unwrap();
    fs::create_dir(temp.file("before-load.config")).unwrap();
    profile_command(&mut game, &["load", "good"]);
    with(|s| assert!(s.modules[0].b("Anti kick")));
    assert_eq!(fs::read_to_string(temp.native().join("config")).unwrap(), "unchanged");
    fs::remove_dir(temp.file("before-load.config")).unwrap();
    fs::remove_file(temp.native().join("config")).unwrap();
    fs::create_dir(temp.native().join("config")).unwrap();
    profile_command(&mut game, &["load", "good"]);
    with(|s| assert!(s.modules[0].b("Anti kick")));
    assert!(game.messages.last().unwrap().starts_with("Profile:"));
}

#[test]
fn command_usage_active_guard_reserved_backup_and_delete() {
    let temp = Temp::new();
    let mut game = game(&temp);
    for args in [vec![], vec!["load"], vec!["list", "extra"], vec!["unknown"], vec!["save", "a", "extra"]] {
        profile_command(&mut game, &args);
        assert!(game.messages.last().unwrap().contains("Use .profile"));
    }
    for action in ["save", "delete"] {
        profile_command(&mut game, &[action, "BEFORE-LOAD"]);
        assert!(game.messages.last().unwrap().contains("reserved"));
    }
    temp.store().write("active", "solstice-settings-profile=1\nhud.fps=bool:false").unwrap();
    profile_command(&mut game, &["load", "active"]);
    assert!(game.messages.last().unwrap().contains("HUD"));
    assert!(!temp.file("before-load.profile").exists());
    profile_command(&mut game, &["list"]);
    assert!(game.messages.last().unwrap().contains("active"));
    profile_command(&mut game, &["delete", "active"]);
    assert!(game.messages.last().unwrap().starts_with("Deleted"));
}
