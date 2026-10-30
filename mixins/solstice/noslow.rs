// No Slow: the item-use slowdown left out.
fn solstice_noslow_hook(input: [f32; 2], item_use: Option<f32>, sneaking: Option<f32>) -> std::ops::ControlFlow<[f32; 2]> {
    use std::sync::atomic::Ordering::Relaxed;
    if item_use.is_some() && solstice_flags::NO_SLOW.load(Relaxed) {
        return std::ops::ControlFlow::Break(modify_input(input, None, sneaking));
    }
    std::ops::ControlFlow::Continue(())
}
