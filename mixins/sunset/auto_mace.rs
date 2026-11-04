// Auto Mace: the mace for your hits while you fall past Min fall. The mace is the hotbar's with the most Density (with
// Breach check, the most Breach while you have fallen 7 blocks or less). Trigger hits the player under your crosshair,
// 800 ms apart; with Silent aim, when none is, the nearest player within Max yaw of your view, 3 blocks off, in sight
// and not behind cobweb, the look turned on the wire only. Your own clicks on anything and Kill Aura's hits take the
// mace as well. Stun slam: at a raised shield (by Chance, or while its key is held, once you have fallen Stun min fall)
// an axe hit first, after the axe was held Axe hold ticks, and the mace after it; your own axe click at the shield of
// the nearest player readies one too. Takeoff elytra puts a chestplate on for the dive onto a nearby player, Expand
// hitbox grows the nearest player's box for the crosshair, and Swap back returns your slot Swap back delay after the
// mace's hit goes out (whoever sent it), or once what it swapped for came to nothing (2 s at most); a slot you or
// another module picked meanwhile stays. Every slot change, click and hit goes at the start of a tick, before its move
// packet; a hit after a slot change or with a look the server does not have yet waits for the next tick.
use super::Game;
use super::sunset::{Category, Module, hold, look_at, turn, with};
use super::sunset_combat_control as control;
use super::sunset_mace_policy::Smash;
use super::sunset_melee as melee;
use super::sunset_util as util;
use horizon_client::items::ItemStack;
use horizon_client::mobs::Mob;
use horizon_core::geometry::Vec3;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const OWNER: &str = "Auto Mace";
// Trigger's and Stun slam's own pace (ms), and how long a slot saved for Swap back waits for its hit.
const PACE_MS: f64 = 800.0;
const STALE_MS: f64 = 2000.0;
// How near a silent target must be (feet to feet), and how far Axe hold follows its target.
const SILENT_RANGE: f64 = 3.0;
const HOLD_RANGE: f64 = 6.0;
// Takeoff elytra: how near the target must be, more with its shield up.
const TAKEOFF_RANGE: f64 = 6.0;
const TAKEOFF_SHIELD_RANGE: f64 = 8.0;
const CHESTPLATES: [&str; 2] = ["minecraft:netherite_chestplate", "minecraft:diamond_chestplate"];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(OWNER, Category::Combat, "Takes the mace for your hits while you fall.")
            .num("Horizontal range", 5.0, 0.0, 6.0, 0.5)
            .num("Min fall", 10.0, 0.0, 100.0, 0.5)
            .bool("Stun slam", false)
            .pick("Stun slam mode", &["Chance", "Hold key"])
            .text("Stun slam key", "None")
            .num("Chance (%)", 100.0, 0.0, 100.0, 5.0)
            .bool("Scale with fall", false)
            .num("Stun min fall", 0.0, 0.0, 100.0, 0.5)
            .num("Axe hold (ticks)", 0.0, 0.0, 10.0, 1.0)
            .bool("Breach check", false)
            .bool("Trigger", true)
            .bool("Silent aim", false)
            .num("Max yaw", 180.0, 0.0, 180.0, 5.0)
            .bool("Takeoff elytra", false)
            .bool("Swap back", false)
            .num("Swap back delay (ms)", 50.0, 0.0, 500.0, 10.0)
            .bool("Expand hitbox", false)
            .num("Hitbox expansion", 0.5, 0.0, 5.0, 0.1),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    let stun = m.b("Stun slam");
    match name {
        "Stun slam mode" | "Stun min fall" | "Axe hold (ticks)" => stun,
        "Chance (%)" | "Scale with fall" => stun && m.p("Stun slam mode") == "Chance",
        "Stun slam key" => stun && m.p("Stun slam mode") == "Hold key",
        "Silent aim" => m.b("Trigger"),
        "Max yaw" => m.b("Trigger") && m.b("Silent aim"),
        "Swap back delay (ms)" => m.b("Swap back"),
        "Hitbox expansion" => m.b("Expand hitbox"),
        _ => true,
    }
}

