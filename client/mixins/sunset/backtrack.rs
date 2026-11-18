// Backtrack (Combat, once a part of Fake Lag): the player you last hit, while within its range band (and with a weapon
// in hand, with Weapon only), is told to the network module each tick, which holds what the server sends while they
// move away from you (horizon_client::net::sunset_packets). It owns its own state: which player that is, for which
// connection and life, so an entity id from another world or session never becomes the target.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module};
use horizon_client::net::sunset_packets::{self as net, Link, Linked};
use horizon_protocol::packets::play::Serverbound as S;
use std::fmt::Write as _;
use std::sync::Mutex;

pub(super) const NAME: &str = "Backtrack";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Holds what the server sends while the player you hit moves away.")
            .num("Min range", 1.0, 0.0, 10.0, 0.5)
            .num("Max range", 5.0, 0.0, 10.0, 0.5)
            .num("Shortest latency (ms)", 100.0, 0.0, 1500.0, 10.0)
            .num("Longest latency (ms)", 150.0, 0.0, 1500.0, 10.0)
            .bool("Weapon only", false)
            .bool("Show server position", true),
    ]
}

// The player you last hit, as it went on the wire, with the connection and life it was hit in.
struct Hit {
    id: u32,
    link: Link,
    life: (i32, super::Dim),
}

static LAST_HIT: Mutex<Option<Hit>> = Mutex::new(None);

fn last_hit<R>(f: impl FnOnce(&mut Option<Hit>) -> R) -> R {
    f(&mut LAST_HIT.lock().unwrap_or_else(|e| e.into_inner()))
}

// Every packet that went on the wire (sunset_combat_wire::delivered).
pub(super) fn sent(game: &Game<'_>, packet: &S) {
    if let (S::Attack(p), Some(r)) = (packet, &game.remote)
        && let Ok(id) = u32::try_from(p.id)
    {
        last_hit(|h| *h = Some(Hit { id, link: Link::of(&r.session), life: (r.entity_id, game.dim) }));
    }
}

// The last hit's player, if it was hit in this connection and life.
fn hit_target(game: &Game<'_>) -> Option<u32> {
    let r = game.remote.as_ref()?;
    last_hit(|h| {
        let current = h.as_ref().is_some_and(|h| h.link.to(Some(&r.session)) == Linked::Same && h.life == (r.entity_id, game.dim));
        if !current {
            *h = None;
        }
        h.as_ref().map(|h| h.id)
    })
}

pub(super) fn tick(game: &Game<'_>, mods: &[Module]) {
    let live = game.remote.is_some()
        && game.screen == super::Screen::Playing
        && !super::sunset::gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    let m = mods.iter().find(|m| m.name == NAME && m.on).filter(|_| live);
    let target = m.and_then(|m| {
        if m.b("Weapon only") && !holds_weapon(game) {
            return None;
        }
        let id = hit_target(game)?;
        let mob = game.mobs.iter().find(|t| t.id == id && super::sunset_util::player_enemy(game, t))?;
        let at = [mob.position.x, mob.position.y, mob.position.z];
        let d = super::sunset_util::dist(super::sunset_util::player_pos(game)?, at);
        (d >= m.n("Min range") && d <= m.n("Max range")).then_some((m, mob.id, at))
    });
    match (target, game.remote.as_ref(), game.eye_position()) {
        (Some((m, id, at)), Some(r), Some(eye)) => {
            let (a, b) = (m.n("Shortest latency (ms)"), m.n("Longest latency (ms)"));
            net::backtrack(r.entity_id, id as i32, at, eye, [a.min(b).max(0.0) as u64, a.max(b).max(0.0) as u64]);
        }
        _ => net::backtrack_off(),
    }
}

pub(super) fn shutdown() {
    last_hit(|h| *h = None);
    net::backtrack_off();
}

fn holds_weapon(game: &Game<'_>) -> bool {
    game.held().is_some_and(|s| {
        &*s.item == "minecraft:mace"
            || ["minecraft:swords", "minecraft:axes"].iter().any(|t| game.res.items().item_tags.get(*t).is_some_and(|t| t.contains(&*s.item)))
    })
}

// Show server position: where the player really is while Backtrack holds what came in.
pub(super) fn draw(cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    if !mods.iter().any(|m| m.name == NAME && m.on && m.b("Show server position")) {
        return;
    }
    if let Some(at) = net::backtrack_shown() {
        let lo = [at[0] - 0.3, at[1], at[2] - 0.3];
        let hi = [at[0] + 0.3, at[1] + 1.8, at[2] + 0.3];
        super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFF55_AAFF, lw);
    }
}

// Debug Report: what Backtrack and Knockback Delay held of what came in.
pub(super) fn report(out: &mut String) {
    let _ = writeln!(out, "\n## Incoming delays (Backtrack, Knockback Delay)");
    for (name, stats) in [("Backtrack", net::backtrack_stats()), ("Knockback Delay", net::delay_stats())] {
        let Some(s) = stats else {
            let _ = writeln!(out, "- {name}: not used this game");
            continue;
        };
        let flushed: Vec<String> = net::FLUSH_REASONS
            .iter()
            .zip(s.flushed)
            .filter(|(_, n)| *n > 0)
            .map(|(why, n)| format!("{why} {n}"))
            .collect();
        let _ = writeln!(
            out,
            "- {name}: {} held now (deepest {}); {} held, {} released since start; let go early by {}; thrown away: {} of another session, {} of an ended life{}",
            s.depth,
            s.max_depth,
            s.held,
            s.released,
            if flushed.is_empty() { "nothing".into() } else { flushed.join(", ") },
            s.discarded_session,
            s.discarded_life,
            if name == "Backtrack" { format!("; {} target switches", s.target_switches) } else { String::new() },
        );
    }
}
