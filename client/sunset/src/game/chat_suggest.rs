// The chat's suggestions for Sunset's . commands, shown and picked as the game's own are for / commands
// (CommandSuggestions): `.` lists the commands, then each argument's choices (modules, settings and their values,
// profiles, players, keys...) as you type. Tab puts the selected one in (again: the next), Up and Down move, a click
// picks one, the wheel scrolls, Escape hides the list until the next edit; the selected one's description shows beside
// it. With no list, a grey line shows the arguments still to come, or a red one the word that can't be read.
//
// The grammar is command_complete.rs; this is its state, keys, mouse and drawing. Hooks: chat_suggest_*.rs in
// src/mixins, and the frame hook (sky_frame.rs), which keeps the list and the grey completion after the cursor in
// step with the box before each frame is drawn.
use crate::game::sunset_compat::SunsetGame as _;

use super::sunset::{Value, with};
use super::sunset_command_complete::{self as grammar, Completion, Kind, ModuleInfo, SettingInfo, Source, Suggestion};
use super::{Game, Screen};
use horizon_client::hud::HudBatch;
use horizon_client::screen_draw::{self as d, Measure};
use horizon_client::text::Text;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

// CommandSuggestions in the chat: ten lines and its fill; SuggestionsList's colours.
const LINES: usize = 10;
const FILL: i32 = -805306368;
const SELECTED: i32 = -256;
const OTHER: i32 = -5592406;
const TIP: i32 = 0xFFA0_A0A0_u32 as i32;

// The list on screen (SuggestionsList).
struct List {
    // The box's text when it was made; a suggestion replaces `start..end` of it.
    original: String,
    start: usize,
    end: usize,
    list: Vec<Suggestion>,
    offset: usize,
    current: usize,
    // Tab moves to the next one before putting it in (after a first Tab).
    tab_cycles: bool,
    last_mouse: (i32, i32),
    // Where it was drawn last: x, y, width, height.
    rect: [i32; 4],
}

impl List {
    fn new(original: &str, c: &Completion) -> Self {
        Self {
            original: original.to_owned(),
            start: c.start,
            end: c.end,
            list: c.list.clone(),
            offset: 0,
            current: 0,
            tab_cycles: false,
            last_mouse: (i32::MIN, i32::MIN),
            rect: [0; 4],
        }
    }
    // The box's text with suggestion `i` in.
    fn applied(&self, i: usize) -> String {
        format!("{}{}{}", &self.original[..self.start], self.list[i].text, &self.original[self.end..])
    }
    fn cycle(&mut self, by: isize) {
        let n = self.list.len() as isize;
        self.current = (self.current as isize + by).rem_euclid(n) as usize;
        let max = self.list.len().saturating_sub(LINES);
        if self.current < self.offset {
            self.offset = self.current.min(max);
        } else if self.current >= self.offset + LINES {
            self.offset = (self.current + 1 - LINES).min(max);
        }
    }
    fn contains(&self, (x, y): (i32, i32)) -> bool {
        let [rx, ry, w, h] = self.rect;
        x >= rx && x <= rx + w && y >= ry && y <= ry + h
    }
}

#[derive(Default)]
struct Suggest {
    // The box's text and cursor (in characters) `info` and `list` were made for.
    seen: Option<(String, usize)>,
    info: Option<Completion>,
    // None: hidden (Escape, or the Command Suggestions option off until Tab).
    list: Option<List>,
    // The text a Tab or click made: the list stays for the next Tab.
    applied: Option<String>,
}

static STATE: Mutex<Option<Suggest>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut Suggest) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(Suggest::default))
}

fn byte_at(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}

// The chat box holds a Sunset command and Sunset reads commands (as runtime_commands.rs `chat` does).
fn active(game: &Game<'_>) -> bool {
    game.screen == Screen::Chat
        && game.ui.chat_screen.as_ref().is_some_and(|s| s.input.value.starts_with('.') && !s.input.value.starts_with(".."))
        && super::sunset::running()
        && game.sunset_window()
        && game.native_on(super::sunset::ID)
}

// Before each frame: the suggestions for what the box holds now, and the grey completion after the cursor.
pub(crate) fn frame(game: &mut Game<'_>) {
    if !active(game) {
        state(|s| {
            if s.seen.is_some() {
                *s = Suggest::default();
            }
        });
        return;
    }
    refresh(game);
    ghost(game);
}

