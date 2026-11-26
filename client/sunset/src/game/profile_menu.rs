// The list the Profiles module's row opens: the saved profiles, (its Presets tab) the pre-made ones, or (its Import
// tab) the .profile files waiting in the imports folder, with a button that takes the profile on the clipboard. A
// click, or the arrows and Enter, loads a saved one; a preset first shows what taking it turns on and off, with Apply
// and Back; an import goes in among the saved ones (a name that is taken asks: keep both, or replace).
// Save (under the saved ones) writes the settings as they are now to the profile in use. Left and Right change tabs. The files are read when it opens and when one loads, never while drawing.
use crate::game::sunset_compat::SunsetGame as _;
use super::{Game, sunset::{Shape, accent, panel_radius}};
use super::sunset_presets::PRESETS;

const ROW: f32 = 18.0;
// The list's title above it and the room under it.
const TOP: f32 = 25.0;
const BOTTOM: f32 = 22.0;
// A preview's lines, and its buttons' row.
const LINE: f32 = 11.0;
const BUTTONS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Tab {
    Saved,
    Presets,
    Import,
}

// What a click or a key in the menu asks for.
pub(super) enum Choice {
    Load(String),
    Save(String),
    Preview(usize),
    Apply(usize),
    // A file of the imports folder taken in; the same with the answer when its name is taken (true: over the saved
    // one, false: beside it, numbered); and the profile on the clipboard.
    Import(String),
    ImportAs(String, bool),
    Paste,
}

// A preset's preview: its lines (text, colour), fitted to the menu's width when it was made.
#[derive(Clone, Debug)]
pub(super) struct Preview {
    pub(super) preset: usize,
    pub(super) lines: Vec<(String, u32)>,
}

#[derive(Clone, Debug)]
pub(super) struct Menu {
    pub(super) names: Vec<String>,
    // The .profile files in the imports folder.
    imports: Vec<String>,
    pub(super) error: Option<String>,
    // What the last import or paste did.
    note: Option<String>,
    // An import whose name is taken: the menu asks whether to keep both or replace.
    pub(super) ask: Option<String>,
    // The file that question is about when it is not one of the imports folder: one dropped on the menu.
    pub(super) ask_file: Option<std::path::PathBuf>,
    pub(super) tab: Tab,
    pub(super) preview: Option<Preview>,
    // The saved profile in use, which Save writes to.
    in_use: Option<String>,
    focus: usize,
    start: usize,
}

pub(super) fn width(w: f32) -> f32 {
    (w - 16.0).min(300.0)
}

fn frame(w: f32, h: f32, count: usize) -> [f32; 4] {
    let ww = width(w);
    let hh = (count.clamp(1, 10) as f32 * ROW + TOP + BOTTOM).min(h - 16.0);
    [(w - ww) / 2.0, (h - hh) / 2.0, ww, hh]
}

fn preview_frame(w: f32, h: f32, lines: usize) -> [f32; 4] {
    let ww = width(w);
    let hh = (lines as f32 * LINE + TOP + BUTTONS + 6.0).min(h - 16.0);
    [(w - ww) / 2.0, (h - hh) / 2.0, ww, hh]
}

// How many preview lines fit.
fn preview_rows(r: [f32; 4]) -> usize {
    ((r[3] - TOP - BUTTONS - 6.0) / LINE).max(1.0) as usize
}

// The preview's Apply and Back buttons.
fn buttons(r: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let (bw, bh) = (60.0, 16.0);
    let y = r[1] + r[3] - bh - 8.0;
    ([r[0] + r[2] - 2.0 * bw - 14.0, y, bw, bh], [r[0] + r[2] - bw - 8.0, y, bw, bh])
}

// The Save button, at the foot of the Saved tab.
fn save_button(r: [f32; 4]) -> [f32; 4] {
    [r[0] + r[2] - 98.0, r[1] + r[3] - 19.0, 90.0, 15.0]
}

// Paste a profile, at the foot of the Import tab; and the two answers when an import's name is taken.
fn paste_button(r: [f32; 4]) -> [f32; 4] {
    [r[0] + r[2] - 118.0, r[1] + r[3] - 19.0, 110.0, 15.0]
}

