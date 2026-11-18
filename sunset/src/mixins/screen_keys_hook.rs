// GUI Move: behind a screen the tick's movement takes the move keys held (the call again has them, so it passes
// through here).
// No Slow's Skip: the move goes without the item-use slowdown.
// A knockback hit since the last movement (Player.attack): the speed across cut to 0.6 for each, the sprint stopped.
// Scaffold's Normal: the move goes by the yaw the server is told (movement correction; Silent already turned the keys
// before the tick, so the PlayerInput sent says the same keys), and without a sprint when its Sprint is Off. Both are
// taken here, so the call again passes.
fn sunset_screen_keys(
    mut world: MotionWorld<'_, '_, '_>,
    keys: horizon_client::sprint::Keys,
    window: i32,
    operation: impl FnOnce(MotionWorld<'_, '_, '_>, horizon_client::sprint::Keys, i32) -> Result<()>,
) -> Result<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return operation(world, keys, window); }

    let block_scale = ::sunset_core::motion::take_block_use_scale();
    if (world.player.item_use_speed.is_some() || block_scale.is_some()) && ::sunset_core::motion::no_slow_grim() {
        ::sunset_core::motion::set_across(None);
        ::sunset_core::motion::set_up(None);
    }
    if ::sunset_core::motion::no_slow_skip() {
        world.player.item_use_speed = None;
    }
    let hits = ::sunset_core::motion::take_attack_slow().min(5);
    if hits > 0 {
        let cut = 0.6f64.powi(hits as i32);
        world.player.velocity.x *= cut;
        world.player.velocity.z *= cut;
        let _ = world.player.set_sprinting(false);
    }
    let core = ::sunset_core::motion::Keys {
        forward: keys.forward,
        backward: keys.backward,
        left: keys.left,
        right: keys.right,
        jump: keys.jump,
        shift: keys.shift,
        sprint: keys.sprint,
        analog: keys.analog,
    };
    let screen = ::sunset_core::motion::screen_keys(core).map(|k| horizon_client::sprint::Keys {
        forward: k.forward,
        backward: k.backward,
        left: k.left,
        right: k.right,
        jump: k.jump,
        shift: k.shift,
        sprint: k.sprint,
        analog: k.analog,
        ..keys
    });
    let turn = ::sunset_core::motion::take_move_yaw();
    let no_sprint = ::sunset_core::motion::take_no_sprint() || block_scale.is_some_and(|s| s < 1.0);
    let freeze = ::sunset_core::motion::take_freeze();
    let scale = match (::sunset_core::motion::take_input_scale(), block_scale) {
        (Some(a), Some(b)) => Some(a * b),
        (a, b) => a.or(b),
    };
    if screen.is_none() && turn.is_none() && !no_sprint && !freeze && scale.is_none() {
        return operation(world, keys, window);
    }
    let mut keys = screen.unwrap_or(keys);
    if let Some(scale) = scale {
        keys.analog = Some(keys.direction().map(|v| v * scale));
    }
    if freeze {
        keys = Default::default();
        ::sunset_core::motion::set_across(None);
        ::sunset_core::motion::set_up(None);
        ::sunset_core::motion::set_strafe_dir(None);
        ::sunset_core::motion::set_flying_speed(None);
    }
    if no_sprint || freeze {
        keys.sprint = false;
        let _ = world.player.set_sprinting(false);
    }
    let view = world.player.yaw;
    if let Some(yaw) = turn {
        world.player.yaw = yaw;
    }
    // The view comes back after the move (screen_keys_after): the world goes into the move.
    if turn.is_some() {
        ::sunset_core::motion::set_restore_yaw(Some(view));
    }
    operation(world, keys, window)
}
