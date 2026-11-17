// Runtime part (see runtime.rs): the HUD each frame and the GUI on top of it.

use super::{Game, Screen};
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use super::sunset::*;

// Where a HUD element `size` big goes: in its corner, below (or above) what that corner holds already, or at its
// Custom place ("<name> x" and "<name> y", hud_place.rs), kept whole on the screen. None while the debug screen (F3)
// covers that corner (for a Custom element, the corner it is nearest). Recorded for the chat's layout editor, which
// moves it by dragging.
fn place_element(
    game: &Game<'_>,
    corners: &mut Corners,
    hud: &Module,
    name: &'static str,
    place: super::sunset_hud_place::Place,
    size: [f32; 2],
    right: bool,
    screen: [f32; 2],
) -> Option<(f32, f32)> {
    use super::sunset_hud_place::{Place, custom, nearest};
    let (x, y) = match place {
        Place::Corner(c) => {
            if !game.native_corner_shown(c) {
                return None;
            }
            corners.place(c, size[0], size[1])
        }
        Place::Custom => {
            let at = [hud.n(&format!("{name} x")), hud.n(&format!("{name} y"))];
            let [x, y] = custom(at, size, screen, right);
            if !game.native_corner_shown(nearest([x, y, size[0], size[1]], screen)) {
                return None;
            }
            (x, y)
        }
    };
    super::sunset_hud_bars::place(name, [x, y, size[0], size[1]], screen[0], screen[1]);
    Some((x, y))
}

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

    // What the layout editor can take is placed again each frame: Horizon's own widgets (drawn before this) first, and
    // one of them being dragged follows the pointer.
    super::sunset_hud_bars::clear_placed();
    drag_widgets(game);
    super::sunset_hud_bars::place_widgets(game, w, h);
    super::sunset_loadouts::draw(game, w, h, &mut shapes);
    // The spear's damage indicator and the Murderer list, where they are drawn or where they were dragged.
    let from = shapes.len();
    super::sunset_melee::draw_spear(game, w, h, &mut shapes);
    if let Some(spear) = get("Auto Spear") {
        super::sunset_hud_bars::editable(game, &mut shapes, from, spear, "Damage indicator", w, h);
    }
    if let Some(esp) = get("ESP") {
        let from = shapes.len();
        super::sunset_murder::draw(game, esp, w, &mut shapes);
        super::sunset_hud_bars::editable(game, &mut shapes, from, esp, "Murderer list", w, h);
    }
    // Where the HUD module's own shapes start: its Text case is for them, not for the menu, the chat or plugins.
    let hud_from = shapes.len();
    if let Some(hud) = get("HUD") {
        // First, so the HUD's text and boxes stay over the tint.
        if game.native_playing() {
            super::sunset_hud_health::draw(game, hud, w, h, &mut shapes);
        }
        if hud.b("Target HUD") && get("Kill Aura").is_some() {
            // Target HUD sits by the crosshair; drawn there, then slid to its dragged place if moved.
            let from = shapes.len();
            super::sunset_aura::draw_target(game, w, h, hud.p("Target HUD mode") == "Potato", &mut shapes);
            super::sunset_hud_bars::editable(game, &mut shapes, from, hud, "Target HUD", w, h);
        }
        let cased = |text: &str| super::sunset_hud_list::cased(hud, text);
        if hud.b("Watermark") {
            let own = hud.t("Watermark text").trim();
            let line = cased(&if own.is_empty() { format!("{NAME} {VERSION}") } else { own.to_owned() });
            let profile = hud.b("Watermark profile").then(|| cased(&with(|s| format!("Profile: {}", s.selection.label()))));
            // One block: the name, and the profile under it (as the Default layout has them).
            let tw = game.native_text_width(&line) + 4.0;
            let pw = profile.as_ref().map_or(0.0, |p| game.native_text_width(p) + 4.0);
            let size = [tw.max(pw), if profile.is_some() { 24.0 } else { 12.0 }];
            let place = super::sunset_hud_place::parse(hud.p("Watermark position"), Corner::TopLeft);
            // In a right corner the lines keep to the screen's edge.
            let flush = matches!(place, super::sunset_hud_place::Place::Corner(c) if !c.left());
            if let Some((x, y)) = place_element(game, corners, hud, "Watermark", place, size, false, [w, h]) {
                let at = |width: f32| if flush { x + size[0] - width + 2.0 } else { x + 2.0 };
                let colour = super::sunset_hud_list::colour_at(hud, hud.p("Watermark color mode"), y);
                shapes.push(Shape::Text(line, [at(tw), y + 2.0], colour));
                if let Some(p) = profile {
                    shapes.push(Shape::Text(p, [at(pw), y + 14.0], 0xE4E4E4));
                }
            }
        }
        let left = hud.p("Module list side") == "Left";
        if hud.b("Module list") {
            use super::sunset_hud_place::Place;
            let place = super::sunset_hud_place::list_place(hud.p("Module list position"), !left);
            let at = [hud.n("Module list x"), hud.n("Module list y")];
            // F3 hides it with its corner; a Custom list by the corner its anchor (its right edge on the right) is in.
            let shown = match place {
                Place::Corner(c) => game.native_corner_shown(c),
                Place::Custom => {
                    let [x, y] = [at[0] as f32 * w, at[1] as f32 * h];
                    game.native_corner_shown(super::sunset_hud_place::nearest([x, y, 0.0, 0.0], [w, h]))
                }
            };
            if shown {
                let mut put = |width: f32, height: f32| {
                    let (x, y) = match place {
                        Place::Corner(c) => corners.place(c, width, height),
                        Place::Custom => {
                            let [x, y] = super::sunset_hud_place::custom(at, [width, height], [w, h], !left);
                            (x, y)
                        }
                    };
                    // Never over the status effect icons at the top right: just under them, in a corner or dragged (the
                    // game's corner leaves them room only while playing, not with the chat open).
                    let (x, y) = super::sunset_hud_bars::below_effects(game, [x, y, width, height], w, h);
                    super::sunset_hud_bars::place("Module list", [x, y, width, height], w, h);
                    (x, y)
                };
                super::sunset_hud_list::draw(game, hud, &mods, !left, &mut put, &mut shapes);
            }
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
        let coordinates_at = super::sunset_hud_place::parse(hud.p("Coordinates position"), Corner::TopLeft);
        // In the top left corner the coordinates head the info lines (FPS, Speed, Facing, Totems); anywhere else
        // (another corner, or where they were dragged to) they go on their own.
        let custom = coordinates_at != super::sunset_hud_place::Place::Corner(Corner::TopLeft);
        if custom && let Some(text) = &coordinates {
            let size = [game.native_text_width(text) + 4.0, 12.0];
            if let Some((x, y)) = place_element(game, corners, hud, "Coordinates", coordinates_at, size, false, [w, h]) {
                shapes.push(Shape::Text(text.clone(), [x + 2.0, y + 2.0], 0xFFFFFF));
            }
        }
        // The Info line's parts (FPS, Speed, Facing, Totems), by name: each in the corner's line (Default) or on its own
        // where it was dragged to (Custom), and each can be taken on its own.
        let mut info: Vec<(&'static str, String)> = Vec::new();
        if let Some(p) = game.native_player() {
            if hud.b("FPS") {
                info.push(("FPS", format!("{} FPS", game.native_frame_rate().0)));
            }
            if hud.b("Speed")
                && let Some(pl) = game.player.as_ref()
            {
                let (dx, dz) = (pl.position.x - game.previous_position.x, pl.position.z - game.previous_position.z);
                info.push(("Speed", format!("{:.1} b/s", (dx * dx + dz * dz).sqrt() * 20.0)));
            }
            if hud.b("Facing") {
                const NAMES: [&str; 8] = ["South +Z", "South-West", "West -X", "North-West", "North -Z", "North-East", "East +X", "South-East"];
                let yaw = f64::from(p.yaw).rem_euclid(360.0);
                info.push(("Facing", NAMES[((yaw + 22.5) / 45.0).floor() as usize % 8].to_owned()));
            }
            if hud.b("Totems") {
                let count = |s: &Option<horizon_client::items::ItemStack>| {
                    s.as_ref().filter(|s| s.item.ends_with("totem_of_undying")).map_or(0, |s| s.count)
                };
                let total: u32 = game.inventory.iter().map(count).sum::<u32>() + count(&game.offhand);
                info.push(("Totems", format!("{total} totems")));
            }
        }
        let (moved, in_line): (Vec<_>, Vec<_>) = info
            .into_iter()
            .map(|(name, text)| (name, cased(&text)))
            .partition(|(name, _)| hud.p(&format!("{name} position")) == "Custom");
        for (name, text) in moved {
            let (tw, th) = (game.native_text_width(&text) + 4.0, 12.0);
            let x = (hud.n(&format!("{name} x")) as f32 * w).clamp(0.0, (w - tw).max(0.0));
            let y = (hud.n(&format!("{name} y")) as f32 * h).clamp(0.0, (h - th).max(0.0));
            super::sunset_hud_bars::place(name, [x, y, tw, th], w, h);
            shapes.push(Shape::Text(text, [x + 2.0, y + 2.0], 0xFFFFFF));
        }
        if game.native_corner_shown(Corner::TopLeft) {
            let mut lines: Vec<String> = Vec::new();
            let in_corner = coordinates.filter(|_| !custom).map(|c| cased(&c));
            if let Some(text) = &in_corner {
                lines.push(text.clone());
            }
            const GAP: &str = "  ";
            if !in_line.is_empty() {
                lines.push(in_line.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(GAP));
            }
            let width = lines.iter().map(|l| game.native_text_width(l)).fold(0.0, f32::max);
            if !lines.is_empty() {
                let (bx, by) = corners.place(Corner::TopLeft, width + 4.0, lines.len() as f32 * 10.0 + 2.0);
                if let Some(text) = &in_corner {
                    super::sunset_hud_bars::place("Coordinates", [bx, by, game.native_text_width(text) + 4.0, 12.0], w, h);
                }
                // The Info line is the last: each part of it framed where it sits in the line.
                let row = by + 1.0 + (lines.len() - 1) as f32 * 10.0;
                let mut x = bx + 1.0;
                for (name, text) in &in_line {
                    let tw = game.native_text_width(text) + 2.0;
                    super::sunset_hud_bars::place(name, [x, row, tw, 11.0], w, h);
                    x += tw - 2.0 + game.native_text_width(GAP);
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

    // Scaffold's block counter: a number and a word above the hotbar, clear of the hearts, armour and item name (or where
    // it was dragged).
    if let Some(m) = get("Scaffold").filter(|m| m.b("Block counter"))
        && game.native_playing()
        && game.player.is_some()
        && !game.spectator()
    {
        let blocks = scaffold_blocks(game, m);
        let text = format!("{blocks} {}", if blocks == 1 { "block" } else { "blocks" });
        let colour = if blocks < 16 { 0xFF5555 } else { 0xFFFFFF };
        let from = shapes.len();
        shapes.push(Shape::Text(text.clone(), [(w - game.native_text_width(&text)) / 2.0, h - 71.0], colour));
        super::sunset_hud_bars::editable(game, &mut shapes, from, m, "Block counter", w, h);
    }
    // Radar's place (after the HUD's corners, as it is drawn last), so the layout editor can take it too.
    let radar = if with(|s| s.gui.open) { None } else { super::sunset_radar::place(game, w, h, corners, &mods) };
    chat_layout(game, &mut shapes);

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
    if let Some(at) = radar.filter(|_| !with(|s| s.gui.open)) {
        super::sunset_radar::draw(game, batch, at, &mods);
    }
}

// The open GUI's shapes this frame and its scale, from hud for gui_top.
pub(super) static GUI_TOP: Mutex<(Vec<Shape>, f32)> = Mutex::new((Vec::new(), 1.0));

// After all of the frame's HUD (chat, titles, the tab list, toasts) and before its prompts (a server's pack, a dialog, a
// link), which stay on top and take the clicks, as ever: the GUI, scaled about the corner its coordinates start from.
// Notifications' toasts go over it.
pub(super) fn gui_top(game: &Game<'_>, batch: &mut HudBatch) {
    let (mut shapes, scale) = std::mem::replace(&mut *GUI_TOP.lock().unwrap_or_else(|e| e.into_inner()), (Vec::new(), 1.0));
    if !shapes.is_empty() {
        gui_shapes(game, batch, &mut shapes, scale);
    }
    if game.native_window() && game.native_on(ID) {
        super::sunset_toast::draw(game, batch);
    }
}

fn gui_shapes(game: &Game<'_>, batch: &mut HudBatch, shapes: &mut Vec<Shape>, scale: f32) {
    let real = batch.scale;
    batch.scale = real * scale;
    let start = batch.vertices.len();
    paint(game, batch, shapes, 1.0 / batch.scale, true);
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