fn ask_buttons(r: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let y = r[1] + r[3] - 19.0;
    ([r[0] + r[2] - 132.0, y, 64.0, 15.0], [r[0] + r[2] - 64.0, y, 56.0, 15.0])
}

// The three tabs in the title row.
fn tabs(r: [f32; 4]) -> [([f32; 4], Tab, &'static str); 3] {
    let (x, y) = (r[0] + r[2], r[1] + 4.0);
    [([x - 190.0, y, 50.0, 15.0], Tab::Saved, "Saved"), ([x - 138.0, y, 56.0, 15.0], Tab::Presets, "Presets"), ([x - 80.0, y, 52.0, 15.0], Tab::Import, "Import")]
}

// How many rows fit.
fn rows(r: [f32; 4]) -> usize {
    ((r[3] - TOP - BOTTOM) / ROW).max(1.0) as usize
}

fn inside(r: [f32; 4], at: (f32, f32)) -> bool {
    at.0 >= r[0] && at.0 < r[0] + r[2] && at.1 >= r[1] && at.1 < r[1] + r[3]
}

impl Menu {
    // On the saved profiles, or the presets when none is saved.
    pub(super) fn new(names: Vec<String>, imports: Vec<String>, selected: Option<&str>, w: f32, h: f32) -> Self {
        let tab = if names.is_empty() { Tab::Presets } else { Tab::Saved };
        let focus = selected.and_then(|n| names.iter().position(|p| p == n)).unwrap_or(0);
        let in_use = selected.filter(|n| names.iter().any(|p| p == n)).map(str::to_owned);
        let mut menu = Self { names, imports, error: None, note: None, ask: None, ask_file: None, tab, preview: None, in_use, focus, start: 0 };
        menu.reveal(w, h);
        menu
    }

    fn count(&self) -> usize {
        match self.tab {
            Tab::Saved => self.names.len(),
            Tab::Presets => PRESETS.len(),
            Tab::Import => self.imports.len(),
        }
    }

    // The list's frame: tall enough for the longest of its three tabs (an empty Import tab says where files go, in
    // two lines), so the menu keeps its height and the tabs their place when one is clicked.
    fn list_frame(&self, w: f32, h: f32) -> [f32; 4] {
        frame(w, h, self.names.len().max(PRESETS.len()).max(self.imports.len().max(2)))
    }

    fn rect(&self, w: f32, h: f32) -> [f32; 4] {
        match &self.preview {
            Some(p) => preview_frame(w, h, p.lines.len()),
            None => self.list_frame(w, h),
        }
    }

    fn reveal(&mut self, w: f32, h: f32) {
        let count = rows(self.list_frame(w, h));
        self.start = self.start.min(self.focus).max((self.focus + 1).saturating_sub(count));
    }

    fn switch(&mut self, tab: Tab, w: f32, h: f32) {
        if self.tab != tab {
            self.tab = tab;
            self.focus = 0;
            self.start = 0;
            self.error = None;
            self.note = None;
            self.ask = None;
            self.ask_file = None;
            self.reveal(w, h);
        }
    }

    // A file whose name is taken: the Import tab asks (keep both, or replace). `file`: one dropped on the menu.
    pub(super) fn asks(&mut self, name: String, file: Option<std::path::PathBuf>, w: f32, h: f32) {
        self.preview = None;
        self.switch(Tab::Import, w, h);
        self.error = None;
        self.note = None;
        self.ask = Some(name);
        self.ask_file = file;
    }

    // An import or a paste went in: the saved profiles again with the new one marked, and what happened.
    pub(super) fn took(&mut self, names: Vec<String>, landed: Option<&str>, note: String, w: f32, h: f32) {
        self.names = names;
        self.tab = Tab::Saved;
        self.focus = landed.and_then(|n| self.names.iter().position(|p| p == n)).unwrap_or(0);
        self.start = 0;
        self.error = None;
        self.ask = None;
        self.ask_file = None;
        self.note = Some(note);
        self.reveal(w, h);
    }

    // The row `i` of the tab shown: a profile to load, a preset to preview or a file to import.
    fn choice(&self, i: usize) -> Option<Choice> {
        match self.tab {
            Tab::Saved => self.names.get(i).cloned().map(Choice::Load),
            Tab::Presets => (i < PRESETS.len()).then_some(Choice::Preview(i)),
            Tab::Import => self.imports.get(i).cloned().map(Choice::Import),
        }
    }

    pub(super) fn scroll(&mut self, steps: i32, w: f32, h: f32) {
        if self.preview.is_some() {
            return;
        }
        let max = self.count().saturating_sub(rows(self.list_frame(w, h)));
        self.start = (self.start as i32 + steps).clamp(0, max as i32) as usize;
    }

    // Esc: out of a preview or a question (true), else the menu closes.
    pub(super) fn back(&mut self) -> bool {
        self.error = None;
        self.ask_file = None;
        self.ask.take().is_some() || self.preview.take().is_some()
    }

    // Up and Down move the mark, Left and Right change tabs, Enter loads or previews (and in a preview, applies).
    pub(super) fn key(&mut self, key: &str, w: f32, h: f32) -> Option<Choice> {
        let enter = matches!(key, "key.keyboard.enter" | "key.keyboard.numpad.enter");
        if let Some(p) = &self.preview {
            return enter.then_some(Choice::Apply(p.preset));
        }
        match key {
            "key.keyboard.up" => self.focus = self.focus.saturating_sub(1),
            "key.keyboard.down" => self.focus = (self.focus + 1).min(self.count().saturating_sub(1)),
            "key.keyboard.left" => self.switch(if self.tab == Tab::Import { Tab::Presets } else { Tab::Saved }, w, h),
            "key.keyboard.right" => self.switch(if self.tab == Tab::Saved { Tab::Presets } else { Tab::Import }, w, h),
            _ if enter => return self.choice(self.focus),
            _ => return None,
        }
        self.reveal(w, h);
        None
    }

    // What a click asks for, if anything (a tab or Back is done here).
    pub(super) fn click(&mut self, at: (f32, f32), w: f32, h: f32) -> Option<Choice> {
        let r = self.rect(w, h);
        if let Some(p) = &self.preview {
            let (apply, back) = buttons(r);
            if inside(apply, at) {
                return Some(Choice::Apply(p.preset));
            }
            if inside(back, at) {
                self.back();
            }
            return None;
        }
        if let Some((_, tab, _)) = tabs(r).into_iter().find(|(t, _, _)| inside(*t, at)) {
            self.switch(tab, w, h);
            return None;
        }
        if self.tab == Tab::Saved && let Some(name) = &self.in_use && inside(save_button(r), at) {
            return Some(Choice::Save(name.clone()));
        }
        if self.tab == Tab::Import {
            if let Some(name) = &self.ask {
                let (keep, replace) = ask_buttons(r);
                if inside(keep, at) || inside(replace, at) {
                    return Some(Choice::ImportAs(name.clone(), inside(replace, at)));
                }
            } else if inside(paste_button(r), at) {
                return Some(Choice::Paste);
            }
        }
        let list = [r[0] + 4.0, r[1] + TOP, r[2] - 8.0, rows(r) as f32 * ROW];
        inside(list, at).then(|| self.choice(self.start + ((at.1 - list[1]) / ROW) as usize)).flatten()
    }

    // A click outside the menu or on its x closes it.
    pub(super) fn closes(&self, at: (f32, f32), w: f32, h: f32) -> bool {
        let r = self.rect(w, h);
        !inside(r, at) || inside([r[0] + r[2] - 25.0, r[1], 25.0, 22.0], at)
    }
}

fn fit(game: &Game<'_>, text: &str, width: f32) -> String {
    super::sunset::fit_to(game, text, width)
}

// The preview's lines at the menu's width: the preset's about (its title is the head), then `lines` (what it turns on
// and off, and the settings count).
pub(super) fn preview_lines(game: &Game<'_>, w: f32, preset: usize, lines: &[String]) -> Vec<(String, u32)> {
    let room = width(w) - 16.0;
    let about = super::sunset::wrap(game, PRESETS[preset].about, room);
    let mut out: Vec<(String, u32)> = about.into_iter().map(|l| (l, 0x909090)).collect();
    for line in lines {
        let colour = if line.starts_with("Turns on") {
            accent()
        } else if line.starts_with("Turns off") {
            0xFF9090
        } else {
            0xFFFFFF
        };
        out.extend(super::sunset::wrap(game, line, room).into_iter().map(|l| (l, colour)));
    }
    out
}

pub(super) fn draw(game: &Game<'_>, w: f32, h: f32, at: (f32, f32), selected: Option<&str>, in_use: &str, menu: Option<&Menu>, out: &mut Vec<Shape>) {
    let Some(menu) = menu else {
        return;
    };
    let r = menu.rect(w, h);
    out.push(Shape::Fill([0.0, 0.0, w, h], 0x8000_0000));
    super::sunset::framed(out, r, panel_radius(), super::sunset::pal().profile_panel, 1.0);
    let close = [r[0] + r[2] - 25.0, r[1], 25.0, 22.0];
    super::sunset::cross(out, close, inside(close, at));
    if let Some(p) = &menu.preview {
        out.push(Shape::Head(fit(game, PRESETS[p.preset].title, r[2] - 40.0), [r[0] + 8.0, r[1] + 7.0], 0xFFFFFF));
        let fits = preview_rows(r);
        for (n, (line, colour)) in p.lines.iter().take(fits).enumerate() {
            let text = if n + 1 == fits && p.lines.len() > fits { format!("{line} ...") } else { line.clone() };
            out.push(Shape::Text(fit(game, &text, r[2] - 16.0), [r[0] + 8.0, r[1] + TOP + n as f32 * LINE], *colour));
        }
        let (apply, back) = buttons(r);
        for (b, label, colour) in [(apply, "Apply", 0xFF00_0000 | accent()), (back, "Back", super::sunset::pal().button)] {
            out.push(Shape::Round(b, panel_radius(), colour));
            let tw = game.sunset_text_width(label);
            out.push(Shape::Text(label.to_owned(), [b[0] + (b[2] - tw) / 2.0, b[1] + super::sunset::text_y(b[3])], 0xFFFFFF));
        }
        if let Some(error) = &menu.error {
            out.push(Shape::Text(fit(game, error, r[2] - 150.0), [r[0] + 8.0, r[1] + r[3] - 20.0], 0xFF9090));
        }
        return;
    }
    out.push(Shape::Head("Profiles".into(), [r[0] + 8.0, r[1] + 7.0], 0xFFFFFF));
    for (t, tab, label) in tabs(r) {
        // The tab shown is filled in the accent colour (its text plain: the menu's shadow would smudge it).
        let tw = game.sunset_text_width(label);
        let text_at = [t[0] + (t[2] - tw) / 2.0, t[1] + 3.0];
        if menu.tab == tab {
            out.push(Shape::Round(t, panel_radius(), 0xFF00_0000 | accent()));
            out.push(Shape::Plain(label.to_owned(), text_at, 0xFFFFFF, false));
        } else {
            out.push(Shape::Text(label.to_owned(), text_at, if inside(t, at) { 0xD4D4D8 } else { 0x909090 }));
        }
    }
    if menu.tab == Tab::Saved && menu.names.is_empty() {
        out.push(Shape::Text("None saved yet: .profile save <name>".into(), [r[0] + 8.0, r[1] + TOP + 5.0], 0xFFFFFF));
    }
    if menu.tab == Tab::Import && menu.imports.is_empty() {
        // Two ways that need no folder (the imports folder still works: .profile import <name> says where it is).
        out.push(Shape::Text(fit(game, "No files waiting. Drop .profile files on this menu,", r[2] - 16.0), [r[0] + 8.0, r[1] + TOP + 5.0], 0xFFFFFF));
        out.push(Shape::Text(fit(game, "or type .profile import to choose them.", r[2] - 16.0), [r[0] + 8.0, r[1] + TOP + 5.0 + LINE], 0xFFFFFF));
    }
    for i in (menu.start..menu.count()).take(rows(r)) {
        let row = [r[0] + 4.0, r[1] + TOP + (i - menu.start) as f32 * ROW, r[2] - 8.0, ROW];
        if inside(row, at) || i == menu.focus {
            out.push(Shape::Round(row, panel_radius(), super::sunset::pal().profile_tab));
        }
        let at_text = [row[0] + 5.0, row[1] + 5.0];
        match menu.tab {
            Tab::Saved => {
                let name = &menu.names[i];
                let colour = if selected == Some(name.as_str()) { accent() } else { 0xFFFFFF };
                out.push(Shape::Text(fit(game, name, row[2] - 10.0), at_text, colour));
            }
            Tab::Presets => {
                let preset = &PRESETS[i];
                let colour = if selected == Some(preset.id) { accent() } else { 0xFFFFFF };
                let title = fit(game, preset.title, row[2] - 10.0);
                let tw = game.sunset_text_width(&title);
                out.push(Shape::Text(title, at_text, colour));
                let room = row[2] - 18.0 - tw;
                if room > 30.0 {
                    out.push(Shape::Text(fit(game, preset.about, room), [at_text[0] + tw + 8.0, at_text[1]], 0x808080));
                }
            }
            Tab::Import => out.push(Shape::Text(fit(game, &menu.imports[i], row[2] - 10.0), at_text, 0xFFFFFF)),
        }
    }
    let button = |out: &mut Vec<Shape>, b: [f32; 4], label: &str| {
        out.push(Shape::Round(b, panel_radius(), if inside(b, at) { 0xFF00_0000 | accent() } else { super::sunset::pal().button }));
        let tw = game.sunset_text_width(label);
        out.push(Shape::Text(label.to_owned(), [b[0] + (b[2] - tw) / 2.0, b[1] + 3.0], 0xFFFFFF));
    };
    // The Import tab's foot: the question when a name is taken, else the clipboard's button.
    let mut taken = 8.0;
    if menu.tab == Tab::Import {
        if let Some(name) = &menu.ask {
            let (keep, replace) = ask_buttons(r);
            button(out, keep, "Keep both");
            button(out, replace, "Replace");
            let said = fit(game, &format!("{name} is saved already."), keep[0] - r[0] - 14.0);
            out.push(Shape::Text(said, [r[0] + 8.0, r[1] + r[3] - 16.0], 0xFFFFFF));
            return;
        }
        let b = paste_button(r);
        button(out, b, "Paste from clipboard");
        taken = b[2] + 14.0;
    }
    let saving = menu.tab == Tab::Saved && menu.in_use.is_some();
    if let Some(name) = menu.in_use.as_deref().filter(|_| saving) {
        let b = save_button(r);
        out.push(Shape::Round(b, panel_radius(), if inside(b, at) { 0xFF00_0000 | accent() } else { super::sunset::pal().button }));
        let label = fit(game, &format!("Save {name}"), b[2] - 8.0);
        let tw = game.sunset_text_width(&label);
        out.push(Shape::Text(label, [b[0] + (b[2] - tw) / 2.0, b[1] + 3.0], 0xFFFFFF));
    }
    // What went wrong, or what the last import did, beside the foot's button.
    let room = r[2] - 8.0 - if saving { 104.0 } else { taken };
    if let Some(error) = &menu.error {
        out.push(Shape::Text(fit(game, error, room), [r[0] + 8.0, r[1] + r[3] - 16.0], 0xFF9090));
    } else if let Some(note) = &menu.note {
        out.push(Shape::Text(fit(game, note, room), [r[0] + 8.0, r[1] + r[3] - 16.0], 0xC8C8C8));
    } else if menu.tab != Tab::Import {
        // Nothing to report: which profile is in use (as the HUD's watermark says), and what a click does.
        let said = match (menu.tab, menu.names.is_empty()) {
            (Tab::Presets, _) => format!("In use: {in_use}. Click a preset to see what it changes."),
            (_, true) => format!("In use: {in_use}."),
            (_, false) => format!("In use: {in_use}. Click a profile to load it."),
        };
        out.push(Shape::Text(fit(game, &said, room), [r[0] + 8.0, r[1] + r[3] - 16.0], 0x909090));
    }
}
