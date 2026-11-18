// Item Durability: remaining/max under the item's name.
fn sunset_durability_line(
    game: &mut crate::game::Game<'_>,
    batch: &mut HudBatch,
    stack: &ItemStack,
    creative: bool,
    x: f32,
    y: f32,
    after_name: &Vec<(horizon_client::text::Text, u32)>,
) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_hud_bars::tooltip(game, batch, stack, creative, [x, y], after_name)
}
