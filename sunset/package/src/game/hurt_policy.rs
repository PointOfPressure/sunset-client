// The hurt time of players hit: a damage event from the server confirms a hit and starts the window in which more
// damage does less. Only for choosing targets: damage to you or a friend is never discounted.
pub const TARGET_CAP: usize = 256;
pub const MAX_AHEAD: u32 = 20;
const HALF_COOLDOWN: u64 = 10;
const REMOVAL_GRACE: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotSource {
    Crystal(u32),
    Anchor([f64; 3]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    PlayerExplosion,
    AnchorExplosion,
    PlayerMelee,
    MobMelee,
    Other,
}
#[derive(Clone, Copy, Debug)]
pub struct DamageNotice {
    pub victim: u32,
    pub enemy_player: bool,
    pub kind: DamageKind,
    pub cause: Option<u32>,
    pub direct: Option<u32>,
    pub position: Option<[f64; 3]>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Baseline {
    Unknown,
    MeleeAssumption,
    Predicted(f32),
}
#[derive(Clone, Copy, Debug)]
struct InFlight {
    began: u64,
    raw: f32,
    source: ShotSource,
    confirmed: bool,
    removed: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
struct Entry {
    victim: u32,
    observed_opens: u64,
    baseline: Baseline,
    in_flight: Option<InFlight>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    FreshWindow,
    PredictedExcess,
    MeleeBaselineAssumption,
    UnknownPreviousDamage,
    Capacity,
    ClockRewind,
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Assessment {
    pub raw_for_reduction: f32,
    pub basis: Basis,
    pub opens_in: u32,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub confirmed: u64,
    pub unconfirmed_removals: u64,
    pub overflow: u64,
    pub invalid: u64,
}
pub struct Clock {
    entries: Vec<Entry>,
    scene: Option<u64>,
    now: u64,
    margin: u8,
    confirmations: u8,
    rewind_until: u64,
    overflow_until: u64,
    counters: Counters,
}
impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}
impl Clock {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            scene: None,
            now: 0,
            margin: 0,
            confirmations: 0,
            rewind_until: 0,
            overflow_until: 0,
            counters: Counters { confirmed: 0, unconfirmed_removals: 0, overflow: 0, invalid: 0 },
        }
    }
    pub fn reset(&mut self, scene: u64, tick: u64) {
        *self = Self { scene: Some(scene), now: tick, ..Self::new() };
    }
    // Call once per native client tick. Scene is a caller-owned generation
    // derived from retained connection/world identity, never a pointer hash.
    pub fn tick(&mut self, scene: u64, tick: u64, mut alive: impl FnMut(u32) -> bool) {
        if self.scene != Some(scene) {
            self.reset(scene, tick);
            return;
        }
        if tick < self.now {
            self.reset(scene, tick);
            self.rewind_until = tick.saturating_add(HALF_COOLDOWN);
            return;
        }
        self.now = tick;
        self.entries.retain(|entry| alive(entry.victim));
        for entry in &mut self.entries {
            let expired = entry.in_flight.is_some_and(|shot| {
                !shot.confirmed && shot.removed.is_some_and(|removed| tick.saturating_sub(removed) > REMOVAL_GRACE)
            });
            if expired {
                entry.in_flight = None;
                self.margin = self.margin.saturating_add(1).min(2);
                self.confirmations = 0;
                self.counters.unconfirmed_removals = self.counters.unconfirmed_removals.saturating_add(1);
            }
        }
    }
    fn entry(&mut self, victim: u32) -> Option<&mut Entry> {
        if let Some(index) = self.entries.iter().position(|e| e.victim == victim) {
            return self.entries.get_mut(index);
        }
        if self.entries.len() == TARGET_CAP {
            self.overflow_until = self.now.saturating_add(MAX_AHEAD.into());
            self.counters.overflow = self.counters.overflow.saturating_add(1);
            return None;
        }
        self.entries.push(Entry { victim, observed_opens: 0, baseline: Baseline::Unknown, in_flight: None });
        self.entries.last_mut()
    }
    fn fresh(&self, victim: u32, ahead: u32, lead: u32) -> bool {
        if self.scene.is_none()
            || self.now < self.rewind_until
            || (self.now < self.overflow_until && !self.entries.iter().any(|e| e.victim == victim))
        {
            return false;
        }
        let arrives = self.now.saturating_add(u64::from(ahead.min(MAX_AHEAD)));
        let Some(entry) = self.entries.iter().find(|e| e.victim == victim) else {
            return true;
        };
        let observes_open = arrives.saturating_add(u64::from(lead.min(MAX_AHEAD))) >= entry.observed_opens;
        let own_open = entry
            .in_flight
            .is_none_or(|shot| arrives >= shot.began.saturating_add(HALF_COOLDOWN + u64::from(self.margin)));
        observes_open && own_open
    }
    pub fn opens_in(&self, victim: u32, lead: u32) -> u32 {
        (0..MAX_AHEAD).find(|ahead| self.fresh(victim, *ahead, lead)).unwrap_or(MAX_AHEAD)
    }
    pub fn assess(&self, victim: u32, raw: f32, ahead: u32, lead: u32) -> Assessment {
        let opens_in = self.opens_in(victim, lead);
        let result = |raw_for_reduction, basis| Assessment { raw_for_reduction, basis, opens_in };
        if self.scene.is_none() {
            return result(0.0, Basis::Invalid);
        }
        if !raw.is_finite() || raw < 0.0 {
            return result(0.0, Basis::Invalid);
        }
        if self.now < self.rewind_until {
            return result(0.0, Basis::ClockRewind);
        }
        if self.now < self.overflow_until && !self.entries.iter().any(|e| e.victim == victim) {
            return result(0.0, Basis::Capacity);
        }
        if self.fresh(victim, ahead, lead) {
            return result(raw, Basis::FreshWindow);
        }
        match self.entries.iter().find(|e| e.victim == victim).map(|e| e.baseline).unwrap_or(Baseline::Unknown) {
            Baseline::Unknown => result(0.0, Basis::UnknownPreviousDamage),
            Baseline::MeleeAssumption => result(raw, Basis::MeleeBaselineAssumption),
            Baseline::Predicted(previous) => result((raw - previous).max(0.0), Basis::PredictedExcess),
        }
    }
    // Invoke ONLY after the final accepted attack/use was actually forwarded.
    // A queued turn, item decrement or optimistic removal is not a shot.
    pub fn sent(&mut self, victim: u32, raw: f32, source: ShotSource, lead: u32) -> bool {
        if self.scene.is_none() || !raw.is_finite() || raw <= 0.0 || !valid_source(source) {
            self.counters.invalid = self.counters.invalid.saturating_add(1);
            return false;
        }
        let fresh = self.fresh(victim, 0, lead);
        let now = self.now;
        let Some(entry) = self.entry(victim) else {
            return false;
        };
        if fresh {
            entry.in_flight = Some(InFlight { began: now, raw, source, confirmed: false, removed: None });
            entry.baseline = Baseline::Predicted(raw);
        } else {
            entry.baseline = match entry.baseline {
                Baseline::Unknown => Baseline::Unknown,
                Baseline::MeleeAssumption => Baseline::Predicted(raw),
                Baseline::Predicted(old) => Baseline::Predicted(old.max(raw)),
            };
        }
        true
    }
    // A native DamageEvent represents a fresh hurt dispatch. The protocol
    // carries no damage amount. Cooldown reconstruction is independent from
    // whether a player qualifies for shared offensive-history bookkeeping.
    pub fn damaged(&mut self, own_entity: u32, notice: DamageNotice) {
        if self.scene.is_none() {
            return;
        }
        let now = self.now;
        let Some(entry) = self.entry(notice.victim) else {
            return;
        };
        entry.observed_opens = now.saturating_add(HALF_COOLDOWN);
        if !notice.enemy_player {
            return;
        }
        let matched = entry.in_flight.filter(|shot| !shot.confirmed && matches_source(shot.source, own_entity, notice));
        if let Some(shot) = matched {
            entry.in_flight.as_mut().unwrap().confirmed = true;
            // Confirmation identifies the originally predicted shot. It does
            // not certify its amount, nor a stronger speculative follow-up.
            entry.baseline = Baseline::Predicted(shot.raw);
            self.counters.confirmed = self.counters.confirmed.saturating_add(1);
            if matches!(shot.source, ShotSource::Crystal(_)) {
                self.confirmations += 1;
                if self.confirmations == 8 {
                    self.margin = self.margin.saturating_sub(1);
                    self.confirmations = 0;
                }
            }
        } else {
            entry.in_flight = None;
            entry.baseline = match notice.kind {
                DamageKind::PlayerMelee | DamageKind::MobMelee => Baseline::MeleeAssumption,
                _ => Baseline::Unknown,
            };
        }
    }
    pub fn removed(&mut self, ids: &[u32]) {
        for entry in &mut self.entries {
            if let Some(shot) = entry.in_flight.as_mut().filter(|s| !s.confirmed) {
                if let ShotSource::Crystal(id) = shot.source {
                    if ids.iter().take(4096).any(|removed| *removed == id) {
                        shot.removed = Some(self.now);
                    }
                }
            }
        }
    }
}
fn valid_source(source: ShotSource) -> bool {
    match source {
        ShotSource::Crystal(_) => true,
        ShotSource::Anchor(p) => p.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0),
    }
}
fn matches_source(source: ShotSource, own: u32, notice: DamageNotice) -> bool {
    match source {
        ShotSource::Crystal(id) => {
            notice.kind == DamageKind::PlayerExplosion && notice.cause == Some(own) && notice.direct == Some(id)
        }
        ShotSource::Anchor(center) => {
            notice.kind == DamageKind::AnchorExplosion
                && notice.position.is_some_and(|p| {
                    p.iter().all(|v| v.is_finite()) && (0..3).map(|i| (p[i] - center[i]).powi(2)).sum::<f64>() < 0.25
                })
        }
    }
}

