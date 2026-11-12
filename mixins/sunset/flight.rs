// Flight's experimental strategies (NCP Hover, NCP Burst, NCP Bounce, Old NCP), each with its own state and
// lifecycle: Idle, Preparing (waiting for its start conditions), Active, Recovery (after a setback, real knockback or a
// burst's end: no override of its own until it settles). Abilities and Velocity stay in runtime.rs; these never take
// the false ground claim or the Packet anti kick those use (runtime.rs gates both on `experimental`).
//
// Written from how the old movement strategies behave (Old NCP after how a 1.8 client's hover moves), not copied from
// any client's code, and for testing against a specific NoCheatPlus build: no mode here is known to pass anything.
// - Hover: vertical speed held at zero, horizontal movement left as the game makes it, and the height sent raised by
//   1e-5 plus, every other tick, a random 1e-10 to 1e-5 (the offset is never negative).
// - Burst: on the ground with room above, a jump with a large horizontal impulse (Burst speed, blocks a tick) that
//   decays by Burst friction each tick; the fall is softened while it lasts; it ends falling, slowed, or after
//   20 ticks, then waits to land (Burst repeat for the next one).
// - Bounce: a cycle of Bounce period ticks: the movement is saved, held at a stop, then given back scaled by Bounce
//   boost (upward at most a jump's 0.42).
// - Old NCP: the old NoCheatPlus hover as 1.8 clients flew it: no vertical speed at all (the height stays where it was),
//   each move packet's height raised by 1e-5 and then, on alternate ticks, raised or lowered by a random 1e-10 to 1e-5,
//   a move packet every tick, and the move across the 1.8 strafe at the allowed distance (legacy_speed: walking's
//   0.221, a sprint's 1.3 of it whenever the keys could sprint, Speed's 20% a level) along your keys, nothing kept of the
//   last move but a random 1/2000 at most. There is no up or down.

use super::Game;
use super::sunset::Module;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Strategy {
    Hover,
    Burst,
    Bounce,
    OldNcp,
}

impl Strategy {
    // The experimental strategy a Flight Mode names, if it names one.
    pub(super) fn of(mode: &str) -> Option<Self> {
        match mode {
            "NCP Hover" => Some(Self::Hover),
            "NCP Burst" => Some(Self::Burst),
            "NCP Bounce" => Some(Self::Bounce),
            "Old NCP" => Some(Self::OldNcp),
            _ => None,
        }
    }
}

pub(super) fn experimental(mode: &str) -> bool {
    Strategy::of(mode).is_some()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Idle,
    Preparing,
    Active,
    Recovery,
}

#[derive(Debug, Default)]
struct State {
    strategy: Option<Strategy>,
    phase: Phase,
    ticks: u32,
    // Burst's speed now, blocks a tick.
    speed: f64,
    // Whether the last tick's horizontal movement was this strategy's (taken back when it stops).
    owns_momentum: bool,
    // Bounce's saved movement.
    saved: Option<[f64; 3]>,
    recover: u32,
    // The height added to this tick's move packet (Hover, Old NCP).
    offset: Option<f64>,
    // Old NCP's last move across.
    last_across: [f64; 2],
    rng: u64,
    corrections: u32,
    knocks: u32,
    bursts: u32,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(|| State { rng: 0x9e37_79b9_7f4a_7c15, ..State::default() }))
}

fn unit(rng: &mut u64) -> f64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    (*rng >> 11) as f64 / (1u64 << 53) as f64
}

// ---- the strategies' rules, apart from the game (unit tested)

// Hover's height offset for a tick: 1e-5, and every other tick a random 1e-10 to 1e-5 more.
pub(super) fn hover_offset(tick: u32, rng: &mut u64) -> f64 {
    let extra = if tick % 2 == 0 { 1e-10 + unit(rng) * (1e-5 - 1e-10) } else { 0.0 };
    1e-5 + extra
}

// Old NCP's height offset for a tick: 1e-5, then a random 1e-10 to 1e-5 added on even ticks and taken away on odd ones.
pub(super) fn old_ncp_offset(tick: u64, rng: &mut u64) -> f64 {
    let r = 1e-10 + unit(rng) * (1e-5 - 1e-10);
    1e-5 + if tick % 2 == 0 { r } else { -r }
}

// Old NCP's move across for this tick, before the tick moves you (None while it recovers from a setback or knockback).
pub(super) fn old_ncp_across(game: &mut Game<'_>) -> Option<[f64; 2]> {
    use super::sunset_legacy_speed::{allowed_distance, strafe_move};
    if state(|s| s.recover > 0 || s.phase == Phase::Recovery) {
        return None;
    }
    let (input, allowance) = super::sunset::legacy_strafe_frame(game)?;
    let ground = game.player.as_ref()?.ground.on_ground;
    let friction = if ground { 0.6 * 0.91 } else { 0.91 };
    Some(state(|s| {
        let keep = unit(&mut s.rng) / 2000.0;
        let residual = [s.last_across[0] * friction, s.last_across[1] * friction];
        let across = strafe_move(allowed_distance(allowance, input), input, residual, keep);
        s.last_across = across;
        across
    }))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BurstCfg {
    pub(super) speed: f64,
    pub(super) friction: f64,
    pub(super) repeat: bool,
}

// What a Burst tick does to the movement: jump, set the horizontal speed, set the vertical speed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct BurstOut {
    pub(super) jump: bool,
    pub(super) horizontal: Option<f64>,
    pub(super) vertical: Option<f64>,
}

