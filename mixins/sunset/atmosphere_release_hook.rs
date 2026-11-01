// Restore and release cached device objects before Horizon replaces its renderer.
fn sunset_atmosphere_release(renderer: &mut Renderer) {
    sunset_atmosphere_renderer::release(renderer);
}
