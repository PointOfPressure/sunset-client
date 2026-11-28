fn baritone_block_interact(game: &mut crate::game::Game<'_>) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::block_interact_use(game);
}
