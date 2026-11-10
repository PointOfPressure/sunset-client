// The plugin API's test plugin: each `.plugcheck <case>` runs one part of the API and says what came back, for
// verify.py to read in the log.

use super::Game;
use super::sunset_api as api;
use std::sync::Mutex;

const _: () = api::needs(2);

const ID: &str = "plugcheck";

// Counts of each event since `.plugcheck reset`, by name, and the order the priority handlers ran in.
static SEEN: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());
static ORDER: Mutex<String> = Mutex::new(String::new());
static LAST: Mutex<String> = Mutex::new(String::new());
static REPLACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WATCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WATCH_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FROZEN: Mutex<Option<Vec<(&'static str, u64)>>> = Mutex::new(None);
static DORMANT_TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn seen(what: &'static str) {
    let mut s = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    match s.iter_mut().find(|(n, _)| *n == what) {
        Some((_, c)) => *c += 1,
        None => s.push((what, 1)),
    }
}

fn last(text: String) {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

pub(super) fn tick(game: &mut Game<'_>) {
    // This test-only direct hook observes the stopped bus without dispatching a
    // plugin callback. Real direct hooks must cooperate with api::running().
    if WATCH.load(std::sync::atomic::Ordering::Relaxed) && !api::running() {
        let now = SEEN.lock().unwrap_or_else(|e| e.into_inner()).iter().copied().filter(|(n, _)| *n != "stop").collect::<Vec<_>>();
        let mut frozen = FROZEN.lock().unwrap_or_else(|e| e.into_inner());
        if frozen.as_ref().is_some_and(|before| *before != now) {
            WATCH_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        frozen.get_or_insert(now);
        drop(frozen);
        // After Panic no plugin command runs (`.plugcheck watch-result` is the server's chat then): the result goes
        // to the game's log, for verify.py, once the stale task's time has long passed.
        if DORMANT_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 == 205 {
            WATCH.store(false, std::sync::atomic::Ordering::Relaxed);
            log::info!("{}", watch_result());
        }
    }
    api::plugin(game, ID, "0.1.0", setup);
    api::plugin(game, "plugcrash", "0.1.0", |p| {
        p.command("crash", "panics (a test)", |_game, _args| panic!("boom from plugcrash"));
    });
    api::plugin(game, "Bad Id", "0.1.0", |_p| {});
}

fn say(game: &mut Game<'_>, r: Result<String, String>) {
    match r {
        Ok(t) => api::chat(game, &format!("plugcheck ok: {t}")),
        Err(e) => api::chat(game, &format!("plugcheck err: {e}")),
    }
}

fn setup(p: &mut api::Plugin) {
    // Names that are taken or wrong: each refused with a chat line.
    p.module(api::Module::new("Kill Aura", api::Category::Combat, "a clash"));
    p.command("set", "a clash", |_game, _args| {});
    p.command("bad name", "a bad name", |_game, _args| {});
    p.module(api::Module::new("Plug Check", api::Category::Client, "The plugin API's test module.").bool("Flag", false));

    p.on_pre_tick(|_game| seen("pretick"));
    p.on_tick(|_game| seen("tick"));
    // Toggles and keys are only counted: the last line keeps hurts, stops and disconnects.
    p.on_toggle(|_game, _name, _on| seen("toggle"));
    p.on_key(|_game, key, down| {
        if down {
            seen("key");
            if key == "key.keyboard.h" {
                seen("hkey");
            }
        }
    });
    p.on_chat(|_game, line| {
        if line.text.contains("plugcheck-chat") {
            seen("chat");
        }
    });
    p.on_packet_in(|_game, packet| {
        if matches!(packet, api::packets::Clientbound::SetTime(_)) {
            seen("settime");
        }
    });
    p.on_hurt(|_game, h| {
        seen("hurt");
        last(format!("hurt {:.0} from {:.0}", h.health, h.before));
    });
    p.on_low_health(10.0, |_game, _h| seen("low"));
    p.on_death(|_game| seen("death"));
    p.on_join(|_game| seen("join"));
    p.on_disconnect(|_game, d| last(format!("left {} server {} retry {}", d.reason, d.by_server, d.retry)));
    p.on_stop(|_game, why| {
        *FROZEN.lock().unwrap_or_else(|e| e.into_inner()) = None;
        seen("stop");
        last(format!("stop {why}"));
    });
    p.on_hud(|_game, _draw| seen("hud"));
    p.on_world(|_game, _draw| seen("world"));

    // Priorities: the HIGH handler runs first and cancels "/plugblock", so the LOW one never sees it.
    p.priority(api::LOW).on_tick(|_game| ORDER.lock().unwrap_or_else(|e| e.into_inner()).push('L'));
    p.on_packet_out(|_game, packet| {
        if let api::packets::Serverbound::ChatCommand(c) = packet.packet()
            && c.command.starts_with("plugblock")
        {
            seen("lowsaw");
        }
    });
    p.priority(api::HIGH).on_tick(|_game| ORDER.lock().unwrap_or_else(|e| e.into_inner()).push('H'));
    p.on_packet_out(|_game, packet| {
        if let api::packets::Serverbound::ChatCommand(c) = packet.packet()
            && c.command.starts_with("plugblock")
        {
            seen("blocked");
            packet.cancel();
        }
        // A non-move packet turned into a move packet: refused.
        if REPLACE.swap(false, std::sync::atomic::Ordering::Relaxed) && !packet.is_move() {
            packet.replace(api::packets::Serverbound::MovePlayerStatus(api::packets::MovePlayerStatus { flags: 1 }));
            seen("replaced");
        }
    });
    p.priority(api::NORMAL);

    p.command("plugcheck", "the plugin API's test cases", run);
}

// What the bus did while Sunset was stopped: nothing changed, one stop, the stale task never ran, no task left.
fn watch_result() -> String {
    let frozen = FROZEN.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    let failed = WATCH_FAILED.load(std::sync::atomic::Ordering::Relaxed);
    let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let stops = seen.iter().find(|(n, _)| *n == "stop").map_or(0, |(_, n)| *n);
    let stale = seen.iter().any(|(n, _)| *n == "stale_task");
    format!("plugcheck panic frozen {frozen} changed {failed} stops {stops} stale {stale} tasks {}", api::tasks())
}

fn run(game: &mut Game<'_>, args: &[&str]) {
    match args.first().copied().unwrap_or("") {
        "watch" => {
            *FROZEN.lock().unwrap_or_else(|e| e.into_inner()) = None;
            WATCH_FAILED.store(false, std::sync::atomic::Ordering::Relaxed);
            DORMANT_TICKS.store(0, std::sync::atomic::Ordering::Relaxed);
            WATCH.store(true, std::sync::atomic::Ordering::Relaxed);
            api::after(200, |_game| seen("stale_task"));
        }
        "watch-result" => {
            WATCH.store(false, std::sync::atomic::Ordering::Relaxed);
            api::chat(game, &watch_result());
        }
        "reset" => {
            SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
            ORDER.lock().unwrap_or_else(|e| e.into_inner()).clear();
            // A line of Sunset's own with the chat marker: on_chat leaves it out.
            api::chat(game, "plugcheck reset (plugcheck-chat own line)");
        }
        "events" => {
            let s = SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let text: Vec<String> = s.iter().map(|(n, c)| format!("{n}={c}")).collect();
            let order: String = ORDER.lock().unwrap_or_else(|e| e.into_inner()).chars().take(6).collect();
            let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
            api::chat(game, &format!("plugcheck events {} order {order} last {last}", text.join(" ")));
        }
        "set" => {
            for (m, s, v) in [("Kill Aura", "Range", "4.5"), ("Kil Aura", "Range", "4"), ("Kill Aura", "Rnge", "4"), ("Kill Aura", "Range", "far"), ("Kill Aura", "Range", "99")] {
                let r = api::set(game, m, s, v);
                say(game, r);
            }
            let r = api::set(game, "Plug Check", "Flag", true);
            say(game, r);
            let r = api::set(game, "Kill Aura", "Rotation mode", "Nope");
            say(game, r);
            let range = api::find_module("Kill Aura").map(|m| m.n("Range"));
            api::chat(game, &format!("plugcheck range {range:?} flag {}", api::find_module("plug check").is_some_and(|m| m.b("Flag"))));
        }
        "toggle" => {
            let a = api::toggle(game, "Sprint", true).map(|()| format!("sprint {}", api::is_on("Sprint")));
            say(game, a);
            let b = api::toggle(game, "Sprint", true).map(|()| format!("sprint again {}", api::is_on("Sprint")));
            say(game, b);
            let c = api::toggle(game, "sprint", false).map(|()| format!("sprint off {}", api::is_on("Sprint")));
            say(game, c);
            let d = api::toggle(game, "Nope Module", true).map(|()| String::new());
            say(game, d);
            let n = api::modules().len();
            api::chat(game, &format!("plugcheck modules {n}"));
        }
        "send" => {
            let a = api::send(game, api::packets::Serverbound::MovePlayerStatus(api::packets::MovePlayerStatus { flags: 1 })).map(|()| String::new());
            say(game, a);
            let b = api::send(game, api::packets::Serverbound::SetCarriedItem(api::packets::SetCarriedItem { slot: 0 })).map(|()| String::new());
            say(game, b);
            let c = api::say_server(game, &"x".repeat(300)).map(|()| String::new());
            say(game, c);
            let d = api::say_server(game, "plugcheck-chat from the plugin").map(|()| "said".to_owned());
            say(game, d);
            REPLACE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        "tasks" => {
            let runs = std::sync::Arc::new(Mutex::new((0u32, 0u32)));
            let r = runs.clone();
            api::after(10, move |_game| r.lock().unwrap_or_else(|e| e.into_inner()).0 += 1);
            let r = runs.clone();
            let id = std::sync::Arc::new(Mutex::new(0u64));
            let me = id.clone();
            *id.lock().unwrap_or_else(|e| e.into_inner()) = api::every(5, move |_game| {
                let mut g = r.lock().unwrap_or_else(|e| e.into_inner());
                g.1 += 1;
                if g.1 == 3 {
                    api::cancel(*me.lock().unwrap_or_else(|e| e.into_inner()));
                }
            });
            let gone = api::after(5, |game| api::chat(game, "plugcheck cancelled task ran"));
            let first = api::cancel(gone);
            let second = api::cancel(gone);
            api::after(40, move |game| {
                let (a, e) = *runs.lock().unwrap_or_else(|e| e.into_inner());
                api::chat(game, &format!("plugcheck tasks after {a} every {e} cancel {first} {second}"));
            });
        }
        "stop" => {
            api::after(20, |game| api::chat(game, "plugcheck task after stop ran"));
            api::stop_everything(game, "plugcheck");
            api::chat(game, &format!("plugcheck tasks left {} fullbright {}", api::tasks(), api::is_on("Fullbright")));
        }
        "store" => {
            let n = api::load(game, ID, "runs").and_then(|t| t.parse::<u32>().ok()).unwrap_or(0) + 1;
            let a = api::save(game, ID, "runs", &n.to_string()).map(|()| format!("runs {n}"));
            say(game, a);
            let b = api::save(game, ID, "../escape", "x").map(|()| String::new());
            say(game, b);
            let c = api::save(game, "sunset", "config", "x").map(|()| String::new());
            say(game, c);
            api::chat(game, &format!("plugcheck load {:?} missing {:?}", api::load(game, ID, "runs"), api::load(game, ID, "never-saved")));
        }
        "reads" => {
            let Some(me) = api::me(game) else {
                api::chat(game, "plugcheck no me");
                return;
            };
            let under = [me.pos[0].floor() as i32, me.pos[1].floor() as i32 - 1, me.pos[2].floor() as i32];
            let block = api::block(game, under);
            let item = api::item(game, 0).map_or("none".to_owned(), |i| format!("{}x{}", i.id, i.count));
            let near = api::entities(game).first().map_or("none".to_owned(), |e| format!("{} {:.1}", e.kind, e.distance));
            api::chat(game, &format!(
                "plugcheck me {:.1} {:.1} {:.1} hp {:.0} food {} ground {} slot {} dim {} in_world {} under {block} slot0 {item} count {} find {:?} near {near} players {}",
                me.pos[0], me.pos[1], me.pos[2], me.health, me.food, me.on_ground, me.slot, me.dimension, api::in_world(game),
                api::count(game, "cobblestone,dirt"), api::find_item(game, "dirt"), api::players(game).len()
            ));
        }
        "friends" => {
            api::add_friend(game, "PlugFriend");
            let a = api::is_friend("plugfriend");
            api::add_friend(game, "PlugFriend");
            let n = api::friends().iter().filter(|f| f.eq_ignore_ascii_case("PlugFriend")).count();
            api::remove_friend(game, "PlugFriend");
            api::chat(game, &format!("plugcheck friend {a} once {n} after {}", api::is_friend("PlugFriend")));
        }
        other => api::chat(game, &format!("plugcheck: no case {other}")),
    }
}
