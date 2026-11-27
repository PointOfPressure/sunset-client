// The Dashboard style of the click GUI: one solid panel with a header (the name and the search), the categories down the
// left, the modules of the one picked, and the settings of the module chosen, in its sections. Its rows are the menu's own,
// so draw_gui draws them and gui_press clicks them as ever; this file places them and draws around them. A left click on a
// module turns it on or off, a right click shows its settings; the switch over its settings turns it on or off too.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset::{CATEGORIES, Hit, Shape, State, accent, pal, control_radius, dash_menu, dash_search_clear, fade, fit_to, panel_radius, row_h, row_hh, scrollbar, search_text, snap_scroll, switch, text_y, wanted_panel_w};
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
    // The reworked style: the cross that empties the search, and the profile at the foot of the categories (it opens
    // the Profiles menu).
    Clear,
    Profile,
    // The Sidebar style's parts (sidebar.rs): the calls here go there while it is the style.
    Side(super::sunset_sidebar::Part),
}

// The modules a category and search list, kept until those change.
struct Listed {
    page: usize,
    query: String,
    count: usize,
    // The modules switched off in Modules.
    hidden: Vec<&'static str>,
    ids: Vec<usize>,
    // Beside each, the settings a search matched in it (listed under it, except in the Legacy style).
    settings: Vec<Vec<usize>>,
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
    // A module was just picked: during a search its settings scroll to the first one the search matched, which is lit
    // for a moment.
    focus: bool,
    flash: Option<(Hit, Instant)>,
    lit: [u32; 2],
    opened: Option<Instant>,
}

static VIEW: Mutex<View> = Mutex::new(View {
    page: 1,
    chosen: None,
    listed: Listed { page: usize::MAX, query: String::new(), count: 0, hidden: Vec::new(), ids: Vec::new(), settings: Vec::new() },
    list: Column { scroll: 0.0, rows: Vec::new(), stops: Vec::new(), content: 0.0 },
    pane: Column { scroll: 0.0, rows: Vec::new(), stops: Vec::new(), content: 0.0 },
    cards: Vec::new(),
    lined: Vec::new(),
    hover: None,
    left: None,
    focus: false,
    flash: None,
    lit: [0; 2],
    opened: None,
});

fn view<R>(f: impl FnOnce(&mut View) -> R) -> R {
    f(&mut VIEW.lock().unwrap_or_else(|e| e.into_inner()))
}

// ClickGUI's Style, as sync_flags last read it: a Dashboard, and whether it is the one from before the rework (Dashboard
// (Legacy): sliders under their names, a pick stepped through by clicks, no settings among a search's finds, no counts
// by the categories, and the window no bigger than at the default scale).
static ON: AtomicBool = AtomicBool::new(false);
static LEGACY: AtomicBool = AtomicBool::new(false);

pub(super) fn set_on(on: bool, legacy: bool) {
    ON.store(on, Ordering::Relaxed);
    LEGACY.store(legacy, Ordering::Relaxed);
}

fn legacy() -> bool {
    LEGACY.load(Ordering::Relaxed)
}

// Whether every setting is one line: its name in the left part of the row, a slider or a pick's options in the right
// (the reworked Dashboard; slider_track, number_band and segments lay them out).
pub(super) fn one_line() -> bool {
    ON.load(Ordering::Relaxed) && !legacy() && !sidebar()
}

// Whether the menu is a single window (this style or Sidebar): draw_gui and the input lay it out through here.
pub(super) fn on() -> bool {
    ON.load(Ordering::Relaxed) || super::sunset_sidebar::on()
}

fn sidebar() -> bool {
    super::sunset_sidebar::on()
}

// How wide the panel is for ClickGUI's Panel width: 400 at its least, 610 by default, 680 at its most.
const WIDTH_BASE: f32 = 190.0;
const WIDTH_PER_PANEL: f32 = 3.5;
const HEIGHT: f32 = 420.0;
const GROW: f32 = 1.33;
const SIDE: f32 = 104.0;
// Above and below the columns, so a lit row stays clear of the panel's corners.
const PAD: f32 = 8.0;
const LIST_MIN: f32 = 150.0;
const LIST_MAX: f32 = 270.0;
const SEARCH_H: f32 = 18.0;
const SEARCH_MAX: f32 = 300.0;
// The header across the top, and the band at the top of the module list and of the settings (their titles).
const HEADER_H: f32 = 28.0;
const BAND_H: f32 = 34.0;
// The reworked style's band: the module's name keeps its place whether its description takes one line or two.
const BAND_TALL: f32 = 46.0;

// Whether the module shown has a two-line description, as the last frame wrapped it (the band is then taller).
static TWO_LINES: AtomicBool = AtomicBool::new(false);

