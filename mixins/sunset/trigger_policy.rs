//! Original native Trigger timing state. Engine supplies actual crit stance,
//! charge, physical inputs, first-contact ray and final accepted attack.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crits {
    Off,
    Smart,
    Only,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CritDecision {
    pub allow: bool,
    pub drop_sprint: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct CritFrame {
    pub mode: Crits,
    pub tick: u64,
    pub would_crit: bool,
    pub crit_stance: bool,
    pub grounded_jumping_without_sprint: bool,
    pub airborne: bool,
    pub sprinting: bool,
    pub fall_positive: bool,
}
#[derive(Clone, Copy, Debug)]
struct Target {
    id: u32,
    seen: u64,
    reaction_due: u64,
}
#[derive(Default, Debug)]
pub struct Trigger {
    target: Option<Target>,
    charged_due: Option<u64>,
    crit_tick: Option<u64>,
    crit_calls: u8,
}
impl Trigger {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn no_context(&mut self) {
        self.target = None;
    }
    /// Caller samples spread only on a new acquisition; pass 0..=2.
    /// None means no eligible current crosshair hit. Grace retains identity,
    /// never permission to attack a target that is no longer under the ray.
    pub fn observe_target(&mut self, tick: u64, id: Option<u32>, reaction: u8, spread: u8) {
        let Some(id) = id else {
            if self.target.is_some_and(|t| tick.saturating_sub(t.seen) > 4) {
                self.target = None;
            }
            return;
        };
        if let Some(t) = self.target.as_mut().filter(|t| t.id == id) {
            t.seen = tick;
        } else {
            self.target = Some(Target {
                id,
                seen: tick,
                reaction_due: tick
                    .saturating_add(u64::from(reaction.min(10)))
                    .saturating_add(u64::from(spread.min(2))),
            });
        }
    }
    pub fn tracked(&self) -> Option<u32> {
        self.target.map(|t| t.id)
    }
    pub fn needs_acquisition(&self, id: u32) -> bool {
        self.tracked() != Some(id)
    }
    /// Exactly the reference's air-MISS suppression, not a block-mining veto.
    pub fn swallow_air_clicks(&self, hold_attack: bool, crosshair_miss: bool) -> bool {
        hold_attack && crosshair_miss && self.target.is_some()
    }
    /// Sample pause 0..=1 only when charge transitions to full.
    pub fn observe_charge(&mut self, tick: u64, charge: f64, spread: u8) {
        if !charge.is_finite() || charge < 1.0 {
            self.charged_due = None;
        } else if self.charged_due.is_none() {
            self.charged_due = Some(tick.saturating_add(u64::from(spread.min(1))));
        }
    }
    pub fn fully_charged_seen(&self) -> bool {
        self.charged_due.is_some()
    }
    pub fn timing_allows(&self, tick: u64, current_hit: u32, smash_ready: bool) -> bool {
        self.target
            .is_some_and(|t| t.id == current_hit && t.seen == tick && tick >= t.reaction_due)
            && (smash_ready || self.charged_due.is_some_and(|due| tick >= due))
    }
    pub fn critical(&mut self, f: CritFrame) -> CritDecision {
        let mut result = CritDecision {
            allow: true,
            drop_sprint: false,
        };
        match f.mode {
            Crits::Off => {}
            Crits::Only => {
                result.drop_sprint = f.airborne && f.sprinting && f.fall_positive;
                result.allow = f.would_crit;
            }
            Crits::Smart
                if !f.would_crit && (f.crit_stance || f.grounded_jumping_without_sprint) =>
            {
                // Multiple validation passes within one native tick are
                // idempotent; a skipped eligible tick breaks the streak.
                if self.crit_tick != Some(f.tick) {
                    if self.crit_tick != f.tick.checked_sub(1) {
                        self.crit_calls = 0;
                    }
                    self.crit_calls = self.crit_calls.saturating_add(1);
                    self.crit_tick = Some(f.tick);
                }
                result.allow = self.crit_calls > 12;
            }
            Crits::Smart => {}
        }
        result
    }
    /// Only a final accepted native attack consumes the full-charge edge.
    pub fn accepted(&mut self) {
        self.charged_due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn crit(tick: u64) -> CritFrame {
        CritFrame {
            mode: Crits::Smart,
            tick,
            would_crit: false,
            crit_stance: false,
            grounded_jumping_without_sprint: false,
            airborne: false,
            sprinting: false,
            fall_positive: false,
        }
    }
    #[test]
    fn ordinary_ground_smart_does_not_wait_twelve_ticks() {
        assert!(Trigger::default().critical(crit(1)).allow);
    }
    #[test]
    fn smart_waits_twelve_consecutive_stance_ticks_not_acquisition_age() {
        let mut s = Trigger::default();
        let mut f = crit(1000);
        f.crit_stance = true;
        for tick in 1000..1012 {
            f.tick = tick;
            assert!(!s.critical(f).allow);
        }
        f.tick = 1012;
        assert!(s.critical(f).allow);
    }
    #[test]
    fn duplicate_validation_does_not_advance_and_a_gap_resets_wait() {
        let mut s = Trigger::default();
        let mut f = crit(1);
        f.crit_stance = true;
        for _ in 0..20 {
            assert!(!s.critical(f).allow);
        }
        for tick in 2..=12 {
            f.tick = tick;
            assert!(!s.critical(f).allow);
        }
        f.tick = 14;
        assert!(!s.critical(f).allow);
    }
    #[test]
    fn actual_critical_wins_immediately_and_ground_jump_enters_wait() {
        let mut s = Trigger::default();
        let mut f = crit(1);
        f.grounded_jumping_without_sprint = true;
        assert!(!s.critical(f).allow);
        f.would_crit = true;
        assert!(s.critical(f).allow);
    }
    #[test]
    fn only_requests_real_sprint_release_and_never_times_out_grounded() {
        let mut s = Trigger::default();
        let mut f = crit(1);
        f.mode = Crits::Only;
        for tick in 1..100 {
            f.tick = tick;
            assert!(!s.critical(f).allow);
        }
        f.airborne = true;
        f.sprinting = true;
        f.fall_positive = true;
        assert_eq!(
            s.critical(f),
            CritDecision {
                allow: false,
                drop_sprint: true
            }
        );
        f.sprinting = false;
        f.would_crit = true;
        assert_eq!(
            s.critical(f),
            CritDecision {
                allow: true,
                drop_sprint: false
            }
        );
    }
    #[test]
    fn short_miss_keeps_reaction_but_cannot_attack_off_ray() {
        let mut s = Trigger::default();
        s.observe_target(10, Some(9), 3, 2);
        s.observe_charge(10, 1., 0);
        s.observe_target(14, None, 3, 0);
        assert_eq!(s.tracked(), Some(9));
        assert!(!s.timing_allows(14, 9, false));
        s.observe_target(14, Some(9), 3, 0);
        assert!(!s.timing_allows(14, 9, false));
        s.observe_target(15, Some(9), 3, 0);
        assert!(s.timing_allows(15, 9, false));
    }
    #[test]
    fn five_tick_miss_reacquires_and_air_swallow_is_not_block_swallow() {
        let mut s = Trigger::default();
        s.observe_target(10, Some(9), 3, 0);
        assert!(s.swallow_air_clicks(true, true));
        assert!(!s.swallow_air_clicks(true, false));
        s.observe_target(15, None, 3, 0);
        assert_eq!(s.tracked(), None);
        assert!(s.needs_acquisition(9));
        s.observe_target(15, Some(9), 3, 0);
        assert!(!s.timing_allows(15, 9, true));
    }
    #[test]
    fn reaction_and_charge_edge_have_separate_deadlines() {
        let mut s = Trigger::default();
        s.observe_target(1, Some(9), 0, 0);
        s.observe_charge(1, 1., 1);
        assert!(!s.timing_allows(1, 9, false));
        assert!(s.timing_allows(1, 9, true));
        s.observe_target(2, Some(9), 0, 0);
        assert!(s.timing_allows(2, 9, false));
        s.accepted();
        assert!(!s.fully_charged_seen());
        s.observe_charge(3, 0.9, 0);
        assert!(!s.fully_charged_seen());
    }
    #[test]
    fn session_cancel_clears_all_timing_and_critical_memory() {
        let mut s = Trigger::default();
        s.observe_target(1, Some(9), 0, 0);
        s.observe_charge(1, 1., 0);
        s.cancel();
        assert_eq!(s.tracked(), None);
        assert!(!s.fully_charged_seen());
        assert!(!s.timing_allows(1, 9, true));
    }
}
