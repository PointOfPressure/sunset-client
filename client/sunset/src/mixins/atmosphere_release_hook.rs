// Restore and release cached device objects before Horizon replaces its renderer.
fn sunset_atmosphere_release(renderer: &mut Renderer) {
    if let Some(release) = ::sunset_core::bridge::RELEASE_SURFACE.get() {
        release(renderer as *mut Renderer as *mut u8);
    }
}
