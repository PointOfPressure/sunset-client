// Once a frame, after your own mouse turn (its TAIL): Aim Assist. The turn's result comes first so the
// game is borrowed only once it is done.
fn sunset_mouse_after(value: (), game: &mut crate::game::Game<'_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_combat::aim_after(game);
    value
}
