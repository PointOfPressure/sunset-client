// Published afresh during HUD construction, before this frame is submitted,
// including disabled and hidden frames. draw_outline runs on the same thread.
// Other renderer entry points use loading_frame(), which has no outline edges.
use std::cell::Cell;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub fn active() -> bool {
    ACTIVE.with(Cell::get)
}

pub fn publish(suppress: bool) {
    ACTIVE.with(|active| active.set(suppress));
}
