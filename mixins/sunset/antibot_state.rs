// Bounded observations and spawn identities, independent of the game and packet types.
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sensitivity { Conservative, Balanced, Sensitive }
impl Sensitivity {
    fn index(self) -> usize { match self { Self::Conservative => 0, Self::Balanced => 1, Self::Sensitive => 2 } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class { Trusted, Suspicious, Rejected }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token { pub id: u32, epoch: u64, generation: u64 }
#[derive(Clone, Copy)]
pub(super) struct Observation {
    pub id: u32,
    pub uuid: Option<u128>,
    pub player: bool,
    pub info: bool,
    pub invalid_name: bool,
    pub duplicate: bool,
    pub mode: Option<i32>,
    pub position: [f64; 3],
    pub local: [f64; 3],
    pub local_yaw: f64,
    pub scale: f64,
    pub size: [f64; 3],
    pub on_ground: bool,
    pub flying: bool,
    pub age: u32,
    pub invisible: bool,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Decision { pub class: Class, pub confidence: u8, pub reasons: u16, pub observed: bool }

pub(super) const NAME: u16 = 1;
pub(super) const INFO: u16 = 2;
pub(super) const DUPLICATE: u16 = 4;
pub(super) const SHAPE: u16 = 8;
pub(super) const INVISIBLE: u16 = 16;
pub(super) const ORBIT: u16 = 32;
pub(super) const TETHER: u16 = 64;
pub(super) const HOVER: u16 = 128;
pub(super) const JUMPS: u16 = 256;
pub(super) const CHURN: u16 = 512;
const HISTORY: usize = 32;
const MAX_ENTITIES: usize = 1024;

#[derive(Clone, Copy, Default)]
struct Sample {
    time: f64,
    position: [f64; 3],
    local: [f64; 3],
    yaw: f64,
    grounded: bool,
}
#[derive(Default)]
struct History { samples: [Sample; HISTORY], len: usize, next: usize }
impl History {
    fn push(&mut self, sample: Sample) {
        self.samples[self.next] = sample;
        self.next = (self.next + 1) % HISTORY;
        self.len = (self.len + 1).min(HISTORY);
    }
    fn at(&self, index: usize) -> Sample {
        self.samples[(self.next + HISTORY - self.len + index) % HISTORY]
    }
    fn last(&self) -> Option<Sample> { (self.len > 0).then(|| self.at(self.len - 1)) }
}
struct Record {
    born: f64,
    seen: f64,
    history: History,
    confidence: f64,
    reasons: u16,
    rejected: [bool; 3],
    sensitivity: Sensitivity,
    supported_until: [f64; 3],
}
struct Identity {
    token: Token,
    uuid: Option<u128>,
    age: u32,
    seen: f64,
}
#[derive(Default)]
pub(super) struct Tracker {
    records: HashMap<u32, Record>,
    // Lightweight identities follow live engine entities; only expensive
    // observation histories have a fixed budget. Overflow must not prevent
    // attacking a legitimate new entity, including when filtering is off.
    identities: HashMap<u32, Identity>,
    generation: u64,
    epoch: u64,
    // Nearby, short-lived removals. This is corroboration, never a verdict.
    removals: VecDeque<f64>,
}
fn length(v: [f64; 3]) -> f64 { v[0].hypot(v[1]).hypot(v[2]) }
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn wrap(radians: f64) -> f64 {
    (radians + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
fn movement(history: &History, o: &Observation) -> u16 {
    if history.len < 12 { return 0; }
    let first = history.at(0);
    let last = history.at(history.len - 1);
    let span = last.time - first.time;
    if span < 0.55 { return 0; }
    let mut radius_sum = 0.0;
    let mut radius_sq = 0.0;
    let mut turn = 0.0;
    let mut absolute_turn = 0.0;
    let mut path = 0.0;
    let mut jumps = 0;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut airborne = 0;
    let mut high = 0;
    let mut local_turn = 0.0;
    let mut previous: Option<Sample> = None;
    for i in 0..history.len {
        let s = history.at(i);
        let r = sub(s.position, s.local);
        let radius = r[0].hypot(r[2]);
        radius_sum += radius;
        radius_sq += radius * radius;
        min_y = min_y.min(s.position[1]);
        max_y = max_y.max(s.position[1]);
        airborne += usize::from(!s.grounded);
        high += usize::from(r[1] >= 1.0);
        if let Some(p) = previous {
            local_turn += wrap(s.yaw - p.yaw).abs();
            let before = sub(p.position, p.local);
            let angle = wrap(r[0].atan2(r[2]) - before[0].atan2(before[2]));
            // A teleport across the observer is not an orbit segment.
            if angle.abs() < 1.2 {
                turn += angle;
                absolute_turn += angle.abs();
            }
            let moved = length(sub(s.position, p.position));
            path += moved;
            jumps += usize::from(moved > 6.0 && s.time - p.time < 0.2);
        }
        previous = Some(s);
    }
    let n = history.len as f64;
    let radius = radius_sum / n;
    let variance = (radius_sq / n - radius * radius).max(0.0);
    let mut reasons = 0;
    if (0.5..=8.0).contains(&radius) && variance.sqrt() < (radius * 0.12).max(0.2)
        && absolute_turn > 2.4 && turn.abs() > absolute_turn * 0.85 && path > 2.5
    { reasons |= ORBIT; }
    // Flight, elytra and creative movement are legitimate reasons to hover.
    let flight = o.flying || matches!(o.mode, Some(1 | 3)) || (o.player && o.mode.is_none());
    if !flight && span >= 0.8 && max_y - min_y < 0.06
        && airborne * 10 >= history.len * 9 && high * 10 >= history.len * 9
    { reasons |= HOVER; }
    if !flight && jumps >= 3 { reasons |= JUMPS; }

    // A stationary opponent cannot be following the observer. This also
    // avoids the more expensive lag comparisons in ordinary quiet scenes.
    if path < 1.0 || local_turn < 1.4 { return reasons; }

    // Compare an entity with past local poses too: delayed network updates must
    // not turn a rigid look-relative follower into unrelated motion. Both
    // translation/turn excitation and low residual error are required.
    for lag in 0..=6 {
        let count = history.len - lag;
        if count < 12 { continue; }
        let mut sum = [0.0; 3];
        let mut squares = 0.0;
        let mut yaw_change = 0.0;
        let mut local_path = 0.0;
        let mut before: Option<Sample> = None;
        for i in lag..history.len {
            let target = history.at(i);
            let local = history.at(i - lag);
            let r = sub(target.position, local.local);
            let (sin, cos) = local.yaw.sin_cos();
            let relative = [r[0] * cos + r[2] * sin, r[1], -r[0] * sin + r[2] * cos];
            for axis in 0..3 { sum[axis] += relative[axis]; }
            squares += relative.iter().map(|v| v * v).sum::<f64>();
            if let Some(p) = before {
                yaw_change += wrap(local.yaw - p.yaw).abs();
                local_path += length(sub(local.local, p.local));
            }
            before = Some(local);
        }
        let n = count as f64;
        let mean = sum.map(|v| v / n);
        let residual = (squares / n - mean.iter().map(|v| v * v).sum::<f64>()).max(0.0);
        if (0.5..=8.0).contains(&mean[0].hypot(mean[2])) && residual < 0.025
            && yaw_change > 1.4 && (local_path > 0.6 || yaw_change > 2.8)
        { reasons |= TETHER; break; }
    }
    reasons
}
impl Tracker {
    pub fn identify(&mut self, now: f64, id: u32, uuid: Option<u128>, age: u32) {
        if self.identities.get(&id).is_some_and(|r|
            (r.uuid.is_some() && uuid.is_some() && r.uuid != uuid) || age < r.age)
        { self.invalidate(id, now); }
        let r = self.identities.entry(id).or_insert_with(|| {
            self.generation = self.generation.wrapping_add(1);
            Identity { token: Token { id, epoch: self.epoch, generation: self.generation }, uuid, age, seen: now }
        });
        r.seen = now;
        r.age = age;
        if uuid.is_some() { r.uuid = uuid; }
    }
    pub fn observe(&mut self, now: f64, o: Observation, sensitivity: Sensitivity) {
        self.identify(now, o.id, o.uuid, o.age);
        if !self.records.contains_key(&o.id) && self.records.len() < MAX_ENTITIES {
            self.records.insert(o.id, Record {
                born: now, seen: now, history: History::default(),
                confidence: 0.0, reasons: 0, rejected: [false; 3], sensitivity, supported_until: [now; 3],
            });
        }
        let Some(r) = self.records.get_mut(&o.id) else { return; };
        r.sensitivity = sensitivity;
        r.seen = now;
        let last = r.history.last();
        if last.is_some_and(|s| now - s.time < 0.04) { return; }
        let dt = last.map_or(0.05, |s| (now - s.time).clamp(0.0, 0.25));
        // Large local teleports and long observation gaps invalidate motion
        // comparisons. They are not evidence against surrounding entities.
        if last.is_some_and(|s| length(sub(s.local, o.local)) > 8.0 || now - s.time > 0.5) {
            r.history = History::default();
            r.confidence = 0.0;
            r.rejected = [false; 3];
        }
        r.history.push(Sample { time: now, position: o.position, local: o.local, yaw: o.local_yaw.to_radians(), grounded: o.on_ground });
        let mut reasons = movement(&r.history, &o);
        if o.player {
            if o.invalid_name { reasons |= NAME; }
            if (!o.info || o.uuid.is_none()) && now - r.born > 2.0 { reasons |= INFO; }
            if o.duplicate { reasons |= DUPLICATE; }
        }
        if !(0.5..=2.0).contains(&o.scale)
            || (o.player && o.size.iter().any(|s| !s.is_finite() || *s < 0.1 || *s > 4.0))
        { reasons |= SHAPE; }
        if o.invisible { reasons |= INVISIBLE; }
        if now - r.born < 2.0 && length(sub(o.position, o.local)) < 8.0
            && self.removals.iter().filter(|at| now - **at < 5.0).count() >= 2
        { reasons |= CHURN; }
        // Correlated observations are capped within their evidence family:
        // name/list/UUID, presentation, relation to local player, physical
        // motion, and lifecycle. No family can independently reject a target.
        let families = [
            if reasons & DUPLICATE != 0 { 2 } else { i32::from(reasons & (NAME | INFO) != 0) },
            if reasons & (SHAPE | INVISIBLE) != 0 { 2 } else { 0 },
            if reasons & (ORBIT | TETHER) != 0 { 4 } else { 0 },
            if reasons & (HOVER | JUMPS) != 0 { 3 } else { 0 },
            if reasons & CHURN != 0 { 2 } else { 0 },
        ];
        let desired = f64::from(families.iter().sum::<i32>() * 10).min(100.0);
        if desired >= r.confidence {
            r.confidence = desired.min(r.confidence + dt * 200.0);
        } else {
            r.confidence = desired.max(r.confidence - dt * 30.0);
        }
        // Share observations and confidence, but retain each sensitivity's
        // recovery state so one combat module cannot change another's policy.
        let corroborated = families.iter().filter(|score| **score > 0).count() >= 2;
        for (index, threshold) in [80.0, 60.0, 50.0].into_iter().enumerate() {
            let supported = corroborated && desired >= threshold;
            if supported { r.supported_until[index] = now + 0.5; }
            if supported && r.confidence + 1e-6 >= threshold {
                r.rejected[index] = true;
            } else if r.confidence < threshold - 20.0 || now > r.supported_until[index] {
                r.rejected[index] = false;
            }
        }
        // Retain the supporting explanation during the short recovery period.
        if reasons != 0 || !r.rejected.iter().any(|rejected| *rejected) { r.reasons = reasons; }
    }
    pub fn decision(&self, id: u32) -> Decision {
        self.decision_for(id, self.records.get(&id).map_or(Sensitivity::Balanced, |r| r.sensitivity))
    }
    pub fn decision_for(&self, id: u32, sensitivity: Sensitivity) -> Decision {
        let Some(r) = self.records.get(&id) else { return Decision { class: Class::Trusted, confidence: 0, reasons: 0, observed: false }; };
        Decision {
            class: if r.rejected[sensitivity.index()] { Class::Rejected } else if r.confidence >= 10.0 { Class::Suspicious } else { Class::Trusted },
            confidence: r.confidence.round() as u8, reasons: r.reasons, observed: true,
        }
    }
    pub fn token(&self, id: u32) -> Option<Token> { self.identities.get(&id).map(|r| r.token) }
    pub fn current(&self, token: Token) -> bool { self.token(token.id) == Some(token) }
    pub fn invalidate(&mut self, id: u32, now: f64) {
        self.identities.remove(&id);
        if let Some(r) = self.records.remove(&id)
            && now - r.born < 1.0
            && r.history.last().is_some_and(|s| length(sub(s.position, s.local)) < 8.0)
        {
            if self.removals.len() == 32 { self.removals.pop_front(); }
            self.removals.push_back(now);
        }
    }
    pub fn clear(&mut self) {
        self.records.clear();
        self.identities.clear();
        self.removals.clear();
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn finish(&mut self, now: f64) {
        self.records.retain(|_, r| r.seen == now);
        self.identities.retain(|_, r| r.seen == now);
        while self.removals.front().is_some_and(|at| now - at > 5.0) { self.removals.pop_front(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn player(id: u32) -> Observation {
        Observation {
            id, uuid: Some(u128::from(id)), player: true, info: true,
            invalid_name: false, duplicate: false, mode: Some(0),
            position: [0.0, 0.0, 2.0], local: [0.0; 3], local_yaw: 0.0,
            scale: 1.0, size: [0.6, 1.8, 0.6], on_ground: true,
            flying: false, age: 0, invisible: false,
        }
    }
    fn run(t: &mut Tracker, steps: u32, mut f: impl FnMut(u32, &mut Observation)) {
        for step in 0..steps {
            let mut o = player(1);
            o.age = step;
            f(step, &mut o);
            t.observe(f64::from(step) * 0.05, o, Sensitivity::Balanced);
        }
    }
    fn orbit(step: u32, o: &mut Observation) {
        let a = f64::from(step) * 0.25;
        o.position = [2.0 * a.sin(), 1.5, 2.0 * a.cos()];
        o.on_ground = false;
    }
    #[test]
    fn listed_orbiting_hover_decoy_does_not_displace_a_real_target() {
        let mut t = Tracker::default();
        for step in 0..100 {
            let now = f64::from(step) * 0.05;
            let mut decoy = player(1);
            orbit(step, &mut decoy);
            decoy.age = step;
            t.observe(now, decoy, Sensitivity::Balanced);
            let mut real = player(2);
            real.age = step;
            real.position = [0.0, 0.0, 2.8];
            t.observe(now, real, Sensitivity::Balanced);
            t.finish(now);
        }
        assert_eq!(t.decision(1).class, Class::Rejected);
        assert_eq!(t.decision(2).class, Class::Trusted);
        // A closer decoy is excluded by the same decision every targeting path consumes.
        let selected = [1, 2].into_iter().find(|id| t.decision(*id).class != Class::Rejected);
        assert_eq!(selected, Some(2));
    }
    #[test]
    fn listed_miniature_orbiter_is_rejected_without_hover_or_bad_name() {
        let mut t = Tracker::default();
        run(&mut t, 100, |step, o| {
            orbit(step, o);
            o.position[1] = 0.0;
            o.on_ground = true;
            o.scale = 0.25;
        });
        assert_eq!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn attributes_names_and_missing_info_never_reject_on_their_own() {
        for case in 0..6 {
            let mut t = Tracker::default();
            run(&mut t, 140, |_, o| match case {
                0 => o.info = false,
                1 => { o.scale = 0.25; o.size = [0.15, 0.45, 0.15]; },
                2 => o.invisible = true,
                3 => o.invalid_name = true,
                4 => o.duplicate = true,
                _ => { o.info = false; o.uuid = None; o.invalid_name = true; },
            });
            assert_ne!(t.decision(1).class, Class::Rejected, "case {case}");
        }
    }
    #[test]
    fn ordinary_circle_strafing_and_flight_remain_selectable() {
        for flying in [false, true] {
            let mut t = Tracker::default();
            run(&mut t, 100, |step, o| {
                orbit(step, o);
                o.position[1] = if flying { 1.5 } else { 0.0 };
                o.on_ground = !flying;
                o.flying = flying;
            });
            assert_ne!(t.decision(1).class, Class::Rejected);
        }
    }
    #[test]
    fn delayed_invisible_nonplayer_tether_combines_presentation_with_motion() {
        let mut t = Tracker::default();
        run(&mut t, 140, |step, o| {
            let delayed = f64::from(step.saturating_sub(5));
            let yaw = delayed * 0.15;
            o.player = false;
            o.info = false;
            o.mode = None;
            o.invisible = true;
            o.on_ground = false;
            o.local = [f64::from(step) * 0.08, 0.0, 0.0];
            o.local_yaw = f64::from(step) * 0.15 * 180.0 / PI;
            o.position = [delayed * 0.08 + yaw.sin() * 2.0, 1.5, yaw.cos() * -2.0];
        });
        assert_eq!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn a_static_neighbor_and_a_straight_line_follower_are_not_tethers() {
        for moving in [false, true] {
            let mut t = Tracker::default();
            run(&mut t, 100, |step, o| {
                o.invisible = true;
                o.local[0] = if moving { f64::from(step) * 0.1 } else { 0.0 };
                o.position[0] = o.local[0];
            });
            assert_ne!(t.decision(1).class, Class::Rejected);
        }
    }
    #[test]
    fn repeated_short_lived_scaled_decoys_accumulate_lifecycle_evidence() {
        let mut t = Tracker::default();
        let mut rejected = false;
        for cycle in 0..12 {
            let id = cycle + 1;
            for step in 0..16 {
                let now = f64::from(cycle * 16 + step) * 0.05;
                let mut o = player(id);
                o.age = step;
                o.info = false;
                o.scale = 0.2;
                o.invisible = true;
                orbit(step, &mut o);
                t.observe(now, o, Sensitivity::Balanced);
                rejected |= t.decision(id).class == Class::Rejected;
            }
            t.invalidate(id, f64::from((cycle + 1) * 16) * 0.05);
        }
        assert!(rejected);
    }
    #[test]
    fn uuid_changes_age_rewinds_and_removal_invalidate_pending_identity() {
        let mut t = Tracker::default();
        t.observe(0.0, player(1), Sensitivity::Balanced);
        let first = t.token(1).unwrap();
        let mut o = player(1);
        o.uuid = Some(77);
        o.age = 20;
        t.observe(1.0, o, Sensitivity::Balanced);
        assert!(!t.current(first));
        let second = t.token(1).unwrap();
        o.age = 0;
        t.observe(1.1, o, Sensitivity::Balanced);
        assert!(!t.current(second));
        let third = t.token(1).unwrap();
        t.invalidate(1, 1.2);
        t.observe(1.2, o, Sensitivity::Balanced);
        assert!(!t.current(third));
        let fourth = t.token(1).unwrap();
        t.clear();
        t.observe(1.3, o, Sensitivity::Balanced);
        assert!(!t.current(fourth));
    }
    #[test]
    fn stale_records_are_removed_and_missing_observations_do_not_add_evidence() {
        let mut t = Tracker::default();
        t.observe(0.0, player(1), Sensitivity::Balanced);
        let token = t.token(1).unwrap();
        t.finish(0.1);
        assert!(!t.current(token));
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn a_local_teleport_does_not_make_every_nearby_player_a_decoy() {
        let mut t = Tracker::default();
        run(&mut t, 120, |step, o| {
            o.local = [if step % 20 < 10 { 0.0 } else { 100.0 }, 0.0, 0.0];
            o.position[0] = o.local[0] + 2.0;
            o.invisible = true;
        });
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn past_suspicion_decays_after_the_motion_stops() {
        let mut t = Tracker::default();
        run(&mut t, 100, orbit);
        assert_eq!(t.decision(1).class, Class::Rejected);
        for step in 100..240 {
            let mut o = player(1);
            o.age = step;
            t.observe(f64::from(step) * 0.05, o, Sensitivity::Balanced);
        }
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn sensitivity_changes_borderline_decisions_without_a_single_signal_rejection() {
        for (sensitivity, want) in [(Sensitivity::Conservative, false), (Sensitivity::Balanced, true), (Sensitivity::Sensitive, true)] {
            let mut t = Tracker::default();
            for step in 0..100 {
                let mut o = player(1);
                o.age = step;
                orbit(step, &mut o);
                t.observe(f64::from(step) * 0.05, o, sensitivity);
            }
            assert_eq!(t.decision(1).class == Class::Rejected, want);
        }
    }
    #[test]
    fn changing_to_conservative_releases_a_borderline_rejection() {
        let mut t = Tracker::default();
        run(&mut t, 100, orbit);
        assert_eq!(t.decision(1).class, Class::Rejected);
        let mut o = player(1);
        o.age = 100;
        orbit(100, &mut o);
        t.observe(5.0, o, Sensitivity::Conservative);
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn combat_sensitivities_share_evidence_without_changing_each_others_verdicts() {
        let mut t = Tracker::default();
        run(&mut t, 100, orbit);
        let confidence = t.decision(1).confidence;
        for _ in 0..100 {
            assert_ne!(t.decision_for(1, Sensitivity::Conservative).class, Class::Rejected);
            assert_eq!(t.decision_for(1, Sensitivity::Balanced).class, Class::Rejected);
            assert_eq!(t.decision_for(1, Sensitivity::Sensitive).class, Class::Rejected);
        }
        assert_eq!(t.decision(1).confidence, confidence);
        for step in 100..240 {
            let mut o = player(1);
            o.age = step;
            t.observe(f64::from(step) * 0.05, o, Sensitivity::Balanced);
        }
        for sensitivity in [Sensitivity::Conservative, Sensitivity::Balanced, Sensitivity::Sensitive] {
            assert_ne!(t.decision_for(1, sensitivity).class, Class::Rejected);
        }
    }
    #[test]
    fn repeated_queries_and_duplicate_samples_cannot_increase_confidence() {
        let mut t = Tracker::default();
        let mut o = player(1);
        o.scale = 0.2;
        t.observe(0.0, o, Sensitivity::Balanced);
        let before = t.decision(1).confidence;
        for _ in 0..1000 {
            t.observe(0.0, o, Sensitivity::Balanced);
            let _ = t.decision(1);
        }
        assert_eq!(t.decision(1).confidence, before);
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn unknown_player_mode_does_not_establish_illegal_flight() {
        let mut t = Tracker::default();
        run(&mut t, 140, |step, o| {
            orbit(step, o);
            o.info = false;
            o.mode = None;
        });
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn rejection_cannot_persist_on_one_remaining_family() {
        let mut t = Tracker::default();
        run(&mut t, 100, orbit);
        assert_eq!(t.decision(1).class, Class::Rejected);
        for step in 100..240 {
            let mut o = player(1);
            o.age = step;
            orbit(step, &mut o);
            o.on_ground = true;
            t.observe(f64::from(step) * 0.05, o, Sensitivity::Balanced);
        }
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
    #[test]
    fn history_capacity_never_prevents_new_target_identity_validation() {
        let mut t = Tracker::default();
        for id in 1..=1100 {
            t.observe(0.0, player(id), Sensitivity::Balanced);
        }
        let token = t.token(1100).expect("a real target beyond the history budget still needs an identity");
        assert!(t.current(token));
        assert_ne!(t.decision(1100).class, Class::Rejected);
        t.invalidate(1100, 0.1);
        t.observe(0.1, player(1100), Sensitivity::Balanced);
        assert!(!t.current(token));
    }
    #[test]
    fn circling_a_stationary_invisible_opponent_does_not_make_them_a_tether() {
        let mut t = Tracker::default();
        run(&mut t, 100, |step, o| {
            let a = f64::from(step) * 0.15;
            o.local = [a.sin() * 2.0, 0.0, a.cos() * 2.0];
            o.local_yaw = 180.0 - a.to_degrees();
            o.position = [0.0; 3];
            o.invisible = true;
        });
        assert_ne!(t.decision(1).class, Class::Rejected);
    }
}
