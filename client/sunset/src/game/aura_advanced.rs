// Kill Aura's Advanced rotation mode, after the behaviour of Rise's (written anew for Sunset). Two parts:
//
// - Where to aim (`aim_point`): the target's upper body, led by how the target moves relative to you (smoothed over a
//   few ticks), held where it was until a reaction time has passed or the target has moved far, then moved part of the
//   way. A human's eye lags and settles; so does this.
// - How the look gets there (`step`), a tick at a time from the look the server has, like a mouse driven by a hand:
//   the turn is pulled toward the point (Gravity) and pushed about by noise far from it (Wind, until Damped
//   distance), its speed capped (Max step, Min step), sped up now and then (bursts) and kept from crawling (Cruise
//   floor). A new target far off may be overshot and come back; inside the Deadzone, with the look on the target, it
//   holds still for Hold ticks; Flick guard caps any one tick's turn. Every turn is cut to whole mouse counts.
//
// The first hit on a new target also waits a reaction (`trigger_ready`).
use super::sunset::Module;
use super::sunset_rotation::{degrees_per_count, whole_counts};
use super::sunset_util::Target;

const SQRT3: f32 = 1.732_050_8;
const SQRT5: f32 = 2.236_068;

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

// The Advanced settings, read once a tick.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tuning {
    gravity: f32,
    wind: f32,
    damped: f32,
    max_step: f32,
    min_step: f32,
    overshoot_chance: f32,
    overshoot_scale: f32,
    overshoot_max: f32,
    gaussian: f32,
    accuracy: f32,
    prediction: f64,
    deadzone: f32,
    anchor: f64,
    hold_ticks: i32,
    cruise_floor: f32,
    pace_jitter: f32,
    burst_chance: f32,
    burst_strength: f32,
    flick_guard: bool,
    flick_max: f32,
    aim_reaction: f64,
    aim_jitter: f64,
    trigger_reaction: f64,
    trigger_jitter: f64,
}

impl Tuning {
    pub(super) fn of(m: &Module) -> Self {
        let f = |name: &str| m.n(name) as f32;
        Tuning {
            gravity: f("Advanced gravity"),
            wind: f("Advanced wind"),
            damped: f("Advanced damped distance"),
            max_step: f("Advanced max step"),
            min_step: f("Advanced min step"),
            overshoot_chance: f("Advanced overshoot chance"),
            overshoot_scale: f("Advanced overshoot scale"),
            overshoot_max: f("Advanced overshoot max"),
            gaussian: f("Advanced gaussian"),
            // Accuracy as a share, as tight as 0.4..1.
            accuracy: (f("Advanced accuracy") / 100.0).clamp(0.4, 1.0),
            prediction: m.n("Advanced prediction"),
            deadzone: f("Advanced deadzone"),
            anchor: m.n("Advanced anchor"),
            hold_ticks: m.n("Advanced hold ticks") as i32,
            cruise_floor: f("Advanced cruise floor"),
            pace_jitter: f("Advanced pace jitter"),
            burst_chance: f("Advanced burst chance"),
            burst_strength: f("Advanced burst strength"),
            flick_guard: m.b("Advanced flick guard"),
            flick_max: f("Advanced flick max"),
            aim_reaction: m.n("Advanced aim reaction"),
            aim_jitter: m.n("Advanced aim reaction jitter"),
            trigger_reaction: m.n("Advanced trigger reaction"),
            trigger_jitter: m.n("Advanced trigger reaction jitter"),
        }
    }
}

// A small xorshift generator with a normal draw: the noise only needs to vary.
#[derive(Clone, Copy, Debug)]
struct Rng(u64);

