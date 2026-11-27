// Runtime part (see runtime.rs): drawing the click GUI: text measuring, pickers, panels and the profile menu.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset_text_edit::Draft;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use super::sunset::*;

// What the menu's text measured: its rows measure the same words every frame. Each entry is by the text and, for a cut,
// the room it was cut to. Kept until the font changes, or there are too many (numbers being dragged).
#[derive(Default)]
struct Measured {
    font: f32,
    widths: std::collections::HashMap<u64, f32>,
    cuts: std::collections::HashMap<(u64, u32), String>,
}

static MEASURED: Mutex<Option<Measured>> = Mutex::new(None);

fn measured<R>(f: impl FnOnce(&mut Measured) -> R) -> R {
    f(MEASURED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Measured::default))
}

// Once a frame, while the menu draws.
fn check_measured(game: &Game<'_>) {
    let font = game.sunset_text_width("Mm");
    measured(|m| {
        if m.font != font || m.widths.len() + m.cuts.len() > 4096 {
            *m = Measured { font, ..Measured::default() };
        }
    });
}

fn text_key(text: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

// A menu row's text width.
fn width_of(game: &Game<'_>, text: &str) -> f32 {
    let key = text_key(text);
    if let Some(width) = measured(|m| m.widths.get(&key).copied()) {
        return width;
    }
    let width = game.sunset_text_width(text);
    measured(|m| m.widths.insert(key, width));
    width
}

// The text, cut to `max` wide with "..." when it is wider.
pub(super) fn fit_to(game: &Game<'_>, text: &str, max: f32) -> String {
    let key = (text_key(text), max.to_bits());
    if let Some(cut) = measured(|m| m.cuts.get(&key).cloned()) {
        return cut;
    }
    let cut = if game.sunset_text_width(text) <= max {
        text.to_owned()
    } else {
        let mut t = text.to_owned();
        while !t.is_empty() && game.sunset_text_width(&format!("{t}...")) > max {
            t.pop();
        }
        format!("{t}...")
    };
    measured(|m| m.cuts.insert(key, cut.clone()));
    cut
}

// The list picker over the panels: its title, the search box, the rows (a lit mark on the chosen ones, which come
// first) and Done.
fn draw_picker(game: &Game<'_>, w: f32, h: f32, edit: &Editor, picker: &super::sunset_picker::Picker, modules: &[Module], at: (f32, f32), out: &mut Vec<Shape>) {
    use super::sunset_picker as pick;
    let fit = |text: &str, max: f32| fit_to(game, text, max);
    let solid = |rgb: u32| 0xFF00_0000 | rgb;
    let r = pick::rect(w, h);
    let [x, y, rw, _] = r;
    let room = (rw - 16.0).max(0.0);
    let control = control_radius();
    out.push(Shape::Fill([0.0, 0.0, w, h], 0x9000_0000));
    framed(out, r, panel_radius(), pal().dialog, 1.0);
    let m = &modules[edit.module];
    out.push(Shape::Head(fit(&format!("{}: {}", m.name, m.settings[edit.setting].name), room), [x + 8.0, y + 6.0], 0xFFFFFF));
    let search = edit.draft.as_ref().map_or("", Draft::text);
    let field = [x + 6.0, y + 22.0, rw - 12.0, 16.0];
    out.push(Shape::Round(field, panel_radius(), pal().field));
    let hint = match picker.kind {
        pick::ListKind::Blocks => "Search blocks",
        pick::ListKind::Items => "Search items",
        pick::ListKind::Entities => "Search mobs",
        pick::ListKind::Particles => "Search particles",
        pick::ListKind::Players => "Search, or type a name to add",
    };
    search_text(game, out, field, search, edit.draft.as_ref().map_or(0, Draft::caret), hint);
    let text = match &m.settings[edit.setting].value {
        Value::Text(t) => t.as_str(),
        _ => "",
    };
    let rows = pick::rows(picker, text, search);
    let area = pick::list_area(r);
    let mut icons = Vec::new();
    let scroll = super::sunset_smooth::shown(super::sunset_smooth::PICKER, picker.scroll).min(pick::max_scroll(rows.len(), r));
    let first = (scroll / pick::ROW) as usize;
    out.push(Shape::ClipStart);
    for (n, (name, have)) in rows.iter().enumerate().skip(first) {
        let ry = area[1] + n as f32 * pick::ROW - scroll;
        if ry >= area[1] + area[3] {
            break;
        }
        let row = [area[0], ry, area[2], pick::ROW];
        if inside(row, at) {
            out.push(Shape::Round(row, control, 0x1CFF_FFFF));
        }
        // A tick box: filled in the accent colour when chosen, an empty outline when not.
        let mark = [area[0] + 3.0, ry + (pick::ROW - 7.0) / 2.0, 7.0, 7.0];
        if *have {
            out.push(Shape::Round(mark, 2.0, solid(accent())));
            let (mx, my) = (mark[0], mark[1]);
            line(out, [mx + 1.5, my + 3.6], [mx + 3.0, my + 5.2], 0xFFFF_FFFF, 1.0);
            line(out, [mx + 3.0, my + 5.2], [mx + 5.6, my + 1.8], 0xFFFF_FFFF, 1.0);
        } else {
            out.push(Shape::Round(mark, 2.0, 0x70FF_FFFF));
            out.push(Shape::Round([mark[0] + 1.0, mark[1] + 1.0, mark[2] - 2.0, mark[3] - 2.0], 1.0, pal().dialog));
        }
        // The block's, item's or mob's icon (only the rows on screen are drawn), then its name.
        let icon = pick::icon(game, picker, name, [area[0] + 13.0, ry + 1.0], pick::ROW - 2.0);
        let left = if icon.is_some() { 26.0 } else { 14.0 };
        // A lighter tile behind it: obsidian, a spawner and the like are dark on a dark dialog.
        if icon.is_some() {
            out.push(Shape::Round([area[0] + 12.5, ry + 0.5, pick::ROW - 1.0, pick::ROW - 1.0], 2.0, 0x2EFF_FFFF));
        }
        // An icon can't be cut, so it waits outside the cut part until its row is all in view.
        icons.extend(icon.filter(|_| ry >= area[1] && ry + pick::ROW <= area[1] + area[3]));
        let label = if picker.kind == pick::ListKind::Players { (*name).to_owned() } else { pick::readable(name) };
        out.push(Shape::Text(fit(&label, area[2] - left - 4.0), [area[0] + left, ry + text_y(pick::ROW)], 0xFFFFFF));
    }
    out.push(Shape::ClipEnd([area[0], area[1], area[0] + area[2], area[1] + area[3]]));
    out.extend(icons);
    // More rows under the list's edge: they fade into the dialog there; and the scroll marker.
    let content = rows.len() as f32 * pick::ROW;
    if content - scroll > area[3] + 0.5 {
        fade(out, [area[0], area[1] + area[3], area[2]], pal().dialog, 0.5, true);
    }
    scrollbar(out, area[0] + area[2] - 2.0, area[1], area[3], content, area[3], scroll);
    let chosen = rows.iter().filter(|(_, have)| *have).count();
    // What the numbers count: with a search typed, what it found (the chosen ones always show).
    let note = if search.trim().is_empty() { format!("{chosen} of {} chosen", rows.len()) } else { format!("{} shown, {chosen} chosen", rows.len()) };
    // Clear unticks everything; it shows while anything is ticked, and is Undo right after, until something is ticked.
    let any = !pick::chosen(text).is_empty();
    let undo = !any && picker.cleared.is_some();
    let clear = pick::clear_button(r);
    let done = pick::done_button(r);
    out.push(Shape::Text(fit(&note, if any || undo { clear[0] } else { done[0] } - x - 12.0), [x + 8.0, done[1] + text_y(done[3])], 0x909090));
    if any || undo {
        let label = if undo { "Undo" } else { "Clear" };
        out.push(Shape::Round(clear, panel_radius(), if inside(clear, at) { pal().value_box } else { pal().button }));
        let tw = game.sunset_text_width(label);
        out.push(Shape::Text(label.to_owned(), [clear[0] + (clear[2] - tw) / 2.0, clear[1] + text_y(clear[3])], 0xFFFFFF));
    }
    out.push(Shape::Round(done, panel_radius(), solid(accent())));
    let tw = game.sunset_text_width("Done");
    out.push(Shape::Plain("Done".to_owned(), [done[0] + (done[2] - tw) / 2.0, done[1] + text_y(done[3])], 0xFFFFFF, false));
}

// A switch: one pill and knob wherever one is drawn (a setting's, the Dashboard's and the settings window's module
// switch), only bigger or smaller. On, the pill is the accent colour, its knob dark on a light accent and white on a
// dark one; off, the theme's grey (`lit` lifts it under the pointer) with a lighter knob.
pub(super) fn switch(out: &mut Vec<Shape>, [x, y, w, h]: [f32; 4], on: bool, lit: u32) {
    let radius = control_radius().min(h / 2.0);
    let inset = if h >= 10.0 { 2.0 } else { 1.0 };
    let [_, r, g, b] = accent().to_be_bytes();
    let light = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000 >= 140;
    let (pill, dot) = if on {
        (0xFF00_0000 | accent(), if light { pal().knob } else { 0xFFFF_FFFF })
    } else {
        (pal().toggle_off + (lit << 16 | lit << 8 | lit), 0xFFC4_C4CC)
    };
    out.push(Shape::Round([x, y, w, h], radius, pill));
    let knob = h - 2.0 * inset;
    let kx = if on { x + w - knob - inset } else { x + inset };
    out.push(Shape::Round([kx, y + inset, knob, knob], (radius - inset).max(0.0), dot));
}

// A close button's cross in the middle of `r`: quiet until the pointer is on it.
pub(super) fn cross(out: &mut Vec<Shape>, r: [f32; 4], lit: bool) {
    let (cx, cy, d) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, 2.5);
    for (a, b) in [([cx - d, cy - d], [cx + d, cy + d]), ([cx - d, cy + d], [cx + d, cy - d])] {
        line(out, a, b, if lit { 0xFFFF_FFFF } else { 0xFF9A_9AA2 }, 1.0);
    }
}

