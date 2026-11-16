// A mouse button pressed or let go on Baritone's page in Options > Mods.
fn baritone_page_mouse(game: &mut crate::game::Game<'_>, pressed: bool, right: bool) -> core::ops::ControlFlow<Result<()>> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if !crate::game::baritone::page_open(game) {
        return core::ops::ControlFlow::Continue(());
    }
    if !pressed {
        game.dragging = false;
        return core::ops::ControlFlow::Break(Ok(()));
    }
    game.dragging = !right;
    match crate::game::baritone::page_press(game, false, right) {
        0 => {}
        1 => game.audio.play_button_click(),
        _ => {
            game.audio.play_button_click();
            let events = game.ui.options.back(&mut game.settings);
            return core::ops::ControlFlow::Break(game.handle_option_events(events));
        }
    }
    core::ops::ControlFlow::Break(Ok(()))
}
