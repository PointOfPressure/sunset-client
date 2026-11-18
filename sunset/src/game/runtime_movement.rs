// Runtime part (see runtime.rs): velocity, Flight, strafe, Bhop and Speed, and pushes.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::atomic::Ordering;
use winit::keyboard::KeyCode;
use super::sunset::*;

pub(super) fn remember_velocity(game: &Game<'_>) {
    let v = game.player.as_ref().map(|p| [p.velocity.x, p.velocity.y, p.velocity.z]);
    with(|s| s.velocity = v);
}

// A correction replaces the player's position: Speed's strafe and Scaffold's waits start again. (Velocity's baseline
// stays: Horizon's server can bring a hit's knockback with a position packet, which Velocity has to see.)
pub(super) fn server_correction(game: &mut Game<'_>) {
    super::sunset_lag::correction(game);
    if !game.native_on(ID) { return; }
    // Flight: a setback is the server refusing where Flight took you; pushing on from there is what loops.
    if let Some((pause, disable)) = with(|s| s.module("Flight").filter(|m| m.on).map(|m| (m.n("Setback pause (ticks)") as u32, m.b("Disable on setback")))) {
        if disable {
            toggle_named(game, "Flight", false);
            say(game, "Flight off: the server set you back.");
        } else {
            with(|s| s.fly_setback = pause);
            super::sunset_flight::correction(pause);
        }
    }
    super::sunset_render::no_slow_reset();
    super::sunset_parity::correction(game);
    let disable_speed = with(|s| {
        s.knockback_ticks = 0;
        s.strafe = Strafe::default();
        s.bhop.reset();
        s.ncp.recover();
        s.ncp_hop.teleported();
        s.watchdog.reset(); // A server correction owns its new velocity; never restore old launch momentum here.
        s.scaffold_stack = None;
        s.module("Speed").is_some_and(|m| m.on && matches!(m.p("Mode"), "NCP" | "Watchdog"))
            && s.legacy_recovery.correction(game.ticks)
    });
    if disable_speed {
        toggle_named(game, "Speed", false);
        say(game, "Speed off: repeated server corrections in an experimental legacy mode.");
    }
    super::sunset_scaffold_normal::correction();
    super::sunset_scaffold_telly::correction();
    ::sunset_core::motion::set_across(None);
    ::sunset_core::motion::set_up(None);
    ::sunset_core::motion::set_strafe_dir(None);
}

fn velocity_share(m: &Module, name: &str) -> f64 {
    let blend = if m.p("Mode") == "Blend" { m.n("Blend %") } else { 100.0 };
    super::sunset_movement::velocity_factor(m.n(name) * 100.0, blend)
}

pub(super) fn knockback_shares(m: &Module, ground: bool) -> [f64; 2] {
    let (h, v) = if !ground && m.b("Use air values") {
        ("Air knockback horizontal", "Air knockback vertical")
    } else {
        ("Knockback horizontal", "Knockback vertical")
    };
    [velocity_share(m, h), velocity_share(m, v)]
}

// Flight (Sunset's) before the tick: hovering after a fall of three blocks, nudge the yaw a degree either way so the
// look the server sees moves (put back after the tick); in Velocity mode set the speed the air gives you, and with
// No sneak let go of sneaking (its key still takes you down).
pub(super) fn flight_pre(game: &mut Game<'_>, m: Option<&Module>) {
    use ::sunset_core::motion as motion;
    let Some(m) = m else {
        super::sunset_flight::stop(game);
        motion::set_flying_speed(None);
        let (delay, off) = (20, 1);
        with(|s| {
            (s.fly_delay, s.fly_off) = (delay, off);
            if std::mem::take(&mut s.unsneaked) && game.physical_keys.contains(&KeyCode::ShiftLeft) {
                game.keys.insert(KeyCode::ShiftLeft);
            }
        });
        FLY_DOWN.store(false, Ordering::Relaxed);
        FLY_LAST_Y.store(u64::MAX, Ordering::Relaxed);
        return;
    };
    if super::sunset_flight::experimental(m.p("Mode")) {
        motion::set_flying_speed(None);
        // Old NCP sets the move across itself (over Speed's), at the 1.8 strafe's allowed distance.
        if m.p("Mode") == "Old NCP" {
            motion::set_across(super::sunset_flight::old_ncp_across(game));
        }
        return;
    }
    super::sunset_flight::stop(game);
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let yaw = game.yaw;
    let still = p.velocity.x.hypot(p.velocity.y).hypot(p.velocity.z) < 0.003;
    let (fallen, sprinting) = (p.fall_distance, p.sprinting);
    let nudge = with(|s| {
        let nudge = (fallen >= 3.0 && yaw == s.fly_last_yaw && still).then(|| if s.fly_flip { 1.0 } else { -1.0 });
        if nudge.is_some() {
            s.fly_flip = !s.fly_flip;
        }
        s.fly_last_yaw = yaw;
        nudge
    });
    if let Some(d) = nudge {
        game.yaw = yaw + d;
    }
    let velocity = m.p("Mode") == "Velocity";
    let speed = m.n("Speed") as f32;
    motion::set_flying_speed(velocity.then(|| speed * if sprinting { 15.0 } else { 10.0 }));
    if velocity && m.b("No sneak") && game.keys.remove(&KeyCode::ShiftLeft) {
        with(|s| s.unsneaked = true);
    }
}

