// Flight's Packet anti kick asks for the next move packet to carry the position (LocalPlayer.positionReminder = 20).
// The window's thread only.

use std::cell::Cell;

thread_local! {
    static REMIND: Cell<bool> = const { Cell::new(false) };
}

pub fn remind() {
    REMIND.with(|c| c.set(true));
}

pub fn take() -> bool {
    REMIND.with(|c| c.replace(false))
}
