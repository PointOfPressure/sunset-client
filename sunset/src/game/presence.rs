// Sunset Presence (Client): anonymous usage counts for Sunset's developers, and (on by default) letting a Sunset
// developer on the same server see that you use Sunset (a sun before your name in their tab list). Settings are the
// module's, in Options > Mods > Sunset > Client > Sunset Presence (it is menu-only: not in the ClickGUI); the module
// off sends nothing at all. The backend is presence-backend/ (Cloudflare Workers).
//
// Anonymous analytics: every 10 minutes while in a world, one ping of the Sunset version, the OS family and four flags:
// whether it is this install's first ping today, this ISO week, this month and ever (the dates are kept in a local file,
// never sent). No ID of any kind: the dashboard counts actives by summing the flags and instances by counting pings.
//
// Share in-game presence (on by default): on a server (not single player), a session in that server's room: a SHA-256
// of its address, your player UUID and a short-lived token from the backend. Developers in the room are sent to your
// client as challenge hashes (SHA-256 of the room's nonce and their UUID); your client answers with those that match a
// player in your own tab list. A developer is shown only the players whose own client saw them on that server, and their
// client shows the sun only for players in its own tab list. Turning it off (or leaving the server) ends the session.
//
// A developer is whoever has a personal key, from the backend's maintainer, in the file presence-dev-key in Sunset's
// data folder (never in a build, a config, a profile or a debug report); the backend checks it on every request and the
// maintainer can revoke it.
//
// All network work is on one background thread ("sunset-presence"); the game thread only hands it a snapshot once a
// second (settings, the server, and the tab list's UUIDs when it asks for them). Failures are silent, with backoff.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset::{Category, ID, Module, with};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub(super) const NAME: &str = "Sunset Presence";
const BASE: &str = "https://sunset-presence.s4yerh4rris0n.workers.dev";
const ANALYTICS_EVERY: Duration = Duration::from_secs(600);
// The game thread's snapshot older than this (no world: the title screen, a loading screen) means nothing is wanted.
const STALE: Duration = Duration::from_secs(5);
// Shown once, before anything is sent. NOTICE_KEY_1 is the notice of 1.2.1 (analytics only): an install that saw it is
// told again only when it shares its presence.
const NOTICE_KEY: &str = "presence-notice-2";
const NOTICE_KEY_1: &str = "presence-notice";
const KEY_FILE: &str = "presence-dev-key";
const SEEN_FILE: &str = "analytics-seen";
const ROSTER_MAX: usize = 500;
pub(super) const NOTICE: &str = "By default Sunset sends anonymous stats, and your UUID and a server hash so Sunset devs \
    there see you. Off: Options > Mods > Sunset > Client > Sunset Presence.";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(
            NAME,
            Category::Client,
            "Anonymous analytics: every 10 minutes in a world, the Sunset version, your OS and whether it is today's, this \
             week's or this month's first launch. No ID, name or server. Share in-game presence (on by default): on a \
             server, your player UUID and a hash of the server's address, so Sunset developers on that server see a sun \
             by your name in the tab list. Off: nothing is sent. Here: Options > Mods > Sunset > Client > Sunset Presence.",
        )
        .on()
        .menu_only()
        .bool("Anonymous analytics", true)
        .bool("Share in-game presence", true),
    ]
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Room {
    key: String,
    uuid: u128,
    dev_key: Option<String>,
}

#[derive(Default)]
struct Shared {
    analytics: bool,
    room: Option<Room>,
    // When the game thread last handed over its snapshot.
    at: Option<Instant>,
    data: PathBuf,
    // The worker wants the tab list's UUIDs (to answer challenges); the game thread fills roster.
    roster_wanted: bool,
    roster: Option<Vec<u128>>,
    // For a developer: the room and the players the backend confirmed there.
    shown: Option<(String, Vec<u128>)>,
    started: bool,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
static WAKE: Condvar = Condvar::new();
static NOTICE_DONE: AtomicBool = AtomicBool::new(false);
// The server last joined and its room key, so the address is hashed and the key file read once per server.
static ROOM_CACHE: Mutex<Option<(String, String, Option<String>)>> = Mutex::new(None);

// The backend: SUNSET_PRESENCE_BASE overrides it (a test backend), and "off" sends nothing at all.
fn base() -> Option<&'static str> {
    static BASE_URL: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BASE_URL
        .get_or_init(|| match std::env::var("SUNSET_PRESENCE_BASE") {
            Ok(v) if v.trim() == "off" => None,
            Ok(v) if !v.trim().is_empty() => Some(v.trim().trim_end_matches('/').to_owned()),
            _ => Some(BASE.to_owned()),
        })
        .as_deref()
}

