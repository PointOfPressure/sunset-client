// The Anti Bot fixture: a player marked for it is replaced between a queued look and the deed it was for.
fn sunset_probe_antibot_turn(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_probe_antibot::replace_on_turn(game);
}
