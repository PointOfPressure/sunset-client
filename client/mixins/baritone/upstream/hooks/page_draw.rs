// Baritone's page in Options > Mods draws itself.
fn baritone_page_draw(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) -> core::ops::ControlFlow<()> {
    if !crate::game::sunset_baritone_full::present(game) { return core::ops::ControlFlow::Continue(()); }

    if crate::game::baritone::page_open(game) {
        crate::game::baritone::page_draw(game, batch);
        return core::ops::ControlFlow::Break(());
    }
    core::ops::ControlFlow::Continue(())
}
