// The HUD's module list (array list) and the colours and text case the rest of the HUD shares.
//
// Each module on is a row, widest first (or by name), with its details after its name (its info, else its Mode):
// in gray (Gray), in brackets, after a dash, or not at all. Rows slide in from the screen's edge as a module turns on
// and out as it turns off, and glide to their new place when the order changes. A row's colour is the HUD's Theme
// color (Static), flows between it and Second color down the list (Fade), pulses between them all at once (Breathe),
// or runs through the rainbow (Rainbow), at Color speed. Behind a row: a background at List background's opacity, and
// a bar on its outer or inner edge in the row's colour. Modules shown leaves out Render modules or those without a
// bind; the menu's own modules (Client, HUD, ClickGUI) never show.
//
// Behaviour after Rise 6's Interface module (array list colour modes, suffix, lowercase, remove spaces, background and
// sidebar, slide animations); written anew for Sunset.

use super::Game;
use super::sunset::{Category, Module, Shape, accent};
use std::sync::Mutex;
use std::time::Instant;

// The colours a HUD colour setting offers: the menu's accent, a darker accent, and the palette's.
pub(super) const COLOURS: &[&str] = &[
    "Accent", "Darker", "White", "Gold", "Red", "Orange", "Yellow", "Lime", "Green", "Cyan", "Blue", "Purple", "Pink", "Gray",
    "Sunset Orange",
];

// Second color's: the same, a darker accent first (a fade of the accent into its own shade).
pub(super) const SECOND_COLOURS: &[&str] = &[
    "Darker", "Accent", "White", "Gold", "Red", "Orange", "Yellow", "Lime", "Green", "Cyan", "Blue", "Purple", "Pink", "Gray",
    "Sunset Orange",
];

fn colour(name: &str) -> u32 {
    match name {
        "Accent" => accent() & 0x00FF_FFFF,
        "Darker" => scale(accent(), 0.55),
        other => super::sunset_visual::palette(other) & 0x00FF_FFFF,
    }
}

fn scale(rgb: u32, k: f32) -> u32 {
    let ch = |s: u32| (((rgb >> s & 0xFF) as f32 * k).round() as u32).min(255) << s;
    ch(16) | ch(8) | ch(0)
}

fn blend(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = ((a >> s & 0xFF) as f32, (b >> s & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32).min(255) << s
    };
    ch(16) | ch(8) | ch(0)
}

// A hue (degrees) at Rise's rainbow saturation (0.6) and full brightness.
fn rainbow(hue: f32) -> u32 {
    let h = hue.rem_euclid(360.0) / 60.0;
    let (s, v) = (0.6f32, 1.0f32);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| (((f + m) * 255.0).round() as u32).min(255);
    to(r) << 16 | to(g) << 8 | to(b)
}

static START: Mutex<Option<Instant>> = Mutex::new(None);

fn seconds() -> f32 {
    START.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Instant::now).elapsed().as_secs_f32()
}

// The HUD's colour at height `y` (pixels down the screen) now, by its Color mode.
pub(super) fn colour_at(hud: &Module, mode: &str, y: f32) -> u32 {
    let (first, second) = (colour(hud.p("Theme color")), colour(hud.p("Second color")));
    let t = seconds() * hud.n("Color speed") as f32;
    match mode {
        "Fade" => blend(first, second, (t * 1.667 + y * 0.06).sin() * 0.5 + 0.5),
        "Breathe" => blend(first, second, (t * 1.667).sin() * 0.5 + 0.5),
        "Rainbow" => rainbow(t * 100.0 + y * 2.5),
        _ => first,
    }
}

// Text as the HUD's Text case has it.
pub(super) fn cased(hud: &Module, text: &str) -> String {
    match hud.p("Text case") {
        "lowercase" => text.to_lowercase(),
        "UPPERCASE" => text.to_uppercase(),
        _ => text.to_owned(),
    }
}

