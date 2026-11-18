// No Render's Hide hurt cam: no tilt of the view when you are hurt (the damage tilt's strength is 0).
fn sunset_hurt_cam(value: f64) -> f64 {
    if ::sunset_core::lifecycle::RUNTIME.running()
        && ::sunset_core::overlay::NO_HURT.load(std::sync::atomic::Ordering::Relaxed)
    {
        return 0.0;
    }
    value
}