fn band_h() -> f32 {
    if legacy() || !TWO_LINES.load(Ordering::Relaxed) { BAND_H } else { BAND_TALL }
}
const TAB_H: f32 = 20.0;
const TAB_GAP: f32 = 2.0;
const TABS_Y: f32 = HEADER_H + 8.0;
// The chosen module's switch, at the right of its band.
// The chosen module's switch: the size of a setting's, and its right edge on theirs.
const TOGGLE_W: f32 = 14.0;
const TOGGLE_H: f32 = 8.0;
// The settings' sections: inset from the column's sides, apart from each other, the rows inside them, a title's height.
const CARD_M: f32 = 8.0;
const CARD_GAP: f32 = 6.0;
const CARD_PAD: f32 = 3.0;
const CARD_IN: f32 = 2.0;
const TITLE_H: f32 = 13.0;
// The reworked style's band: the name's line, and where its description starts.
const TITLE_Y: f32 = 5.0;
const ABOUT_Y: f32 = 17.0;

const MUTED: u32 = 0x8C8C94;
// A lit row or category at its brightest.
const LIT: u32 = 0x10;
const FADE: f32 = 0.08;
const TIP_AFTER: f32 = 0.5;
// The menu coming in.
const OPEN_TIME: f32 = 0.12;
// How long a search's matched setting stays lit.
const FLASH_TIME: f32 = 1.6;
const OPEN_RISE: f32 = 6.0;

fn frame(s: &State) -> [f32; 4] {
    let menu = dash_menu(s);
    frame_in(menu.view_w, menu.view_h)
}

// The screen width, in the menu's units, that gives the window the width it wants (None in the Sidebar style).
pub(super) fn wanted_width() -> Option<f32> {
    (!sidebar()).then(|| WIDTH_BASE + wanted_panel_w() * WIDTH_PER_PANEL + 17.0)
}

fn frame_in(view_w: f32, view_h: f32) -> [f32; 4] {
    let wanted = WIDTH_BASE + wanted_panel_w() * WIDTH_PER_PANEL;
    // Where the screen has more room than that (a small GUI scale), the window takes most of it, up to a third more.
    let (most_w, most_h) = if legacy() {
        (wanted, HEIGHT)
    } else {
        (wanted.max((view_w * 0.8).min(wanted * GROW)), HEIGHT.max((view_h * 0.82).min(HEIGHT * GROW)))
    };
    let (w, h) = ((view_w - 16.0).clamp(0.0, most_w).floor(), (view_h - 16.0).clamp(0.0, most_h).floor());
    [((view_w - w) / 2.0).floor(), ((view_h - h) / 2.0).floor(), w, h]
}

// Where the module list's and the settings' rows scroll, under their bands.
fn columns(s: &State) -> ([f32; 4], [f32; 4]) {
    let [x, y, w, h] = frame(s);
    let (top, room, rest) = (y + HEADER_H + band_h(), (h - HEADER_H - band_h() - PAD).max(0.0), (w - SIDE).max(0.0));
    let list = (rest * 0.42).clamp(LIST_MIN, LIST_MAX).min(rest).floor();
    ([x + SIDE, top, list, room], [x + SIDE + list, top, rest - list, room])
}

// The search box in the header: in the reworked style the same share of it at every scale.
fn search_rect(s: &State) -> [f32; 4] {
    let (f, (list, pane)) = (frame(s), columns(s));
    let w = if legacy() { (list[2] + pane[2] - 2.0 * CARD_M).clamp(0.0, SEARCH_MAX) } else { ((list[2] + pane[2]) * 0.5).floor() };
    [list[0] + CARD_M, f[1] + (HEADER_H - SEARCH_H) / 2.0, w, SEARCH_H]
}

// The cross at the search box's end.
fn clear_rect(s: &State) -> [f32; 4] {
    let r = search_rect(s);
    [r[0] + r[2] - SEARCH_H, r[1], SEARCH_H, SEARCH_H]
}

// The profile in use, at the foot of the categories: a button that opens the Profiles menu.
fn profile_rect(s: &State) -> [f32; 4] {
    let f = frame(s);
    [f[0] + 8.0, f[1] + f[3] - 34.0, SIDE - 16.0, 26.0]
}

// The chosen module's switch.
fn toggle_rect(s: &State) -> [f32; 4] {
    let pane = columns(s).1;
    let band = band_h();
    let y = if legacy() { pane[1] - band + (band - TOGGLE_H) / 2.0 } else { pane[1] - band + TITLE_Y + 0.5 };
    [pane[0] + pane[2] - CARD_M - CARD_IN - super::sunset::RIGHT_PAD - TOGGLE_W, y, TOGGLE_W, TOGGLE_H]
}

fn module_h() -> f32 {
    row_hh() + 4.0
}

