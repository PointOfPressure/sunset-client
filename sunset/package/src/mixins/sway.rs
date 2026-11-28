// Hand View's No sway: the hands do not lag behind the view as it turns.
fn sunset_sway() -> std::ops::ControlFlow<[f32; 2]> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if ::sunset_core::hand_view::NO_SWAY.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break([0.0, 0.0]);
    }
    std::ops::ControlFlow::Continue(())
}
