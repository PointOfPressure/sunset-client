// The Dashboard style of the click GUI: one solid panel with the categories, a list of modules and the settings of the
// one chosen. Its rows are the menu's own, so draw_gui draws them and gui_press clicks them as ever; this file places them
// and draws around them. A left click on a module turns it on or off, a right click shows its settings.

use super::Game;
use super::sunset::{CATEGORIES, Hit, Module, Shape, State, accent, aliases, dash_menu, dash_search_clear, fit_to, norm, panel_radius, row_h, row_hh, snap_scroll, text_y, wanted_panel_w};
use super::sunset_smooth as smooth;
use horizon_client::hud::HudBatch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

// What can be clicked around the rows: anywhere on the panel, a category.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Part {
    Frame,
    Page(usize),
}

// The modules a category and search list, kept until those change.
struct Listed {
    page: usize,
    query: String,
    count: usize,
    ids: Vec<usize>,
}

// One scrolling column: where the wheel put it, its rows' offsets, and how tall they are.
struct Column {
    scroll: f32,
    rows: Vec<(f32, Hit)>,
    content: f32,
}

// The category shown (0 while a search shows every module's matches), the module chosen, this frame's columns, what the
// pointer is on and was on with when that changed and how lit each is, and when the menu opened.
struct View {
    page: usize,
    chosen: Option<usize>,
    listed: Listed,
    list: Column,
    pane: Column,
    hover: Option<(Hit, Instant)>,
    left: Option<(Hit, Instant)>,
    lit: [u32; 2],
    opened: Option<Instant>,
}

static VIEW: Mutex<View> = Mutex::new(View {
    page: 1,
    chosen: None,
    listed: Listed { page: usize::MAX, query: String::new(), count: 0, ids: Vec::new() },
    list: Column { scroll: 0.0, rows: Vec::new(), content: 0.0 },
    pane: Column { scroll: 0.0, rows: Vec::new(), content: 0.0 },
    hover: None,
    left: None,
    lit: [0; 2],
    opened: None,
});

fn view<R>(f: impl FnOnce(&mut View) -> R) -> R {
    f(&mut VIEW.lock().unwrap_or_else(|e| e.into_inner()))
}

// ClickGUI's Style, as sync_flags last read it.
static ON: AtomicBool = AtomicBool::new(false);

pub(super) fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub(super) fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

// How wide the panel is for ClickGUI's Panel width: 400 at its least, 540 by default, 680 at its most.
const WIDTH_BASE: f32 = 190.0;
const WIDTH_PER_PANEL: f32 = 3.5;
const HEIGHT: f32 = 420.0;
const SIDE: f32 = 116.0;
// Above and below the columns, so a lit row stays clear of the panel's corners.
const PAD: f32 = 8.0;
const LIST_MIN: f32 = 150.0;
const LIST_MAX: f32 = 270.0;
const SEARCH_H: f32 = 18.0;
const TAB_H: f32 = 20.0;
const TAB_GAP: f32 = 2.0;
const TABS_Y: f32 = 34.0;
const TOGGLE_W: f32 = 14.0;
const TOGGLE_H: f32 = 8.0;
// To the right of a row's toggle.
const TOGGLE_PAD: f32 = 7.0;
const CORNER: f32 = 10.0;

const SURFACE: u32 = 0xFF12_1214;
const HAIRLINE: u32 = 0xFF26_262B;
const CONTROL: u32 = 0xFF1C_1C20;
// A lit row or category at its brightest.
const LIT: u32 = 0x10;
const FADE: f32 = 0.08;
const TIP_AFTER: f32 = 0.5;
// The menu coming in.
const OPEN_TIME: f32 = 0.12;
const OPEN_RISE: f32 = 6.0;

fn frame(s: &State) -> [f32; 4] {
    let menu = dash_menu(s);
    let wanted = WIDTH_BASE + wanted_panel_w() * WIDTH_PER_PANEL;
    let (w, h) = ((menu.view_w - 16.0).clamp(0.0, wanted).floor(), (menu.view_h - 16.0).clamp(0.0, HEIGHT).floor());
    [((menu.view_w - w) / 2.0).floor(), ((menu.view_h - h) / 2.0).floor(), w, h]
}

// The list and the settings beside it.
fn columns(s: &State) -> ([f32; 4], [f32; 4]) {
    let [x, y, w, h] = frame(s);
    let (top, room, rest) = (y + PAD, (h - 2.0 * PAD).max(0.0), (w - SIDE).max(0.0));
    let list = (rest * 0.42).clamp(LIST_MIN, LIST_MAX).min(rest).floor();
    ([x + SIDE, top, list, room], [x + SIDE + list, top, rest - list, room])
}

