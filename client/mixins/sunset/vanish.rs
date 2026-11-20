// Anti Vanish (Sunset's AntiVanish): a player who leaves the tab list with no chat line about them has likely gone
// into vanish, and one who comes back with none has come out of it. Completion check also asks the server which
// names a command (/msg) completes: a name it completes that the tab list doesn't show is online but hidden; it is said
// once it has come back Confirmations times in a row (a stale completion comes once). Watched players narrows all of
// it to some names; Server watchlists gives a server its own.
//
// What vanish plugins leave in the player info (after Donut Client 3.0's Staff Detector): Spectators, a player the
// server reports in spectator mode (staff watching from the camera); Invisible players, a player entity sent within
// Nearby range while invisible; Unlisted players, one the server keeps in the player info but hides from the tab list.
// Shop and lobby NPCs are unlisted players too, so that check needs a watchlist: it looks only at the names on it.
// Each is said once when it starts.

use super::Game;
use super::sunset::{Category, Module, with};
use horizon_protocol::buf::{Reader, Writer};
use horizon_protocol::packets::play::Serverbound as S;
use super::sunset_staff_check as sc;
use horizon_protocol::packets::play::Clientbound as C;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Anti Vanish", Category::Player, "Reports vanished players.")
            .num("Interval", 100.0, 20.0, 600.0, 10.0)
            .text("Watched players", "")
            .text("Server watchlists", "")
            .bool("Completion check", false)
            .text("Command", "msg")
            .num("Confirmations", 2.0, 1.0, 10.0, 1.0)
            .bool("Spectators", true)
            .bool("Invisible players", true)
            .num("Nearby range", 64.0, 8.0, 256.0, 8.0)
            .bool("Unlisted players", true)
            .group("Staff check")
            .bool("Staff check", false)
            .num("Staff window", 5.0, 1.0, 20.0, 1.0)
            .text("Staff names", "")
            .num("Staff cooldown", 10.0, 0.0, 300.0, 1.0),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    if let Some(shown) = sc::shown(name, m.b("Staff check"), false) {
        return shown;
    }
    match name {
        "Command" | "Confirmations" => m.b("Completion check"),
        "Nearby range" => m.b("Invisible players"),
        _ => true,
    }
}

// Sunset's completion requests: the game numbers its own up from 0.
const ASK: i32 = -0x5355;

#[derive(Default)]
struct Vs {
    // The server joined last (its address as typed), the ticks since the last check, the tab list's names then
    // (None before the first).
    server: String,
    timer: u32,
    tab: Option<Vec<String>>,
    // How many completions in a row have named each hidden name.
    streak: HashMap<String, u32>,
    // The players in spectator mode, invisible near you and unlisted at the last look.
    spectators: Vec<String>,
    invisible: Vec<String>,
    unlisted: Vec<String>,
    staff: sc::Staff,
}

static VS: Mutex<Option<Vs>> = Mutex::new(None);

fn vs<R>(f: impl FnOnce(&mut Vs) -> R) -> R {
    let mut g = VS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Vs::default))
}

// A connection starts: everything known is the last server's.
pub(super) fn connecting(address: &str) {
    vs(|v| {
        *v = Vs {
            server: address.to_owned(),
            ..Vs::default()
        }
    });
}

// An address without its port, for comparing.
fn host(address: &str) -> &str {
    let address = address.trim();
    match address.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => address,
    }
}

// The names the module tells you about here: the server's own watchlist if it has one, else Watched players (empty:
// everyone).
fn watchlist<'a>(m: &'a Module, server: &str) -> &'a str {
    m.t("Server watchlists")
        .split(';')
        .filter_map(|entry| entry.split_once('='))
        .find(|(s, _)| host(s).eq_ignore_ascii_case(host(server)))
        .map_or(m.t("Watched players"), |(_, names)| names)
}

fn watched(m: &Module, server: &str, name: &str) -> bool {
    let list = watchlist(m, server);
    list.trim().is_empty() || list.split(',').any(|n| n.trim().eq_ignore_ascii_case(name))
}

