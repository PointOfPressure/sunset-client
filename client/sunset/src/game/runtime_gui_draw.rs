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
    out.push(Shape::Round(r, panel_radius(), 0xF814_1414));
    let m = &modules[edit.module];
    out.push(Shape::Head(fit(&format!("{} / {}", m.name, m.settings[edit.setting].name), room), [x + 8.0, y + 6.0], accent()));
    let search = edit.draft.as_ref().map_or("", Draft::text);
    out.push(Shape::Round([x + 6.0, y + 22.0, rw - 12.0, 16.0], panel_radius(), 0xFF08_0808));
    let (shown, colour) = if search.is_empty() { ("Type to search", 0x909090) } else { (search, 0xFFFFFF) };
    out.push(Shape::Text(fit(shown, room - 4.0), [x + 10.0, y + 22.0 + text_y(16.0)], colour));
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
            out.push(Shape::Round(row, control, 0x40FF_FFFF));
        }
        out.push(Shape::Round([area[0] + 3.0, ry + 3.0, 6.0, 6.0], 3.0, if *have { solid(accent()) } else { 0xFF3A_3A3A }));
        // The block's, item's or mob's icon (only the rows on screen are drawn), then its name.
        let icon = pick::icon(game, picker, name, [area[0] + 13.0, ry + 1.0], pick::ROW - 2.0);
        let left = if icon.is_some() { 26.0 } else { 14.0 };
        // An icon can't be cut, so it waits outside the cut part until its row is all in view.
        icons.extend(icon.filter(|_| ry >= area[1] && ry + pick::ROW <= area[1] + area[3]));
        let label = if picker.kind == pick::ListKind::Players { (*name).to_owned() } else { pick::readable(name) };
        out.push(Shape::Text(fit(&label, area[2] - left - 4.0), [area[0] + left, ry + text_y(pick::ROW)], 0xFFFFFF));
    }
    out.push(Shape::ClipEnd([area[0], area[1], area[0] + area[2], area[1] + area[3]]));
    out.extend(icons);
    let chosen = rows.iter().filter(|(_, have)| *have).count();
    let note = format!("{chosen} chosen, {} more. Click to add or take away; Esc closes.", rows.len() - chosen);
    let done = pick::done_button(r);
    out.push(Shape::Text(fit(&note, done[0] - x - 12.0), [x + 8.0, done[1] + text_y(done[3])], 0x909090));
    out.push(Shape::Round(done, panel_radius(), solid(accent())));
    let tw = game.sunset_text_width("Done");
    out.push(Shape::Text("Done".to_owned(), [done[0] + (done[2] - tw) / 2.0, done[1] + text_y(done[3])], 0xFFFFFF));
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
    // three lines at most, the last cut short (its module's tooltip has it all).
    let about = with(|s| s.gui.window.map(|w| (s.modules[w.module].about, win_w(s)))).map_or_else(Vec::new, |(text, ww)| {
        let mut lines = wrap(game, text, ww - 2.0 * indent());
        if lines.len() > 3 {
            let rest = lines.split_off(2).join(" ");
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
    let (rows, binding, hover, editing) =
        with(|s| (panel_rows(s), s.binding, hit_at(s, at).map(|(_, h)| h), s.gui.editor.clone()));
    // Modules whose settings are open (inline or in the window): a mark by the name.
    let open: Vec<usize> = with(|s| s.gui.expanded.iter().copied().chain(s.gui.window.map(|w| w.module)).collect());
    // Each panel's height of rows, room, scroll and whether it is folded.
    let bodies: Vec<(f32, f32, f32, bool)> = with(|s| {
        (0..CATEGORIES.len())
            .filter(|_| !super::sunset_dashboard::on())
            .map(|c| (panel_body(s, c).1, body_room(s, c), super::sunset_smooth::shown(c, s.gui.panels[c].scroll), s.gui.panels[c].collapsed))
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
        out.push(Shape::Round([bx, r[1] + 2.0, bw, row_hh() - 4.0], control, 0xFF1E_1E1E));
        out.push(Shape::Text(text, [bx + 4.0, r[1] + text_y(row_hh())], colour));
        bw
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
                if !tag.is_empty() {
                    out.push(Shape::Text(tag, [x + rw - RIGHT_PAD - tag_w, y + text_y(rh)], 0x707070));
                }
                (label(out, m.name, r, name_x, tag_w, if m.on { accent() } else { 0xE4E4E4 }) && hovered).then_some(m.name)
            }
            Hit::Bind(i) => {
                let waiting = binding == Some(i);
                let key = if waiting { "Press a key or button".to_owned() } else { key_label(modules[i].key.as_deref()) };
                let bw = value_box(out, &key, r, if waiting { accent() } else { 0xC8C8C8 });
                label(out, "Bind", r, setting_x, bw, 0xB4B4B4);
                None
            }
            Hit::Setting(i, j) => {
                let st = &modules[i].settings[j];
                let right = match st.value {
                    Value::Bool(b) => {
                        let (tw, th) = (14.0, 8.0);
                        let (tx, ty) = (x + rw - RIGHT_PAD - tw, y + (rh - th) / 2.0);
                        // Off is the same pill in grey; on follows the accent colour.
                        out.push(Shape::Round([tx, ty, tw, th], control, if b { solid(accent()) } else { 0xFF5A_5A5A }));
                        let knob = th - 2.0;
                        let kx = if b { tx + tw - knob - 1.0 } else { tx + 1.0 };
                        out.push(Shape::Round([kx, ty + 1.0, knob, knob], (control - 1.0).max(0.0), 0xFF10_1010));
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
                                    (None, false) => 0xFF08_0808,
                                };
                                out.push(Shape::Round([fx - 3.0, y + 1.0, fw, row_hh() - 2.0], control, colour));
                                out.push(Shape::Text(draft.text().to_owned(), [fx, y + text_y(row_hh())], 0xFFFFFF));
                                let caret = game.sunset_text_width(&draft.text()[..draft.caret()]);
                                out.push(Shape::Fill([fx + caret, y + 3.0, 1.0, row_hh() - 6.0], solid(accent())));
                                fw
                            }
                            None => {
                                let value = st.value_text();
                                let vw = width_of(game, &value);
                                out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0xE4E4E4));
                                vw
                            }
                        };
                        let (tx, track) = slider_track(r);
                        let t = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                        let ty = y + row_hh();
                        out.push(Shape::Round([tx, ty, track, 2.0], 1.0, 0xFF2A_2A2A));
                        out.push(Shape::Round([tx, ty, track * t, 2.0], 1.0, solid(accent())));
                        out.push(Shape::Round([tx + track * t - 2.5, ty - 1.5, 5.0, 5.0], control.min(2.5), 0xFFE4_E4E4));
                        vw
                    }
                    Value::Pick { .. } => value_box(out, &st.value_text(), r, 0xE4E4E4),
                    Value::Text(_) => {
                        // The text itself: the name keeps its width, the text gets what is left (a quarter at least).
                        let room = x + rw - RIGHT_PAD - setting_x - width_of(game, st.name) - 6.0;
                        let value = fit(&st.value_text(), room.max(rw * 0.25));
                        let vw = width_of(game, &value);
                        out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0xE4E4E4));
                        vw
                    }
                };
                (label(out, st.name, r, setting_x, right, 0xB4B4B4) && hovered).then_some(st.name)
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
            out.push(Shape::Fill([x, y, rw, px], 0x12FF_FFFF));
            true
        };
        if let Hit::Header(c) = *hit {
            let (content, room, scroll, folded) = bodies[c];
            body = [y + rh, y + rh + room];
            let body = if folded || content <= 0.0 { 0.0 } else { content.min(room) + body_pad() };
            out.push(Shape::Round([x, y, rw, rh + body], panel_radius(), bg(0xC00E_0E0E)));
            if body > 0.0 {
                out.push(Shape::Fill([x, y + rh - px, rw, px], 0x24FF_FFFF));
            }
            let name = CATEGORIES[c].label();
            let on = modules.iter().filter(|m| m.category == CATEGORIES[c] && m.in_gui && m.on).count();
            let count = format!("{on}");
            let cw = game.sunset_text_width(&count);
            out.push(Shape::Head(fit(name, rw - indent() - RIGHT_PAD - cw - 3.0), [x + indent(), y + text_y(rh)], 0xFFFFFF));
            out.push(Shape::Text(count, [x + rw - RIGHT_PAD - cw, y + text_y(rh)], 0x808080));
            // The scroll marker: how much of the panel shows, and where.
            if !folded && content > room {
                let thumb = (room * room / content).max(10.0);
                let t = (scroll / (content - room)).min(1.0);
                out.push(Shape::Round([x + rw - 4.0, y + rh + 1.0 + (room - 2.0 - thumb) * t, 2.0, thumb], 1.0, 0x70FF_FFFF));
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
        let text = if dash && matches!(hit, Hit::Module(_)) { super::sunset_dashboard::module_text(*r) } else { *r };
        if let Some(name) = draw_row(out, text, *hit, hovered) {
            whole = Some(name);
        }
        if cut {
            out.push(Shape::ClipEnd([x, body[0], x + rw, body[1]]));
        }
    }
    // The settings window, over the panels: its title, close button, description, groups and rows.
    if let Some((win, frame, list, about)) = &window {
        let [x, y, ww, wh] = *frame;
        out.push(Shape::Round(*frame, panel_radius(), 0xF812_1212));
        let hh = head_h();
        out.push(Shape::Head(fit(modules[win.module].name, ww - indent() - hh - 4.0), [x + indent(), y + text_y(hh)], accent()));
        // The close button: a cross, lit under the pointer.
        let close = win_close(*frame);
        if hover == Some(Hit::WinClose) {
            out.push(Shape::Corners(close, panel_radius(), [false, true, false, false], 0x24FF_FFFF));
        }
        let (cx, cy, d) = (close[0] + close[2] / 2.0, close[1] + close[3] / 2.0, 3.0);
        for (a, b) in [([cx - d, cy - d], [cx + d, cy + d]), ([cx - d, cy + d], [cx + d, cy - d])] {
            line(out, a, b, 0xFFE4_E4E4, 1.5);
        }
        out.push(Shape::Fill([x, y + hh - px, ww, px], 0x24FF_FFFF));
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
                    out.push(Shape::Text((*name).to_owned(), [r[0] + indent(), r[1] + text_y(r[3])], accent()));
                    line = false;
                }
                WinRow::Row(hit) => {
                    if line {
                        out.push(Shape::Fill([r[0], r[1], r[2], px], 0x12FF_FFFF));
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
        let (content, room) = (win.content, win.room);
        if content > room {
            let thumb = (room * room / content).max(10.0);
            let t = (super::sunset_smooth::shown(super::sunset_smooth::WINDOW, win.scroll) / (content - room)).min(1.0);
            out.push(Shape::Round([x + ww - 4.0, y + hh + win.about_h + 1.0 + (room - 2.0 - thumb) * t, 2.0, thumb], 1.0, 0x70FF_FFFF));
        }
    }
    // The hovered module's description, under the pointer.
    if let Some(Hit::Module(i)) = hover
        && with(|s| s.gui.drag.is_none())
        && super::sunset_dashboard::wants_tip(i)
    {
        let text = modules[i].about;
        let about = match whole {
            Some(name) => format!("{name}: {text}"),
            None => text.to_owned(),
        };
        // Wrapped to half the screen and kept on it.
        let lines = wrap(game, &about, (w * 0.5).max(120.0).min(w - 16.0));
        let tw = lines.iter().map(|l| game.sunset_text_width(l)).fold(0.0, f32::max);
        let th = lines.len() as f32 * 10.0 + 4.0;
        let bx = (at.0 + 8.0).min(w - tw - 8.0).max(0.0);
        let by = (at.1 + 10.0).min(h - th).max(0.0);
        out.push(Shape::Round([bx, by, tw + 8.0, th], panel_radius(), 0xF80A_0A0A));
        for (n, line) in lines.into_iter().enumerate() {
            out.push(Shape::Text(line, [bx + 4.0, by + 3.0 + n as f32 * 10.0], 0xE4E4E4));
        }
    }
    // A setting's name cut short, whole under the pointer.
    if let (Some(Hit::Setting(..)), Some(name)) = (hover, whole) {
        let tw = game.sunset_text_width(name);
        let bx = (at.0 + 8.0).min(w - tw - 8.0).max(0.0);
        let by = (at.1 + 10.0).min(h - 14.0);
        out.push(Shape::Round([bx, by, tw + 8.0, 14.0], panel_radius(), 0xF80A_0A0A));
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
        out.push(Shape::Round(r, panel_radius(), 0xF814_1414));
        let m = &modules[edit.module];
        out.push(Shape::Head(fit(&format!("{} / {}", m.name, m.settings[edit.setting].name), room), [x + 8.0, y + 6.0], accent()));
        let selected = edit.draft.as_ref().is_ok_and(Draft::selected);
        out.push(Shape::Round([x + 6.0, y + 22.0, rw - 12.0, 16.0], panel_radius(), if selected { 0xFF30_60B0 } else { 0xFF08_0808 }));
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
        for (b, label, colour) in [(apply, "Apply", solid(accent())), (cancel, "Cancel", 0xFF3A_3A3A)] {
            out.push(Shape::Round(b, panel_radius(), colour));
            let tw = game.sunset_text_width(label);
            out.push(Shape::Text(label.to_owned(), [b[0] + (b[2] - tw) / 2.0, b[1] + text_y(b[3])], 0xFFFFFF));
        }
    }
    // The saved profiles, when the Profiles module's row was clicked.
    with(|s| super::sunset_profile_menu::draw(game, w, h, at, s.selection.selected(), s.gui.profile_menu.as_ref(), out));
    // The Dashboard has its own search box.
    if dash {
        return;
    }
    // The search box, and over it a reminder of the clicks.
    let search = with(|s| s.gui.search.clone());
    let (text, colour) = if search.is_empty() {
        ("Type to search".to_owned(), 0x909090)
    } else {
        (format!("Search: {search}_"), accent())
    };
    let selected = with(|s| s.gui.select_all) && !search.is_empty();
    let lw_ = game.sunset_text_width(&text);
    out.push(Shape::Round([(w - lw_) / 2.0 - 6.0, 14.0, lw_ + 12.0, 14.0], panel_radius(), if selected { 0xFF30_60B0 } else { bg(0xC00E_0E0E) }));
    out.push(Shape::Text(text, [(w - lw_) / 2.0, 14.0 + text_y(14.0)], if selected { 0xFFFFFF } else { colour }));
    let hint = "Right click: settings   Enter: toggle search result   Esc: close";
    let hw = game.sunset_text_width(hint);
    out.push(Shape::Text(hint.to_owned(), [(w - hw) / 2.0, 3.0], 0xA0A0A0));
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
