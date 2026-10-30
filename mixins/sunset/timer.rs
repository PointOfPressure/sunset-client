// Timer: the frame's elapsed time, scaled.
fn sunset_timer_hook(this: &mut FixedStep, elapsed: std::time::Duration) -> std::ops::ControlFlow<u32> {
    use std::sync::atomic::Ordering::Relaxed;
    let m = f32::from_bits(sunset_timer::MULT.load(Relaxed));
    if m == 1.0 || sunset_timer::BUSY.swap(true, Relaxed) {
        return std::ops::ControlFlow::Continue(());
    }
    let ticks = this.advance(elapsed.mul_f32(m));
    sunset_timer::BUSY.store(false, Relaxed);
    std::ops::ControlFlow::Break(ticks)
}
