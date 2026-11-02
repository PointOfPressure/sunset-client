// Client Sort draws its three buttons instead of Horizon's sort button.
fn sunset_sort_draw(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, ox: f32, oy: f32) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_extras::on() {
        crate::game::tweaks::sunset_sort_buttons::draw(game, batch, ox, oy);
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
