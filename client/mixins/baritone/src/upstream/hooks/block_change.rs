fn baritone_block_change(game: &mut crate::game::Game<'_>, pos: [i32; 3], state: i32) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::block_changed(game, pos, state);
}