// A scroll marker on its track, `len` long from `y` down: how much of a list shows (`room` of `content`), and where.
pub(super) fn scrollbar(out: &mut Vec<Shape>, x: f32, y: f32, len: f32, content: f32, room: f32, at: f32) {
    if content <= room + 0.01 || len <= 0.0 {
        return;
    }
    out.push(Shape::Round([x, y, 2.0, len], 1.0, 0x18FF_FFFF));
    let thumb = (len * room / content).max(10.0).min(len);
    let t = (at / (content - room)).clamp(0.0, 1.0);
    out.push(Shape::Round([x, y + (len - thumb) * t, 2.0, thumb], 1.0, 0x80FF_FFFF));
}

// The edge of a scrolling list that has more past it: the rows fade into the surface (`colour`) over a few pixels, so
// a row the edge cuts reads as "more this way". `down`: the list's bottom edge at `y`; else its top edge.
pub(super) fn fade(out: &mut Vec<Shape>, [x, y, w]: [f32; 3], colour: u32, px: f32, down: bool) {
    const STEPS: u32 = 12;
    for k in 0..STEPS {
        // The strongest strip is the one at the edge.
        let alpha = 0xF0 * (k + 1) / STEPS;
        let off = if down { -((STEPS - k) as f32) } else { (STEPS - 1 - k) as f32 };
        out.push(Shape::Fill([x, y + off * px, w, px], alpha << 24 | colour & 0x00FF_FFFF));
    }
}

// What a search box holds: what was typed, with a caret at byte `caret` of it (the end of a long one shows), or what
// it searches, in grey. A caret past the text's end is none: the box is not where typing goes just now (a bind waits
// for its key, a menu is open over it). The box itself is the caller's.
pub(super) fn search_text(game: &Game<'_>, out: &mut Vec<Shape>, r: [f32; 4], typed: &str, caret: usize, hint: &str) {
    let room = (r[2] - 16.0).max(0.0);
    let at = [r[0] + 8.0, r[1] + text_y(r[3])];
    if typed.is_empty() {
        out.push(Shape::Text(fit_to(game, hint, room), at, 0x8C8C94));
        return;
    }
    let mut from = 0;
    while from < typed.len() && game.sunset_text_width(&typed[from..]) > room - 2.0 {
        from += typed[from..].chars().next().map_or(1, char::len_utf8);
    }
    out.push(Shape::Text(typed[from..].to_owned(), at, 0xFFFFFF));
    if caret > typed.len() {
        return;
    }
    let caret = caret.clamp(from, typed.len());
    let cx = game.sunset_text_width(&typed[from..caret]);
    out.push(Shape::Fill([at[0] + cx + 0.5, r[1] + 3.0, 1.0, r[3] - 6.0], 0xFF00_0000 | accent()));
}