// Flight after the tick: Sunset's anti kick (Packet: every `Delay` ticks the next move packets sink a little for
// `Off time` ticks, Normal: the flying stops for those ticks), the yaw put back, then the mode: Abilities flies as
// creative does at Speed; Velocity holds you still but for the jump and sneak keys, and Speed sets the air's pull.
// A tick of Flight's setback pause: true while it holds still, counting down.
pub(super) fn fly_paused(left: &mut u32) -> bool {
    let paused = *left > 0;
    *left = left.saturating_sub(1);
    paused
}

pub(super) fn flight_post(game: &mut Game<'_>, m: Option<&Module>, jump: bool, sneak: bool, screen_dir: Option<[f64; 2]>) {
    let Some(m) = m else {
        return;
    };
    if super::sunset_flight::experimental(m.p("Mode")) {
        super::sunset_flight::tick(game, m);
        // Old NCP: a move packet every tick, as a 1.8 client sends one, so each carries its height offset. The game
        // sends none on a tick you neither moved nor turned; this one goes then (never a second in a tick).
        if m.p("Mode") == "Old NCP"
            && !MOVED_THIS_TICK.load(Ordering::Relaxed)
            && game.remote.is_some()
            && let Some(p) = game.player.as_ref()
        {
            let (x, y, z) = (p.position.x, p.position.y, p.position.z);
            let flags = u8::from(p.ground.on_ground) | (u8::from(p.collision.horizontal) << 1);
            game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }));
        }
        return;
    }
    let (velocity, kick) = (m.p("Mode") == "Velocity", m.p("Anti kick mode"));
    let (delay, off_time) = (m.n("Delay") as i32, m.n("Off time") as i32);
    let (rest, remind, last_yaw) = with(|s| {
        let (mut rest, mut remind) = (false, false);
        if s.fly_delay > 0 {
            s.fly_delay -= 1;
        }
        if s.fly_off <= 0 && s.fly_delay <= 0 {
            (s.fly_delay, s.fly_off) = (delay, off_time);
            remind = kick == "Packet";
        } else if s.fly_delay <= 0 {
            rest = kick == "Normal" && !velocity;
            remind = kick == "Packet" && s.fly_off == off_time;
            s.fly_off -= 1;
        }
        FLY_DOWN.store(kick == "Packet" && s.fly_delay <= 0, Ordering::Relaxed);
        (rest, remind, s.fly_last_yaw)
    });
    if remind {
        ::sunset_core::sync::remind();
    }
    let (creative, spectator) = (game.creative, game.spectator());
    let paused = with(|s| fly_paused(&mut s.fly_setback));
    let Some(p) = game.player.as_mut() else {
        return;
    };
    if paused {
        // Held still where the server put you: no push, and vanilla's flying speed.
        if velocity {
            p.velocity = horizon_core::geometry::Vec3::new(0.0, 0.0, 0.0);
        } else {
            p.flying_speed = 0.05;
        }
        return;
    }
    if rest {
        p.flying = false;
        p.flying_speed = 0.05;
        p.may_fly = creative || spectator;
        return;
    }
    game.yaw = last_yaw;
    let speed = m.n("Speed");
    if velocity {
        p.flying = false;
        let up = speed * if m.b("Vertical speed match") { 10.0 } else { 5.0 };
        let down = sneak || game.physical_keys.contains(&KeyCode::ShiftLeft);
        p.velocity = horizon_core::geometry::Vec3::new(0.0, f64::from(u8::from(jump)) * up - f64::from(u8::from(down)) * up, 0.0);
        // Damage fly: the hit's knockback carried on across the ground.
        if let Some([x, z]) = super::sunset_flight::carry() {
            (p.velocity.x, p.velocity.z) = (x, z);
        }
        // GUI Move: behind a menu the game takes no move keys, so the pull they would give is set by hand.
        if let Some([x, z]) = screen_dir {
            let pull = speed * if p.sprinting { 15.0 } else { 10.0 };
            (p.velocity.x, p.velocity.z) = (x * pull, z * pull);
        }
        if m.b("No sneak") {
            p.ground.on_ground = false;
        }
    } else if !spectator {
        p.flying_speed = speed as f32;
        p.flying = true;
        p.may_fly = true;
    }
}

