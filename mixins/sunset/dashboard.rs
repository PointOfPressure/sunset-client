// The Dashboard style of the click GUI: one solid panel with a header (the name and the search), the categories down the
// left, the modules of the one picked, and the settings of the module chosen, in its sections. Its rows are the menu's own,
// so draw_gui draws them and gui_press clicks them as ever; this file places them and draws around them. A left click on a
// module turns it on or off, a right click shows its settings; the switch over its settings turns it on or off too.

use super::Game;
use super::sunset::{CATEGORIES, Hit, Shape, State, accent, aliases, control_radius, dash_menu, dash_search_clear, fit_to, norm, panel_radius, row_h, row_hh, snap_scroll, text_y, wanted_panel_w};
use super::sunset_smooth as smooth;
use horizon_client::hud::HudBatch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

// What can be clicked around the rows: anywhere on the panel, a category, the chosen module's switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Part {
    Frame,
    Page(usize),
    Toggle,
}

// The modules a category and search list, kept until those change.
struct Listed {
    page: usize,
    query: String,
    count: usize,
    // The modules switched off in Modules.
    hidden: Vec<&'static str>,
    ids: Vec<usize>,
}

// One scrolling column: where the wheel put it, its rows' offsets, where the wheel stops (the rows, and a section's top so
// its title shows), and how tall they are.
struct Column {
    scroll: f32,
    rows: Vec<(f32, Hit)>,
    stops: Vec<(f32, Hit)>,
    content: f32,
}

// A section of the chosen module's settings: its card's top and bottom in the column, and its title.
struct Card {
    top: f32,
    bottom: f32,
    title: Option<&'static str>,
}

// The category shown (0 while a search shows every module's matches), the module chosen, this frame's columns, what the
// pointer is on and was on with when that changed and how lit each is, and when the menu opened.
struct View {
    page: usize,
    chosen: Option<usize>,
    listed: Listed,
    list: Column,
    pane: Column,
    cards: Vec<Card>,
    // The settings with a line above them: all but the first of a section.
    lined: Vec<Hit>,
    hover: Option<(Hit, Instant)>,
    left: Option<(Hit, Instant)>,
    lit: [u32; 2],
    opened: Option<Instant>,
}

