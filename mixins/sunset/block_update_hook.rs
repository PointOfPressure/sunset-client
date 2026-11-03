// New Chunks' liquid method: each block the server changes.
fn sunset_block_update(game: &crate::game::Game<'_>, pos: [i32; 3], state: i32) {
    crate::game::sunset_new_chunks::block_update(game, pos, state);
}