// Speed's Strafe mode between moves: Sunset's stage, the last tick's distance, the speed, and the speed limit's clock.
#[derive(Default)]
pub(super) struct Strafe {
    pub(super) stage: u8,
    pub(super) distance: f64,
    pub(super) speed: f64,
    since: Option<std::time::Instant>,
}

// Speed's Bhop mode between ticks: the speed it carries (blocks a tick), the ticks in the air since its jump (None: on
// the ground), the ticks on the ground since the last landing, hops in a row (Hop gain) and whether the last one was
// a hop, a knockback's speed for Damage boost, whether the last tick was still rising (Fast fall's top), and whether
// Hop height's jump modifier is on the player.
#[derive(Default)]
pub(super) struct Bhop {
    pub(super) speed: f64,
    air: Option<u32>,
    pub(super) ground: u32,
    pub(super) chain: u32,
    hopped: bool,
    kick: f64,
    rising: bool,
    jump_set: bool,
}

impl Bhop {
    // Back to standing (Speed off, a correction, Panic); Hop height's modifier is taken off by bhop_body.
    pub(super) fn reset(&mut self) {
        *self = Bhop { jump_set: self.jump_set, ..Bhop::default() };
    }
}

// Speed's Bhop: hops on the game's own jump each landing (Hop height's), and takes the move across the ground:
// Hop speed on the jump's tick (Hop gain more for each hop in a row, up to Hop max speed), Hop slope of the speed over
// the sprint-jump speed lost the next tick, then Air friction each tick in the air, never under the sprint-jump speed
// (Speed's effect counted). It goes the way the move keys point, turned by Air control from the way you were going;
// a wall or a stop starts it again. Damage boost takes a knockback's speed when it is more. With no move key held,
// Stop on release stands you still.
pub(super) fn bhop(game: &Game<'_>, m: &Module, base: f64, scale: f64) -> Option<([f64; 2], Option<f64>)> {
    let p = game.player.as_ref()?;
    let Some(wish) = move_direction(game) else {
        with(|s| s.bhop.reset());
        return m.b("Stop on release").then_some(([0.0; 2], None));
    };
    let ground = p.ground.on_ground;
    // The jump goes this tick: on the ground with jump held (pressed by Bhop, or by you with Hop when Jump held).
    let jumping = ground && game.keys.raw(&KeyCode::Space);
    let (hop, gain, max) = (m.n("Hop speed") / 20.0, m.n("Hop gain (%)") / 100.0, m.n("Hop max speed") / 20.0);
    let (slope, friction) = (m.n("Hop slope").clamp(0.0, 1.0), (m.n("Air friction (%)") / 100.0).clamp(0.0, 1.0));
    let boost = m.b("Damage boost");
    let wall = p.collision.horizontal;
    let speed = with(|s| {
        let b = &mut s.bhop;
        if wall {
            b.speed = base;
            b.chain = 0;
        }
        if jumping {
            b.chain = if b.hopped && b.ground <= 2 { b.chain + 1 } else { 0 };
            let top = if gain > 0.0 { max.max(hop) } else { hop };
            b.speed = (hop * (1.0 + gain * f64::from(b.chain))).min(top).max(base);
            b.air = Some(0);
            b.ground = 0;
            b.hopped = true;
        } else if ground {
            b.ground = b.ground.saturating_add(1);
            if b.ground > 2 {
                b.chain = 0;
                b.hopped = false;
            }
            b.air = None;
            b.speed = base;
        } else {
            let n = b.air.map_or(1, |a| a.saturating_add(1));
            b.air = Some(n);
            if n == 1 {
                b.speed -= slope * (b.speed - base);
            } else {
                b.speed -= b.speed * friction;
            }
            b.speed = b.speed.max(base);
        }
        if boost && b.kick > 0.0 {
            b.speed = b.speed.max(b.kick.min(max.max(hop)));
        }
        b.kick = 0.0;
        b.speed
    });
    // Air control: on the ground the keys' way at once; in the air turned that share from the way you were going.
    let was = [p.position.x - game.previous_position.x, p.position.z - game.previous_position.z];
    let was_len = was[0].hypot(was[1]);
    let control = m.n("Air control").clamp(0.0, 1.0);
    let dir = if ground || control >= 1.0 || was_len < 1e-4 {
        wish
    } else {
        let mix = [was[0] / was_len * (1.0 - control) + wish[0] * control, was[1] / was_len * (1.0 - control) + wish[1] * control];
        let len = mix[0].hypot(mix[1]);
        if len < 1e-6 { wish } else { [mix[0] / len, mix[1] / len] }
    };
    Some(([dir[0] * speed * scale, dir[1] * speed * scale], None))
}

