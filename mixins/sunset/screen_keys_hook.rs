// GUI Move: behind a screen the tick's movement takes the move keys held (the call again has them, so it passes
// through here).
// No Slow's Skip: the move goes without the item-use slowdown.
// A knockback hit since the last movement (Player.attack): the speed across cut to 0.6 for each, the sprint stopped.
// Scaffold's Normal: the move goes by the yaw the server is told (movement correction; Silent already turned the keys
// before the tick, so the PlayerInput sent says the same keys), and without a sprint when its Sprint is Off. Both are
// taken here, so the call again passes.
fn sunset_screen_keys(world: &mut MotionWorld<'_, '_, '_>, keys: crate::sprint::Keys, window: i32) -> std::ops::ControlFlow<Result<()>> {
    if world.player.item_use_speed.is_some() && crate::motion::sunset_motion::no_slow_grim() {
        crate::motion::sunset_motion::set_across(None);
        crate::motion::sunset_motion::set_up(None);
    }
    if crate::motion::sunset_motion::no_slow_skip() {
        world.player.item_use_speed = None;
    }
    let hits = crate::motion::sunset_motion::take_attack_slow().min(5);
    if hits > 0 {
        let cut = 0.6f64.powi(hits as i32);
        world.player.velocity.x *= cut;
        world.player.velocity.z *= cut;
        let _ = world.player.set_sprinting(false);
    }
    let screen = crate::motion::sunset_motion::screen_keys(keys);
    let turn = crate::motion::sunset_motion::take_move_yaw();
    let no_sprint = crate::motion::sunset_motion::take_no_sprint();
    let freeze = crate::motion::sunset_motion::take_freeze();
    let scale = crate::motion::sunset_motion::take_input_scale();
    if screen.is_none() && turn.is_none() && !no_sprint && !freeze && scale.is_none() {
        return std::ops::ControlFlow::Continue(());
    }
    let mut keys = screen.unwrap_or(keys);
    if let Some(scale) = scale {
        keys.analog = Some(keys.direction().map(|v| v * scale));
    }
    if freeze {
        keys = Default::default();
        crate::motion::sunset_motion::set_across(None);
        crate::motion::sunset_motion::set_up(None);
        crate::motion::sunset_motion::set_strafe_dir(None);
        crate::motion::sunset_motion::set_flying_speed(None);
    }
    if no_sprint || freeze {
        keys.sprint = false;
        let _ = world.player.set_sprinting(false);
    }
    let view = world.player.yaw;
    if let Some(yaw) = turn {
        world.player.yaw = yaw;
    }
    let done = world.controlled_simulation_tick_with_sprint_window(keys, window);
    if turn.is_some() {
        world.player.yaw = view;
    }
    std::ops::ControlFlow::Break(done)
}
