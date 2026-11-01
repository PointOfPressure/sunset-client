//! Original threshold policy, derived from observed behavior of the verified reference.
//! Geometry, friend safety and HurtClock belong to separate layers.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArmorCondition {
    /// Already resolved native components: unbreakable is false here.
    pub damageable: bool,
    pub maximum: u32,
    pub lost: u32,
}
impl ArmorCondition {
    pub fn low(self) -> bool {
        self.damageable && self.maximum > 0 &&
            u64::from(self.maximum.saturating_sub(self.lost)) * 10 <= u64::from(self.maximum)
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FaceFacts {
    /// Actual health plus absorption; None means the observer cannot establish it.
    pub effective_health: Option<f64>,
    /// Four horizontal neighbors at the target's CURRENT feet cell.
    pub neighbor_resistance: [Option<f32>; 4],
    pub armor: [Option<ArmorCondition>; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason { Normal, Forced, LowHealth, Hole, LowArmor }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threshold { pub minimum: f64, pub reason: Reason }
fn health(facts: &FaceFacts) -> Option<f64> {
    facts.effective_health.filter(|v| v.is_finite() && *v > 0.0)
}
pub fn crystal(configured: f64, enabled: bool, forced: bool, breaking: bool,
               facts: &FaceFacts) -> Option<Threshold> {
    if !configured.is_finite() || configured < 0.0 { return None; }
    let reason = if forced { Reason::Forced }
        else if !enabled { Reason::Normal }
        else if health(facts).is_some_and(|hp| hp <= 10.0) { Reason::LowHealth }
        else if facts.neighbor_resistance.iter().all(|r|
            r.is_some_and(|v| v.is_finite() && v >= 600.0)) { Reason::Hole }
        else if facts.armor.iter().flatten().any(|a| a.low()) { Reason::LowArmor }
        else { Reason::Normal };
    let minimum = if reason == Reason::Normal { configured } else { configured.min(2.0) };
    Some(Threshold { minimum: minimum * if breaking { 0.6 } else { 1.0 }, reason })
}
pub fn anchor(configured: f64, effective_health: Option<f64>) -> Option<f64> {
    if !configured.is_finite() || configured < 0.0 { return None; }
    Some(effective_health.filter(|v| v.is_finite() && *v > 0.0)
         .map_or(configured, |hp| configured.min(hp)))
}
/// Call only AFTER raw hurt-window adjustment and native defense reduction.
/// Unknown absorption never establishes estimated lethality.
pub fn sufficient(incremental_estimate: f64, threshold: f64,
                  effective_health: Option<f64>) -> bool {
    incremental_estimate.is_finite() && incremental_estimate >= 0.0 &&
        threshold.is_finite() && threshold >= 0.0 &&
        (incremental_estimate >= threshold || effective_health.is_some_and(|hp|
            hp.is_finite() && hp > 0.0 && incremental_estimate >= hp))
}
