// The fall a mace smash goes by (Rust's PR #16), for Auto Mace. A smash resets the server's fall (vanilla's mace
// does), and the client is never told: from then the fall is what you drop below the smash's height, or below the top
// of a rise after it (a Wind Burst bounce), so the next smash of a chain waits for Min fall again, and nothing takes a
// mace for a fall the server no longer has.
#[derive(Default, Debug)]
pub(super) struct Smash {
    top: Option<f64>,
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
}
