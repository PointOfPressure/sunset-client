// HUD construction runs before Renderer::render submits this frame's outline.
// Reset before its early exits or config reads, then publish style ownership.
fn sunset_block_overlay_render(game: &crate::game::Game<'_>) {
    use horizon_client::gfx::sunset_block_overlay_flags as flags;
    flags::publish(false);
    if !game.native_window() || !game.native_on(crate::game::sunset::ID) {
        return;
    }
    crate::game::sunset::ensure_loaded(game);
    flags::publish(crate::game::sunset_block_overlay::suppress_native_outline(game));
}
