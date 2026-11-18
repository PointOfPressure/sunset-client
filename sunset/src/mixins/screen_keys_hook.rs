// The keys the tick's move takes (after the PlayerInput the server hears was made from the real ones):
// GUI Move behind a screen takes the move keys held; a scale, a freeze or a stopped sprint from Sunset's movement.
// What it decided for the player itself is applied once the game synced the player (screen_move_hook.rs).
fn sunset_screen_keys(value: Keys) -> Keys {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return value;
    }
    let keys = value;
    let block_scale = ::sunset_core::motion::take_block_use_scale();
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
    let screen = ::sunset_core::motion::screen_keys(core).map(|k| Keys {
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
    ::sunset_core::motion::set_pending_move(Some(::sunset_core::motion::PendingMove { block_scale, turn, no_sprint, freeze }));
    if screen.is_none() && !no_sprint && !freeze && scale.is_none() {
        return keys;
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
    }
    keys
}
