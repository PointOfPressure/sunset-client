// Hand View's No sway: the hands do not lag behind the view as it turns.
fn sunset_sway() -> std::ops::ControlFlow<[f32; 2]> {
    if crate::first_person::sunset_hand_view::NO_SWAY.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break([0.0, 0.0]);
    }
    std::ops::ControlFlow::Continue(())
}
