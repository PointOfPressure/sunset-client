// The list of saved profiles the Profiles module's row opens: a click, or the arrows and Enter, loads one. The files
// are read when it opens and when one loads, never while drawing.
use super::{Game, sunset::{Shape, accent, panel_radius}};

const ROW: f32 = 18.0;
// The list's title above it and the room under it.
const TOP: f32 = 25.0;
const BOTTOM: f32 = 22.0;

#[derive(Clone, Debug)]
pub(super) struct Menu {
    pub(super) names: Vec<String>,
    pub(super) error: Option<String>,
    focus: usize,
    start: usize,
}

fn frame(w: f32, h: f32, count: usize) -> [f32; 4] {
    let ww = (w - 16.0).min(300.0);
    let hh = (count.clamp(1, 10) as f32 * ROW + TOP + BOTTOM).min(h - 16.0);
    [(w - ww) / 2.0, (h - hh) / 2.0, ww, hh]
}

// How many rows fit.
fn rows(r: [f32; 4]) -> usize {
    ((r[3] - TOP - BOTTOM) / ROW).max(1.0) as usize
}

fn inside(r: [f32; 4], at: (f32, f32)) -> bool {
    at.0 >= r[0] && at.0 < r[0] + r[2] && at.1 >= r[1] && at.1 < r[1] + r[3]
}

impl Menu {
    pub(super) fn new(names: Vec<String>, selected: Option<&str>, w: f32, h: f32) -> Self {
        let focus = selected.and_then(|n| names.iter().position(|p| p == n)).unwrap_or(0);
        let mut menu = Self { names, error: None, focus, start: 0 };
        menu.reveal(w, h);
        menu
    }

    fn reveal(&mut self, w: f32, h: f32) {
        let count = rows(frame(w, h, self.names.len()));
        self.start = self.start.min(self.focus).max((self.focus + 1).saturating_sub(count));
    }

    pub(super) fn scroll(&mut self, steps: i32, w: f32, h: f32) {
        let max = self.names.len().saturating_sub(rows(frame(w, h, self.names.len())));
        self.start = (self.start as i32 + steps).clamp(0, max as i32) as usize;
    }

    // Up and Down move the mark, Enter gives the profile to load.
    pub(super) fn key(&mut self, key: &str, w: f32, h: f32) -> Option<String> {
        match key {
            "key.keyboard.up" => self.focus = self.focus.saturating_sub(1),
            "key.keyboard.down" => self.focus = (self.focus + 1).min(self.names.len().saturating_sub(1)),
            "key.keyboard.enter" | "key.keyboard.numpad.enter" => return self.names.get(self.focus).cloned(),
            _ => return None,
        }
        self.reveal(w, h);
        None
    }

    // The profile under a click, if any.
    pub(super) fn click(&self, at: (f32, f32), w: f32, h: f32) -> Option<String> {
        let r = frame(w, h, self.names.len());
        let list = [r[0] + 4.0, r[1] + TOP, r[2] - 8.0, rows(r) as f32 * ROW];
        inside(list, at).then(|| self.names.get(self.start + ((at.1 - list[1]) / ROW) as usize).cloned()).flatten()
    }

    // A click outside the list or on its x closes it.
    pub(super) fn closes(&self, at: (f32, f32), w: f32, h: f32) -> bool {
        let r = frame(w, h, self.names.len());
        !inside(r, at) || inside([r[0] + r[2] - 25.0, r[1], 25.0, 22.0], at)
    }
}

fn fit(game: &Game<'_>, text: &str, width: f32) -> String {
    super::sunset::fit_to(game, text, width)
}

pub(super) fn draw(game: &Game<'_>, w: f32, h: f32, at: (f32, f32), selected: Option<&str>, menu: Option<&Menu>, out: &mut Vec<Shape>) {
    let Some(menu) = menu else {
        return;
    };
    let r = frame(w, h, menu.names.len());
    out.push(Shape::Fill([0.0, 0.0, w, h], 0x8000_0000));
    out.push(Shape::Round(r, panel_radius(), 0xFF12_1214));
    out.push(Shape::Head("Profiles".into(), [r[0] + 8.0, r[1] + 7.0], 0xFFFFFF));
    out.push(Shape::Text("x".into(), [r[0] + r[2] - 15.0, r[1] + 7.0], 0xFFFFFF));
    if menu.names.is_empty() {
        out.push(Shape::Text("None saved yet: .profile save <name>".into(), [r[0] + 8.0, r[1] + TOP + 5.0], 0xFFFFFF));
    }
    for (i, name) in menu.names.iter().enumerate().skip(menu.start).take(rows(r)) {
        let row = [r[0] + 4.0, r[1] + TOP + (i - menu.start) as f32 * ROW, r[2] - 8.0, ROW];
        let chosen = selected == Some(name.as_str());
        if inside(row, at) || i == menu.focus {
            out.push(Shape::Round(row, panel_radius(), 0xFF28_282C));
        }
        out.push(Shape::Text(fit(game, name, row[2] - 10.0), [row[0] + 5.0, row[1] + 5.0], if chosen { accent() } else { 0xFFFFFF }));
    }
    if let Some(error) = &menu.error {
        out.push(Shape::Text(fit(game, error, r[2] - 16.0), [r[0] + 8.0, r[1] + r[3] - 16.0], 0xFF9090));
    }
}