// A name a player can have: up to 16 letters, digits and underscores (NPCs often have others).
fn player_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    staff_tick(game, m);
    let Some(m) = m.filter(|_| game.remote.is_some()) else {
        vs(|v| {
            v.timer = 0;
            v.tab = None;
            v.streak.clear();
            v.spectators.clear();
            v.invisible.clear();
            v.unlisted.clear();
        });
        return;
    };
    if game.ticks % 10 == 0 {
        presence(game, m);
    }
    let due = vs(|v| {
        v.timer += 1;
        v.timer >= m.n("Interval") as u32
    });
    if !due {
        return;
    }
    let now = super::remote::sunset_remote::tab_names(game);
    // A leave or join message names the player: the newest chat lines are where it would be.
    let chat = game.remote_chat(20).unwrap_or_default();
    let quiet = |name: &str| !chat.iter().any(|line| line.contains(name));
    let (server, before) = vs(|v| {
        v.timer = 0;
        (v.server.clone(), v.tab.replace(now.clone()))
    });
    let mut lines = Vec::new();
    for name in before.iter().flatten().filter(|n| !now.contains(n)) {
        if watched(m, &server, name) && quiet(name) {
            lines.push(format!("{name} left the tab list with no leave message: gone into vanish?"));
        }
    }
    for name in now.iter().filter(|n| before.as_ref().is_some_and(|b| !b.contains(n))) {
        if watched(m, &server, name) && quiet(name) {
            lines.push(format!("{name} came back to the tab list with no join message: out of vanish?"));
        }
    }
    if m.b("Completion check") {
        let mut w = Writer::new();
        w.varint(ASK);
        w.string(&format!("/{} ", m.t("Command").trim().trim_start_matches('/')));
        game.send(S::Unknown(horizon_protocol::ids::play::serverbound::COMMAND_SUGGESTION, w.data));
    }
    for line in lines {
        super::sunset_util::report(game, &line);
    }
}

// Spectators, Invisible players and Unlisted players: who each shows now, and a line for each new one.
fn presence(game: &mut Game<'_>, m: &Module) {
    let server = vs(|v| v.server.clone());
    let info = super::remote::sunset_remote::tab_presence(game);
    let spectators: Vec<String> = if m.b("Spectators") {
        info.iter().filter(|(n, mode, _)| *mode == 3 && watched(m, &server, n)).map(|(n, ..)| n.clone()).collect()
    } else {
        Vec::new()
    };
    let unlisted: Vec<String> = if m.b("Unlisted players") && !watchlist(m, &server).trim().is_empty() {
        info.iter().filter(|(n, _, listed)| !listed && player_name(n) && watched(m, &server, n)).map(|(n, ..)| n.clone()).collect()
    } else {
        Vec::new()
    };
    let mut invisible: Vec<(String, f64)> = Vec::new();
    if m.b("Invisible players") && let Some(here) = super::sunset_util::player_pos(game) {
        let me = super::sunset::own_id(game);
        for mob in game.mobs.iter().filter(|x| x.kind.id == "minecraft:player" && x.invisible && x.alive() && !x.removed && Some(x.id) != me) {
            let d = super::sunset_util::dist(here, [mob.position.x, mob.position.y, mob.position.z]);
            if d <= m.n("Nearby range") && let Some(name) = game.remote_player_name(mob.id).filter(|n| watched(m, &server, n)) {
                invisible.push((name.to_owned(), d));
            }
        }
    }
    let lines = vs(|v| {
        let mut lines = Vec::new();
        for n in spectators.iter().filter(|n| !v.spectators.contains(n)) {
            lines.push(format!("{n} is in spectator mode: watching?"));
        }
        for n in unlisted.iter().filter(|n| !v.unlisted.contains(n)) {
            lines.push(format!("{n} is online but kept out of the tab list: in vanish?"));
        }
        for (n, d) in invisible.iter().filter(|(n, _)| !v.invisible.contains(n)) {
            lines.push(format!("{n} is invisible {d:.0} m away"));
        }
        v.spectators = spectators;
        v.unlisted = unlisted;
        v.invisible = invisible.into_iter().map(|(n, _)| n).collect();
        lines
    });
    for line in lines {
        super::sunset_util::report(game, &line);
    }
}