// One Burst tick from its phase, ticks and speed, and the player's: on the ground, room above, vertical speed.
pub(super) fn burst_tick(phase: Phase, ticks: u32, speed: f64, bursts: u32, ground: bool, room: bool, vy: f64, cfg: BurstCfg)
    -> (Phase, f64, BurstOut) {
    match phase {
        Phase::Idle | Phase::Preparing => {
            if bursts > 0 && !cfg.repeat {
                return (Phase::Idle, 0.0, BurstOut::default());
            }
            if ground && room {
                (Phase::Active, cfg.speed, BurstOut { jump: true, horizontal: Some(cfg.speed), vertical: Some(0.42) })
            } else {
                (Phase::Preparing, 0.0, BurstOut::default())
            }
        }
        Phase::Active => {
            let speed = speed * cfg.friction.clamp(0.5, 0.999);
            // Ends falling fast, slowed to about a sprint, or after 20 ticks; landing ends it too.
            if (ground && ticks > 1) || vy < -0.3 || speed < 0.3 || ticks >= 20 {
                return (Phase::Recovery, 0.0, BurstOut::default());
            }
            // The fall softened while it lasts: no more than a slow drop.
            let vertical = (vy < 0.0).then(|| vy.max(-0.05));
            (Phase::Active, speed, BurstOut { jump: false, horizontal: Some(speed), vertical })
        }
        Phase::Recovery => (if ground { Phase::Idle } else { Phase::Recovery }, 0.0, BurstOut::default()),
    }
}

// Bounce: the movement for tick `t` of its cycle, from the movement now and what was saved.
pub(super) fn bounce_tick(t: u32, period: u32, boost: f64, now: [f64; 3], saved: Option<[f64; 3]>) -> ([f64; 3], Option<[f64; 3]>) {
    let period = period.max(2);
    match t % period {
        0 => (now, Some(now)),
        x if x + 1 < period => ([0.0; 3], saved),
        _ => {
            let s = saved.unwrap_or(now);
            ([s[0] * boost, (s[1] * boost).clamp(-0.42, 0.42), s[2] * boost], None)
        }
    }
}

// ---- in the game

// The way you move, as a unit vector on the ground (your keys along your yaw, or straight ahead with none).
fn heading(game: &Game<'_>) -> [f64; 2] {
    let held = |k: KeyCode| f64::from(u8::from(game.keys.contains(&k)));
    let (forward, left) = (held(KeyCode::KeyW) - held(KeyCode::KeyS), held(KeyCode::KeyA) - held(KeyCode::KeyD));
    let (forward, left) = if forward == 0.0 && left == 0.0 { (1.0, 0.0) } else { (forward, left) };
    let yaw = f64::from(game.yaw).to_radians();
    let (fx, fz) = (-yaw.sin(), yaw.cos());
    let (lx, lz) = (fz, -fx);
    let (x, z) = (fx * forward + lx * left, fz * forward + lz * left);
    let len = x.hypot(z).max(1e-9);
    [x / len, z / len]
}

