// Flight's Packet anti kick asks for the next move packet to carry the position (LocalPlayer.positionReminder = 20).
// And a move packet whose ground Sunset changed: what the server was told, so the game's next update compares with
// that and sends the real ground as soon as Sunset stops changing it (else the server keeps the changed one until
// the next move). The window's thread only.

use std::cell::Cell;

thread_local! {
    static REMIND: Cell<bool> = const { Cell::new(false) };
    static GROUND: Cell<Option<bool>> = const { Cell::new(None) };
}

pub fn remind() {
    REMIND.with(|c| c.set(true));
}

pub fn take() -> bool {
    REMIND.with(|c| c.replace(false))
}

pub fn told_ground(on_ground: bool) {
    GROUND.with(|c| c.set(Some(on_ground)));
}

pub fn take_ground() -> Option<bool> {
    GROUND.with(|c| c.take())
}
