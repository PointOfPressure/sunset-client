// Characters typed on Baritone's page in Options > Mods go to its text fields, and nowhere else.
fn baritone_page_text(game: &mut crate::game::Game<'_>, text: &str) -> core::ops::ControlFlow<Result<()>> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if crate::game::baritone::page_open(game) {
        crate::game::baritone::page_text(game, text);
        return core::ops::ControlFlow::Break(Ok(()));
    }
    core::ops::ControlFlow::Continue(())
}
