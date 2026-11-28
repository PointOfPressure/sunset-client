// After a move made by the yaw the server is told (Scaffold's movement correction): the view as it was.
fn sunset_screen_keys_after(game: &mut Game<'_>) {
    if let Some(yaw) = ::sunset_core::motion::take_restore_yaw()
        && let Some(p) = game.player.as_mut()
    {
        p.yaw = yaw;
    }
}
