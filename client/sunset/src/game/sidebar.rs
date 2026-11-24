// The Sidebar style of the click GUI: one window with the tabs down its left (Rage, Legit, ESP...), each tab's modules
// split into groups (pills over the page), and each module a card. A card is just its name, switch and what it does
// while the module is off; on, it also shows its main settings and a link to the module's page with all of them, in
// sections. The settings are the menu's own rows, so draw_gui draws them and gui_press clicks them as ever: this file
// places them and draws around them, as the Dashboard style does (dashboard.rs hands its calls here while this is on).
use crate::game::sunset_compat::SunsetExtra as _;
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset::*;
use super::sunset_dashboard::Part as DashPart;
use super::sunset_smooth as smooth;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

// What can be clicked around the rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Part {
    Tab(usize),
    Group(usize),
    // ClickGUI's own page (how the menu looks).
    Appearance,
    // A card's header: the module's page.
    Open(usize),
    // A module's switch, on its card or its page.
    Switch(usize),
    // A card's "All N settings": the module's page.
    All(usize),
    Back,
    // The title bar: a press there drags the window.
    Title,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Tab,
    Module(usize),
}

// What a card is.
#[derive(Clone, Copy)]
enum Kind {
    // A module's card: how many settings it has in all, and whether the link to them shows.
    Module { i: usize, all: usize, more: bool },
    // A section of a module's page, and its title.
    Section(&'static str),
    // The top of a module's page: what it does, and its switch.
    Intro(usize),
}

// A card where it sits before scrolling: x from the content's left, y from its top.
#[derive(Clone, Copy)]
struct Card {
    r: [f32; 4],
    kind: Kind,
}

const TABS_MAX: usize = 16;

struct View {
    tab: usize,
    // The group open in each tab.
    groups: [usize; TABS_MAX],
    page: Page,
    scroll: f32,
    content: f32,
    cards: Vec<Card>,
    // The settings' rows, placed like the cards.
    rows: Vec<([f32; 4], Hit)>,
    // The rows with a line above them: all but the first of a card.
    lined: Vec<Hit>,
    // The open tab's groups that list something.
    pills: Vec<usize>,
    // Whether the pill row shows (more than one group to pick).
    pill_bar: bool,
    searching: bool,
    // What back drew last frame that can be clicked (it measures text, which the hits can't).
    parts: Vec<([f32; 4], Part)>,
}

static VIEW: Mutex<View> = Mutex::new(View {
    tab: 0,
    groups: [0; TABS_MAX],
    page: Page::Tab,
    scroll: 0.0,
    content: 0.0,
    cards: Vec::new(),
    rows: Vec::new(),
    lined: Vec::new(),
    pills: Vec::new(),
    pill_bar: false,
    searching: false,
    parts: Vec::new(),
});

fn view<R>(f: impl FnOnce(&mut View) -> R) -> R {
    f(&mut VIEW.lock().unwrap_or_else(|e| e.into_inner()))
}

// Where the window was dragged to (its top left), for as long as the game runs; None: in the middle.
static PLACE: Mutex<Option<[f32; 2]>> = Mutex::new(None);

fn place() -> Option<[f32; 2]> {
    *PLACE.lock().unwrap_or_else(|e| e.into_inner())
}

// The window dragged by its title bar: its top left at `at` (kept on the screen as it is drawn).
pub(super) fn move_to(at: [f32; 2]) {
    *PLACE.lock().unwrap_or_else(|e| e.into_inner()) = Some(at);
}

// Where the window's top left is now, for a drag taking it by the title bar.
pub(super) fn corner(s: &State) -> [f32; 2] {
    let f = frame(s);
    [f[0], f[1]]
}

// ClickGUI's Style, as sync_flags last read it.
static ON: AtomicBool = AtomicBool::new(false);

pub(super) fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub(super) fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

// The window, in GUI pixels: at most this big, in the middle (or where it was dragged). Its width for ClickGUI's Panel width: 470 at its least, 680 by default, 750 at its most.
const WIDTH_BASE: f32 = 260.0;
const WIDTH_PER_PANEL: f32 = 3.5;
const HEIGHT: f32 = 440.0;
const SIDE: f32 = 116.0;
const PAD: f32 = 10.0;
// Between cards, and the widest a column gets before another fits.
const GAP: f32 = 8.0;
const COLUMN: f32 = 230.0;
// A card: its header, its description line, its "All N settings" line, and the space under its last row.
const BOTTOM: f32 = 5.0;
// The main settings a card shows while its module is on (ESP's people set the most).
const QUICK: usize = 4;
const QUICK_ESP: usize = 8;
const SWITCH: [f32; 2] = [22.0, 12.0];

// Lucide's icons (font.rs loads their font), by what they show.
mod icon {
    pub const SWORDS: &str = "\u{e2b4}";
    pub const CROSSHAIR: &str = "\u{e0b0}";
    pub const EYE: &str = "\u{e53b}";
    pub const SUN: &str = "\u{e17c}";
    pub const FOOT: &str = "\u{e3bd}";
    pub const WRENCH: &str = "\u{e1b1}";
    pub const SERVER: &str = "\u{e157}";
    pub const MONITOR: &str = "\u{e121}";
    pub const PALETTE: &str = "\u{e1dd}";
    pub const SEARCH: &str = "\u{e155}";
    pub const BACK: &str = "\u{e072}";
    pub const MORE: &str = "\u{e073}";
    pub const OTHER: &str = "\u{e065}";

    // A tab's icon, by its key in MENU.
    pub fn of_tab(key: &str) -> &'static str {
        match key {
            "rage" => SWORDS,
            "legit" => CROSSHAIR,
            "esp" => EYE,
            "visuals" => SUN,
            "movement" => FOOT,
            "utility" => WRENCH,
            "server" => SERVER,
            "client" => MONITOR,
            _ => OTHER,
        }
    }
}