#[derive(Clone, Copy)]
struct Placement {
    base: [i32; 3],
    sent: u64,
}
// Original bounded receipt estimator: only observed crystal spawns sample RTT.
pub struct ReceiptTiming {
    pending: Vec<Placement>,
    smoothed: Option<f64>,
}
impl Default for ReceiptTiming {
    fn default() -> Self {
        Self::new()
    }
}
impl ReceiptTiming {
    pub const fn new() -> Self {
        Self { pending: Vec::new(), smoothed: None }
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    pub fn placed(&mut self, base: [i32; 3], tick: u64) -> bool {
        self.pending.retain(|p| tick >= p.sent && tick - p.sent <= 20);
        if let Some(old) = self.pending.iter_mut().find(|p| p.base == base) {
            old.sent = tick;
            return true;
        }
        if self.pending.len() == 64 {
            return false;
        }
        self.pending.push(Placement { base, sent: tick });
        true
    }
    pub fn spawned(&mut self, base: [i32; 3], tick: u64) -> bool {
        let Some(index) = self.pending.iter().position(|p| p.base == base) else {
            return false;
        };
        let placement = self.pending.swap_remove(index);
        let Some(age) = tick.checked_sub(placement.sent).filter(|age| (1..=20).contains(age)) else {
            return false;
        };
        let sample = age as f64;
        self.smoothed = Some(self.smoothed.map_or(sample, |prior| (prior * 3.0 + sample) / 4.0));
        true
    }
    pub fn rtt_ticks(&self, tab_latency_ms: i32) -> u32 {
        self.smoothed.map_or((tab_latency_ms.max(0) as u32 / 50).saturating_add(1), |sample| sample.round() as u32)
    }
}
