// Anti Vanish (Meteor's AntiVanish): a player who leaves the tab list with no chat line about them has likely gone
// into vanish, and one who comes back with none has come out of it. Completion check also asks the server which
// names a command (/msg) completes: a name it completes that the tab list doesn't show is online but hidden.
// Watched players narrows all of it to some names; Server watchlists gives a server its own.

use super::Game;
use super::sunset::{Category, Module, with};
use horizon_protocol::buf::{Reader, Writer};
use horizon_protocol::packets::play::Serverbound as S;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Anti Vanish", Category::Player, "Says when a player leaves the tab list with no leave message (gone into vanish) or comes back with no join message. Completion check also asks the server whom its /msg completes: one the tab list hides shows up. Watched players: only these (everyone when empty); Server watchlists: a server's own (play.example.net = Steve, Alex; other.net = Bob).")
            .num("Interval", 100.0, 20.0, 600.0, 10.0)
            .text("Watched players", "")
            .text("Server watchlists", "")
            .bool("Completion check", false)
            .text("Command", "msg"),
    ]
}

// Sunset's completion requests: the game numbers its own up from 0.
const ASK: i32 = -0x5355;

#[derive(Default)]
struct Vs {
    // The server joined last (its address as typed), the ticks since the last check, the tab list's names then
    // (None before the first) and the hidden names the completions showed last.
    server: String,
    timer: u32,
    tab: Option<Vec<String>>,
    hidden: Vec<String>,
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

// Whether the module tells you about `name` here: the server's own watchlist if it has one, else Watched players
// (everyone when empty).
fn watched(m: &Module, server: &str, name: &str) -> bool {
    let list = m
        .t("Server watchlists")
        .split(';')
        .filter_map(|entry| entry.split_once('='))
        .find(|(s, _)| host(s).eq_ignore_ascii_case(host(server)))
        .map_or(m.t("Watched players"), |(_, names)| names);
    list.trim().is_empty() || list.split(',').any(|n| n.trim().eq_ignore_ascii_case(name))
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| game.remote.is_some()) else {
        vs(|v| {
            v.timer = 0;
            v.tab = None;
            v.hidden.clear();
        });
        return;
    };
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

// The server's completions for Sunset's request (true: the game never sees them): names it completes that the tab
// list doesn't show, each told once while it stays hidden.
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
    let (server, new) = vs(|v| {
        let new: Vec<String> = hidden.iter().filter(|n| !v.hidden.contains(n)).cloned().collect();
        v.hidden = hidden;
        (v.server.clone(), new)
    });
    for name in new.iter().filter(|n| watched(&m, &server, n)) {
        super::sunset_util::report(game, &format!("{name} is online but not in the tab list: in vanish?"));
    }
    true
}
