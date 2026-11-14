// Runtime part (see runtime.rs): the HUD each frame and the GUI on top of it.

use super::{Game, Screen};
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use super::sunset::*;

pub(super) fn hud(game: &mut Game<'_>, batch: &mut HudBatch, corners: &mut Corners) {
    if !running() || !game.native_window() || !game.native_on(ID) {
        return;
    }
    ensure_loaded(game);
    super::sunset_crosshair::draw(game, batch);
    let mods = snapshot();
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (w, h) = (batch.width, batch.height);
    // GUI scale, as far as a row of the narrowest panels still fits the window.
    let scale = f32::from_bits(LOOK[6].load(Ordering::Relaxed)).min((w / MIN_ROW).max(1.0));
    LOOK[7].store(scale.to_bits(), Ordering::Relaxed);
    let lw = (1.5 / batch.scale).max(0.5);
    let mut shapes = std::mem::take(&mut *SHAPES.lock().unwrap_or_else(|e| e.into_inner()));

    if let Some(cam) = Camera::of(game, w, h)
        && game.player.is_some()
    {
        // World labels use the open GUI's text scale too: one glyph size a frame, so Sunset's font is not rebuilt
        // twice a frame and the labels' glyphs are not taken from an atlas rebuilt under them.
        let real = batch.scale;
        if game.screen == Screen::Playing && with(|s| s.gui.open && !s.gui.closing) {
            batch.scale = real * scale;
        }
        world_overlays(game, &cam, &mods, lw, &mut Painter { game, batch: &mut *batch });
        super::sunset_api::world(game, &cam, &mut Painter { game, batch: &mut *batch }, lw);
        batch.scale = real;
    }

    super::sunset_loadouts::draw(game, w, h, &mut shapes);
    super::sunset_melee::draw_spear(game, w, h, &mut shapes);
    if let Some(esp) = get("ESP") {
        super::sunset_murder::draw(game, esp, w, &mut shapes);
    }
    super::sunset_hud_bars::clear_placed();
    // Where the HUD module's own shapes start: its Text case is for them, not for the menu, the chat or plugins.
    let hud_from = shapes.len();
    if let Some(hud) = get("HUD") {
        if hud.b("Target HUD") && get("Kill Aura").is_some() {
            // Target HUD sits by the crosshair; drawn there, then slid to its dragged place if moved.
            let from = shapes.len();
            super::sunset_aura::draw_target(game, w, h, hud.p("Target HUD mode") == "Potato", &mut shapes);
            super::sunset_hud_bars::editable(&mut shapes, from, hud, "Target HUD", w, h);
        }
        let cased = |text: &str| super::sunset_hud_list::cased(hud, text);
        if hud.b("Watermark") && (hud.p("Watermark position") == "Custom" || game.native_corner_shown(Corner::TopLeft)) {
            let own = hud.t("Watermark text").trim();
            let line = cased(&if own.is_empty() { format!("{NAME} {VERSION}") } else { own.to_owned() });
            let profile = hud.b("Watermark profile").then(|| cased(&with(|s| format!("Profile: {}", s.selection.label()))));
            let bw = game.native_text_width(&line).max(profile.as_ref().map_or(0.0, |t| game.native_text_width(t))) + 4.0;
            let bh = if profile.is_some() { 24.0 } else { 12.0 };
            let (x, y) = if hud.p("Watermark position") == "Custom" {
                (
                    (hud.n("Watermark x") as f32 * w).clamp(0.0, (w - bw).max(0.0)),
                    (hud.n("Watermark y") as f32 * h).clamp(0.0, (h - bh).max(0.0)),
                )
            } else {
                corners.place(Corner::TopLeft, bw, bh)
            };
            let colour = super::sunset_hud_list::colour_at(hud, hud.p("Watermark color mode"), y);
            shapes.push(Shape::Text(line, [x + 2.0, y + 2.0], colour));
            if let Some(text) = profile {
                shapes.push(Shape::Text(text, [x + 2.0, y + 14.0], 0xE4E4E4));
            }
            super::sunset_hud_bars::place("Watermark", [x, y, bw, bh], w, h);
        }
        let left = hud.p("Module list side") == "Left";
        let corner = if left { Corner::TopLeft } else { Corner::TopRight };
        let moved = hud.p("Module list position") == "Custom";
        if hud.b("Module list") && (moved || game.native_corner_shown(corner)) {
            let mut place = |width: f32, height: f32| {
                let (x, y) = if moved {
                    (
                        (hud.n("Module list x") as f32 * w).clamp(0.0, (w - width).max(0.0)),
                        (hud.n("Module list y") as f32 * h).clamp(0.0, (h - height).max(0.0)),
                    )
                } else {
                    corners.place(corner, width, height)
                };
                super::sunset_hud_bars::place("Module list", [x, y, width, height], w, h);
                (x, y)
            };
            super::sunset_hud_list::draw(game, hud, &mods, !left, &mut place, &mut shapes);
        }
        // The server's scoreboard sidebar (drawn by the game itself): framed here so the chat editor can grab it, and
        // moved by the offset the render crate applies to its draws, to where it was dragged.
        if let Some((nx, ny, nw, nh)) = horizon_client_render::hud_overlays::sunset_scoreboard_pos::natural() {
            let (nxf, nyf, nwf, nhf) = (nx as f32, ny as f32, nw as f32, nh as f32);
            let custom = hud.p("Scoreboard position") == "Custom";
            let (x, y) = if custom {
                (
                    (hud.n("Scoreboard x") as f32 * w).clamp(0.0, (w - nwf).max(0.0)),
                    (hud.n("Scoreboard y") as f32 * h).clamp(0.0, (h - nhf).max(0.0)),
                )
            } else {
                (nxf, nyf)
            };
            super::sunset_hud_bars::place("Scoreboard", [x, y, nwf, nhf], w, h);
            horizon_client_render::hud_overlays::sunset_scoreboard_pos::set_offset(custom, (x - nxf) as i32, (y - nyf) as i32);
        }
        let coordinates = game.native_player().filter(|_| hud.b("Coordinates")).map(|p| {
            let (x, y, z) = (p.x.floor(), p.y.floor(), p.z.floor());
            let other = match p.dimension {
                "minecraft:overworld" => format!(" (Nether {} {})", (p.x / 8.0).floor(), (p.z / 8.0).floor()),
                "minecraft:the_nether" => format!(" (Overworld {} {})", (p.x * 8.0).floor(), (p.z * 8.0).floor()),
                _ => String::new(),
            };
            format!("XYZ {x} {y} {z}{other}")
        });
        let custom = hud.p("Coordinates position") == "Custom";
        // Coordinates moved off the corner: on their own where they were dragged to.
        if custom && let Some(text) = &coordinates {
            let (tw, th) = (game.native_text_width(text) + 4.0, 12.0);
            let x = (hud.n("Coordinates x") as f32 * w).clamp(0.0, (w - tw).max(0.0));
            let y = (hud.n("Coordinates y") as f32 * h).clamp(0.0, (h - th).max(0.0));
            super::sunset_hud_bars::place("Coordinates", [x, y, tw, th], w, h);
            shapes.push(Shape::Text(text.clone(), [x + 2.0, y + 2.0], 0xFFFFFF));
        }
        if game.native_corner_shown(Corner::TopLeft)
            && let Some(p) = game.native_player()
        {
            let mut lines: Vec<String> = Vec::new();
            let in_corner = coordinates.filter(|_| !custom);
            if let Some(text) = &in_corner {
                lines.push(text.clone());
            }
            let mut info: Vec<String> = Vec::new();
            if hud.b("FPS") {
                info.push(format!("{} FPS", game.native_frame_rate().0));
            }
            if hud.b("Speed")
                && let Some(pl) = game.player.as_ref()
            {
                let (dx, dz) = (pl.position.x - game.previous_position.x, pl.position.z - game.previous_position.z);
                info.push(format!("{:.1} b/s", (dx * dx + dz * dz).sqrt() * 20.0));
            }
            if hud.b("Facing") {
                const NAMES: [&str; 8] = ["South +Z", "South-West", "West -X", "North-West", "North -Z", "North-East", "East +X", "South-East"];
                let yaw = f64::from(p.yaw).rem_euclid(360.0);
                info.push(NAMES[((yaw + 22.5) / 45.0).floor() as usize % 8].to_owned());
            }
            if hud.b("Totems") {
                let count = |s: &Option<horizon_client::items::ItemStack>| {
                    s.as_ref().filter(|s| s.item.ends_with("totem_of_undying")).map_or(0, |s| s.count)
                };
                let total: u32 = game.inventory.iter().map(count).sum::<u32>() + count(&game.offhand);
                info.push(format!("{total} totems"));
            }
            if !info.is_empty() {
                lines.push(info.join("  "));
            }
            let lines: Vec<String> = lines.iter().map(|l| cased(l)).collect();
            let width = lines.iter().map(|l| game.native_text_width(l)).fold(0.0, f32::max);
            if !lines.is_empty() {
                let (bx, by) = corners.place(Corner::TopLeft, width + 4.0, lines.len() as f32 * 10.0 + 2.0);
                if let Some(text) = &in_corner {
                    super::sunset_hud_bars::place("Coordinates", [bx, by, game.native_text_width(text) + 4.0, 12.0], w, h);
                }
                for (i, line) in lines.into_iter().enumerate() {
                    shapes.push(Shape::Text(line, [bx + 2.0, by + 2.0 + i as f32 * 10.0], 0xFFFFFF));
                }
            }
        }
        // After the corners' text, so a corner keeps that at its edge.
        if game.native_playing() {
            super::sunset_hud_bars::draw(game, hud, w, h, corners, &mut shapes);
        }
        super::sunset_hud_list::case_shapes(hud, &mut shapes[hud_from..]);
    }
    let shadow = get("HUD").is_none_or(|hud| hud.b("Text shadow"));
    chat_layout(game, &mut shapes);

    // Scaffold's block counter: a number and a word above the hotbar, clear of the hearts, armour and item name.
    if let Some(m) = get("Scaffold").filter(|m| m.b("Block counter"))
        && game.native_playing()
        && game.player.is_some()
        && !game.spectator()
    {
        let blocks = scaffold_blocks(game, m);
        let text = format!("{blocks} {}", if blocks == 1 { "block" } else { "blocks" });
        let colour = if blocks < 16 { 0xFF5555 } else { 0xFFFFFF };
        shapes.push(Shape::Text(text.clone(), [(w - game.native_text_width(&text)) / 2.0, h - 71.0], colour));
    }

    if with(|s| s.gui.open && s.gui.closing) {
        close_gui(game);
    }
    let mut gui_shapes = Vec::new();
    // A player picker follows the players online, with their faces.
    let mut picker = with(|s| s.gui.editor.as_mut().and_then(|e| e.list.take()));
    super::sunset_picker::see_players(game, &mut picker);
    with(|s| {
        if let Some(e) = s.gui.editor.as_mut() {
            e.list = picker;
        }
    });
    if with(|s| s.gui.open) {
        if game.screen == Screen::Playing && game.player.is_some() {
            // The world dims behind the panels by Background dim (percent), the one fill behind them.
            let dim = LOOK[3].load(Ordering::Relaxed).min(100) * 255 / 100;
            if dim > 0 {
                shapes.push(Shape::Fill([0.0, 0.0, w, h], dim << 24));
            }
            draw_gui(game, &mods, w / scale, h / scale, 1.0 / (batch.scale * scale), &mut gui_shapes);
        } else {
            with(|s| {
                s.gui.open = false;
                s.gui.drag = None;
                s.gui.editor = None;
                s.gui.window = None;
            });
        }
    }
    let game = &*game;
    // Sunset's font is drawn for the scale it is told, and rebuilds its glyphs when that changes: while the GUI is
    // open all of the frame's text uses the GUI's, so it is sharp at any GUI scale and they rebuild only once.
    let real = batch.scale;
    if !gui_shapes.is_empty() {
        batch.scale = real * scale;
    }
    super::sunset_api::hud(game, &mut shapes, [w, h], lw);
    paint(game, batch, &mut shapes, 1.0 / batch.scale, shadow);
    *SHAPES.lock().unwrap_or_else(|e| e.into_inner()) = shapes;
    // The GUI goes on top of the rest of the frame too: gui_top paints it last.
    *GUI_TOP.lock().unwrap_or_else(|e| e.into_inner()) = (gui_shapes, scale);
    batch.scale = real;
    if !with(|s| s.gui.open) {
        super::sunset_radar::draw(game, batch, corners, &mods);
    }
}