// How many names a list setting holds, as its row says it: "11 blocks", "1 item", "None".
pub(super) fn list_count(kind: super::sunset_picker::ListKind, text: &str) -> String {
    use super::sunset_picker::ListKind;
    let n = super::sunset_picker::chosen(text).len();
    let (one, many) = match kind {
        ListKind::Blocks => ("block", "blocks"),
        ListKind::Items => ("item", "items"),
        ListKind::Entities => ("mob", "mobs"),
        ListKind::Particles => ("particle", "particles"),
        ListKind::Players => ("player", "players"),
    };
    match n {
        0 => "None".to_owned(),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

// The width the panels are drawn: the Panel width asked for, or less, so all of them sit in a row on a screen `w` wide.
pub(super) fn panel_width(asked: f32, w: f32) -> f32 {
    let fits = ((w - 4.0 - 3.0 * (CATEGORIES.len() as f32 - 1.0)) / CATEGORIES.len() as f32).floor();
    asked.min(fits).max(50.0)
}

// Where each panel is drawn on a screen `w` by `h`, the panels `width` wide (`asked` wanted). Each panel's place of
// your own (its home) stays as you left it, spaced for the Panel width asked for; where it is drawn is worked out from
// it every frame, spread or closed up to the width the panels have now and kept on the screen. So no window size (F11
// out and back, a resize, the sizes a window passes through on the way) moves a place: the panels come back where
// they were, apart as they were.
pub(super) fn lay_out(gui: &mut Gui, asked: f32, width: f32, w: f32, h: f32, head: f32) {
    if gui.laid_for > 0.0 && gui.laid_for != asked {
        // Another Panel width: the places spread or close up with it, so a layout of your own keeps its shape.
        let k = (asked + 3.0) / (gui.laid_for + 3.0);
        for p in gui.panels.iter_mut().filter(|p| p.home[0] >= 0.0) {
            p.home[0] = 2.0 + (p.home[0] - 2.0).max(0.0) * k;
        }
    }
    gui.laid_for = asked;
    // Panels with no place yet go left to right in a row (the width drawn always fits one).
    let mut x = 2.0;
    for p in gui.panels.iter_mut().filter(|p| p.home[0] < 0.0) {
        p.home = [x, 32.0];
        x += asked + 3.0;
    }
    let k = (width + 3.0) / (asked + 3.0);
    for p in &mut gui.panels {
        p.x = (2.0 + (p.home[0] - 2.0) * k).clamp(0.0, (w - width).max(0.0));
        p.y = p.home[1].clamp(0.0, (h - head).max(0.0));
    }
}

// The width, in the menu's own units, at which nothing of it has to be narrowed: the panels in one row at the Panel
// width asked for, or the Dashboard's window as wide as it wants to be. None for the Sidebar, which sizes itself.
pub(super) fn wanted_row() -> Option<f32> {
    if super::sunset_dashboard::on() {
        return super::sunset_dashboard::wanted_width();
    }
    let n = CATEGORIES.len() as f32;
    Some(4.0 + n * wanted_panel_w() + 3.0 * (n - 1.0) + 1.0)
}

// The panels' search box, in the middle over them, and the cross at its end that empties it.
pub(super) fn search_field(w: f32) -> [f32; 4] {
    let fw = 150.0_f32.min(w - 8.0);
    [((w - fw) / 2.0).floor(), 15.0, fw, 14.0]
}

pub(super) fn search_clear(w: f32) -> [f32; 4] {
    let [x, y, fw, fh] = search_field(w);
    [x + fw - fh, y, fh, fh]
}

// What the keys and clicks do now, for the strip over the menu: while a bind waits for its key, with the Profiles
// menu open, while searching, or else. The panels and the Dashboard say the same.
pub(super) fn gui_hint(binding: bool, menu_open: bool, searching: bool) -> &'static str {
    if binding {
        "Press a key or a mouse button   Esc: cancel   Backspace: clear the bind"
    } else if menu_open {
        "Click a row to choose it   Left and Right: tabs   Esc: back"
    } else if searching {
        "Enter: toggle the first result   Left click: on or off   Right click: settings   Esc: clear the search"
    } else {
        "Left click: on or off   Right click: settings   Type to search   Esc: close"
    }
}

pub(super) fn draw_gui(game: &Game<'_>, modules: &[Module], w: f32, h: f32, px: f32, out: &mut Vec<Shape>) {
    check_measured(game);
    follow_drag(game, w, h);
    with(|s| {
        use super::sunset_smooth as smooth;
        super::sunset_dashboard::prepare(s);
        let mut targets = [None; smooth::SLOTS];
        for (target, p) in targets.iter_mut().zip(&s.gui.panels) {
            *target = Some(p.scroll);
        }
        targets[smooth::WINDOW] = s.gui.window.map(|w| w.scroll);
        targets[smooth::DASHBOARD] = Some(super::sunset_dashboard::list_scroll());
        targets[smooth::PANE] = Some(super::sunset_dashboard::pane_scroll());
        targets[smooth::PICKER] = s.gui.editor.as_ref().and_then(|e| e.list.as_ref()).map(|p| p.scroll);
        smooth::frame(targets);
    });
    // The panels are as wide as asked, or as wide as lets all of them sit in a row.
    let asked = wanted_panel_w();
    let width = panel_width(asked, w);
    LOOK[4].store(width.to_bits(), Ordering::Relaxed);
    with(|s| {
        s.gui.view_h = h;
        s.gui.view_w = w;
        lay_out(&mut s.gui, asked, width, w, h, head_h());
        // The Dashboard has no panels to keep in order.
        if super::sunset_dashboard::on() {
            return;
        }
        // A scroll past the end (a shorter body, a taller window) comes back.
        for c in 0..CATEGORIES.len() {
            let (rows, content) = panel_body(s, c);
            let room = body_room(s, c);
            s.gui.panels[c].scroll = snap_scroll(&rows, content, room, s.gui.panels[c].scroll, 0);
        }
    });
    // The settings window: its description in lines, sized to what it holds and kept on the screen where it is, 4
    // short of the bottom: a taller one scrolls (three rows at least, moved up if need be). The description takes
    // six lines at most, the last cut short (the window's own module shows no tip).
    let about = with(|s| s.gui.window.map(|w| (s.modules[w.module].about, win_w(s)))).map_or_else(Vec::new, |(text, ww)| {
        let mut lines = wrap(game, text, ww - 2.0 * indent());
        if lines.len() > 6 {
            let rest = lines.split_off(5).join(" ");
            lines.push(fit_to(game, &rest, ww - 2.0 * indent()));
        }
        lines
    });
    let window = with(|s| {
        let mut win = s.gui.window?;
        win.about_h = if about.is_empty() { 0.0 } else { about.len() as f32 * 10.0 + 4.0 };
        win.content = win_body(s, win.module).1;
        // Drawn where it was opened or dragged to (its home), kept on the screen; the home itself stays.
        (win.x, win.y) = (win.home[0], win.home[1]);
        win.room = (h - 4.0 - win.y.max(0.0) - head_h() - win.about_h - body_pad()).max(row_hh() * 3.0);
        win.scroll = win.scroll.clamp(0.0, (win.content - win.room).max(0.0));
        let [_, _, ww, wh] = win_frame(s, &win);
        win.x = win.home[0].clamp(0.0, (w - ww).max(0.0));
        win.y = win.home[1].clamp(0.0, (h - wh).max(0.0));
        s.gui.window = Some(win);
        Some((win, win_frame(s, &win), win_rows(s), about))
    });
    let at = gui_cursor(game);
    let (rows, binding, under, editing) = with(|s| (panel_rows(s), s.binding, hit_at(s, at), s.gui.editor.clone()));
    // What the pointer is on, and that row's place (a tip goes beside it).
    // Behind a dialog or the profiles menu nothing is pointed at.
    let modal = editing.as_ref().is_some_and(|e| !e.inline) || with(|s| s.gui.profile_menu.is_some());
    let under = under.filter(|_| !modal);
    let (hover, hover_row) = (under.map(|(_, h)| h), under.map(|(r, _)| r));
    let searching = with(|s| !s.gui.search.trim().is_empty());
    // Modules whose settings are open (inline or in the window): a mark by the name.
    let open: Vec<usize> = with(|s| s.gui.expanded.iter().copied().chain(s.gui.window.map(|w| w.module)).collect());
    // Each panel's height of rows, room, scroll, whether it is folded, and how many modules it lists.
    let bodies: Vec<(f32, f32, f32, bool, usize)> = with(|s| {
        (0..CATEGORIES.len())
            .filter(|_| !super::sunset_dashboard::on())
            .map(|c| {
                let (rows, content) = panel_body(s, c);
                let listed = rows.iter().filter(|(_, hit)| matches!(hit, Hit::Module(_))).count();
                (content, body_room(s, c), super::sunset_smooth::shown(c, s.gui.panels[c].scroll), s.gui.panels[c].collapsed, listed)
            })
            .collect()
    });
    let fit = |text: &str, max: f32| fit_to(game, text, max);
    let solid = |rgb: u32| 0xFF00_0000 | rgb;
    let control = control_radius();
    let dash = super::sunset_dashboard::on();
    // The label on the left of a row, cut short of what sits to its right: whether it had to be.
    let label = |out: &mut Vec<Shape>, text: &str, r: [f32; 4], x: f32, right: f32, colour: u32| -> bool {
        let room = r[0] + r[2] - RIGHT_PAD - right - x - 3.0;
        let shown = fit(text, room);
        let cut = shown != text;
        out.push(Shape::Text(shown, [x, r[1] + text_y(row_hh())], colour));
        cut
    };
    // A value in its rounded box at the right of a row; how wide it is.
    let value_box = |out: &mut Vec<Shape>, text: &str, r: [f32; 4], colour: u32| -> f32 {
        let text = fit(text, r[2] * 0.5);
        let tw = width_of(game, &text);
        let bw = tw + 8.0;
        let bx = r[0] + r[2] - RIGHT_PAD - bw;
        out.push(Shape::Round([bx, r[1] + 2.0, bw, row_hh() - 4.0], control, pal().value_box));
        out.push(Shape::Text(text, [bx + 4.0, r[1] + text_y(row_hh())], colour));
        bw
    };
    // A value box with a mark at its end (one-line rows): ">" for a value a click steps on, "..." for a list a click
    // opens; how wide.
    let marked_box = |out: &mut Vec<Shape>, text: &str, r: [f32; 4], list: bool| -> f32 {
        value_box(out, &format!("{text} {}", if list { "..." } else { ">" }), r, 0xE4E4E4)
    };
    // A number being typed in its slider row: the field's width (the draft and its caret are drawn in it).
    let typing = |i: usize, j: usize| editing.as_ref().filter(|e| e.inline && e.module == i && e.setting == j).and_then(|e| e.draft.as_ref().ok());
    // One row of a panel or of the settings window: what it shows, and its name when it had to be cut short.
    let draw_row = |out: &mut Vec<Shape>, r: [f32; 4], hit: Hit, hovered: bool| -> Option<&'static str> {
        let [x, y, rw, rh] = r;
        let setting_x = x + indent() + 4.0;
        match hit {
            Hit::Module(i) => {
                let m = &modules[i];
                // A bound key shows at the row's end.
                let tag = m.key.as_deref().map(|k| fit(&key_label(Some(k)), 30.0)).unwrap_or_default();
                let tag_w = if tag.is_empty() { 0.0 } else { width_of(game, &tag) };
                // The name clears the mark of open settings at any Text indent.
                let name_x = x + indent().max(6.0);
                if open.contains(&i) && !dash {
                    out.push(Shape::Round([name_x - 5.0, y + 3.0, 2.0, rh - 6.0], 1.0, 0xFFE4_E4E4));
                }
                // The reworked Dashboard: a switch at each module's row end, its key before it.
                let sw = if super::sunset_dashboard::one_line() { MODULE_SWITCH } else { 0.0 };
                if sw > 0.0 {
                    switch(out, [x + rw - RIGHT_PAD - 14.0, y + ((rh - 8.0) / 2.0).floor(), 14.0, 8.0], m.on, 0);
                }
                if !tag.is_empty() {
                    out.push(Shape::Text(tag, [x + rw - RIGHT_PAD - sw - tag_w, y + text_y(rh)], 0x707070));
                }
                (label(out, m.name, r, name_x, tag_w + sw, if m.on { accent() } else { 0xE4E4E4 }) && hovered).then_some(m.name)
            }
            Hit::Bind(i) => {
                let waiting = binding == Some(i);
                // One line: the bind's value box keeps its place and asks for the key.
                if waiting && super::sunset_dashboard::one_line() {
                    let bw = value_box(out, "Press a key", r, accent());
                    label(out, "Bind", r, setting_x, bw, 0xB4B4B4);
                    return None;
                }
                if waiting {
                    // The row keeps its name with the prompt beside it; where both do not fit, the prompt takes the
                    // row, so it is never cut to "Press a...".
                    let (prompt, lw) = ("Press a key", width_of(game, "Bind") + 6.0);
                    let named = setting_x + lw + width_of(game, prompt) + 8.0 <= x + rw - RIGHT_PAD;
                    let from = if named { setting_x + lw } else { setting_x };
                    let b = [from, y + 2.0, x + rw - RIGHT_PAD - from, row_hh() - 4.0];
                    out.push(Shape::Round(b, control, pal().value_box));
                    let text = fit(prompt, b[2] - 6.0);
                    let tw = width_of(game, &text);
                    out.push(Shape::Text(text, [b[0] + (b[2] - tw) / 2.0, y + text_y(row_hh())], accent()));
                    if named {
                        out.push(Shape::Text("Bind".to_owned(), [setting_x, y + text_y(row_hh())], 0xB4B4B4));
                    }
                    return None;
                }
                let key = key_label(modules[i].key.as_deref());
                let bw = value_box(out, &key, r, 0xC8C8C8);
                label(out, "Bind", r, setting_x, bw, 0xB4B4B4);
                None
            }
            Hit::Setting(i, j) => {
                let st = &modules[i].settings[j];
                let right = match st.value {
                    Value::Bool(b) => {
                        let (tw, th) = (14.0, 8.0);
                        let (tx, ty) = (x + rw - RIGHT_PAD - tw, y + (rh - th) / 2.0);
                        switch(out, [tx, ty, tw, th], b, 0);
                        tw
                    }
                    Value::Num { v, min, max, .. } => {
                        let vw = match typing(i, j) {
                            Some(draft) => {
                                let fw = field_width(game, draft.text());
                                let fx = x + rw - RIGHT_PAD - fw + 3.0;
                                let colour = match (draft.error, draft.selected()) {
                                    (Some(_), _) => 0xFF60_2020,
                                    (None, true) => 0xFF30_60B0,
                                    (None, false) => pal().field,
                                };
                                out.push(Shape::Round([fx - 3.0, y + 1.0, fw, row_hh() - 2.0], control, colour));
                                out.push(Shape::Text(draft.text().to_owned(), [fx, y + text_y(row_hh())], 0xFFFFFF));
                                let caret = game.sunset_text_width(&draft.text()[..draft.caret()]);
                                out.push(Shape::Fill([fx + caret, y + 3.0, 1.0, row_hh() - 6.0], solid(accent())));
                                fw
                            }
                            None => {
                                let value = st.shown_value();
                                let vw = width_of(game, &value);
                                out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0xE4E4E4));
                                vw
                            }
                        };
                        let (tx, track) = slider_track(r);
                        let t = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                        // One line (the Dashboard): the track beside the name, in the row's middle, and not under a
                        // number being typed (its field can be wider than the number's place).
                        let one = super::sunset_dashboard::one_line();
                        let ty = if one { y + ((rh - 2.0) / 2.0).floor() } else { y + row_hh() };
                        if !(one && typing(i, j).is_some()) {
                            out.push(Shape::Round([tx, ty, track, 2.0], 1.0, pal().track));
                            out.push(Shape::Round([tx, ty, track * t, 2.0], 1.0, solid(accent())));
                            out.push(Shape::Round([tx + track * t - 2.5, ty - 1.5, 5.0, 5.0], control.min(2.5), 0xFFE4_E4E4));
                        }
                        if one { x + rw - RIGHT_PAD - tx + 3.0 } else { vw }
                    }
                    // A short pick in a one-line row: its options side by side, the chosen one lit (not in the accent
                    // colour, which is for what is on); a click picks one.
                    Value::Pick { i: picked, of } => match segments(game, r, of) {
                        Some(parts) => {
                            segments_drawn(|drawn| {
                                drawn.retain(|(m, s, _)| (*m, *s) != (i, j));
                                if drawn.len() >= 64 {
                                    drawn.remove(0);
                                }
                                drawn.push((i, j, parts.clone()));
                            });
                            let from = parts[0][0];
                            out.push(Shape::Round([from, parts[0][1], x + rw - RIGHT_PAD - from, parts[0][3]], control, pal().value_box));
                            for (k, part) in parts.iter().enumerate() {
                                let tw = width_of(game, of[k]);
                                let at = [(part[0] + (part[2] - tw) / 2.0).floor(), y + text_y(row_hh())];
                                if k == picked {
                                    out.push(Shape::Round(*part, control, 0x5AFF_FFFF));
                                    out.push(Shape::Head(of[k].to_owned(), at, 0xFFFFFF));
                                } else {
                                    out.push(Shape::Text(of[k].to_owned(), at, 0x9A9AA2));
                                }
                            }
                            x + rw - RIGHT_PAD - from + 3.0
                        }
                        // One line: a mark that a click steps it on.
                        None if super::sunset_dashboard::one_line() => marked_box(out, &st.value_text(), r, false),
                        None => value_box(out, &st.value_text(), r, 0xE4E4E4),
                    },
                    // A list of blocks, items, mobs or players: how many it holds, in a box (a click opens its picker).
                    Value::Text(ref t) if super::sunset_picker::list_kind(modules[i].name, st.name).is_some() => {
                        use super::sunset_picker as pick;
                        let kind = pick::list_kind(modules[i].name, st.name).unwrap_or(pick::ListKind::Blocks);
                        // Xray's Simple list ticks kinds of block (ore and deepslate ore are one row): counted so.
                        let full = if modules[i].name == "Xray" && modules[i].p("List") == "Simple" {
                            let have = pick::chosen(t);
                            let n = pick::VALUABLES.iter().filter(|g| g.iter().all(|b| have.contains(b))).count();
                            format!("{n} of {}", pick::VALUABLES.len())
                        } else {
                            list_count(kind, t)
                        };
                        // Where the name would be cut for it, only the number.
                        let room = x + rw - RIGHT_PAD - setting_x - 3.0;
                        let text = if width_of(game, st.label()) + width_of(game, &full) + 8.0 > room { full.split(' ').next().unwrap_or_default().to_owned() } else { full };
                        // One line: a mark that a click opens its list.
                        if super::sunset_dashboard::one_line() { marked_box(out, &text, r, true) } else { value_box(out, &text, r, 0xE4E4E4) }
                    }
                    Value::Text(_) => {
                        // The text itself: the name keeps its width, the text gets what is left (a quarter at least).
                        let room = x + rw - RIGHT_PAD - setting_x - width_of(game, st.name) - 6.0;
                        let value = fit(&st.value_text(), room.max(rw * 0.25));
                        let vw = width_of(game, &value);
                        out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0xE4E4E4));
                        vw
                    }
                };
                (label(out, st.label(), r, setting_x, right, 0xB4B4B4) && hovered).then_some(st.label())
            }
            _ => None,
        }
    };
    // The pointer's row lit edge to edge, between the lines; at the bottom of a panel, down to its rounded corners.
    let lit = |out: &mut Vec<Shape>, [x, y, rw, rh]: [f32; 4], line: bool, last: bool| {
        let top = if line { px } else { 0.0 };
        let r = [x, y + top, rw, rh - top + if last { body_pad() } else { 0.0 }];
        out.push(Shape::Corners(r, panel_radius(), [false, false, last, last], 0x14FF_FFFF));
    };
    if dash {
        with(|s| super::sunset_dashboard::back(s, game, hover, px, out));
    }
    let mut whole: Option<&str> = None;
    let mut first_row = false;
    let mut fades: Vec<[f32; 3]> = Vec::new();
    // The list the rows scroll in, top and bottom: a row part way out of it is cut.
    let mut body = with(|s| super::sunset_dashboard::list_y(s)).unwrap_or([f32::MIN, f32::MAX]);
    for (k, (r, hit)) in rows.iter().enumerate() {
        let [x, y, rw, rh] = *r;
        let hovered = hover == Some(*hit);
        let cut = !matches!(hit, Hit::Header(_)) && (y < body[0] || y + rh > body[1]);
        if cut {
            out.push(Shape::ClipStart);
        }
        let line = if let Hit::Header(_) = hit {
            first_row = true;
            false
        } else if std::mem::take(&mut first_row) {
            // The header's line is under it already.
            false
        } else if dash && (y <= body[0] + 0.01 || !super::sunset_dashboard::line_above(*hit)) {
            // The Dashboard draws lines only between the settings of a section.
            false
        } else {
            out.push(Shape::Fill([x, y, rw, px], pal().line));
            true
        };
        if let Hit::Header(c) = *hit {
            let (content, room, scroll, folded, listed) = bodies[c];
            body = [y + rh, y + rh + room];
            let body = if folded || content <= 0.0 { 0.0 } else { content.min(room) + body_pad() };
            framed(out, [x, y, rw, rh + body], panel_radius(), bg(pal().panel), px);
            if body > 0.0 {
                out.push(Shape::Fill([x, y + rh - px, rw, px], pal().rule));
            }
            let name = CATEGORIES[c].label();
            // Beside the name, what the number counts: the panel's modules that are on, or what the search found in
            // it. A panel too narrow for the words keeps the number.
            let on = modules.iter().filter(|m| m.category == CATEGORIES[c] && m.in_gui && m.on).count();
            let (n, what) = if searching { (listed, "found") } else { (on, "on") };
            let mut count = format!("{n} {what}");
            if game.sunset_text_width(&count) + game.sunset_text_width(name) + 6.0 > rw - indent() - RIGHT_PAD {
                count = format!("{n}");
            }
            let cw = game.sunset_text_width(&count);
            out.push(Shape::Head(fit(name, rw - indent() - RIGHT_PAD - cw - 3.0), [x + indent(), y + text_y(rh)], 0xFFFFFF));
            out.push(Shape::Text(count, [x + rw - RIGHT_PAD - cw, y + text_y(rh)], 0x808080));
            // The scroll marker: how much of the panel shows, and where.
            if !folded {
                scrollbar(out, x + rw - 4.0, y + rh + 1.0, room - 2.0, content, room, scroll);
                // More rows under the screen's edge: the last ones fade (drawn over the rows, after them).
                if content - scroll > room + 0.5 {
                    fades.push([x + 2.0, y + rh + room, rw - 8.0]);
                }
            }
            continue;
        }
        if dash {
            // A Dashboard row fades as the pointer comes and goes.
            let glow = super::sunset_dashboard::glow(*hit);
            if glow > 0 {
                out.push(Shape::Fill(*r, glow << 24 | 0x00FF_FFFF));
            }
        } else if hovered {
            let last = rows.get(k + 1).is_none_or(|(_, next)| matches!(next, Hit::Header(_)));
            lit(out, *r, line, last);
        }
        // A panel's settings (open in it, or found by the search) hang off a thin line under their module.
        if !dash && matches!(hit, Hit::Setting(..) | Hit::Bind(_)) {
            out.push(Shape::Fill([x + indent() + 1.0, y, px, rh], pal().rule));
        }
        let text = if dash && matches!(hit, Hit::Module(_)) { super::sunset_dashboard::module_text(*r) } else { *r };
        if let Some(name) = draw_row(out, text, *hit, hovered) {
            whole = Some(name);
        }
        if cut {
            out.push(Shape::ClipEnd([x, body[0], x + rw, body[1]]));
        }
    }
    if dash {
        with(|s| super::sunset_dashboard::front(s, px, out));
    }
    for at in fades {
        fade(out, at, pal().panel, px, true);
    }
    // The settings window, over the panels: its title, the module's switch, the close button, description, groups and
    // rows.
    if let Some((win, frame, list, about)) = &window {
        let [x, y, ww, wh] = *frame;
        // An edge of its own in both themes: it lies over panels of nearly its colour.
        out.push(Shape::Round([x - px, y - px, ww + 2.0 * px, wh + 2.0 * px], panel_radius() + px, 0xFF4A_4A52));
        out.push(Shape::Round(*frame, panel_radius(), pal().frame));
        let hh = head_h();
        let toggle = win_toggle(*frame);
        out.push(Shape::Head(fit(modules[win.module].name, toggle[0] - x - indent() - 4.0), [x + indent(), y + text_y(hh)], 0xFFFFFF));
        switch(out, toggle, modules[win.module].on, if hover == Some(Hit::WinToggle) { 0x10 } else { 0 });
        // The close button: a quiet cross, lit under the pointer.
        let close = win_close(*frame);
        let on_close = hover == Some(Hit::WinClose);
        if on_close {
            out.push(Shape::Corners(close, panel_radius(), [false, true, false, false], 0x24FF_FFFF));
        }
        cross(out, close, on_close);
        out.push(Shape::Fill([x, y + hh - px, ww, px], pal().rule));
        for (n, l) in about.iter().enumerate() {
            out.push(Shape::Text(l.clone(), [x + indent(), y + hh + 3.0 + n as f32 * 10.0], 0xA8A8A8));
        }
        let mut line = false;
        let view = [y + hh + win.about_h, y + hh + win.about_h + win.room];
        for (n, (r, row)) in list.iter().enumerate() {
            let cut = r[1] < view[0] || r[1] + r[3] > view[1];
            if cut {
                out.push(Shape::ClipStart);
            }
            match row {
                WinRow::Group(name) => {
                    // A section's name is quiet: the accent colour is for what is on.
                    out.push(Shape::Text((*name).to_owned(), [r[0] + indent(), r[1] + text_y(r[3])], 0x8C8C94));
                    line = false;
                }
                WinRow::Row(hit) => {
                    if line {
                        out.push(Shape::Fill([r[0], r[1], r[2], px], pal().line));
                    }
                    let hovered = hover == Some(*hit);
                    if hovered {
                        lit(out, *r, line, n + 1 == list.len() && r[1] + r[3] + body_pad() >= y + wh - 0.5);
                    }
                    if let Some(name) = draw_row(out, *r, *hit, hovered) {
                        whole = Some(name);
                    }
                    line = true;
                }
            }
            if cut {
                out.push(Shape::ClipEnd([r[0], view[0], r[0] + r[2], view[1]]));
            }
        }
        // Its scroll marker.
        let at = super::sunset_smooth::shown(super::sunset_smooth::WINDOW, win.scroll);
        if win.content - at > win.room + 0.5 {
            fade(out, [x + 2.0, view[1], ww - 8.0], pal().frame, px, true);
        }
        scrollbar(out, x + ww - 4.0, y + hh + win.about_h + 1.0, win.room - 2.0, win.content, win.room, at);
    }
    // Where a tip goes: beside the row it is about (to its right, or its left where the screen ends), level with it,
    // so it covers neither the row's own controls nor the rows under it. In the Dashboard, whose settings are beside
    // the list: under the row (over it at the list's end), inside its column.
    let floor = if dash { body[1] } else { h - 2.0 };
    let open_window = window.as_ref().map(|(_, frame, ..)| *frame);
    let tip_at = |row: [f32; 4], tw: f32, th: f32| -> [f32; 2] {
        if dash {
            let under = row[1] + row[3] + 1.0;
            let y = if under + th <= floor { under } else { row[1] - th - 1.0 };
            return [(row[0] + 6.0).min(w - tw - 2.0).max(0.0), y.max(0.0)];
        }
        let y = row[1].min(h - th - 2.0).max(0.0);
        let sides = [row[0] + row[2] + 3.0, row[0] - 3.0 - tw];
        let fits = |x: f32| x >= 2.0 && x + tw <= w - 2.0;
        // Off the settings window too, which opens beside its row as well.
        let clear = |x: f32| open_window.is_none_or(|f| x + tw <= f[0] || x >= f[0] + f[2] || y + th <= f[1] || y >= f[1] + f[3]);
        let x = sides.into_iter().find(|&x| fits(x) && clear(x)).or_else(|| sides.into_iter().find(|&x| fits(x)));
        [x.unwrap_or_else(|| sides[0].min(w - tw - 2.0).max(0.0)), y]
    };
    // The hovered module's description, beside its row.
    // (Not for the module whose settings window is open: the window says it.)
    if let (Some(Hit::Module(i)), Some(row)) = (hover, hover_row)
        && window.as_ref().is_none_or(|(win, ..)| win.module != i)
        && with(|s| s.gui.drag.is_none())
        && super::sunset_dashboard::wants_tip(i)
    {
        let text = modules[i].about;
        let about = match whole {
            Some(name) => format!("{name}: {text}"),
            None => text.to_owned(),
        };
        // A narrow card (in the Dashboard, as wide as the list) kept on the screen.
        let width = if dash { (row[2] - 20.0).max(100.0) } else { (w * 0.3).clamp(120.0, 200.0) };
        let lines = wrap(game, &about, width.min(w - 16.0));
        let tw = lines.iter().map(|l| game.sunset_text_width(l)).fold(0.0, f32::max);
        let th = lines.len() as f32 * 10.0 + 4.0;
        let [bx, by] = tip_at(row, tw + 8.0, th);
        framed(out, [bx, by, tw + 8.0, th], panel_radius(), pal().tooltip, px);
        for (n, line) in lines.into_iter().enumerate() {
            out.push(Shape::Text(line, [bx + 4.0, by + 3.0 + n as f32 * 10.0], 0xE4E4E4));
        }
    }
    // A setting's name cut short, whole under the pointer.
    if let (Some(Hit::Setting(..)), Some(name), Some(row)) = (hover, whole, hover_row) {
        let tw = game.sunset_text_width(name);
        let [bx, by] = tip_at(row, tw + 8.0, 14.0);
        framed(out, [bx, by, tw + 8.0, 14.0], panel_radius(), pal().tooltip, px);
        out.push(Shape::Text(name.to_owned(), [bx + 4.0, by + text_y(14.0)], 0xE4E4E4));
    }
    // The list picker, over everything.
    if let Some((edit, picker)) = editing.as_ref().and_then(|e| e.list.as_ref().map(|p| (e, p))) {
        draw_picker(game, w, h, edit, picker, &modules, at, out);
        return;
    }
    // The text editor, over everything.
    if let Some(edit) = editing.filter(|e| !e.inline) {
        let r = editor_rect(w, h);
        let [x, y, rw, _] = r;
        let room = (rw - 16.0).max(0.0);
        out.push(Shape::Fill([0.0, 0.0, w, h], 0x9000_0000));
        framed(out, r, panel_radius(), pal().dialog, px);
        let m = &modules[edit.module];
        out.push(Shape::Head(fit(&format!("{}: {}", m.name, m.settings[edit.setting].name), room), [x + 8.0, y + 6.0], 0xFFFFFF));
        let selected = edit.draft.as_ref().is_ok_and(Draft::selected);
        out.push(Shape::Round([x + 6.0, y + 22.0, rw - 12.0, 16.0], panel_radius(), if selected { 0xFF30_60B0 } else { pal().field }));
        let note = match &edit.draft {
            Ok(draft) => {
                // The end of the text near the caret shows, with the caret at its place.
                let before = &draft.text()[..draft.caret()];
                let mut from = 0;
                while game.sunset_text_width(&before[from..]) > room - 4.0 {
                    from += before[from..].chars().next().map_or(1, char::len_utf8);
                }
                let shown = fit(&draft.text()[from..], room);
                let caret = game.sunset_text_width(&before[from..]);
                out.push(Shape::Text(shown, [x + 10.0, y + 22.0 + text_y(16.0)], 0xFFFFFF));
                out.push(Shape::Fill([x + 10.0 + caret, y + 25.0, 1.0, 10.0], solid(accent())));
                draft.error.unwrap_or("Enter or Apply keeps it, Esc or Cancel leaves it as it was")
            }
            Err(e) => e,
        };
        out.push(Shape::Text(fit(note, room), [x + 8.0, y + 41.0], if matches!(&edit.draft, Ok(d) if d.error.is_none()) { 0x909090 } else { 0xFF7070 }));
        let (apply, cancel) = editor_buttons(r);
        for (b, label, colour) in [(apply, "Apply", solid(accent())), (cancel, "Cancel", pal().button)] {
            out.push(Shape::Round(b, panel_radius(), colour));
            let tw = game.sunset_text_width(label);
            let at = [b[0] + (b[2] - tw) / 2.0, b[1] + text_y(b[3])];
            out.push(if label == "Apply" { Shape::Plain(label.to_owned(), at, 0xFFFFFF, false) } else { Shape::Text(label.to_owned(), at, 0xFFFFFF) });
        }
    }
    // The saved profiles, when the Profiles module's row was clicked: over everything, the search box too.
    let profiles = |out: &mut Vec<Shape>| with(|s| super::sunset_profile_menu::draw(game, w, h, at, s.selection.selected(), &s.selection.label(), s.gui.profile_menu.as_ref(), out));
    // The Dashboard has its own search box.
    if dash {
        profiles(out);
        return;
    }
    // The search box (typing anywhere goes to it): a field of one width in the middle, as the Dashboard's and the
    // pickers' are; and over it a reminder of the clicks, on a strip of its own so it reads on any sky.
    let search = with(|s| s.gui.search.clone());
    let selected = with(|s| s.gui.select_all) && !search.is_empty();
    let menu_open = with(|s| s.gui.profile_menu.is_some());
    let field = search_field(w);
    framed(out, field, panel_radius(), if selected { 0xFF30_60B0 } else { bg(pal().panel) }, px);
    // Typing goes to the box unless a bind waits for its key or the Profiles menu is open: no caret then.
    let live = binding.is_none() && !menu_open;
    let clear = search_clear(w);
    let text_room = if search.is_empty() { field } else { [field[0], field[1], field[2] - clear[2] + 4.0, field[3]] };
    search_text(game, out, text_room, &search, if live { search.len() } else { usize::MAX }, "Search modules and settings");
    // A cross at its end empties it.
    if !search.is_empty() {
        cross(out, clear, live && inside(clear, at));
    }
    // A search that finds nothing says so (every panel is gone then).
    if searching && rows.is_empty() {
        let none = fit(&format!("Nothing matches \"{}\"", search.trim()), w - 16.0);
        let nw = game.sunset_text_width(&none);
        framed(out, [((w - nw) / 2.0 - 6.0).floor(), 33.0, nw + 12.0, 14.0], panel_radius(), bg(pal().panel), px);
        out.push(Shape::Text(none, [((w - nw) / 2.0).floor(), 33.0 + text_y(14.0)], 0xB4B4B4));
    }
    // What the keys and clicks do now: while a bind waits, while searching, or else.
    let hint = gui_hint(binding.is_some(), menu_open, searching);
    let hint = fit(hint, w - 16.0);
    let hw = game.sunset_text_width(&hint);
    framed(out, [((w - hw) / 2.0 - 6.0).floor(), 1.0, hw + 12.0, 12.0], panel_radius(), bg(pal().panel), px);
    out.push(Shape::Text(hint, [((w - hw) / 2.0).floor(), 1.0 + text_y(12.0)], 0xB4B4B4));
    profiles(out);
}