// Bhop on the player itself, before the tick moves it: Hop height's jump (the game's jump strength scaled, so the
// jump is the game's own), no jump delay on the ground (a held jump hops again on landing), and Fast fall's pull down
// at the top of a hop. `m`: Speed in Bhop mode, on.
pub(super) fn bhop_body(game: &mut Game<'_>, m: Option<&Module>) {
    let moving = move_input(game) != (0.0, 0.0);
    let was = with(|s| s.bhop.jump_set);
    let Some(p) = game.player.as_mut() else {
        return;
    };
    let Some(m) = m else {
        if was {
            let _ = p.set_modifier("minecraft:jump_strength", "sunset:bhop", 0.0, "add_multiplied_total", false);
            with(|s| s.bhop.jump_set = false);
        }
        return;
    };
    let _ = p.set_modifier("minecraft:jump_strength", "sunset:bhop", m.n("Hop height") / 0.42 - 1.0, "add_multiplied_total", true);
    if p.ground.on_ground {
        p.no_jump_delay = 0;
    }
    let fast_fall = m.n("Fast fall");
    let rising = with(|s| std::mem::replace(&mut s.bhop.rising, p.velocity.y > 0.0));
    if fast_fall > 0.0 && moving && rising && !p.ground.on_ground && p.velocity.y <= 0.0 && !p.fall_flying && !p.flying {
        p.velocity.y -= fast_fall;
        with(|s| s.bhop.rising = false);
    }
    with(|s| s.bhop.jump_set = true);
}

// Bhop presses the game's jump this tick: on the ground with a move key held (Hop when Moving), where Speed moves you.
pub(super) fn bhop_hops(game: &Game<'_>, m: &Module) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    m.p("Hop when") == "Moving"
        && move_input(game) != (0.0, 0.0)
        && p.ground.on_ground
        && !p.flying
        && !p.fall_flying
        && game.riding.is_none()
        && (m.b("When sneaking") || !game.keys.raw(&KeyCode::ShiftLeft))
        && (m.b("In liquids") || !(p.fluids.was_touching_water || p.is_in_lava()))
        && !game.sunset_block_in_tag("minecraft:climbable", &game.block_name(feet))
}

// NCP's distinct controller; actual attributes include Speed/Slowness and server modifications. Remove only the
// native sprint multiplier before applying the historical walk/sprint baseline (do not count potion levels twice).
fn legacy_speed_move(game: &mut Game<'_>, m: &Module, item_scale: f64) -> Option<([f64; 2], Option<f64>)> {
    if !game.native_playing() || gui_open() || super::sunset_gui_move::inventory_busy()
        || with(|s| s.is_on("Scaffold"))
        || CRIT_STAGE.load(Ordering::Relaxed) != 0
    {
        watchdog_stop(game, false);
        with(|s| s.ncp.recover());
        return None;
    }
    let direction = move_direction(game);
    let speed_level = game.potions.effects.amplifier("speed").map(|a| a.max(0) as u32);
    let p = game.player.as_mut()?;
    let native_speed = p.attribute("minecraft:movement_speed").ok()?;
    let base = if p.sprinting { 0.2873 * native_speed / 0.13 } else { 0.216 * native_speed / 0.1 };
    let frame = super::sunset_legacy_speed::Frame {
        tick: game.ticks,
        position: [p.position.x, p.position.z],
        grounded: p.ground.on_ground,
        blocked: p.collision.horizontal || (p.collision.vertical && !p.ground.on_ground),
        direction,
        base,
        jump_scale: if m.b("NCP reduced jump") { 0.4 / 0.42 } else { 1.0 },
        speed_effect: speed_level.is_some(),
    };
    let velocity = [p.velocity.x, p.velocity.z];
    if m.p("Mode") == "Watchdog" && (frame.direction.is_none() || frame.blocked) {
        watchdog_stop(game, false);
        return None;
    }
    let step = with(|s| {
        if m.p("Mode") == "NCP" { s.ncp.step(frame) }
        else { s.watchdog.step(frame, velocity, speed_level, m.b("Watchdog knockback boost")) }
    });
    if step.is_none() && m.p("Mode") == "Watchdog" && with(|s| s.watchdog.recovering()) {
        watchdog_stop(game, false);
    }
    let step = step?;
    // Item-use settings scale owned acceleration, not a server impulse already filtered by Velocity.
    let scale = if m.p("Mode") == "Watchdog" && step.jump_scale.is_none() { 1.0 } else { item_scale };
    Some(([step.across[0] * scale, step.across[1] * scale], step.jump_scale))
}

