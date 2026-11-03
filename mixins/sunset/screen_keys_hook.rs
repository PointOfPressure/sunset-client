// GUI Move: behind a screen the tick's movement takes the move keys held (the call again has them, so it passes
// through here).
// No Slow's Skip (vape-rs's NoSlowdown): the move goes without the item-use slowdown.
fn sunset_screen_keys(world: &mut MotionWorld<'_, '_, '_>, keys: crate::sprint::Keys, window: i32) -> std::ops::ControlFlow<Result<()>> {
    if crate::motion::sunset_motion::no_slow_skip() {
        world.player.item_use_speed = None;
    }
    match crate::motion::sunset_motion::screen_keys(keys) {
        Some(keys) => std::ops::ControlFlow::Break(world.controlled_simulation_tick_with_sprint_window(keys, window)),
        None => std::ops::ControlFlow::Continue(()),
    }
}