fn height(s: &State, hit: Hit) -> f32 {
    match hit {
        Hit::Module(_) => module_h(),
        Hit::Setting(..) if one_line() => row_hh(),
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
    if sidebar() {
        return super::sunset_sidebar::line_above(hit);
    }
    view(|v| v.lined.contains(&hit))
}

// The module whose settings show.
pub(super) fn chosen() -> Option<usize> {
    view(|v| v.chosen)
}

// Once a frame, before anything reads the columns; a column that got shorter comes back to its end.
pub(super) fn prepare(s: &State) {
    if sidebar() {
        super::sunset_sidebar::prepare(s);
        return;
    }
    if !on() {
        return;
    }
    let menu = dash_menu(s);
    let (room, pane_room) = (columns(s).0[3], columns(s).1[3]);
    view(|v| {
        let page = if menu.search.is_empty() { v.page } else { 0 };
        let hidden = super::sunset_extra_modules::hidden(&s.modules);
        if v.listed.page != page || v.listed.query != menu.search || v.listed.count != s.modules.len() || v.listed.hidden != hidden {
            // Searching: what it finds, best first (search.rs); else the page's modules A to Z.
            let shown = s.modules.iter().enumerate().filter(|(_, m)| (page == 0 || m.category == CATEGORIES[page - 1]) && m.in_gui && !hidden.contains(&m.name)).map(|(i, _)| i);
            let found = (!menu.search.trim().is_empty()).then(|| super::sunset_search::results(&s.modules, menu.search));
            let ids = match &found {
                Some(found) => super::sunset_search::ranked(found, shown),
                None => super::sunset::alphabetical(&s.modules, shown),
            };
            v.listed.settings = ids.iter().map(|&i| found.as_ref().and_then(|f| f[i].as_ref()).map_or_else(Vec::new, |hit| hit.settings.clone())).collect();
            // A new search shows its best find.
            if v.listed.query != menu.search && !menu.search.trim().is_empty() {
                v.chosen = None;
            }
            // A search ended: the settings it scrolled to its find go back to their top.
            if menu.search.trim().is_empty() && !v.listed.query.trim().is_empty() {
                v.pane.scroll = 0.0;
                smooth::jump(smooth::PANE);
            }
            v.listed.ids.clear();
            v.listed.ids.extend(ids);
            v.listed.page = page;
            v.listed.query.clear();
            v.listed.query.push_str(menu.search);
            v.listed.count = s.modules.len();
            v.listed.hidden = hidden;
        }
        // The first module listed, until another is chosen.
        // (A search that finds nothing keeps the module shown before it, in the reworked style.)
        let keep = !legacy() && v.listed.ids.is_empty() && v.chosen.is_some();
        if !keep && !v.chosen.is_some_and(|i| v.listed.ids.contains(&i)) {
            v.chosen = v.listed.ids.first().copied();
            v.pane.scroll = 0.0;
            v.focus = true;
            smooth::jump(smooth::PANE);
        }
        let View { listed, list, pane, chosen, cards, lined, focus, flash, .. } = v;
        list.rows.clear();
        let mut y = 0.0;
        // Under each module a search found, the settings it matched there, to change in place as in the panels.
        for (k, &i) in listed.ids.iter().enumerate() {
            list.rows.push((y, Hit::Module(i)));
            y += module_h();
            if legacy() {
                continue;
            }
            for &j in listed.settings.get(k).into_iter().flatten().filter(|&&j| s.modules[i].setting_visible(j)) {
                list.rows.push((y, Hit::Setting(i, j)));
                y += module_h();
            }
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
            // The bind has a card of its own, as every section has.
            y += CARD_PAD;
            for (n, hit) in std::iter::once(Hit::Bind(i)).chain(bind.into_iter().map(|j| Hit::Setting(i, j))).enumerate() {
                if n > 0 && legacy() {
                    lined.push(hit);
                }
                pane.rows.push((y, hit));
                y += height(s, hit);
            }
            y += CARD_PAD;
            cards.push(Card { top: 0.0, bottom: y, title: None });
            let mut section: Option<Option<&'static str>> = None;
            for j in settings {
                let hit = Hit::Setting(i, j);
                let group = m.group_of(j);
                if section != Some(group) {
                    if section.is_some()
                        && let Some(card) = cards.last_mut()
                    {
                        y += CARD_PAD;
                        card.bottom = y;
                    }
                    y += CARD_GAP;
                    pane.stops.push((y, Hit::Dash(Part::Frame)));
                    cards.push(Card { top: y, bottom: y, title: group });
                    y += if group.is_some() { TITLE_H } else { CARD_PAD };
                    section = Some(group);
                } else if legacy() {
                    // (The reworked style draws no line between a section's settings: its card holds them, and the
                    // same setting listed under a search's find would get the line too.)
                    lined.push(hit);
                }
                pane.rows.push((y, hit));
                pane.stops.push((y, hit));
                y += height(s, hit);
            }
            if section.is_some()
                && let Some(card) = cards.last_mut()
            {
                y += CARD_PAD;
                card.bottom = y;
            }
            y += PAD;
        }
        pane.content = y;
        // Picked during a search: its settings start at the first the search matched in it.
        if std::mem::take(focus)
            && !legacy()
            && !menu.search.trim().is_empty()
            && let Some(i) = *chosen
            && let Some(&j) = listed.ids.iter().position(|&x| x == i).and_then(|k| listed.settings.get(k)).and_then(|found| found.first())
            && let Some(&(off, _)) = pane.rows.iter().find(|(_, hit)| *hit == Hit::Setting(i, j))
        {
            pane.scroll = off;
            *flash = Some((Hit::Setting(i, j), Instant::now()));
            smooth::jump(smooth::PANE);
        }
        pane.stops.insert(0, (0.0, Hit::Dash(Part::Frame)));
        list.scroll = snap_scroll(&list.stops, list.content, room, list.scroll, 0);
        pane.scroll = snap_scroll(&pane.stops, pane.content, pane_room, pane.scroll, 0);
    });
}

pub(super) fn list_scroll() -> f32 {
    if sidebar() {
        return 0.0;
    }
    view(|v| v.list.scroll)
}

pub(super) fn pane_scroll() -> f32 {
    if sidebar() {
        return super::sunset_sidebar::scroll();
    }
    view(|v| v.pane.scroll)
}

// The top and bottom of both columns, in this style.
pub(super) fn list_y(s: &State) -> Option<[f32; 2]> {
    if sidebar() {
        return Some(super::sunset_sidebar::list_y(s));
    }
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

// The module list's rows: clear of its scroll marker on the right; a search's settings set in under their module, in
// the middle of a module's height.
fn list_rows(s: &State, column: &Column, [x, y, w, room]: [f32; 4], scroll: f32, out: &mut Vec<([f32; 4], Hit)>) {
    let w = w - 4.0;
    out.extend(column.rows[visible(&column.rows, scroll, room)].iter().map(|&(off, hit)| match hit {
        // Their right edge on the module rows' (module_text takes 4 off those).
        Hit::Setting(..) => ([x + SET_IN, y + off - scroll + ((module_h() - row_hh()) / 2.0).floor(), w - SET_IN - 4.0, row_hh()], hit),
        _ => ([x, y + off - scroll, w, height(s, hit)], hit),
    }));
}
const SET_IN: f32 = 10.0;

// The rows in view, and where. One part way in view is drawn cut. The settings sit inside their cards.
pub(super) fn rows(s: &State) -> Vec<([f32; 4], Hit)> {
    if sidebar() {
        return super::sunset_sidebar::rows(s);
    }
    let (list, pane) = columns(s);
    let inset = CARD_M + CARD_IN;
    let pane = [pane[0] + inset, pane[1], (pane[2] - 2.0 * inset).max(0.0), pane[3]];
    let (list_at, pane_at) = view(|v| (v.list.scroll, v.pane.scroll));
    let (list_at, pane_at) = (smooth::shown(smooth::DASHBOARD, list_at), smooth::shown(smooth::PANE, pane_at));
    let mut out = Vec::new();
    view(|v| {
        if legacy() {
            column_rows(s, &v.list, list, list_at, &mut out);
        } else {
            list_rows(s, &v.list, list, list_at, &mut out);
        }
        column_rows(s, &v.pane, pane, pane_at, &mut out);
    });
    out
}

// What the pointer can hit around the rows.
pub(super) fn hits(s: &State) -> Vec<([f32; 4], Hit)> {
    if sidebar() {
        return super::sunset_sidebar::hits(s);
    }
    if !on() {
        return Vec::new();
    }
    let mut out = vec![(frame(s), Hit::Dash(Part::Frame))];
    out.extend((1..=CATEGORIES.len()).map(|p| (tab_rect(s, p), Hit::Dash(Part::Page(p)))));
    if view(|v| v.chosen.is_some()) {
        out.push((toggle_rect(s), Hit::Dash(Part::Toggle)));
    }
    if !legacy() {
        if !dash_menu(s).search.is_empty() {
            out.push((clear_rect(s), Hit::Dash(Part::Clear)));
        }
        out.push((profile_rect(s), Hit::Dash(Part::Profile)));
    }
    out
}

pub(super) fn press(s: &mut State, part: Part, right: bool) {
    if let Part::Side(p) = part {
        super::sunset_sidebar::press(s, p);
        return;
    }
    if let (Part::Clear, false) = (part, right) {
        dash_search_clear(s);
        return;
    }
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
            v.focus = true;
            smooth::jump(smooth::PANE);
        }
    });
}

