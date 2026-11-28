// The Anti Bot fixture, after Sunset's tick: a player marked for it moves away while a strike waits for its tick.
fn sunset_probe_antibot_after_tick(value: (), game: &mut crate::game::Game<'_>) {
    crate::game::sunset_probe_antibot::evade_pending(game);
    value
}
