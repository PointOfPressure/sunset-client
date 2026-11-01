// Crosshair: Sunset's own crosshair, drawn as shapes. Each hud_layers call hands the frame to the hud crate's hook,
// which draws it only where the game draws its own crosshair.

use super::Game;
use super::sunset::{Category, ID, Module};
use super::sunset_crosshair_math::{Style, geometry};
use horizon_client::hud::sunset_crosshair::{self as bridge, Frame};
use horizon_client::hud::HudBatch;

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(
        "Crosshair",
        Category::Render,
        "Replaces the crosshair with a crisp cross or dot. Sizes are physical pixels, independent of GUI scale.",
    )
    .pick("Mode", &["Cross", "Dot"])
    .num("Length", 6.0, 1.0, 32.0, 1.0)
    .num("Gap", 3.0, 0.0, 32.0, 1.0)
    .num("Thickness", 1.0, 1.0, 8.0, 1.0)
    .bool("Center dot", false)
    .num("Dot size", 2.0, 1.0, 8.0, 1.0)
    .num("Outline", 1.0, 0.0, 4.0, 1.0)
    .colour("Color", "White")
    .colour("Outline color", "Black")]
}

// Called at HEAD Game::hud_layers. Reset before every gate, including an early
// screen/loading exit, so a previous enabled frame cannot suppress vanilla.
pub(super) fn prepare(game: &Game<'_>, size: [u32; 2]) {
    bridge::set_frame(Frame::empty());
    if !game.native_window() || !game.native_on(ID) || !game.native_playing()
        || game.player.is_none() || game.ui.hide_gui || !game.camera_type.first_person()
        || game.debug_crosshair() || (game.spectator() && !game.spectator_crosshair())
    {
        return;
    }
    super::sunset::ensure_loaded(game);
    // Sunset leaves Screen::Playing active while its pointer-driven ClickGUI
    // is open. This extra gate covers that overlay, including its close frame.
    if super::sunset::gui_open() {
        return;
    }
    // Borrow settings under the existing state lock, copying only scalar
    // values. Unlike snapshot()/Module::clone(), this allocates nothing.
    let settings = super::sunset::with(|s| {
        let m = s.module("Crosshair").filter(|m| m.on)?;
        Some((Style {
            dot_only: m.p("Mode") == "Dot",
            center_dot: m.b("Center dot"),
            length: m.n("Length"),
            gap: m.n("Gap"),
            thickness: m.n("Thickness"),
            dot_size: m.n("Dot size"),
            outline: m.n("Outline"),
        }, m.c("Color"), m.c("Outline color")))
    });
    let Some((style, color, outline_color)) = settings else {
        return;
    };
    let Some(shape) = geometry(size, 1.0, style) else {
        return;
    };
    let mut frame = Frame::empty();
    let Some(mark) = game.gui.atlas.sprites.get("gui/sprites/hud/crosshair") else {
        return;
    };
    frame.set_crosshair([mark.u0, mark.v0, mark.u1, mark.v1]);
    // All outlines precede all foreground bars, so neighboring outlines
    // never cover the cross itself at small gaps or large thicknesses.
    if shape.outline > 0.0 {
        for rect in shape.rects() {
            frame.push(rect.expanded(shape.outline).gui(1.0), outline_color);
        }
    }
    for rect in shape.rects() {
        frame.push(rect.gui(1.0), color);
    }
    bridge::set_frame(frame);
}

// Runs after vanilla's mark and indicators, within the host show_hud branch.
// The bridge draws only if that mark was actually suppressed this frame.
pub(super) fn draw(game: &Game<'_>, batch: &mut HudBatch) {
    bridge::draw(batch, &game.gui.atlas);
}
