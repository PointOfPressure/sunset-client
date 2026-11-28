fn sunset_crosshair(
    game: &crate::game::Game<'_>,
    size: [u32; 2],
) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_crosshair::prepare(game, size);
}
