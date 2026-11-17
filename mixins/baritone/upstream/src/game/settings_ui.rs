// Baritone's settings screen as the game draws it: the page Options > Mods > Baritone, drawn in place of the options screen's own
// page for it (the hooks are hooks/page_*.rs). What it shows, where everything stands and what a press does is baritone_screen;
// this draws it with the parts of a Bedrock Edition screen: bevelled buttons, tabs and rows with an item picture, a switch, a
// slider, a dropdown, text fields, section labels, a scrollbar, and a tooltip on hover. The colour is Baritone's own, the light
// purple of its `[Baritone]` in chat.
use super::baritone::Baritone;
use super::baritone_screen::{self as screen, Control, Field, Rect, View, CATEGORIES};
use super::Game;
use horizon_client::{
    hud::{rgba, HudAtlas, HudBatch},
    items::ItemStack,
    text::Text,
    ui, widgets,
};
use std::sync::Arc;

fn c(rgb: u32) -> u32 {
    rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

// Baritone's purple.
const ACCENT: u32 = 0xA83FD6;
const ACCENT_HOVER: u32 = 0xBC5BE8;
const ACCENT_LIGHT: u32 = 0xD58AF2;
const ACCENT_DARK: u32 = 0x6B2491;
const ACCENT_DEEP: u32 = 0x9738C2;
// The grays of the screen.
const EDGE: u32 = 0x1D1E1F;
const PANEL: u32 = 0x313233;
const HEADER: u32 = 0x3A3B3D;
const WELL: u32 = 0x242526;
const ROW_A: u32 = 0x2C2D2E;
const ROW_B: u32 = 0x282829;
const ROW_HOVER: u32 = 0x363738;
const BUTTON: u32 = 0xD0D1D4;
const BUTTON_HOVER: u32 = 0xE6E7EA;
const BUTTON_LIGHT: u32 = 0xF4F4F6;
const BUTTON_DARK: u32 = 0x58585A;
const INK: u32 = 0x1E1E1F;
const WHITE: u32 = 0xFFFFFF;
const MUTED: u32 = 0xA9AAAC;

fn fill(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, color: u32) {
    batch.fill(atlas, r[0], r[1], r[2], r[3], color);
}

fn inset(r: Rect, by: f32) -> Rect {
    [r[0] + by, r[1] + by, (r[2] - 2.0 * by).max(0.0), (r[3] - 2.0 * by).max(0.0)]
}

fn text_of(s: &str, bold: bool) -> Text {
    let t = Text::literal(s);
    if bold { t.bold() } else { t }
}

/// How wide `s` is at `scale`; bold glyphs are a pixel wider each.
fn width(atlas: &HudAtlas, s: &str, scale: f32, bold: bool) -> f32 {
    (HudBatch::text_width(atlas, s) + if bold { s.chars().count() as f32 } else { 0.0 }) * scale
}

/// Text with its top left at `(x, y)`, `scale` times the size of the font.
fn put(batch: &mut HudBatch, atlas: &HudAtlas, s: &str, x: f32, y: f32, scale: f32, color: u32, shadow: bool, bold: bool) {
    let start = batch.vertices.len();
    batch.text_styled(atlas, &text_of(s, bold), 0.0, 0.0, color, shadow);
    for v in &mut batch.vertices[start..] {
        v.pos = [(x + v.pos[0] * scale).round(), (y + v.pos[1] * scale).round()];
    }
}

/// Text centred in `r`.
fn centered(batch: &mut HudBatch, atlas: &HudAtlas, s: &str, r: Rect, scale: f32, color: u32, shadow: bool, bold: bool) {
    let w = width(atlas, s, scale, bold);
    put(batch, atlas, s, r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - 8.0 * scale) / 2.0, scale, color, shadow, bold);
}

