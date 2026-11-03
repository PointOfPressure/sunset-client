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

// The fall the server counts after a smash, and the axe hit sent at a shield (not a confirmed break) for a second. A
// smash resets the server's fall (vanilla's mace does), and the client is never told: from then the fall is what you
// drop below the smash's height, or below the top of a rise after it (a Wind Burst bounce), so the next smash of a
// chain waits for Min fall again, and nothing takes a mace for a fall the server no longer has.
#[derive(Default, Debug)]
pub(super) struct Smash {
    top: Option<f64>,
    shield_attempt: Option<(u32, u64)>,
}

impl Smash {
    // Each tick: the ground ends the count, a rise moves its top up.
    pub(super) fn observe(&mut self, grounded: bool, y: f64) {
        if grounded || !y.is_finite() {
            self.top = None;
        } else if let Some(top) = &mut self.top {
            *top = top.max(y);
        }
    }
    // A smash went out at height `y`.
    pub(super) fn smashed(&mut self, y: f64) {
        if y.is_finite() {
            self.top = Some(y);
        }
    }
    // The fall the server has: the game's own `native`, or no more than what you dropped since the last smash.
    pub(super) fn fall(&self, native: f64, y: f64) -> f64 {
        let native = if native.is_finite() { native.max(0.0) } else { 0.0 };
        match self.top {
            Some(top) if y.is_finite() => (top - y).clamp(0.0, native),
            Some(_) => 0.0,
            None => native,
        }
    }
    pub(super) fn shield_attempted(&self, id: u32, tick: u64) -> bool {
        self.shield_attempt.is_some_and(|(target, until)| target == id && tick < until)
    }
    pub(super) fn plan(&mut self, f: Frame) -> Intent {
        self.shield_attempt = self.shield_attempt.filter(|(_, until)| f.tick < *until);
        if f.grounded
            || !f.descending
            || !f.native_smash
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
    // After the axe hit at a shield went out. (A smash counts from its own packet: see `smashed`.)
    pub(super) fn accepted(&mut self, intent: Intent, target: u32, tick: u64) {
        if let Intent::BreakShield { .. } = intent {
            self.shield_attempt = Some((target, tick.saturating_add(20)));
        }
    }
}
