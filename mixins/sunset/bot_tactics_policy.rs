// Original bounded tactical policy. Geometry and actual actions remain native.
pub type V = [f64; 3];
#[derive(Clone, Copy, Debug)]
pub struct CriticalFoodView {
    pub life: f64,
    pub distance: f64,
    pub needs_food: bool,
    pub eating: bool,
    pub using_food: bool,
    pub gapple_hotbar: bool,
    pub offhand_totem: bool,
}
pub fn critical_food_retreat(v: CriticalFoodView) -> bool {
    v.life.is_finite()
        && v.distance.is_finite()
        && v.life > 0.
        && v.life <= 4.
        && v.distance >= 0.
        && v.distance < 8.
        && !v.offhand_totem
        && v.gapple_hotbar
        && (v.needs_food || v.eating && v.using_food)
}
pub fn pearl_health_ready(life: f64, offhand_totem: bool) -> bool {
    life.is_finite() && life > 0. && (life > 6. || offhand_totem)
}
pub fn finite(v: V) -> bool {
    v.iter().all(|x| x.is_finite())
}
pub fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn mul(a: V, n: f64) -> V {
    a.map(|x| x * n)
}
pub fn length(a: V) -> f64 {
    a.iter().map(|x| x * x).sum::<f64>().sqrt()
}
pub fn horizontal(a: V) -> f64 {
    a[0].hypot(a[2])
}
pub fn unit(a: V) -> V {
    let n = length(a);
    if n > 1e-8 && n.is_finite() {
        mul(a, 1.0 / n)
    } else {
        [0.; 3]
    }
}
pub fn limited(a: V, cap: f64) -> V {
    let n = length(a);
    if n > cap { mul(a, cap / n) } else { a }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Press,
    Hold,
    Guard,
    Run,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    pub target: u32,
    pub min: f64,
    pub self_cap: f64,
    pub anchor_min: f64,
    pub floor: i32,
    pub eager: bool,
    pub full_speed: bool,
}
impl Stance {
    pub fn steer(
        self,
        target: u32,
        life: f64,
        totem: bool,
        finishing: bool,
        level: bool,
        feet: i32,
    ) -> Steer {
        let (min, cap, anchor) = match self {
            Self::Press => (3.5, 10., 5.),
            Self::Hold => (4.5, 8., 6.),
            Self::Guard => (6., 5., 7.),
            Self::Run => (99., 3., 99.),
        };
        let min = if finishing {
            2.
        } else if level {
            f64::min(min, 2.5)
        } else {
            min
        };
        Steer {
            target,
            min,
            self_cap: f64::min(cap + 2., life - if totem { 1. } else { 4. }).max(0.5),
            anchor_min: if finishing { 2. } else { anchor },
            floor: feet + 1,
            eager: true,
            full_speed: true,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub id: u32,
    pub distance: f64,
    pub life: f64,
    pub totem: bool,
    pub low_armor: bool,
    pub retaliate: bool,
}
pub fn worth(c: Candidate) -> Option<f64> {
    if ![c.distance, c.life]
        .iter()
        .all(|x| x.is_finite() && *x >= 0.)
    {
        return None;
    }
    let near = if c.distance <= 6. {
        1.
    } else {
        ((10. - c.distance) / 4.).max(0.)
    };
    Some(
        4. * near - 0.2 * c.distance
            + (20. - c.life.min(20.)) / 5.
            + if c.totem { 0. } else { 1.5 }
            + if c.low_armor { 1. } else { 0. }
            + if c.retaliate && c.distance <= 4.5 {
                4.
            } else {
                0.
            },
    )
}
#[derive(Default, Debug)]
pub struct TargetChoice {
    pub kept: Option<u32>,
    challenger: Option<(u32, u64)>,
}
impl TargetChoice {
    pub fn choose(
        &mut self,
        tick: u64,
        range: f64,
        regroup: bool,
        all: &[Candidate],
    ) -> Option<u32> {
        let kept = self.kept.and_then(|id| {
            all.iter()
                .find(|c| c.id == id && c.distance <= if regroup { 64. } else { range + 8. })
        });
        let best = all
            .iter()
            .filter(|c| c.distance <= range)
            .filter_map(|c| Some((*c, worth(*c)?)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let next = match (kept, best) {
            (Some(old), Some((new, score))) if new.id != old.id => {
                let close = old.distance <= 6.;
                let margin = if close { 8. } else { 2. };
                let wait = if close { 20 } else { 6 };
                if score < worth(*old).unwrap_or(f64::INFINITY) + margin {
                    self.challenger = None;
                    Some(old.id)
                } else {
                    let began = match self.challenger {
                        Some((id, at)) if id == new.id && at <= tick => at,
                        _ => tick,
                    };
                    self.challenger = Some((new.id, began));
                    if tick.saturating_sub(began) >= wait {
                        Some(new.id)
                    } else {
                        Some(old.id)
                    }
                }
            }
            (Some(old), _) => {
                self.challenger = None;
                Some(old.id)
            }
            (_, best) => {
                self.challenger = None;
                best.map(|(c, _)| c.id)
            }
        };
        self.kept = next;
        next
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pause {
    Go,
    Yield,
    Hold,
    Stop,
}
#[derive(Debug)]
pub struct PauseClock {
    manual_until: u64,
    hold_until: u64,
    corrections: std::collections::VecDeque<u64>,
    last_time: Option<(i64, u64)>,
    pub tps: f64,
    stopped: bool,
}
impl Default for PauseClock {
    fn default() -> Self {
        Self {
            manual_until: 0,
            hold_until: 0,
            corrections: Default::default(),
            last_time: None,
            tps: 20.,
            stopped: false,
        }
    }
}
impl PauseClock {
    pub fn manual(&mut self, tick: u64) {
        self.manual_until = tick.saturating_add(30);
    }
    pub fn context(&mut self, tick: u64, new_world: bool) {
        self.hold_until = tick.saturating_add(if new_world { 40 } else { 30 });
        self.corrections.clear();
    }
    pub fn correction(&mut self, tick: u64, wall_ms: u64, distance: f64, pearl_age: u64) {
        if !distance.is_finite() || distance > 6. || pearl_age < 60 {
            return;
        }
        while self
            .corrections
            .front()
            .is_some_and(|t| wall_ms.saturating_sub(*t) > 10_000)
        {
            self.corrections.pop_front();
        }
        if self.corrections.back().is_some_and(|t| *t > wall_ms) {
            self.corrections.clear();
        }
        if self.corrections.len() == 6 {
            self.corrections.pop_front();
        }
        self.corrections.push_back(wall_ms);
        self.hold_until = self.hold_until.max(tick.saturating_add(10));
        self.stopped = self.corrections.len() >= 6;
    }
    pub fn time(&mut self, game_time: i64, wall_ms: u64) {
        if let Some((prior, at)) = self.last_time {
            if game_time > prior && wall_ms > at {
                let sample =
                    ((game_time - prior) as f64 * 1000. / (wall_ms - at) as f64).clamp(0., 20.);
                self.tps = (self.tps + sample) * 0.5;
            }
        }
        self.last_time = Some((game_time, wall_ms));
    }
    pub fn state(&self, tick: u64) -> Pause {
        if self.stopped {
            Pause::Stop
        } else if tick < self.hold_until {
            Pause::Hold
        } else if tick < self.manual_until {
            Pause::Yield
        } else {
            Pause::Go
        }
    }
    pub fn slow(&self) -> bool {
        self.tps < 15.
    }
}
#[derive(Clone, Debug)]
pub struct Rival {
    pub fights: u32,
    pub launches: u32,
    pub launch_rate: f64,
    pub flights: u32,
    pub push: V,
    pub retotems: u32,
    pub retotem: f64,
    pub holes: u32,
    pub hole_ticks: f64,
    pub low_spells: u32,
    pub pearl_rate: f64,
    pub rewards: [f64; 6],
    pub tries: [f64; 6],
    pub dirty: bool,
}
impl Default for Rival {
    fn default() -> Self {
        Self {
            fights: 0,
            launches: 0,
            launch_rate: 0.5,
            flights: 0,
            push: [0.; 3],
            retotems: 0,
            retotem: 0.,
            holes: 0,
            hole_ticks: 0.,
            low_spells: 0,
            pearl_rate: 0.,
            rewards: [0.; 6],
            tries: [0.; 6],
            dirty: false,
        }
    }
}
fn average(value: &mut f64, n: &mut u32, sample: f64) {
    if !sample.is_finite() {
        return;
    }
    *value = if *n == 0 {
        sample
    } else {
        *value + (sample - *value) * 0.25
    };
    *n = n.saturating_add(1);
}
impl Rival {
    pub fn launched(&mut self, up: bool) {
        average(
            &mut self.launch_rate,
            &mut self.launches,
            if up { 1. } else { 0. },
        );
        self.dirty = true;
    }
    pub fn flew(&mut self, push: V) {
        if !finite(push) || horizontal(push) > 0.15 {
            return;
        }
        let n = self.flights;
        for i in [0, 2] {
            let mut count = n;
            average(&mut self.push[i], &mut count, push[i]);
        }
        self.flights = n.saturating_add(1);
        self.dirty = true;
    }
    pub fn retotemed(&mut self, ticks: u64) {
        average(&mut self.retotem, &mut self.retotems, ticks.min(100) as f64);
        self.dirty = true;
    }
    pub fn left_hole(&mut self, ticks: u64) {
        average(&mut self.hole_ticks, &mut self.holes, ticks.min(600) as f64);
        self.dirty = true;
    }
    pub fn low_spell(&mut self, pearled: bool) {
        average(
            &mut self.pearl_rate,
            &mut self.low_spells,
            if pearled { 1. } else { 0. },
        );
        self.dirty = true;
    }
    pub fn shrugs(&self) -> bool {
        self.launches >= 4 && self.launch_rate < 0.35
    }
    pub fn camper(&self) -> bool {
        self.holes >= 2 && self.hole_ticks >= 100.
    }
    pub fn pearl_low(&self) -> bool {
        self.low_spells >= 2 && self.pearl_rate >= 0.5
    }
    pub fn window(&self) -> u64 {
        if self.retotems >= 2 {
            self.retotem.round().clamp(3., 30.) as u64
        } else {
            8
        }
    }
    fn bucket(reach: f64) -> usize {
        if reach <= 2.6 {
            0
        } else if reach <= 3. {
            1
        } else {
            2
        }
    }
    pub fn opener(&self, reach: f64, entropy: f64) -> usize {
        let b = Self::bucket(reach);
        let total = self.tries[b] + self.tries[b + 3];
        let mut best = (0, f64::NEG_INFINITY);
        for arm in 0..2 {
            let i = arm * 3 + b;
            if self.tries[i] < 0.5 {
                return arm;
            }
            let score = (self.rewards[i] + [0.45, 0.55][arm]) / (self.tries[i] + 1.)
                + 0.5 * ((total + 1.).ln() / self.tries[i]).sqrt()
                + entropy.clamp(0., 0.019);
            if score > best.1 {
                best = (arm, score);
            }
        }
        best.0
    }
    pub fn opened(&mut self, arm: usize, reach: f64, reward: f64) {
        if arm > 1 || !reward.is_finite() {
            return;
        }
        let b = Self::bucket(reach);
        for i in [b, b + 3] {
            self.rewards[i] *= 0.97;
            self.tries[i] *= 0.97;
        }
        self.rewards[arm * 3 + b] += reward.clamp(0., 1.5);
        self.tries[arm * 3 + b] += 1.;
        self.dirty = true;
    }
}
#[derive(Default, Debug)]
pub struct Fight {
    pub target: Option<u32>,
    pub run_until: u64,
    pub own_pop: Option<u64>,
    pub their_pop: Option<u64>,
    pops: u8,
    last_pop: Option<u64>,
    pub needs_food: bool,
    pub last_retreat: u64,
    pub next_launch: u64,
    pub launch: Option<(u64, f64, f64)>,
    pub launching_until: Option<u64>,
    pub follow_until: u64,
    pub follow_laid: bool,
    pub progress_at: u64,
    pub next_breach: u64,
    pub city_fails: u8,
    pub below_since: Option<u64>,
    pub next_dig: u64,
    pub next_base_dig: u64,
    pub next_pearl: u64,
    pub throw_until: Option<u64>,
    pub regroup_until: u64,
    pub last_distance: f64,
    pub closing_at: u64,
    pub hole_since: Option<u64>,
    pub low_since: Option<(u64, bool)>,
    pub opener: Option<(usize, f64, u64, f64, u32)>,
    pub seen_health: Option<f64>,
}
#[derive(Clone, Copy, Debug)]
pub struct FightView {
    pub tick: u64,
    pub health: f64,
    pub absorption: f64,
    pub totems: u32,
    pub offhand_totem: bool,
    pub enemies: usize,
    pub risk: f64,
    pub target_life: f64,
    pub target_totem: bool,
    pub distance: f64,
    pub hole: bool,
    pub target_y: f64,
    pub eat_below: f64,
}
impl Fight {
    pub fn start(&mut self, target: u32, tick: u64) {
        let own_pop = self.own_pop;
        let pops = self.pops;
        let last_pop = self.last_pop;
        let run_until = self.run_until;
        *self = Self::default();
        self.target = Some(target);
        self.progress_at = tick;
        self.closing_at = tick;
        self.last_distance = f64::INFINITY;
        self.own_pop = own_pop;
        self.pops = pops;
        self.last_pop = last_pop;
        self.run_until = run_until;
    }
    pub fn own_pop(&mut self, tick: u64) {
        if self.last_pop.is_none_or(|t| tick < t || tick - t > 600) {
            self.pops = 0;
        }
        self.last_pop = Some(tick);
        self.own_pop = Some(tick);
        self.pops = self.pops.saturating_add(1);
        if self.pops >= 3 {
            self.run_until = tick.saturating_add(200);
            self.pops = 0;
        }
    }
    pub fn observe(&mut self, v: FightView, r: &mut Rival) {
        if self
            .seen_health
            .is_some_and(|old| old - v.target_life > 0.5)
            || !v.hole
        {
            self.progress_at = v.tick;
        }
        self.seen_health = Some(v.target_life);
        if self.last_distance - v.distance > 0.5 {
            self.last_distance = v.distance;
            self.closing_at = v.tick;
        }
        if let Some(pop) = self.their_pop {
            if v.target_totem || v.tick.saturating_sub(pop) >= 100 {
                r.retotemed(v.tick.saturating_sub(pop));
                self.their_pop = None;
            }
        }
        match (self.hole_since, v.hole) {
            (None, true) => self.hole_since = Some(v.tick),
            (Some(t), false) => {
                r.left_hole(v.tick.saturating_sub(t));
                self.hole_since = None;
            }
            _ => {}
        }
        if v.target_life <= 8. && self.low_since.is_none() {
            self.low_since = Some((v.tick, false));
        }
        if v.target_life > 12. {
            if let Some((_, p)) = self.low_since.take() {
                r.low_spell(p);
            }
        }
        if let Some((at, y, peak)) = self.launch {
            let peak = peak.max(v.target_y - y);
            self.launch = Some((at, y, peak));
            if v.tick.saturating_sub(at) >= 8 {
                r.launched(peak >= 0.3);
                self.launch = None;
            }
        }
        if let Some((arm, reach, at, hp, pops)) = self.opener {
            if v.tick.saturating_sub(at) >= 40 {
                r.opened(
                    arm,
                    reach,
                    ((hp - v.target_life).max(0.) + 10. * pops as f64) / 12.,
                );
                self.opener = None;
            }
        }
    }
    pub fn stance(&mut self, v: FightView, r: &Rival) -> Stance {
        if v.health <= v.eat_below - 2. && v.absorption < 4. {
            self.needs_food = true;
            self.regroup_until = v.tick.saturating_add(300);
        }
        if v.health > v.eat_below + 4. || v.absorption >= 4. {
            self.needs_food = false;
        }
        if v.totems == 0 && v.enemies >= 2 {
            self.run_until = self.run_until.max(v.tick.saturating_add(40));
        }
        if v.tick < self.run_until {
            self.last_retreat = v.tick;
            return Stance::Run;
        }
        if self.own_pop.is_some_and(|t| v.tick.saturating_sub(t) <= 40) {
            return Stance::Guard;
        }
        if self
            .their_pop
            .is_some_and(|t| v.tick.saturating_sub(t) <= r.window())
            && !v.target_totem
        {
            return Stance::Press;
        }
        if (v.health <= 6. || v.totems <= 1 || v.risk >= 0.35) && v.health <= 8. {
            Stance::Guard
        } else {
            Stance::Press
        }
    }
    pub fn finishing(&self, v: FightView, r: &Rival) -> bool {
        v.target_life <= 3.
            || self
                .their_pop
                .is_some_and(|t| v.tick.saturating_sub(t) <= r.window())
                && !v.target_totem
    }
    pub fn struck(&mut self, tick: u64, target_y: f64) {
        self.launching_until = None;
        self.next_launch = tick.saturating_add(80);
        self.launch = Some((tick, target_y, 0.));
        self.follow_until = tick.saturating_add(12);
        self.follow_laid = false;
    }
    pub fn cancel_launch(&mut self, tick: u64) {
        self.launching_until = None;
        self.next_launch = tick.saturating_add(40);
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub tick: u64,
    pub at: V,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub at: V,
    pub ticks: u32,
    pub landed: bool,
}
// The caller supplies real swept native body collision; requested displacement
// is never directly applied to an entity or the world.
pub fn body_path(
    mut at: V,
    mut velocity: V,
    push: V,
    knocked: bool,
    grip: f64,
    ticks: u32,
    mut collide: impl FnMut(V, V) -> Option<V>,
) -> Option<Stop> {
    if !finite(at) || !finite(velocity) || !finite(push) || !grip.is_finite() {
        return None;
    }
    for step in 1..=ticks.min(40) {
        if step > 1 || !knocked {
            let drag = if knocked && step == 2 { grip } else { 0.91 };
            velocity[0] *= drag;
            velocity[2] *= drag;
            velocity[1] = (velocity[1] - 0.08) * 0.98;
        }
        velocity[0] += push[0];
        velocity[2] += push[2];
        let moved = collide(at, velocity)?;
        if !finite(moved) {
            return None;
        }
        at = add(at, moved);
        let landed = velocity[1] < 0. && (moved[1] - velocity[1]).abs() > 1e-8;
        for i in 0..3 {
            if (moved[i] - velocity[i]).abs() > 1e-8 {
                velocity[i] = 0.;
            }
        }
        if landed {
            return Some(Stop {
                at,
                ticks: step,
                landed: true,
            });
        }
    }
    Some(Stop {
        at,
        ticks: ticks.min(40),
        landed: false,
    })
}
#[derive(Default, Debug)]
pub struct Air {
    early: Option<V>,
    pub grip: f64,
    pub target: Option<u32>,
    samples: std::collections::VecDeque<Sample>,
    push_sum: V,
    pushes: u32,
    pub kick: Option<(u64, V, V, V)>,
    kick_seen: Option<u64>,
    spoiled: bool,
    pub bare: Option<Stop>,
    pub gain: f64,
    pub airborne: bool,
}
impl Air {
    pub fn note(&mut self, id: u32, tick: u64, at: V, ground: bool, floating: bool) {
        if !finite(at) {
            return;
        }
        if self.target != Some(id) {
            *self = Self::default();
            self.target = Some(id);
        }
        if let Some((began, from, _, _)) = self.kick {
            if self.kick_seen.is_none()
                && (at[1] > from[1] + 0.05 || horizontal(sub(at, from)).powi(2) > 0.01)
            {
                self.kick_seen = Some(tick);
            }
            if tick.saturating_sub(began) > 40
                || self.kick_seen.is_none() && tick.saturating_sub(began) > 8
            {
                self.kick = None;
            }
        }
        if ground || floating {
            self.airborne = false;
            self.samples.clear();
            self.push_sum = [0.; 3];
            self.pushes = 0;
            return;
        }
        self.airborne = true;
        if self.samples.back().is_some_and(|s| s.at == at) {
            return;
        }
        if self.samples.len() == 40 {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { tick, at });
        let n = self.samples.len();
        if n == 2 && !self.spoiled {
            if let Some((_, _, kick, _)) = self.kick {
                self.early = infer_early(kick, self.grip, self.samples[0], self.samples[1]);
            }
        }
        if n < 3 {
            return;
        }
        let a = self.samples[n - 3];
        let b = self.samples[n - 2];
        let c = self.samples[n - 1];
        let d1 = b.tick.saturating_sub(a.tick);
        let d2 = c.tick.saturating_sub(b.tick);
        if d1 == 0 || d2 == 0 || d1 > 4 || d2 > 4 {
            return;
        }
        let u = mul(sub(b.at, a.at), 1. / d1 as f64);
        let v = mul(sub(c.at, b.at), 1. / d2 as f64);
        let drag = 0.91f64.powf((d1 + d2) as f64 * 0.5);
        let scale = 0.09 / (1. - drag);
        let p = [
            (v[0] - drag * u[0]) * scale,
            0.,
            (v[2] - drag * u[2]) * scale,
        ];
        if horizontal(p) <= 0.045 {
            self.push_sum = add(self.push_sum, p);
            self.pushes = self.pushes.saturating_add(1);
        }
    }
    pub fn kicked(
        &mut self,
        tick: u64,
        from: V,
        own: V,
        yaw: f64,
        sprint: bool,
        resistance: f64,
        grip: f64,
    ) {
        if !finite(from) || !finite(own) || !yaw.is_finite() || !resistance.is_finite() {
            return;
        }
        let keep = 1. - resistance.clamp(0., 1.);
        let away = unit([from[0] - own[0], 0., from[2] - own[2]]);
        if keep <= 0. || length(away) < 1e-8 {
            return;
        }
        let base = mul(away, 0.4 * keep);
        let velocity = if sprint {
            let a = yaw.to_radians();
            add(
                mul(base, 0.5),
                [
                    -a.sin() * 0.5 * keep,
                    (0.2 * keep + 0.5 * keep).min(0.4),
                    a.cos() * 0.5 * keep,
                ],
            )
        } else {
            [base[0], (0.4 * keep).min(0.4), base[2]]
        };
        let way = unit([velocity[0], 0., velocity[2]]);
        self.kick = Some((tick, from, velocity, way));
        self.grip = grip;
        self.early = None;
        self.kick_seen = None;
        self.spoiled = false;
        self.samples.clear();
        self.push_sum = [0.; 3];
        self.pushes = 0;
    }
    pub fn blast(&mut self, center: V) {
        if self
            .samples
            .back()
            .is_some_and(|s| length(sub(s.at, center)) <= 10.)
            || self
                .kick
                .is_some_and(|(_, at, _, _)| length(sub(at, center)) <= 10.)
        {
            self.spoiled = true;
            if self.kick_seen.is_none() {
                self.kick = None;
            }
            self.samples.clear();
            self.pushes = 0;
            self.push_sum = [0.; 3];
        }
    }
    pub fn request(&self, tick: u64, ahead: u32, r: &Rival) -> Option<(V, V, V, bool, u32)> {
        if ahead == 0 || ahead > 40 {
            return None;
        }
        if let Some((_, from, v, way)) = self.kick {
            if self.kick_seen.is_none() || self.samples.len() < 2 {
                let learned = if r.flights >= 2 {
                    add(mul(way, r.push[0]), mul([-way[2], 0., way[0]], r.push[2]))
                } else {
                    [0.; 3]
                };
                let gone = self
                    .kick_seen
                    .map_or(0, |t| tick.saturating_sub(t) + 1)
                    .min(40) as u32;
                return Some((from, v, limited(learned, 0.1), true, (ahead + gone).min(40)));
            }
        }
        if (self.pushes < 2 && self.kick.is_none() && ahead != 40) || self.samples.len() < 2 {
            return None;
        }
        let b = self.samples[self.samples.len() - 2];
        let c = *self.samples.back()?;
        let span = c.tick.checked_sub(b.tick)?;
        if span == 0 || span > 4 {
            return None;
        }
        let mean = mul(sub(c.at, b.at), 1. / span as f64);
        let half = (span - 1) as f64 * 0.5;
        let drag = 0.91f64.powf(half);
        let v = [mean[0] * drag, mean[1] - 0.0784 * half, mean[2] * drag];
        let usual = if let Some((_, _, _, way)) = self.kick {
            if r.flights >= 2 {
                limited(
                    add(mul(way, r.push[0]), mul([-way[2], 0., way[0]], r.push[2])),
                    0.026,
                )
            } else {
                [0.; 3]
            }
        } else {
            limited([v[0] * 0.09, 0., v[2] * 0.09], 0.026)
        };
        let push = if self.pushes >= 2 {
            limited(mul(self.push_sum, 1. / self.pushes as f64), 0.045)
        } else if self.pushes == 1 {
            limited(
                self.early
                    .map_or(self.push_sum, |early| mul(add(self.push_sum, early), 0.5)),
                0.045,
            )
        } else {
            self.early.map_or(usual, |early| {
                limited(
                    if r.flights >= 2 {
                        mul(add(early, usual), 0.5)
                    } else {
                        early
                    },
                    0.045,
                )
            })
        };
        Some((
            c.at,
            v,
            push,
            false,
            (ahead + tick.saturating_sub(c.tick).min(3) as u32).min(40),
        ))
    }
    pub fn landing_learn(&mut self, at: V, r: &mut Rival) {
        if self.spoiled || self.gain < 1. {
            return;
        }
        let (Some(bare), Some((_, _, _, way))) = (self.bare, self.kick) else {
            return;
        };
        if (at[1] - bare.at[1]).abs() > 0.6 {
            return;
        }
        let off = sub(at, bare.at);
        let along = (off[0] * way[0] + off[2] * way[2]) / self.gain;
        let cross = (-off[0] * way[2] + off[2] * way[0]) / self.gain;
        if along.abs() <= 0.15 && cross.abs() <= 0.15 {
            r.flew([along, 0., cross]);
        }
        self.kick = None;
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PearlLanding {
    pub at: V,
    pub ticks: u32,
}
pub fn pearl_path(
    mut at: V,
    mut velocity: V,
    mut step: impl FnMut(V, V) -> Option<(Option<V>, bool)>,
) -> Option<PearlLanding> {
    if !finite(at) || !finite(velocity) {
        return None;
    }
    for tick in 1..=100 {
        let (to, water) = step(at, at)?;
        if to.is_some() {
            return None;
        }
        velocity = mul(
            [velocity[0], velocity[1] - 0.03, velocity[2]],
            if water { 0.8 } else { 0.99 },
        );
        let next = add(at, velocity);
        let (hit, _) = step(at, next)?;
        if let Some(hit) = hit {
            return finite(hit).then_some(PearlLanding {
                at: hit,
                ticks: tick,
            });
        }
        at = next;
    }
    None
}
pub fn pearl_velocity(yaw: f64, pitch: f64, motion: V, grounded: bool) -> V {
    let y = yaw.to_radians();
    let p = pitch.to_radians();
    add(
        [
            -y.sin() * p.cos() * 1.5,
            -p.sin() * 1.5,
            y.cos() * p.cos() * 1.5,
        ],
        [motion[0], if grounded { 0. } else { motion[1] }, motion[2]],
    )
}
pub fn stand_score(at: V, own: V, target: V, cover: f64, stance: Stance) -> Option<f64> {
    if !finite(at) || !finite(own) || !finite(target) || !cover.is_finite() {
        return None;
    }
    let d = horizontal(sub(at, target));
    if !(1.9..=5.5).contains(&d) {
        return Some(-10. - (d - 3.2).abs());
    }
    let weight = match stance {
        Stance::Press => 0.7,
        Stance::Hold => 1.,
        Stance::Guard => 1.6,
        Stance::Run => 2.,
    };
    Some(-(d - 3.2).abs() + 3. * weight * cover.clamp(0., 1.) - 0.15 * horizontal(sub(at, own)))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Surround,
    AnchorCover,
    LandingBase,
    LandingAnchor,
    DigAnchor,
    CloseDanger,
    CampCrystal,
}
#[derive(Clone, Copy, Debug)]
pub struct PendingBlock {
    pub cell: [i32; 3],
    pub before: u32,
    pub after: Option<u32>,
    pub sent: u64,
    pub tries: u8,
    pub purpose: Purpose,
}
#[derive(Default, Debug)]
pub struct Blocks {
    pub entries: Vec<PendingBlock>,
}
impl Blocks {
    pub fn can_send(&self, cell: [i32; 3], before: u32, tick: u64) -> bool {
        self.entries
            .iter()
            .find(|e| e.cell == cell)
            .map_or(self.entries.len() < 64, |e| {
                e.after.is_some_and(|after| before != after)
                    || e.after.is_none() && before != e.before
                    || e.after.is_none() && e.tries < 3 && tick.saturating_sub(e.sent) >= 20
            })
    }
    pub fn sent(&mut self, cell: [i32; 3], before: u32, tick: u64, purpose: Purpose) -> bool {
        if !self.can_send(cell, before, tick) {
            return false;
        }
        if let Some(e) = self.entries.iter_mut().find(|e| e.cell == cell) {
            if before != e.before {
                e.tries = 0;
            }
            e.before = before;
            e.after = None;
            e.sent = tick;
            e.tries += 1;
            e.purpose = purpose;
        } else {
            self.entries.push(PendingBlock {
                cell,
                before,
                after: None,
                sent: tick,
                tries: 1,
                purpose,
            });
        }
        true
    }
    pub fn payload(&mut self, cell: [i32; 3], id: u32) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.cell == cell) {
            if id != e.before {
                e.after = Some(id);
            }
        }
    }
    pub fn acknowledged(&self, cell: [i32; 3], world: u32) -> bool {
        self.entries
            .iter()
            .any(|e| e.cell == cell && e.after == Some(world) && world != e.before)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CrystalReceipt {
    pub base: [i32; 3],
    pub sent: u64,
    pub tries: u8,
    pub id: Option<u32>,
    pub removed: bool,
}
#[derive(Default, Debug)]
pub struct CampLedger {
    pub entries: Vec<CrystalReceipt>,
}
impl CampLedger {
    pub fn can_send(&self, base: [i32; 3], tick: u64) -> bool {
        self.entries
            .iter()
            .find(|r| r.base == base)
            .map_or(self.entries.len() < 64, |r| {
                r.removed || r.id.is_none() && r.tries < 3 && tick.saturating_sub(r.sent) >= 20
            })
    }
    pub fn sent(&mut self, base: [i32; 3], tick: u64) -> bool {
        if !self.can_send(base, tick) {
            return false;
        }
        if let Some(r) = self.entries.iter_mut().find(|r| r.base == base) {
            if r.removed {
                r.tries = 0;
            }
            r.tries += 1;
            r.id = None;
            r.removed = false;
            r.sent = tick;
        } else {
            self.entries.push(CrystalReceipt {
                base,
                sent: tick,
                tries: 1,
                id: None,
                removed: false,
            });
        }
        true
    }
    pub fn spawned(&mut self, base: [i32; 3], id: u32, tick: u64) {
        if let Some(r) = self
            .entries
            .iter_mut()
            .find(|r| r.base == base && r.id.is_none() && tick >= r.sent && tick - r.sent <= 20)
        {
            r.id = Some(id);
        }
    }
    pub fn removed(&mut self, ids: &[u32]) {
        for r in &mut self.entries {
            if r.id.is_some_and(|id| ids.contains(&id)) {
                r.removed = true;
            }
        }
    }
}

fn infer_early(kick: V, grip: f64, a: Sample, b: Sample) -> Option<V> {
    let span = b.tick.checked_sub(a.tick)? as usize;
    if span == 0 || span > 4 || !grip.is_finite() || grip <= 0. {
        return None;
    }
    let mean = mul(sub(b.at, a.at), 1. / span as f64);
    let mut y = [0.; 21];
    let mut carry = [0.; 21];
    let mut gain = [0.; 21];
    let (mut vy, mut h, mut g) = (kick[1], 1., 1.);
    for i in 1..=20 {
        if i > 1 {
            let drag = if i == 2 { grip } else { 0.91 };
            vy = (vy - 0.08) * 0.98;
            h *= drag;
            g = g * drag + 1.;
        }
        y[i] = vy;
        carry[i] = h;
        gain[i] = g;
    }
    let avg =
        |v: &[f64; 21], end: usize| v[(end - span + 1)..=end].iter().sum::<f64>() / span as f64;
    let end = (span..=20).min_by(|a, b| {
        (avg(&y, *a) - mean[1])
            .abs()
            .total_cmp(&(avg(&y, *b) - mean[1]).abs())
    })?;
    if (avg(&y, end) - mean[1]).abs() > 0.04 {
        return None;
    }
    let alone = avg(&carry, end);
    let g = avg(&gain, end);
    let push = [
        (mean[0] - kick[0] * alone) / g,
        0.,
        (mean[2] - kick[2] * alone) / g,
    ];
    (finite(push) && horizontal(push) <= 0.1).then_some(push)
}

impl Fight {
    pub fn observe_unlearned(&mut self, view: FightView) {
        let mut discarded = Rival::default();
        self.observe(view, &mut discarded);
    }
}
// Unrelated forwarded hits must leave this exact pending victim untouched.
pub fn take_forwarded_attack(
    pending: &mut Option<(u32, u64, f64, bool)>,
    victim: u32,
    tick: u64,
) -> Option<(u32, u64, f64, bool)> {
    let value = (*pending)?;
    if value.0 != victim {
        return None;
    }
    if tick < value.1 || tick - value.1 > 1 {
        pending.take();
        return None;
    }
    pending.take()
}
