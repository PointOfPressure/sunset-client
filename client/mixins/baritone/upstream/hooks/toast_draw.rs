fn baritone_toast_draw(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::draw_toast(game, batch);
}
