// The Dashboard style of the click GUI: one solid panel with a header (the name and the search), the categories down the
// left, the modules of the one picked, and the settings of the module chosen, in its sections. Its rows are the menu's own,
// so draw_gui draws them and gui_press clicks them as ever; this file places them and draws around them. A click on a
// module shows its settings and the switch at its row's end turns it on or off (so does the one over its settings);
// Enabled, over the categories, lists every module that is on; the arrow keys move through the list and the views,
// Enter and Space switch the module shown; a section of settings folds at a click on its title, which stays at the
// settings' top while they scroll under it. (Dashboard (Legacy) keeps the panels' clicks: left toggles, right shows.)
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
    // A section's title over the chosen module's settings (by its card): a click folds it or opens it again.
    Section(usize),
    // Under the bind: every section shut, or open again.
    FoldAll,
    // The shown module's name and description (the pointer on a description cut short shows all of it), and the mark at
    // the list's edge for a shown module scrolled out of it (a click brings its row back).
    About,
    Reveal,
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
    // Beside each, the settings a search matched in it (named on its row; its settings open at the first).
    settings: Vec<Vec<usize>>,
    // The search found its modules only by their descriptions.
    about: bool,
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
    // Shut at a click on its title: its settings are not listed, only how many they are.
    folded: bool,
    count: usize,
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
    // The sections shut, by module and title, until the game closes.
    folded: Vec<(&'static str, &'static str)>,
    // Titles in the list, by their offset in it: Enabled's categories, each over its modules (a click turns to that
    // category: its page), and a search's "In settings" over the modules found by a setting, not by their name.
    heads: Vec<(f32, &'static str, Option<usize>)>,
    // Where "Fold all" is in the settings (a module with two sections or more), and whether the description over
    // them was cut short as last drawn.
    fold_at: Option<f32>,
    cut: bool,
}

