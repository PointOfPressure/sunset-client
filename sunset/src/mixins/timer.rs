// Timer: the frame's elapsed time, scaled.
fn sunset_timer_hook(this: &mut FixedStep, elapsed: std::time::Duration) -> std::ops::ControlFlow<u32> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use std::sync::atomic::Ordering::Relaxed;
    let m = f32::from_bits(::sunset_core::timer::MULT.load(Relaxed));
    if m == 1.0 || ::sunset_core::timer::BUSY.swap(true, Relaxed) {
        return std::ops::ControlFlow::Continue(());
    }
    let ticks = this.advance(elapsed.mul_f32(m));
    ::sunset_core::timer::BUSY.store(false, Relaxed);
    std::ops::ControlFlow::Break(ticks)
}