fn module_h() -> f32 {
    row_hh() + 4.0
}

fn height(s: &State, hit: Hit) -> f32 {
    match hit {
        Hit::Module(_) => module_h(),
        _ => row_h(s, hit),
    }
}

// A category's place, for pages 1 and up.
fn tab_rect(s: &State, page: usize) -> [f32; 4] {
    let [x, y, ..] = frame(s);
    [x + 8.0, y + TABS_Y + (page - 1) as f32 * (TAB_H + TAB_GAP), SIDE - 16.0, TAB_H]
}

// Where a module's name and bound key go in its row: clear of its toggle.
pub(super) fn module_text([x, y, w, h]: [f32; 4]) -> [f32; 4] {
    [x, y + 2.0, w - TOGGLE_W - 4.0, h - 4.0]
}

// Once a frame, before anything reads the columns; a column that got shorter comes back to its end.
pub(super) fn prepare(s: &State) {
    if !on() {
        return;
    }
    let menu = dash_menu(s);
    let room = columns(s).0[3];
    view(|v| {
        let page = if menu.search.is_empty() { v.page } else { 0 };
        if v.listed.page != page || v.listed.query != menu.search || v.listed.count != s.modules.len() {
            let query = norm(menu.search);
            v.listed.ids.clear();
            v.listed.ids.extend(s.modules.iter().enumerate().filter_map(|(i, m)| {
                let shown = (page == 0 || m.category == CATEGORIES[page - 1]) && (norm(m.name).contains(&query) || norm(aliases(m.name)).contains(&query));
                shown.then_some(i)
            }));
            v.listed.page = page;
            v.listed.query.clear();
            v.listed.query.push_str(menu.search);
            v.listed.count = s.modules.len();
        }
        // The first module listed, until another is chosen.
        if !v.chosen.is_some_and(|i| v.listed.ids.contains(&i)) {
            v.chosen = v.listed.ids.first().copied();
            v.pane.scroll = 0.0;
            smooth::jump(smooth::PANE);
        }
        let View { listed, list, pane, chosen, .. } = v;
        list.rows.clear();
        let mut y = 0.0;
        for &i in &listed.ids {
            list.rows.push((y, Hit::Module(i)));
            y += module_h();
        }
        list.content = y;
        pane.rows.clear();
        let mut y = 0.0;
        if let Some(i) = *chosen {
            let m = &s.modules[i];
            pane.rows.push((y, Hit::Bind(i)));
            y += row_hh();
            for j in (0..m.settings.len()).filter(|&j| m.setting_visible(j)) {
                let hit = Hit::Setting(i, j);
                pane.rows.push((y, hit));
                y += row_h(s, hit);
            }
        }
        pane.content = y;
        list.scroll = snap_scroll(&list.rows, list.content, room, list.scroll, 0);
        pane.scroll = snap_scroll(&pane.rows, pane.content, room, pane.scroll, 0);
    });
}

pub(super) fn list_scroll() -> f32 {
    view(|v| v.list.scroll)
}

pub(super) fn pane_scroll() -> f32 {
    view(|v| v.pane.scroll)
}

// The top and bottom of both columns, in this style.
pub(super) fn list_y(s: &State) -> Option<[f32; 2]> {
    on().then(|| {
        let [_, y, _, room] = columns(s).0;
        [y, y + room]
    })
}

// The rows from the one the column's top is in, up to its bottom.
fn visible(rows: &[(f32, Hit)], scroll: f32, room: f32) -> std::ops::Range<usize> {
    let first = rows.partition_point(|(off, _)| *off <= scroll).saturating_sub(1);
    first..first + rows[first..].partition_point(|(off, _)| *off < scroll + room)
}

fn column_rows(s: &State, column: &Column, [x, y, w, room]: [f32; 4], scroll: f32, out: &mut Vec<([f32; 4], Hit)>) {
    out.extend(column.rows[visible(&column.rows, scroll, room)].iter().map(|&(off, hit)| ([x, y + off - scroll, w, height(s, hit)], hit)));
}

