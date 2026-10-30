// SPDX-License-Identifier: MIT
// Capture a pending native-page mouse bind. AI-assisted contribution.
fn solstice_bind_options_mouse(
    game: &mut crate::game::Game<'_>,
    pressed: bool,
) -> std::ops::ControlFlow<Result<()>> {
    if pressed && crate::game::solstice::capture_mouse_bind(game) {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
