// No Render: the screen overlays the player chose to hide are left out.
fn sunset_overlay_hook(this: &ScreenEffects, input: &ScreenInputs<'_>) -> std::ops::ControlFlow<Vec<ScreenQuad>> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use std::sync::atomic::Ordering::Relaxed;
    let (fire, liquid, wall, gui) = (
        ::sunset_core::overlay::NO_FIRE.load(Relaxed),
        ::sunset_core::overlay::NO_LIQUID.load(Relaxed),
        ::sunset_core::overlay::NO_WALL.load(Relaxed),
        ::sunset_core::overlay::NO_GUI.load(Relaxed),
    );
    if !(fire || liquid || wall || gui) || ::sunset_core::overlay::busy() {
        return std::ops::ControlFlow::Continue(());
    }
    ::sunset_core::overlay::set_busy(true);
    let none: &[GuiDraw] = &[];
    let changed = ScreenInputs {
        sprites: input.sprites,
        flags: input.flags,
        aspect: input.aspect,
        partial: input.partial,
        block_overlay: if wall { None } else { input.block_overlay.clone() },
        water: if liquid { None } else { input.water },
        on_fire: input.on_fire && !fire,
        fire_lower: input.fire_lower,
        gui_overlays: if gui { none } else { input.gui_overlays },
        custom: input.custom,
    };
    let quads = this.build(&changed);
    ::sunset_core::overlay::set_busy(false);
    std::ops::ControlFlow::Break(quads)
}