// The rows in view, and where. One part way in view is drawn cut.
pub(super) fn rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let (list, pane) = columns(s);
    let (list_at, pane_at) = view(|v| (v.list.scroll, v.pane.scroll));
    let (list_at, pane_at) = (smooth::shown(smooth::DASHBOARD, list_at), smooth::shown(smooth::PANE, pane_at));
    let mut out = Vec::new();
    view(|v| {
        column_rows(s, &v.list, list, list_at, &mut out);
        column_rows(s, &v.pane, pane, pane_at, &mut out);
    });
    out
}

// What the pointer can hit around the rows.
pub(super) fn hits(s: &State) -> Vec<([f32; 4], Hit)> {
    if !on() {
        return Vec::new();
    }
    let mut out = vec![(frame(s), Hit::Dash(Part::Frame))];
    out.extend((1..=CATEGORIES.len()).map(|p| (tab_rect(s, p), Hit::Dash(Part::Page(p)))));
    out
}

pub(super) fn press(s: &mut State, part: Part, right: bool) {
    if let (Part::Page(page), false) = (part, right) {
        view(|v| {
            v.page = page;
            v.list.scroll = 0.0;
        });
        smooth::jump(smooth::DASHBOARD);
        dash_search_clear(s);
    }
}

// A module's settings go in the pane.
pub(super) fn choose(i: usize) {
    view(|v| {
        if v.chosen != Some(i) {
            v.chosen = Some(i);
            v.pane.scroll = 0.0;
            smooth::jump(smooth::PANE);
        }
    });
}

pub(super) fn wheel(s: &State, steps: f32, at: (f32, f32)) {
    let (list, pane) = columns(s);
    let by = -steps * 2.0;
    let by = if by > 0.0 { by.ceil() } else { by.floor() } as i32;
    let over_pane = super::sunset::inside(pane, at);
    view(|v| {
        let column = if over_pane { &mut v.pane } else { &mut v.list };
        column.scroll = snap_scroll(&column.rows, column.content, list[3], column.scroll, by);
    });
}

// The menu has just opened.
pub(super) fn opened() {
    view(|v| v.opened = Some(Instant::now()));
}

// How far the menu has come in, while it is coming in: its opacity and how far below its place it still is.
pub(super) fn open_motion() -> Option<(f32, f32)> {
    if !on() || !smooth::enabled() {
        return None;
    }
    view(|v| {
        let t = v.opened?.elapsed().as_secs_f32() / OPEN_TIME;
        if t >= 1.0 {
            v.opened = None;
            return None;
        }
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        Some((eased, (1.0 - eased) * OPEN_RISE))
    })
}

// How lit a row or category is: the one under the pointer fades in, the one it left fades out.
pub(super) fn glow(hit: Hit) -> u32 {
    view(|v| {
        if v.hover.is_some_and(|(h, _)| h == hit) {
            v.lit[0]
        } else if v.left.is_some_and(|(h, _)| h == hit) {
            v.lit[1]
        } else {
            0
        }
    })
}

fn follow_hover(hover: Option<Hit>) {
    let animate = smooth::enabled();
    view(|v| {
        if v.hover.map(|(h, _)| h) != hover {
            v.left = v.hover.map(|(h, _)| (h, Instant::now()));
            v.hover = hover.map(|h| (h, Instant::now()));
        }
        let k = |since: Instant| if animate { (since.elapsed().as_secs_f32() / FADE).min(1.0) } else { 1.0 };
        v.lit[0] = v.hover.map_or(0, |(_, since)| (LIT as f32 * k(since)) as u32);
        v.lit[1] = v.left.map_or(0, |(_, since)| (LIT as f32 * (1.0 - k(since))) as u32);
    });
}

// Whether a module's description shows under the pointer: in the panels always, here after a moment on it.
pub(super) fn wants_tip(i: usize) -> bool {
    !on() || view(|v| v.hover.is_some_and(|(h, since)| h == Hit::Module(i) && since.elapsed().as_secs_f32() >= TIP_AFTER))
}

// A panel's corner as a quarter of a squircle (x^4 + y^4 = 1), from its side round to its top.
const CURVE: usize = 14;

fn curve() -> &'static [[f32; 2]; CURVE + 1] {
    static TABLE: OnceLock<[[f32; 2]; CURVE + 1]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let a = i as f32 / CURVE as f32 * std::f32::consts::FRAC_PI_2;
            [a.cos().max(0.0).sqrt(), a.sin().max(0.0).sqrt()]
        })
    })
}

