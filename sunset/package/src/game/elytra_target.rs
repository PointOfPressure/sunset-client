// Elytra Target (Combat), with Kill Aura: while you glide, it picks a player in sight with Kill Aura's entity filters
// and priority (Kill Aura's own target first) and steers your view at them: where a gliding target will be in Prediction
// ticks, at the eyes or the body, holding back at Ideal distance until Kill Aura's hit is due, then straight in. Each
// tick's turn is capped (Yaw/Pitch speed), in whole mouse counts, and is your real view: the game's glide physics and
// its one move packet carry the same look, so nothing but the look steers (no velocity is added) and an unchanged
// look is not sent again. Override Kill Aura: Kill Aura leaves the look to it while it steers and hits only when that
// look (or the one the server has) meets the target. Safe tips the look up when the next tick's box would hit a block.
// Auto firework: a rocket from the offhand or the hotbar when the target is beyond Firework distance, at the start of
// a tick with no hit or slot change in it (one slot change a tick, the slot back the next), with Sync only just after
// Kill Aura's hit. It never starts or stops a glide itself.
use super::Game;
use super::sunset::{Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_aura_aim as steer;
use super::sunset_util::{self as util, OFFHAND, Target, dist};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;

pub(super) const NAME: &str = "Elytra Target";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Steers your glide at a player for Kill Aura's hits, with fireworks.")
            .num("Range", 64.0, 8.0, 128.0, 4.0)
            .pick("Aim point", &["Eyes", "Body"])
            .num("Ideal distance", 10.0, 0.0, 30.0, 1.0)
            .bool("Prediction", true)
            .num("Prediction ticks", 1.9, 0.5, 2.0, 0.1)
            .bool("Gliding targets only", true)
            .bool("Gravity drop", false)
            .num("Yaw speed", 45.0, 5.0, 90.0, 5.0)
            .num("Pitch speed", 35.0, 5.0, 90.0, 5.0)
            .bool("Override Kill Aura", true)
            .bool("Safe", true)
            .bool("Auto firework", true)
            .num("Firework distance", 16.0, 3.0, 100.0, 1.0)
            .num("Firework delay", 10.0, 1.0, 50.0, 1.0)
            .bool("Sync with Kill Aura", true),
    ]
}

#[derive(Default)]
struct State {
    target: Option<u32>,
    track: steer::Track,
    // The target's distance from your eyes, last tick.
    distance: f64,
    // The look was set this tick for Kill Aura to leave alone and hit by.
    steering: bool,
    last_firework: Option<u64>,
    // The hotbar slot from before a rocket was held (it goes back next tick), with the hand leased meanwhile.
    slot_back: Option<usize>,
    fireworks: u32,
    turns: u32,
    // Ticks Kill Aura was handed the look to hit by.
    steered: u32,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

// Elytra Target has the look this tick and Kill Aura should hit by it rather than turn (Override Kill Aura).
pub(super) fn steering() -> bool {
    st(|s| s.steering)
}

fn gliding(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| p.fall_flying && p.health.health() > 0.0)
        && game.riding.is_none()
        && !game.spectator()
        && util::in_world(game)
}

// A player in sight within Range whom Kill Aura's filters allow: Kill Aura's current target if it is one, else the
// first by Kill Aura's Priority (nearest for Smart, which needs Kill Aura's own look).
fn pick(game: &Game<'_>, m: &Module, eye: [f64; 3], held: Option<u32>) -> Option<Target> {
    let aura = with(|s| s.module("Kill Aura").cloned())?;
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let range = m.n("Range");
    let mut list: Vec<Target> = util::every_living(game)
        .into_iter()
        .filter(|t| {
            t.player
                && Some(t.id) != own
                && dist(eye, steer::aim_point(t.lo, t.hi, false)) <= range
                && game.mobs.iter().find(|mob| mob.id == t.id).is_some_and(|mob| super::sunset_aura::target_filters(game, &aura, mob, t))
                && super::sunset_aura::in_sight(game, t)
        })
        .collect();
    let first = super::sunset_aura::target_id().or(held);
    if let Some(i) = first.and_then(|id| list.iter().position(|t| t.id == id)) {
        return Some(list.swap_remove(i));
    }
    util::sort_targets(game, &mut list, aura.p("Priority"));
    list.into_iter().next()
}

// Would your box, moved by this tick's velocity (ahead and down, not up into a ceiling), touch a block?
fn collides_next(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let b = p.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let v = [p.velocity.x, p.velocity.y.min(0.0), p.velocity.z];
    let f = |a: f64| a.floor() as i32;
    for x in f(lo.x + v[0])..=f(hi.x + v[0]) {
        for y in f(lo.y + v[1])..=f(hi.y + v[1]) {
            for z in f(lo.z + v[2])..=f(hi.z + v[2]) {
                if !util::replaceable(game, [x, y, z]) {
                    return true;
                }
            }
        }
    }
    false
}

fn drop_target() {
    st(|s| {
        s.target = None;
        s.steering = false;
    });
}