fn refresh(game: &Game<'_>) {
    let Some(screen) = game.ui.chat_screen.as_ref() else {
        return;
    };
    let now = (screen.input.value.clone(), screen.input.cursor);
    let (stale, keep) = state(|s| (s.seen.as_ref() != Some(&now), s.applied.as_deref() == Some(now.0.as_str()) && s.list.is_some()));
    if !stale {
        return;
    }
    if keep {
        state(|s| s.seen = Some(now));
        return;
    }
    let info = grammar::complete(&now.0, byte_at(&now.0, now.1), &Live(game));
    let show = screen.auto_suggestions;
    state(|s| {
        s.applied = None;
        s.list = info.as_ref().filter(|i| show && !i.list.is_empty()).map(|i| List::new(&now.0, i));
        s.info = info;
        s.seen = Some(now);
    });
}

// The rest of the selected suggestion, grey after the cursor (SuggestionsList.select). While the box holds a Sunset
// command this is Sunset's: the game sets none for text that is not a / command.
fn ghost(game: &mut Game<'_>) {
    let applied = state(|s| s.list.as_ref().map(|l| l.applied(l.current)));
    if let Some(screen) = game.ui.chat_screen.as_mut() {
        let value = &screen.input.value;
        screen.input.suggestion = applied.and_then(|a| a.strip_prefix(value.as_str()).filter(|r| !r.is_empty()).map(str::to_owned));
    }
}

// Tab or a click: the selected suggestion into the box, the cursor after it (SuggestionsList.useSuggestion).
fn use_current(game: &mut Game<'_>) {
    let Some((value, cursor)) = state(|s| {
        let l = s.list.as_mut()?;
        let value = l.applied(l.current);
        let cursor = l.original[..l.start].chars().count() + l.list[l.current].text.chars().count();
        l.tab_cycles = true;
        Some((value, cursor))
    }) else {
        return;
    };
    let m = Measure(&game.gui.atlas.font);
    // Only a / command is parsed with the tree; the box's other text only completes players.
    let c = horizon_client::chat::Commands {
        dispatcher: horizon_client::command::vanilla::game_dispatcher(),
        registries: &game.ui.registries,
    };
    let Some(screen) = game.ui.chat_screen.as_mut() else {
        return;
    };
    screen.edit_input(&m, &c, |field| {
        field.set_value(&m, &value);
        field.set_cursor_and_highlight(&m, cursor);
    });
    // The box may hold less than asked (its length limit): then the next frame makes a new list.
    let now = (screen.input.value.clone(), screen.input.cursor);
    state(|s| {
        s.applied = Some(now.0.clone());
        s.seen = Some(now);
    });
}

// A key in the chat box: whether the suggestions took it.
pub(crate) fn key(game: &mut Game<'_>, key: KeyCode) -> bool {
    if !active(game) {
        return false;
    }
    refresh(game);
    let shift = game.has_shift_down();
    let taken = match key {
        // The game would complete players' names: a Sunset command's Tab is always its own.
        KeyCode::Tab => {
            let shown = state(|s| {
                if let Some(l) = s.list.as_mut() {
                    if l.tab_cycles {
                        l.cycle(if shift { -1 } else { 1 });
                    }
                    return true;
                }
                // The first Tab shows a hidden list (CommandSuggestions.showSuggestions); the next puts one in.
                if let (Some((value, _)), Some(info)) = (s.seen.as_ref(), s.info.as_ref().filter(|i| !i.list.is_empty())) {
                    s.list = Some(List::new(value, info));
                }
                false
            });
            if shown {
                use_current(game);
            }
            true
        }
        KeyCode::ArrowUp | KeyCode::ArrowDown => state(|s| {
            s.list.as_mut().map(|l| {
                l.cycle(if key == KeyCode::ArrowUp { -1 } else { 1 });
                l.tab_cycles = false;
            })
        })
        .is_some(),
        KeyCode::Escape => state(|s| s.list.take().is_some()),
        _ => false,
    };
    if taken {
        ghost(game);
    }
    taken
}

fn mouse(game: &Game<'_>) -> (i32, i32) {
    (game.cursor.0.floor() as i32, game.cursor.1.floor() as i32)
}

// A left click on the chat screen: whether it picked a suggestion.
pub(crate) fn click(game: &mut Game<'_>) -> bool {
    if !active(game) {
        return false;
    }
    let at = mouse(game);
    let picked = state(|s| {
        let l = s.list.as_mut()?;
        if !l.contains(at) {
            return None;
        }
        let row = ((at.1 - l.rect[1]) / 12).max(0) as usize + l.offset;
        (row < l.list.len()).then(|| l.current = row)
    })
    .is_some();
    if picked {
        use_current(game);
        ghost(game);
    }
    picked
}

