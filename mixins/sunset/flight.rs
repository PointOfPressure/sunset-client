// Flight's experimental strategies (NCP Hover, NCP Burst, NCP Bounce), each with its own state and lifecycle: Idle,
// Preparing (waiting for its start conditions), Active, Recovery (after a setback, real knockback or a burst's end:
// no override of its own until it settles). Abilities and Velocity stay in runtime.rs; these never take the false
// ground claim or the Packet anti kick those use (runtime.rs gates both on `experimental`).
//
// Written from how the old movement strategies are described, not from any client's code, and for testing against a
// specific NoCheatPlus build: no mode here is known to pass anything.
// - Hover: vertical speed held at zero, horizontal movement left as the game makes it, and the height sent raised by
//   1e-5 plus, every other tick, a random 1e-10 to 1e-5 (the offset is never negative).
// - Burst: on the ground with room above, a jump with a large horizontal impulse (Burst speed, blocks a tick) that
//   decays by Burst friction each tick; the fall is softened while it lasts; it ends falling, slowed, or after
//   20 ticks, then waits to land (Burst repeat for the next one).
// - Bounce: a cycle of Bounce period ticks: the movement is saved, held at a stop, then given back scaled by Bounce
//   boost (upward at most a jump's 0.42).

use super::Game;
use super::sunset::Module;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Strategy {
    Hover,
    Burst,
    Bounce,
}

impl Strategy {
    // The experimental strategy a Flight Mode names, if it names one.
    pub(super) fn of(mode: &str) -> Option<Self> {
        match mode {
            "NCP Hover" => Some(Self::Hover),
            "NCP Burst" => Some(Self::Burst),
            "NCP Bounce" => Some(Self::Bounce),
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
    // The height added to this tick's move packet (Hover).
    offset: Option<f64>,
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
        Strategy::Burst => {
            let feet = [position[0].floor() as i32, position[1].floor() as i32, position[2].floor() as i32];
            let room = (2..4).all(|dy| super::sunset_util::is_air(game, [feet[0], feet[1] + dy, feet[2]]));
            let cfg = BurstCfg { speed: m.n("Burst speed"), friction: m.n("Burst friction"), repeat: m.b("Burst repeat") };
            let (phase, ticks, speed, bursts) = state(|s| (s.phase, s.ticks, s.speed, s.bursts));
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

// The server set you back: hands off for `pause` ticks, nothing saved kept.
pub(super) fn correction(pause: u32) {
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

// Real knockback came: it is the server's movement, not to be held or replayed. Hands off for 10 ticks.
pub(super) fn knocked() {
    state(|s| {
        if s.strategy.is_some() {
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

// A new world or life: the player is new, so only the state goes.
pub(super) fn reset() {
    state(|s| {
        let (corrections, knocks, rng) = (s.corrections, s.knocks, s.rng);
        *s = State { corrections, knocks, rng, ..State::default() };
    });
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
