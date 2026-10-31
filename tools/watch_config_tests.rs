#[path = "../mixins/sunset/watch_config.rs"]
mod watch_config;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use watch_config::{Config, Profile, Store, normalize_address, parse_players};

fn profile(id: &str, address: &str, players: &[&str]) -> Profile {
    Profile { id: id.into(), addresses: vec![address.into()], players: players.iter().map(|s| (*s).into()).collect(), command: "msg".into() }
}

static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("sunset-watch-tests-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn host_ports_and_ipv6_are_exact_without_dns() {
    for (input, expected) in [
        (" DONUTSMP.NET. ", "donutsmp.net:25565"),
        ("DonutSMP.net:025565", "donutsmp.net:25565"),
        ("localhost:1", "localhost:1"), ("127.0.0.1", "127.0.0.1:25565"),
        ("[2001:0db8:0:0::1]:25566", "[2001:db8::1]:25566"),
        ("::1", "[::1]:25565"), ("[::ffff:192.0.2.1]", "[::ffff:192.0.2.1]:25565"),
    ] {
        assert_eq!(normalize_address(input).unwrap(), expected, "{input}");
    }
    for input in ["", ":25565", "host:", "host:0", "host:65536", "host:-1", "host:+1", "host:2:3", "x..net", "x.net..", "-x.net", "x_.net", "https://x.net", "x.net/a", "x.net?q", "a b", "é.net", "999.1.2.3", "127.00.0.1", "[::1]evil", "[::1]:", "[::1", "[host]:25565", "[fe80::1%3]"] {
        assert!(normalize_address(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn auto_selection_matches_alias_and_port_only() {
    let mut config = Config::defaults();
    assert!(config.profiles[0].players.is_empty());
    assert_eq!(config.resolve("auto", Some("DONUTSMP.NET.")).unwrap().unwrap().id, "donutsmp");
    for input in ["evil-donutsmp.net", "donutsmp.net.evil.test", "donutsmp.net:25566"] {
        assert!(config.resolve("auto", Some(input)).unwrap().is_none());
    }
    assert!(config.resolve("auto", None).unwrap().is_none());
    assert_eq!(config.resolve("DonutSMP", None).unwrap().unwrap().id, "donutsmp");
    assert!(config.resolve("missing", None).is_err());
    let mut custom = profile("custom", "somewhere.test:25566", &["Alice"]);
    custom.addresses.push("[::1]:25566".into());
    config.upsert(custom).unwrap();
    assert_eq!(config.resolve("auto", Some("somewhere.test:25566")).unwrap().unwrap().id, "custom");
    assert_eq!(config.resolve("auto", Some("[0:0:0:0:0:0:0:1]:25566")).unwrap().unwrap().id, "custom");
    let old = config.clone();
    assert!(config.upsert(profile("conflict", "DONUTSMP.NET.:25565", &[])).is_err());
    assert_eq!(config, old);
    assert!(config.upsert(profile("auto", "other.test", &[])).is_err());
    assert_eq!(config, old);
}

#[test]
fn java_names_are_bounded_and_case_insensitively_deduplicated() {
    assert_eq!(parse_players("Alice,alice BOB\nBob\t_123").unwrap(), ["Alice", "BOB", "_123"]);
    for name in [".Bedrock", "a-b", "a/b", "a;stop", "a\\b", "é", "abcdefghijklmnopq"] {
        assert!(parse_players(name).is_err(), "accepted {name}");
    }
    assert!(parse_players(&"a".repeat(4097)).is_err());
    let many = (0..129).map(|i| format!("Player{i}")).collect::<Vec<_>>().join(",");
    assert!(parse_players(&many).is_err());
    let boundary = (0..128).map(|i| format!("Player{i}")).collect::<Vec<_>>().join(",");
    assert_eq!(parse_players(&boundary).unwrap().len(), 128);
}

#[test]
fn edits_are_transactional_and_limits_apply() {
    let mut config = Config::defaults();
    let mut p = profile("CUSTOM", "EXAMPLE.NET.", &["Alice", "alice"]);
    p.addresses.push("example.net:25565".into());
    config.upsert(p).unwrap();
    let p = config.profile("custom").unwrap();
    assert_eq!(p.addresses, ["example.net:25565"]);
    assert_eq!(p.players, ["Alice"]);
    assert_eq!(config.list(), ["donutsmp", "custom"]);
    let previous = config.clone();
    for command in ["/msg", "msg ", "msg Alice", "MSG", "msg\nstop", "msg;stop", "execute", ""] {
        let mut bad = profile("custom", "example.net", &[]);
        bad.command = command.into();
        assert!(config.upsert(bad).is_err());
        assert_eq!(config, previous);
    }
    for id in ["../x", "x/y", "", "a b", "-start", "é", "abcdefghijklmnopqrstuvwxyz0123456"] {
        assert!(config.upsert(profile(id, "unused.test", &[])).is_err());
    }
    let mut bad = profile("custom", "example.net", &[]);
    bad.addresses = (0..9).map(|n| format!("alias{n}.test")).collect();
    assert!(config.upsert(bad).is_err());
    assert_eq!(config, previous);
    config.remove("CUSTOM").unwrap();
    assert!(config.remove("custom").is_err());
    for n in 1..32 { config.upsert(profile(&format!("p{n}"), &format!("p{n}.test"), &[])).unwrap(); }
    assert!(config.upsert(profile("overflow", "extra.test", &[])).is_err());
    assert_eq!(config.profiles.len(), 32);
}

#[test]
fn schema_is_strict_and_round_trips_without_unknown_fields() {
    let mut config = Config::defaults();
    config.upsert(profile("private", "[::1]:25566", &["Alice", "Bob"])).unwrap();
    let text = config.serialize().unwrap();
    assert_eq!(Config::parse(&text).unwrap(), config);
    assert_eq!(Config::parse("sunset-watchlists=1\n").unwrap().profiles.len(), 0);
    for bad in [
        text.replace("sunset-watchlists=1", "sunset-watchlists=2"),
        text.replace("command=msg", "command=msg\nunknown=1"),
        text.replace("players=", "players=,"),
        text.replace("addresses=", "addresses=,"),
        text.replace("\n", "\r\n"),
        text.trim_end().into(), format!("{text}\n"),
        format!("{text}profile=incomplete\n"),
        text.replace("end", "end=1"),
        format!("{text}{}", "x".repeat(128 * 1024)),
    ] { assert!(Config::parse(&bad).is_err(), "accepted {bad:?}"); }
    let duplicate = format!("{}{}", text, text.strip_prefix("sunset-watchlists=1\n").unwrap());
    assert!(Config::parse(&duplicate).is_err());
}

#[test]
fn store_load_save_replaces_and_rejects_bad_input_without_overwriting() {
    let scratch = Scratch::new();
    let store = Store::new(scratch.0.join("data")).unwrap();
    let path = scratch.0.join("data/watchlists-v1");
    assert_eq!(store.load().unwrap(), Config::defaults());
    assert!(!path.exists(), "loading defaults must not write a file");
    let mut config = Config::defaults();
    store.save(&config).unwrap();
    config.upsert(profile("custom", "custom.test", &["Alice"])).unwrap();
    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);
    let previous_bytes = fs::read(&path).unwrap();
    let mut bad = config.clone();
    bad.profiles[0].command = "msg Alice".into();
    assert!(store.save(&bad).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous_bytes);
    fs::write(&path, b"malformed user file\n").unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&config).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"malformed user file\n");
    fs::write(&path, [0xff]).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&config).is_err());
    assert_eq!(fs::read(&path).unwrap(), [0xff]);
    fs::write(&path, vec![b'a'; 128 * 1024 + 1]).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&config).is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), 128 * 1024 + 1);
}

