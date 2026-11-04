// No Render's Hide hurt cam: no tilt of the view when you are hurt.
fn sunset_hurt_cam() -> std::ops::ControlFlow<()> {
    if crate::screen_effects::sunset_overlay::NO_HURT.load(std::sync::atomic::Ordering::Relaxed) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