static VIEW: Mutex<View> = Mutex::new(View {
    page: 1,
    chosen: None,
    listed: Listed { page: usize::MAX, query: String::new(), count: 0, hidden: Vec::new(), ids: Vec::new(), settings: Vec::new(), about: false },
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
    folded: Vec::new(),
    heads: Vec::new(),
    fold_at: None,
    cut: false,
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
// The reworked style: a slimmer column of categories, and a module list no wider than its names, switches and keys
// need, so the settings get the room.
const SIDE_SLIM: f32 = 96.0;
const SLIM_MIN: f32 = 132.0;
const SLIM_MAX: f32 = 184.0;
// The view over the categories that lists every module that is on (the page after the categories' own; 0 is a search).
const ENABLED: usize = CATEGORIES.len() + 1;
// Between Enabled and the categories under it.
const ENABLED_GAP: f32 = 5.0;
// The reworked style's module list starts under one line (its title and count), higher than the settings do.
const LIST_HEAD: f32 = 22.0;
// The room kept for the key hints beside the search box, and the line "Fold all" takes over a module's sections.
const HINTS_W: f32 = 176.0;
const FOLD_H: f32 = 12.0;
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

// (Always that tall there: a click on a module shows it, and the rows must not move under the pointer as its
// description takes one line or two.)
fn band_h() -> f32 {
    if legacy() { BAND_H } else { BAND_TALL }
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
const LIT: u32 = 0x1C;
const FADE: f32 = 0.08;
const TIP_AFTER: f32 = 0.5;
// The menu coming in.
const OPEN_TIME: f32 = 0.12;
// How long a search's matched setting stays lit.
const FLASH_TIME: f32 = 2.4;
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
    let (top, room, rest) = (y + HEADER_H + band_h(), (h - HEADER_H - band_h() - PAD).max(0.0), (w - side()).max(0.0));
    let list = if legacy() { (rest * 0.42).clamp(LIST_MIN, LIST_MAX) } else { (rest * 0.32).clamp(SLIM_MIN, SLIM_MAX) };
    let list = list.min(rest).floor();
    let rise = if legacy() { 0.0 } else { band_h() - LIST_HEAD };
    ([x + side(), top - rise, list, room + rise], [x + side() + list, top, rest - list, room])
}

fn side() -> f32 {
    if legacy() { SIDE } else { SIDE_SLIM }
}

// The views down the left, top to bottom, and what each is called.
fn pages() -> impl Iterator<Item = usize> {
    (!legacy()).then_some(ENABLED).into_iter().chain(1..=CATEGORIES.len())
}

fn page_label(page: usize) -> &'static str {
    CATEGORIES.get(page.wrapping_sub(1)).map_or("Enabled", |c| c.label())
}

// The search box in the header: in the reworked style the same share of it at every scale.
fn search_rect(s: &State) -> [f32; 4] {
    let (f, (list, pane)) = (frame(s), columns(s));
    // (The reworked style: half the columns' width, less where the key hints beside it need the room.)
    let both = list[2] + pane[2];
    let w = if legacy() { (both - 2.0 * CARD_M).clamp(0.0, SEARCH_MAX) } else { (both * 0.5).floor().min((both - 30.0 - HINTS_W).max(110.0)) };
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
    [f[0] + 8.0, f[1] + f[3] - 34.0, side() - 16.0, 26.0]
}

// The chosen module's name and description, left of its switch.
fn about_rect(s: &State) -> [f32; 4] {
    let (pane, t) = (columns(s).1, toggle_rect(s));
    [pane[0] + CARD_M, pane[1] - band_h(), (t[0] - 8.0 - pane[0] - CARD_M).max(0.0), band_h()]
}
const FOLD_W: f32 = 70.0;

// The chosen module's switch.
fn toggle_rect(s: &State) -> [f32; 4] {
    let pane = columns(s).1;
    let band = band_h();
    let y = if legacy() { pane[1] - band + (band - TOGGLE_H) / 2.0 } else { pane[1] - band + TITLE_Y + 0.5 };
    [pane[0] + pane[2] - CARD_M - CARD_IN - super::sunset::RIGHT_PAD - TOGGLE_W, y, TOGGLE_W, TOGGLE_H]
}

fn module_h() -> f32 {
    row_hh() + if legacy() { 4.0 } else { 2.0 }
}

fn height(s: &State, hit: Hit) -> f32 {
    match hit {
        Hit::Module(_) => module_h(),
        Hit::Setting(..) if one_line() => row_hh(),
        _ => row_h(s, hit),
    }
}

// A category's place, for pages 1 and up. The reworked style has Enabled over them, a little apart, and makes them
// all shorter where a small window has no room for them over the profile.
fn tab_rect(s: &State, page: usize) -> [f32; 4] {
    let [x, y, ..] = frame(s);
    if legacy() {
        return [x + 8.0, y + TABS_Y + (page - 1) as f32 * (TAB_H + TAB_GAP), SIDE - 16.0, TAB_H];
    }
    let room = profile_rect(s)[1] - 6.0 - (y + TABS_Y) - ENABLED_GAP;
    let h = ((room / (CATEGORIES.len() + 1) as f32).floor() - TAB_GAP).clamp(11.0, TAB_H);
    let (slot, gap) = if page == ENABLED { (0, 0.0) } else { (page, ENABLED_GAP) };
    [x + 8.0, y + TABS_Y + slot as f32 * (h + TAB_GAP) + gap, side() - 16.0, h]
}

// Where a module's name and bound key go in its row.
pub(super) fn module_text([x, y, w, h]: [f32; 4]) -> [f32; 4] {
    [x, y + ((h - row_hh()) / 2.0).floor(), w - 4.0, row_hh()]
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
        if legacy() && v.page > CATEGORIES.len() {
            v.page = 1;
        }
        let page = if menu.search.is_empty() { v.page } else { 0 };
        let hidden = super::sunset_extra_modules::hidden(&s.modules);
        let stale = v.listed.page != page || v.listed.query != menu.search || v.listed.count != s.modules.len() || v.listed.hidden != hidden;
        // Enabled lists what is on, and what was while it has been showing: a module switched off there keeps its row
        // until the view is left, so a slip is one click to undo. One switched on meanwhile joins the list.
        let was = if page == ENABLED && !stale { v.listed.ids.clone() } else { Vec::new() };
        let mut relisted = false;
        let joined = page == ENABLED && !stale && s.modules.iter().enumerate().any(|(i, m)| m.on && m.in_gui && !hidden.contains(&m.name) && !was.contains(&i));
        if stale || joined {
            // Searching: what it finds, best first (search.rs); else the page's modules A to Z.
            let here = |i: usize, m: &super::sunset::Module| match page {
                0 => true,
                ENABLED => m.on || was.contains(&i),
                p => m.category == CATEGORIES[p - 1],
            };
            let shown = s.modules.iter().enumerate().filter(|(i, m)| here(*i, m) && m.in_gui && !hidden.contains(&m.name)).map(|(i, _)| i);
            let found = (!menu.search.trim().is_empty()).then(|| super::sunset_search::results(&s.modules, menu.search));
            let mut ids = match &found {
                Some(found) => super::sunset_search::ranked(found, shown),
                None => super::sunset::alphabetical(&s.modules, shown),
            };
            // Enabled: category by category in their order, A to Z in each.
            if page == ENABLED {
                ids.sort_by_key(|&i| CATEGORIES.iter().position(|c| *c == s.modules[i].category));
            }
            // A search: the modules found by their name first, then those found by a setting, each as ranked.
            if let Some(found) = &found {
                ids.sort_by_key(|&i| found[i].as_ref().is_some_and(|f| !f.settings.is_empty()));
            }
            v.listed.about = found.as_ref().is_some_and(|f| f.iter().flatten().next().is_some_and(|f| f.about));
            relisted = true;
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
        let View { listed, list, pane, chosen, cards, lined, focus, flash, folded, heads, fold_at, .. } = v;
        // The list is modules only: one a search found by a setting names it on its row (back), and its settings
        // open at that setting. Enabled's have their category's name over them (the wheel stops with it in view).
        list.rows.clear();
        list.stops.clear();
        heads.clear();
        let mut y = 0.0;
        let mut group = None;
        let mut loose = false;
        for (k, &i) in listed.ids.iter().enumerate() {
            let category = s.modules[i].category;
            list.stops.push((y, Hit::Module(i)));
            if page == ENABLED && group != Some(category) {
                heads.push((y, category.label(), CATEGORIES.iter().position(|c| *c == category).map(|p| p + 1)));
                y += TITLE_H;
                group = Some(category);
            }
            // A search says why the modules under its best finds are there: found by a setting (named on each row),
            // or, with nothing found by a name or a setting, by their descriptions.
            let by_setting = listed.settings.get(k).is_some_and(|f| !f.is_empty());
            if page == 0 && !legacy() && !loose && (by_setting || listed.about) {
                heads.push((y, if by_setting { "In settings" } else { "In descriptions" }, None));
                y += TITLE_H;
                loose = true;
            }
            list.rows.push((y, Hit::Module(i)));
            y += module_h();
        }
        list.content = y;
        // A list just made: the module shown, if it is in it, is in view.
        if relisted
            && let Some(i) = *chosen
        {
            let at = |rows: &[(f32, Hit)]| rows.iter().find(|(_, hit)| *hit == Hit::Module(i)).map(|row| row.0);
            if let (Some(stop), Some(off)) = (at(&list.stops), at(&list.rows)) {
                if stop < list.scroll {
                    list.scroll = stop;
                } else if off + module_h() > list.scroll + room {
                    list.scroll = off + module_h() - room;
                }
            }
        }
        // Picked during a search: the setting the search matched in it, which its settings start at (below).
        let target = (std::mem::take(focus) && !legacy() && !menu.search.trim().is_empty())
            .then(|| {
                let i = (*chosen)?;
                let found = listed.ids.iter().position(|&x| x == i).and_then(|k| listed.settings.get(k))?;
                found.iter().copied().find(|&j| s.modules[i].setting_visible(j)).map(|j| (i, j))
            })
            .flatten();
        // (Its section opens for it.)
        if let Some((i, j)) = target
            && let Some(group) = s.modules[i].group_of(j)
        {
            folded.retain(|f| *f != (s.modules[i].name, group));
        }
        // The bind first, then each section in its card (one with no title for a module without sections).
        pane.rows.clear();
        pane.stops.clear();
        cards.clear();
        lined.clear();
        let mut y = 0.0;
        *fold_at = None;
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
            cards.push(Card { top: 0.0, bottom: y, title: None, folded: false, count: 0 });
            // Two sections or more: a line under the bind that shuts them all (what is left is a list of the sections,
            // each a click from open), or opens them again.
            let mut titles: Vec<&'static str> = settings.iter().filter_map(|&j| m.group_of(j)).collect();
            titles.dedup();
            *fold_at = (!legacy() && titles.len() >= 2).then_some(y);
            if fold_at.is_some() {
                y += FOLD_H;
            }
            let mut section: Option<Option<&'static str>> = None;
            // A card ends: a shut one just under its title.
            let end = |card: &mut Card, y: &mut f32| {
                *y += if card.folded { 1.0 } else { CARD_PAD };
                card.bottom = *y;
            };
            let mut first = false;
            for j in settings {
                let hit = Hit::Setting(i, j);
                let group = m.group_of(j);
                if section != Some(group) {
                    if section.is_some()
                        && let Some(card) = cards.last_mut()
                    {
                        end(card, &mut y);
                    }
                    y += CARD_GAP;
                    pane.stops.push((y, Hit::Dash(Part::Frame)));
                    let shut = !legacy() && group.is_some_and(|g| folded.contains(&(m.name, g)));
                    cards.push(Card { top: y, bottom: y, title: group, folded: shut, count: 0 });
                    y += if group.is_some() { TITLE_H } else { CARD_PAD };
                    section = Some(group);
                    first = true;
                } else if legacy() {
                    // (The reworked style draws no line between a section's settings: its card holds them.)
                    lined.push(hit);
                }
                if let Some(card) = cards.last_mut() {
                    card.count += 1;
                    if card.folded {
                        continue;
                    }
                }
                pane.rows.push((y, hit));
                // Where the wheel stops: a row at the column's top, or (the reworked style) just under its section's
                // title, which stays there while its settings scroll (the first of them: the section's own stop).
                if legacy() || group.is_none() {
                    pane.stops.push((y, hit));
                } else if !first {
                    pane.stops.push((y - TITLE_H, hit));
                }
                first = false;
                y += height(s, hit);
            }
            if section.is_some()
                && let Some(card) = cards.last_mut()
            {
                end(card, &mut y);
            }
            y += PAD;
        }
        pane.content = y;
        // Picked during a search: its settings start at the first the search matched in it (under its section's title).
        if let Some((i, j)) = target
            && let Some(&(off, _)) = pane.rows.iter().find(|(_, hit)| *hit == Hit::Setting(i, j))
        {
            pane.scroll = if s.modules[i].group_of(j).is_some() { off - TITLE_H } else { off };
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
        let [_, y, _, room] = columns(s).1;
        [y, y + room]
    })
}

