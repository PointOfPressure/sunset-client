fn baritone_keys_remote(game: &mut crate::game::Game<'_>) {
    if !crate::game::sunset_baritone_full::present(game) { return; }
    crate::game::baritone_input::through(game);
    crate::game::baritone::look_pre(game);
}