// A squircle: its middle in plain fills, its corners in fans with a soft edge one pixel wide.
pub(super) fn squircle(game: &Game<'_>, batch: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32, px: f32) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    game.native_fill(batch, [x + r, y, w - 2.0 * r, h], argb);
    game.native_fill(batch, [x, y + r, r, h - 2.0 * r], argb);
    game.native_fill(batch, [x + w - r, y + r, r, h - 2.0 * r], argb);
    let [a, red, g, b] = argb.to_be_bytes();
    let colour = horizon_client::hud::rgba(red, g, b, a);
    let (inner, outer) = ((r - px * 0.5).max(0.0), r + px * 0.5);
    let points = curve();
    for ([cx, cy], [sx, sy]) in [
        ([x + r, y + r], [-1.0, -1.0]),
        ([x + w - r, y + r], [1.0, -1.0]),
        ([x + w - r, y + h - r], [1.0, 1.0]),
        ([x + r, y + h - r], [-1.0, 1.0]),
    ] {
        let at = |p: [f32; 2], d: f32| [cx + sx * p[0] * d, cy + sy * p[1] * d];
        for pair in points.windows(2) {
            let (p0, p1) = (at(pair[0], inner), at(pair[1], inner));
            batch.fill_quad(&game.gui.atlas, [[cx, cy], p0, p1, p1], colour);
            let start = batch.vertices.len();
            batch.fill_quad(&game.gui.atlas, [p0, at(pair[0], outer), at(pair[1], outer), p1], colour);
            // fill_quad's two triangles use corners 0,1,2,0,2,3.
            for (vertex, corner) in batch.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                if corner == 1 || corner == 2 {
                    vertex.color &= 0x00FF_FFFF;
                }
            }
        }
    }
}

// Everything under the rows: the panel, its dividers, the search box, the categories, and each listed module's mark for
// being chosen and its toggle.
pub(super) fn back(s: &State, game: &Game<'_>, hover: Option<Hit>, px: f32, out: &mut Vec<Shape>) {
    follow_hover(hover);
    let solid = 0xFF00_0000 | accent();
    let radius = panel_radius().max(CORNER);
    let f = frame(s);
    let (list, pane) = columns(s);
    let menu = dash_menu(s);
    let page = if menu.search.is_empty() { view(|v| v.page) } else { 0 };

    out.push(Shape::Squircle([f[0] - px, f[1] - px, f[2] + 2.0 * px, f[3] + 2.0 * px], radius + px, HAIRLINE));
    out.push(Shape::Squircle(f, radius, SURFACE));
    out.push(Shape::Fill([list[0], f[1], px, f[3]], HAIRLINE));
    out.push(Shape::Fill([pane[0], f[1], px, f[3]], HAIRLINE));

    // The search box: typing anywhere goes to it.
    let r = [f[0] + 8.0, f[1] + 8.0, SIDE - 16.0, SEARCH_H];
    let selected = menu.select_all && !menu.search.is_empty();
    out.push(Shape::Round(r, SEARCH_H / 2.0, if selected { 0xFF30_60B0 } else { CONTROL }));
    let text = if menu.search.is_empty() { "Search".to_owned() } else { format!("{}_", menu.search) };
    let colour = if menu.search.is_empty() { 0x6E6E76 } else { 0xFFFFFF };
    out.push(Shape::Text(fit_to(game, &text, r[2] - 16.0), [r[0] + 8.0, r[1] + text_y(r[3])], colour));

    // The categories: the one shown a capsule in the accent colour.
    for p in 1..=CATEGORIES.len() {
        let r = tab_rect(s, p);
        if p == page {
            out.push(Shape::Round(r, TAB_H / 2.0, solid));
        } else {
            let lit = glow(Hit::Dash(Part::Page(p)));
            if lit > 0 {
                out.push(Shape::Round(r, TAB_H / 2.0, lit << 24 | 0x00FF_FFFF));
            }
        }
        let colour = if p == page { 0xFFFFFF } else { 0xD4D4D8 };
        out.push(Shape::Head(CATEGORIES[p - 1].label().to_owned(), [r[0] + 10.0, r[1] + text_y(r[3])], colour));
    }

    // The list's rows: the chosen one lit, and each module's toggle.
    let scroll = smooth::shown(smooth::DASHBOARD, list_scroll());
    let (content, pane_content) = view(|v| {
        let shown = visible(&v.list.rows, scroll, list[3]);
        out.push(Shape::ClipStart);
        for &(off, hit) in &v.list.rows[shown] {
            let Hit::Module(i) = hit else {
                continue;
            };
            let y = list[1] + off - scroll;
            if v.chosen == Some(i) {
                out.push(Shape::Fill([list[0], y, list[2], module_h()], 0x16FF_FFFF));
            }
            let on = s.modules[i].on;
            let (tx, ty) = (list[0] + list[2] - TOGGLE_PAD - TOGGLE_W, y + (module_h() - TOGGLE_H) / 2.0);
            out.push(Shape::Round([tx, ty, TOGGLE_W, TOGGLE_H], TOGGLE_H / 2.0, if on { solid } else { 0xFF2E_2E33 }));
            let knob = TOGGLE_H - 2.0;
            out.push(Shape::Round([if on { tx + TOGGLE_W - knob - 1.0 } else { tx + 1.0 }, ty + 1.0, knob, knob], knob / 2.0, SURFACE));
        }
        out.push(Shape::ClipEnd([list[0], list[1], list[0] + list[2], list[1] + list[3]]));
        (v.list.content, v.pane.content)
    });

    // The scroll markers: how much of a column shows, and where.
    let pane_at = smooth::shown(smooth::PANE, pane_scroll());
    for (c, content, at) in [(list, content, scroll), (pane, pane_content, pane_at)] {
        let room = c[3];
        if content > room && room > 0.0 {
            let thumb = (room * room / content).max(10.0);
            let t = (at / (content - room)).min(1.0);
            out.push(Shape::Round([c[0] + c[2] - 3.0, c[1] + (room - thumb) * t, 2.0, thumb], 1.0, 0x50FF_FFFF));
        }
    }
}

