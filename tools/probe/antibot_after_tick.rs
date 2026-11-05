fn probe_antibot_after_tick(value: (), game: &mut crate::game::Game<'_>) {
    crate::game::remote::probe_antibot::evade_pending(game);
    value
}
