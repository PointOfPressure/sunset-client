// The player as the move takes it, set up after the game synced it from the window (its look and the item in use):
// No Slow's Grim and Skip modes, the slowdown of a knockback hit (Player.attack: the speed across cut to 0.6 for
// each, the sprint stopped), a stopped sprint, and Scaffold's movement correction (the move goes by the yaw the
// server is told; the view comes back after the move, screen_keys_after.rs).
fn sunset_screen_move(game: &mut Game<'_>) {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return;
    }
    let Some(pending) = ::sunset_core::motion::take_pending_move() else {
        return;
    };
    let hits = ::sunset_core::motion::take_attack_slow().min(5);
    if game.screen == Screen::Dead || game.sleeping.is_some() || game.riding.is_some() {
        return;
    }
    let Some(player) = game.player.as_mut() else {
        return;
    };
    if (player.item_use_speed.is_some() || pending.block_scale.is_some()) && ::sunset_core::motion::no_slow_grim() {
        ::sunset_core::motion::set_across(None);
        ::sunset_core::motion::set_up(None);
    }
    if ::sunset_core::motion::no_slow_skip() {
        player.item_use_speed = None;
    }
    if hits > 0 {
        let cut = 0.6f64.powi(hits as i32);
        player.velocity.x *= cut;
        player.velocity.z *= cut;
        let _ = player.set_sprinting(false);
    }
    if pending.no_sprint || pending.freeze {
        let _ = player.set_sprinting(false);
    }
    if let Some(yaw) = pending.turn {
        ::sunset_core::motion::set_restore_yaw(Some(player.yaw));
        player.yaw = yaw;
    }
}
