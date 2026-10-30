// Things Sunset needs from the connection's private state.
use crate::game::Game;

// The game's own sequence counter (the server wants block-action sequence
// numbers that never go down), for Sunset's placing and breaking.
pub(crate) fn next(game: &mut Game<'_>) -> i32 {
    game.next_sequence()
}

// The wait the game puts between one block's breaking and the next (5 ticks).
pub(crate) fn clear_destroy_delay(game: &mut Game<'_>, wait: u32) {
    if let Some(r) = game.remote.as_mut() {
        r.destroy_delay = r.destroy_delay.min(wait);
    }
}