/// Text starting at `x`, centred up and down in `r`.
fn left(batch: &mut HudBatch, atlas: &HudAtlas, s: &str, x: f32, r: Rect, scale: f32, color: u32, shadow: bool) {
    put(batch, atlas, s, x, r[1] + (r[3] - 8.0 * scale) / 2.0, scale, color, shadow, false);
}

/// `s` cut with "..." to fit in `room` pixels at `scale`.
fn fitted(atlas: &HudAtlas, s: &str, room: f32, scale: f32) -> String {
    if width(atlas, s, scale, false) <= room {
        return s.to_owned();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let next = format!("{out}{ch}...");
        if width(atlas, &next, scale, false) > room {
            break;
        }
        out.push(ch);
    }
    format!("{}...", out.trim_end())
}

/// The end of `s` that fits in `room` pixels: what a text field shows while it is being typed in.
fn tail(atlas: &HudAtlas, s: &str, room: f32, scale: f32) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut from = 0;
    while from < chars.len() && width(atlas, &chars[from..].iter().collect::<String>(), scale, false) > room {
        from += 1;
    }
    chars[from..].iter().collect()
}

/// A Bedrock slab: the dark outline, a bright ring, the body inside it, and a thick shadow strip under all of it.
fn bevel(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, body: u32, light: u32, dark: u32, outline: u32, line: f32) {
    fill(batch, atlas, r, c(outline));
    let i = inset(r, line);
    fill(batch, atlas, i, c(dark));
    let ring = [i[0], i[1], i[2], (i[3] - 2.0 * line).max(0.0)];
    fill(batch, atlas, ring, c(light));
    fill(batch, atlas, inset(ring, line), c(body));
}

/// The colour of a label on a button: white on Baritone's purple, dark on the light gray one.
fn ink(main: bool) -> u32 {
    c(if main { WHITE } else { INK })
}

/// A Bedrock button: light gray with dark text, or Baritone's purple when it is the main one or the tab that is open; a hovered
/// one is lighter and outlined.
fn button(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, label: &str, hover: bool, main: bool, f: f32, scale: f32) {
    let line = f.max(1.0).floor();
    let outline = if hover { if main { WHITE } else { ACCENT } } else { EDGE };
    if main {
        bevel(batch, atlas, r, if hover { ACCENT_HOVER } else { ACCENT }, ACCENT_LIGHT, ACCENT_DARK, outline, line);
    } else {
        bevel(batch, atlas, r, if hover { BUTTON_HOVER } else { BUTTON }, BUTTON_LIGHT, BUTTON_DARK, outline, line);
    }
    centered(batch, atlas, label, r, scale, ink(main), main, false);
}

/// A dark well: what a text field and the list sit in.
fn well(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, edge: u32, f: f32) {
    let line = f.max(1.0).floor();
    fill(batch, atlas, r, c(edge));
    fill(batch, atlas, inset(r, line), c(WELL));
}

/// The switch: Baritone's purple under an I when it is on, dark under an O when it is off, the knob on the other half.
fn switch(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, on: bool, hover: bool, f: f32, scale: f32) {
    // A square knob, and beside it a cell two pixels shorter that stands on the same bottom edge, so the knob rises above it.
    // The cell's far side is its own outline; the near side is the knob's, which is drawn over it.
    let line = f.max(1.0).floor();
    let outline = if hover { WHITE } else { EDGE };
    let (knob_w, cell_h) = (r[3], r[3] - 2.0 * f);
    let cell_w = r[2] - knob_w + line;
    let cell_y = r[1] + r[3] - cell_h;
    let (cell, knob) = if on {
        ([r[0], cell_y, cell_w, cell_h], [r[0] + r[2] - knob_w, r[1], knob_w, r[3]])
    } else {
        ([r[0] + r[2] - cell_w, cell_y, cell_w, cell_h], [r[0], r[1], knob_w, r[3]])
    };
    fill(batch, atlas, cell, c(outline));
    let body = inset(cell, line);
    // The part of the cell the knob leaves showing, where the I or the O goes.
    let shown = if on {
        [body[0], body[1], knob[0] - body[0], body[3]]
    } else {
        [knob[0] + knob_w, body[1], body[0] + body[2] - knob[0] - knob_w, body[3]]
    };
    if on {
        fill(batch, atlas, body, c(ACCENT_DEEP));
        fill(batch, atlas, [body[0], body[1], body[2], line], c(ACCENT_HOVER));
        centered(batch, atlas, "I", shown, scale, c(WHITE), false, false);
    } else {
        fill(batch, atlas, body, c(0x2B2C2D));
        centered(batch, atlas, "O", shown, scale, c(0xD4D4D4), false, false);
    }
    bevel(batch, atlas, knob, BUTTON, BUTTON_LIGHT, BUTTON_DARK, outline, line);
}

