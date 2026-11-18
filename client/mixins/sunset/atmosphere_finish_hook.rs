// Game::finish_perf is macro-free and runs before application shutdown drops Renderer.
fn sunset_atmosphere_finish(game: &mut crate::game::Game<'_>) {
    if let Some(renderer) = game.renderer.as_mut() {
        horizon_client::gfx::sunset_atmosphere_renderer::release(renderer);
    }
}