// Top and bottom of where a row shows and of where it can be hit, in this style (not Sidebar's): the module list's
// rows, the settings' rows, and the same two again for the pointer, clear of what is held over the rows (a section's
// title at the settings' top, the mark at the list's edge for a shown module scrolled out of it).
pub(super) fn bodies(s: &State) -> Option<[[f32; 2]; 4]> {
    if sidebar() || !on() {
        return None;
    }
    let (list, pane) = columns(s);
    let (seen, set) = ([list[1], list[1] + list[3]], [pane[1], pane[1] + pane[3]]);
    let hit = match stray(s) {
        Some((true, r, _)) => [r[1] + r[3], seen[1]],
        Some((false, r, _)) => [seen[0], r[1]],
        None => seen,
    };
    Some([seen, set, hit, [pinned(s).map_or(set[0], |(_, r)| r[1] + r[3]), set[1]]])
}

// The shown module's row scrolled out of the list: whether it is above what shows, the strip at that edge of the list
// that says so, and the module.
fn stray(s: &State) -> Option<(bool, [f32; 4], usize)> {
    if legacy() || sidebar() || !on() {
        return None;
    }
    let list = columns(s).0;
    let at = smooth::shown(smooth::DASHBOARD, list_scroll());
    let (i, off) = view(|v| {
        let i = v.chosen?;
        v.list.rows.iter().find(|(_, hit)| *hit == Hit::Module(i)).map(|row| (i, row.0))
    })?;
    let above = off + module_h() <= at + 0.5;
    if !above && off < at + list[3] - 0.5 {
        return None;
    }
    let y = if above { list[1] } else { list[1] + list[3] - TITLE_H };
    Some((above, [list[0], y, list[2] - 4.0, TITLE_H], i))
}

// The rows from the one the column's top is in, up to its bottom.
fn visible(rows: &[(f32, Hit)], scroll: f32, room: f32) -> std::ops::Range<usize> {
    let first = rows.partition_point(|(off, _)| *off <= scroll).saturating_sub(1);
    first..first + rows[first..].partition_point(|(off, _)| *off < scroll + room)
}