// The server's completions for Sunset's request (true: the game never sees them): names it completes that the tab
// list doesn't show, each told once it has come back Confirmations times in a row, and once only while it stays hidden.
pub(super) fn completions(game: &mut Game<'_>, data: &[u8]) -> bool {
    let mut r = Reader::new(data);
    if r.varint().ok() != Some(ASK) {
        return false;
    }
    let names = (|| {
        r.varint().ok()?;
        r.varint().ok()?;
        let n = r.count(4096).ok()?;
        let mut names = Vec::with_capacity(n);
        for _ in 0..n {
            names.push(r.string(256).ok()?);
            if r.bool().ok()? {
                r.nbt().ok()?;
            }
        }
        Some(names)
    })()
    .unwrap_or_default();
    let tab = super::remote::sunset_remote::tab_names(game);
    let own = game.options.player_name();
    let Some(m) = with(|s| s.module("Anti Vanish").filter(|m| m.on).cloned()) else {
        return true;
    };
    let hidden: Vec<String> = names
        .into_iter()
        .filter(|n| n.len() <= 16 && !n.contains(' ') && *n != own && !tab.contains(n))
        .collect();
    let need = m.n("Confirmations").max(1.0) as u32;
    let (server, new) = vs(|v| {
        v.streak.retain(|n, _| hidden.contains(n));
        let mut new = Vec::new();
        for n in &hidden {
            let streak = v.streak.entry(n.clone()).or_default();
            *streak += 1;
            if *streak == need {
                new.push(n.clone());
            }
        }
        (v.server.clone(), new)
    });
    for name in new.iter().filter(|n| watched(&m, &server, n)) {
        super::sunset_util::report(game, &format!("{name} is online but not in the tab list: in vanish?"));
    }
    true
}

// ---------------------------------------------------------------- Staff check
// A push for you (SetEntityMotion) that nothing explains: see staff_check.rs. The packets come as copies before the
// game (and Velocity, Anti Knockback or Knockback Delay) handles them; the alert waits Staff window ticks for a late
// hit. In HorizonSMP it goes out as that mode's notifications do (Horizon notifications, Anonymous notifications,
// Notify interval for the cooldown).

static STAFF_ON: AtomicBool = AtomicBool::new(false);

// What was around you just now that would push you: attackers, projectiles and hooks, riding, own motion.
fn surroundings(game: &Game<'_>) -> sc::Around {
    let me = super::sunset::own_id(game);
    let (attacker, projectile) = super::sunset_util::player_pos(game).map_or((false, false), |here| {
        let others = game.mobs.iter().filter(|x| x.alive() && !x.removed && Some(x.id) != me);
        let near = others.map(|x| (x.kind.id.as_str(), super::sunset_util::dist(here, [x.position.x, x.position.y, x.position.z])));
        // Single player has no own id here; a player where you stand is you.
        sc::entities(near.filter(|(kind, d)| !(*kind == "minecraft:player" && *d < 0.3)))
    });
    let own = game.player.as_ref().is_some_and(|p| p.fall_flying || p.auto_spin_attack_ticks > 0)
        || game.potions.effects.get("levitation").is_some();
    sc::Around { attacker, projectile, riding: game.riding.is_some(), own, ..sc::Around::default() }
}

// Each packet Sunset reads (a copy, at its next tick).
pub(super) fn staff_packet(game: &Game<'_>, packet: &C) {
    if !STAFF_ON.load(Ordering::Relaxed) {
        return;
    }
    let me = game.remote.as_ref().map(|r| r.entity_id);
    let now = game.ticks;
    match packet {
        C::SetEntityMotion(p) if Some(p.id) == me => {
            let around = surroundings(game);
            vs(|v| v.staff.push(sc::Push { tick: now, velocity: p.velocity, around }));
        }
        C::DamageEvent(p) if Some(p.id) == me => vs(|v| v.staff.event(sc::Event::Hurt, now)),
        C::SetHealth(p) => vs(|v| v.staff.health(p.health, now)),
        C::Explode(x) => {
            let near = super::sunset_util::player_pos(game).is_some_and(|h| super::sunset_util::dist(h, x.center) <= sc::BLAST_RANGE);
            if x.knockback.is_some() || near {
                vs(|v| v.staff.event(sc::Event::Blast, now));
            }
        }
        C::Login(_) | C::Respawn(_) | C::PlayerPosition(_) => vs(|v| v.staff.event(sc::Event::Teleport, now)),
        _ => {}
    }
}

