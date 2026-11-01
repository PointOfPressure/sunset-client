//! Original native combat policy. This file decides intentions only.
//! Inventory, ray/collision queries and accepted attacks remain native deeds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    Direct,
    Delayed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldReason {
    Timing,
    Charge,
    Immunity,
    Sprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Idle,
    BreakShield { slot: usize },
    Hold(HoldReason),
    Smash,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub tick: u64,
    pub target: u32,
    pub grounded: bool,
    pub descending: bool,
    pub native_smash: bool,
    pub fall: f64,
    pub min_fall: f64,
    pub target_allowed: bool,
    pub mace_available: bool,
    pub shield_blocks: bool,
    pub axe: Option<usize>,
    pub auto_swap: bool,
    pub swap_back: bool,
    // A fresh native next-position collision + target-ray + LOS query.
    pub can_continue: bool,
    pub timing: Timing,
    pub charge: f64,
    pub charge_next: f64,
    pub target_immune: bool,
    pub sprinting: bool,
}

#[derive(Default, Debug)]
pub struct Smash {
    spent: bool,
    // Local accepted-axe interval. It is not a server-confirmed shield disable.
    shield_attempt: Option<(u32, u64)>,
}
impl Smash {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn observe_grounded(&mut self, grounded: bool) {
        if grounded {
            self.spent = false;
        }
    }
    pub fn spent(&self) -> bool {
        self.spent
    }
    pub fn shield_attempted(&self, id: u32, tick: u64) -> bool {
        self.shield_attempt
            .is_some_and(|(target, until)| target == id && tick < until)
    }

    pub fn plan(&mut self, f: Frame) -> Intent {
        if f.grounded {
            self.spent = false;
        }
        self.shield_attempt = self.shield_attempt.filter(|(_, until)| f.tick < *until);
        if f.grounded
            || !f.descending
            || !f.native_smash
            || self.spent
            || !f.target_allowed
            || !f.mace_available
            || !f.fall.is_finite()
            || !f.min_fall.is_finite()
            || f.min_fall < 1.5
            || f.fall < f.min_fall
            || !f.charge.is_finite()
            || !f.charge_next.is_finite()
        {
            return Intent::Idle;
        }
        let shield_recent = self.shield_attempt.is_some_and(|(id, _)| id == f.target);
        if f.shield_blocks && !shield_recent {
            return if f.can_continue && (f.auto_swap || f.swap_back) {
                f.axe
                    .filter(|slot| *slot < 9)
                    .map_or(Intent::Idle, |slot| Intent::BreakShield { slot })
            } else {
                Intent::Idle
            };
        }
        // A hold is allowed only when this exact target remains reachable
        // along the next eye's sent ray, without colliding on the next step.
        if f.can_continue {
            let reason = if f.timing == Timing::Delayed {
                Some(HoldReason::Timing)
            } else if f.charge <= 0.9 && f.charge_next > 0.9 {
                Some(HoldReason::Charge)
            } else if f.target_immune {
                Some(HoldReason::Immunity)
            } else if f.sprinting {
                Some(HoldReason::Sprint)
            } else {
                None
            };
            if let Some(reason) = reason {
                return Intent::Hold(reason);
            }
        }
        Intent::Smash
    }

