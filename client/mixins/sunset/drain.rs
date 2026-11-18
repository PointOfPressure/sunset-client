// Liquid Filler's Drain mode: with an enemy player within 10 blocks, still water or lava taken away. An empty bucket
// scoops it (Legit: the source under your crosshair; Silent aim: the nearest within reach and in view, the look
// turned to it on the wire first, at most Silent aim FOV off your yaw); with no bucket, a cobweb goes in it (Fallback
// to cobweb). Never a source you put down yourself. Drain swap brings a bucket into your hand first (else one must
// be held), Swap back returns your slot after. One step a tick at most, Delay apart, before the tick's move packet.
use super::Game;
use super::sunset::{Module, hold, look_at, turn, with};
use super::sunset_util::{self as util, dist};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;

const OWNER: &str = "Liquid Filler";
const ENEMY_RANGE: f64 = 10.0;

#[derive(Default)]
struct Drain {
    // Sources you put down (and the tick: kept 2 s at least, as the server's water can come a tick or more after the
    // click), and those scooped or webbed already (both forgotten once no liquid is left there).
    own: Vec<([i32; 3], u64)>,
    done: Vec<[i32; 3]>,
    // The slot from before a Drain swap; whether a scoop went since.
    back: Option<usize>,
    used: bool,
    next: u64,
}

static DRAIN: Mutex<Option<Drain>> = Mutex::new(None);

pub(super) fn shutdown(game: &mut Game<'_>) {
    if let Some(back) = st(|s| s.back.take()) {
        if game.player.is_some() && game.selected != back { hold(game, back); }
    }
    st(|s| *s = Drain::default());
    super::sunset_combat_control::release(OWNER);
}

fn st<R>(f: impl FnOnce(&mut Drain) -> R) -> R {
    f(DRAIN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Drain::default))
}

fn liquid(game: &Game<'_>, p: [i32; 3]) -> Option<&'static str> {
    match util::short(&util::facts(game, p).name) {
        "water" => Some("water"),
        "lava" => Some("lava"),
        _ => None,
    }
}

fn still(game: &Game<'_>, p: [i32; 3]) -> bool {
    let b = game.world.block(p[0], p[1], p[2]);
    game.pack.assets.registry.state(b).and_then(|s| s.properties.get("level")).is_some_and(|l| l == "0")
}

// A source Drain may take: the kind Place in names (only one by a cobweb, with Only by cobwebs), not yours, not done.
fn wanted(game: &Game<'_>, m: &Module, p: [i32; 3]) -> bool {
    let Some(kind) = liquid(game, p) else {
        return false;
    };
    let kind_ok = match m.p("Place in") {
        "Water" => kind == "water",
        "Lava" => kind == "lava",
        _ => true,
    };
    let by_web = || {
        [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]]
            .iter()
            .any(|d| util::short(&util::facts(game, [p[0] + d[0], p[1] + d[1], p[2] + d[2]]).name) == "cobweb")
    };
    kind_ok && still(game, p) && st(|s| !s.own.iter().any(|o| o.0 == p) && !s.done.contains(&p)) && (!m.b("Only by cobwebs") || by_web())
}

// Your bucket put water or lava down: that spot is yours.
pub(super) fn sent(game: &Game<'_>, packet: &S) {
    let S::UseItemOn(u) = packet else {
        return;
    };
    let stack = if u.hand == 1 { game.offhand.as_ref() } else { game.held() };
    if !stack.is_some_and(|s| matches!(util::short(&s.item), "water_bucket" | "lava_bucket")) {
        return;
    }
    let d = match u.face {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    };
    let at = [u.pos[0] + d[0], u.pos[1] + d[1], u.pos[2] + d[2]];
    let now = game.ticks;
    st(|s| {
        s.own.retain(|p| p.0 != at);
        s.own.push((at, now));
        if s.own.len() > 256 {
            s.own.remove(0);
        }
    });
}

fn enemy_near(game: &Game<'_>) -> bool {
    let Some(me) = util::player_pos(game) else {
        return false;
    };
    let own = super::sunset::own_id(game);
    let friends = with(|s| s.friends.clone());
    let names = game.remote_player_names();
    game.mobs.iter().any(|mob| {
        mob.kind.id == "minecraft:player"
            && Some(mob.id) != own
            && mob.health > 0.0
            && !super::remote::sunset_radar_players::spectator(game, mob.id)
            && !super::sunset::is_friend(&friends, &names, mob.id)
            && dist(me, [mob.position.x, mob.position.y, mob.position.z]) <= ENEMY_RANGE
    })
}

