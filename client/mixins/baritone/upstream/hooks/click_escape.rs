// Escape closes the click screen of `#click` and goes back to playing, instead of opening the pause menu.
fn baritone_click_escape(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode) -> core::ops::ControlFlow<bool> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if key == winit::keyboard::KeyCode::Escape && crate::game::baritone::click_escape(game) {
        core::ops::ControlFlow::Break(true)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