#[derive(Clone, Copy)]
enum Act {
    // The mace's hit on `target`, from the hotbar's `mace` (Silent aim's target, or the crosshair's).
    Mace { target: u32, mace: usize, silent: bool },
    // Stun slam's axe hit; after Axe hold the mace's slot is taken the tick after it.
    Axe { target: u32, mace: Option<usize> },
}

// An action waiting a tick: for the look asked for, or after the slot change that went first.
#[derive(Clone, Copy)]
struct Ready {
    act: Act,
    look: (f32, f32),
    tick: u64,
}

// Axe hold: the axe is in hand, its hit on `target` waits; the look Silent aim holds meanwhile.
#[derive(Clone, Copy)]
struct AxeHold {
    target: u32,
    look: Option<(f32, f32)>,
    since: Instant,
    mace: usize,
}

#[derive(Default)]
struct State {
    fall: Smash,
    on: bool,
    // Your axe click at a raised shield, and its tick: a stun slam goes on one of the next ticks (see armed()).
    stun_armed: Option<u64>,
    // Swap back: the slot to return to, the slot Auto Mace put in hand last (the return waits on it still being
    // there), whether the mace's hit went (and when), and when the slot was saved.
    back: Option<usize>,
    given: Option<usize>,
    back_armed: bool,
    armed_at: Option<Instant>,
    saved_at: Option<Instant>,
    // The last mace hit and the last stun slam: each waits 800 ms for the next.
    hit_at: Option<Instant>,
    stun_at: Option<Instant>,
    hold: Option<AxeHold>,
    // Takeoff elytra took a hotbar chestplate this tick: that slot is never the one Swap back returns to.
    took_off: bool,
    ready: Option<Ready>,
    mace_next: Option<usize>,
    grown: Option<(u32, f64)>,
    rng: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(OWNER).filter(|m| m.on).cloned())
}

// More than `ms` since `at` (never set counts as long ago).
fn over(at: Option<Instant>, ms: f64) -> bool {
    at.is_none_or(|t| t.elapsed().as_secs_f64() * 1000.0 > ms)
}

fn random() -> f64 {
    state(|s| {
        if s.rng == 0 {
            s.rng = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64) | 1;
        }
        s.rng ^= s.rng << 13;
        s.rng ^= s.rng >> 7;
        s.rng ^= s.rng << 17;
        (s.rng >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn wrap(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

// The fall the server counts for a smash: the game's own, or what you dropped since your last smash. Not zeroed while
// gliding (Takeoff elytra goes by it then).
fn fall(game: &Game<'_>) -> f64 {
    let Some(p) = game.player.as_ref() else {
        return 0.0;
    };
    let (native, y) = (p.fall_distance, p.position.y);
    state(|s| s.fall.fall(native, y))
}

fn gliding(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| p.fall_flying)
}

fn stack<'a>(game: &'a Game<'_>, slot: usize) -> Option<&'a ItemStack> {
    game.inventory.get(slot)?.as_ref().filter(|s| s.count > 0)
}

fn is_axe(game: &Game<'_>, stack: &ItemStack) -> bool {
    game.res.items().item_tags.get("minecraft:axes").is_some_and(|items| items.contains(&*stack.item))
}

// The hotbar mace with the most Density, or with Breach check the most Breach while the fall is 7 blocks or less;
// the first of equals.
fn best_mace(game: &Game<'_>, m: &Module, fall: f64) -> Option<usize> {
    let enchantment = if m.b("Breach check") && fall <= 7.0 { "minecraft:breach" } else { "minecraft:density" };
    let mut best: Option<(usize, u32)> = None;
    for slot in 0..9 {
        if let Some(s) = stack(game, slot).filter(|s| &*s.item == "minecraft:mace") {
            let level = s.enchantment_level(enchantment);
            if best.is_none_or(|(_, top)| level > top) {
                best = Some((slot, level));
            }
        }
    }
    best.map(|(slot, _)| slot)
}

// The first hotbar axe.
fn axe_slot(game: &Game<'_>) -> Option<usize> {
    (0..9).find(|&slot| stack(game, slot).is_some_and(|s| is_axe(game, s)))
}

fn friend(game: &Game<'_>, id: u32) -> bool {
    game.remote_player_name(id).is_some_and(|name| with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(name))))
}

fn mob<'a>(game: &'a Game<'_>, id: u32) -> Option<&'a Mob> {
    game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed)
}

