// Mace's plan for a fall (Rust's PR #16): break a raised shield with an axe first, hold the smash for a better
// moment, smash, or nothing. It decides only; melee.rs does the hit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Timing {
    Direct,
    Delayed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Intent {
    Idle,
    BreakShield { slot: usize },
    Hold,
    Smash,
}

// What the plan sees this tick.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
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
    // The target is still in reach along the look a tick from now, with nothing in the way of the next step.
    pub can_continue: bool,
    pub timing: Timing,
    pub charge: f64,
    pub charge_next: f64,
    pub target_immune: bool,
    pub sprinting: bool,
}

// One smash a fall, and the axe hit sent at a shield (not a confirmed break) for a second.
#[derive(Default, Debug)]
pub(super) struct Smash {
    spent: bool,
    shield_attempt: Option<(u32, u64)>,
}

impl Smash {
    pub(super) fn observe_grounded(&mut self, grounded: bool) {
        if grounded {
            self.spent = false;
        }
    }
    pub(super) fn spent(&self) -> bool {
        self.spent
    }
    pub(super) fn shield_attempted(&self, id: u32, tick: u64) -> bool {
        self.shield_attempt.is_some_and(|(target, until)| target == id && tick < until)
    }
    pub(super) fn plan(&mut self, f: Frame) -> Intent {
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
                f.axe.filter(|slot| *slot < 9).map_or(Intent::Idle, |slot| Intent::BreakShield { slot })
            } else {
                Intent::Idle
            };
        }
        // Hold only while this target stays in reach along the next tick's look.
        let hold = f.timing == Timing::Delayed
            || (f.charge <= 0.9 && f.charge_next > 0.9)
            || f.target_immune
            || f.sprinting;
        if f.can_continue && hold { Intent::Hold } else { Intent::Smash }
    }
    // After the hit went out: planning, refused turns and failed hits never use up a fall.
    pub(super) fn accepted(&mut self, intent: Intent, target: u32, tick: u64) {
        match intent {
            Intent::BreakShield { .. } => self.shield_attempt = Some((target, tick.saturating_add(20))),
            Intent::Smash => self.spent = true,
            Intent::Idle | Intent::Hold => {}
        }
    }
}