// A profile picked in the Profile list: loaded and the list closed, or the error shown in it.
// A saved profile loaded, a preset's preview shown, or the preset taken, from the Profiles row's menu.
pub(super) fn profile_menu_choice(game: &mut Game<'_>, choice: super::sunset_profile_menu::Choice) {
    use super::sunset_profile_menu::{self as menu, Choice};
    let preset = match choice {
        Choice::Load(name) => return load_profile_from_menu(game, &name),
        Choice::Save(name) => {
            let result = profile_action(game, &["save", &name]);
            return menu_result(game, result);
        }
        Choice::Apply(i) => {
            let result = take_preset(game, i);
            return menu_result(game, result);
        }
        Choice::Import(name) => {
            use super::sunset_profiles as profiles;
            // A name that is taken: the menu asks (keep both, or replace) before anything is written.
            let taken = profiles::Store::new(game.native_data_dir(ID)).is_ok_and(|store| profiles::name(&name).is_ok_and(|n| store.exists(&n)));
            if taken {
                return with(|s| {
                    if let Some(m) = &mut s.gui.profile_menu {
                        m.error = None;
                        m.ask = Some(name);
                        m.ask_file = None;
                    }
                });
            }
            let result = profile_action(game, &["import", &name]);
            return taken_in(game, result);
        }
        Choice::ImportAs(name, replace) => {
            // The answer for a file dropped on the menu goes to that file; else to the imports folder's, by its name.
            let dropped = with(|s| s.gui.profile_menu.as_mut().and_then(|m| m.ask_file.take()));
            let result = match dropped {
                Some(file) => profile_take_file(game, &file, Some(replace)),
                None => profile_action(game, &["import", &name, if replace { "replace" } else { "keep" }]),
            };
            return taken_in(game, result);
        }
        // The clipboard's profile is kept as "pasted" (numbered when that is taken: nothing of yours is replaced).
        Choice::Paste => {
            let result = profile_action(game, &["paste", "pasted", "keep"]);
            return taken_in(game, result);
        }
        Choice::Preview(i) => i,
    };
    let preview = with(|s| s.preview(&super::sunset_presets::PRESETS[preset]));
    let w = with(|s| s.gui.view_w);
    let preview = preview.map(|p| menu::preview_lines(game, w, preset, &preview_lines(&super::sunset_presets::PRESETS[preset], &p)[1..]));
    with(|s| {
        if let Some(m) = &mut s.gui.profile_menu {
            match preview {
                Ok(lines) => m.preview = Some(menu::Preview { preset, lines }),
                Err(e) => m.error = Some(e),
            }
        }
    });
}

