// Once a frame, before your own mouse turn (MouseHandler.handleAccumulatedMovement's HEAD): Aim Assist.
fn sunset_mouse_before(game: &crate::game::Game<'_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_combat::aim_before(game);
}