// ClickGUI's Spacing: Compact shrinks the menu's own rows and bars as it does the settings' rows.
fn spacing() -> f32 {
    row_hh() / 14.0
}

fn bar_top() -> f32 {
    30.0 * spacing()
}
fn pill_row() -> f32 {
    30.0 * spacing()
}
fn card_head() -> f32 {
    20.0 * spacing()
}
fn about_line() -> f32 {
    10.0 * spacing()
}
fn foot_line() -> f32 {
    20.0 * spacing()
}
fn section_title() -> f32 {
    20.0 * spacing()
}
fn back_h() -> f32 {
    20.0 * spacing()
}
fn intro_h() -> f32 {
    30.0 * spacing()
}
fn nav_h() -> f32 {
    20.0 * spacing()
}

// ClickGUI's Background opacity on one of the menu's surfaces.
fn see_through(argb: u32) -> u32 {
    let k = f32::from_bits(LOOK[2].load(Ordering::Relaxed)).clamp(0.0, 1.0);
    (((argb >> 24) as f32 * k).round() as u32) << 24 | (argb & 0x00FF_FFFF)
}

// A control `h` tall rounded by ClickGUI's Control radius: at its default (3) a full pill, square at 0.
fn round(h: f32) -> f32 {
    (control_radius() / 3.0 * h / 2.0).min(h / 2.0)
}

// Where a card's text starts: ClickGUI's Text indent in from its edge.
fn inset() -> f32 {
    6.0 + indent()
}

const MUTED: u32 = 0x8C8C94;
const DIM: u32 = 0xB4B4BA;
const WHITE: u32 = 0xFFFFFF;

fn frame(s: &State) -> [f32; 4] {
    let menu = dash_menu(s);
    let (w, h) = ((menu.view_w - 16.0).clamp(0.0, WIDTH_BASE + wanted_panel_w() * WIDTH_PER_PANEL).floor(), (menu.view_h - 16.0).clamp(0.0, HEIGHT).floor());
    match place() {
        Some([x, y]) => [x.clamp(0.0, (menu.view_w - w).max(0.0)).floor(), y.clamp(0.0, (menu.view_h - h).max(0.0)).floor(), w, h],
        None => [((menu.view_w - w) / 2.0).floor(), ((menu.view_h - h) / 2.0).floor(), w, h],
    }
}

// Where the cards scroll: right of the sidebar, under the top bar (and the pills).
fn content_in(f: [f32; 4], pill_bar: bool) -> [f32; 4] {
    let top = bar_top() + if pill_bar { pill_row() } else { 0.0 } + PAD;
    [f[0] + SIDE + PAD, f[1] + top, (f[2] - SIDE - 2.0 * PAD).max(0.0), (f[3] - top - PAD).max(0.0)]
}

fn content(s: &State) -> [f32; 4] {
    content_in(frame(s), view(|v| v.pill_bar))
}

// A colour between two (0xAARRGGBB), `t` of the way to the second.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, k: u32| ((c >> k) & 0xFF) as f32;
    let one = |k: u32| ((ch(a, k) + (ch(b, k) - ch(a, k)) * t).round() as u32) << k;
    one(24) | one(16) | one(8) | one(0)
}

fn tint(alpha: u32) -> u32 {
    alpha << 24 | accent()
}

// The modules the menu lists: shown in the GUI, and not switched off in Modules.
fn listed(s: &State) -> Vec<usize> {
    let hidden = super::sunset_extra_modules::hidden(&s.modules);
    (0..s.modules.len()).filter(|&i| s.modules[i].in_gui && !hidden.contains(&s.modules[i].name)).collect()
}

// A module's main settings while it is on (its first settings tab, else all), and how many it has in all.
fn main_settings(m: &Module) -> (Vec<usize>, usize) {
    let shown: Vec<usize> = (0..m.settings.len()).filter(|&j| m.setting_visible(j)).collect();
    // Modules is its switches, one per module: its card only opens them (as in the click GUI).
    if !m.on || switches_only(m) {
        return (Vec::new(), shown.len());
    }
    let first: Vec<usize> = match tabs_of(m).into_iter().next() {
        Some((_, list)) => list.into_iter().filter(|j| shown.contains(j)).collect(),
        None => shown.clone(),
    };
    let quick = if m.name == "ESP" { QUICK_ESP } else { QUICK };
    (first.into_iter().take(quick).collect(), shown.len())
}

// Modules (the switch for each module in the menu): not turned on or off itself, only opened.
fn switches_only(m: &Module) -> bool {
    m.name == super::sunset_extra_modules::NAME
}

// A module with no switch of its own on its card and page: ClickGUI (Appearance) and Modules.
fn no_switch(m: &Module) -> bool {
    m.name == "ClickGUI" || switches_only(m)
}

