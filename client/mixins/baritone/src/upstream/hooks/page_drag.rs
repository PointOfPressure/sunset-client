// The mouse moved with the button held on Baritone's page in Options > Mods: a slider follows it.
fn baritone_page_drag(game: &mut crate::game::Game<'_>) -> core::ops::ControlFlow<Result<()>> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if !crate::game::baritone::page_open(game) {
        return core::ops::ControlFlow::Continue(());
    }
    crate::game::baritone::page_press(game, true, false);
    core::ops::ControlFlow::Break(Ok(()))
}
