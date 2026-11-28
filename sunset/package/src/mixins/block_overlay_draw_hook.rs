// Sunset's block overlay owns the frame: the game's own outline of the block looked at is left out (as a mod's
// block_highlight answer does).
fn sunset_block_overlay_draw(canceled: &mut bool) {
    if ::sunset_core::lifecycle::RUNTIME.running() && ::sunset_core::block_overlay::active() {
        *canceled = true;
    }
}