// One tick of the experimental strategy Flight's Mode names (runtime.rs flight_post calls it in place of Abilities'
// and Velocity's work).
pub(super) fn tick(game: &mut Game<'_>, m: &Module) {
    let Some(strategy) = Strategy::of(m.p("Mode")) else {
        return;
    };
    // A new strategy starts from nothing.
    if state(|s| s.strategy != Some(strategy)) {
        stop(game);
        state(|s| s.strategy = Some(strategy));
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let ground = p.ground.on_ground;
    let (velocity, position) = ([p.velocity.x, p.velocity.y, p.velocity.z], [p.position.x, p.position.y, p.position.z]);
    // Recovery after a setback or real knockback: hands off until it has passed.
    let recovering = state(|s| {
        s.offset = None;
        if s.recover > 0 {
            s.recover -= 1;
            s.phase = Phase::Recovery;
            return true;
        }
        if s.phase == Phase::Recovery && strategy != Strategy::Burst {
            s.phase = Phase::Idle;
        }
        false
    });
    if recovering {
        state(|s| s.owns_momentum = false);
        return;
    }
    match strategy {
        Strategy::Hover => {
            let offset = state(|s| {
                s.phase = Phase::Active;
                s.ticks = s.ticks.wrapping_add(1);
                let t = s.ticks;
                hover_offset(t, &mut s.rng)
            });
            state(|s| s.offset = (!ground).then_some(offset));
            if let Some(p) = game.player.as_mut() {
                p.velocity.y = 0.0;
            }
        }
        Strategy::OldNcp => {
            let tick = game.ticks;
            state(|s| {
                s.phase = Phase::Active;
                s.ticks = s.ticks.wrapping_add(1);
                s.offset = Some(old_ncp_offset(tick, &mut s.rng));
            });
            if let Some(p) = game.player.as_mut() {
                p.velocity.y = 0.0;
            }
        }
        Strategy::Burst => {
            let feet = [position[0].floor() as i32, position[1].floor() as i32, position[2].floor() as i32];
            let room = (2..4).all(|dy| super::sunset_util::is_air(game, [feet[0], feet[1] + dy, feet[2]]));
            let cfg = BurstCfg { speed: m.n("Burst speed"), friction: m.n("Burst friction"), repeat: m.b("Burst repeat") };
            let (phase, ticks, speed, bursts) = state(|s| (s.phase, s.ticks, s.speed, s.bursts));
            // Damage fly: the hit itself starts a burst, in the air too.
            let ground = ground || damage(|d| std::mem::take(&mut d.fresh));
            let (next, speed, out) = burst_tick(phase, ticks, speed, bursts, ground, room, velocity[1], cfg);
            let dir = heading(game);
            state(|s| {
                if next == Phase::Active && phase != Phase::Active {
                    s.bursts += 1;
                    s.ticks = 0;
                } else {
                    s.ticks = s.ticks.saturating_add(1);
                }
                s.phase = next;
                s.speed = speed;
                s.owns_momentum = out.horizontal.is_some();
            });
            if let Some(p) = game.player.as_mut() {
                if let Some(h) = out.horizontal {
                    (p.velocity.x, p.velocity.z) = (dir[0] * h, dir[1] * h);
                }
                if let Some(v) = out.vertical {
                    p.velocity.y = v;
                }
                if out.jump {
                    p.ground.on_ground = false;
                }
            }
        }
        Strategy::Bounce => {
            let period = m.n("Bounce period (ticks)") as u32;
            let boost = m.n("Bounce boost");
            let (t, saved) = state(|s| {
                s.phase = Phase::Active;
                let t = s.ticks;
                s.ticks = s.ticks.wrapping_add(1);
                (t, s.saved)
            });
            let (out, saved) = bounce_tick(t, period, boost, velocity, saved);
            state(|s| {
                s.saved = saved;
                s.owns_momentum = true;
            });
            if let Some(p) = game.player.as_mut() {
                (p.velocity.x, p.velocity.y, p.velocity.z) = (out[0], out[1], out[2]);
            }
        }
    }
}

// The height Hover adds to this tick's move packet.
pub(super) fn offset() -> Option<f64> {
    state(|s| s.offset)
}

// The server set you back: hands off for `pause` ticks, nothing saved kept; a self damage under way is over.
pub(super) fn correction(pause: u32) {
    damage(|d| d.broken = true);
    state(|s| {
        if s.strategy.is_some() {
            s.recover = pause.max(1);
            s.corrections += 1;
            s.saved = None;
            s.speed = 0.0;
            s.offset = None;
            s.phase = Phase::Recovery;
        }
    });
}

// Real knockback came: it is the server's movement, not to be held or replayed. Hands off for 10 ticks; with Damage fly
// on it is what the flight starts from instead (`damage_tick`).
pub(super) fn knocked(velocity: [f64; 3]) {
    if damage(|d| {
        d.knock = Some(velocity);
        d.trigger != Trigger::Off
    }) {
        return;
    }
    state(|s| {
        // Old NCP's strafe and held height take over the knockback as a 1.8 client's do: no recovery for it.
        if s.strategy.is_some_and(|k| k != Strategy::OldNcp) {
            s.recover = s.recover.max(10);
            s.knocks += 1;
            s.saved = None;
            s.phase = Phase::Recovery;
        }
    });
}

// Flight off, another mode, Panic or a new world: the horizontal speed a strategy gave is taken back (gravity and your
// own keys are left as they are), and nothing is kept.
pub(super) fn stop(game: &mut Game<'_>) {
    let owned = state(|s| {
        let owned = s.owns_momentum;
        let (corrections, knocks, rng) = (s.corrections, s.knocks, s.rng);
        *s = State { corrections, knocks, rng, ..State::default() };
        owned
    });
    if owned && let Some(p) = game.player.as_mut() {
        p.velocity.x = 0.0;
        p.velocity.z = 0.0;
    }
}

// A new world or life: the player is new, so only the state goes (Damage fly waits again, its health unknown).
pub(super) fn reset() {
    damage(|d| {
        (d.phase, d.hurt, d.owed, d.knock, d.carry, d.broken, d.health) = (Wait::Armed, false, false, None, None, false, None);
    });
    state(|s| {
        let (corrections, knocks, rng) = (s.corrections, s.knocks, s.rng);
        *s = State { corrections, knocks, rng, ..State::default() };
    });
}

// ---- Damage fly
//
// Flight's Damage fly, over every Mode: Flight waits (no flight, no false ground, no anti kick: you move as the game
// moves you, and Speed works) until you are hurt, then flies in its Mode for Damage time ticks; another hurt meanwhile
// starts the time again. Then it waits again, or (After damage Disable) turns off.
// - The hit's knockback (SetEntityMotion for you, with the hurt or just after) is the speed anticheats that allow a
//   hit's velocity for a while allow, so the flight starts from it: its speed across the ground times Damage boost,
//   along the knockback (Keep) or your keys (Redirect), and its rise as it came. Velocity mode, which sets the
//   movement each tick, carries it on at the air's 0.91 a tick; the other modes leave it to the game's friction. An
//   anticheat that predicts movement (Grim, Horizon's) expects the knockback exactly: only Keep with a boost of 1
//   matches it, and the flight after it is checked as any flight is.
// - Self damage, standing on the ground: a fall the server counts by vanilla's rules, told in position packets while
//   you stay where you are. The server deals it on the landing; a hurt then opens the window like any other.
//   - Burst (default): in one tick, pairs of moves BURST_STEP up and back down, off the ground, then the ground. A
//     Bukkit server fires PlayerMoveEvent, which its anticheat checks movement on (NoCheatPlus), only for a move of
//     more than 1/16 block from where it last fired, so none of these reaches it; the server's own movement code
//     still adds each drop to the fall. First a fall just past the safe distance (a 1.8 server's `ceil(fall - 3)`
//     costs half a heart), and when that brings no hurt one past it by 1.05 (a modern server's
//     `floor(fall + 1e-6 - 3)`); the height that worked is kept. Over a hundred packets in a tick: a packet-rate check
//     may count them, and Horizon's own server kicks for a second position packet in a tick (use Spread there).
//   - Spread: the game's own move packets, one a tick, while you are held still: up at most a jump's 0.42 a tick to
//     the safe distance plus 1.05, down as gravity pulls, then the ground. Plainly a climb to a checked move.
//   An anticheat that predicts movement (Grim, Horizon's) sees either. A failed try (a setback, you moved, no hurt)
//   waits two seconds before the next; three in a row and it only waits for other damage until Flight is turned on
//   again.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Trigger {
    #[default]
    Off,
    Wait,
    SelfDamage,
}

