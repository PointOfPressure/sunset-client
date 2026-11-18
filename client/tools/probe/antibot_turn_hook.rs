fn probe_antibot_turn(game: &mut crate::game::Game<'_>) {
    crate::game::remote::probe_antibot::replace_on_turn(game);
}