// The move keys and the place you are in as a 1.8 client has them, for the 1.8 strafe (legacy_speed::strafe_move):
// Speed's NCP Hop and Flight's Old NCP. An item in use slows the keys to 20% (No Slow's share instead), as 1.8 does.
pub(super) fn legacy_strafe_frame(
    game: &mut Game<'_>,
) -> Option<(super::sunset_legacy_speed::Input, super::sunset_legacy_speed::Allowance)> {
    use super::sunset_legacy_speed::{Allowance, Input};
    let held = |k: KeyCode| f64::from(u8::from(game.keys.raw(&k)));
    let (forward, left) = (held(KeyCode::KeyW) - held(KeyCode::KeyS), held(KeyCode::KeyA) - held(KeyCode::KeyD));
    let sneaking = game.keys.raw(&KeyCode::ShiftLeft);
    let no_slow = with(|s| s.module("No Slow").filter(|m| m.on).map(|m| [m.n("Ground %"), m.n("Air %")]));
    let using = game.using.is_some() || super::sunset_hands::legacy_using();
    let level = |id: &str| game.potions.effects.amplifier(id).map(|a| (a + 1).max(0) as u32);
    let (speed, slowness) = (level("speed"), level("slowness").is_some());
    let p = game.player.as_ref()?;
    let ground = p.ground.on_ground;
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let liquid = p.fluids.was_touching_water || p.is_in_lava();
    let web = [feet, [feet[0], feet[1] + 1, feet[2]]]
        .iter()
        .any(|&b| super::sunset_util::short(&super::sunset_util::facts(game, b).name) == "cobweb");
    let use_factor = if using { super::sunset_movement::item_use_factor(true, Some(no_slow.unwrap_or([20.0; 2])), ground) } else { 1.0 };
    let efficiency = game.player.as_mut()?.attribute("minecraft:water_movement_efficiency").unwrap_or(0.0);
    let depth_strider = (efficiency * 3.0).round().clamp(0.0, 3.0) as u32;
    Some((
        Input::of(forward, left, sneaking, use_factor, game.yaw),
        Allowance { web, liquid, depth_strider, sneaking, speed, slowness },
    ))
}

// Speed's NCP Hop (legacy_speed::NcpHop): this tick's move across, the hop's vertical speed written on the player
// before the tick moves it (the jump, Low hop's and Y-port's pull down).
pub(super) fn ncp_hop(game: &mut Game<'_>, m: &Module) -> Option<([f64; 2], Option<f64>)> {
    use super::sunset_legacy_speed::{HopCfg, HopFrame, allowed_distance, strafe_move};
    let (input, allowance) = legacy_strafe_frame(game)?;
    let jump_boost = game.potions.effects.amplifier("jump_boost").map_or(0, |a| (a + 1).max(0) as u32);
    let p = game.player.as_ref()?;
    let ground = p.ground.on_ground;
    let air = with(|s| {
        s.hop_air = if ground { 0 } else { s.hop_air.saturating_add(1) };
        s.hop_air
    });
    let hurt = p.health.hurt.hurt_time;
    let frame = HopFrame {
        air,
        moving: input.moving(),
        blocked: p.collision.horizontal,
        allowed: allowed_distance(allowance, input),
        vy: p.velocity.y,
        hurt: (hurt > 0).then(|| (10 - hurt).max(0) as u32),
    };
    let cfg = HopCfg {
        jump: m.n("NCP hop jump"),
        ground_speed: m.n("NCP hop ground speed"),
        slope: m.n("NCP hop slope"),
        custom_boost: m.b("NCP hop custom boost").then(|| (m.n("NCP hop boost speed"), m.n("NCP hop hurt ticks") as u32)),
        low_hop: m.b("NCP hop low hop"),
        y_port: m.b("NCP hop Y-port"),
        jump_boost,
    };
    // What the last move left (a 1.8 tick's friction on it), kept at a random 0 to 1/2000.
    let keep = super::sunset_rotation::Rng::new(game.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x4E43_5048).next() / 2000.0;
    let friction = if ground { 0.6 * 0.91 } else { 0.91 };
    let (out, residual) = with(|s| (s.ncp_hop.step(frame, cfg), [s.hop_last[0] * friction, s.hop_last[1] * friction]));
    if let (Some(vy), Some(p)) = (out.vy, game.player.as_mut()) {
        p.velocity.y = vy;
    }
    let across = strafe_move(out.speed, input, residual, keep);
    with(|s| s.hop_last = across);
    Some((across, None))
}

// Restore only surplus momentum created by Watchdog's own launch. Genuine server impulses relinquish this ownership.
pub(super) fn watchdog_stop(game: &mut Game<'_>, reset: bool) {
    let owned = with(|s| if reset { s.watchdog.reset() } else { s.watchdog.suspend() });
    if !owned { return; }
    let Some(p) = game.player.as_mut() else { return };
    let Ok(attribute) = p.attribute("minecraft:movement_speed") else { return };
    let base = if p.sprinting { 0.2873 * attribute / 0.13 } else { 0.216 * attribute / 0.1 };
    let magnitude = p.velocity.x.hypot(p.velocity.z);
    if base.is_finite() && base >= 0.0 && magnitude.is_finite() && magnitude > base {
        p.velocity.x *= base / magnitude;
        p.velocity.z *= base / magnitude;
    }
}

