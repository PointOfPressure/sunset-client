fn probe_antibot_tick(game: &mut crate::game::Game<'_>) {
    crate::game::remote::probe_antibot::tick(game);
}
