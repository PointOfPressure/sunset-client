// The Anti Bot fixture's turn each client tick, before the tick's input: its command file, its players' moves, its state.
fn sunset_probe_antibot_tick(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_probe_antibot::tick(game);
}