// The wheel over the list scrolls it (SuggestionsList.mouseScrolled).
pub(crate) fn scroll(game: &Game<'_>, amount: f32) -> bool {
    if !active(game) {
        return false;
    }
    let at = mouse(game);
    state(|s| {
        let l = s.list.as_mut().filter(|l| l.contains(at))?;
        let max = l.list.len().saturating_sub(LINES) as f32;
        l.offset = (l.offset as f32 - amount).clamp(0.0, max) as usize;
        Some(())
    })
    .is_some()
}

// Over the chat screen, after its box (the screen's foreground phase).
pub(crate) fn draw(game: &Game<'_>, batch: &mut HudBatch) {
    if !active(game) {
        return;
    }
    let Some(screen) = game.ui.chat_screen.as_ref() else {
        return;
    };
    let m = Measure(&game.gui.atlas.font);
    let input = &screen.input;
    let (w, h) = (batch.width as i32, batch.height as i32);
    let mouse = mouse(game);
    // The x of byte `at` of `text` in the box, moved left so `width` fits in it (CommandSuggestions' clamp: the left
    // edge wins).
    let column = |text: &str, at: usize, width: i32| {
        let x = input.screen_x(&m, text[..at.min(text.len())].chars().count());
        let right = input.screen_x(&m, 0) + input.inner_width() - width;
        if x < 0 { 0 } else { x.min(right.max(0)) }
    };
    let mut out = Vec::new();
    state(|s| {
        if let Some(l) = s.list.as_mut() {
            let x = column(&l.original, l.start, l.list.iter().map(|s| m.width_str(&s.text)).max().unwrap_or(0));
            draw_list(&m, l, x, h, w, mouse, &mut out);
        } else if let Some(info) = &s.info {
            let line = match (&info.error, &info.usage) {
                (Some(e), _) => Some(Text::colored(e.clone(), horizon_client::text::RED)),
                (None, Some(u)) => Some(Text::colored(u.clone(), horizon_client::text::GRAY)),
                _ => None,
            };
            if let Some(line) = line {
                let value = &input.value;
                let width = m.width(&line);
                let x = column(value, info.usage_at.min(value.len()), width);
                let y = h - 27;
                d::fill(&mut out, x - 1, y, x + width + 1, y + 12, FILL);
                d::text(&mut out, line, x, y + 2, -1, true);
            }
        }
    });
    if !out.is_empty() {
        let tooltip = d::paint(batch, &game.gui.atlas, &out);
        d::paint_tooltip(batch, &game.gui.atlas, &out, tooltip);
    }
}

// SuggestionsList.extractRenderState over the box, and the selected one's description to its right.
fn draw_list(m: &Measure, l: &mut List, x: i32, h: i32, w: i32, mouse: (i32, i32), out: &mut Vec<d::Draw>) {
    let width = l.list.iter().map(|s| m.width_str(&s.text)).max().unwrap_or(0);
    let n = l.list.len().min(LINES);
    // The chat's box has no border: the list starts a pixel left of its text.
    let (rx, rw, rh) = (x - 1, width + 1, n as i32 * 12);
    let ry = h - 12 - 3 - rh;
    l.rect = [rx, ry, rw, rh];
    if mouse.0 >= rx && mouse.0 < rx + rw && mouse.1 >= ry && mouse.1 < ry + rh {
        horizon_client::widgets::request_cursor(horizon_client::widgets::Cursor::PointingHand);
    }
    let moved = l.last_mouse != mouse;
    l.last_mouse = mouse;
    let (more_above, more_below) = (l.offset > 0, l.list.len() > l.offset + n);
    if more_above || more_below {
        d::fill(out, rx, ry - 1, rx + rw, ry, FILL);
        d::fill(out, rx, ry + rh, rx + rw, ry + rh + 1, FILL);
        for dx in (0..rw).step_by(2) {
            if more_above {
                d::fill(out, rx + dx, ry - 1, rx + dx + 1, ry, -1);
            }
            if more_below {
                d::fill(out, rx + dx, ry + rh, rx + dx + 1, ry + rh + 1, -1);
            }
        }
    }
    for i in 0..n {
        let y = ry + 12 * i as i32;
        d::fill(out, rx, y, rx + rw, y + 12, FILL);
        if moved && mouse.0 > rx && mouse.0 < rx + rw && mouse.1 > y && mouse.1 < y + 12 {
            l.current = i + l.offset;
        }
        let s = &l.list[i + l.offset];
        let colour = if i + l.offset == l.current { SELECTED } else { OTHER };
        d::text(out, Text::literal(s.text.clone()), rx + 1, y + 2, colour, true);
    }
    // What the selected one is or does, beside its line, cut to the screen.
    let row = l.current.wrapping_sub(l.offset);
    let tip = &l.list[l.current].tip;
    let tx = rx + rw + 3;
    let room = w - 2 - tx - 2;
    if row < n && !tip.is_empty() && room > 24 {
        let text = fit(m, tip, room);
        let tw = m.width_str(&text);
        let y = ry + 12 * row as i32;
        d::fill(out, tx - 1, y, tx + tw + 1, y + 12, FILL);
        d::text(out, Text::literal(text), tx, y + 2, TIP, true);
    }
}