fn column_rows(s: &State, column: &Column, [x, y, w, room]: [f32; 4], scroll: f32, out: &mut Vec<([f32; 4], Hit)>) {
    out.extend(column.rows[visible(&column.rows, scroll, room)].iter().map(|&(off, hit)| ([x, y + off - scroll, w, height(s, hit)], hit)));
}

// The module list's rows: clear of its scroll marker on the right.
fn list_rows(s: &State, column: &Column, [x, y, w, room]: [f32; 4], scroll: f32, out: &mut Vec<([f32; 4], Hit)>) {
    column_rows(s, column, [x, y, w - 4.0, room], scroll, out);
}

// Whether a click at `at` on a module's row `r` is on the switch at its end (draw_row puts it there): it turns the
// module on or off, where the rest of the row shows its settings.
pub(super) fn on_switch(r: [f32; 4], at: (f32, f32)) -> bool {
    at.0 >= r[0] + r[2] - 4.0 - super::sunset::RIGHT_PAD - super::sunset::MODULE_SWITCH
}

// The section whose title is held at the settings' top while its settings scroll under it: its card, and the strip.
fn pinned(s: &State) -> Option<(usize, [f32; 4])> {
    if legacy() || sidebar() || !on() {
        return None;
    }
    let pane = columns(s).1;
    let at = smooth::shown(smooth::PANE, pane_scroll());
    let card = view(|v| v.cards.iter().position(|c| c.title.is_some() && !c.folded && c.top < at - 0.5 && c.bottom - CARD_PAD > at + TITLE_H))?;
    Some((card, [pane[0] + CARD_M, pane[1], (pane[2] - 2.0 * CARD_M).max(0.0), TITLE_H]))
}

// A section's title in its strip `r`, in capitals so it does not read as a setting, and the mark at its end that
// says it folds: down while it is open, sideways once shut.
fn section_head(out: &mut Vec<Shape>, r: [f32; 4], title: &str, folded: bool, lit: u32) {
    let colour = if lit > 0 { 0xD4D4D8 } else { MUTED };
    out.push(Shape::Text(title.to_uppercase(), [r[0] + CARD_IN + super::sunset::indent() + 4.0, r[1] + 3.0], colour));
    super::sunset::chevron(out, [r[0] + r[2] - CARD_IN - super::sunset::RIGHT_PAD - 3.0, r[1] + 7.0], if folded { 1 } else { 0 }, 0xFF00_0000 | colour);
}

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
    out.extend(pages().map(|p| (tab_rect(s, p), Hit::Dash(Part::Page(p)))));
    if view(|v| v.chosen.is_some()) {
        out.push((toggle_rect(s), Hit::Dash(Part::Toggle)));
    }
    if !legacy() {
        // The sections' titles, as far as they show, and the one held at the top.
        let pane = columns(s).1;
        let at = smooth::shown(smooth::PANE, pane_scroll());
        view(|v| {
            for (k, card) in v.cards.iter().enumerate().filter(|(_, c)| c.title.is_some()) {
                let r = [pane[0] + CARD_M, pane[1] + card.top - at, (pane[2] - 2.0 * CARD_M).max(0.0), TITLE_H];
                out.extend(super::sunset::in_view(r, [pane[1], pane[1] + pane[3]]).map(|r| (r, Hit::Dash(Part::Section(k)))));
            }
        });
        out.extend(pinned(s).map(|(k, r)| (r, Hit::Dash(Part::Section(k)))));
        // Enabled's categories over their modules (a click turns to one), "Fold all" over a module's sections, the
        // shown module's name and description, and the mark for its row scrolled out of the list.
        let list = columns(s).0;
        let list_at = smooth::shown(smooth::DASHBOARD, list_scroll());
        view(|v| {
            for &(off, _, page) in &v.heads {
                if let Some(page) = page {
                    let r = [list[0], list[1] + off - list_at, list[2] - 4.0, TITLE_H];
                    out.extend(super::sunset::in_view(r, [list[1], list[1] + list[3]]).map(|r| (r, Hit::Dash(Part::Page(page)))));
                }
            }
            if let Some(y) = v.fold_at {
                let r = [pane[0] + pane[2] - CARD_M - FOLD_W, pane[1] + y - at, FOLD_W, FOLD_H];
                out.extend(super::sunset::in_view(r, [pane[1], pane[1] + pane[3]]).map(|r| (r, Hit::Dash(Part::FoldAll))));
            }
        });
        if view(|v| v.chosen.is_some()) {
            out.push((about_rect(s), Hit::Dash(Part::About)));
        }
        out.extend(stray(s).map(|(_, r, _)| (r, Hit::Dash(Part::Reveal))));
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
        turn(s, page);
    }
    // A section's title: shut, or open again. One shut from the title held at the top comes up to it.
    if let (Part::Section(k), false) = (part, right) {
        view(|v| {
            let (Some(i), Some(&Card { top, title: Some(title), .. })) = (v.chosen, v.cards.get(k)) else {
                return;
            };
            let key = (s.modules[i].name, title);
            if let Some(n) = v.folded.iter().position(|f| *f == key) {
                v.folded.remove(n);
            } else {
                v.folded.push(key);
                if top < v.pane.scroll {
                    v.pane.scroll = top;
                    smooth::jump(smooth::PANE);
                }
            }
        });
    }
    // "Fold all": every section of the module shut (or, with all of them shut, open again), from the top.
    if let (Part::FoldAll, false) = (part, right) {
        view(|v| {
            let Some(i) = v.chosen else {
                return;
            };
            let name = s.modules[i].name;
            let titles: Vec<&'static str> = v.cards.iter().filter_map(|c| c.title).collect();
            let all = v.cards.iter().filter(|c| c.title.is_some()).all(|c| c.folded);
            v.folded.retain(|f| f.0 != name);
            if !all {
                v.folded.extend(titles.into_iter().map(|t| (name, t)));
            }
            v.pane.scroll = 0.0;
            smooth::jump(smooth::PANE);
        });
    }
    // The mark for the shown module's row, scrolled out of the list: its row comes back, rows to both its sides.
    if let (Part::Reveal, false) = (part, right) {
        let room = columns(s).0[3];
        view(|v| {
            let stop = v.chosen.and_then(|i| v.list.stops.iter().find(|(_, hit)| *hit == Hit::Module(i))).map(|row| row.0);
            if let Some(stop) = stop {
                v.list.scroll = (stop - (room / 2.0 - module_h()).max(0.0)).max(0.0);
            }
        });
    }
}