// Speed (Sunset's): this tick's own move across the ground, blocks a tick (and Strafe's hop up), or None to leave
// it to the game. Vanilla moves the setting's blocks a second the way the move keys point, and stands still without
// them; Strafe hops, faster at each jump, slowing in the air.
pub(super) fn speed(game: &mut Game<'_>, m: &Module) -> Option<([f64; 2], Option<f64>)> {
    if m.p("Mode") != "Watchdog" { watchdog_stop(game, true); }
    let p = game.player.as_ref()?;
    let held = |k: KeyCode| game.keys.raw(&k);
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let mode = m.p("Mode");
    if mode != "NCP" { with(|s| s.ncp.reset()); }
    if mode != "NCP Hop" { with(|s| { s.ncp_hop.reset(); s.hop_air = 0; }); }
    if matches!(mode, "NCP" | "Watchdog") && with(|s| s.legacy_recovery.waiting(game.ticks)) {
        watchdog_stop(game, false);
        with(|s| s.ncp.recover());
        return None;
    }
    if !matches!(mode, "NCP" | "Watchdog") { with(|s| s.legacy_recovery = Default::default()); }
    let vanilla = mode == "Vanilla";
    let (flight, no_slow, pause) = with(|s| {
        let pause = s.knockback_ticks;
        s.knockback_ticks = pause.saturating_sub(1);
        let no_slow = s.module("No Slow").filter(|m| m.on).map(|m| [m.n("Ground %"), m.n("Air %")]);
        (s.is_on("Flight") && (super::sunset_flight::damage_open() || super::sunset_flight::self_falling()), no_slow, pause)
    });
    let using = game.using.is_some() || super::sunset_hands::legacy_using();
    let slow_factor = super::sunset_movement::item_use_factor(
        using, no_slow, p.ground.on_ground,
    );
    if !super::sunset_movement::speed_can_write(
        flight || p.flying, p.fall_flying, using, no_slow.is_some(),
        pause, p.collision.horizontal, m.b("Stop at wall"),
    ) {
        if matches!(mode, "NCP" | "Watchdog") { watchdog_stop(game, false); with(|s| s.ncp.recover()); }
        return None;
    }
    let scale_across = |v: [f64; 2]| [v[0] * slow_factor, v[1] * slow_factor];
    let effect = |id: &str| game.potions.effects.amplifier(id).map_or(0.0, |a| f64::from(a + 1));
    // Sunset's stopSpeed.
    if p.fall_flying
        || game.sunset_block_in_tag("minecraft:climbable", &game.block_name(feet))
        || game.riding.is_some()
        || (!m.b("When sneaking") && held(KeyCode::ShiftLeft))
        || (vanilla && m.b("Only on ground") && !p.ground.on_ground)
        || (!m.b("In liquids") && (p.fluids.was_touching_water || p.is_in_lava()))
    {
        if matches!(mode, "NCP" | "Watchdog") { watchdog_stop(game, false); with(|s| s.ncp.recover()); }
        return None;
    }
    if matches!(mode, "NCP" | "Watchdog") {
        return legacy_speed_move(game, m, slow_factor);
    }
    if mode == "NCP Hop" {
        return ncp_hop(game, m);
    }
    if mode == "Bhop" {
        let base = 0.2873 * (1.0 + 0.2 * effect("speed")) / (1.0 + 0.2 * effect("slowness"));
        return bhop(game, m, base, slow_factor);
    }
    if matches!(mode, "Smooth" | "Low Hop" | "Custom") {
        if m.b("Only on ground") && !p.ground.on_ground { return None; }
        let ground = p.ground.on_ground;
        let multiplier = if ground {
            if mode == "Custom" { m.n("Ground multiplier") } else { 1.0 }
        } else { m.n("Air multiplier") };
        let sprint = if p.sprinting { m.n("Sprint multiplier") } else { 1.0 };
        let target = m.n("Vanilla speed") / 20.0 * multiplier * sprint;
        let dir = move_direction(game);
        let up = if mode == "Low Hop" && ground && dir.is_some()
            && !held(KeyCode::Space) && !held(KeyCode::ShiftLeft)
        { Some(m.n("Jump height")) } else { None };
        let across = if mode == "Low Hop" {
            dir.map_or([0.0; 2], |d| {
                let speed = target.min(m.n("Max speed"));
                [d[0] * speed, d[1] * speed]
            })
        } else {
            super::sunset_movement::steer(
                [p.velocity.x, p.velocity.z], dir, target, m.n("Acceleration"),
                m.n("Deceleration"), m.n("Strafe strength"), m.n("Max speed"),
            )
        };
        return Some((scale_across(across), up));
    }
    let yaw = f64::from(game.yaw).to_radians();
    if vanilla {
        // Horizontal velocity from the keys, with the Speed effect's boost.
        let (forward, right) = ([-yaw.sin(), yaw.cos()], [-yaw.cos(), -yaw.sin()]);
        let f = f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS)));
        let r = f64::from(i8::from(held(KeyCode::KeyD)) - i8::from(held(KeyCode::KeyA)));
        let diagonal = if (held(KeyCode::KeyW) || held(KeyCode::KeyS)) && (held(KeyCode::KeyA) || held(KeyCode::KeyD)) { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
        let k = m.n("Vanilla speed") / 20.0 * diagonal * (1.0 + effect("speed") * 0.205);
        return Some((scale_across([(forward[0] * f + right[0] * r) * k, (forward[1] * f + right[1] * r) * k]), None));
    }
    let base = 0.2873 * (1.0 + 0.2 * effect("speed")) / (1.0 + 0.2 * effect("slowness"));
    let forward = f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS)));
    let side = f64::from(i8::from(held(KeyCode::KeyA)) - i8::from(held(KeyCode::KeyD)));
    let moving = forward != 0.0 || side != 0.0;
    let distance = (p.position.x - game.previous_position.x).hypot(p.position.z - game.previous_position.z);
    let (ground, landing) = (p.ground.on_ground, p.ground.on_ground || p.collision.vertical);
    let (strafe_speed, limit) = (m.n("Strafe speed"), m.b("Speed limit"));
    let (speed, up) = with(|s| {
        let st = &mut s.strafe;
        st.distance = distance;
        let mut up = None;
        match st.stage {
            0 => {
                if moving {
                    st.stage = 1;
                    st.speed = 1.18 * base - 0.01;
                }
            }
            1 => {
                if moving && ground {
                    up = Some(0.40123128 + effect("jump_boost") * 0.1);
                    st.speed *= strafe_speed;
                    st.stage = 2;
                }
            }
            2 => {
                st.speed = st.distance - 0.76 * (st.distance - base);
                st.stage = 3;
            }
            _ => {
                if landing {
                    st.stage = 0;
                }
                st.speed = st.distance - st.distance / 159.0;
            }
        }
        st.speed = st.speed.max(base);
        if limit {
            let now = std::time::Instant::now();
            let since = *st.since.get_or_insert(now);
            if now.duration_since(since).as_millis() > 2500 {
                st.since = Some(now);
            }
            let held_for = now.duration_since(st.since.unwrap_or(now)).as_millis();
            st.speed = st.speed.min(if held_for > 1250 { 0.44 } else { 0.43 });
        }
        (st.speed, up)
    });
    // Strafe.transformStrafe: the speed the way the move keys point.
    if !moving {
        return Some((scale_across([0.0; 2]), up));
    }
    let mut turn = 90.0 * side.signum();
    if forward != 0.0 {
        turn *= forward.signum() * 0.5;
    }
    let mut dir = f64::from(game.yaw) - turn;
    if forward < 0.0 {
        dir -= 180.0;
    }
    let dir = dir.to_radians();
    Some((scale_across([-dir.sin() * speed, dir.cos() * speed]), up))
}