// Modules' page: its switches in the menu's tabs (the module each is for decides), in the sidebar's order.
fn module_switches(m: &Module, i: usize) -> Vec<(&'static str, Vec<Hit>)> {
    let mut out: Vec<(&'static str, Vec<Hit>)> = MENU.iter().map(|t| (t.1, Vec::new())).collect();
    let mut keyed: Vec<((usize, usize, usize), usize)> = (0..m.settings.len())
        .filter(|&j| m.setting_visible(j) && matches!(m.settings[j].value, Value::Bool(_)))
        .map(|j| (place_of(m.settings[j].name).2, j))
        .collect();
    keyed.sort();
    for (key, j) in keyed {
        out[key.0.min(MENU.len() - 1)].1.push(Hit::Setting(i, j));
    }
    out.retain(|(_, list)| !list.is_empty());
    out
}

// A module's page in sections: its settings tabs, else its own sections, else one; the bind first. Only the settings
// that apply now, and no empty sections.
fn sections(m: &Module, i: usize) -> Vec<(&'static str, Vec<Hit>)> {
    if switches_only(m) {
        return module_switches(m, i);
    }
    let n = m.settings.len();
    let raw: Vec<(&'static str, Vec<usize>)> = {
        let tabs = tabs_of(m);
        if !tabs.is_empty() {
            tabs
        } else if let Some(&(first, _)) = m.groups.first() {
            let mut v = Vec::new();
            if first > 0 {
                v.push(("General", (0..first).collect()));
            }
            for (k, &(start, title)) in m.groups.iter().enumerate() {
                let end = m.groups.get(k + 1).map_or(n, |g| g.0);
                v.push((title, (start..end).collect()));
            }
            v
        } else {
            vec![("Settings", (0..n).collect())]
        }
    };
    let mut out: Vec<(&'static str, Vec<Hit>)> = raw
        .into_iter()
        .map(|(title, list)| (title, list.into_iter().filter(|&j| m.setting_visible(j)).map(|j| Hit::Setting(i, j)).collect::<Vec<_>>()))
        .filter(|(_, list)| !list.is_empty())
        .collect();
    match out.first_mut() {
        Some((_, list)) => list.insert(0, Hit::Bind(i)),
        None => out.push(("Settings", vec![Hit::Bind(i)])),
    }
    out
}

// Cards in columns, each under the shortest so far.
struct Masonry {
    col_w: f32,
    heights: Vec<f32>,
    top: f32,
}

impl Masonry {
    fn new(width: f32, top: f32) -> Self {
        let cols = ((width + GAP) / (COLUMN + GAP)).floor().clamp(1.0, 3.0) as usize;
        let col_w = ((width - GAP * (cols - 1) as f32) / cols as f32).floor();
        Masonry { col_w, heights: vec![top; cols], top }
    }

    fn place(&mut self, h: f32) -> [f32; 2] {
        let c = (0..self.heights.len()).min_by(|&a, &b| self.heights[a].total_cmp(&self.heights[b])).unwrap_or(0);
        let at = [c as f32 * (self.col_w + GAP), self.heights[c]];
        self.heights[c] += h + GAP;
        at
    }

    fn bottom(&self) -> f32 {
        self.heights.iter().copied().fold(self.top, f32::max)
    }
}

// Once a frame, before anything reads the rows: what the page lists, its cards and their rows.
pub(super) fn prepare(s: &State) {
    if !on() {
        return;
    }
    let menu = dash_menu(s);
    let searching = !menu.search.trim().is_empty();
    let listed = listed(s);
    let f = frame(s);
    view(|v| {
        v.searching = searching;
        if let Page::Module(i) = v.page
            && !listed.contains(&i)
            && s.modules.get(i).is_none_or(|m| m.name != "ClickGUI")
        {
            v.page = Page::Tab;
        }
        let t = v.tab.min(MENU.len() - 1);
        // The open tab's groups with something in them.
        v.pills.clear();
        for (g, _) in MENU[t].3.iter().enumerate() {
            let in_group: Vec<usize> = listed.iter().copied().filter(|&i| {
                let (_, _, key) = place_of(s.modules[i].name);
                key.0 == t && (key.1 == g || (key.1 == usize::MAX && MENU[t].3[g].0 == "Other"))
            }).collect();
            if !in_group.is_empty() {
                v.pills.push(g);
            }
        }
        if !v.pills.contains(&v.groups[t]) {
            v.groups[t] = v.pills.first().copied().unwrap_or(0);
        }
        v.pill_bar = !searching && v.page == Page::Tab && v.pills.len() > 1;
        let c = content_in(f, v.pill_bar);
        let width = c[2];
        let (page, open_group) = (v.page, v.groups[t]);
        v.cards.clear();
        v.rows.clear();
        v.lined.clear();
        let View { cards, rows, lined, .. } = &mut *v;
        // A card's rows from `y`, inset from its sides; the height they take.
        let mut put_rows = |list: &[Hit], x: f32, w: f32, mut y: f32| -> f32 {
            let top = y;
            for (k, &hit) in list.iter().enumerate() {
                if k > 0 {
                    lined.push(hit);
                }
                let h = row_h(s, hit);
                rows.push(([x + 2.0, y, w - 4.0, h], hit));
                y += h;
            }
            y - top
        };
        let content_h = match page {
            Page::Module(i) => {
                let m = &s.modules[i];
                cards.push(Card { r: [0.0, back_h(), width, intro_h()], kind: Kind::Intro(i) });
                let mut grid = Masonry::new(width, back_h() + intro_h() + GAP);
                for (title, list) in sections(m, i) {
                    let h = section_title() + list.iter().map(|&hit| row_h(s, hit)).sum::<f32>() + BOTTOM;
                    let [x, y] = grid.place(h);
                    cards.push(Card { r: [x, y, grid.col_w, h], kind: Kind::Section(title) });
                    put_rows(&list, x, grid.col_w, y + section_title());
                }
                grid.bottom()
            }
            Page::Tab => {
                let ids: Vec<usize> = if searching {
                    super::sunset_search::ranked(&super::sunset_search::results(&s.modules, menu.search), listed.iter().copied())
                } else {
                    let g = open_group;
                    let mut ids: Vec<usize> = listed.iter().copied().filter(|&i| {
                        let (_, _, key) = place_of(s.modules[i].name);
                        key.0 == t && (key.1 == g || (key.1 == usize::MAX && MENU[t].3.get(g).is_some_and(|x| x.0 == "Other")))
                    }).collect();
                    ids.sort_by_key(|&i| place_of(s.modules[i].name).2);
                    ids
                };
                let mut grid = Masonry::new(width, 0.0);
                for i in ids {
                    let m = &s.modules[i];
                    let (main, all) = main_settings(m);
                    let list: Vec<Hit> = main.iter().map(|&j| Hit::Setting(i, j)).collect();
                    let more = (m.on || switches_only(m)) && all > main.len();
                    let h = card_head()
                        + if m.about.is_empty() { 0.0 } else { about_line() }
                        + list.iter().map(|&hit| row_h(s, hit)).sum::<f32>()
                        + if more { foot_line() } else { 0.0 }
                        + BOTTOM;
                    let [x, y] = grid.place(h);
                    cards.push(Card { r: [x, y, grid.col_w, h], kind: Kind::Module { i, all, more } });
                    let rows_y = y + card_head() + if m.about.is_empty() { 0.0 } else { about_line() };
                    put_rows(&list, x, grid.col_w, rows_y);
                }
                grid.bottom()
            }
        };
        v.content = content_h;
        v.scroll = v.scroll.clamp(0.0, (content_h - c[3]).max(0.0));
    });
}

pub(super) fn scroll() -> f32 {
    view(|v| v.scroll)
}

fn shown_scroll() -> f32 {
    smooth::shown(smooth::PANE, scroll())
}

// The top and bottom of the cards' area.
pub(super) fn list_y(s: &State) -> [f32; 2] {
    let c = content(s);
    [c[1], c[1] + c[3]]
}

// The rows in view, and where.
pub(super) fn rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let c = content(s);
    let at = shown_scroll();
    view(|v| {
        v.rows
            .iter()
            .map(|&([x, y, w, h], hit)| ([c[0] + x, c[1] + y - at, w, h], hit))
            .filter(|&([_, y, _, h], _)| y + h > c[1] && y < c[1] + c[3])
            .collect()
    })
}

pub(super) fn line_above(hit: Hit) -> bool {
    view(|v| v.lined.contains(&hit))
}

// What the pointer can hit around the rows: the window, then what back drew.
pub(super) fn hits(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut out = vec![(frame(s), Hit::Dash(DashPart::Frame))];
    view(|v| out.extend(v.parts.iter().map(|&(r, p)| (r, Hit::Dash(DashPart::Side(p))))));
    out
}

// The module a click on `part` turns on or off.
pub(super) fn toggles(s: &State, part: Part) -> Option<usize> {
    match part {
        Part::Switch(i) => Some(i),
        _ => None,
    }
}

pub(super) fn press(s: &mut State, part: Part) {
    let open = |v: &mut View, page: Page| {
        if v.page != page {
            v.page = page;
            v.scroll = 0.0;
            smooth::jump(smooth::PANE);
        }
    };
    match part {
        Part::Tab(t) => {
            view(|v| {
                v.tab = t;
                v.page = Page::Tab;
                v.scroll = 0.0;
            });
            smooth::jump(smooth::PANE);
            dash_search_clear(s);
        }
        Part::Group(g) => {
            view(|v| {
                v.groups[v.tab.min(TABS_MAX - 1)] = g;
                v.scroll = 0.0;
            });
            smooth::jump(smooth::PANE);
        }
        Part::Appearance => {
            if let Some(i) = s.modules.iter().position(|m| m.name == "ClickGUI") {
                view(|v| open(v, Page::Module(i)));
                dash_search_clear(s);
            }
        }
        // ClickGUI's Settings: Window opens the click GUI's settings window over the menu (the same module again
        // closes it); Inline, the module's page.
        Part::Open(i) | Part::All(i) if s.module("ClickGUI").is_some_and(|m| m.p("Settings") == "Window") => {
            let at = [(s.gui.view_w - win_w(s)) / 2.0, 40.0];
            s.gui.window = match s.gui.window {
                Some(w) if w.module == i => None,
                _ => Some(Win { module: i, x: at[0], y: at[1], home: at, scroll: 0.0, about_h: 0.0, content: 0.0, room: 0.0 }),
            };
            s.binding = None;
        }
        Part::Open(i) | Part::All(i) => {
            view(|v| open(v, Page::Module(i)));
            dash_search_clear(s);
        }
        Part::Back => view(|v| open(v, Page::Tab)),
        // A switch goes through gui_press's toggle (toggles), the title bar to its drag.
        Part::Switch(_) | Part::Title => {}
    }
}

pub(super) fn wheel(s: &State, steps: f32, at: (f32, f32)) {
    let c = content(s);
    if !inside(c, at) {
        return;
    }
    view(|v| {
        let max = (v.content - c[3]).max(0.0);
        v.scroll = (v.scroll - steps * 24.0).clamp(0.0, max);
    });
}

// A switch: a pill in the accent while on, with its knob at the end it is on.
fn switch(out: &mut Vec<Shape>, r: [f32; 4], on: bool, lit: u32) {
    let off = pal().dash_toggle_off;
    out.push(Shape::Round(r, round(r[3]), if on { 0xFF00_0000 | accent() } else { off + (lit << 16 | lit << 8 | lit) }));
    let knob = r[3] - 4.0;
    let kx = if on { r[0] + r[2] - knob - 2.0 } else { r[0] + 2.0 };
    out.push(Shape::Round([kx, r[1] + 2.0, knob, knob], round(knob), if on { 0xFFFF_FFFF } else { 0xFFB4_B4BA }));
}

// Everything under the rows: the window, the sidebar, the top bar with the search, the pills, the cards and their
// headers, and the scroll marker. What can be clicked is kept for the hits.
pub(super) fn back(s: &State, game: &Game<'_>, hover: Option<Hit>, px: f32, out: &mut Vec<Shape>) {
    super::sunset_dashboard::follow_hover(hover);
    let glow = super::sunset_dashboard::glow;
    let lit = |p: Part| glow(Hit::Dash(DashPart::Side(p)));
    let width = |t: &str| game.sunset_text_width(t);
    let fit = |t: &str, max: f32| fit_to(game, t, max);
    let mut parts: Vec<([f32; 4], Part)> = Vec::new();
    let f = frame(s);
    let [x, y, w, h] = f;
    let radius = panel_radius();
    let surface = see_through(pal().dash_surface);
    let card = pal().dash_card;
    let hair = pal().dash_hairline;
    let menu = dash_menu(s);
    let (tab, page, searching, pill_bar) = view(|v| (v.tab.min(MENU.len() - 1), v.page, v.searching, v.pill_bar));

    // The window and its sidebar.
    out.push(Shape::Squircle([x - px, y - px, w + 2.0 * px, h + 2.0 * px], radius + px, hair));
    out.push(Shape::Squircle(f, radius, surface));
    out.push(Shape::Corners([x, y, SIDE, h], radius, [true, false, false, true], 0x0AFF_FFFF));
    out.push(Shape::Fill([x + SIDE, y, px, h], hair));

    // The name at the top: the HUD's watermark text when it has one.
    let name = s.modules.iter().find(|m| m.name == "HUD").map(|m| m.t("Watermark text").trim().to_owned()).filter(|t| !t.is_empty());
    let name = name.unwrap_or_else(|| NAME.to_owned());
    out.push(Shape::Head(fit(&name, SIDE - 24.0), [x + 14.0, y + text_y(bar_top())], WHITE));
    out.push(Shape::Round([x + 14.0, y + bar_top() - 6.0, 16.0, 2.0], 1.0, 0xFF00_0000 | accent()));

    // The tabs, under their sections, then Appearance. Where the window is too short for them (a high GUI scale), the
    // section titles go, then the rows get shorter.
    let sections = 1 + MENU.windows(2).filter(|p| p[0].0 != p[1].0).count();
    let rows = MENU.len() + 1;
    let room = h - bar_top() - 4.0 - 6.0 - 4.0;
    let titled = room >= sections as f32 * 13.0 + rows as f32 * (nav_h() + 1.0);
    let row = if titled { nav_h() } else { (room / rows as f32 - 1.0).max(12.0).min(nav_h()) };
    let mut yy = y + bar_top() + 4.0;
    let mut section = "";
    let on_page = |p: Part| match p {
        Part::Tab(t) => !searching && t == tab && page == Page::Tab
            || matches!(page, Page::Module(i) if s.modules.get(i).is_some_and(|m| m.name != "ClickGUI" && place_of(m.name).2.0 == t)),
        Part::Appearance => matches!(page, Page::Module(i) if s.modules.get(i).is_some_and(|m| m.name == "ClickGUI")),
        _ => false,
    };
    let nav = |out: &mut Vec<Shape>, parts: &mut Vec<([f32; 4], Part)>, yy: &mut f32, glyph: &'static str, label: &str, part: Part| {
        let r = [x + 6.0, *yy, SIDE - 12.0, row];
        let selected = on_page(part);
        if selected {
            out.push(Shape::Round(r, control_radius(), tint(0x2A)));
            out.push(Shape::Round([r[0], r[1] + 4.0, 2.0, row - 8.0], 1.0, 0xFF00_0000 | accent()));
        } else if lit(part) > 0 {
            out.push(Shape::Round(r, control_radius(), lit(part) << 24 | 0x00FF_FFFF));
        }
        out.push(Shape::Icon(glyph, [r[0] + 7.0, r[1] + text_y(row) + 0.5], if selected || lit(part) > 0 { 0xFF00_0000 | mix(0xFF00_0000 | accent(), 0xFFFF_FFFF, 0.25) } else { 0xFF00_0000 | accent() }, 1.0));
        out.push(Shape::Text(label.to_owned(), [r[0] + 21.0, r[1] + text_y(row)], if selected { WHITE } else { DIM }));
        parts.push((r, part));
        *yy += row + 1.0;
    };
    for (t, (sec, label, key, _)) in MENU.iter().enumerate() {
        if *sec != section && titled {
            section = sec;
            yy += 4.0;
            out.push(Shape::Scaled((*sec).to_owned(), [x + 12.0, yy], MUTED, 0.75));
            yy += 9.0;
        }
        nav(out, &mut parts, &mut yy, icon::of_tab(key), label, Part::Tab(t));
    }
    yy += if titled { 6.0 } else { 2.0 };
    nav(out, &mut parts, &mut yy, icon::PALETTE, "Appearance", Part::Appearance);

    // The top bar: the page's title, and the search (typing anywhere goes to it).
    let title = if searching {
        "Search".to_owned()
    } else {
        match page {
            Page::Tab => MENU[tab].1.to_owned(),
            Page::Module(i) => s.modules.get(i).map_or_else(String::new, |m| if m.name == "ClickGUI" { "Appearance".to_owned() } else { m.name.to_owned() }),
        }
    };
    let main_x = x + SIDE + PAD + 2.0;
    out.push(Shape::Head(fit(&title, w * 0.3), [main_x, y + text_y(bar_top())], WHITE));
    let sw = (w * 0.34).min(190.0);
    let sr = [x + w - PAD - sw, y + (bar_top() - 17.0) / 2.0, sw, 17.0];
    let selected = menu.select_all && !menu.search.is_empty();
    out.push(Shape::Round(sr, round(sr[3]), if selected { 0xFF30_60B0 } else { pal().dash_control }));
    let (text, colour) = if menu.search.is_empty() { ("Search modules".to_owned(), MUTED) } else { (format!("{}_", menu.search), WHITE) };
    out.push(Shape::Icon(icon::SEARCH, [sr[0] + 7.0, sr[1] + text_y(sr[3]) + 0.5], 0xFF00_0000 | MUTED, 1.0));
    out.push(Shape::Text(fit(&text, sw - 28.0), [sr[0] + 20.0, sr[1] + text_y(sr[3])], colour));
    out.push(Shape::Fill([x + SIDE + px, y + bar_top(), w - SIDE - px, px], hair));
    // The title bar left of the search, and the name at the top of the sidebar: where the window is taken to move it.
    parts.push(([x, y, SIDE, bar_top()], Part::Title));
    parts.push(([x + SIDE, y, (sr[0] - x - SIDE - 4.0).max(0.0), bar_top()], Part::Title));

    // The pills: the tab's groups.
    if pill_bar {
        let (pills, open) = view(|v| (v.pills.clone(), v.groups[tab.min(TABS_MAX - 1)]));
        let ph = 16.0;
        let py = y + bar_top() + (pill_row() - ph) / 2.0 + 3.0;
        let mut px0 = main_x - 2.0;
        for g in pills {
            let label = MENU[tab].3[g].0;
            let lw = width(label);
            let pw = lw + 18.0;
            let r = [px0, py, pw, ph];
            let selected = g == open;
            if selected {
                out.push(Shape::Round(r, round(ph), tint(0xB0)));
                out.push(Shape::Round([r[0] + px, r[1] + px, r[2] - 2.0 * px, r[3] - 2.0 * px], (round(ph) - px).max(0.0), mix(surface | 0xFF00_0000, 0xFF00_0000 | accent(), 0.22)));
            } else {
                let l = lit(Part::Group(g));
                out.push(Shape::Round(r, round(ph), card));
                if l > 0 {
                    out.push(Shape::Round(r, round(ph), l << 24 | 0x00FF_FFFF));
                }
            }
            out.push(Shape::Text(label.to_owned(), [r[0] + 9.0, r[1] + text_y(ph)], if selected { WHITE } else { MUTED }));
            parts.push((r, Part::Group(g)));
            px0 += pw + 5.0;
        }
    }

    // The cards, scrolled and cut to their area.
    let c = content_in(f, pill_bar);
    let at = shown_scroll();
    let (cards, content_h) = view(|v| (v.cards.clone(), v.content));
    let card_radius = control_radius() + 2.0;
    out.push(Shape::ClipStart);
    // Part of what can be clicked only where the cards' area shows it.
    let clickable = |parts: &mut Vec<([f32; 4], Part)>, r: [f32; 4], p: Part| {
        let (y0, y1) = (r[1].max(c[1]), (r[1] + r[3]).min(c[1] + c[3]));
        if y1 > y0 {
            parts.push(([r[0], y0, r[2], y1 - y0], p));
        }
    };
    if let Page::Module(_) = page {
        let r = [c[0], c[1] - at, 60.0, back_h()];
        let l = lit(Part::Back);
        let colour = if l > 0 { WHITE } else { MUTED };
        out.push(Shape::Icon(icon::BACK, [r[0] + 2.0, r[1] + 2.5], 0xFF00_0000 | colour, 1.0));
        out.push(Shape::Text("Back".to_owned(), [r[0] + 14.0, r[1] + 2.0], colour));
        clickable(&mut parts, r, Part::Back);
    }
    if cards.is_empty() && page == Page::Tab {
        out.push(Shape::Text((if searching { "No modules match." } else { "Nothing here yet." }).to_owned(), [c[0] + 2.0, c[1] + 2.0], MUTED));
    }
    for card_at in &cards {
        let [cx, cy, cw, ch] = card_at.r;
        let r = [c[0] + cx, c[1] + cy - at, cw, ch];
        if r[1] + r[3] < c[1] || r[1] > c[1] + c[3] {
            continue;
        }
        out.push(Shape::Round(r, card_radius, card));
        match card_at.kind {
            Kind::Module { i, all, more } => {
                let m = &s.modules[i];
                let head = [r[0], r[1], r[2] - SWITCH[0] - 14.0, card_head()];
                let l = lit(Part::Open(i));
                if l > 0 {
                    out.push(Shape::Round([head[0] + 2.0, head[1] + 2.0, head[2] - 2.0, head[3] - 4.0], card_radius - 1.0, l << 24 | 0x00FF_FFFF));
                }
                let shown = fit(m.name, head[2] - 16.0);
                let nw = width(&shown);
                out.push(Shape::Head(shown, [r[0] + inset(), r[1] + text_y(card_head())], if m.on || switches_only(m) { WHITE } else { DIM }));
                if searching {
                    let (tab_name, _, _) = place_of(m.name);
                    if nw + width(tab_name) * 0.75 + 24.0 < head[2] {
                        out.push(Shape::Scaled(tab_name.to_owned(), [r[0] + inset() + 6.0 + nw, r[1] + text_y(card_head()) + 1.5], MUTED, 0.75));
                    }
                }
                clickable(&mut parts, head, Part::Open(i));
                if !no_switch(m) {
                    let sr = [r[0] + r[2] - 8.0 - SWITCH[0], r[1] + (card_head() - SWITCH[1]) / 2.0, SWITCH[0], SWITCH[1]];
                    switch(out, sr, m.on, lit(Part::Switch(i)));
                    clickable(&mut parts, [sr[0] - 4.0, r[1], SWITCH[0] + 8.0, card_head()], Part::Switch(i));
                }
                if !m.about.is_empty() {
                    let yy = r[1] + card_head();
                    out.push(Shape::Fill([r[0] + inset(), yy - 2.0, r[2] - 2.0 * inset(), px], hair));
                    out.push(Shape::Scaled(fit(m.about, (r[2] - 2.0 * inset()) / 0.8), [r[0] + inset(), yy + 1.0], MUTED, 0.8));
                }
                if more {
                    let fy = r[1] + r[3] - BOTTOM - foot_line();
                    let label = format!("All {all} settings");
                    let lw = width(&label);
                    let fr = [r[0] + inset() - 4.0, fy, lw + 20.0, foot_line()];
                    let colour = if lit(Part::All(i)) > 0 { WHITE } else { accent() };
                    out.push(Shape::Text(label, [fr[0] + 4.0, fy + text_y(foot_line())], colour));
                    out.push(Shape::Icon(icon::MORE, [fr[0] + 7.0 + lw, fy + text_y(foot_line()) + 0.5], 0xFF00_0000 | colour, 1.0));
                    clickable(&mut parts, fr, Part::All(i));
                }
            }
            Kind::Section(title) => {
                out.push(Shape::Scaled(title.to_owned(), [r[0] + inset(), r[1] + 5.0], MUTED, 0.85));
            }
            Kind::Intro(i) => {
                let m = &s.modules[i];
                let sr = [r[0] + r[2] - 8.0 - SWITCH[0], r[1] + (intro_h() - SWITCH[1]) / 2.0, SWITCH[0], SWITCH[1]];
                let state = if m.on { "Enabled" } else { "Disabled" };
                let stw = width(state);
                let about = if m.name == "ClickGUI" { "How the menu looks: its style, colours and size." } else { m.about };
                out.push(Shape::Text(fit(about, r[2] - SWITCH[0] - stw - 36.0), [r[0] + inset(), r[1] + text_y(intro_h())], DIM));
                if !no_switch(m) {
                    out.push(Shape::Text(state.to_owned(), [sr[0] - 6.0 - stw, r[1] + text_y(intro_h())], if m.on { WHITE } else { MUTED }));
                    switch(out, sr, m.on, lit(Part::Switch(i)));
                    clickable(&mut parts, [sr[0] - 4.0, r[1], SWITCH[0] + 8.0, intro_h()], Part::Switch(i));
                }
            }
        }
    }
    out.push(Shape::ClipEnd([c[0] - 1.0, c[1], c[0] + c[2] + 1.0, c[1] + c[3]]));

    // The scroll marker: how much of the page shows, and where.
    let room = c[3];
    if content_h > room && room > 0.0 {
        let thumb = (room * room / content_h).max(10.0);
        let t = (at / (content_h - room)).min(1.0);
        out.push(Shape::Round([x + w - 5.0, c[1] + (room - thumb) * t, 2.0, thumb], 1.0, 0x50FF_FFFF));
    }
    view(|v| v.parts = parts);
}