impl Trigger {
    pub(super) fn of(setting: &str) -> Self {
        match setting {
            "Wait" => Self::Wait,
            "Self damage" => Self::SelfDamage,
            _ => Self::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Wait {
    // Waiting for a hurt.
    #[default]
    Armed,
    // Self damage: the tick of its fall this tick's move tells (the last one past its heights: the landing).
    Falling(usize),
    // Self damage landed: ticks since, waiting for the server's hurt.
    Landed(u32),
    // Flying: ticks left, this one included.
    Open(u32),
    // Over with After damage Disable: Flight goes off.
    Spent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DamageCfg {
    pub(super) trigger: Trigger,
    pub(super) window: u32,
    pub(super) again: bool,
}

// The ticks a self damage's landing waits for the server's hurt before it counts as failed, and a failed try waits.
const LANDED_WAIT: u32 = 20;
const RETRY_WAIT: u64 = 40;
// Burst's step: under the 1/16 block (squared 1/256) a Bukkit server's PlayerMoveEvent needs, with room for where it
// last fired being a little off to the side.
pub(super) const BURST_STEP: f64 = 0.049;
// Self damage tries that bring no hurt before it stops trying.
const MOST_FAILS: u32 = 3;
// How far past the safe fall distance a fall goes for a point of damage under a modern server's rule.
const FALL_EXTRA: f64 = 1.05;

// One tick of Damage fly's wait (not Off): the phase now, from the last one and this tick's news: a hurt (never while
// a self damage still falls; that one is kept for its landing), whether a self damage may start, whether the one
// falling broke (a setback, you moved). `fall`: the self damage's heights.
pub(super) fn wait_tick(phase: Wait, cfg: DamageCfg, hurt: bool, can_start: bool, broken: bool, fall: usize) -> Wait {
    if hurt && phase != Wait::Spent {
        return Wait::Open(cfg.window.max(1));
    }
    match phase {
        Wait::Armed if cfg.trigger == Trigger::SelfDamage && can_start => Wait::Falling(0),
        Wait::Armed => Wait::Armed,
        Wait::Falling(_) if broken => Wait::Armed,
        Wait::Falling(t) if t < fall => Wait::Falling(t + 1),
        Wait::Falling(_) => Wait::Landed(0),
        Wait::Landed(w) if w < LANDED_WAIT => Wait::Landed(w + 1),
        Wait::Landed(_) => Wait::Armed,
        Wait::Open(n) if n > 1 => Wait::Open(n - 1),
        Wait::Open(_) if cfg.again => Wait::Armed,
        Wait::Open(_) | Wait::Spent => Wait::Spent,
    }
}

// Self damage's fall: the heights above where you stand its moves tell, one a tick: up at most a jump's 0.42 a tick to
// `top`, then down as gravity pulls (0.08 a tick, 0.98 drag) to 0. The server counts the way down: `top`.
pub(super) fn fall_heights(top: f64) -> Vec<f64> {
    let up = (top / 0.42).ceil().max(1.0) as usize;
    let mut out: Vec<f64> = (1..=up).map(|k| top * k as f64 / up as f64).collect();
    let (mut y, mut v) = (top, 0.0);
    while y > 0.0 {
        v = (v - 0.08) * 0.98;
        y = (y + v).max(0.0);
        out.push(y);
    }
    out
}

// Burst's moves above where you stand, in order: (height, on the ground). The drops add up to at least `top`; none is
// more than `step`.
pub(super) fn burst_moves(top: f64, step: f64) -> Vec<(f64, bool)> {
    let pairs = (top / step).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(pairs * 2 + 1);
    for _ in 0..pairs {
        out.push((step, false));
        out.push((0.0, false));
    }
    out.push((0.0, true));
    out
}

// The knockback Damage fly flies on with: its speed across the ground times `boost`, along it or along `toward`
// (Redirect), and its rise as it came.
pub(super) fn kick(knock: [f64; 3], boost: f64, toward: Option<[f64; 2]>) -> [f64; 3] {
    let across = knock[0].hypot(knock[2]);
    let speed = across * boost.max(0.0);
    let [x, z] = match toward {
        Some(d) => d,
        None if across > 0.0 => [knock[0] / across, knock[2] / across],
        None => [0.0, 0.0],
    };
    [x * speed, knock[1], z * speed]
}

#[derive(Debug, Default)]
struct Damage {
    trigger: Trigger,
    phase: Wait,
    // A hurt since the last tick, and one that came while a self damage still fell (for its landing).
    hurt: bool,
    owed: bool,
    // The server's last knockback, kept until the window uses it (or the tick ends without one open).
    knock: Option<[f64; 3]>,
    // Velocity mode's knockback speed across the ground, carried on.
    carry: Option<[f64; 2]>,
    // The window opened this tick (NCP Burst starts from the hit).
    fresh: bool,
    heights: Vec<f64>,
    base: [f64; 3],
    broken: bool,
    fails: u32,
    // When the next try may start; why the tries in a row failed; the fall the try under way tells, and the one a
    // Burst got a hurt from.
    retry_at: u64,
    reasons: Vec<&'static str>,
    try_top: f64,
    burst_top: Option<f64>,
    health: Option<f32>,
    hurts: u32,
    windows: u32,
    self_damages: u32,
}

static DAMAGE: Mutex<Option<Damage>> = Mutex::new(None);
// Whether Flight may fly this tick: Damage fly Off, or its window open. The move packet's false ground and anti kick
// read it.
static OPEN: AtomicBool = AtomicBool::new(true);
// A Spread self damage holds you still: Speed and the like wait.
static SELF_FALLING: AtomicBool = AtomicBool::new(false);

fn damage<R>(f: impl FnOnce(&mut Damage) -> R) -> R {
    f(DAMAGE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Damage::default))
}

pub(super) fn damage_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

// A Spread self damage is under way: movement modules leave you be.
pub(super) fn self_falling() -> bool {
    SELF_FALLING.load(Ordering::Relaxed)
}

// You were hurt (a DamageEvent for you).
pub(super) fn hurt() {
    damage(|d| d.hurt = true);
}

// Your health as the server set it: lower than before is a hurt too (a server that sends no DamageEvent).
pub(super) fn health(now: f32) {
    damage(|d| {
        if d.health.is_some_and(|h| now < h - 1e-3) {
            d.hurt = true;
        }
        d.health = Some(now);
    });
}

// Velocity mode's carried knockback.
pub(super) fn carry() -> Option<[f64; 2]> {
    damage(|d| d.carry)
}

// This tick's move as a self damage tells it: the height sent and whether it lands.
pub(super) fn fall_move() -> Option<(f64, bool)> {
    damage(|d| match d.phase {
        Wait::Falling(t) if t < d.heights.len() => Some((d.base[1] + d.heights[t], false)),
        Wait::Falling(_) => Some((d.base[1], true)),
        _ => None,
    })
}

// Flight off: the next turning on starts from waiting, with self damage's tries back.
pub(super) fn damage_reset() {
    damage(|d| {
        let (hurts, windows, self_damages, health, burst_top) = (d.hurts, d.windows, d.self_damages, d.health, d.burst_top);
        *d = Damage { hurts, windows, self_damages, health, burst_top, ..Damage::default() };
    });
    OPEN.store(true, Ordering::Relaxed);
    SELF_FALLING.store(false, Ordering::Relaxed);
}

// Whether a self damage may start now, and the safe fall distance: on the ground, out of water, above Self damage
// health, nothing of an earlier fall still counted, and room above for a climb of `climb` blocks.
fn self_damage_safe(game: &mut Game<'_>, m: &Module, climb: f64) -> Option<f64> {
    if game.creative || game.spectator() || !super::sunset::fall_clean() {
        return None;
    }
    let p = game.player.as_mut()?;
    if !p.ground.on_ground || p.flying || p.fall_flying || p.fluids.was_touching_water || p.health.health() <= m.n("Self damage health") as f32 {
        return None;
    }
    let safe = p.attribute("minecraft:safe_fall_distance").ok().unwrap_or(3.0);
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    let (from, to) = ((y + 1.8).floor() as i32, (y + 1.8 + climb).floor() as i32);
    let mut columns: Vec<[i32; 2]> = [(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)]
        .iter()
        .map(|(dx, dz)| [(x + dx).floor() as i32, (z + dz).floor() as i32])
        .collect();
    columns.dedup();
    let clear = columns.iter().all(|[cx, cz]| (from..=to).all(|by| super::sunset_util::is_air(game, [*cx, by, *cz])));
    clear.then_some(safe)
}

// Damage fly's tick, before the game's moves the player (with Flight on): whether Flight flies this tick. It stands you
// still through a self damage, starts the window's flight from the knockback, and turns Flight off when it is spent.
pub(super) fn damage_tick(game: &mut Game<'_>, m: &Module) -> bool {
    let trigger = Trigger::of(m.p("Damage fly"));
    if trigger == Trigger::Off {
        if damage(|d| d.trigger != Trigger::Off) {
            damage_reset();
        }
        damage(|d| d.knock = None);
        OPEN.store(true, Ordering::Relaxed);
        return true;
    }
    if damage(|d| d.trigger != trigger) {
        damage_reset();
        damage(|d| d.trigger = trigger);
    }
    let cfg = DamageCfg { trigger, window: m.n("Damage time (ticks)") as u32, again: m.p("After damage") == "Wait again" };
    let burst = m.p("Self damage method") != "Spread";
    let now = game.ticks;
    let (prev, len, base, fails, retry_at, burst_top) = damage(|d| (d.phase, d.heights.len(), d.base, d.fails, d.retry_at, d.burst_top));
    // Not with Blink holding moves: these would pass what it holds.
    let ready = trigger == Trigger::SelfDamage && prev == Wait::Armed && fails < MOST_FAILS && now >= retry_at
        && !super::sunset::with(|s| s.is_on("Blink"));
    let top = ready
        .then(|| self_damage_safe(game, m, if burst { BURST_STEP } else { 3.0 + FALL_EXTRA }))
        .flatten()
        .map(|safe| match (burst, burst_top) {
            (true, Some(top)) => top,
            (true, None) if fails == 0 => safe + 0.1,
            _ => safe + FALL_EXTRA,
        });
    let here = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]);
    let falling = matches!(prev, Wait::Falling(t) if t < len);
    let moved = falling && here.is_none_or(|h| (0..3).any(|i| (h[i] - base[i]).abs() > 0.05));
    let (next, gave_up, send) = damage(|d| {
        let hurt = std::mem::take(&mut d.hurt);
        d.owed |= falling && hurt;
        let hurt = !falling && (hurt || std::mem::take(&mut d.owed));
        let setback = std::mem::take(&mut d.broken);
        let broken = falling && (setback || moved);
        let mut next = wait_tick(d.phase, cfg, hurt, top.is_some(), broken, len);
        d.hurts += u32::from(hurt);
        d.fresh = matches!(next, Wait::Open(_)) && !matches!(d.phase, Wait::Open(_));
        d.windows += u32::from(d.fresh);
        let failed = next == Wait::Armed && matches!(d.phase, Wait::Falling(_) | Wait::Landed(_));
        if failed {
            d.fails += 1;
            d.retry_at = now + RETRY_WAIT;
            d.reasons.push(match d.phase {
                Wait::Landed(_) => "no hurt",
                _ if setback => "setback",
                _ => "moved",
            });
        }
        if matches!(next, Wait::Open(_)) {
            // A Burst that brought it: its height is the one to use from now on.
            if burst && matches!(d.phase, Wait::Landed(_)) {
                d.burst_top = Some(d.try_top);
            }
            d.fails = 0;
            d.reasons.clear();
        } else {
            d.carry = None;
            d.knock = None;
        }
        let mut send = None;
        if next == Wait::Falling(0)
            && let (Some(top), Some(h)) = (top, here)
        {
            d.self_damages += 1;
            d.try_top = top;
            if burst {
                // All of it this tick: next, the wait for the hurt.
                send = Some((h, top));
                next = Wait::Landed(0);
            } else {
                d.heights = fall_heights(top);
                d.base = h;
            }
        }
        d.phase = next;
        let gave_up = (failed && d.fails == MOST_FAILS).then(|| d.reasons.join(", "));
        (next, gave_up, send)
    });
    if let Some(reasons) = gave_up {
        super::sunset_util::report(game, &format!("Flight: self damage brought no hurt three times ({reasons}); it waits for damage instead."));
    }
    if let Some((at, top)) = send {
        use horizon_protocol::packets::play::{self, Serverbound as S};
        // Past Sunset's own changes to moves (the look, the ground, the fall it keeps): these are the fall as it is.
        super::sunset_gui_move::direct(|| {
            for (dy, ground) in burst_moves(top, BURST_STEP) {
                game.send(S::MovePlayerPos(play::MovePlayerPos { x: at[0], y: at[1] + dy, z: at[2], flags: u8::from(ground) }));
            }
        });
    }
    match next {
        Wait::Falling(_) => {
            // Still through the fall: no keys, no speed across, and a position in every move packet.
            horizon_client::motion::sunset_motion::set_freeze(true);
            if let Some(p) = game.player.as_mut() {
                p.velocity.x = 0.0;
                p.velocity.z = 0.0;
            }
            horizon_client::player_sync::sunset_sync::remind();
        }
        Wait::Open(_) => match damage(|d| d.knock.take()) {
            Some(knock) => {
                let toward = (m.p("Knockback") == "Redirect").then(|| heading(game));
                let v = kick(knock, m.n("Damage boost"), toward);
                if let Some(p) = game.player.as_mut() {
                    p.velocity = horizon_core::geometry::Vec3::new(v[0], v[1], v[2]);
                }
                damage(|d| d.carry = Some([v[0], v[2]]));
            }
            None => damage(|d| d.carry = d.carry.map(|[x, z]| [x * 0.91, z * 0.91]).filter(|[x, z]| x.hypot(*z) > 0.01)),
        },
        Wait::Spent => {
            super::sunset::toggle_named(game, "Flight", false);
            super::sunset_util::report(game, "Flight off: its damage time is over.");
        }
        _ => {}
    }
    let open = matches!(next, Wait::Open(_));
    OPEN.store(open, Ordering::Relaxed);
    SELF_FALLING.store(matches!(next, Wait::Falling(_)), Ordering::Relaxed);
    open
}

// The Debug Report's line.
pub(super) fn report(out: &mut String) {
    use std::fmt::Write;
    let (strategy, phase, ticks, speed, recover, corrections, knocks, bursts) =
        state(|s| (s.strategy, s.phase, s.ticks, s.speed, s.recover, s.corrections, s.knocks, s.bursts));
    if let Some(strategy) = strategy {
        let _ = writeln!(out, "- Flight strategy: {strategy:?}, {phase:?} for {ticks} ticks, speed {speed:.3}, recovery {recover}, \
            {corrections} setbacks, {knocks} knockbacks, {bursts} bursts");
    }
    let (trigger, wait, hurts, windows, self_damages, fails) = damage(|d| (d.trigger, d.phase, d.hurts, d.windows, d.self_damages, d.fails));
    if trigger != Trigger::Off {
        let _ = writeln!(out, "- Flight damage fly: {trigger:?}, {wait:?}, {hurts} hurts, {windows} windows, {self_damages} self damages \
            ({fails} failed in a row)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_ncp_offset_alternates_around_its_raise() {
        let mut rng = 7;
        for t in 0..200u64 {
            let o = old_ncp_offset(t, &mut rng);
            if t % 2 == 0 {
                assert!(o > 1e-5 && o < 2e-5, "{t}: {o}");
            } else {
                assert!(o < 1e-5 && o > 0.0, "{t}: {o}");
            }
        }
        assert_eq!(Strategy::of("Old NCP"), Some(Strategy::OldNcp));
        assert!(experimental("Old NCP"));
    }

    #[test]
    fn hover_offset_is_small_and_never_negative() {
        let mut rng = 7;
        for t in 0..200 {
            let o = hover_offset(t, &mut rng);
            if t % 2 == 1 {
                assert_eq!(o, 1e-5);
            } else {
                assert!((1e-5 + 1e-10..=2e-5).contains(&o), "{o}");
            }
        }
    }

    #[test]
    fn a_burst_waits_for_room_then_decays_and_ends() {
        let cfg = BurstCfg { speed: 9.0, friction: 0.935, repeat: false };
        // No room above: it waits.
        assert_eq!(burst_tick(Phase::Idle, 0, 0.0, 0, true, false, 0.0, cfg).0, Phase::Preparing);
        let (phase, speed, out) = burst_tick(Phase::Preparing, 0, 0.0, 0, true, true, 0.0, cfg);
        assert_eq!((phase, speed, out.jump, out.horizontal), (Phase::Active, 9.0, true, Some(9.0)));
        // Each tick 0.935 of the last, the fall softened.
        let (phase, speed, out) = burst_tick(Phase::Active, 1, 9.0, 1, false, true, -0.2, cfg);
        assert_eq!(phase, Phase::Active);
        assert!((speed - 8.415).abs() < 1e-9);
        assert_eq!(out.vertical, Some(-0.05));
        // It ends after 20 ticks, then waits to land; without Burst repeat no second one.
        assert_eq!(burst_tick(Phase::Active, 20, 3.0, 1, false, true, 0.0, cfg).0, Phase::Recovery);
        assert_eq!(burst_tick(Phase::Recovery, 0, 0.0, 1, false, true, 0.0, cfg).0, Phase::Recovery);
        assert_eq!(burst_tick(Phase::Recovery, 0, 0.0, 1, true, true, 0.0, cfg).0, Phase::Idle);
        assert_eq!(burst_tick(Phase::Idle, 0, 0.0, 1, true, true, 0.0, cfg).0, Phase::Idle);
        assert_eq!(burst_tick(Phase::Idle, 0, 0.0, 1, true, true, 0.0, BurstCfg { repeat: true, ..cfg }).0, Phase::Active);
    }

    #[test]
    fn bounce_saves_stops_and_gives_back() {
        let v = [0.3, 0.1, -0.2];
        let (out, saved) = bounce_tick(0, 4, 1.5, v, None);
        assert_eq!((out, saved), (v, Some(v)));
        assert_eq!(bounce_tick(1, 4, 1.5, [9.0; 3], saved).0, [0.0; 3]);
        assert_eq!(bounce_tick(2, 4, 1.5, [9.0; 3], saved).0, [0.0; 3]);
        let (out, saved) = bounce_tick(3, 4, 1.5, [0.0; 3], saved);
        assert!((out[0] - 0.45).abs() < 1e-9 && (out[1] - 0.15).abs() < 1e-9 && (out[2] + 0.3).abs() < 1e-9);
        assert_eq!(saved, None);
        // Upward never past a jump.
        assert_eq!(bounce_tick(3, 4, 2.0, [0.0; 3], Some([0.0, 0.4, 0.0])).0[1], 0.42);
    }

    #[test]
    fn damage_fly_waits_flies_its_time_and_waits_again_or_ends() {
        let cfg = DamageCfg { trigger: Trigger::Wait, window: 3, again: true };
        assert_eq!(wait_tick(Wait::Armed, cfg, false, true, false, 0), Wait::Armed, "Wait never deals itself damage");
        let mut phase = wait_tick(Wait::Armed, cfg, true, false, false, 0);
        let mut flown = 0;
        while let Wait::Open(_) = phase {
            flown += 1;
            phase = wait_tick(phase, cfg, flown == 2, false, false, 0);
            if flown == 2 {
                assert_eq!(phase, Wait::Open(3), "another hurt starts the time again");
            }
        }
        assert_eq!((flown, phase), (5, Wait::Armed));
        let once = DamageCfg { again: false, ..cfg };
        assert_eq!(wait_tick(Wait::Open(1), once, false, false, false, 0), Wait::Spent);
        assert_eq!(wait_tick(Wait::Spent, once, true, false, false, 0), Wait::Spent, "spent stays spent until Flight is off");
    }

    #[test]
    fn self_damage_falls_lands_then_waits_for_the_hurt() {
        let cfg = DamageCfg { trigger: Trigger::SelfDamage, window: 20, again: true };
        assert_eq!(wait_tick(Wait::Armed, cfg, false, false, false, 0), Wait::Armed, "not without room or health");
        let heights = fall_heights(4.05);
        let mut phase = wait_tick(Wait::Armed, cfg, false, true, false, heights.len());
        let mut ticks = 0;
        while let Wait::Falling(_) = phase {
            ticks += 1;
            phase = wait_tick(phase, cfg, false, true, false, heights.len());
        }
        assert_eq!((ticks, phase), (heights.len() + 1, Wait::Landed(0)), "each height, then the landing");
        assert_eq!(wait_tick(Wait::Landed(3), cfg, true, false, false, 0), Wait::Open(20));
        assert_eq!(wait_tick(Wait::Landed(LANDED_WAIT), cfg, false, true, false, 0), Wait::Armed, "no hurt: a failed try");
        assert_eq!(wait_tick(Wait::Falling(4), cfg, false, true, true, heights.len()), Wait::Armed, "a setback ends it");
        // The fall: never more than a jump up, gravity down, and the server counts all of it.
        let top = heights.iter().copied().fold(0.0, f64::max);
        assert!((top - 4.05).abs() < 1e-9 && *heights.last().unwrap() == 0.0, "{heights:?}");
        let mut last = 0.0;
        let mut counted = 0.0;
        for h in &heights {
            assert!(h - last <= 0.42 + 1e-9, "{heights:?}");
            counted += (last - h).max(0.0);
            last = *h;
        }
        assert!((counted - 4.05).abs() < 1e-9);
        assert_eq!((counted + 1e-6 - 3.0f64).floor(), 1.0, "one point of fall damage at the default safe distance");
        assert!(heights.len() <= 24, "about a second: {}", heights.len());
    }

    #[test]
    fn a_burst_stays_under_bukkits_move_event_and_adds_up_the_fall() {
        for top in [3.1, 4.05] {
            let moves = burst_moves(top, BURST_STEP);
            assert_eq!(moves.last(), Some(&(0.0, true)), "it ends on the ground");
            assert!(moves[..moves.len() - 1].iter().all(|m| !m.1), "everything before is off the ground");
            let (mut last, mut counted) = (0.0, 0.0);
            for &(h, _) in &moves {
                let d: f64 = h - last;
                assert!(d * d < 1.0 / 256.0, "{d}: a move Bukkit would report");
                counted += (last - h).max(0.0);
                last = h;
            }
            assert!(counted >= top && counted < top + BURST_STEP, "{counted}");
        }
        // 1.8: ceil(fall - 3) is half a heart just past 3; modern servers want a fall past 4.
        assert_eq!((3.1f64 - 3.0).ceil(), 1.0);
        assert_eq!((3.1f64 + 1e-6 - 3.0).floor(), 0.0);
        assert!(burst_moves(4.05, BURST_STEP).len() < 200);
    }

    #[test]
    fn the_knockback_is_kept_turned_and_scaled() {
        let knock = [0.3, 0.36, 0.4];
        let kept = kick(knock, 1.0, None);
        assert!((0..3).all(|i| (kept[i] - knock[i]).abs() < 1e-12), "Keep with a boost of 1 is the knockback as sent: {kept:?}");
        let v = kick(knock, 2.0, Some([0.0, -1.0]));
        assert!(v[0].abs() < 1e-12 && (v[2] + 1.0).abs() < 1e-12 && v[1] == 0.36, "{v:?}");
        assert_eq!(kick([0.0, 0.4, 0.0], 3.0, None), [0.0, 0.4, 0.0]);
        assert_eq!(kick(knock, 0.0, None)[0], 0.0);
        assert_eq!(Trigger::of("Self damage"), Trigger::SelfDamage);
        assert_eq!(Trigger::of("Off"), Trigger::Off);
    }
}