fn eye_of(mob: &Mob) -> [f64; 3] {
    [mob.position.x, mob.position.y + mob.eye_height(), mob.position.z]
}

// Feet to feet.
fn distance(game: &Game<'_>, mob: &Mob) -> f64 {
    game.player.as_ref().map_or(f64::INFINITY, |p| p.position.distance_squared(mob.position).sqrt())
}

// The nearest other player (no friend) by the flat distance, within `range`.
fn nearest_flat(game: &Game<'_>, range: f64) -> Option<u32> {
    let p = game.player.as_ref()?;
    let own = super::sunset::own_id(game);
    game.mobs
        .iter()
        .filter(|t| t.kind.id == "minecraft:player" && t.alive() && !t.removed && Some(t.id) != own)
        .filter(|t| !friend(game, t.id) && !super::sunset_parity::fake(t.id))
        .filter_map(|t| {
            let d = (t.position.x - p.position.x).powi(2) + (t.position.z - p.position.z).powi(2);
            (d <= range * range).then_some((t.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

// The player under the crosshair (in reach, before any block).
fn crosshair(game: &Game<'_>) -> Option<u32> {
    let (index, distance) = game.mob_hit()?;
    let target = &game.mobs[index];
    (game.before_block(distance) && target.kind.id == "minecraft:player").then_some(target.id)
}

// Nothing solid between your eyes and theirs.
fn in_view(game: &Game<'_>, target: &Mob) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let to = eye_of(target);
    super::sunset_combat::clip_clear(game, eye, to, util::dist(eye, to))
}

// A cobweb on the line from your eyes to theirs, before it reaches their box.
fn webbed(game: &Game<'_>, target: &Mob) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let to = eye_of(target);
    let d = util::dist(eye, to);
    if !d.is_finite() || d <= 0.0 {
        return false;
    }
    let step = [(to[0] - eye[0]) / d, (to[1] - eye[1]) / d, (to[2] - eye[2]) / d];
    let bounds = target.bounds();
    let mut along = 0.0;
    while along < d {
        let at = [eye[0] + step[0] * along, eye[1] + step[1] * along, eye[2] + step[2] * along];
        if bounds.contains(Vec3::new(at[0], at[1], at[2])) {
            return false;
        }
        if util::short(&util::facts(game, [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32]).name) == "cobweb" {
            return true;
        }
        along += 0.1;
    }
    false
}

// How far a look is from yours, yaw and pitch together.
fn off_view(game: &Game<'_>, look: (f32, f32)) -> f32 {
    let (yaw, pitch) = (wrap(look.0 - game.yaw), look.1 - game.pitch);
    (yaw * yaw + pitch * pitch).sqrt()
}

// Silent aim's target and the look at its eyes: of the players you may hit within Max yaw of your view the nearest,
// if it is within 3 blocks, in sight, not behind cobweb and its yaw within Max yaw of yours.
fn silent_target(game: &Game<'_>, m: &Module) -> Option<(u32, (f32, f32))> {
    let (eye, p) = (game.eye_position()?, game.player.as_ref()?);
    let max = m.n("Max yaw") as f32;
    let mut best: Option<(&Mob, f64)> = None;
    for target in game.mobs.iter().filter(|t| util::player_enemy(game, t) && !super::sunset_parity::fake(t.id)) {
        let d = p.position.distance_squared(target.position);
        if best.is_none_or(|(_, near)| d < near) && off_view(game, look_at(eye, eye_of(target))) <= max {
            best = Some((target, d));
        }
    }
    let (target, d) = best?;
    if d.sqrt() > SILENT_RANGE || !in_view(game, target) || webbed(game, target) {
        return None;
    }
    let look = look_at(eye, eye_of(target));
    (wrap(look.0 - game.yaw).abs() <= max).then_some((target.id, look))
}

fn stun_fall(m: &Module, fall: f64) -> bool {
    fall >= m.n("Min fall").max(m.n("Stun min fall"))
}

// Stun slam may go: its key held (Hold key), or the Chance roll, scaled with Scale with fall from nothing at Stun min
// fall to all of it 10 blocks further.
fn stun_roll(game: &Game<'_>, m: &Module, fall: f64) -> bool {
    if !m.b("Stun slam") {
        return false;
    }
    if m.p("Stun slam mode") == "Hold key" {
        return util::setting_key_down(game, m, "Stun slam key");
    }
    let mut chance = m.n("Chance (%)");
    if m.b("Scale with fall") {
        chance *= ((fall - m.n("Min fall").max(m.n("Stun min fall"))) / 10.0).clamp(0.0, 1.0);
    }
    random() * 100.0 < chance
}

// The shield of the player `id` is up at you, and Stun slam may go on it now.
fn slam_due(game: &Game<'_>, m: &Module, id: u32, fall: f64) -> bool {
    melee::observed_shield(game, id) && m.b("Stun slam") && state(|s| over(s.stun_at, PACE_MS)) && stun_roll(game, m, fall) && stun_fall(m, fall)
}

// Swap back: the slot in hand is kept to return to (never Takeoff elytra's chestplate slot).
fn save_back(game: &Game<'_>, m: &Module) {
    if m.b("Swap back") {
        state(|s| {
            if s.back.is_none() && !s.took_off {
                s.back = Some(game.selected);
                s.saved_at = Some(Instant::now());
            }
        });
    }
}

// Swap back's delay starts: the mace's hit went out (see forwarded), or what Auto Mace swapped for came to nothing.
fn arm_back(m: &Module) {
    if m.b("Swap back") {
        state(|s| {
            if s.back.is_some() {
                s.armed_at = Some(Instant::now());
                s.back_armed = true;
            }
        });
    }
}

// The slot to return to now (Swap back's, else `fallback`), Swap back's forgotten.
fn back_or(m: &Module, fallback: usize) -> usize {
    state(|s| {
        let slot = if m.b("Swap back") { s.back.unwrap_or(fallback) } else { fallback };
        s.back = None;
        s.back_armed = false;
        slot
    })
}

// Selects `slot`; true if that was a change.
fn take(game: &mut Game<'_>, slot: usize) -> bool {
    if game.selected == slot || slot >= 9 {
        return false;
    }
    hold(game, slot);
    true
}

// Puts `slot` in hand for Auto Mace's own use: the slot Swap back waits on.
fn give(game: &mut Game<'_>, slot: usize) {
    hold(game, slot);
    state(|s| s.given = Some(slot));
}

// Swap back, due: the slot goes back if what Auto Mace put in hand is still there; one you or another module chose
// since stays. True if the slot changed.
fn swap_back(game: &mut Game<'_>) -> bool {
    let (back, given) = state(|s| {
        s.back_armed = false;
        (s.back.take(), s.given.take())
    });
    match back {
        Some(slot) if given.is_none_or(|g| g == game.selected) => take(game, slot),
        _ => false,
    }
}

// The stun slam your axe click readied, if it is still due: it lasts three ticks, so one landing before it ends it.
fn armed(game: &Game<'_>) -> bool {
    state(|s| {
        s.stun_armed = s.stun_armed.filter(|at| game.ticks <= at.saturating_add(3));
        s.stun_armed.is_some()
    })
}

fn attack(game: &mut Game<'_>, id: u32) {
    game.send(S::Attack(play::Attack { id: id as i32 }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    game.reset_attack_strength();
    melee::claim_tick(game);
}

// The server has `look` from the last move packet.
fn has_look(game: &Game<'_>, look: (f32, f32)) -> bool {
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    wrap(have.0 - look.0).abs() + (have.1 - look.1).abs() <= 0.5
}

// Within your entity reach of `target`'s box.
fn reachable(game: &Game<'_>, target: &Mob) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let b = target.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let reach = if game.creative { 5.0 } else { 3.0 };
    super::sunset_melee_policy::box_distance(eye, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]).is_some_and(|d| d <= reach)
}

// The action goes now when no slot changed this tick and the server has its look; else the look is asked for and it
// goes next tick.
fn go(game: &mut Game<'_>, m: &Module, act: Act, look: (f32, f32), changed: bool) -> bool {
    if !changed && has_look(game, look) {
        return run(game, m, act);
    }
    turn(OWNER, look.0, look.1);
    let tick = game.ticks;
    state(|s| s.ready = Some(Ready { act, look, tick }));
    changed
}

// The hit itself, its target checked again. A mace hit that cannot go still starts Swap back's delay. True if
// anything was sent.
fn run(game: &mut Game<'_>, m: &Module, act: Act) -> bool {
    match act {
        Act::Mace { target, mace, silent } => {
            let fit = game.selected == mace
                && stack(game, mace).is_some_and(|s| &*s.item == "minecraft:mace")
                && !super::sunset_parity::fake(target)
                && !melee::guard_attack(game, target)
                && mob(game, target).is_some_and(|t| {
                    if silent { distance(game, t) <= SILENT_RANGE && in_view(game, t) } else { reachable(game, t) }
                });
            if fit {
                attack(game, target);
                state(|s| s.hit_at = Some(Instant::now()));
            }
            arm_back(m);
            fit
        }
        Act::Axe { target, mace } => {
            let fit = game.held().is_some_and(|s| is_axe(game, s))
                && mob(game, target).is_some()
                && !super::sunset_parity::fake(target)
                && !melee::guard_attack(game, target);
            if fit {
                attack(game, target);
                state(|s| s.stun_at = Some(Instant::now()));
            }
            if let Some(slot) = mace {
                state(|s| s.mace_next = Some(slot));
            }
            fit
        }
    }
}

// Trigger's mace hit: the mace selected first if it is not in hand.
fn strike(game: &mut Game<'_>, m: &Module, target: u32, look: Option<(f32, f32)>, mace: usize) -> bool {
    if !control::claim(OWNER) {
        return false;
    }
    let changed = game.selected != mace;
    if changed {
        save_back(game, m);
        give(game, mace);
    }
    go(game, m, Act::Mace { target, mace, silent: look.is_some() }, look.unwrap_or((game.yaw, game.pitch)), changed)
}

// Stun slam: the axe in hand, and its hit now, or after Axe hold.
fn stun_slam(game: &mut Game<'_>, m: &Module, target: u32, look: Option<(f32, f32)>, axe: usize, mace: usize) -> bool {
    if !control::claim(OWNER) {
        return false;
    }
    let changed = game.selected != axe;
    if changed {
        save_back(game, m);
        give(game, axe);
    }
    if m.n("Axe hold (ticks)") <= 0.0 {
        return go(game, m, Act::Axe { target, mace: None }, look.unwrap_or((game.yaw, game.pitch)), changed);
    }
    if let Some(look) = look {
        turn(OWNER, look.0, look.1);
    }
    state(|s| s.hold = Some(AxeHold { target, look, since: Instant::now(), mace }));
    changed
}

// Axe hold, each tick: the look kept until the axe has been held long enough, then its hit (and the mace the tick
// after). A target gone or more than 6 blocks off ends it: the slot goes back (or to the mace).
fn hold_tick(game: &mut Game<'_>, m: &Module, h: AxeHold) -> bool {
    if !mob(game, h.target).is_some_and(|t| distance(game, t) <= HOLD_RANGE) {
        state(|s| {
            s.hold = None;
            s.given = None;
        });
        let slot = back_or(m, h.mace);
        return take(game, slot);
    }
    if h.since.elapsed().as_secs_f64() * 1000.0 < m.n("Axe hold (ticks)") * 50.0 {
        if let Some(look) = h.look {
            turn(OWNER, look.0, look.1);
        }
        return false;
    }
    state(|s| s.hold = None);
    go(game, m, Act::Axe { target: h.target, mace: Some(h.mace) }, h.look.unwrap_or((game.yaw, game.pitch)), false)
}

// Your axe click at a shield readied a stun slam: on the crosshair's player, else Silent aim's target (with its look).
fn armed_slam(game: &mut Game<'_>, m: &Module, fall: f64, mace: usize) -> bool {
    let Some(axe) = axe_slot(game) else {
        state(|s| s.stun_armed = None);
        return false;
    };
    let target = match crosshair(game).filter(|&id| !friend(game, id)) {
        Some(id) => Some((id, None)),
        None => silent_target(game, m).map(|(id, look)| (id, Some(look))),
    };
    state(|s| s.stun_armed = None);
    match target {
        Some((target, look)) if stun_fall(m, fall) => stun_slam(game, m, target, look, axe, mace),
        _ => false,
    }
}

// Takeoff elytra: gliding with an elytra on, falling past Min fall toward a player within Horizontal range and 6 blocks
// (8 with their shield up), a netherite or else a diamond chestplate goes on: one from the hotbar is selected and
// used, else one from the inventory is swapped on with clicks. Either ends the tick (one slot change a tick). True if
// anything was sent.
fn takeoff(game: &mut Game<'_>, m: &Module, fall: f64) -> bool {
    if !m.b("Takeoff elytra") || fall < m.n("Min fall") {
        return false;
    }
    let Some(target) = nearest_flat(game, m.n("Horizontal range")) else {
        return false;
    };
    let range = if melee::observed_shield(game, target) { TAKEOFF_SHIELD_RANGE } else { TAKEOFF_RANGE };
    if !mob(game, target).is_some_and(|t| distance(game, t) <= range)
        || !gliding(game)
        || !game.armor[1].as_ref().is_some_and(|s| &*s.item == "minecraft:elytra")
        || !control::claim(OWNER)
    {
        return false;
    }
    for item in CHESTPLATES {
        if let Some(slot) = (0..9).find(|&i| stack(game, i).is_some_and(|s| &*s.item == item)) {
            hold(game, slot);
            // The use goes with your own look, the one this tick's move packet carries.
            turn(OWNER, game.yaw, game.pitch);
            let sequence = util::sequence(game);
            let yaw = super::sunset::wire_yaw(game, game.yaw);
            game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: game.pitch.clamp(-90.0, 90.0) }));
            game.swing_arm();
            game.send(S::Punch(Empty));
            state(|s| s.took_off = true);
            return true;
        }
    }
    for item in CHESTPLATES {
        if let Some(i) = (9..36).find(|&i| stack(game, i).is_some_and(|s| &*s.item == item)) {
            super::sunset::wear(game, i, 1);
            return true;
        }
    }
    false
}

// The start of a tick, before its move packet, after the hits and slot changes of the modules before it (`busy`:
// one went). True if Auto Mace sent a slot change, use or hit.
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) -> bool {
    if let Some(p) = game.player.as_ref() {
        let (grounded, y) = (p.ground.on_ground, p.position.y);
        state(|s| s.fall.observe(grounded, y));
    }
    let Some(m) = module() else {
        // Switched off: the slot it swapped goes back first, at the start of a tick with nothing else in it.
        if state(|s| s.on) && !busy && !melee::acted(game) {
            let sent = melee::allowed(game) && swap_back(game);
            off();
            return sent;
        }
        return false;
    };
    let fall = fall(game);
    let grown = if m.b("Expand hitbox") && m.n("Hitbox expansion") > 0.0 && fall > m.n("Min fall") {
        nearest_flat(game, m.n("Horizontal range")).map(|id| (id, m.n("Hitbox expansion") / 5.0))
    } else {
        None
    };
    state(|s| {
        s.on = true;
        s.took_off = false;
        s.grown = grown;
    });
    // A slot saved for Swap back whose mace hit never came goes back after 2 s all the same.
    if state(|s| s.back.is_some() && !s.back_armed && over(s.saved_at, STALE_MS)) {
        state(|s| s.saved_at = None);
        arm_back(&m);
    }
    if !melee::allowed(game) || !control::available(OWNER) {
        if state(|s| s.ready.take()).is_some() {
            arm_back(&m);
        }
        return false;
    }
    if busy || melee::acted(game) {
        return false;
    }
    let sent = act(game, &m, fall);
    // The hand is Auto Mace's only while an action is under way; Swap back's wait leaves it to the others (a slot one
    // of them changes meanwhile is theirs).
    if state(|s| s.hold.is_none() && s.ready.is_none() && s.mace_next.is_none()) {
        control::release(OWNER);
    } else {
        control::claim(OWNER);
    }
    sent
}

fn act(game: &mut Game<'_>, m: &Module, fall: f64) -> bool {
    // What waited a tick goes first, once the server has its look (one tick more for it at most). One that cannot go
    // starts Swap back's delay, so a slot swapped for it does not stay.
    if let Some(r) = state(|s| s.ready.take()) {
        if has_look(game, r.look) {
            return run(game, m, r.act);
        }
        if game.ticks < r.tick + 2 {
            state(|s| s.ready = Some(r));
            return false;
        }
        arm_back(m);
    }
    // Axe hold's mace, the tick after its axe hit.
    if let Some(slot) = state(|s| s.mace_next.take()) {
        if game.selected == slot {
            return false;
        }
        give(game, slot);
        return true;
    }
    let stun = m.b("Stun slam");
    let armed = armed(game);
    // Swap back: the mace's hit went and Swap back delay is over, with no stun slam under way.
    if state(|s| s.back_armed && s.hold.is_none() && over(s.armed_at, m.n("Swap back delay (ms)"))) && !armed && swap_back(game) {
        return true;
    }
    if let Some(h) = state(|s| s.hold) {
        // Stun slam switched off while the axe was held: the slot goes back (or to the mace).
        if !stun {
            state(|s| {
                s.hold = None;
                s.given = None;
            });
            let slot = back_or(m, h.mace);
            return take(game, slot);
        }
        return hold_tick(game, m, h);
    }
    if takeoff(game, m, fall) {
        return true;
    }
    let Some(mace) = best_mace(game, m, fall) else {
        state(|s| s.stun_armed = None);
        return false;
    };
    if fall <= m.n("Min fall") {
        return false;
    }
    if stun && armed {
        return armed_slam(game, m, fall, mace);
    }
    let trigger = m.b("Trigger") && state(|s| over(s.hit_at, PACE_MS));
    if let Some(target) = crosshair(game) {
        if !trigger || friend(game, target) {
            return false;
        }
        if slam_due(game, m, target, fall)
            && let Some(axe) = axe_slot(game)
        {
            return stun_slam(game, m, target, None, axe, mace);
        }
        if gliding(game) {
            return false;
        }
        return strike(game, m, target, None, mace);
    }
    if !trigger || !m.b("Silent aim") {
        state(|s| s.stun_armed = None);
        return false;
    }
    let Some((target, look)) = silent_target(game, m) else {
        return false;
    };
    if !armed && slam_due(game, m, target, fall) {
        let Some(axe) = axe_slot(game) else {
            state(|s| s.stun_armed = None);
            return false;
        };
        return stun_slam(game, m, target, Some(look), axe, mace);
    }
    if gliding(game) {
        return false;
    }
    strike(game, m, target, Some(look), mace)
}

// Readies a stun slam: falling past Min fall with an axe in hand at the raised shield of the nearest player within
// Horizontal range, with an axe in the hotbar.
fn ready_slam(game: &Game<'_>, m: &Module, fall: f64) -> bool {
    if armed(game) || fall <= m.n("Min fall") || !game.held().is_some_and(|s| is_axe(game, s)) {
        return false;
    }
    let ready = nearest_flat(game, m.n("Horizontal range")).is_some_and(|id| slam_due(game, m, id, fall)) && axe_slot(game).is_some();
    if ready {
        state(|s| s.stun_armed = Some(game.ticks));
    }
    ready
}

// Your own click, before it goes (after Auto Weapon's choice): with an axe at a raised shield it readies a stun slam;
// else on anything under the crosshair, falling past Min fall, the mace goes in hand for it (not when Auto Weapon's
// Axe on shields has the hit). Swap back's delay starts once its hit goes out, which Criticals may hold for its hop.
pub(super) fn before_click(game: &mut Game<'_>) {
    let Some(m) = module() else {
        return;
    };
    if !melee::allowed(game) || !control::available(OWNER) {
        return;
    }
    let fall = fall(game);
    if ready_slam(game, &m, fall) {
        return;
    }
    let Some(mace) = best_mace(game, &m, fall) else {
        return;
    };
    let Some((index, distance)) = game.mob_hit() else {
        return;
    };
    let id = game.mobs[index].id;
    if game.selected == mace || !game.before_block(distance) || fall <= m.n("Min fall") || melee::axe_at_shield(game, id) {
        return;
    }
    if !control::claim(OWNER) {
        return;
    }
    save_back(game, &m);
    give(game, mace);
    control::release(OWNER);
}

// Kill Aura's or Trigger Bot's hit, about to go: with an axe at a raised shield it readies a stun slam, as your click
// does.
pub(super) fn before_strike(game: &Game<'_>) {
    if let Some(m) = module()
        && melee::allowed(game)
    {
        ready_slam(game, &m, fall(game));
    }
}

// Falling past Min fall with a mace in the hotbar: Auto Mace has the hit, Auto Weapon keeps out of it.
pub(super) fn claims(game: &Game<'_>) -> bool {
    module().is_some_and(|m| {
        let fall = fall(game);
        fall > m.n("Min fall") && best_mace(game, &m, fall).is_some()
    })
}

// Kill Aura's or Trigger Bot's hit on `id`: the mace's slot when it is not in hand (none when Auto Weapon's Axe on
// shields has the hit), and with Swap back the ticks after the hit before it goes back.
pub(super) fn strike_weapon(game: &Game<'_>, id: u32) -> Option<(usize, Option<u64>)> {
    let m = module()?;
    let fall = fall(game);
    if fall <= m.n("Min fall") || !melee::allowed(game) || !control::available(OWNER) || melee::axe_at_shield(game, id) {
        return None;
    }
    let mace = best_mace(game, &m, fall).filter(|&slot| slot != game.selected)?;
    Some((mace, m.b("Swap back").then(|| (m.n("Swap back delay (ms)").max(0.0) / 50.0) as u64)))
}

// An Attack packet went out with a mace in hand (anyone's: your click, Criticals' held one, Kill Aura's, Trigger's):
// a smash counts the fall again from here, and with the mace Auto Mace put in hand Swap back's delay starts.
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    let S::Attack(p) = packet else {
        return;
    };
    if p.id < 0 || !game.held().is_some_and(|s| &*s.item == "minecraft:mace") {
        return;
    }
    if state(|s| s.back.is_some() && s.given == Some(game.selected))
        && let Some(m) = module()
    {
        arm_back(&m);
    }
    if gliding(game) {
        return;
    }
    let smash = fall(game) > 1.5;
    if let Some(y) = game.player.as_ref().map(|p| p.position.y)
        && smash
    {
        state(|s| s.fall.smashed(y));
    }
}

// Expand hitbox's player and how far each side of its box grows, for the crosshair's pick.
pub(super) fn grown() -> Option<(u32, f64)> {
    state(|s| s.grown)
}

// Switched off: a stun slam or Axe hold under way is dropped (pre_tick has returned the swapped slot).
fn off() {
    super::sunset::cancel_turns(OWNER);
    control::release(OWNER);
    state(|s| {
        let (fall, rng) = (std::mem::take(&mut s.fall), s.rng);
        *s = State { fall, rng, ..State::default() };
    });
}

// A new world or life, Sunset off: all of it goes.
pub(super) fn cancel() {
    super::sunset::cancel_turns(OWNER);
    state(|s| {
        let rng = s.rng;
        *s = State { rng, ..State::default() };
    });
}