#[test]
fn store_reports_invalid_file_type_without_removing_it() {
    let scratch = Scratch::new();
    let store = Store::new(scratch.0.clone()).unwrap();
    fs::create_dir(scratch.0.join("watchlists-v1")).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&Config::defaults()).is_err());
    assert!(scratch.0.join("watchlists-v1").is_dir());
    assert!(Store::new(scratch.0.join("watchlists-v1/file")).is_ok());
    fs::write(scratch.0.join("ordinary-file"), b"keep").unwrap();
    assert!(Store::new(scratch.0.join("ordinary-file/subdirectory")).is_err());
    assert_eq!(fs::read(scratch.0.join("ordinary-file")).unwrap(), b"keep");
}

#[cfg(windows)]
#[test]
fn windows_failed_atomic_replace_preserves_previous_file_and_cleans_owned_temp() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let scratch = Scratch::new();
    let store = Store::new(scratch.0.clone()).unwrap();
    store.save(&Config::defaults()).unwrap();
    let path = scratch.0.join("watchlists-v1");
    let previous = fs::read(&path).unwrap();
    // Allow readers, but deny delete/replace while the original is open.
    let lock = OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
    let mut changed = Config::defaults();
    changed.upsert(profile("custom", "custom.test", &["Alice"])).unwrap();
    assert!(store.save(&changed).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
    drop(lock);
    store.save(&changed).unwrap();
    assert_eq!(store.load().unwrap(), changed);
}

#[cfg(unix)]
#[test]
fn links_are_rejected() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    fs::create_dir(scratch.0.join("real")).unwrap();
    symlink(scratch.0.join("real"), scratch.0.join("linked")).unwrap();
    assert!(Store::new(scratch.0.join("linked/nested")).is_err());
    let store = Store::new(scratch.0.join("real")).unwrap();
    fs::write(scratch.0.join("outside"), b"keep").unwrap();
    symlink(scratch.0.join("outside"), scratch.0.join("real/watchlists-v1")).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&Config::defaults()).is_err());
    assert_eq!(fs::read(scratch.0.join("outside")).unwrap(), b"keep");
}
