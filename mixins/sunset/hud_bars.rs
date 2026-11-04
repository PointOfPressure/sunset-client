// HUD's Armor HUD (with Compact style), Compass, Shield status, Item durability (on every damageable item's icon, and remaining/max in its tooltip), the
// settings that change the game's own bars (Mount HUD, Food preview, Saturation, Exhaustion; drawn by hunger_flags.rs),
// and the layout editor: with the chat (or the menu) open, Coordinates, Compass and the Armor HUD move by dragging.
use super::Game;
use super::sunset::{Module, Shape};
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use horizon_client::hud_layout::sunset_hunger::{BARS, Bars};
use horizon_client::items::ItemStack;
use std::cell::Cell;
use std::ops::ControlFlow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed};

// Item durability: 0 off, 1 the number left, 2 the percent left; and the tooltip line.
static DURABILITY: AtomicU8 = AtomicU8::new(0);
static TOOLTIP: AtomicBool = AtomicBool::new(false);
// The elements that can be dragged: where each was drawn last frame (x, y, width, height) and the screen it was on.
static PLACED: Mutex<Vec<Placed>> = Mutex::new(Vec::new());
// The left button was down last frame, while the chat was open.
static CHAT_PRESSED: AtomicBool = AtomicBool::new(false);

type Placed = (&'static str, [f32; 4], [f32; 2]);
// Shield status: the tick the shield's cooldown was first seen, and how much of it was left then.
static SHIELD_SINCE: Mutex<Option<(f32, f32)>> = Mutex::new(None);

thread_local! {
    // An icon drawn without Item durability's number (Sunset's own icons, and the icon under the number).
    static PLAIN: Cell<bool> = const { Cell::new(false) };
}

// A slot box of the hotbar's sprite: one slot and its frame.
const SLOT: f32 = 20.0;

// Each tick: the settings the game's own drawing reads.
pub(super) fn tick(game: &Game<'_>, mods: &[Module], shown: bool) {
    let hud = mods.iter().find(|m| m.name == "HUD" && m.on).filter(|_| shown);
    let on = |name: &str| hud.is_some_and(|m| m.b(name));
    let durability = if !on("Item durability") {
        0
    } else if hud.is_some_and(|m| m.p("Durability format") == "Percent") {
        2
    } else {
        1
    };
    DURABILITY.store(durability, Relaxed);
    TOOLTIP.store(on("Item durability") && on("Durability tooltip"), Relaxed);
    let mut bars = Bars::default();
    if let Some(p) = game.player.as_ref() {
        let food = &p.health.food;
        bars.mount = on("Mount HUD");
        bars.mount_xp = bars.mount && game.riding.is_some() && !game.mount_jumping() && !game.waypoints.has_waypoints();
        // The air bubbles take the row over the food while they show.
        let bubbles = p.air < 300 || p.is_eye_in_fluid(horizon_client::motion::WATER);
        bars.raise = if bubbles { 20 } else { 10 };
        bars.saturation = on("Saturation");
        bars.exhaustion = on("Exhaustion").then_some(food.exhaustion);
        bars.preview = [game.held(), game.offhand.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|s| game.res.items().get(&s.item)?.food)
            .filter(|_| on("Food preview"))
            .map(|(nutrition, saturation)| {
                let after = (food.food + nutrition as i32).min(20);
                (after, (food.saturation + saturation).min(after as f32))
            });
    }
    *BARS.lock().unwrap_or_else(|e| e.into_inner()) = bars;
}

// What is left of `max`, from 1 (all of it) to 0.
fn left_fraction(damage: u32, max: u32) -> f32 {
    (max.saturating_sub(damage) as f32 / max.max(1) as f32).clamp(0.0, 1.0)
}

// The game's durability bar colour: green when whole, through yellow, to red as it wears.
fn wear_rgb(fraction: f32) -> u32 {
    horizon_client::hud_layout::hsv_to_argb(fraction / 3.0, 1.0, 1.0, 0) & 0x00FF_FFFF
}

// Percent left rounds as follows: 1 for the last point, never 0 while it holds.
fn percent(damage: u32, max: u32) -> u32 {
    let left = max.saturating_sub(damage);
    if left == 0 { 0 } else { ((left * 100).div_ceil(max)).max(1) }
}

fn max_damage(game: &Game<'_>, stack: &ItemStack) -> Option<u32> {
    game.res.items().get(&stack.item).map(|i| i.max_damage).filter(|m| *m > 0)
}

// An icon drawn by Sunset itself: no Item durability number on it.
pub(super) fn plain<R>(f: impl FnOnce() -> R) -> R {
    PLAIN.with(|p| p.set(true));
    let r = f();
    PLAIN.with(|p| p.set(false));
    r
}

// Item durability: any damageable item's icon (hotbar, inventories, the stack on the pointer) gets what it has left,
// small, in its top left corner, in the colour of its bar.
pub(super) fn item_model(game: &Game<'_>, batch: &mut HudBatch, stack: &ItemStack, x: f32, y: f32) -> ControlFlow<()> {
    let mode = DURABILITY.load(Relaxed);
    if mode == 0 || PLAIN.with(Cell::get) {
        return ControlFlow::Continue(());
    }
    let Some(max) = max_damage(game, stack) else {
        return ControlFlow::Continue(());
    };
    plain(|| game.draw_item_model(batch, stack, x, y));
    let text = if mode == 2 { format!("{}%", percent(stack.damage, max)) } else { max.saturating_sub(stack.damage).to_string() };
    let start = batch.vertices.len();
    game.native_text(batch, &text, [x, y], wear_rgb(left_fraction(stack.damage, max)), true);
    batch.scale_since(start, x, y, 0.5, 0.5);
    ControlFlow::Break(())
}

thread_local! {
    // The tooltip being drawn again with the durability line.
    static TOOLTIP_AGAIN: Cell<bool> = const { Cell::new(false) };
}

// Item durability's tooltip line, under the name: remaining / max (the game's own line shows instead with advanced
// tooltips on a worn item). The tooltip is drawn again with the line, and the first one left out.
pub(super) fn tooltip(
    game: &mut Game<'_>,
    batch: &mut HudBatch,
    stack: &ItemStack,
    creative: bool,
    [x, y]: [f32; 2],
    after_name: &[(horizon_client::text::Text, u32)],
) -> ControlFlow<()> {
    if TOOLTIP_AGAIN.with(Cell::get) || !TOOLTIP.load(Relaxed) || stack.damage > 0 && game.settings.file.bool("advancedItemTooltips") {
        return ControlFlow::Continue(());
    }
    let Some(max) = max_damage(game, stack) else {
        return ControlFlow::Continue(());
    };
    let [_, r, g, b] = wear_rgb(left_fraction(stack.damage, max)).to_be_bytes();
    let text = format!("Durability: {} / {max}", max.saturating_sub(stack.damage));
    let mut lines = after_name.to_vec();
    lines.push((horizon_client::text::Text::literal(text), horizon_client::hud::rgba(r, g, b, 255)));
    TOOLTIP_AGAIN.with(|c| c.set(true));
    game.draw_stack_tooltip_with(batch, stack, creative, x, y, lines);
    TOOLTIP_AGAIN.with(|c| c.set(false));
    ControlFlow::Break(())
}

fn placed<R>(f: impl FnOnce(&mut Vec<Placed>) -> R) -> R {
    f(&mut PLACED.lock().unwrap_or_else(|e| e.into_inner()))
}

// A new frame of the HUD: what it draws is placed again.
pub(super) fn clear_placed() {
    placed(Vec::clear);
}

// `name` was drawn at `rect` on a screen `w` by `h`.
pub(super) fn place(name: &'static str, rect: [f32; 4], w: f32, h: f32) {
    placed(|p| {
        p.retain(|e| e.0 != name);
        p.push((name, rect, [w, h]));
    });
}

// An element's position as fractions of the screen, from where its top left corner is dragged to.
pub(super) fn drag_to(name: &str, x: f32, y: f32) -> Option<(f64, f64)> {
    let (_, [_, _, w, h], [sw, sh]) = placed(|p| p.iter().find(|e| e.0 == name).copied())?;
    Some(fractions([x, y], [w, h], [sw, sh]))
}

fn fractions(at: [f32; 2], size: [f32; 2], screen: [f32; 2]) -> (f64, f64) {
    let fx = at[0].clamp(0.0, (screen[0] - size[0]).max(0.0)) / screen[0].max(1.0);
    let fy = at[1].clamp(0.0, (screen[1] - size[1]).max(0.0)) / screen[1].max(1.0);
    (f64::from(fx), f64::from(fy))
}

// A press on an element (the chat or the menu open): which, and where in it the pointer took it.
pub(super) fn grab(at: (f32, f32)) -> Option<(&'static str, f32, f32)> {
    placed(|p| {
        p.iter()
            .rev()
            .find(|(_, [x, y, w, h], _)| at.0 >= *x && at.0 < x + w && at.1 >= *y && at.1 < y + h)
            .map(|(name, [x, y, _, _], _)| (*name, at.0 - x, at.1 - y))
    })
}

// What the chat's layout editor does this frame with a drag.
pub(super) enum ChatDrag {
    // A press took an element, where in it.
    Start(&'static str, f32, f32),
    // The button came up.
    End,
}

// The layout editor with the chat open: a frame round each element, a press on one takes it, its release ends the drag.
pub(super) fn chat_layout(game: &Game<'_>, dragging: Option<&'static str>, out: &mut Vec<Shape>) -> Option<ChatDrag> {
    let down = game.physical_buttons.contains(&winit::event::MouseButton::Left);
    let was = CHAT_PRESSED.swap(down, Relaxed);
    for (name, [x, y, w, h], _) in placed(|p| p.clone()) {
        let colour = if dragging == Some(name) { 0xC0FF_FFFF } else { 0x60FF_FFFF };
        out.push(Shape::Fill([x - 1.0, y - 1.0, w + 2.0, 1.0], colour));
        out.push(Shape::Fill([x - 1.0, y + h, w + 2.0, 1.0], colour));
        out.push(Shape::Fill([x - 1.0, y, 1.0, h], colour));
        out.push(Shape::Fill([x + w, y, 1.0, h], colour));
    }
    match dragging {
        Some(_) if !down => Some(ChatDrag::End),
        None if down && !was => grab(game.cursor).map(|(name, dx, dy)| ChatDrag::Start(name, dx, dy)),
        _ => None,
    }
}

// The chat closed: the next press there starts afresh.
pub(super) fn chat_closed() {
    CHAT_PRESSED.store(false, Relaxed);
}

// Compass: 140 degrees of headings round the one you face, a tick every 5 degrees, a label
// every 15 (N, NE, E... at the eighths), and your heading over the middle.
fn compass(game: &Game<'_>, hud: &Module, w: f32, h: f32, out: &mut Vec<Shape>) {
    const WIDTH: f32 = 180.0;
    const HEIGHT: f32 = 26.0;
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (x0, y0) = if hud.p("Compass position") == "Custom" {
        (
            (hud.n("Compass x") as f32 * w).clamp(0.0, (w - WIDTH).max(0.0)),
            (hud.n("Compass y") as f32 * h).clamp(0.0, (h - HEIGHT).max(0.0)),
        )
    } else {
        ((w - WIDTH) / 2.0, 2.0)
    };
    place("Compass", [x0, y0, WIDTH, HEIGHT], w, h);
    out.push(Shape::Fill([x0, y0 + 9.0, WIDTH, HEIGHT - 9.0], 0x5000_0000));
    let heading = heading(p.yaw);
    let middle = y0 + 9.0 + (HEIGHT - 9.0) / 2.0;
    for (offset, degree) in ticks(heading) {
        let x = x0 + offset * WIDTH;
        let major = degree % 45 == 0;
        let (top, bottom) = if major { (middle - 7.0, middle - 1.0) } else { (middle - 5.0, middle - 3.0) };
        out.push(Shape::Fill([x, top, 1.0, bottom - top], if major { 0xD0FF_FFFF } else { 0x90FF_FFFF }));
        if degree % 15 == 0 {
            let text = if major { ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][(degree / 45) as usize].to_owned() } else { degree.to_string() };
            let (scale, colour) = if major { (0.8, 0xFFFFFF) } else { (0.6, 0xAAAAAA) };
            let tx = x - game.native_text_width(&text) * scale / 2.0;
            out.push(Shape::Scaled(text, [tx, middle + 1.0], colour, scale));
        }
    }
    let mid = x0 + WIDTH / 2.0;
    out.push(Shape::Fill([mid - 0.5, y0 + 9.0, 1.0, HEIGHT - 9.0], 0xFFE5_2E33));
    let text = (heading as i32).rem_euclid(360).to_string();
    let tx = mid - game.native_text_width(&text) * 0.4;
    out.push(Shape::Scaled(text, [tx, y0], 0xE6E6E6, 0.8));
}

// Your heading from north (0) round through east (90), as a compass reads it: the game's yaw is 0 facing south.
fn heading(yaw: f32) -> f32 {
    (yaw + 180.0).rem_euclid(360.0)
}

// The compass's ticks for `heading`: each 5 degrees within 70 either side, as how far across the strip it is (0 to 1)
// and its heading (0 to 355).
fn ticks(heading: f32) -> Vec<(f32, i32)> {
    let from = heading - 70.0;
    let first = (from / 5.0).ceil() as i32 * 5;
    (0..)
        .map(|i| first + i * 5)
        .take_while(|&d| d as f32 <= heading + 70.0)
        .map(|d| ((d as f32 - from) / 140.0, d.rem_euclid(360)))
        .collect()
}

// One piece the Armor HUD shows: its item, damage and maximum (0: none), or an empty slot.
type Piece = Option<(std::sync::Arc<str>, u32, u32)>;

pub(super) fn draw(game: &Game<'_>, hud: &Module, w: f32, h: f32, corners: &mut Corners, out: &mut Vec<Shape>) {
    if hud.b("Compass") {
        compass(game, hud, w, h, out);
    }
    if hud.b("Shield status") {
        shield(game, w, h, out);
    } else {
        *SHIELD_SINCE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    if !hud.b("Armor HUD") || game.player.is_none() || game.spectator() {
        return;
    }
    let piece = |s: Option<&ItemStack>| -> Piece {
        s.map(|s| (std::sync::Arc::clone(&s.item), s.damage, max_damage(game, s).unwrap_or(0)))
    };
    let mut pieces: Vec<Piece> = game.armor.iter().map(|s| piece(s.as_ref())).collect();
    if hud.b("Armor HUD hands") {
        pieces.push(piece(game.held()));
        pieces.push(piece(game.offhand.as_ref()));
    }
    if hud.b("Armor HUD hide empty") {
        pieces.retain(Option::is_some);
    }
    if pieces.iter().all(Option::is_none) {
        return;
    }
    let vertical = hud.p("Armor HUD layout") == "Vertical";
    let compact = hud.p("Armor HUD style") == "Compact";
    let shows = hud.p("Armor HUD durability");
    let (bar, words) = (shows == "Bar" || shows == "Bar and percent", shows != "Bar");
    let label = |p: &Piece| -> Option<String> {
        let (_, damage, max) = p.as_ref().filter(|p| words && p.2 > 0)?;
        Some(if shows == "Number" { max.saturating_sub(*damage).to_string() } else { format!("{}%", percent(*damage, *max)) })
    };
    // Each piece's size: a slot box, or (Compact) the icon and its words beside it.
    let sizes: Vec<[f32; 2]> = pieces
        .iter()
        .map(|p| if compact { [16.0 + label(p).map_or(0.0, |t| 3.0 + game.native_text_width(&t)), 16.0] } else { [SLOT + 2.0, SLOT + 2.0] })
        .collect();
    let gap = if compact { 4.0 } else { 0.0 };
    // Boxes in a row share their frames, as on the hotbar.
    let step = |s: [f32; 2]| if vertical { s[1] + gap } else if compact { s[0] + gap } else { SLOT };
    let along: f32 = sizes.iter().map(|s| step(*s)).sum::<f32>() - gap + if vertical || compact { 0.0 } else { 2.0 };
    let across = sizes.iter().map(|s| if vertical { s[0] } else { s[1] }).fold(0.0, f32::max);
    let (bw, bh) = if vertical { (across, along) } else { (along, across) };
    let (x0, y0) = match hud.p("Armor HUD position") {
        "Top left" => corners.place(Corner::TopLeft, bw, bh),
        "Top right" => corners.place(Corner::TopRight, bw, bh),
        "Bottom left" => corners.place(Corner::BottomLeft, bw, bh),
        "Bottom right" => corners.place(Corner::BottomRight, bw, bh),
        "Custom" => (
            (hud.n("Armor HUD x") as f32 * w).clamp(0.0, (w - bw).max(0.0)),
            (hud.n("Armor HUD y") as f32 * h).clamp(0.0, (h - bh).max(0.0)),
        ),
        // Beside the hotbar, on the side the off hand slot isn't.
        _ if game.settings.main_hand_left => (w / 2.0 - 91.0 - 6.0 - bw, h - bh),
        _ => (w / 2.0 + 91.0 + 6.0, h - bh),
    };
    place("Armor HUD", [x0, y0, bw, bh], w, h);
    if !compact && !vertical {
        frame(out, x0, y0, pieces.len());
    }
    let mut at = 0.0;
    for (p, size) in pieces.iter().zip(&sizes) {
        let (x, y) = if vertical { (x0, y0 + at) } else { (x0 + at, y0) };
        at += step(*size);
        if !compact && vertical {
            frame(out, x, y, 1);
        }
        let (ix, iy) = if compact { (x, y) } else { (x + 3.0, y + 3.0) };
        let Some((item, damage, max)) = p else {
            continue;
        };
        out.push(Shape::Item(std::sync::Arc::clone(item), [ix, iy], 16.0));
        if *max == 0 {
            continue;
        }
        let fraction = left_fraction(*damage, *max);
        let rgb = wear_rgb(fraction);
        if bar {
            out.push(Shape::Fill([ix + 2.0, iy + 13.0, 13.0, 2.0], 0xFF00_0000));
            out.push(Shape::Fill([ix + 2.0, iy + 13.0, (13.0 * fraction).round(), 1.0], 0xFF00_0000 | rgb));
        }
        let Some(text) = label(p) else {
            continue;
        };
        if compact {
            out.push(Shape::Text(text, [ix + 19.0, iy + 4.0], rgb));
        } else {
            // In the box: over the icon's foot, or (with the bar) its head.
            let tw = game.native_text_width(&text) * 0.75;
            let ty = if bar { iy } else { iy + 16.0 - 6.75 };
            out.push(Shape::Scaled(text, [ix + 16.0 - tw, ty], rgb, 0.75));
        }
    }
}

// Slot boxes from the hotbar's own sprite: its left end and `slots` slots, then its right edge.
fn frame(out: &mut Vec<Shape>, x: f32, y: f32, slots: usize) {
    let width = 1.0 + SLOT * slots as f32;
    out.push(Shape::Sprite("gui/sprites/hud/hotbar", [x, y, width, 22.0], [0.0, 0.0, width / 182.0, 1.0]));
    out.push(Shape::Sprite("gui/sprites/hud/hotbar", [x + width, y, 1.0, 22.0], [181.0 / 182.0, 0.0, 1.0, 1.0]));
}

// Shield status: while an axe has your shield down, its icon, the seconds left and a bar, left of the crosshair (the
// Target HUD has its right).
fn shield(game: &Game<'_>, w: f32, h: f32, out: &mut Vec<Shape>) {
    let shield = [game.offhand.as_ref(), game.held()].into_iter().flatten().find(|s| s.item.ends_with("shield"));
    let group = shield.map_or_else(
        || std::sync::Arc::from("minecraft:shield"),
        |s| horizon_client::cooldowns::group(s, game.res.items().get(&s.item).and_then(|i| i.use_cooldown.as_ref())),
    );
    let partial = game.clock.fraction() as f32;
    let left = game.cooldowns.percent(&group, partial);
    let mut since = SHIELD_SINCE.lock().unwrap_or_else(|e| e.into_inner());
    if left <= 0.0 {
        *since = None;
        return;
    }
    let now = game.ticks as f32 + partial;
    let (start, first) = *since.get_or_insert((now, left));
    // The whole cooldown, from how fast it runs down: an axe's five seconds until that can be told.
    let (elapsed, run) = (now - start, first - left);
    let total = if elapsed >= 2.0 && run > 0.005 { elapsed / run } else { 100.0 };
    let text = format!("{:.1}s", left * total / 20.0);
    let width = 24.0 + game.native_text_width(&text);
    let (x, y) = (w / 2.0 - 12.0 - width, h / 2.0 + 12.0);
    out.push(Shape::Fill([x, y, width, 24.0], 0x9000_0000));
    out.push(Shape::Item(std::sync::Arc::from("minecraft:shield"), [x + 2.0, y + 2.0], 16.0));
    out.push(Shape::Text(text, [x + 20.0, y + 6.0], 0xFF5555));
    out.push(Shape::Fill([x + 2.0, y + 20.0, width - 4.0, 2.0], 0xFF30_3030));
    out.push(Shape::Fill([x + 2.0, y + 20.0, (width - 4.0) * left, 2.0], 0xFFFF_5555));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compass_reads_north_at_the_games_yaw_180() {
        assert_eq!([heading(180.0), heading(-90.0), heading(0.0), heading(90.0), heading(-180.0)], [0.0, 90.0, 180.0, 270.0, 0.0]);
    }

    #[test]
    fn compass_ticks_cover_140_degrees_every_5() {
        let t = ticks(0.0);
        assert_eq!(t.len(), 29);
        assert_eq!(t.first().copied(), Some((0.0, 290)));
        assert_eq!(t[14], (0.5, 0));
        assert_eq!(t.last().copied(), Some((1.0, 70)));
        // Off the 5s: the first tick is past the strip's left edge, never before it.
        let t = ticks(12.5);
        assert!(t.iter().all(|&(at, d)| (0.0..=1.0).contains(&at) && d % 5 == 0));
        assert_eq!(t.first().map(|v| v.1), Some(305));
    }

    #[test]
    fn a_dragged_element_stays_on_the_screen() {
        assert_eq!(fractions([64.0, 32.0], [32.0, 16.0], [128.0, 64.0]), (0.5, 0.5));
        assert_eq!(fractions([-30.0, 90.0], [32.0, 16.0], [128.0, 64.0]), (0.0, 0.75));
        assert_eq!(fractions([120.0, 0.0], [32.0, 16.0], [128.0, 64.0]), (0.75, 0.0));
    }
}
