#![allow(dead_code)]
// Only Game/native surfaces are mocked. test_watch_ui.py stages production
// watchlist sources and extracts runtime's actual profile and save integration.
use std::{marker::PhantomData, path::PathBuf};
pub struct Game<'a> {
    data: PathBuf,
    messages: Vec<String>,
    _p: PhantomData<&'a ()>,
}
impl Game<'_> {
    fn native_data_dir(&self, _: &str) -> PathBuf {
        self.data.clone()
    }
    fn native_setting(&self, _: &str, _: &str) -> Option<String> {
        None
    }
    fn native_set_setting(&self, _: &str, _: &str, _: &str) {}
}
#[path = "profiles.rs"]
mod sunset_profiles;
#[path = "watch_config.rs"]
mod sunset_watch_config;
#[path = "watch_ui.rs"]
mod sunset_watch_ui;
mod sunset_watch_runtime {
    pub fn status() -> String {
        "no multiplayer connection".into()
    }
}
mod sunset {
    use super::Game;
    use std::sync::Mutex;
    pub const ID: &str = "sunset";
    #[derive(Clone)]
    pub enum Value {
        Text(String),
        Bool(bool),
        Pick {
            i: usize,
            of: &'static [&'static str],
        },
    }
    #[derive(Clone)]
    pub struct Setting {
        pub name: &'static str,
        pub value: Value,
    }
    impl Setting {
        fn parse(&mut self, value: &str) -> bool {
            match &mut self.value {
                Value::Text(v) => *v = value.into(),
                Value::Bool(v) => match value {
                    "true" => *v = true,
                    "false" => *v = false,
                    _ => return false,
                },
                Value::Pick { i, of } => match of.iter().position(|v| *v == value) {
                    Some(p) => *i = p,
                    None => return false,
                },
            }
            true
        }
    }
    #[derive(Clone)]
    pub struct Module {
        pub name: &'static str,
        pub on: bool,
        pub key: Option<String>,
        pub settings: Vec<Setting>,
    }
    impl Module {
        pub fn t(&self, n: &str) -> &str {
            self.settings
                .iter()
                .find_map(|s| match &s.value {
                    Value::Text(v) if s.name == n => Some(v.as_str()),
                    _ => None,
                })
                .unwrap_or("")
        }
        pub fn p(&self, n: &str) -> &'static str {
            self.settings
                .iter()
                .find_map(|s| match &s.value {
                    Value::Pick { i, of } if s.name == n => Some(of[*i]),
                    _ => None,
                })
                .unwrap_or("")
        }
        pub fn b(&self, n: &str) -> bool {
            self.settings
                .iter()
                .any(|s| s.name == n && matches!(s.value, Value::Bool(true)))
        }
    }
    pub struct State {
        pub modules: Vec<Module>,
    }
    impl State {
        pub fn module(&self, n: &str) -> Option<&Module> {
            self.modules.iter().find(|m| m.name == n)
        }
        fn save_text(&self) -> String {
            self.modules
                .iter()
                .flat_map(|m| {
                    m.settings.iter().map(move |s| {
                        let value = match &s.value {
                            Value::Text(v) => v.clone(),
                            Value::Bool(v) => v.to_string(),
                            Value::Pick { i, of } => of[*i].into(),
                        };
                        format!("{}.{}={}\n", norm(m.name), norm(s.name), value)
                    })
                })
                .collect()
        }
        fn profile_text(&self) -> String {
            self.save_text()
        }
    }
    include!("runtime_methods.rs");
    static STATE: Mutex<Option<State>> = Mutex::new(None);
    pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
        let mut guard = STATE.lock().unwrap();
        f(guard.as_mut().unwrap())
    }
    pub fn init() {
        *STATE.lock().unwrap() = Some(State {
            modules: vec![Module {
                name: "Anti Vanish",
                on: false,
                key: None,
                settings: vec![
                    Setting {
                        name: "Server profile",
                        value: Value::Text("auto".into()),
                    },
                    Setting {
                        name: "Edit profile",
                        value: Value::Text("donutsmp".into()),
                    },
                    Setting {
                        name: "Server addresses",
                        value: Value::Text("donutsmp.net".into()),
                    },
                    Setting {
                        name: "Watched players",
                        value: Value::Text("UntrustedModuleConfig".into()),
                    },
                    Setting {
                        name: "Completion command",
                        value: Value::Pick {
                            i: 0,
                            of: &["msg", "tell", "w"],
                        },
                    },
                    Setting {
                        name: "TAB checks",
                        value: Value::Bool(true),
                    },
                    Setting {
                        name: "Completion checks",
                        value: Value::Bool(false),
                    },
                ],
            }],
        });
    }
    pub fn ensure_loaded(game: &Game<'_>) {
        super::sunset_watch_ui::ensure_loaded(game);
    }
    fn norm(value: &str) -> String {
        value
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
    fn key_from_text(value: &str) -> Option<String> {
        Some(value.into())
    }
    fn release_changed_binds(_: &mut State) {}
    fn sync_flags(_: &State) {}
    include!("runtime_integration.rs");
    pub fn load_module_profile(game: &mut Game<'_>, name: &str) {
        profile_command(game, &["load", name]);
    }
    pub fn say(game: &mut Game<'_>, msg: &str) {
        game.messages.push(msg.into());
    }
}
fn text(name: &str, value: &str) {
    sunset::with(|s| {
        let setting = s.modules[0]
            .settings
            .iter_mut()
            .find(|s| s.name == name)
            .unwrap();
        setting.value = sunset::Value::Text(value.into());
    });
}
fn field(name: &str) -> String {
    sunset::with(|s| s.modules[0].t(name).into())
}
#[test]
fn actual_ui_controller_preserves_watchlists_and_reverts_failed_edits() {
    use sunset_watch_config::{Config, Profile, Store};
    let data = std::env::temp_dir().join(format!("watch-ui-regressions-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    let mut game = Game {
        data: data.clone(),
        messages: vec![],
        _p: PhantomData,
    };
    let store = Store::new(data.clone()).unwrap();
    let mut original = Config::defaults();
    original.profiles[0].players = vec!["Alice".into()];
    original
        .upsert(Profile {
            id: "second".into(),
            addresses: vec!["second.example".into()],
            players: vec!["Bob".into()],
            command: "w".into(),
        })
        .unwrap();
    store.save(&original).unwrap();
    sunset::init();
    let initial = sunset_watch_ui::settings(&game);
    assert!(!initial.enabled);
    assert!(initial.tab_checks);
    assert!(!initial.completion_checks);
    assert_eq!(*initial.config, original);
    assert_eq!(field("Watched players"), "Alice");
    // Polling unchanged fields shares the same immutable configuration snapshot.
    let unchanged = sunset_watch_ui::settings(&game);
    assert!(std::sync::Arc::ptr_eq(&initial.config, &unchanged.config));
    assert_eq!(initial.generation, unchanged.generation);
    // Every command works with the module disabled.
    sunset_watch_ui::command(&mut game, &["create", "third", "third.example"]);
    assert!(store.load().unwrap().profile("third").is_some());
    sunset_watch_ui::command(&mut game, &["add", "third", "Carol"]);
    assert_eq!(
        store.load().unwrap().profile("third").unwrap().players,
        ["Carol"]
    );
    sunset_watch_ui::command(&mut game, &["edit", "second"]);
    assert_eq!(field("Watched players"), "Bob");
    // A selector change must load the new profile before old fields can commit.
    let before = store.load().unwrap();
    text("Edit profile", "donutsmp");
    text("Watched players", "Hijacked");
    sunset_watch_ui::sync(&game);
    assert_eq!(store.load().unwrap(), before);
    assert_eq!(field("Watched players"), "Alice");
    // Alias conflict and invalid player both revert as a whole.
    text("Server profile", "second");
    text("Server addresses", "second.example");
    text("Watched players", "Other");
    sunset_watch_ui::sync(&game);
    assert_eq!(field("Server profile"), "auto");
    assert_eq!(field("Watched players"), "Alice");
    assert_eq!(store.load().unwrap(), before);
    assert!(!sunset_watch_ui::take_notices().is_empty());
    text("Watched players", "bad-name");
    sunset_watch_ui::sync(&game);
    assert_eq!(field("Watched players"), "Alice");
    assert_eq!(store.load().unwrap(), before);
    // Valid comma / whitespace names are normalized and actually persisted.
    text("Watched players", "Alice, Dave dave");
    sunset_watch_ui::sync(&game);
    assert_eq!(
        store.load().unwrap().profile("donutsmp").unwrap().players,
        ["Alice", "Dave"]
    );
    assert_eq!(field("Watched players"), "Alice, Dave");
    // Exercise runtime's actual .profile load boundary, including its backup,
    // validation and save hook, with different embedded watchlist editor text.
    let before = store.load().unwrap();
    sunset_profiles::Store::new(data.clone()).unwrap().write("different", "sunset-profile=1\nantivanish.editprofile=donutsmp\nantivanish.watchedplayers=UntrustedProfile\n").unwrap();
    sunset::load_module_profile(&mut game, "different");
    assert_eq!(store.load().unwrap(), before);
    assert_eq!(field("Watched players"), "Alice, Dave");
    assert!(
        game.messages
            .iter()
            .any(|m| m.starts_with("Loaded profile different."))
    );
    // Explicit selection then deletion switches back to safe auto mode.
    sunset_watch_ui::command(&mut game, &["use", "third"]);
    assert_eq!(sunset_watch_ui::settings(&game).selection, "third");
    sunset_watch_ui::command(&mut game, &["remove", "third", "carol"]);
    assert!(
        store
            .load()
            .unwrap()
            .profile("third")
            .unwrap()
            .players
            .is_empty()
    );
    sunset_watch_ui::command(&mut game, &["delete", "third"]);
    assert_eq!(sunset_watch_ui::settings(&game).selection, "auto");
    // Checked write failure leaves disk, memory and displayed fields unchanged.
    sunset_watch_ui::command(&mut game, &["edit", "donutsmp"]);
    let before = sunset_watch_ui::settings(&game).config;
    let blocked = data.join("not-a-directory");
    std::fs::write(&blocked, b"x").unwrap();
    game.data = blocked;
    text("Watched players", "Eve");
    sunset_watch_ui::sync(&game);
    assert_eq!(sunset_watch_ui::settings(&game).config, before);
    assert_eq!(field("Watched players"), "Alice, Dave");
    assert_eq!(store.load().unwrap(), *before);
    game.data = data.clone();
    // An externally corrupted file is preserved instead of being overwritten.
    std::fs::write(data.join("watchlists-v1"), b"unsupported-version\n").unwrap();
    text("Watched players", "Eve");
    sunset_watch_ui::sync(&game);
    assert_eq!(
        std::fs::read(data.join("watchlists-v1")).unwrap(),
        b"unsupported-version\n"
    );
    assert_eq!(sunset_watch_ui::settings(&game).config, before);
    assert_eq!(field("Watched players"), "Alice, Dave");
    assert!(!sunset_watch_ui::take_notices().is_empty());
    // Cleanup is constrained to the freshly created, process-specific temp dir.
    std::fs::remove_dir_all(&data).unwrap();
}

#[test]
fn unreadable_startup_file_blocks_edits_and_network_settings() {
    let data = std::env::temp_dir().join(format!("watch-ui-load-error-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    let file = data.join("watchlists-v1");
    let bad = b"sunset-watchlists=999\n";
    std::fs::write(&file, bad).unwrap();
    let mut game = Game {
        data: data.clone(),
        messages: vec![],
        _p: PhantomData,
    };
    sunset::init();
    sunset::with(|s| s.modules[0].on = true);
    let settings = sunset_watch_ui::settings(&game);
    assert!(!settings.enabled);
    assert!(!sunset_watch_ui::take_notices().is_empty());
    sunset_watch_ui::command(&mut game, &["add", "donutsmp", "Alice"]);
    assert_eq!(std::fs::read(&file).unwrap(), bad);
    assert!(
        sunset_watch_ui::settings(&game)
            .config
            .profile("donutsmp")
            .unwrap()
            .players
            .is_empty()
    );
    assert!(
        game.messages
            .iter()
            .any(|m| m.contains("cannot edit an unreadable watchlists-v1"))
    );
    std::fs::remove_dir_all(&data).unwrap();
}