// Legit: the first source along your look before any block, within reach (flowing liquid passed by, as a bucket's
// own look does).
fn on_crosshair(game: &Game<'_>, m: &Module) -> Option<[i32; 3]> {
    let (eye, dir) = game.aim()?;
    let mut t = 0.0;
    while t <= game.reach() {
        let p = [0, 1, 2].map(|k| (eye[k] + dir[k] * t).floor() as i32);
        // A bucket's look passes flowing liquid by, to the first source.
        if liquid(game, p).is_some() && still(game, p) {
            return wanted(game, m, p).then_some(p);
        }
        if !util::replaceable(game, p) {
            return None;
        }
        t += 0.05;
    }
    None
}

// Silent aim: the nearest wanted source within 5 blocks whose middle is in reach with nothing in the way, and the
// look at it.
fn nearest(game: &Game<'_>, m: &Module) -> Option<([i32; 3], (f32, f32))> {
    let (eye, at) = (game.eye_position()?, util::block_pos(game)?);
    let reach = game.reach();
    let mut best: Option<([i32; 3], f64)> = None;
    for x in -5..=5 {
        for y in -5..=5 {
            for z in -5..=5 {
                let p = [at[0] + x, at[1] + y, at[2] + z];
                let c = util::center_of(p);
                let d = dist(eye, c);
                if d > reach || best.is_some_and(|(_, b)| d >= b) || !wanted(game, m, p) {
                    continue;
                }
                let dir = [0, 1, 2].map(|k| (c[k] - eye[k]) / d);
                if game.clip_ray(eye, dir, d).is_none_or(|h| h.pos == p) {
                    best = Some((p, d));
                }
            }
        }
    }
    let (p, _) = best?;
    let look = look_at(eye, util::center_of(p));
    let off = (look.0 - game.yaw + 540.0).rem_euclid(360.0) - 180.0;
    (f64::from(off.abs()) <= m.n("Silent aim FOV")).then_some((p, look))
}

fn holding(game: &Game<'_>, item: &str) -> bool {
    game.held().is_some_and(|s| util::short(&s.item) == item)
}

pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    let m = with(|s| s.module(OWNER).filter(|m| m.on && m.p("Mode") == "Drain").cloned());
    let Some(m) = m.filter(|_| util::in_world(game) && !game.spectator()) else {
        if let Some(back) = st(|s| s.back.take()) {
            if game.player.is_some() && back != game.selected {
                hold(game, back);
            }
            super::sunset_combat_control::release(OWNER);
        }
        return;
    };
    let now = game.ticks;
    st(|s| {
        s.own.retain(|&(p, at)| now < at + 40 || liquid(game, p).is_some());
        s.done.retain(|p| liquid(game, *p).is_some());
    });
    if busy || game.using.is_some() || now < st(|s| s.next) {
        return;
    }
    let delay = (m.n("Delay (ticks)") as u64).max(1);
    // After a scoop: the slot from before, a step later.
    if st(|s| s.used) {
        let back = st(|s| {
            s.used = false;
            s.back.take()
        });
        if let Some(b) = back.filter(|&b| m.b("Swap back") && b != game.selected) {
            hold(game, b);
        }
        super::sunset_combat_control::release(OWNER);
        st(|s| s.next = now + delay);
        return;
    }
    let bucket = util::find_hotbar(game, |n| n == "bucket");
    let web = util::find_hotbar(game, |n| n == "cobweb").filter(|_| m.b("Fallback to cobweb"));
    if (bucket.is_none() && web.is_none()) || !enemy_near(game) {
        return;
    }
    let silent = m.p("Drain aim") == "Silent aim";
    let target = if silent { nearest(game, &m) } else { on_crosshair(game, &m).map(|p| (p, (game.yaw, game.pitch))) };
    let Some((pos, look)) = target else {
        return;
    };
    if !super::sunset_combat_control::claim(OWNER) {
        return;
    }
    st(|s| s.next = now + delay);
    if let Some(slot) = bucket {
        if !holding(game, "bucket") {
            if m.b("Drain swap") {
                let before = game.selected;
                st(|s| {
                    s.back.get_or_insert(before);
                });
                hold(game, slot);
            } else {
                super::sunset_combat_control::release(OWNER);
            }
            return;
        }
        // The scoop goes with a look the server has: Silent's is asked for, and the scoop waits a tick for it.
        let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
        turn(OWNER, look.0, look.1);
        if silent && ((have.0 - look.0 + 540.0).rem_euclid(360.0) - 180.0).abs() + (have.1 - look.1).abs() > 0.5 {
            st(|s| s.next = now + 1);
            return;
        }
        let sequence = util::sequence(game);
        let yaw = super::sunset::wire_yaw(game, look.0);
        game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: look.1 }));
        game.swing_arm();
        st(|s| {
            s.done.push(pos);
            s.used = true;
        });
        return;
    }
    if let Some(slot) = web {
        if util::place_block(game, pos, slot, true, true) {
            st(|s| s.done.push(pos));
        }
        super::sunset_combat_control::release(OWNER);
    }
}