// The HUD's shapes from `from` on in its Text case (what was drawn after the text was measured: left-aligned).
pub(super) fn case_shapes(hud: &Module, shapes: &mut [Shape]) {
    if hud.p("Text case") == "Normal" {
        return;
    }
    for s in shapes {
        if let Shape::Text(t, ..) | Shape::Head(t, ..) | Shape::Scaled(t, ..) = s {
            *t = cased(hud, t);
        }
    }
}

// A module's details for the list: its own info, else its Mode.
fn detail(m: &Module) -> Option<String> {
    m.info().or_else(|| m.setting("Mode").map(|_| m.p("Mode").to_owned()))
}

// Whether a module on shows in the list.
fn shown(hud: &Module, m: &Module) -> bool {
    if !m.on || matches!(m.name, "HUD" | "ClickGUI") || m.category == Category::Client {
        return false;
    }
    match hud.p("Modules shown") {
        "Exclude render" => m.category != Category::Render,
        "Only bound" => m.key.is_some(),
        _ => true,
    }
}

// A row: its module, the text drawn (name, details) and their widths, how far in it is (0 out, 1 in) and its height.
struct Row {
    module: &'static str,
    name: String,
    detail: String,
    name_w: f32,
    detail_w: f32,
    shown: bool,
    t: f32,
    y: f32,
}

impl Row {
    fn width(&self) -> f32 {
        self.name_w + if self.detail.is_empty() { 0.0 } else { 4.0 + self.detail_w }
    }
}

struct List {
    rows: Vec<Row>,
    last: Option<Instant>,
    font: u32,
}

static LIST: Mutex<List> = Mutex::new(List { rows: Vec::new(), last: None, font: u32::MAX });

// The text of a module's row: its name and details as Text case, Remove spaces and Module details have them.
fn row_text(hud: &Module, m: &Module) -> (String, String) {
    let tidy = |s: &str| {
        let s = cased(hud, s);
        if hud.b("Remove spaces") { s.replace(' ', "") } else { s }
    };
    let detail = match (hud.p("Module details"), detail(m)) {
        ("Off", _) | (_, None) => String::new(),
        ("Brackets", Some(d)) => format!("[{}]", tidy(&d)),
        ("Dash", Some(d)) => format!("- {}", tidy(&d)),
        (_, Some(d)) => tidy(&d),
    };
    (tidy(m.name), detail)
}