// After the move packet (before Kill Aura): the look for the next tick's glide and move packet.
pub(super) fn post_tick(game: &mut Game<'_>) {
    st(|s| s.steering = false);
    let Some(m) = module() else {
        drop_target();
        return;
    };
    if !gliding(game) || game.remote.is_none() {
        drop_target();
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let held = st(|s| s.target);
    let Some(t) = pick(game, &m, eye, held) else {
        drop_target();
        return;
    };
    let aura = with(|s| s.module("Kill Aura").filter(|k| k.on).cloned());
    let distance = dist(eye, steer::aim_point(t.lo, t.hi, false));
    // Where the server last put it (before the client's smoothing) is what its velocity is read from.
    let at = super::remote::sunset_combat_remote::entity_position(game, t.id).unwrap_or(t.pos);
    let target_gliding = game.mobs.iter().any(|mob| mob.id == t.id && mob.extra.fall_flying);
    let ahead = steer::Ahead {
        on: m.b("Prediction"),
        ticks: m.n("Prediction ticks"),
        gliding_only: m.b("Gliding targets only"),
        gravity: m.b("Gravity drop"),
    };
    let shift = st(|s| {
        s.target = Some(t.id);
        s.distance = distance;
        s.track.observe(t.id, at);
        steer::predicted(&s.track, at, target_gliding, ahead)
    });
    // Override off: Kill Aura turns while it has someone (its own rotations), Elytra Target steers the rest of the way.
    if !m.b("Override Kill Aura") && aura.is_some() && super::sunset_aura::engaged() {
        return;
    }
    // The box where the server has it, moved on by the prediction.
    let point = steer::aim_point(t.lo, t.hi, m.p("Aim point") == "Eyes");
    let point: [f64; 3] = std::array::from_fn(|i| point[i] + at[i] - t.pos[i] + shift[i]);
    // Your eyes where this tick's motion takes them.
    let eye_next = game.player.as_ref().map_or(eye, |p| [eye[0] + p.velocity.x, eye[1] + p.velocity.y, eye[2] + p.velocity.z]);
    // Close in when Kill Aura's hit is due (or there is no Kill Aura to wait for).
    let due = aura.is_none() || game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 0.8;
    let hold_up = steer::hold_pitch(distance, m.n("Ideal distance"), due);
    let colliding = m.b("Safe") && collides_next(game);
    let to = steer::wanted(eye_next, point, hold_up, colliding);
    let from = (game.yaw, game.pitch);
    let count = super::sunset_rotation::degrees_per_count(f64::from(game.settings.sensitivity));
    let next = steer::step(from, to, (m.n("Yaw speed") as f32, m.n("Pitch speed") as f32), count);
    if next != from {
        (game.yaw, game.pitch) = next;
        st(|s| s.turns = s.turns.wrapping_add(1));
    }
    st(|s| {
        s.steering = m.b("Override Kill Aura") && aura.is_some();
        s.steered = s.steered.wrapping_add(u32::from(s.steering));
    });
}

fn rocket(game: &Game<'_>, slot: usize) -> bool {
    game.stack_in(slot).is_some_and(|s| util::short(&s.item) == "firework_rocket" && s.count > 0)
}

// The start of a tick, before its move packet: a borrowed slot back, else a rocket if one is due. `busy`: a hit or a
// slot change went this tick already.
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    if let Some(back) = st(|s| s.slot_back) {
        if busy {
            return;
        }
        if util::in_world(game) && game.selected != back && back < 9 {
            hold(game, back);
        }
        st(|s| s.slot_back = None);
        control::release(NAME);
        return;
    }
    let Some(m) = module().filter(|m| m.b("Auto firework")) else {
        return;
    };
    let Some(_) = st(|s| s.target) else {
        return;
    };
    if busy || !gliding(game) || game.using.is_some() || game.mining.is_some() || !util::inventory_idle(game) {
        return;
    }
    let now = game.ticks;
    let engaged = with(|s| s.is_on("Kill Aura")) && super::sunset_aura::engaged();
    let wait = m.b("Sync with Kill Aura") && steer::sync_wait(engaged, now, super::sunset_aura::last_hit_tick());
    let (distance, last) = st(|s| (s.distance, s.last_firework));
    if !steer::firework_due(distance, m.n("Firework distance"), now, last, m.n("Firework delay") as u64, wait) {
        return;
    }
    let Some(slot) = std::iter::once(OFFHAND).chain(std::iter::once(game.selected)).chain(0..9).find(|&s| rocket(game, s)) else {
        return;
    };
    if slot != OFFHAND && slot != game.selected {
        if !control::claim(NAME) {
            return;
        }
        let before = game.selected;
        st(|s| s.slot_back = Some(before));
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    let yaw = super::sunset::wire_yaw(game, game.yaw);
    let pitch = game.pitch;
    game.send(S::UseItem(play::UseItem { hand: i32::from(slot == OFFHAND), sequence, yaw, pitch }));
    game.swing_arm();
    st(|s| {
        s.last_firework = Some(now);
        s.fireworks = s.fireworks.wrapping_add(1);
    });
}

// `.elytratarget`: what it is doing, for checks.
pub(super) fn diagnostics() -> String {
    st(|s| {
        format!(
            "Elytra Target: target {:?}, distance {:.1}, steering {}, turns {}, steered {}, fireworks {}, slot back {:?}",
            s.target, s.distance, s.steering, s.turns, s.steered, s.fireworks, s.slot_back
        )
    })
}
