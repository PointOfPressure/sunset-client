// A key on Baritone's page in Options > Mods: a text field takes it, and Escape in a field only leaves the field.
fn baritone_page_key(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode, text: Option<&str>) -> core::ops::ControlFlow<Result<()>> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if crate::game::baritone::page_open(game) && crate::game::baritone::page_key(game, key, text) {
        return core::ops::ControlFlow::Break(Ok(()));
    }
    core::ops::ControlFlow::Continue(())
}