// A file dropped on the game's window while the click GUI is open: a .profile file is taken in as an import is, with
// the Profiles menu open to show it. A name that is taken asks there (keep both, or replace); while a question is
// already waiting, the next taken name keeps both, and the question stays.
pub(super) fn profile_dropped(game: &mut Game<'_>, path: &std::path::Path) {
    use super::sunset_profiles as profiles;
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("profile")) || !with(|s| s.gui.open) {
        return;
    }
    if with(|s| s.gui.profile_menu.is_none()) {
        open_profile_menu(game);
    }
    let waiting = with(|s| s.gui.profile_menu.as_ref().and_then(|m| Some((m.ask.clone()?, m.ask_file.clone()))));
    let asks = |name: String, file: Option<std::path::PathBuf>| {
        with(|s| {
            let (w, h) = (s.gui.view_w, s.gui.view_h);
            if let Some(m) = &mut s.gui.profile_menu {
                m.asks(name, file, w, h);
            }
        });
    };
    let name = profiles::outside_name(path).ok();
    let taken = name.as_ref().is_some_and(|n| profiles::Store::new(game.native_data_dir(ID)).is_ok_and(|store| store.exists(n)));
    if let (true, None, Some(name)) = (taken, &waiting, name) {
        return asks(name, Some(path.to_owned()));
    }
    let result = profile_take_file(game, path, None);
    match waiting {
        None => taken_in(game, result),
        // The menu goes back to its question: what this file did is said as Sunset says things (a toast).
        Some((name, file)) => {
            if let Err(e) = &result {
                say(game, &format!("Profile: {e}."));
            }
            taken_in(game, result);
            asks(name, file);
        }
    }
}

