// GUI Move: behind a screen the tick's movement takes the move keys held (the call again has them, so it passes
// through here).
fn sunset_screen_keys(world: &mut MotionWorld<'_, '_, '_>, keys: crate::sprint::Keys, window: i32) -> std::ops::ControlFlow<Result<()>> {
    match crate::motion::sunset_motion::screen_keys(keys) {
        Some(keys) => std::ops::ControlFlow::Break(world.controlled_simulation_tick_with_sprint_window(keys, window)),
        None => std::ops::ControlFlow::Continue(()),
    }
}
