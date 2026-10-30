// Once a frame, after the mouse: Aim Assist.
fn sunset_frame(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_render::frame(game);
}
