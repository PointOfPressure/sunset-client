// Original fixed-storage recent-death protection. No world or inventory edits.
pub const CAPACITY: usize = 16;
pub const TTL_MS: u64 = 30_000;
pub const DROP_MARGIN: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason { Clear, NearDeath, Overflow, InvalidBlast, ClockRewind, UnknownScene }
impl Reason {
    pub const fn text(self) -> &'static str {
        match self { Self::Clear=>"clear",Self::NearDeath=>"recent-player-death",Self::Overflow=>"death-capacity-or-position-unknown",
            Self::InvalidBlast=>"invalid-blast",Self::ClockRewind=>"clock-rewind",Self::UnknownScene=>"scene-unavailable" }
    }
}
#[derive(Clone, Copy)]
struct Spot { entity: i32, at: [f64; 3], until: u64 }
pub struct Guard { spots: [Option<Spot>; CAPACITY], overflow_until: u64, last_ms: u64 }
impl Guard {
    pub const fn new() -> Self { Self { spots: [None; CAPACITY], overflow_until: 0, last_ms: 0 } }
    pub fn clear(&mut self) { *self = Self::new(); }
    fn expired(until: u64, now: u64) -> bool { until != u64::MAX && until <= now }
    fn prune(&mut self, now: u64) {
        for spot in &mut self.spots { if spot.is_some_and(|s| Self::expired(s.until, now)) { *spot = None; } }
        if Self::expired(self.overflow_until, now) { self.overflow_until = 0; }
    }
    pub fn unknown_death(&mut self, now: u64) {
        self.last_ms = self.last_ms.max(now);
        self.overflow_until = self.overflow_until.max(self.last_ms.saturating_add(TTL_MS));
    }
    pub fn record(&mut self, now: u64, entity: i32, at: [f64; 3]) {
        if now < self.last_ms || !valid(at) { self.unknown_death(now); return; }
        self.last_ms = now; self.prune(now);
        let until = now.saturating_add(TTL_MS);
        // Duplicate death notifications for the same known place need one cell.
        if let Some(old) = self.spots.iter_mut().flatten().find(|s| s.entity == entity && s.at == at) {
            old.until = old.until.max(until); return;
        }
        if let Some(cell) = self.spots.iter_mut().find(|s| s.is_none()) {
            *cell = Some(Spot { entity, at, until });
        } else {
            // Dropping an unexpired location would silently unprotect it.
            self.overflow_until = self.overflow_until.max(until);
        }
    }
    pub fn check(&mut self, now: u64, center: [f64; 3], power: f32) -> Reason {
        if !valid(center) || !power.is_finite() || power <= 0.0 || power > 16.0 { return Reason::InvalidBlast; }
        if now < self.last_ms { return Reason::ClockRewind; }
        self.last_ms = now; self.prune(now);
        if self.overflow_until != 0 { return Reason::Overflow; }
        let reach = f64::from(power) * 2.0 + DROP_MARGIN;
        if self.spots.iter().flatten().any(|spot| {
            (0..3).map(|i| (center[i]-spot.at[i]).powi(2)).sum::<f64>() <= reach*reach
        }) { Reason::NearDeath } else { Reason::Clear }
    }
    pub fn active(&self) -> usize { self.spots.iter().filter(|s| s.is_some()).count() }
    pub fn overflow(&self) -> bool { self.overflow_until != 0 }
    pub fn expire(&mut self, now: u64) {
        if now >= self.last_ms { self.last_ms = now; self.prune(now); }
    }
}
fn valid(at: [f64; 3]) -> bool { at.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0) }

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Dimension { bytes: [u8; 256], len: usize }
impl Dimension {
    pub fn of(name: &str) -> Option<Self> {
        if name.is_empty() || name.len() > 256 { return None; }
        let mut bytes = [0; 256]; bytes[..name.len()].copy_from_slice(name.as_bytes());
        Some(Self { bytes, len: name.len() })
    }
}