// The file chooser that .profile import opened has closed: each file picked is taken in (a taken name keeps both),
// said in the chat (the late answer to a command typed there) and, with the Profiles menu open, shown there.
pub(super) fn profile_files_chosen(game: &mut Game<'_>) {
    let Some(answer) = super::sunset_profiles::chosen() else {
        return;
    };
    REPLYING.store(true, Ordering::Relaxed);
    profile_files_taken(game, answer);
    REPLYING.store(false, Ordering::Relaxed);
}

fn profile_files_taken(game: &mut Game<'_>, answer: Option<Vec<std::path::PathBuf>>) {
    let Some(files) = answer else {
        let folder = super::sunset_profiles::Store::new(game.native_data_dir(ID)).map(|store| store.imports().display().to_string()).unwrap_or_default();
        return say(game, &format!("Profile: no file chooser on this computer (zenity or kdialog). Drop .profile files on the Profiles menu, or put them in {folder} and use .profile import <name>."));
    };
    if files.is_empty() {
        return say(game, "Profile: nothing was chosen.");
    }
    for file in files {
        let result = profile_take_file(game, &file, None);
        if let Err(e) = &result {
            say(game, &format!("Profile: {e}."));
        }
        taken_in(game, result);
    }
}

// An import or a paste from the Profiles menu: done, the menu shows the saved profiles with the new one marked and
// says so (the chat too); else the menu shows why not.
fn taken_in(game: &mut Game<'_>, result: Result<String, String>) {
    let message = match result {
        Ok(message) => message,
        Err(e) => {
            return with(|s| {
                if let Some(m) = &mut s.gui.profile_menu {
                    m.ask = None;
                    m.error = Some(e);
                }
            });
        }
    };
    let names = super::sunset_profiles::Store::new(game.native_data_dir(ID)).and_then(|store| store.list()).unwrap_or_default();
    // "Imported profile NAME; .profile load NAME uses it." and what was left out: the menu says it its own way.
    let landed = message.split_once("profile ").and_then(|(_, rest)| rest.split_once(';')).map(|(name, _)| name.to_owned());
    let skipped = message.split_once(" uses it.").map_or("", |(_, rest)| rest);
    let note = match &landed {
        Some(name) => format!("{name} is saved: click it to load it.{skipped}"),
        None => message.clone(),
    };
    with(|s| {
        let (w, h) = (s.gui.view_w, s.gui.view_h);
        if let Some(m) = &mut s.gui.profile_menu {
            m.took(names, landed.as_deref(), note, w, h);
        }
    });
    say(game, &message);
}

fn load_profile_from_menu(game: &mut Game<'_>, name: &str) {
    let result = profile_action(game, &["load", name]);
    menu_result(game, result);
}

// A load from the Profiles menu: done, the menu closes and the chat says so; else the menu shows why not.
fn menu_result(game: &mut Game<'_>, result: Result<String, String>) {
    match result {
        Ok(message) => {
            with(|s| {
                s.gui.profile_menu = None;
                s.binding = None;
                s.gui.drag = None;
            });
            say(game, &message);
        }
        Err(message) => with(|s| {
            if let Some(menu) = &mut s.gui.profile_menu {
                menu.error = Some(message);
            }
        }),
    }
}