// A line in the game's log, with no IDs, names or servers: what Presence did, for bug reports and the test rig.
fn log_line(text: &str) {
    horizon_client::logging::write_only(log::Level::Info, &format!("[Sunset presence] {text}"));
}

fn shared<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Shared::default))
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

// The room of a server: its address in lower case, without the default port.
pub(super) fn room_key(address: &str) -> String {
    let a = address.trim().trim_end_matches('.').to_ascii_lowercase();
    let a = a.strip_suffix(":25565").unwrap_or(&a);
    sha256_hex(&format!("sunset-presence:{a}"))
}

pub(super) fn challenge(nonce: &str, uuid: u128) -> String {
    sha256_hex(&format!("{nonce}:{uuid:032x}"))[..16].to_owned()
}

// The challenges a tab list answers: those that are the hash of a player in it.
fn answer(nonce: &str, challenges: &[String], roster: &[u128]) -> Vec<String> {
    let mine: std::collections::HashSet<String> = roster.iter().map(|u| challenge(nonce, *u)).collect();
    challenges.iter().filter(|c| mine.contains(*c)).cloned().collect()
}

// Once a second from the game's tick, in a world.
pub(super) fn tick(game: &mut Game<'_>) {
    if game.ticks % 20 != 0 {
        return;
    }
    let (on, analytics, presence) =
        with(|s| s.module(NAME).map_or((false, false, false), |m| (m.on, m.b("Anonymous analytics"), m.b("Share in-game presence"))));
    let analytics = on && analytics;
    // The one first-launch notice, before anything is sent.
    if (analytics || on && presence) && !NOTICE_DONE.load(Relaxed) {
        NOTICE_DONE.store(true, Relaxed);
        if game.native_setting(ID, NOTICE_KEY).is_none() && (presence || game.native_setting(ID, NOTICE_KEY_1).is_none()) {
            super::sunset_toast::push_notice(NOTICE);
            game.native_set_setting(ID, NOTICE_KEY, "1");
        }
    }
    let network = base().is_some();
    let analytics = analytics && network;
    let room = if on && presence && network {
        room_of(game)
    } else {
        // Off: the next time it goes on, the address is hashed and the key file read again.
        *ROOM_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        None
    };
    let in_room = room.as_ref().map(|r| r.key.clone());
    let data = shared(|s| s.data.as_os_str().is_empty()).then(|| game.native_data_dir(ID));
    let (wanted, shown, changed, start) = shared(|s| {
        let changed = s.analytics != analytics || s.room != room;
        s.analytics = analytics;
        s.room = room;
        s.at = Some(Instant::now());
        if let Some(data) = data {
            s.data = data;
        }
        if s.room.is_none() {
            s.shown = None;
        }
        let start = (analytics || s.room.is_some()) && !s.started;
        s.started |= start;
        (s.roster_wanted, s.shown.clone(), changed, start)
    });
    if start {
        let _ = std::thread::Builder::new().name("sunset-presence".into()).spawn(worker);
    } else if changed {
        WAKE.notify_all();
    }
    // The tab list's players, when the worker needs them or a developer's suns are to be placed.
    let mark = shown.filter(|(key, users)| Some(key) == in_room.as_ref() && !users.is_empty());
    if !wanted && mark.is_none() {
        crate::game::sunset_tab_modes::set_sunset(Vec::new());
        return;
    }
    let roster = roster_of(game);
    if let Some((_, users)) = mark {
        crate::game::sunset_tab_modes::set_sunset(roster.iter().copied().filter(|u| users.contains(u)).collect());
    } else {
        crate::game::sunset_tab_modes::set_sunset(Vec::new());
    }
    if wanted {
        shared(|s| {
            s.roster = Some(roster);
            s.roster_wanted = false;
        });
        WAKE.notify_all();
    }
}