impl Rng {
    fn float(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
    fn gaussian(&mut self) -> f64 {
        let u = f64::from(self.float()).max(1e-9);
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * f64::from(self.float())).cos()
    }
    fn below(&mut self, n: u32) -> i32 {
        ((self.float() * n as f32) as i32).min(n as i32 - 1)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Advanced {
    rng: Option<Rng>,
    // Who the turn is on.
    target: Option<u32>,
    wind: [f32; 2],
    velocity: [f32; 2],
    step: f32,
    overshoot: Option<(f32, f32)>,
    overshoot_ticks: i32,
    hold: i32,
    burst_ticks: i32,
    burst: f32,
    // The held aim point and whose it is, when it may move next (ms), and the smoothed motions of the target and you.
    aim: Option<[f64; 3]>,
    aim_for: Option<u32>,
    next_aim: u64,
    target_motion: [f64; 3],
    own_motion: [f64; 3],
    // The first hit's reaction: on whom, and from when it may go (ms).
    trigger: Option<(u32, u64)>,
}

impl Advanced {
    fn rng(&mut self, seed: u64) -> &mut Rng {
        self.rng.get_or_insert(Rng(seed | 1))
    }

    // A normal draw of spread `sd`, kept within ±`limit`.
    fn noise(&mut self, sd: f32, limit: f32) -> f32 {
        if sd <= 0.0 {
            return 0.0;
        }
        let limit = limit.abs().max(1e-4);
        let r = self.rng(0x9e37).gaussian() as f32 * sd;
        r.clamp(-limit, limit)
    }

    fn jittered(&mut self, base: f64, jitter: f64, lo: f64, hi: f64) -> u64 {
        let d = if jitter > 1e-4 { base + self.rng(0x51ed).gaussian() * jitter } else { base };
        d.clamp(lo, hi).round() as u64
    }

    // Nothing to aim at: the next target starts afresh.
    pub(super) fn reset(&mut self) {
        let rng = self.rng;
        *self = Advanced { rng, ..Advanced::default() };
    }

    // Where to aim on `t` now (`now` in ms), given your own move this tick.
    pub(super) fn aim_point(&mut self, t: &Target, own_move: [f64; 3], eye: [f64; 3], now: u64, k: &Tuning) -> [f64; 3] {
        if self.aim_for != Some(t.id) {
            self.aim = None;
            self.target_motion = [0.0; 3];
            self.aim_for = Some(t.id);
        }
        let moved = [t.pos[0] - t.prev[0], t.pos[1] - t.prev[1], t.pos[2] - t.prev[2]];
        for i in 0..3 {
            self.target_motion[i] = self.target_motion[i] * 0.72 + moved[i] * 0.28;
            self.own_motion[i] = self.own_motion[i] * 0.76 + own_move[i] * 0.24;
        }
        let height = t.hi[1] - t.lo[1];
        let chest = [t.pos[0], t.pos[1] + height * 0.75, t.pos[2]];
        let far = super::sunset_util::dist(eye, chest);
        let lead = (k.prediction + far * 0.017).clamp(0.0, 3.5);
        let up = (height * 0.82).min(height - 0.12).max(0.35);
        let clamp_to = |p: [f64; 3], grow: [f64; 3]| {
            [0, 1, 2].map(|i| p[i].clamp(t.lo[i] - grow[i], t.hi[i] + grow[i]))
        };
        let predicted = clamp_to(
            [0, 1, 2].map(|i| [t.pos[0], t.pos[1] + up, t.pos[2]][i] + (self.target_motion[i] - self.own_motion[i]) * lead),
            [0.18, 0.1, 0.18],
        );
        let Some(held) = self.aim else {
            self.aim = Some(predicted);
            self.next_aim = now + self.reaction(false, k);
            return predicted;
        };
        let delta = [predicted[0] - held[0], predicted[1] - held[1], predicted[2] - held[2]];
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        let least = 0.028 + (far * 0.006).min(0.2);
        let big = length > 0.36 + (far * 0.07).min(0.85);
        if length > least && (now >= self.next_aim || big) {
            let share = (k.anchor + length * 0.32).clamp(0.08, 0.7);
            self.aim = Some(clamp_to([0, 1, 2].map(|i| held[i] + delta[i] * share), [0.12; 3]));
            self.next_aim = now + self.reaction(big, k);
        }
        self.aim.unwrap_or(predicted)
    }

    // How long the eye takes to follow (ms): quicker after a big jump.
    fn reaction(&mut self, big: bool, k: &Tuning) -> u64 {
        let (base, jitter) = if big { (k.aim_reaction * 0.6, k.aim_jitter * 0.5) } else { (k.aim_reaction, k.aim_jitter) };
        self.jittered(base, jitter, 20.0, 700.0)
    }

    // The first hit on `id` may go (`now` in ms): it waits Trigger reaction once each time a target comes in reach.
    pub(super) fn trigger_ready(&mut self, id: u32, now: u64, k: &Tuning) -> bool {
        if let Some((on, at)) = self.trigger
            && on == id
        {
            return now >= at;
        }
        let wait = self.jittered(k.trigger_reaction, k.trigger_jitter, 0.0, 450.0);
        self.trigger = Some((id, now + wait));
        wait == 0
    }

    // Out of reach: the next time in it waits again.
    pub(super) fn disarm_trigger(&mut self) {
        self.trigger = None;
    }

    // A new target far off: maybe aim past it first, a little to the side.
    fn plan_overshoot(&mut self, from: (f32, f32), to: (f32, f32), k: &Tuning) {
        if k.overshoot_chance <= 0.0 {
            return;
        }
        let chance = k.overshoot_chance * (1.15 - k.accuracy * 0.35);
        let (dx, dy) = (wrap(to.0 - from.0), to.1 - from.1);
        let d = dx.hypot(dy);
        if d < 9.0 || self.rng(0x0ce5).float() * 100.0 > chance {
            return;
        }
        let amount = (d * k.overshoot_scale).max(1.5).min(k.overshoot_max) * (0.9 + (1.0 - k.accuracy) * 0.35);
        let (ux, uy) = (dx / d.max(0.001), dy / d.max(0.001));
        let along = amount + self.noise(amount * 0.25, amount).abs();
        let side = self.noise(amount * 0.35, amount);
        self.overshoot = Some((to.0 + ux * along - uy * side, (to.1 + uy * along + ux * side * 0.65).clamp(-89.0, 89.0)));
        self.overshoot_ticks = 8 + self.rng(0x0ce5).below(6);
    }

    // The look for this tick's move packet, from the look the server has (`server`) toward `goal`. `on_target`: a ray
    // along the server's look meets the target now.
    pub(super) fn step(&mut self, server: (f32, f32), goal: (f32, f32), id: u32, on_target: bool, sensitivity: f64, k: &Tuning) -> (f32, f32) {
        let counts = degrees_per_count(sensitivity);
        let snap = |to: (f32, f32)| whole_counts(server, to, counts);
        let min_step = k.min_step.max(0.25);
        let acc = k.accuracy;
        if self.step <= 0.001 {
            self.step = k.max_step;
        }
        if self.target != Some(id) {
            let rng = self.rng;
            // The turn starts afresh; the aim point (aim_point's, already this target's) and the reaction stay.
            *self = Advanced {
                rng,
                target: Some(id),
                step: k.max_step,
                aim: self.aim,
                aim_for: self.aim_for,
                next_aim: self.next_aim,
                target_motion: self.target_motion,
                own_motion: self.own_motion,
                trigger: self.trigger,
                ..Advanced::default()
            };
            self.plan_overshoot(server, goal, k);
        }
        let mut aim = goal;
        let mut overshooting = false;
        if let Some(o) = self.overshoot {
            let left = wrap(o.0 - server.0).hypot(o.1 - server.1);
            // As Rise counts it: a tick of the overshoot used only while it is still away from it.
            let going = !(left < 1.15) && {
                let ticks = self.overshoot_ticks;
                self.overshoot_ticks -= 1;
                ticks > 0
            };
            if going {
                aim = o;
                overshooting = true;
            } else {
                self.overshoot = None;
            }
        }
        let (dx, dy) = (wrap(aim.0 - server.0), aim.1 - server.1);
        let d = dx.hypot(dy);
        let norm = d.max(0.001);
        if !overshooting && on_target && d <= k.deadzone {
            self.hold = self.hold.max(k.hold_ticks);
        }
        if !overshooting && self.hold > 0 && on_target && d <= k.deadzone * 1.35 {
            self.hold -= 1;
            self.velocity = self.velocity.map(|v| v * 0.42);
            return snap(server);
        }
        if self.hold > 0 {
            self.hold -= 1;
        }
        if d < 0.001 {
            self.velocity = self.velocity.map(|v| v * 0.6);
            return snap((server.0, server.1.clamp(-90.0, 90.0)));
        }
        let wind = k.wind.min(d);
        if d >= k.damped {
            self.wind[0] = self.wind[0] / SQRT3 + self.noise(wind / SQRT5, wind);
            self.wind[1] = self.wind[1] / SQRT3 + self.noise(wind / SQRT5, wind);
            let fresh = k.max_step * (0.78 + self.rng(7).float() * 0.32);
            self.step = self.step.max(fresh);
        } else {
            self.wind = self.wind.map(|w| w / SQRT3);
            self.step = if self.step < min_step { min_step + self.rng(7).float() * 0.7 } else { (self.step / SQRT5).max(min_step) };
        }
        let gravity = if d < 3.5 { k.gravity * (0.82 + acc * 0.12) } else { k.gravity };
        self.velocity[0] += self.wind[0] + gravity * dx / norm;
        self.velocity[1] += self.wind[1] + gravity * dy / norm;
        let mut pace = 1.0;
        if k.pace_jitter > 1e-4 {
            pace += self.noise(k.pace_jitter * 0.42, k.pace_jitter);
        }
        if !overshooting && d > k.deadzone * 1.6 && d < k.damped * 1.45 {
            let chance = k.burst_chance / 100.0;
            if self.burst_ticks <= 0 && chance > 1e-4 && self.rng(11).float() < chance * 0.12 {
                self.burst_ticks = 1 + self.rng(11).below(2);
                self.burst = k.burst_strength * (0.65 + self.rng(11).float() * 0.55);
            }
        }
        if self.burst_ticks > 0 {
            pace *= 1.0 + self.burst;
            self.burst_ticks -= 1;
        } else {
            self.burst *= 0.55;
        }
        let pace = pace.clamp(0.72, 1.55);
        let cap = (self.step * pace).max(min_step);
        let speed = self.velocity[0].hypot(self.velocity[1]);
        if speed > cap {
            let to = cap * (0.52 + self.rng(13).float() * 0.48);
            self.velocity = self.velocity.map(|v| v / speed * to);
        }
        let ease = if d < 2.0 { 0.86 + acc * 0.08 } else if d < 5.0 { 0.94 } else { 1.0 };
        self.velocity = self.velocity.map(|v| v * ease);
        if !overshooting && k.cruise_floor > 0.01 && d > k.deadzone * 1.25 && d < 14.0 {
            let want = k.cruise_floor * (0.86 + self.rng(17).float() * 0.24);
            let now = self.velocity[0].hypot(self.velocity[1]);
            if now < want {
                let add = (want - now) * (0.72 + self.rng(17).float() * 0.36);
                self.velocity[0] += dx / norm * add;
                self.velocity[1] += dy / norm * add;
            }
        }
        let mut mix = (0.16 + (d / 45.0).clamp(0.0, 1.0) * 0.42) * (0.55 + acc * 0.45);
        mix *= (0.88 + (pace - 1.0) * 0.5).clamp(0.74, 1.18);
        if self.burst > 0.02 {
            mix *= 1.0 + self.burst * 0.35;
        }
        if overshooting {
            mix *= 0.55;
        }
        let mut yaw = server.0 + self.velocity[0] + dx * mix;
        let mut pitch = (server.1 + self.velocity[1] + dy * mix * 0.85).clamp(-89.9, 89.9);
        if !overshooting && d < 1.65 {
            // Close: settle onto it, the built-up speed mostly let go.
            let f = (0.56 + acc * 0.28) * (0.94 + self.rng(19).float() * 0.1);
            yaw = server.0 + dx * f;
            pitch = (server.1 + dy * f).clamp(-89.9, 89.9);
            self.velocity = self.velocity.map(|v| v * 0.55);
        }
        if k.gaussian > 0.0 && d > 0.35 {
            let mut a = (d / 16.0).clamp(0.25, 1.0) * (1.05 - acc * 0.35);
            if overshooting {
                a *= 0.8;
            }
            yaw += self.noise(k.gaussian * a, k.gaussian * 2.2 * a);
            pitch = (pitch + self.noise(k.gaussian * 0.45 * a, k.gaussian * 1.5 * a)).clamp(-89.9, 89.9);
        }
        if k.flick_guard && !overshooting {
            let (fx, fy) = (wrap(yaw - server.0), pitch - server.1);
            let f = fx.hypot(fy);
            if f > 0.001 {
                let limit = k.flick_max * (0.85 + (d / 24.0).clamp(0.0, 1.0) * 0.4) * (0.92 + self.rng(23).float() * 0.2);
                if f > limit {
                    let r = limit / f;
                    yaw = server.0 + fx * r;
                    pitch = (server.1 + fy * r).clamp(-89.9, 89.9);
                    self.velocity = self.velocity.map(|v| v * 0.82);
                }
            }
        }
        snap((yaw, pitch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> Tuning {
        // Rise's defaults.
        Tuning {
            gravity: 9.0, wind: 6.0, damped: 12.0, max_step: 15.0, min_step: 0.0, overshoot_chance: 77.0, overshoot_scale: 0.0,
            overshoot_max: 17.0, gaussian: 0.0, accuracy: 0.4, prediction: 1.0, deadzone: 1.0, anchor: 0.0, hold_ticks: 2,
            cruise_floor: 1.0, pace_jitter: 0.0, burst_chance: 21.0, burst_strength: 0.0, flick_guard: true, flick_max: 29.0,
            aim_reaction: 180.0, aim_jitter: 44.0, trigger_reaction: 95.0, trigger_jitter: 30.0,
        }
    }

    #[test]
    fn it_reaches_the_target_without_flicks() {
        let k = tuning();
        let mut a = Advanced { rng: Some(Rng(12345)), ..Advanced::default() };
        let goal = (90.0, 10.0);
        let mut look = (0.0f32, 0.0f32);
        let mut reached = None;
        for tick in 0..200 {
            let next = a.step(look, goal, 7, false, 0.5, &k);
            let turn = wrap(next.0 - look.0).hypot(next.1 - look.1);
            // Flick guard caps a turn at about 41 degrees; an overshoot (not guarded) stays under 60.
            assert!(turn <= 60.0, "tick {tick}: a {turn:.1} degree flick");
            look = next;
            if reached.is_none() && wrap(goal.0 - look.0).hypot(goal.1 - look.1) < 2.0 {
                reached = Some(tick);
            }
        }
        let tick = reached.expect("never got there");
        assert!((3..60).contains(&tick), "{tick}");
        assert!(wrap(goal.0 - look.0).hypot(goal.1 - look.1) < 3.0);
    }

    #[test]
    fn it_holds_still_on_the_target_in_the_deadzone() {
        let k = tuning();
        let mut a = Advanced { rng: Some(Rng(99)), ..Advanced::default() };
        let look = (10.0f32, 0.0f32);
        let _ = a.step(look, (10.5, 0.2), 3, true, 0.5, &k);
        let held = a.step(look, (10.5, 0.2), 3, true, 0.5, &k);
        assert!(wrap(held.0 - look.0).abs() < 0.2 && (held.1 - look.1).abs() < 0.2, "{held:?}");
    }

    #[test]
    fn the_first_hit_waits_its_reaction_once_per_target() {
        let k = tuning();
        let mut a = Advanced { rng: Some(Rng(5)), ..Advanced::default() };
        assert!(!a.trigger_ready(1, 1_000, &k) || k.trigger_reaction == 0.0);
        assert!(a.trigger_ready(1, 1_000 + 450, &k));
        a.disarm_trigger();
        assert!(!a.trigger_ready(1, 2_000, &k));
    }
}