// Draws the list at the corner's place (`at`, the top of its side's edge: x is the screen's edge for Right), and the
// room it takes now (its widest row and its height). `place` asks the corner for that room first.
pub(super) fn draw(game: &Game<'_>, hud: &Module, mods: &[Module], right: bool, place: &mut dyn FnMut(f32, f32) -> (f32, f32), out: &mut Vec<Shape>) {
    let mut list = LIST.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let dt = list.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
    list.last = Some(now);
    let font = u32::from(horizon_client::font::sunset_font::STYLE.load(std::sync::atomic::Ordering::Relaxed));
    let remeasure = std::mem::replace(&mut list.font, font) != font;
    let spacing = hud.n("List spacing") as f32;
    let animate = hud.b("List animations");
    // The rows wanted now, with their text: new ones start out at the edge, changed text is measured again.
    for row in &mut list.rows {
        row.shown = false;
    }
    for m in mods.iter().filter(|m| shown(hud, m)) {
        let (name, detail) = row_text(hud, m);
        let i = match list.rows.iter().position(|r| r.module == m.name) {
            Some(i) => i,
            None => {
                list.rows.push(Row { module: m.name, name: String::new(), detail: String::new(), name_w: 0.0, detail_w: 0.0, shown: true, t: 0.0, y: -1.0 });
                list.rows.len() - 1
            }
        };
        let row = &mut list.rows[i];
        row.shown = true;
        if remeasure || row.name != name {
            row.name_w = game.native_text_width(&name);
            row.name = name;
        }
        if remeasure || row.detail != detail {
            row.detail_w = if detail.is_empty() { 0.0 } else { game.native_text_width(&detail) };
            row.detail = detail;
        }
    }
    // Order: widest first, or by name; the rows going out keep their place while they leave.
    let by_name = hud.p("List sort") == "Name";
    list.rows.sort_by(|a, b| {
        b.shown.cmp(&a.shown).then_with(|| if by_name { a.name.cmp(&b.name) } else { b.width().total_cmp(&a.width()) })
    });
    let count = list.rows.iter().filter(|r| r.shown).count();
    let speed = if animate { dt * 6.0 } else { 1.0 };
    for (n, row) in list.rows.iter_mut().enumerate() {
        let target = if row.shown { n as f32 * spacing } else { row.y.max(0.0) };
        row.t = if row.shown { (row.t + speed).min(1.0) } else { (row.t - speed).max(0.0) };
        row.y = if row.y < 0.0 || !animate { target } else { row.y + (target - row.y) * (dt * 12.0).min(1.0) };
    }
    list.rows.retain(|r| r.shown || r.t > 0.0);
    let widest = list.rows.iter().filter(|r| r.t > 0.0).map(Row::width).fold(0.0, f32::max);
    if list.rows.is_empty() {
        return;
    }
    let bar = hud.p("List bar");
    let pad = 2.0;
    let room = widest + 2.0 * pad + if bar == "Off" { 0.0 } else { 2.0 };
    let (x, y) = place(room, count as f32 * spacing + 2.0);
    let mode = hud.p("List color mode");
    let background = hud.n("List background") as f32;
    for row in &list.rows {
        let ease = row.t * row.t * (3.0 - 2.0 * row.t);
        let w = row.width() + 2.0 * pad;
        let ry = y + row.y;
        let colour = colour_at(hud, mode, ry);
        // The bar on the row's right edge or its left (Outer: the screen's side).
        let bar_right = bar != "Off" && (right == (bar == "Outer"));
        let bar_left = bar != "Off" && !bar_right;
        // In from (and out to) the screen's edge, fading as it goes.
        let slide = (1.0 - ease) * (w + 4.0);
        let rx = if right {
            x + room - w - if bar_right { 2.0 } else { 0.0 } + slide
        } else {
            x + if bar_left { 2.0 } else { 0.0 } - slide
        };
        let alpha = ((ease * 255.0) as u32).clamp(1, 255);
        if background > 0.0 {
            let a = ((background * 255.0 * ease) as u32).min(255);
            out.push(Shape::Fill([rx, ry, w, spacing], a << 24));
        }
        if bar_right {
            out.push(Shape::Fill([rx + w, ry, 1.5, spacing], alpha << 24 | colour));
        } else if bar_left {
            out.push(Shape::Fill([rx - 1.5, ry, 1.5, spacing], alpha << 24 | colour));
        }
        let ty = ry + (spacing - 8.0) / 2.0;
        out.push(Shape::Text(row.name.clone(), [rx + pad, ty], alpha << 24 | colour));
        if !row.detail.is_empty() {
            let tint = if hud.p("Module details") == "Gray" { 0xAAAAAA } else { colour };
            out.push(Shape::Text(row.detail.clone(), [rx + pad + row.name_w + 4.0, ty], alpha << 24 | tint));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{blend, rainbow, scale};

    #[test]
    fn colours_blend_and_cycle() {
        assert_eq!(blend(0x000000, 0xFFFFFF, 0.0), 0x000000);
        assert_eq!(blend(0x000000, 0xFFFFFF, 1.0), 0xFFFFFF);
        assert_eq!(blend(0xFF0000, 0x0000FF, 0.5), 0x800080);
        assert_eq!(scale(0x804020, 0.5), 0x402010);
        assert_eq!(rainbow(0.0), 0xFF6666);
        assert_eq!(rainbow(360.0), rainbow(0.0));
        assert_eq!(rainbow(120.0) >> 8 & 0xFF, 0xFF);
    }
}
