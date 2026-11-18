// Atmosphere's sky, prepared on the game's side (crate::game::sunset_atmosphere_renderer) before the frame renders.
fn sunset_atmosphere_render(renderer: &mut Renderer, frame: &Frame<'_>) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    if let Some(prepare) = ::sunset_core::bridge::RENDER.get() {
        prepare(renderer as *mut Renderer as *mut u8, frame as *const Frame<'_> as *mut u8);
    }
}