// A module's one dry line under the pointer; one not listed here says the first sentence of its own.
pub(super) fn short(m: &Module) -> &'static str {
    match m.name {
        "Atmosphere" => "Custom skies.",
        "Kill Aura" => "Attacks nearby entities.",
        "Block Overlay" => "Colors the targeted block.",
        "Surround" => "Surrounds you in blocks.",
        "Self Trap" => "Places blocks above your head.",
        "Auto City" => "Mines into a player's surround.",
        "Auto Trap" => "Traps players in blocks.",
        "Self Web" => "Places webs on you.",
        "Auto Web" => "Places webs on players.",
        "Hole Filler" => "Fills holes with blocks.",
        "Burrow" => "Places a block inside you.",
        "Offhand" => "Keeps an item or totem in your offhand.",
        "Aim Assist" => "Helps your aim.",
        "Crystal Optimizer" => "Breaks hit crystals at once.",
        "Auto EXP" => "Repairs gear with XP bottles.",
        "Crosshair" => "Custom crosshair.",
        "Wind Jump" => "Jumps with wind charges.",
        "Hold to Eat" => "Eats from a held hotbar key.",
        "Pearl Catch" => "Catches pearls with wind charges.",
        "Loadouts" => "Saved hotbar layouts.",
        "Auto Fish" => "Fishes for you.",
        "Auto Gap" => "Eats golden apples.",
        "Auto Steal" => "Empties containers you open.",
        "Inventory Cleaner" => "Drops listed items.",
        "Fast Use" => "Uses items faster.",
        "Spam" => "Spams chat messages.",
        "Reach" => "Longer reach.",
        "Ghost Hand" => "Uses containers through blocks.",
        "Air Place" => "Places blocks in the air.",
        "Speed Mine" => "Mines faster.",
        "Packet Mine" => "Mines blocks in the background.",
        "No Break Reset" => "Keeps break progress on item swap.",
        "Anchor" => "Pulls you into holes.",
        "Radar" => "Shows nearby players.",
        "Freecam" => "Moves the camera off your body.",
        "Free Look" => "Looks around without turning.",
        "Xray" => "Shows only listed blocks.",
        "Tunnel ESP" => "Highlights tunnels.",
        "Void ESP" => "Marks holes to the void.",
        "Light Overlay" => "Shows mob spawn spots.",
        "No Render" => "Hides fog and overlays.",
        "Timer" => "Changes game speed.",
        "No Slow" => "No item-use slowdown.",
        "Hand View" => "Moves and sizes your hands.",
        "Trigger Bot" => "Attacks what you aim at.",
        "Criticals" => "Makes hits critical.",
        "Sprint" => "Sprints for you.",
        "Auto Walk" => "Walks forward for you.",
        "Flight" => "Lets you fly.",
        "Speed" => "Moves faster on the ground.",
        "No Fall" => "No fall damage.",
        "Velocity" => "Reduces knockback.",
        "Spider" => "Climbs walls.",
        "Step" => "Walks up blocks.",
        "GUI Move" => "Moves while a menu is open.",
        "Glide" => "Falls slowly.",
        "No Jump Delay" => "Jumps again on landing.",
        "Air Jump" => "Jumps in mid-air.",
        "Elytra Boost" => "Speeds up gliding.",
        "Anti Void" => "Bounces you out of the void.",
        "Fast Place" => "Places blocks faster.",
        "Break Delay" => "Changes the delay between breaks.",
        "Auto Respawn" => "Respawns when you die.",
        "Death Location" => "Saves where you died.",
        "Panic" => "Turns everything off and hides Sunset.",
        "ClickGUI" => "How this menu looks.",
        "HUD" => "On-screen info.",
        "Fullbright" => "Lights everything up.",
        "ESP" => "Boxes around entities.",
        "Tracers" => "Lines to entities.",
        "Nametags" => "Bigger name tags.",
        "Storage ESP" => "Boxes around storage.",
        "Auto Weapon" => "Switches to your best weapon.",
        "Auto Log" => "Leaves at low health.",
        "Sneak" => "Sneaks for you.",
        "Safe Walk" => "Stops you walking off edges.",
        "Fast Climb" => "Climbs faster.",
        "High Jump" => "Jumps higher.",
        "Parkour" => "Jumps at block edges.",
        "Blink" => "Holds back your movement, then sends it.",
        "Scaffold" => "Places blocks under you.",
        "Anti Hunger" => "Uses less food.",
        "Anti AFK" => "Stops AFK kicks.",
        "Auto Clicker" => "Clicks for you.",
        "Auto Eat" => "Eats when hungry.",
        "Zoom" => "Zooms in.",
        "Breadcrumbs" => "Draws your path.",
        "Block ESP" => "Boxes around chosen blocks.",
        "Waypoints" => "Shows saved places.",
        "Auto Armor" => "Wears your best armor.",
        "Reverse Step" => "Falls fast off blocks.",
        "Long Jump" => "Jumps further.",
        "Auto Jump" => "Jumps as you walk.",
        "Chest Swap" => "Swaps chestplate and elytra.",
        "Auto Replenish" => "Refills hotbar stacks.",
        "Auto Tool" => "Switches to the best tool.",
        "Middle Click Extra" => "Uses a hotbar item on middle click.",
        "Trajectories" => "Shows where throws land.",
        "Logout Spots" => "Marks where players logged out.",
        "Bow Spam" => "Fires arrows quickly.",
        "Notifier" => "Reports joins and leaves.",
        "Weather Changer" => "Sets the weather you see.",
        "Hole ESP" => "Marks safe holes.",
        "Nuker" => "Breaks blocks around you.",
        "Time Changer" => "Sets the time you see.",
        "Target ESP" => "Marks your target.",
        "Anti Vanish" => "Reports vanished players.",
        "Auto Shearer" => "Shears sheep.",
        "Auto Nametag" => "Name tags nearby mobs.",
        "Auto Mount" => "Mounts nearby mobs.",
        "Flamethrower" => "Sets food animals on fire.",
        "Liquid Filler" => "Fills water and lava with blocks.",
        "Spawn Proofer" => "Blocks mob spawn spots.",
        "Bow Aimbot" => "Aims your bow.",
        "Spawner Alerts" => "Reports nearby spawners.",
        "Crystal Aura" => "Places and breaks crystals on enemies.",
        "Anchor Aura" => "Sets off anchors on enemies.",
        "Crystal Macro" => "Places and breaks crystals while held.",
        "Anchor Macro" => "Places and sets off an anchor.",
        "Shield Breaker" => "Disables shields with an axe.",
        "Mace" => "Picks the best mace for a fall.",
        "Spear Swap" => "Lunges with your best spear.",
        "W Tap" => "Resets sprint between hits.",
        "Jump Reset" => "Jumps to take less knockback.",
        "Friend Guard" => "Stops hits on friends.",
        "Key Throw" => "Throws a pearl or wind charge on a key.",
        "RF Click" => "R and F act as mouse buttons.",
        "Auto Shield" => "Keeps your shield raised.",
        "Fake Lag" => "Delays what you send.",
        "Knockback Delay" => "Delays knockback.",
        "Sword PvP Bot" => "Fights the nearest player.",
        "Unlock All Recipes" => "Shows every recipe as unlocked.",
        "Client Sort" => "Sorts inventories in your order.",
        "Shulker Peek" => "Shows what is in a shulker box.",
        _ => m.about.split(". ").next().unwrap_or(m.about),
    }
}