pub(super) fn wheel(s: &State, steps: f32, at: (f32, f32)) {
    if sidebar() {
        super::sunset_sidebar::wheel(s, steps, at);
        return;
    }
    let (list, pane) = columns(s);
    let by = -steps * 2.0;
    let by = if by > 0.0 { by.ceil() } else { by.floor() } as i32;
    let over_pane = super::sunset::inside(pane, at);
    view(|v| {
        let (column, room) = if over_pane { (&mut v.pane, pane[3]) } else { (&mut v.list, list[3]) };
        column.scroll = snap_scroll(&column.stops, column.content, room, column.scroll, by);
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

pub(super) fn follow_hover(hover: Option<Hit>) {
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

// The profile in use, as the HUD's watermark names it.
fn with_profile(s: &State) -> String {
    s.selection.label()
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
    if sidebar() {
        super::sunset_sidebar::back(s, game, hover, px, out);
        return;
    }
    follow_hover(hover);
    let solid = 0xFF00_0000 | accent();
    let radius = panel_radius();
    let f = frame(s);
    let (list, pane) = columns(s);
    let menu = dash_menu(s);
    let page = if menu.search.is_empty() { view(|v| v.page) } else { 0 };
    let body = f[1] + HEADER_H;

    out.push(Shape::Squircle([f[0] - px, f[1] - px, f[2] + 2.0 * px, f[3] + 2.0 * px], radius + px, pal().dash_hairline));
    out.push(Shape::Squircle(f, radius, pal().dash_surface));
    out.push(Shape::Fill([f[0], body, f[2], px], pal().dash_hairline));
    out.push(Shape::Fill([list[0], body, px, f[1] + f[3] - body], pal().dash_hairline));
    out.push(Shape::Fill([pane[0], body, px, f[1] + f[3] - body], pal().dash_hairline));
    // The reworked style: a line under the module's band, which its settings scroll up to (none over a pane with
    // nothing in it: a search that found nothing, with no module shown).
    if !legacy() && !view(|v| v.chosen.is_none() && v.listed.ids.is_empty()) {
        out.push(Shape::Fill([pane[0], pane[1] - px, pane[2], px], pal().dash_hairline));
    }

    // The header: the name, and the search box over the columns (typing anywhere goes to it).
    out.push(Shape::Head("Sunset".to_owned(), [f[0] + 14.0, f[1] + text_y(HEADER_H)], 0xFFFFFF));
    if !legacy() {
        // The version beside the name, and the profile in use at the foot of the categories (the HUD's watermark,
        // which says both, is under the window).
        let nw = game.sunset_text_width("Sunset") + 1.0;
        out.push(Shape::Text(super::sunset::VERSION.to_owned(), [f[0] + 14.0 + nw + 5.0, f[1] + text_y(HEADER_H)], MUTED));
        // The profile: a button (a click opens the Profiles menu), its text in line with the categories' names.
        let b = profile_rect(s);
        let lit = glow(Hit::Dash(Part::Profile));
        out.push(Shape::Round(b, control_radius().max(3.0), pal().dash_control));
        if lit > 0 {
            out.push(Shape::Round(b, control_radius().max(3.0), lit << 24 | 0x00FF_FFFF));
        }
        let profile = fit_to(game, &with_profile(s), b[2] - 22.0);
        out.push(Shape::Text("Profile".to_owned(), [b[0] + 10.0, b[1] + 3.0], MUTED));
        out.push(Shape::Text(profile, [b[0] + 10.0, b[1] + 14.0], 0xE4E4E4));
        out.push(Shape::Text(">".to_owned(), [b[0] + b[2] - 10.0, b[1] + text_y(b[3])], MUTED));
    }
    // The search box, the hints after it.
    let r = search_rect(s);
    let selected = menu.select_all && !menu.search.is_empty();
    out.push(Shape::Round(r, radius, if selected { 0xFF30_60B0 } else { pal().dash_control }));
    // No caret while a bind waits for its key or the Profiles menu is open: typing does not go to the box then.
    let (binding, menu_open) = (menu.binding, menu.menu_open);
    let empty = if legacy() { "Search modules" } else { "Search modules and settings" };
    // The reworked style: a cross at its end while it holds a search (a click empties it).
    let text_r = if legacy() || menu.search.is_empty() { r } else { [r[0], r[1], r[2] - SEARCH_H + 4.0, r[3]] };
    search_text(game, out, text_r, menu.search, if binding || menu_open { usize::MAX } else { menu.search.len() }, empty);
    if !legacy() && !menu.search.is_empty() {
        super::sunset::cross(out, clear_rect(s), hover == Some(Hit::Dash(Part::Clear)));
    }
    // What the clicks and keys do: on a strip over the window as over the panels, where the screen has the room
    // above it; else beside the search box, as many of the hints (the first matter most) as fit there whole. The
    // reworked style keeps them in the header at every scale; "Enter" only while a search lists something, and while a
    // bind waits only Esc and Backspace (its row says to press a key).
    let finds = view(|v| !v.listed.ids.is_empty());
    let searching = !menu.search.trim().is_empty();
    let enter = legacy() || super::sunset::first_match(s).is_some();
    let hint = super::sunset::gui_hint(binding, menu_open, searching && enter);
    let hint = match (legacy(), searching && !finds && !binding && !menu_open) {
        (true, _) => hint.to_owned(),
        (false, true) => "Esc: clear the search".to_owned(),
        // Enter names what it will do; while searching, Esc empties the search first.
        (false, false) => {
            let hint = hint.strip_prefix("Press a key or a mouse button   ").unwrap_or(hint);
            let named = super::sunset::first_match(s).map(|i| format!("Enter: turn {} {}", s.modules[i].name, if s.modules[i].on { "off" } else { "on" }));
            let hint = match named {
                Some(enter) if searching => hint.replace("Enter: toggle the first result", &enter),
                _ => hint.to_owned(),
            };
            if searching { hint.replace("Esc: close", "Esc: clear the search") } else { hint }
        }
    };
    if f[1] >= 15.0 && legacy() {
        let hint = fit_to(game, &hint, menu.view_w - 16.0);
        let hw = game.sunset_text_width(&hint);
        let strip = [((menu.view_w - hw) / 2.0 - 6.0).floor(), f[1] - 14.0, hw + 12.0, 12.0];
        out.push(Shape::Round(strip, radius, pal().dash_surface));
        out.push(Shape::Text(hint, [strip[0] + 6.0, strip[1] + text_y(12.0)], 0xB4B4B4));
    } else {
        let (from, to) = (r[0] + r[2] + 10.0, f[0] + f[2] - 12.0);
        let parts: Vec<&str> = hint.split("   ").collect();
        // The reworked style keeps the last hint (how to close or cancel) when the middle ones have to go.
        let shown = (1..=parts.len())
            .rev()
            .map(|n| {
                let mut kept = parts[..n].to_vec();
                if n < parts.len() && !legacy() {
                    kept.extend(parts.last());
                }
                kept.join("   ")
            })
            .find(|text| game.sunset_text_width(text) + if legacy() { 0.0 } else { 6.0 * text.matches("   ").count() as f32 } <= to - from);
        if let Some(text) = shown {
            if legacy() {
                let tw = game.sunset_text_width(&text);
                out.push(Shape::Text(text, [to - tw, f[1] + text_y(HEADER_H)], MUTED));
            } else {
                // Each hint on its own, a dot between them, so they do not run together into one sentence.
                let parts: Vec<&str> = text.split("   ").collect();
                let mut x = to;
                for (n, part) in parts.iter().rev().enumerate() {
                    if n > 0 {
                        x -= 6.0;
                        out.push(Shape::Round([x - 1.0, f[1] + (HEADER_H / 2.0).floor() - 1.0, 2.0, 2.0], 1.0, 0xFF5A_5A62));
                        x -= 6.0;
                    }
                    let tw = game.sunset_text_width(part);
                    x -= tw;
                    out.push(Shape::Text((*part).to_owned(), [x, f[1] + text_y(HEADER_H)], MUTED));
                }
            }
        }
    }

    // The categories: the one shown a capsule in the accent colour, and (not in the Legacy style) how many of each are
    // on, of how many it lists.
    let hidden = if legacy() { Vec::new() } else { super::sunset_extra_modules::hidden(&s.modules) };
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
        let (label, at) = (CATEGORIES[p - 1].label().to_owned(), [r[0] + 10.0, r[1] + text_y(r[3])]);
        // On the accent capsule the menu's text shadow would show (it is lost on the dark surface everywhere else).
        out.push(if p == page { Shape::Plain(label, at, 0xFFFFFF, true) } else { Shape::Head(label, at, 0xD4D4D8) });
        if !legacy() {
            let listed = s.modules.iter().filter(|m| m.category == CATEGORIES[p - 1] && m.in_gui && !hidden.contains(&m.name));
            // Nothing where none is on.
            let on = listed.filter(|m| m.on).count();
            if on > 0 {
                let count = format!("{on} on");
                let at = [r[0] + r[2] - 8.0 - game.sunset_text_width(&count), r[1] + text_y(r[3])];
                out.push(if p == page { Shape::Plain(count, at, 0xFFE8DC, true) } else { Shape::Text(count, at, MUTED) });
            }
        }
    }

    // The module list's title: the category (or the search) and how many modules it lists.
    let (title, count) = view(|v| (if page == 0 { "Search" } else { CATEGORIES[page - 1].label() }, v.listed.ids.len()));
    let band_y = list[1] - band_h();
    // The reworked style: on the module title's line, at the rows' names' inset.
    let (title_x, title_y) = if legacy() { (list[0] + 10.0, band_y + text_y(BAND_H)) } else { (list[0] + super::sunset::indent().max(6.0), band_y + TITLE_Y) };
    out.push(Shape::Head(title.to_owned(), [title_x, title_y], 0xFFFFFF));
    // What the number counts: the category's modules, or the search's finds.
    let on = view(|v| v.listed.ids.iter().filter(|&&i| s.modules[i].on).count());
    let settings = view(|v| v.list.rows.iter().filter(|(_, hit)| matches!(hit, Hit::Setting(..))).count());
    let count = match (page, count) {
        // The reworked style: what a search found in one form, and a category's size (its count of those on is by
        // its name on the left).
        (_, n) if !legacy() => {
            let modules = format!("{n} {}", if n == 1 { "module" } else { "modules" });
            if settings > 0 { format!("{modules}, {settings} {}", if settings == 1 { "setting" } else { "settings" }) } else { modules }
        }
        (0, n) => format!("{n} found"),
        (_, 1) => format!("1 module, {on} on"),
        (_, n) => format!("{n} modules, {on} on"),
    };
    // A search that finds nothing says so in the list.
    if page == 0 && view(|v| v.listed.ids.is_empty()) {
        let none = format!("Nothing matches \"{}\"", menu.search.trim());
        let x = if legacy() { list[0] + 10.0 } else { list[0] + super::sunset::indent().max(6.0) };
        out.push(Shape::Text(fit_to(game, &none, list[2] + pane[2] - 24.0), [x, list[1] + 6.0], MUTED));
    }
    let tw = game.sunset_text_width(title) + 1.0;
    // A search that found nothing says so below; the old style gives no count then. The reworked style: the count
    // under the title (as the module's description is under its name; "0 modules" for a search that found nothing,
    // so the title never stands alone), and a line under the band as in the settings.
    if !legacy() || page != 0 || view(|v| !v.listed.ids.is_empty()) {
        if legacy() {
            out.push(Shape::Text(fit_to(game, &count, (list[2] - 20.0 - tw - 6.0).max(0.0)), [title_x + tw + 6.0, title_y], MUTED));
        } else {
            out.push(Shape::Text(fit_to(game, &count, (list[2] - 16.0).max(0.0)), [title_x, band_y + ABOUT_Y], MUTED));
        }
    }
    if !legacy() {
        out.push(Shape::Fill([list[0], list[1] - px, list[2], px], pal().dash_hairline));
    }

    // The list's rows: the chosen one lit (an enabled module's name is in the accent colour: draw_row).
    let scroll = smooth::shown(smooth::DASHBOARD, list_scroll());
    let chosen = view(|v| {
        let shown = visible(&v.list.rows, scroll, list[3]);
        out.push(Shape::ClipStart);
        for &(off, hit) in &v.list.rows[shown] {
            let Hit::Module(i) = hit else { continue };
            let y = list[1] + off - scroll;
            if v.chosen == Some(i) {
                // The chosen module: lit, with the light bar the panels put by a module whose settings are open (the
                // accent colour is for what is on), so a row only pointed at never looks the same.
                // (The reworked style: a light an off switch still shows on; its name is in bold, draw_row.)
                let (fill, bar) = if legacy() { (0x22FF_FFFF, 2.0) } else { (0x2CFF_FFFF, 3.0) };
                out.push(Shape::Fill([list[0], y, list[2] - if legacy() { 0.0 } else { 4.0 }, module_h()], fill));
                out.push(Shape::Fill([list[0], y, bar, module_h()], 0xFFE4_E4E4));
            }
            // A search lists modules of every category: each row says which, where its name leaves the room.
            if page == 0 {
                let m = &s.modules[i];
                let (label, name_w) = (m.category.label(), game.sunset_text_width(m.name));
                let lw = game.sunset_text_width(label);
                // Clear of the bound key, and of the row's switch in the reworked style.
                let key_w = m.key.as_deref().map_or(0.0, |_| 34.0) + if legacy() { 0.0 } else { super::sunset::MODULE_SWITCH + 8.0 };
                if super::sunset::indent().max(6.0) + name_w + 10.0 + lw + key_w + 10.0 <= list[2] {
                    let colour = if v.chosen == Some(i) { 0xA4A4AC } else { 0x6C6C74 };
                    out.push(Shape::Text(label.to_owned(), [list[0] + list[2] - 10.0 - key_w - lw, y + text_y(module_h())], colour));
                }
            }
        }
        out.push(Shape::ClipEnd([list[0], list[1], list[0] + list[2], list[1] + list[3]]));
        v.chosen
    });

    // The chosen module's band: its name and what it does (two lines of it, the second cut where it is longer), and
    // its switch.
    if let Some(i) = chosen {
        let m = &s.modules[i];
        let t = toggle_rect(s);
        let room = (t[0] - pane[0] - CARD_M - 8.0).max(0.0);
        let mut about = if room >= 40.0 { super::sunset::wrap(game, m.about, room) } else { Vec::new() };
        if about.len() > 2 {
            let rest = about.split_off(1).join(" ");
            about.push(fit_to(game, &rest, room));
        }
        let two = about.len() > 1;
        TWO_LINES.store(two, Ordering::Relaxed);
        // The reworked style's band is tall enough for two lines: the name stays put.
        let (name_y, about_y) = match (legacy(), two) {
            (false, _) => (TITLE_Y, ABOUT_Y),
            (true, true) => (2.0, 13.0),
            (true, false) => (5.0, 18.0),
        };
        // The reworked style: at the settings' names' inset.
        let tx = if legacy() { pane[0] + CARD_M + 2.0 } else { pane[0] + CARD_M + CARD_IN + super::sunset::indent() + 4.0 };
        out.push(Shape::Head(fit_to(game, m.name, room), [tx, band_y + name_y], 0xFFFFFF));
        for (n, line) in about.into_iter().enumerate() {
            out.push(Shape::Text(line, [tx, band_y + about_y + n as f32 * 10.0], MUTED));
        }
        let lit = glow(Hit::Dash(Part::Toggle));
        // The same switch as a setting's.
        switch(out, t, m.on, lit);
    } else if pane[2] > 60.0 {
        // No module chosen (a new search, one that finds nothing): the pane says how one gets there.
        // A search with no finds (and no module shown before it) says what brings the modules back.
        let nothing = !legacy() && page == 0 && view(|v| v.listed.ids.is_empty());
        if !nothing {
            let text = fit_to(game, "Right click a module to see its settings here", pane[2] - 2.0 * CARD_M - 4.0);
            out.push(Shape::Text(text, [pane[0] + CARD_M + 2.0, pane[1] + 6.0], MUTED));
        }
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
            out.push(Shape::Round([pane[0] + CARD_M, top, pane[2] - 2.0 * CARD_M, bottom - top], control_radius().max(4.0), pal().dash_card));
            if let Some(title) = card.title {
                // In capitals in the reworked style, so a section's name does not read as a setting.
                let title = if legacy() { title.to_owned() } else { title.to_uppercase() };
                out.push(Shape::Text(title, [pane[0] + CARD_M + CARD_IN + super::sunset::indent() + 4.0, top + 3.0], MUTED));
            }
        }
        // The setting a search opened its module at, lit for a moment.
        if let Some((hit, since)) = v.flash {
            let t = since.elapsed().as_secs_f32() / FLASH_TIME;
            match v.pane.rows.iter().find(|(_, h)| *h == hit) {
                Some(&(off, _)) if t < 1.0 => {
                    let alpha = (0x40 as f32 * (1.0 - t * t)) as u32;
                    let y = pane[1] + off - pane_at;
                    out.push(Shape::Fill([pane[0] + CARD_M, y, pane[2] - 2.0 * CARD_M, row_hh()], alpha << 24 | (accent() & 0x00FF_FFFF)));
                }
                _ => v.flash = None,
            }
        }
        // A module with nothing but its bind says so under it.
        if !legacy() && v.chosen.is_some() && v.cards.len() == 1 {
            let top = pane[1] + v.cards[0].bottom - pane_at + CARD_GAP + 4.0;
            out.push(Shape::Text("Nothing else to set".to_owned(), [pane[0] + CARD_M + CARD_IN + super::sunset::indent() + 4.0, top], MUTED));
        }
        out.push(Shape::ClipEnd([pane[0], pane[1], pane[0] + pane[2], pane[1] + pane[3]]));
    });
}

// Over the rows: where a column has more past its top or bottom edge the rows fade into the surface there (a row the
// edge cuts reads as "more this way", not as a broken row), and each column's scroll marker on its track.
pub(super) fn front(s: &State, px: f32, out: &mut Vec<Shape>) {
    if sidebar() || !on() {
        return;
    }
    let (list, pane) = columns(s);
    let (list_at, pane_at) = (smooth::shown(smooth::DASHBOARD, list_scroll()), smooth::shown(smooth::PANE, pane_scroll()));
    let (content, pane_content) = view(|v| (v.list.content, v.pane.content));
    for (c, content, at) in [(list, content, list_at), (pane, pane_content, pane_at)] {
        let room = c[3];
        if room <= 0.0 {
            continue;
        }
        // Clear of the hairline down the column's left.
        let (x, w) = (c[0] + px, c[2] - px);
        if at > 0.5 {
            fade(out, [x, c[1], w], pal().dash_surface, px, false);
        }
        if content - at > room + 0.5 {
            fade(out, [x, c[1] + room, w], pal().dash_surface, px, true);
        }
        // In the reworked style the settings' marker sits in the margin beside the cards, off the window's edge.
        let inset = if legacy() || c[0] == list[0] { 4.0 } else { 6.0 };
        scrollbar(out, c[0] + c[2] - inset, c[1] + 1.0, room - 2.0, content, room, at);
    }
}

