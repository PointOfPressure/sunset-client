// Item Durability: the number on the item's icon, after the icon.
fn sunset_item_durability(
    game: &crate::game::Game<'_>,
    batch: &mut HudBatch,
    stack: &ItemStack,
    x: f32,
    y: f32,
) -> std::ops::ControlFlow<()> {
    crate::game::sunset_hud_bars::item_model(game, batch, stack, x, y)
}
