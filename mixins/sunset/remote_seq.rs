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

// The block being broken by hand and how far it is (0 to 1), for Speed Mine.
pub(crate) fn breaking(game: &Game<'_>) -> Option<([i32; 3], f32)> {
    game.remote.as_ref()?.destroying.map(|(pos, _, done)| (pos, done))
}

pub(crate) fn set_breaking(game: &mut Game<'_>, done: f32) {
    if let Some((_, _, d)) = game.remote.as_mut().and_then(|r| r.destroying.as_mut()) {
        *d = done;
    }
}

// No Break Reset: the item in hand is no longer the one the break began with (the game would start it again).
pub(crate) fn breaking_item_changed(game: &Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|r| r.destroying.is_some() && r.destroying_item != game.main_hand_key())
}

// ...so the break goes on with the new one.
pub(crate) fn keep_breaking(game: &mut Game<'_>) {
    let held = game.main_hand_key();
    if let Some(r) = game.remote.as_mut() {
        r.destroying_item = held;
    }
}
