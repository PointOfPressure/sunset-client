fn baritone_keys_tick(game: &mut crate::game::Game<'_>) {
    crate::game::baritone_input::back(game);
    crate::game::baritone_input::mend(game);
}