// An explosion packet's push on the player, as it arrives (before the game adds it).
pub(super) fn explosion_push(game: &mut Game<'_>, push: Option<[f64; 3]>) {
    if let Some(push) = push {
        with(|s| { s.watchdog.reset(); }); // Preserve the explosion; it is not a damage-steering event.
        super::sunset_render::no_slow_knocked();
        let shares = with(|s| {
            s.knockback_ticks = s.module("Speed").filter(|m| m.on).map_or(0, |m| m.n("Knockback pause") as u32);
            s.module("Velocity").filter(|m| m.on && m.b("Explosions"))
                .map(|m| [velocity_share(m, "Explosions horizontal"), velocity_share(m, "Explosions vertical")])
        });
        ::sunset_core::motion::set_across(None);
        ::sunset_core::motion::set_up(None);
        if let (Some([h, v]), Some(p)) = (shares, game.player.as_mut()) {
            // Horizon adds the original push after this hook; this leaves the chosen share.
            p.velocity.x += push[0] * (h - 1.0);
            p.velocity.y += push[1] * (v - 1.0);
            p.velocity.z += push[2] * (h - 1.0);
            let after = [p.velocity.x + push[0], p.velocity.y + push[1], p.velocity.z + push[2]];
            with(|s| { s.velocity = Some(after); s.blast = [0.0; 3]; });
        } else {
            with(|s| (0..3).for_each(|i| s.blast[i] += push[i]));
        }
    }
}