/// The slider: a dark track with a purple part up to the knob, a notch at each step, and the gray knob.
fn slider(batch: &mut HudBatch, atlas: &HudAtlas, t: Rect, steps: usize, at: Option<f32>, hover: bool, f: f32) {
    let line = f.max(1.0).floor();
    fill(batch, atlas, t, c(EDGE));
    let bar = inset(t, line);
    fill(batch, atlas, bar, c(0x3A3B3D));
    let pos = at.unwrap_or(0.0);
    fill(batch, atlas, [bar[0], bar[1], bar[2] * pos, bar[3]], c(ACCENT));
    for i in 1..steps.saturating_sub(1) {
        let x = bar[0] + bar[2] * i as f32 / (steps - 1) as f32;
        fill(batch, atlas, [x.floor(), bar[1], line, bar[3]], c(EDGE));
    }
    let (kw, kh) = (7.0 * f, 9.0 * f);
    let knob = [(bar[0] + bar[2] * pos - kw / 2.0).round().max(t[0]), t[1] + t[3] / 2.0 - kh / 2.0, kw, kh];
    bevel(batch, atlas, knob, if hover { BUTTON_HOVER } else { BUTTON }, BUTTON_LIGHT, BUTTON_DARK, if hover { WHITE } else { EDGE }, line);
}

/// The little arrow on a dropdown.
fn chevron(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, color: u32, f: f32) {
    let (cx, cy) = (r[0] + r[2] - 8.0 * f, r[1] + r[3] / 2.0 - 1.5 * f);
    for i in 0..3 {
        let w = (6.0 - 2.0 * i as f32) * f;
        fill(batch, atlas, [cx - w / 2.0, cy + i as f32 * f, w, f], color);
    }
}

/// The magnifying glass of the search field.
fn magnifier(batch: &mut HudBatch, atlas: &HudAtlas, x: f32, y: f32, f: f32) {
    let line = f.max(1.0).floor();
    let lens = [x, y, 6.0 * f, 6.0 * f];
    fill(batch, atlas, lens, c(MUTED));
    fill(batch, atlas, inset(lens, line), c(WELL));
    fill(batch, atlas, [x + 5.0 * f, y + 5.0 * f, 2.0 * f, 2.0 * f], c(MUTED));
    fill(batch, atlas, [x + 6.0 * f, y + 6.0 * f, 2.0 * f, 2.0 * f], c(MUTED));
}

/// A text field: a dark well with the text, or a hint in gray when it is empty, and a caret while it has the keys.
fn text_field(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, text: &str, hint: &str, focused: bool, pad: f32, f: f32, scale: f32) {
    well(batch, atlas, r, if focused { ACCENT } else { EDGE }, f);
    let room = r[2] - pad - 6.0 * f;
    if text.is_empty() && !focused {
        left(batch, atlas, &fitted(atlas, hint, room, scale), r[0] + pad, r, scale, c(0x707173), true);
        return;
    }
    let shown = tail(atlas, text, room, scale);
    left(batch, atlas, &shown, r[0] + pad, r, scale, c(WHITE), true);
    let blink = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 500 % 2);
    if focused && blink == 0 {
        let x = r[0] + pad + width(atlas, &shown, scale, false);
        fill(batch, atlas, [x, r[1] + 3.0 * f, f.max(1.0).floor(), r[3] - 6.0 * f], c(WHITE));
    }
}

