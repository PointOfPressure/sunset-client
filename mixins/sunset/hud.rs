// The HUD: ESP, tracers, name tags, then the watermark, module list and
// coordinates in their corners.
fn sunset_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset::hud(game, batch, corners);
}