// Velocity (Sunset's), before the tick moves the player: of what changed the velocity since the last tick, the
// explosions' pushes keep their share and the rest, the server's knockback, its own; flowing liquids push by their
// share (the flow hook), and of the entities pushing you only the push amount is left.
pub(super) fn velocity(game: &mut Game<'_>, m: Option<&Module>) {
    use ::sunset_core::motion as motion;
    let (last, blast) = with(|s| (s.velocity.take(), std::mem::take(&mut s.blast)));
    if let (Some(last), Some(p)) = (last, game.player.as_ref()) {
        let impulse = [p.velocity.x - last[0] - blast[0], p.velocity.y - last[1] - blast[1], p.velocity.z - last[2] - blast[2]];
        if game.remote.is_some() && impulse[0].hypot(impulse[2]).hypot(impulse[1]) > 0.08 {
            // Bhop's Damage boost takes the knockback's speed across the ground.
            let kick = impulse[0].hypot(impulse[2]);
            with(|s| {
                let (pause, bhop, hop) = s.module("Speed").filter(|m| m.on).map_or((0, false, false), |m| {
                    (m.n("Knockback pause") as u32, m.p("Mode") == "Bhop", m.p("Mode") == "NCP Hop" && m.b("Damage boost"))
                });
                s.knockback_ticks = if hop { 0 } else { pause };
                if bhop {
                    s.bhop.kick = s.bhop.kick.max(kick);
                }
                if hop {
                    s.ncp_hop.knocked(kick);
                }
            });
        }
    }
    let share = |on: &str, h: &str, v: &str| m.filter(|m| m.b(on)).map_or([1.0; 2], |m| [velocity_share(m, h), velocity_share(m, v)]);
    let liquids = m.filter(|m| m.b("Liquids")).map(|m| [velocity_share(m, "Liquids horizontal"), velocity_share(m, "Liquids vertical")]);
    motion::set_liquid_flow(liquids);
    let Some(m) = m else {
        return;
    };
    let pushed = if m.b("Entity push") { entity_push(game) } else { [0.0; 2] };
    let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    // Knockback chance: each knockback is cut with that chance, else taken whole.
    let roll = super::sunset_rotation::Rng::new(game.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x7E10).next() * 100.0;
    let knock =
        if m.b("Knockback") && roll < m.n("Knockback chance (%)") { knockback_shares(m, ground) } else { [1.0; 2] };
    with(|s| s.watchdog.filtered_knockback(game.ticks, knock[0]));
    let blasts = share("Explosions", "Explosions horizontal", "Explosions vertical");
    let Some(p) = game.player.as_mut() else {
        return;
    };
    let mut v = [p.velocity.x, p.velocity.y, p.velocity.z];
    if let Some(last) = last {
        for i in 0..3 {
            let (k, b) = if i == 1 { (knock[1], blasts[1]) } else { (knock[0], blasts[0]) };
            v[i] = last[i] + (v[i] - last[i] - blast[i]) * k + blast[i] * b;
        }
    }
    // The game adds the pushes later in the tick: take back what the push amount does not keep.
    let keep = velocity_share(m, "Entity push amount");
    v[0] -= pushed[0] * (1.0 - keep);
    v[2] -= pushed[1] * (1.0 - keep);
    (p.velocity.x, p.velocity.y, p.velocity.z) = (v[0], v[1], v[2]);
}

// The entities' push on the player this tick, x and z (the game's `LivingEntity.pushEntities` with
// `Entity.push(Entity)`): every living one but bats whose box overlaps yours, pushable by your team.
fn entity_push(game: &Game<'_>) -> [f64; 2] {
    let Some(p) = game.player.as_ref() else {
        return [0.0; 2];
    };
    let (bounds, own) = (p.bounds(), own_id(game));
    let team = game.score.board.players_team(&game.options.player_name());
    let mut push = [0.0; 2];
    for m in &game.mobs {
        if Some(m.id) == own
            || !m.alive()
            || m.species() == horizon_client::mobs::Species::Bat
            || game.riding.is_some_and(|v| v == m.id || m.extra.mount == Some(v))
            || !m.bounds().intersects(bounds)
            || !horizon_client::scoreboard::pushable_by(game.mob_team(m.id), team, true)
        {
            continue;
        }
        let (mut xa, mut za) = (m.position.x - p.position.x, m.position.z - p.position.z);
        let dd = xa.abs().max(za.abs());
        if dd < f64::from(0.01_f32) {
            continue;
        }
        let dd = dd.sqrt();
        let pow = (1.0 / dd).min(1.0) * f64::from(0.05_f32);
        xa = xa / dd * pow;
        za = za / dd * pow;
        push[0] -= xa;
        push[1] -= za;
    }
    push
}
