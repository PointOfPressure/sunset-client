// Shulker Peek: a shulker box's contents drawn with its tooltip.
fn sunset_peek(game: &crate::game::Game<'_>, batch: &mut HudBatch, stack: &ItemStack, x: f32, y: f32) {
    crate::game::sunset_render::peek(game, batch, stack, x, y);
}