// `text`, cut with an ellipsis to `room` pixels.
fn fit(m: &Measure, text: &str, room: i32) -> String {
    if m.width_str(text) <= room {
        return text.to_owned();
    }
    let mut out: String = text.to_owned();
    while !out.is_empty() && m.width_str(&format!("{out}...")) > room {
        out.pop();
    }
    format!("{}...", out.trim_end())
}

// What the grammar reads, from Sunset's state and the game.
struct Live<'a, 'b>(&'a Game<'b>);

fn module_info(m: &super::sunset::Module) -> ModuleInfo {
    ModuleInfo { name: m.name.to_owned(), about: m.about.to_owned(), on: m.on, key: m.key.clone() }
}

impl Live<'_, '_> {
    fn store(&self) -> Option<super::sunset_profiles::Store> {
        super::sunset_profiles::Store::new(self.0.native_data_dir(super::sunset::ID)).ok()
    }
}

impl Source for Live<'_, '_> {
    // The menus' modules: not the ones switched off in Modules (`.t` still finds those).
    fn modules(&self) -> Vec<ModuleInfo> {
        with(|s| {
            let listed = s.listed();
            s.modules.iter().filter(|m| listed(m)).map(module_info).collect()
        })
    }
    fn find_module(&self, word: &str) -> Option<ModuleInfo> {
        with(|s| s.find(word).map(|i| module_info(&s.modules[i])))
    }
    fn settings(&self, module: &str) -> Vec<SettingInfo> {
        with(|s| {
            s.module(module).map_or_else(Vec::new, |m| {
                m.settings
                    .iter()
                    .map(|st| SettingInfo {
                        name: st.name.to_owned(),
                        kind: match &st.value {
                            Value::Bool(b) => Kind::Bool(*b),
                            Value::Num { v, min, max, step } => Kind::Number { now: *v, min: *min, max: *max, step: *step },
                            Value::Pick { i, of } => Kind::Pick { now: *i, of: of.iter().map(|o| (*o).to_owned()).collect() },
                            Value::Text(t) => Kind::Text(t.clone()),
                        },
                    })
                    .collect()
            })
        })
    }
    fn profiles(&self) -> Vec<String> {
        self.store().and_then(|s| s.list().ok()).unwrap_or_default()
    }
    fn imports(&self, extension: &str) -> Vec<String> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        let Ok(dir) = std::fs::read_dir(store.imports()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = dir
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                (path.extension().and_then(|x| x.to_str()) == Some(extension))
                    .then(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
                    .flatten()
            })
            .take(200)
            .collect();
        names.sort();
        names
    }
    fn presets(&self) -> Vec<(String, String)> {
        super::sunset_presets::PRESETS.iter().map(|p| (p.id.to_owned(), p.about.to_owned())).collect()
    }
    fn friends(&self) -> Vec<String> {
        with(|s| s.friends.clone())
    }
    fn players(&self) -> Vec<String> {
        self.0.ui.registries.players.clone()
    }
    fn waypoints(&self) -> Vec<String> {
        with(|s| s.waypoints.iter().map(|w| w.name.clone()).collect())
    }
    fn loadouts(&self) -> Vec<String> {
        super::sunset_loadouts::names()
    }
    fn plugin_commands(&self) -> Vec<(String, String)> {
        super::sunset_api::help().into_iter().map(|(name, about)| (name.trim_start_matches('.').to_owned(), about.to_owned())).collect()
    }
    fn position(&self) -> Option<[i32; 3]> {
        self.0.native_player().map(|p| [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32])
    }
}
