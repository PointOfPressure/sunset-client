// No Slow: the item-use slowdown left out.
fn sunset_noslow_hook(input: [f32; 2], item_use: Option<f32>, sneaking: Option<f32>) -> std::ops::ControlFlow<[f32; 2]> {
    use std::sync::atomic::Ordering::Relaxed;
    if item_use.is_some() && sunset_flags::NO_SLOW.load(Relaxed) {
        // The slowdown by No Slow's factor instead of the game's: the input without it, scaled (modify_input with
        // Some(factor) would come back into this hook for ever).
        let factor = f32::from_bits(sunset_flags::NO_SLOW_FACTOR.load(Relaxed)).clamp(0.2, 1.0);
        let [x, z] = modify_input(input, None, sneaking);
        return std::ops::ControlFlow::Break([x * factor, z * factor]);
    }
    std::ops::ControlFlow::Continue(())
}