/// An item's picture, `size` design pixels wide, with its top left at `(x, y)`; nothing for an item the game does not know.
fn picture(game: &Game<'_>, batch: &mut HudBatch, item: &str, x: f32, y: f32, size: f32, f: f32) {
    let id = format!("minecraft:{item}");
    if game.res.items().get(&id).is_none() {
        return;
    }
    let stack = ItemStack { item: Arc::from(id.as_str()), count: 1, damage: 0, extra: None };
    let start = batch.vertices.len();
    game.draw_item_model(batch, &stack, x, y);
    batch.scale_since(start, x, y, size * f / 16.0, size * f / 16.0);
}

/// A thin scrollbar thumb at the right edge of `r`, when the content is taller than `r`.
fn scrollbar(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, content: f32, scroll: f32, f: f32) {
    if content <= r[3] {
        return;
    }
    let line = f.max(1.0).floor();
    let track = [r[0] + r[2] - 4.0 * f, r[1], 3.0 * f, r[3]];
    fill(batch, atlas, track, c(EDGE));
    let bar = (r[3] * r[3] / content).clamp(10.0 * f, r[3] - 4.0 * f);
    let y = r[1] + (scroll / (content - r[3])) * (r[3] - bar);
    bevel(batch, atlas, [track[0], y, track[2], bar], BUTTON, BUTTON_LIGHT, BUTTON_DARK, EDGE, line.min(1.0));
}