fn roster_of(game: &Game<'_>) -> Vec<u128> {
    super::remote::sunset_remote::tab_uuids(game, ROSTER_MAX).map(|(_, list)| list).unwrap_or_default()
}

fn room_of(game: &Game<'_>) -> Option<Room> {
    let r = game.remote.as_ref()?;
    let address = horizon_client::net::shown_address(&r.session.address);
    if address == "local" {
        return None;
    }
    let mut cache = ROOM_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.as_ref().is_none_or(|(a, _, _)| *a != address) {
        let key = std::fs::read_to_string(game.native_data_dir(ID).join(KEY_FILE))
            .ok()
            .map(|k| k.trim().to_owned())
            .filter(|k| (24..=128).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        *cache = Some((address.clone(), room_key(&address), key));
    }
    let (_, key, dev_key) = cache.as_ref()?;
    let (uuid, _) = super::remote::sunset_remote::tab_uuids(game, 0)?;
    Some(Room { key: key.clone(), uuid, dev_key: dev_key.clone() })
}

// ---------------------------------------------------------------- the background thread

struct Session {
    room: Room,
    token: Option<String>,
    next: Instant,
    nonce: String,
    challenges: Vec<String>,
    // The challenges answered last, so a new set is answered at once.
    answered: Vec<String>,
    fails: u32,
}

// After n failures in a row: 30 s, 1, 2, 4... minutes, at most 30.
fn backoff(fails: u32) -> Duration {
    Duration::from_secs((30u64 << fails.saturating_sub(1).min(6)).min(1800))
}

fn running() -> bool {
    ::sunset_core::lifecycle::RUNTIME.running()
}

fn worker() {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .user_agent("Sunset")
        .build();
    let mut session: Option<Session> = None;
    let mut next_ping: Option<Instant> = None;
    let mut ping_fails = 0u32;
    let mut started = false;
    let mut dev_rejected = false;
    loop {
        let (analytics, room, data, roster) = shared(|s| {
            let fresh = s.at.is_some_and(|t| t.elapsed() < STALE) && running();
            (fresh && s.analytics, if fresh { s.room.clone() } else { None }, s.data.clone(), s.roster.take())
        });
        let now = Instant::now();

        if analytics && next_ping.is_none_or(|t| now >= t) {
            match ping(&agent, &data, if started { "beat" } else { "start" }) {
                Ok(()) => {
                    log_line(if started { "analytics ping sent" } else { "analytics ping sent (start)" });
                    started = true;
                    ping_fails = 0;
                    next_ping = Some(now + ANALYTICS_EVERY);
                }
                Err(()) => {
                    log_line("analytics ping failed");
                    ping_fails += 1;
                    next_ping = Some(now + backoff(ping_fails).max(Duration::from_secs(60)));
                }
            }
        }

        let room = room.map(|mut r| {
            if dev_rejected {
                r.dev_key = None;
            }
            r
        });
        if session.as_ref().is_some_and(|s| Some(&s.room) != room.as_ref()) {
            let old = session.take().expect("checked");
            leave(&agent, &old);
            log_line("presence session ended");
            shared(|s| s.shown = None);
        }
        if let Some(room) = room {
            let s = session.get_or_insert_with(|| Session {
                room,
                token: None,
                next: now,
                nonce: String::new(),
                challenges: Vec::new(),
                answered: Vec::new(),
                fails: 0,
            });
            // Challenges to answer wait for the tab list (a second at most).
            let need_roster = !s.challenges.is_empty() && roster.is_none();
            if now >= s.next && need_roster {
                shared(|sh| sh.roster_wanted = true);
            } else if now >= s.next {
                let seen = roster.as_deref().filter(|_| !s.challenges.is_empty()).map(|r| answer(&s.nonce, &s.challenges, r));
                match beat(&agent, s, seen.as_deref()) {
                    Ok(reply) => {
                        if s.token.is_none() {
                            log_line(if s.room.dev_key.is_some() { "presence session started (developer)" } else { "presence session started" });
                        }
                        if seen.is_some() {
                            log_line(&format!("presence challenges answered ({} of {})", seen.as_ref().map_or(0, Vec::len), s.challenges.len()));
                        }
                        s.fails = 0;
                        if seen.is_some() {
                            s.answered = s.challenges.clone();
                        }
                        let new_challenges = reply.challenges != s.answered && !reply.challenges.is_empty();
                        s.token = Some(reply.token);
                        s.nonce = reply.nonce;
                        s.challenges = reply.challenges;
                        // A new set of challenges is answered soon (once the tab list is in), not at the next beat.
                        s.next = now + if new_challenges { Duration::from_secs(2) } else { Duration::from_secs(reply.beat.clamp(30, 600)) };
                        if new_challenges {
                            shared(|sh| sh.roster_wanted = true);
                        }
                        if s.room.dev_key.is_some() {
                            let key = s.room.key.clone();
                            let count = reply.users.len();
                            let before = shared(|sh| sh.shown.replace((key, reply.users)).map(|(_, u)| u.len()));
                            if before != Some(count) {
                                log_line(&format!("developer: {count} confirmed here"));
                            }
                        }
                    }
                    Err(Fail::Rejected) => {
                        // A key the backend refused: on as a player without it from now on.
                        dev_rejected = true;
                        s.room.dev_key = None;
                        s.token = None;
                        s.next = now;
                        shared(|sh| sh.shown = None);
                        super::sunset_toast::push(super::sunset_toast::Kind::Warning, "Sunset Presence: the developer key was refused.");
                    }
                    Err(Fail::Other) => {
                        log_line("presence beat failed");
                        s.fails += 1;
                        s.token = None;
                        s.next = now + backoff(s.fails);
                    }
                }
            }
        }

        if !running() && session.is_none() {
            return;
        }
        // Sleep until something is due, the game thread changes what is wanted, or (at most) 5 s to notice the game
        // thread has stopped handing over snapshots.
        let mut due = now + STALE;
        if analytics && let Some(t) = next_ping {
            due = due.min(t);
        }
        if let Some(s) = &session {
            due = due.min(s.next.max(now + Duration::from_millis(500)));
        }
        let g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        let wait = due.saturating_duration_since(Instant::now());
        drop(WAKE.wait_timeout(g, wait).unwrap_or_else(|e| e.into_inner()));
    }
}

enum Fail {
    Rejected,
    Other,
}

struct Reply {
    token: String,
    beat: u64,
    nonce: String,
    challenges: Vec<String>,
    users: Vec<u128>,
}

// A request to the backend: the response, or the status it refused with (None: unreachable, or no backend).
fn post(agent: &ureq::Agent, path: &str, body: &serde_json::Value, key: Option<&str>) -> Result<ureq::Response, Option<u16>> {
    let base = base().ok_or(None)?;
    let mut req = agent.post(&format!("{base}{path}")).set("content-type", "application/json");
    if let Some(key) = key {
        req = req.set("authorization", &format!("Bearer {key}"));
    }
    req.send_string(&body.to_string()).map_err(|e| match e {
        ureq::Error::Status(code, _) => Some(code),
        ureq::Error::Transport(_) => None,
    })
}

fn beat(agent: &ureq::Agent, s: &Session, seen: Option<&[String]>) -> Result<Reply, Fail> {
    let mut body = serde_json::json!({ "room": s.room.key, "uuid": format!("{:032x}", s.room.uuid) });
    if let Some(t) = &s.token {
        body["token"] = t.as_str().into();
    }
    if let Some(seen) = seen {
        body["seen"] = seen.iter().take(16).map(|h| serde_json::Value::from(h.as_str())).collect();
    }
    let reply = match post(agent, "/v1/p/beat", &body, s.room.dev_key.as_deref()) {
        Ok(r) => r,
        Err(Some(403)) => return Err(Fail::Rejected),
        Err(_) => return Err(Fail::Other),
    };
    parse_reply(&reply.into_string().map_err(|_| Fail::Other)?).ok_or(Fail::Other)
}

fn parse_reply(text: &str) -> Option<Reply> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let strings = |k: &str| -> Vec<String> {
        v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).take(64).collect()).unwrap_or_default()
    };
    Some(Reply {
        token: v["token"].as_str().filter(|t| t.len() == 32)?.to_owned(),
        beat: v["beat"].as_u64().unwrap_or(120),
        nonce: v["nonce"].as_str().unwrap_or_default().to_owned(),
        challenges: strings("challenges"),
        users: strings("users").iter().filter_map(|u| u128::from_str_radix(u, 16).ok()).take(ROSTER_MAX).collect(),
    })
}

