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
use std::collections::HashMap;
use std::sync::Mutex;

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
            .bool("Unlisted players", true),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::player_name;

    #[test]
    fn only_real_player_names_count_as_unlisted() {
        assert!(player_name("Notch") && player_name("a_b_9"));
        assert!(!player_name("") && !player_name("CIT-1a2b3c4d") && !player_name("§6Shop") && !player_name("seventeen_chars_x"));
    }
}