/// The words of `s` in lines of at most `max` letters.
fn wrap(s: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in s.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= max => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// One frame of the screen, in place of the options screen's own page.
pub fn draw(b: &mut Baritone, game: &Game<'_>, batch: &mut HudBatch) {
    let atlas = &game.gui.atlas;
    let cursor = game.cursor;
    let (w, h) = (batch.width, batch.height);
    let l = screen::layout(w, h);
    let (f, t) = (l.scale, l.text);
    let line = f.max(1.0).floor();
    b.core.ui.clamp(&l);
    let shake = b.core.ui.flash;
    b.core.ui.flash = shake.saturating_sub(1);
    let (state, settings) = (&b.core.ui, b.core.ui.shown(&b.core.settings));
    let view = state.rows();
    let rects = screen::rows(&l, &view, state.scroll);
    let hover = |r: Rect| ui::hovered(cursor, r[0], r[1], r[2], r[3]);
    let dropdown_open = state.open.is_some();

    // The panel, with its shadow, and the header.
    fill(batch, atlas, [0.0, 0.0, w, h], rgba(0, 0, 0, 90));
    fill(batch, atlas, [l.panel[0] + 3.0 * f, l.panel[1] + 3.0 * f, l.panel[2], l.panel[3]], rgba(0, 0, 0, 100));
    fill(batch, atlas, l.panel, c(EDGE));
    fill(batch, atlas, inset(l.panel, line), c(PANEL));
    fill(batch, atlas, [l.header[0] + line, l.header[1] + line, l.header[2] - 2.0 * line, l.header[3] - line], c(HEADER));
    fill(batch, atlas, [l.header[0], l.header[1] + l.header[3] - line, l.header[2], line], c(EDGE));

    // The header: back, the name with its picture, the version, and the search field.
    let back = hover(l.back) && !dropdown_open;
    button(batch, atlas, l.back, "<", back, false, f, t * 1.1);
    widgets::record("Back", l.back, true);
    widgets::widget_cursor(back, true);
    picture(game, batch, "diamond_pickaxe", l.back[0] + l.back[2] + 6.0 * f, l.header[1] + 5.0 * f, 16.0, f);
    let name_x = l.back[0] + l.back[2] + 26.0 * f;
    put(batch, atlas, "Baritone", name_x, l.header[1] + (l.header[3] - 8.0 * t * 1.25) / 2.0, t * 1.25, c(WHITE), true, true);
    let name_w = width(atlas, "Baritone", t * 1.25, true);
    put(batch, atlas, super::baritone::VERSION, name_x + name_w + 6.0 * f, l.header[1] + (l.header[3] - 8.0 * t * 0.8) / 2.0 + f, t * 0.8, c(MUTED), false, false);
    text_field(batch, atlas, l.search, &state.search, "Search settings", state.focus == Some(Field::Search), 16.0 * f, f, t * 0.9);
    magnifier(batch, atlas, l.search[0] + 5.0 * f, l.search[1] + 4.5 * f, f);
    widgets::record("Search", l.search, true);
    widgets::widget_cursor(hover(l.search), true);

    // The tabs, each with the picture of an item.
    for (i, r) in l.tabs.iter().enumerate() {
        let selected = i == state.category && state.search.is_empty();
        let over = hover(*r) && !dropdown_open;
        button(batch, atlas, *r, "", over, selected, f, t);
        let cat = &CATEGORIES[i];
        picture(game, batch, cat.icon, r[0] + 4.0 * f, r[1] + (r[3] - 12.0 * f) / 2.0, 12.0, f);
        left(batch, atlas, &fitted(atlas, cat.label, r[2] - 24.0 * f, t), r[0] + 20.0 * f, *r, t, ink(selected), selected);
        widgets::record(cat.label, *r, true);
        widgets::widget_cursor(over, true);
    }

    // The list in its well.
    well(batch, atlas, l.frame, EDGE, f);
    let start = batch.vertices.len();
    let mut tip: Option<usize> = None;
    for (i, (row, r)) in view.iter().zip(&rects).enumerate() {
        let r = *r;
        if r[1] + r[3] < l.list[1] || r[1] > l.list[1] + l.list[3] {
            continue;
        }
        match row {
            View::Count(n) => {
                let label = if state.search.trim().is_empty() { format!("{n} settings") } else { format!("{n} found") };
                put(batch, atlas, &label, r[0] + 8.0 * f, r[1] + (r[3] - 8.0 * t * 0.8) / 2.0, t * 0.8, c(MUTED), false, false);
            }
            View::Heading(text) => {
                fill(batch, atlas, [r[0], r[1] + r[3] - line, r[2], line], c(EDGE));
                put(batch, atlas, &text.to_uppercase(), r[0] + 8.0 * f, r[1] + (r[3] - 8.0 * t * 0.8) / 2.0 + f, t * 0.8, c(ACCENT_LIGHT), false, true);
            }
            View::Item(item) => {
                let over = hover(r) && hover(l.list) && !dropdown_open;
                if over {
                    tip = Some(i);
                }
                fill(batch, atlas, r, c(if over { ROW_HOVER } else if i % 2 == 0 { ROW_A } else { ROW_B }));
                fill(batch, atlas, [r[0], r[1] + r[3] - line, r[2], line], c(EDGE));
                let top = if matches!(item.control, Control::Steps(..)) { r[1] + 6.0 * f } else { r[1] + (r[3] - 16.0 * f) / 2.0 };
                picture(game, batch, item.icon, r[0] + 7.0 * f, top, 16.0, f);
                let text_x = r[0] + 30.0 * f;
                put(batch, atlas, item.label, text_x, top + 0.5 * f, t, c(WHITE), true, false);
                let note = if matches!(item.control, Control::BlockInput) { state.note.as_ref() } else { None };
                let desc = note.map_or_else(|| screen::describe(item), |(text, _)| text.clone());
                let room = match item.control {
                    Control::Steps(..) => r[2] - 40.0 * f,
                    Control::BlockInput => screen::block_field(&l, r)[0] - text_x - 6.0 * f,
                    Control::Choice(..) => screen::dropdown(&l, r)[0] - text_x - 6.0 * f,
                    _ => screen::switch(&l, r)[0] - text_x - 6.0 * f,
                };
                put(batch, atlas, &fitted(atlas, &desc, room, t * 0.8), text_x, top + 9.5 * t * 0.9 + f, t * 0.8, c(note.map_or(MUTED, |(_, failed)| if *failed { 0xFF6B6B } else { 0x7CE07C })), false, false);
                match item.control {
                    Control::Toggle(_) | Control::Block(_) => {
                        let sw = screen::switch(&l, r);
                        switch(batch, atlas, sw, screen::is_on(item.control, settings) == Some(true), hover(sw) && !dropdown_open, f, t * 0.9);
                    }
                    Control::Steps(_, values) => {
                        let shown = screen::value(item, settings);
                        let vw = width(atlas, &shown, t, false);
                        put(batch, atlas, &shown, r[0] + r[2] - 14.0 * f - vw, top + 0.5 * f, t, c(WHITE), true, false);
                        let track = screen::track(&l, r);
                        slider(batch, atlas, track, values.len(), screen::position(item.control, settings), hover(track) || state.dragging == Some(i), f);
                    }
                    Control::Choice(..) => {
                        let d = screen::dropdown(&l, r);
                        let open = state.open == Some(i);
                        button(batch, atlas, d, "", hover(d) && !dropdown_open, open, f, t * 0.9);
                        left(batch, atlas, &fitted(atlas, &screen::value(item, settings), d[2] - 18.0 * f, t * 0.9), d[0] + 6.0 * f, d, t * 0.9, ink(open), open);
                        chevron(batch, atlas, d, ink(open), f);
                    }
                    Control::BlockInput => {
                        let field = screen::block_field(&l, r);
                        text_field(batch, atlas, field, &state.block, "Block name", state.focus == Some(Field::Block), 6.0 * f, f, t * 0.9);
                        let add = screen::add_button(&l, r);
                        button(batch, atlas, add, "Add", hover(add) && !dropdown_open, true, f, t * 0.9);
                        widgets::record("Add", add, true);
                    }
                    Control::Link(_) => {
                        let open = screen::open_button(&l, r);
                        button(batch, atlas, open, "Open", hover(open) && !dropdown_open, false, f, t * 0.9);
                    }
                    Control::ResetAll => {
                        let reset = screen::open_button(&l, r);
                        button(batch, atlas, reset, "Reset", hover(reset) && !dropdown_open, false, f, t * 0.9);
                    }
                }
                if r[1] >= l.list[1] && r[1] + r[3] <= l.list[1] + l.list[3] {
                    widgets::record(item.label, r, true);
                }
                widgets::widget_cursor(over, true);
            }
        }
    }
    batch.clip_since(start, l.list[0], l.list[1], l.list[0] + l.list[2], l.list[1] + l.list[3]);
    scrollbar(batch, atlas, l.frame, screen::content_height(&l, &view), state.scroll, f);

    // The footer: the page's defaults, or, while changes wait, the bar that asks to save them.
    if state.dirty() {
        // The bar shakes when the back button or Escape was pressed with changes waiting.
        let dx = if shake == 0 { 0.0 } else if shake % 4 < 2 { 2.0 * f } else { -2.0 * f };
        let at = |r: Rect| [r[0] + dx, r[1], r[2], r[3]];
        let bar = at(l.bar);
        fill(batch, atlas, bar, c(if shake > 0 { 0xE04646 } else { EDGE }));
        fill(batch, atlas, inset(bar, line), c(0x1A1B1D));
        let icon = [bar[0] + 7.0 * f, bar[1] + (bar[3] - 10.0 * f) / 2.0, 10.0 * f, 10.0 * f];
        for r in [[icon[0] + 2.0 * f, icon[1], 6.0 * f, 10.0 * f], [icon[0], icon[1] + 2.0 * f, 10.0 * f, 6.0 * f], [icon[0] + f, icon[1] + f, 8.0 * f, 8.0 * f]] {
            fill(batch, atlas, r, c(0xF0B323));
        }
        centered(batch, atlas, "!", icon, t * 0.9, c(INK), false, true);
        let label = fitted(atlas, "Careful: unsaved changes!", at(l.bar_reset)[0] - icon[0] - 16.0 * f, t * 0.85);
        left(batch, atlas, &label, icon[0] + 14.0 * f, bar, t * 0.85, c(WHITE), true);
        let reset = at(l.bar_reset);
        let over = hover(reset) && !dropdown_open;
        centered(batch, atlas, "Reset", reset, t * 0.85, c(if over { WHITE } else { ACCENT_LIGHT }), false, false);
        if over {
            fill(batch, atlas, [reset[0] + 4.0 * f, reset[1] + reset[3] - 2.0 * f, reset[2] - 8.0 * f, line], c(WHITE));
        }
        widgets::record("Reset", reset, true);
        widgets::widget_cursor(over, true);
        let save = at(l.bar_save);
        let over = hover(save) && !dropdown_open;
        button(batch, atlas, save, "Save Changes", over, true, f, t * 0.85);
        widgets::record("Save Changes", save, true);
        widgets::widget_cursor(over, true);
    } else {
        let over = hover(l.reset) && !dropdown_open;
        button(batch, atlas, l.reset, "Page defaults", over, false, f, t);
        widgets::record("Page defaults", l.reset, true);
        widgets::widget_cursor(over, true);
    }

    // An open dropdown hangs over everything under it.
    if let Some(i) = state.open
        && let (Some(View::Item(item)), Some(r)) = (view.get(i), rects.get(i))
        && let Control::Choice(name, values) = item.control
    {
        let d = screen::dropdown(&l, *r);
        for (value, o) in values.iter().zip(screen::dropdown_options(d, values.len())) {
            let current = settings.get(name) == *value;
            button(batch, atlas, o, "", hover(o), current, f, t * 0.9);
            left(batch, atlas, &screen::pretty(value), o[0] + 6.0 * f, o, t * 0.9, ink(current), current);
            widgets::record(&screen::pretty(value), o, true);
        }
    }

    // The blocks the text in the block field could be, under the field: picking one fills the field.
    if state.focus == Some(Field::Block)
        && !state.suggestions.is_empty()
        && let Some(row) = view.iter().position(|v| matches!(v, View::Item(i) if matches!(i.control, Control::BlockInput)))
    {
        let field = screen::block_field(&l, rects[row]);
        for (name, o) in state.suggestions.iter().zip(screen::suggestion_rects(field, state.suggestions.len())) {
            button(batch, atlas, o, "", hover(o), false, f, t * 0.9);
            picture(game, batch, name, o[0] + 3.0 * f, o[1] + (o[3] - 10.0 * f) / 2.0, 10.0, f);
            left(batch, atlas, &fitted(atlas, name, o[2] - 18.0 * f, t * 0.9), o[0] + 16.0 * f, o, t * 0.9, ink(false), false);
            widgets::record(name, o, true);
        }
    }

    // The tooltip of the row under the cursor: its name, what it does, and its default.
    if let Some(i) = tip
        && let Some(View::Item(item)) = view.get(i)
        && state.open.is_none()
        && !matches!(item.control, Control::BlockInput)
    {
        let mut lines = vec![Text::literal(item.label)];
        for part in wrap(&screen::describe(item), 44) {
            lines.push(Text::literal(part).with_color(0xAAAAAA));
        }
        if let Some(default) = screen::default_of(item) {
            lines.push(Text::literal(format!("Default: {default}")).with_color(0xD58AF2));
        }
        ui::tooltip(batch, atlas, &lines, cursor);
    }
}