    // Call only after the final native deed reports acceptance. Planning,
    // rejected turns, stale tickets and failed attacks never consume a fall.
    pub fn accepted(&mut self, intent: Intent, target: u32, tick: u64) {
        match intent {
            Intent::BreakShield { .. } => {
                self.shield_attempt = Some((target, tick.saturating_add(20)))
            }
            Intent::Smash => self.spent = true,
            Intent::Idle | Intent::Hold(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        Frame {
            tick: 100,
            target: 7,
            grounded: false,
            descending: true,
            native_smash: true,
            fall: 4.,
            min_fall: 3.,
            target_allowed: true,
            mace_available: true,
            shield_blocks: false,
            axe: Some(2),
            auto_swap: true,
            swap_back: true,
            can_continue: true,
            timing: Timing::Direct,
            charge: 1.,
            charge_next: 1.,
            target_immune: false,
            sprinting: false,
        }
    }
    #[test]
    fn accepted_axe_allows_same_fall_mace_but_other_shield_is_independent() {
        let mut s = Smash::default();
        let mut f = frame();
        f.shield_blocks = true;
        let axe = s.plan(f);
        assert_eq!(axe, Intent::BreakShield { slot: 2 });
        s.accepted(axe, f.target, f.tick);
        f.tick += 1;
        assert_eq!(s.plan(f), Intent::Smash);
        f.target = 8;
        assert_eq!(s.plan(f), Intent::BreakShield { slot: 2 });
    }
    #[test]
    fn failed_or_rejected_axe_does_not_claim_shield_broken() {
        let mut s = Smash::default();
        let mut f = frame();
        f.shield_blocks = true;
        assert_eq!(s.plan(f), Intent::BreakShield { slot: 2 });
        f.tick += 1;
        assert_eq!(s.plan(f), Intent::BreakShield { slot: 2 });
    }
    #[test]
    fn shield_step_requires_safe_next_geometry_and_permitted_slot_change() {
        let mut s = Smash::default();
        let mut f = frame();
        f.shield_blocks = true;
        f.can_continue = false;
        assert_eq!(s.plan(f), Intent::Idle);
        f.can_continue = true;
        f.auto_swap = false;
        f.swap_back = false;
        assert_eq!(s.plan(f), Intent::Idle);
        f.swap_back = true;
        assert!(matches!(s.plan(f), Intent::BreakShield { .. }));
        f.axe = Some(9);
        assert_eq!(s.plan(f), Intent::Idle);
    }
    #[test]
    fn delayed_wait_is_not_a_one_tick_toggle() {
        let mut s = Smash::default();
        let mut f = frame();
        f.timing = Timing::Delayed;
        for _ in 0..4 {
            assert_eq!(s.plan(f), Intent::Hold(HoldReason::Timing));
            f.tick += 1;
        }
        f.can_continue = false;
        assert_eq!(s.plan(f), Intent::Smash);
    }
    #[test]
    fn direct_holds_only_for_actionable_charge_immunity_or_sprint() {
        let mut s = Smash::default();
        let mut f = frame();
        f.charge = 0.8;
        f.charge_next = 0.91;
        assert_eq!(s.plan(f), Intent::Hold(HoldReason::Charge));
        f.charge_next = 0.89;
        assert_eq!(s.plan(f), Intent::Smash);
        f.target_immune = true;
        assert_eq!(s.plan(f), Intent::Hold(HoldReason::Immunity));
        f.target_immune = false;
        f.sprinting = true;
        assert_eq!(s.plan(f), Intent::Hold(HoldReason::Sprint));
        f.can_continue = false;
        assert_eq!(s.plan(f), Intent::Smash);
    }
    #[test]
    fn exactly_one_accepted_smash_per_grounded_fall() {
        let mut s = Smash::default();
        let mut f = frame();
        assert_eq!(s.plan(f), Intent::Smash);
        s.accepted(Intent::Smash, f.target, f.tick);
        assert_eq!(s.plan(f), Intent::Idle);
        f.descending = false;
        assert_eq!(s.plan(f), Intent::Idle);
        f.descending = true;
        assert_eq!(s.plan(f), Intent::Idle);
        f.grounded = true;
        assert_eq!(s.plan(f), Intent::Idle);
        f.grounded = false;
        assert_eq!(s.plan(f), Intent::Smash);
    }
    #[test]
    fn shield_interval_expires_and_cancel_clears_old_context() {
        let mut s = Smash::default();
        let mut f = frame();
        f.shield_blocks = true;
        s.accepted(Intent::BreakShield { slot: 2 }, f.target, f.tick);
        f.tick = 119;
        assert_eq!(s.plan(f), Intent::Smash);
        f.tick = 120;
        assert_eq!(s.plan(f), Intent::BreakShield { slot: 2 });
        s.accepted(Intent::Smash, f.target, f.tick);
        s.cancel();
        assert_eq!(s.plan(f), Intent::BreakShield { slot: 2 });
    }
    #[test]
    fn invalid_remote_values_and_subthreshold_fall_do_not_arm() {
        let mut s = Smash::default();
        let mut f = frame();
        f.fall = f64::NAN;
        assert_eq!(s.plan(f), Intent::Idle);
        f.fall = 2.99;
        assert_eq!(s.plan(f), Intent::Idle);
        f.fall = 3.;
        f.charge = f64::INFINITY;
        assert_eq!(s.plan(f), Intent::Idle);
    }
}
