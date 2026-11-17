fn baritone_keys_after(game: &mut crate::game::Game<'_>) {
    crate::game::baritone_input::back(game);
    // In the remote tick the state goes to the server after this; the rotation comes back at the start of Baritone's tick.
    if game.remote.is_none() {
        crate::game::baritone::look_post(game);
    }
}