// Another view: a category's modules, or Enabled's, from its top (its first module shown). A search ends.
fn turn(s: &mut State, page: usize) {
    view(|v| {
        v.page = page;
        v.list.scroll = 0.0;
        if !legacy() {
            v.chosen = None;
        }
    });
    smooth::jump(smooth::DASHBOARD);
    dash_search_clear(s);
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

// The arrow keys in the reworked style: Up and Down choose the module before or after the one shown (the list
// scrolls to keep it in view), Left and Right turn to the view before or after (not during a search, whose finds are
// from all of them). Whether the key was one of them.
pub(super) fn key(s: &mut State, name: &str) -> bool {
    if !one_line() {
        return false;
    }
    // (Page Up and Page Down: a list's height at a time; Home and End: its first and last.)
    let room = columns(s).0[3];
    let page = ((room / module_h()) as i32 - 1).max(1);
    let step: i32 = match name {
        "key.keyboard.up" => -1,
        "key.keyboard.down" => 1,
        "key.keyboard.page.up" => -page,
        "key.keyboard.page.down" => page,
        "key.keyboard.home" => i32::MIN,
        "key.keyboard.end" => i32::MAX,
        "key.keyboard.left" | "key.keyboard.right" => 0,
        _ => return false,
    };
    if step == 0 {
        if dash_menu(s).search.is_empty() {
            let order: Vec<usize> = pages().collect();
            let at = view(|v| order.iter().position(|&p| p == v.page)).unwrap_or(0);
            let to = if name.ends_with("left") { at.saturating_sub(1) } else { (at + 1).min(order.len() - 1) };
            if to != at {
                turn(s, order[to]);
            }
        }
        return true;
    }
    view(|v| {
        if v.listed.ids.is_empty() {
            return;
        }
        let at = v.chosen.and_then(|i| v.listed.ids.iter().position(|&x| x == i));
        let to = at.map_or(0, |k| (k as i32).saturating_add(step).clamp(0, v.listed.ids.len() as i32 - 1) as usize);
        let i = v.listed.ids[to];
        if v.chosen != Some(i) {
            v.chosen = Some(i);
            v.pane.scroll = 0.0;
            v.focus = true;
            smooth::jump(smooth::PANE);
        }
        // Into view: with its category's name over it, where it is the first under one.
        let at = |rows: &[(f32, Hit)]| rows.iter().find(|(_, hit)| *hit == Hit::Module(i)).map_or(0.0, |row| row.0);
        let (stop, off) = (at(&v.list.stops), at(&v.list.rows));
        if stop < v.list.scroll {
            v.list.scroll = stop;
        } else if off + module_h() > v.list.scroll + room {
            v.list.scroll = off + module_h() - room;
        }
    });
    true
}

// The module Enter (and Space, with nothing typed) turns on or off in the reworked style: the one shown, if it is in
// the list (not one kept from before a search that found nothing). Not one a search found by a setting: whoever
// typed "range" is after the setting its settings have opened at, not after switching the module; the header offers
// Enter only where it does something.
pub(super) fn enter_target(s: &State) -> Option<usize> {
    let searching = !dash_menu(s).search.trim().is_empty();
    view(|v| {
        let i = v.chosen?;
        let k = v.listed.ids.iter().position(|&x| x == i)?;
        (!searching || v.listed.settings.get(k).is_none_or(|found| found.is_empty())).then_some(i)
    })
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

// The menu has just opened (Enabled lists afresh what is on).
pub(super) fn opened() {
    view(|v| {
        v.opened = Some(Instant::now());
        v.listed.page = usize::MAX;
    });
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
    view(|v| glow_in(v, hit))
}

fn glow_in(v: &View, hit: Hit) -> u32 {
    if v.hover.is_some_and(|(h, _)| h == hit) {
        v.lit[0]
    } else if v.left.is_some_and(|(h, _)| h == hit) {
        v.lit[1]
    } else {
        0
    }
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
    // (Not for the module shown in the reworked style: its description is over its settings.)
    let shown = one_line();
    !on() || view(|v| !(shown && v.chosen == Some(i)) && v.hover.is_some_and(|(h, since)| h == Hit::Module(i) && since.elapsed().as_secs_f32() >= TIP_AFTER))
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
    let empty = match legacy() {
        true => "Search modules",
        false if game.sunset_text_width("Search modules and settings") + 16.0 <= r[2] => "Search modules and settings",
        false => "Search",
    };
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
        (false, false) if binding || menu_open => hint.strip_prefix("Press a key or a mouse button   ").unwrap_or(hint).to_owned(),
        // The keys only (a click on a row and its switch say what they do themselves); while searching, Esc empties
        // the search first.
        (false, false) => {
            // Short, so that all of them show at every scale (the search box says that typing searches).
            let turns = enter_target(s).is_some();
            let mut parts = vec!["Arrows: move"];
            if searching {
                parts.extend(turns.then_some("Enter: on/off"));
                parts.push("Esc: clear");
            } else {
                parts.extend(turns.then_some("Space: on/off"));
                parts.push("Esc: close");
            }
            parts.join("   ")
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
            .find(|text| game.sunset_text_width(text) + if legacy() { 0.0 } else { 6.0 * text.matches("   ").count() as f32 } <= to - from)
            // (Or the first alone, where a small window has room for just one.)
            .or_else(|| parts.first().filter(|first| !legacy() && game.sunset_text_width(first) <= to - from).map(|first| (*first).to_owned()));
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
    for p in pages() {
        let r = tab_rect(s, p);
        if p == page {
            out.push(Shape::Round(r, r[3] / 2.0, solid));
        } else {
            let lit = glow(Hit::Dash(Part::Page(p)));
            if lit > 0 {
                out.push(Shape::Round(r, r[3] / 2.0, lit << 24 | 0x00FF_FFFF));
            }
        }
        let (label, at) = (page_label(p).to_owned(), [r[0] + 10.0, r[1] + text_y(r[3])]);
        let label_w = game.sunset_text_width(&label);
        // On the accent capsule the menu's text shadow would show (it is lost on the dark surface everywhere else).
        out.push(if p == page { Shape::Plain(label, at, 0xFFFFFF, true) } else { Shape::Head(label, at, 0xD4D4D8) });
        if !legacy() {
            let listed = s.modules.iter().filter(|m| (p == ENABLED || m.category == CATEGORIES[p - 1]) && m.in_gui && !hidden.contains(&m.name));
            // How many of its modules are on (nothing where none is): Enabled's is the count of them all, and the
            // categories' read as its parts.
            let on = listed.filter(|m| m.on).count();
            let count = on.to_string();
            let cw = game.sunset_text_width(&count);
            if on > 0 && 10.0 + label_w + 6.0 + cw + 8.0 <= r[2] {
                let at = [r[0] + r[2] - 8.0 - cw, r[1] + text_y(r[3])];
                out.push(if p == page { Shape::Plain(count, at, 0xFFE8DC, true) } else { Shape::Text(count, at, MUTED) });
            }
        }
    }

    // The module list's title: the view (or the search) and how many modules it lists.
    let mixed = page == 0 || page == ENABLED;
    let (title, count) = view(|v| (if page == 0 { "Search" } else { page_label(page) }, v.listed.ids.len()));
    let band_y = pane[1] - band_h();
    // The reworked style: one line over the list (the title, then the count), at the rows' names' inset.
    let (title_x, title_y) = if legacy() { (list[0] + 10.0, band_y + text_y(BAND_H)) } else { (list[0] + super::sunset::indent().max(6.0), list[1] - LIST_HEAD + text_y(LIST_HEAD)) };
    out.push(Shape::Head(title.to_owned(), [title_x, title_y], 0xFFFFFF));
    // What the number counts: the category's modules, or the search's finds.
    let on = view(|v| v.listed.ids.iter().filter(|&&i| s.modules[i].on).count());
    let count = match (page, count) {
        // The reworked style: what a search found, and a category's size (its count of those on is by its name on
        // the left).
        (_, n) if !legacy() => format!("{n} {}", if n == 1 { "module" } else { "modules" }),
        (0, n) => format!("{n} found"),
        (_, 1) => format!("1 module, {on} on"),
        (_, n) => format!("{n} modules, {on} on"),
    };
    // A search that finds nothing says so in the list.
    if page == 0 && view(|v| v.listed.ids.is_empty()) {
        let none = format!("Nothing matches \"{}\"", menu.search.trim());
        let x = if legacy() { list[0] + 10.0 } else { list[0] + super::sunset::indent().max(6.0) };
        out.push(Shape::Text(fit_to(game, &none, list[2] + pane[2] - 24.0), [x, list[1] + 6.0], MUTED));
        // (And what a search looks at, and how to get the modules back.)
        if !legacy() {
            for (n, line) in ["A search looks at modules' names, their", "settings and their descriptions.", "Esc clears it."].into_iter().enumerate() {
                out.push(Shape::Text(fit_to(game, line, list[2] + pane[2] - 24.0), [x, list[1] + 22.0 + n as f32 * 10.0], 0x6C6C74));
            }
        }
    }
    // So does Enabled with nothing on.
    if page == ENABLED && view(|v| v.listed.ids.is_empty()) {
        out.push(Shape::Text(fit_to(game, "No module is on", list[2] - 12.0), [list[0] + super::sunset::indent().max(6.0), list[1] + 6.0], MUTED));
    }
    let tw = game.sunset_text_width(title) + 1.0;
    // A search that found nothing says so below; the old style gives no count then. The reworked style: the count
    // under the title (as the module's description is under its name; "0 modules" for a search that found nothing,
    // so the title never stands alone), and a line under the band as in the settings.
    if !legacy() || page != 0 || view(|v| !v.listed.ids.is_empty()) {
        if legacy() {
            out.push(Shape::Text(fit_to(game, &count, (list[2] - 20.0 - tw - 6.0).max(0.0)), [title_x + tw + 6.0, title_y], MUTED));
        } else {
            out.push(Shape::Text(fit_to(game, &count, (list[2] - 16.0 - tw - 6.0).max(0.0)), [title_x + tw + 6.0, title_y], MUTED));
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
        for (k, &(off, hit)) in v.list.rows.iter().enumerate().skip(shown.start).take(shown.len()) {
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
            // The Legacy style's search lists modules of every category: each row says which, where its name leaves
            // the room. The reworked style says that over the settings; a row names the setting a search found its
            // module by (its settings open at it).
            let m = &s.modules[i];
            // (A setting its module's other settings hide is named all the same: it is why the row is here. A module
            // found by its name says its category, where that fits whole.)
            let found = v.listed.settings.get(k).and_then(|f| f.iter().find(|&&j| m.setting_visible(j)).or(f.first())).map(|&j| m.settings[j].name);
            let found = found.filter(|_| !legacy());
            if let Some(label) = found.or_else(|| (page == 0 && (legacy() || !v.listed.about)).then(|| m.category.label())) {
                // Clear of the bound key, and of the row's switch in the reworked style.
                let key_w = m.key.as_deref().map_or(0.0, |_| 34.0) + if legacy() { 0.0 } else { super::sunset::MODULE_SWITCH + 8.0 };
                let room = list[2] - super::sunset::indent().max(6.0) - game.sunset_text_width(m.name) - 10.0 - key_w - 10.0;
                // The reason a search found it is cut to the room it has, never left out.
                let label = if found.is_some() && room >= 20.0 { fit_to(game, label, room) } else { label.to_owned() };
                let lw = game.sunset_text_width(&label);
                if lw <= room {
                    let colour = if v.chosen == Some(i) { 0xA4A4AC } else { 0x6C6C74 };
                    out.push(Shape::Text(label, [list[0] + list[2] - 10.0 - key_w - lw, y + text_y(module_h())], colour));
                }
            }
        }
        for &(off, label, turns) in &v.heads {
            let y = list[1] + off - scroll;
            if y + TITLE_H > list[1] && y < list[1] + list[3] {
                // One that turns to its category at a click lights under the pointer and has the mark that says so.
                let lit = turns.map_or(0, |p| glow_in(v, Hit::Dash(Part::Page(p))));
                let colour = if lit > 0 { 0xD4D4D8 } else { MUTED };
                out.push(Shape::Text(label.to_uppercase(), [list[0] + super::sunset::indent().max(6.0), y + 4.0], colour));
                if turns.is_some() {
                    super::sunset::chevron(out, [list[0] + list[2] - 8.0 - super::sunset::RIGHT_PAD - 3.0, y + 8.0], 1, 0xFF00_0000 | colour);
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
        // (Cut short: the pointer on it shows all of it, front.)
        let cut = about.len() > 2;
        view(|v| v.cut = cut);
        if about.len() > 2 {
            let rest = about.split_off(1).join(" ");
            about.push(fit_to(game, &rest, room));
        }
        let two = about.len() > 1;
        // The reworked style's band is tall enough for two lines: the name stays put.
        let (name_y, about_y) = match (legacy(), two) {
            (false, _) => (TITLE_Y, ABOUT_Y),
            (true, true) => (2.0, 13.0),
            (true, false) => (5.0, 18.0),
        };
        // The reworked style: at the settings' names' inset.
        let tx = if legacy() { pane[0] + CARD_M + 2.0 } else { pane[0] + CARD_M + CARD_IN + super::sunset::indent() + 4.0 };
        let name = fit_to(game, m.name, room);
        // Among modules of every category (a search's finds, Enabled's): which this one is in, after its name.
        let (name_w, label) = (game.sunset_text_width(&name) + 1.0, m.category.label());
        if mixed && !legacy() && name_w + 8.0 + game.sunset_text_width(label) <= room {
            out.push(Shape::Text(label.to_owned(), [tx + name_w + 8.0, band_y + name_y], 0x6C6C74));
        }
        out.push(Shape::Head(name, [tx, band_y + name_y], 0xFFFFFF));
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
            let text = if legacy() { "Right click a module to see its settings here" } else { "Click a module to see its settings here" };
            let text = fit_to(game, text, pane[2] - 2.0 * CARD_M - 4.0);
            out.push(Shape::Text(text, [pane[0] + CARD_M + 2.0, pane[1] + 6.0], MUTED));
        }
    }

    // Its sections: a card each, with its title.
    let pane_at = smooth::shown(smooth::PANE, pane_scroll());
    view(|v| {
        out.push(Shape::ClipStart);
        for (k, card) in v.cards.iter().enumerate() {
            let (top, bottom) = (pane[1] + card.top - pane_at, pane[1] + card.bottom - pane_at);
            if bottom < pane[1] || top > pane[1] + pane[3] {
                continue;
            }
            let strip = [pane[0] + CARD_M, top, pane[2] - 2.0 * CARD_M, TITLE_H];
            out.push(Shape::Round([strip[0], top, strip[2], bottom - top], control_radius().max(4.0), pal().dash_card));
            match card.title {
                Some(title) if legacy() => out.push(Shape::Text(title.to_owned(), [strip[0] + CARD_IN + super::sunset::indent() + 4.0, top + 3.0], MUTED)),
                Some(title) => {
                    section_head(out, strip, title, card.folded, glow_in(v, Hit::Dash(Part::Section(k))));
                    // A shut one says how much it holds.
                    if card.folded {
                        let count = format!("{} hidden", card.count);
                        let x = strip[0] + strip[2] - CARD_IN - super::sunset::RIGHT_PAD - 12.0 - game.sunset_text_width(&count);
                        out.push(Shape::Text(count, [x, top + 3.0], 0x6C6C74));
                    }
                }
                None => {}
            }
        }
        // The setting a search opened its module at, lit for a moment.
        if let Some((hit, since)) = v.flash {
            let t = since.elapsed().as_secs_f32() / FLASH_TIME;
            match v.pane.rows.iter().find(|(_, h)| *h == hit) {
                Some(&(off, _)) if t < 1.0 => {
                    let alpha = (0x60 as f32 * (1.0 - t * t)) as u32;
                    let y = pane[1] + off - pane_at;
                    out.push(Shape::Fill([pane[0] + CARD_M, y, pane[2] - 2.0 * CARD_M, row_hh()], alpha << 24 | (accent() & 0x00FF_FFFF)));
                }
                _ => v.flash = None,
            }
        }
        // While a search shows the module, the settings it matched keep a mark at their left.
        if page == 0
            && !legacy()
            && let Some(i) = v.chosen
            && let Some(k) = v.listed.ids.iter().position(|&x| x == i)
        {
            for &j in v.listed.settings.get(k).into_iter().flatten() {
                if let Some(&(off, _)) = v.pane.rows.iter().find(|(_, h)| *h == Hit::Setting(i, j)) {
                    out.push(Shape::Fill([pane[0] + CARD_M, pane[1] + off - pane_at + 2.0, 2.0, row_hh() - 4.0], 0xFF00_0000 | accent()));
                }
            }
        }
        // "Fold all" over a module's sections ("Open all" once they are all shut).
        if let Some(y) = v.fold_at {
            let all = v.cards.iter().filter(|c| c.title.is_some()).all(|c| c.folded);
            let text = if all { "Open all" } else { "Fold all" };
            let colour = if glow_in(v, Hit::Dash(Part::FoldAll)) > 0 { 0xD4D4D8 } else { MUTED };
            let x = pane[0] + pane[2] - CARD_M - CARD_IN - super::sunset::RIGHT_PAD - game.sunset_text_width(text);
            out.push(Shape::Text(text.to_owned(), [x, pane[1] + y - pane_at + 3.0], colour));
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
pub(super) fn front(s: &State, game: &Game<'_>, px: f32, out: &mut Vec<Shape>) {
    if sidebar() || !on() {
        return;
    }
    let (list, pane) = columns(s);
    let (list_at, pane_at) = (smooth::shown(smooth::DASHBOARD, list_scroll()), smooth::shown(smooth::PANE, pane_scroll()));
    let (content, pane_content) = view(|v| (v.list.content, v.pane.content));
    let pin = pinned(s);
    for (c, content, at) in [(list, content, list_at), (pane, pane_content, pane_at)] {
        let room = c[3];
        if room <= 0.0 {
            continue;
        }
        // Clear of the hairline down the column's left.
        let (x, w) = (c[0] + px, c[2] - px);
        match pin.filter(|_| c[0] == pane[0]) {
            // A section's title held at the settings' top: over the rows scrolling under it, which fade into it.
            Some((k, r)) => {
                let (title, lit) = view(|v| (v.cards.get(k).and_then(|c| c.title).unwrap_or_default(), glow_in(v, Hit::Dash(Part::Section(k)))));
                out.push(Shape::Fill([x, c[1], w, TITLE_H], 0xFF00_0000 | pal().dash_surface));
                out.push(Shape::Corners(r, control_radius().max(4.0), [true, true, false, false], pal().dash_card));
                section_head(out, r, title, false, lit);
                fade(out, [r[0], r[1] + r[3], r[2]], pal().dash_card, px, false);
            }
            None if at > 0.5 => fade(out, [x, c[1], w], pal().dash_surface, px, false),
            None => {}
        }
        if content - at > room + 0.5 {
            fade(out, [x, c[1] + room, w], pal().dash_surface, px, true);
        }
        // In the reworked style the settings' marker sits in the margin beside the cards, off the window's edge.
        let inset = if legacy() || c[0] == list[0] { 4.0 } else { 6.0 };
        scrollbar(out, c[0] + c[2] - inset, c[1] + 1.0, room - 2.0, content, room, at);
    }
    // The shown module's row scrolled out of the list: its name at that edge of the list, and which way it is (a
    // click brings the row back), so the settings on the right are never without their module.
    if let Some((above, r, i)) = stray(s) {
        let colour = if glow(Hit::Dash(Part::Reveal)) > 0 { 0xFFFFFF } else { 0xB4B4BC };
        out.push(Shape::Fill([r[0] + px, r[1], list[2] - px, r[3]], 0xFF00_0000 | pal().dash_surface));
        out.push(Shape::Fill([r[0] + px, if above { r[1] + r[3] - px } else { r[1] }, list[2] - px, px], pal().dash_hairline));
        out.push(Shape::Text(fit_to(game, s.modules[i].name, r[2] - 30.0), [r[0] + super::sunset::indent().max(6.0), r[1] + 3.0], colour));
        super::sunset::chevron(out, [r[0] + r[2] - 4.0 - super::sunset::RIGHT_PAD - 3.0, r[1] + 6.5], if above { 2 } else { 0 }, 0xFF00_0000 | colour);
    }
    // The pointer on a description that was cut short: all of it, in its place and down over the settings' top.
    let shown = view(|v| (v.cut && v.hover.is_some_and(|(h, since)| h == Hit::Dash(Part::About) && since.elapsed().as_secs_f32() >= 0.3)).then_some(v.chosen).flatten());
    if let Some(i) = shown {
        let w = (pane[2] - 2.0 * CARD_M).max(60.0);
        let lines = super::sunset::wrap(game, s.modules[i].about, w - 12.0);
        let r = [pane[0] + CARD_M, pane[1] - band_h() + ABOUT_Y - 3.0, w, lines.len() as f32 * 10.0 + 6.0];
        super::sunset::framed(out, r, control_radius().max(4.0), pal().tooltip, px);
        for (n, line) in lines.into_iter().enumerate() {
            out.push(Shape::Text(line, [r[0] + 6.0, r[1] + 3.0 + n as f32 * 10.0], 0xE4E4E4));
        }
    }
}