// ---------------------------------------------------------------- the tabs and their groups

type Group = (&'static str, &'static [&'static str]);

// The sidebar: (section, tab, icon, groups), each group's modules the most used first. A tab shows one group at a time
// (pills over its panels), so nothing is one long scroll. Modules not named here (a plugin's) go under Client > Other.
const MENU: &[(&str, &str, &str, &[Group])] = &[
    (
        "Combat",
        "Rage",
        "rage",
        &[
            (
                "Aura",
                &[
                    "Kill Aura", "Criticals", "Reach", "Hitboxes", "Backtrack", "Fake Lag", "Hit Through Walls", "Displace",
                    "Elytra Target", "PvP Bot",
                ],
            ),
            ("Crystal", &["Crystal Aura", "Anchor Aura", "Crystal Optimizer", "Cart", "Anvil"]),
            ("Trap", &["Auto Trap", "Self Trap", "BlockIn", "Auto Web", "Self Web"]),
            ("Support", &["Offhand", "Auto EXP", "Auto Gap", "Quiver", "Multitask", "Friend Guard"]),
        ],
    ),
    (
        "Combat",
        "Legit",
        "legit",
        &[
            ("Aim", &["Aim Assist", "Trigger Bot", "Bow Aimbot", "Auto Clicker"]),
            ("Weapons", &["Auto Weapon", "Auto Mace", "Auto Spear", "Shield", "Tap Use", "Eat Tap"]),
            ("Knockback", &["Velocity", "W Tap", "Jump Reset", "Snap Tap", "RF Click"]),
        ],
    ),
    (
        "Visuals",
        "ESP",
        "esp",
        &[
            ("Players", &["ESP", "Tracers", "Nametags", "Gear ESP", "Radar", "Use Tracker", "Logout Spots", "Trajectories"]),
            (
                "Blocks",
                &["Block ESP", "Storage ESP", "Hole ESP", "Void ESP", "Tunnel ESP", "Light ESP", "Xray", "Light Overlay"],
            ),
        ],
    ),
    (
        "Visuals",
        "Visuals",
        "visuals",
        &[
            (
                "World",
                &[
                    "Fullbright", "Atmosphere", "Clear Water", "No Render", "Time Changer", "Weather Changer", "Effects",
                    "Breadcrumbs",
                ],
            ),
            ("Camera", &["Zoom", "Free Look", "Freecam", "Hand View", "Crosshair", "Block Overlay"]),
            ("HUD", &["HUD", "Waypoints", "Shulker Peek"]),
        ],
    ),
    (
        "General",
        "Movement",
        "movement",
        &[
            ("Ground", &["Sprint", "Pearl Catch", "Speed", "Step", "Safe Walk", "Sneak", "Auto Walk", "Auto Jump", "Parkour"]),
            ("Control", &["No Slow", "No Jump Delay", "Jesus", "Slippy", "Fast Climb", "Spider", "GUI Move"]),
            ("Air", &["Flight", "Glide", "High Jump", "Long Jump", "Air Jump", "No Fall", "Anti Void", "Blink", "Click TP"]),
            (
                "Travel",
                &["Elytra Fly", "Chest Swap", "Trident Boost", "Auto Trident", "Entity Control", "Wind Jump", "Timer"],
            ),
        ],
    ),
    (
        "General",
        "Utility",
        "utility",
        &[
            (
                "Inventory",
                &[
                    "Inventory Manager", "Chest Stealer", "Auto Replenish", "Auto Tool", "Loadouts", "Client Sort",
                    "Middle Click Extra", "Ghost Hand",
                ],
            ),
            (
                "Player",
                &[
                    "Fake Player", "Auto Eat", "Auto Respawn", "Auto Log", "Anti AFK", "Anti Hunger", "No Status Effects",
                    "Rotations", "Portals", "Death Location", "Depth Alert", "Spam",
                ],
            ),
            (
                "Mining",
                &[
                    "Fast Break", "Fast Place", "Fast Use", "Break Delay", "No Break Reset", "Packet Mine", "Nuker", "Vein Miner",
                    "Mine Through Mobs",
                ],
            ),
            (
                "Building",
                &["Scaffold", "Air Place", "Build Height", "Liquid Interact", "Liquid Filler", "Spawn Proofer", "No Interact", "Auto Sign"],
            ),
            (
                "Automation",
                &[
                    "Baritone", "Auto Fish", "Auto Breed", "Auto Mount", "Auto Nametag", "Auto Shearer", "Flamethrower",
                    "Enderman Look", "Villager Roller", "Auction Sniper", "Auto Sell", "Unlock All Recipes",
                ],
            ),
        ],
    ),
    (
        "General",
        "Server",
        "server",
        &[
            ("Intel", &["Server Scan", "Staff List", "Anti Vanish", "Notifier", "Base Guard", "Sound Detector", "Client Spoofer"]),
            (
                "Hunting",
                &[
                    "Base Finder", "Stash Finder", "New Chunks", "Player Activity Detector", "Activated Spawner Detector",
                    "Cave Disturbance Detector", "Portal Pattern Finder", "Drill Logger", "Sign Historian", "Chest Index",
                ],
            ),
            ("Seed", &["Ore Sim", "Geode Finder", "Spawner Alerts"]),
        ],
    ),
    (
        "Client",
        "Client",
        "client",
        &[
            (
                "Client",
                &[
                    "Profiles", "ClickGUI", "Modules", "Notifications", "Streamer Mode", "Sunset Presence", "Debug Report", "Panic",
                    "Gooning",
                ],
            ),
            ("Other", &[]),
        ],
    ),
];

// Where a module is listed: its tab, its group, and a key that sorts it there (tab, group, place in the group).
fn place_of(name: &str) -> (&'static str, &'static str, (usize, usize, usize)) {
    for (t, (_, tab, _, groups)) in MENU.iter().enumerate() {
        for (g, (group, names)) in groups.iter().enumerate() {
            if let Some(k) = names.iter().position(|n| *n == name) {
                return (tab, group, (t, g, k));
            }
        }
    }
    ("Client", "Other", (MENU.len() - 1, usize::MAX, 0))
}

// ---------------------------------------------------------------- tabs for long settings lists

// Lists longer than this are split into tabs.
const LONG: usize = 14;

// ESP's settings by tab, the most used first; any not named here go under More.
const ESP_TABS: &[(&str, &[&str])] = &[
    (
        "General",
        &[
            "Mode", "Players color", "Health bar", "Players", "Hostiles", "Passive", "Hostiles color", "Passive color",
            "Show friend colors",
            "Shape mode", "Fill opacity", "Fade distance", "Range", "Max",
        ],
    ),
    ("2D", &["2D style", "Health side", "Health text", "2D names", "2D distance", "Glow"]),
    ("Filters", &["List mode", "Entities", "Shield colors", "Shield mode", "Shield color", "Broken shield color"]),
    ("Villagers", &["Villager filter", "Villager babies", "Villager profession colors", "Villager level label"]),
    ("Items", &["Items", "Item names", "Item tracers", "Item filter", "Item list", "Items color"]),
    (
        "Murder",
        &[
            "Murderer finder", "Murderer items", "Murderer color", "Murderer list", "Murderer list position",
            "Murderer list x", "Murderer list y", "Callout", "Callout delay (ms)", "Callout messages",
        ],
    ),
];

// A long settings list as tabs: by a hand-made layout (ESP), else by the module's own sections, else its first ten
// (the catalogs list the main ones first) and the rest under More. Short lists: no tabs.
fn tabs_of(m: &Module) -> Vec<(&'static str, Vec<usize>)> {
    let n = m.settings.len();
    if n <= LONG {
        return Vec::new();
    }
    let layout = match m.name {
        "ESP" => Some(ESP_TABS),
        _ => None,
    };
    if let Some(layout) = layout {
        let mut tabs: Vec<(&'static str, Vec<usize>)> = layout.iter().map(|(title, _)| (*title, Vec::new())).collect();
        let mut more = Vec::new();
        for j in 0..n {
            match layout.iter().position(|(_, names)| names.contains(&m.settings[j].name)) {
                Some(k) => tabs[k].1.push(j),
                None => more.push(j),
            }
        }
        if !more.is_empty() {
            tabs.push(("More", more));
        }
        return tabs;
    }
    if let Some(&(first, _)) = m.groups.first() {
        let mut tabs = Vec::new();
        if first > 0 {
            tabs.push(("General", (0..first).collect()));
        }
        for (k, &(start, title)) in m.groups.iter().enumerate() {
            let end = m.groups.get(k + 1).map_or(n, |g| g.0);
            tabs.push((title, (start..end).collect()));
        }
        return tabs;
    }
    vec![("General", (0..10).collect()), ("More", (10..n).collect())]
}