// The open GUI's shapes this frame and its scale, from hud for gui_top.
pub(super) static GUI_TOP: Mutex<(Vec<Shape>, f32)> = Mutex::new((Vec::new(), 1.0));

// After all of the frame's HUD (chat, titles, the tab list, toasts) and before its prompts (a server's pack, a dialog, a
// link), which stay on top and take the clicks, as ever: the GUI, scaled about the corner its coordinates start from.
pub(super) fn gui_top(game: &Game<'_>, batch: &mut HudBatch) {
    let (mut shapes, scale) = std::mem::replace(&mut *GUI_TOP.lock().unwrap_or_else(|e| e.into_inner()), (Vec::new(), 1.0));
    if shapes.is_empty() {
        return;
    }
    let real = batch.scale;
    batch.scale = real * scale;
    let start = batch.vertices.len();
    paint(game, batch, &mut shapes, 1.0 / batch.scale, true);
    // Opening, it rises into place as it fades in.
    if let Some((opacity, below)) = super::sunset_dashboard::open_motion() {
        for v in &mut batch.vertices[start..] {
            v.pos[1] += below;
            v.color = (v.color & 0x00FF_FFFF) | (((v.color >> 24) as f32 * opacity) as u32) << 24;
        }
    }
    batch.scale_since(start, 0.0, 0.0, scale, scale);
    batch.scale = real;
}
