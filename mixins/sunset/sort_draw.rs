// Client Sort draws its three buttons instead of Horizon's sort button, and notes the slot under the pointer (Shift fill).
fn sunset_sort_draw(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, ox: f32, oy: f32) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_extras::note_hover(game.slot_at(batch, game.cursor.0, game.cursor.1));
    if crate::game::sunset_extras::on() {
        crate::game::tweaks::sunset_sort_buttons::draw(game, batch, ox, oy);
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