fn staff_tick(game: &mut Game<'_>, m: Option<&Module>) {
    let m = m.filter(|m| m.b("Staff check"));
    STAFF_ON.store(m.is_some(), Ordering::Relaxed);
    let Some(m) = m else {
        vs(|v| v.staff.clear());
        return;
    };
    let now = game.ticks;
    let judged = vs(|v| v.staff.settle(now, m.n("Staff window").max(0.0) as u64));
    let Some(push) = judged.iter().filter(|(_, c)| c.is_none()).map(|(p, _)| *p).max_by(|a, b| a.across().total_cmp(&b.across())) else {
        return;
    };
    let horizon = m.p("Mode") == "HorizonSMP";
    if horizon && !m.b("Horizon notifications") {
        vs(|v| v.staff.held[2] += 1);
        return;
    }
    // Who is on: the player info (vanished staff are often kept there unlisted) and the players near.
    let info = super::remote::sunset_remote::tab_presence(game);
    let near: Vec<String> = game.mobs.iter().filter(|x| x.kind.id == "minecraft:player")
        .filter_map(|x| game.remote_player_name(x.id).map(str::to_owned)).collect();
    let on = |n: &str| info.iter().any(|(i, ..)| i.eq_ignore_ascii_case(n)) || near.iter().any(|i| i.eq_ignore_ascii_case(n));
    let wanted = sc::names(m.t("Staff names"));
    let about: Vec<String> = wanted.iter().filter(|n| on(n)).map(|n| (*n).to_owned()).collect();
    if !wanted.is_empty() && about.is_empty() {
        vs(|v| v.staff.held[0] += 1);
        return;
    }
    let cooldown = if horizon { m.n("Notify interval") } else { m.n("Staff cooldown") };
    if !vs(|v| v.staff.may_alert(now, (cooldown.max(0.0) * 20.0) as u64)) {
        return;
    }
    let mut likely: Vec<String> = info.iter().filter(|(_, mode, _)| *mode == 3).map(|(n, ..)| n.clone()).collect();
    for n in about {
        if !likely.iter().any(|l| l.eq_ignore_ascii_case(&n)) {
            likely.push(n);
        }
    }
    let mut line = format!(
        "Anti Vanish: pushed {:.2} across, {:+.2} up with no hit, explosion, attacker, hook or own motion: a staff knockback test?",
        push.across(),
        push.velocity[1]
    );
    if !likely.is_empty() {
        if horizon && m.b("Anonymous notifications") {
            line.push_str(&format!(" {} spectating or on Staff names", likely.len()));
        } else {
            line.push_str(&format!(" Likely: {}", likely.join(", ")));
        }
    }
    super::sunset_util::report(game, &line);
}

// .staffcheck: what explained your pushes so far; .staffcheck test [x y z]: a push as if the server sent one now.
pub(super) fn staff_command(game: &mut Game<'_>, args: &[&str]) {
    let on = with(|s| s.module("Anti Vanish").is_some_and(|m| m.on && m.b("Staff check")));
    if args.first().is_some_and(|a| a.eq_ignore_ascii_case("test")) {
        if !on {
            super::sunset::say(game, "Turn on Anti Vanish and its Staff check first.");
            return;
        }
        let v: Vec<f64> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
        let velocity = if v.len() == 3 { [v[0], v[1], v[2]] } else { [0.4, 0.36, 0.0] };
        let around = surroundings(game);
        let tick = game.ticks;
        let taken = vs(|v| v.staff.push(sc::Push { tick, velocity, around }));
        let line = match (taken, around.cause()) {
            (false, _) => "Staff check: that push is too small to count.".to_owned(),
            (true, Some(c)) => format!("Staff check: test push sent ({} now explains it).", sc::CAUSES[c]),
            (true, None) => "Staff check: test push sent.".to_owned(),
        };
        super::sunset::say(game, &line);
        return;
    }
    let (counts, alerts, held) = vs(|v| (v.staff.counts, v.staff.alerts, v.staff.held));
    let total: u32 = counts.iter().sum();
    let parts: Vec<String> = counts.iter().zip(sc::CAUSES).filter(|(n, _)| **n > 0).map(|(n, c)| format!("{c} {n}")).collect();
    let state = if on { "on" } else { "off" };
    // The rows the menu shows for it now (they change with Staff check and Mode).
    let menu: Vec<&str> = with(|s| {
        s.module("Anti Vanish").map_or_else(Vec::new, |m| {
            m.settings.iter().enumerate().filter(|(j, x)| x.name.starts_with("Staff ") && m.setting_visible(*j)).map(|(_, x)| x.name).collect()
        })
    });
    let parts = if parts.is_empty() { String::new() } else { format!(": {}", parts.join(", ")) };
    let held: Vec<String> = held.iter().zip(sc::HELD).filter(|(n, _)| **n > 0).map(|(n, h)| format!("{h} {n}")).collect();
    let held = if held.is_empty() { String::new() } else { format!(", held back: {}", held.join(", ")) };
    super::sunset::say(game, &format!("Staff check ({state}; menu: {}): {total} pushes judged{parts}; alerts {alerts}{held}", menu.join(", ")));
}

#[cfg(test)]
mod tests {
    use super::player_name;

    #[test]
    fn only_real_player_names_count_as_unlisted() {
        assert!(player_name("Notch") && player_name("a_b_9"));
        assert!(!player_name("") && !player_name("CIT-1a2b3c4d") && !player_name("§6Shop") && !player_name("seventeen_chars_x"));
    }
}
