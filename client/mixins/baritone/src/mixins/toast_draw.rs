// The Baritone toast (`logAsToast`), drawn over the HUD the game built this frame.
fn baritone_toast_draw(
    game: &mut crate::game::Game<'_>,
    mut value: HudBatch,
) -> HudBatch {
    if crate::game::sunset_baritone_full::present(game) {
        crate::game::baritone::draw_toast(game, &mut value);
    }
    value
}
