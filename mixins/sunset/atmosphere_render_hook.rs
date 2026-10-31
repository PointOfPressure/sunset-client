// Renderer::render is a macro-free supported Mixins HEAD boundary.
fn sunset_atmosphere_render(renderer: &mut Renderer, frame: &Frame<'_>) {
    sunset_atmosphere_renderer::prepare(renderer, frame);
}