fn leave(agent: &ureq::Agent, s: &Session) {
    if let Some(token) = &s.token {
        let _ = post(agent, "/v1/p/leave", &serde_json::json!({ "room": s.room.key, "token": token }), None);
    }
}

// The flags of a ping: first today (1), this ISO week (2), this month (4), ever (8), from the dates kept in the file.
fn flags(kept: Option<&str>, today: &[String; 3]) -> u8 {
    let Some(kept) = kept else {
        return 15;
    };
    let old: Vec<&str> = kept.split_whitespace().collect();
    (0..3).filter(|&i| old.get(i) != Some(&today[i].as_str())).fold(0, |f, i| f | 1 << i)
}

fn ping(agent: &ureq::Agent, data: &std::path::Path, event: &str) -> Result<(), ()> {
    let now = chrono::Utc::now();
    let today = [now.format("%Y-%m-%d").to_string(), now.format("%G-W%V").to_string(), now.format("%Y-%m").to_string()];
    let file = data.join(SEEN_FILE);
    let kept = std::fs::read_to_string(&file).ok();
    let f = flags(kept.as_deref(), &today);
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "windows" => "windows",
        "macos" => "macos",
        _ => "other",
    };
    let body = serde_json::json!({ "e": event, "v": super::sunset::VERSION, "os": os, "f": f });
    post(agent, "/v1/a", &body, None).map_err(|_| ())?;
    if f != 0 {
        let _ = std::fs::write(&file, today.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_keys_ignore_case_and_the_default_port() {
        let k = room_key("play.example.net");
        assert_eq!(k.len(), 64);
        assert_eq!(room_key("Play.Example.NET:25565"), k);
        assert_eq!(room_key("play.example.net."), k);
        assert_ne!(room_key("play.example.net:25566"), k);
        assert_ne!(room_key("other.example.net"), k);
    }

    #[test]
    fn challenges_match_the_backend_and_only_the_tab_list() {
        // The backend's hash: sha256("<nonce>:<uuid as 32 hex>"), first 16 hex digits.
        let dev = 0xdddd_dddd_dddd_dddd_dddd_dddd_dddd_dddd_u128;
        assert_eq!(challenge("n", dev), sha256_hex(&format!("n:{}", "d".repeat(32)))[..16]);
        let challenges = vec![challenge("n", dev), challenge("n", 5)];
        assert_eq!(answer("n", &challenges, &[1, dev, 3]), vec![challenge("n", dev)]);
        assert!(answer("n", &challenges, &[1, 3]).is_empty());
        // Another nonce (a restarted room) matches nothing.
        assert!(answer("m", &challenges, &[dev, 5]).is_empty());
        assert!(answer("n", &[], &[dev]).is_empty());
    }

    #[test]
    fn ping_flags_follow_the_kept_dates() {
        let today = ["2026-11-16".to_owned(), "2026-W47".to_owned(), "2026-11".to_owned()];
        assert_eq!(flags(None, &today), 15);
        assert_eq!(flags(Some("2026-11-16 2026-W47 2026-11"), &today), 0);
        assert_eq!(flags(Some("2026-11-15 2026-W47 2026-11"), &today), 1);
        assert_eq!(flags(Some("2026-11-09 2026-W46 2026-11"), &today), 3);
        assert_eq!(flags(Some("2026-10-31 2026-W44 2026-10"), &today), 7);
        assert_eq!(flags(Some("garbage"), &today), 7);
    }

    #[test]
    fn replies_are_read_defensively() {
        let r = parse_reply(r#"{"token":"0123456789abcdef0123456789abcdef","beat":60,"nonce":"x","challenges":["aa"],"users":["0000000000000000000000000000000a","zz"]}"#).unwrap();
        assert_eq!((r.beat, r.challenges.len()), (60, 1));
        assert_eq!(r.users, vec![10]);
        assert!(parse_reply(r#"{"token":"short"}"#).is_none());
        assert!(parse_reply("not json").is_none());
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff(1), Duration::from_secs(30));
        assert_eq!(backoff(2), Duration::from_secs(60));
        assert_eq!(backoff(50), Duration::from_secs(1800));
    }
}