static VIEW: Mutex<View> = Mutex::new(View {
    page: 1,
    chosen: None,
    listed: Listed { page: usize::MAX, query: String::new(), count: 0, hidden: Vec::new(), ids: Vec::new() },
    list: Column { scroll: 0.0, rows: Vec::new(), stops: Vec::new(), content: 0.0 },
    pane: Column { scroll: 0.0, rows: Vec::new(), stops: Vec::new(), content: 0.0 },
    cards: Vec::new(),
    lined: Vec::new(),
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

// How wide the panel is for ClickGUI's Panel width: 400 at its least, 610 by default, 680 at its most.
const WIDTH_BASE: f32 = 190.0;
const WIDTH_PER_PANEL: f32 = 3.5;
const HEIGHT: f32 = 420.0;
const SIDE: f32 = 116.0;
// Above and below the columns, so a lit row stays clear of the panel's corners.
const PAD: f32 = 8.0;
const LIST_MIN: f32 = 150.0;
const LIST_MAX: f32 = 270.0;
const SEARCH_H: f32 = 18.0;
const SEARCH_MAX: f32 = 300.0;
// The header across the top, and the band at the top of the module list and of the settings (their titles).
const HEADER_H: f32 = 28.0;
const BAND_H: f32 = 34.0;
const TAB_H: f32 = 20.0;
const TAB_GAP: f32 = 2.0;
const TABS_Y: f32 = HEADER_H + 8.0;
// The chosen module's switch, at the right of its band.
const TOGGLE_W: f32 = 22.0;
const TOGGLE_H: f32 = 12.0;
// The settings' sections: inset from the column's sides, apart from each other, the rows inside them, a title's height.
const CARD_M: f32 = 8.0;
const CARD_GAP: f32 = 6.0;
const CARD_PAD: f32 = 3.0;
const CARD_IN: f32 = 2.0;
const TITLE_H: f32 = 13.0;

const SURFACE: u32 = 0xFF12_1214;
const HAIRLINE: u32 = 0xFF26_262B;
const CONTROL: u32 = 0xFF1C_1C20;
const CARD: u32 = 0xFF17_171A;
const MUTED: u32 = 0x8C8C94;
// A lit row or category at its brightest.
const LIT: u32 = 0x10;
const FADE: f32 = 0.08;
const TIP_AFTER: f32 = 0.5;
// The menu coming in.
const OPEN_TIME: f32 = 0.12;
const OPEN_RISE: f32 = 6.0;

fn frame(s: &State) -> [f32; 4] {
    let menu = dash_menu(s);
    frame_in(menu.view_w, menu.view_h)
}

fn frame_in(view_w: f32, view_h: f32) -> [f32; 4] {
    let wanted = WIDTH_BASE + wanted_panel_w() * WIDTH_PER_PANEL;
    let (w, h) = ((view_w - 16.0).clamp(0.0, wanted).floor(), (view_h - 16.0).clamp(0.0, HEIGHT).floor());
    [((view_w - w) / 2.0).floor(), ((view_h - h) / 2.0).floor(), w, h]
}

// Where the module list's and the settings' rows scroll, under their bands.
fn columns(s: &State) -> ([f32; 4], [f32; 4]) {
    let [x, y, w, h] = frame(s);
    let (top, room, rest) = (y + HEADER_H + BAND_H, (h - HEADER_H - BAND_H - PAD).max(0.0), (w - SIDE).max(0.0));
    let list = (rest * 0.42).clamp(LIST_MIN, LIST_MAX).min(rest).floor();
    ([x + SIDE, top, list, room], [x + SIDE + list, top, rest - list, room])
}

// The chosen module's switch.
fn toggle_rect(s: &State) -> [f32; 4] {
    let pane = columns(s).1;
    [pane[0] + pane[2] - CARD_M - TOGGLE_W, pane[1] - BAND_H + (BAND_H - TOGGLE_H) / 2.0, TOGGLE_W, TOGGLE_H]
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

// Where a module's name and bound key go in its row.
pub(super) fn module_text([x, y, w, h]: [f32; 4]) -> [f32; 4] {
    [x, y + 2.0, w - 4.0, h - 4.0]
}

// Whether a row has a line above it: only the settings after the first of a section.
pub(super) fn line_above(hit: Hit) -> bool {
    view(|v| v.lined.contains(&hit))
}

// The module whose settings show.
pub(super) fn chosen() -> Option<usize> {
    view(|v| v.chosen)
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
        let hidden = super::sunset_extra_modules::hidden(&s.modules);
        if v.listed.page != page || v.listed.query != menu.search || v.listed.count != s.modules.len() || v.listed.hidden != hidden {
            let query = norm(menu.search);
            v.listed.ids.clear();
            v.listed.ids.extend(s.modules.iter().enumerate().filter_map(|(i, m)| {
                let shown = (page == 0 || m.category == CATEGORIES[page - 1])
                    && !hidden.contains(&m.name)
                    && (norm(m.name).contains(&query) || norm(aliases(m.name)).contains(&query));
                shown.then_some(i)
            }));
            v.listed.page = page;
            v.listed.query.clear();
            v.listed.query.push_str(menu.search);
            v.listed.count = s.modules.len();
            v.listed.hidden = hidden;
        }
        // The first module listed, until another is chosen.
        if !v.chosen.is_some_and(|i| v.listed.ids.contains(&i)) {
            v.chosen = v.listed.ids.first().copied();
            v.pane.scroll = 0.0;
            smooth::jump(smooth::PANE);
        }
        let View { listed, list, pane, chosen, cards, lined, .. } = v;
        list.rows.clear();
        let mut y = 0.0;
        for &i in &listed.ids {
            list.rows.push((y, Hit::Module(i)));
            y += module_h();
        }
        list.content = y;
        list.stops.clone_from(&list.rows);
        // The bind first, then each section in its card (one with no title for a module without sections).
        pane.rows.clear();
        pane.stops.clear();
        cards.clear();
        lined.clear();
        let mut y = 0.0;
        if let Some(i) = *chosen {
            let m = &s.modules[i];
            let (bind, settings): (Vec<usize>, Vec<usize>) =
                (0..m.settings.len()).filter(|&j| m.setting_visible(j)).partition(|&j| m.settings[j].name == "Bind mode");
            for hit in std::iter::once(Hit::Bind(i)).chain(bind.into_iter().map(|j| Hit::Setting(i, j))) {
                pane.rows.push((y, hit));
                y += row_h(s, hit);
            }
            let mut section: Option<Option<&'static str>> = None;
            for j in settings {
                let hit = Hit::Setting(i, j);
                let group = m.group_of(j);
                if section != Some(group) {
                    if let Some(card) = cards.last_mut() {
                        y += CARD_PAD;
                        card.bottom = y;
                    }
                    y += CARD_GAP;
                    pane.stops.push((y, Hit::Dash(Part::Frame)));
                    cards.push(Card { top: y, bottom: y, title: group });
                    y += if group.is_some() { TITLE_H } else { CARD_PAD };
                    section = Some(group);
                } else {
                    lined.push(hit);
                }
                pane.rows.push((y, hit));
                pane.stops.push((y, hit));
                y += row_h(s, hit);
            }
            if let Some(card) = cards.last_mut() {
                y += CARD_PAD;
                card.bottom = y;
            }
            y += PAD;
        }
        pane.content = y;
        pane.stops.insert(0, (0.0, Hit::Dash(Part::Frame)));
        list.scroll = snap_scroll(&list.stops, list.content, room, list.scroll, 0);
        pane.scroll = snap_scroll(&pane.stops, pane.content, room, pane.scroll, 0);
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

// The rows in view, and where. One part way in view is drawn cut. The settings sit inside their cards.
pub(super) fn rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let (list, pane) = columns(s);
    let inset = CARD_M + CARD_IN;
    let pane = [pane[0] + inset, pane[1], (pane[2] - 2.0 * inset).max(0.0), pane[3]];
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
    if view(|v| v.chosen.is_some()) {
        out.push((toggle_rect(s), Hit::Dash(Part::Toggle)));
    }
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
        column.scroll = snap_scroll(&column.stops, column.content, list[3], column.scroll, by);
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

// Everything under the rows: the panel, the header with the search box, the categories, the module list's title and the
// mark of the chosen module, and the chosen module's band (name, description, switch) and sections.
pub(super) fn back(s: &State, game: &Game<'_>, hover: Option<Hit>, px: f32, out: &mut Vec<Shape>) {
    follow_hover(hover);
    let solid = 0xFF00_0000 | accent();
    let radius = panel_radius();
    let f = frame(s);
    let (list, pane) = columns(s);
    let menu = dash_menu(s);
    let page = if menu.search.is_empty() { view(|v| v.page) } else { 0 };
    let body = f[1] + HEADER_H;

    out.push(Shape::Squircle([f[0] - px, f[1] - px, f[2] + 2.0 * px, f[3] + 2.0 * px], radius + px, HAIRLINE));
    out.push(Shape::Squircle(f, radius, SURFACE));
    out.push(Shape::Fill([f[0], body, f[2], px], HAIRLINE));
    out.push(Shape::Fill([list[0], body, px, f[1] + f[3] - body], HAIRLINE));
    out.push(Shape::Fill([pane[0], body, px, f[1] + f[3] - body], HAIRLINE));

    // The header: the name, and the search box over the columns (typing anywhere goes to it).
    out.push(Shape::Head("Sunset".to_owned(), [f[0] + 14.0, f[1] + text_y(HEADER_H)], 0xFFFFFF));
    let r = [list[0] + CARD_M, f[1] + (HEADER_H - SEARCH_H) / 2.0, (list[2] + pane[2] - 2.0 * CARD_M).clamp(0.0, SEARCH_MAX), SEARCH_H];
    let selected = menu.select_all && !menu.search.is_empty();
    out.push(Shape::Round(r, radius, if selected { 0xFF30_60B0 } else { CONTROL }));
    let text = if menu.search.is_empty() { "Search modules".to_owned() } else { format!("{}_", menu.search) };
    let colour = if menu.search.is_empty() { MUTED } else { 0xFFFFFF };
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

    // The module list's title: the category (or the search) and how many modules it lists.
    let (title, count) = view(|v| (if page == 0 { "Search" } else { CATEGORIES[page - 1].label() }, v.listed.ids.len()));
    let band_y = list[1] - BAND_H;
    out.push(Shape::Head(title.to_owned(), [list[0] + 10.0, band_y + text_y(BAND_H)], 0xFFFFFF));
    let count = count.to_string();
    let tw = game.native_text_width(title) + 1.0;
    out.push(Shape::Text(count, [list[0] + 10.0 + tw + 6.0, band_y + text_y(BAND_H)], MUTED));

    // The list's rows: the chosen one lit (an enabled module's name is in the accent colour: draw_row).
    let scroll = smooth::shown(smooth::DASHBOARD, list_scroll());
    let chosen = view(|v| {
        let shown = visible(&v.list.rows, scroll, list[3]);
        out.push(Shape::ClipStart);
        for &(off, hit) in &v.list.rows[shown] {
            if let Hit::Module(i) = hit
                && v.chosen == Some(i)
            {
                out.push(Shape::Fill([list[0], list[1] + off - scroll, list[2], module_h()], 0x16FF_FFFF));
            }
        }
        out.push(Shape::ClipEnd([list[0], list[1], list[0] + list[2], list[1] + list[3]]));
        v.chosen
    });

    // The chosen module's band: its name and what it does, and its switch.
    if let Some(i) = chosen {
        let m = &s.modules[i];
        let t = toggle_rect(s);
        let room = (t[0] - pane[0] - CARD_M - 8.0).max(0.0);
        out.push(Shape::Head(fit_to(game, m.name, room), [pane[0] + CARD_M + 2.0, band_y + 5.0], 0xFFFFFF));
        out.push(Shape::Text(fit_to(game, m.about, room), [pane[0] + CARD_M + 2.0, band_y + 18.0], MUTED));
        let lit = glow(Hit::Dash(Part::Toggle));
        out.push(Shape::Round(t, TOGGLE_H / 2.0, if m.on { solid } else { 0xFF2E_2E33 + (lit << 16 | lit << 8 | lit) }));
        let knob = TOGGLE_H - 4.0;
        let kx = if m.on { t[0] + t[2] - knob - 2.0 } else { t[0] + 2.0 };
        out.push(Shape::Round([kx, t[1] + 2.0, knob, knob], knob / 2.0, if m.on { 0xFFFF_FFFF } else { 0xFFB4_B4BA }));
    }

    // Its sections: a card each, with its title.
    let pane_at = smooth::shown(smooth::PANE, pane_scroll());
    view(|v| {
        out.push(Shape::ClipStart);
        for card in &v.cards {
            let (top, bottom) = (pane[1] + card.top - pane_at, pane[1] + card.bottom - pane_at);
            if bottom < pane[1] || top > pane[1] + pane[3] {
                continue;
            }
            out.push(Shape::Round([pane[0] + CARD_M, top, pane[2] - 2.0 * CARD_M, bottom - top], control_radius().max(4.0), CARD));
            if let Some(title) = card.title {
                out.push(Shape::Text(title.to_owned(), [pane[0] + CARD_M + 7.0, top + 3.0], MUTED));
            }
        }
        out.push(Shape::ClipEnd([pane[0], pane[1], pane[0] + pane[2], pane[1] + pane[3]]));
    });
    let (content, pane_content) = view(|v| (v.list.content, v.pane.content));

    // The scroll markers: how much of a column shows, and where.
    for (c, content, at) in [(list, content, scroll), (pane, pane_content, pane_at)] {
        let room = c[3];
        if content > room && room > 0.0 {
            let thumb = (room * room / content).max(10.0);
            let t = (at / (content - room)).min(1.0);
            out.push(Shape::Round([c[0] + c[2] - 3.0, c[1] + (room - thumb) * t, 2.0, thumb], 1.0, 0x50FF_FFFF));
        }
    }
}

