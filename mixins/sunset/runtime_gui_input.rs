// Runtime part (see runtime.rs): the click GUI's keys, clicks, wheel and the HUD layout editor.

use super::{Game, Screen};
use super::sunset_text_edit::{Draft, EditKey};
use super::sunset::*;

pub(super) fn gui_open() -> bool {
    with(|s| s.gui.open)
}

// A key while the GUI is open: a bind being chosen, or Esc and the menu key
// closing it. Every key stops here.
pub(super) fn gui_key(game: &mut Game<'_>, name: &str) {
    if with(|s| s.gui.profile_menu.is_some()) {
        let choice = with(|s| {
            if name == "key.keyboard.escape" {
                // Out of a preview first, then the menu closes.
                if !s.gui.profile_menu.as_mut().is_some_and(super::sunset_profile_menu::Menu::back) {
                    s.gui.profile_menu = None;
                }
                return None;
            }
            let (w, h) = (s.gui.view_w, s.gui.view_h);
            s.gui.profile_menu.as_mut().and_then(|m| m.key(name, w, h))
        });
        if let Some(choice) = choice {
            profile_menu_choice(game, choice);
        }
        return;
    }
    // The editor takes every key: from the window's event already (text_event), or else here (a key without text of
    // its own, as horizon-ctl's are, types its letter).
    if with(|s| s.gui.editor.is_some() || std::mem::take(&mut s.gui.typed)) {
        let control = game.modifiers.control_key();
        with(|s| {
            if std::mem::take(&mut s.gui.typed) {
                return;
            }
            if name == "key.keyboard.escape" {
                s.gui.editor = None;
                return;
            }
            if name.ends_with("enter") {
                apply_editor(s);
                return;
            }
            let Some(Editor { draft: Ok(draft), list, .. }) = s.gui.editor.as_mut() else {
                return;
            };
            if control && name == "key.keyboard.a" {
                draft.key(EditKey::SelectAll);
            } else if let Some(k) = edit_key(name) {
                draft.key(k);
            } else if let Some(c) = key_char(name).filter(|_| !control) {
                draft.insert(&c.to_string());
            }
            if let Some(picker) = list {
                picker.scroll = 0.0;
            }
        });
        return;
    }
    if let Some(i) = with(|s| s.binding) {
        let chosen = match name {
            "key.keyboard.escape" => {
                with(|s| s.binding = None);
                return;
            }
            "key.keyboard.backspace" | "key.keyboard.delete" => None,
            k => Some(k.to_owned()),
        };
        with(|s| {
            set_bind(s, i, chosen);
            s.binding = None;
        });
        save(game);
        return;
    }
    if name == "key.keyboard.escape" && with(|s| s.gui.window.take().is_some()) {
        // Esc closes the settings window before the menu.
        return;
    }
    if name == "key.keyboard.escape" {
        // The first Esc clears a search; the next closes the GUI.
        with(|s| s.gui.select_all = false);
        if with(|s| std::mem::take(&mut s.gui.search)).is_empty() {
            with(|s| s.gui.closing = true);
        }
        return;
    }
    if is_menu_key(game, name) {
        with(|s| s.gui.closing = true);
        return;
    }
    let control = game.modifiers.control_key();
    if name.ends_with("enter") {
        // Enter in the search turns on (or off) the module it found.
        if let Some(i) = with(|s| first_match(s)) {
            toggle(game, i, chat_lines("Toggle messages"));
        }
    } else if control && name == "key.keyboard.a" {
        with(|s| s.gui.select_all = !s.gui.search.is_empty());
    } else if name == "key.keyboard.backspace" || name == "key.keyboard.delete" {
        with(|s| {
            if control || s.gui.select_all {
                s.gui.search.clear();
            } else {
                s.gui.search.pop();
            }
            s.gui.select_all = false;
        });
    } else if let Some(c) = key_char(name)
        && !control
    {
        with(|s| {
            if std::mem::take(&mut s.gui.select_all) {
                s.gui.search.clear();
            }
            if s.gui.search.chars().count() < 32 {
                s.gui.search.push(c);
            }
        });
    }
}

// The module a search points at: the one it scores best (search.rs).
fn first_match(s: &State) -> Option<usize> {
    if s.gui.search.trim().is_empty() {
        return None;
    }
    let listed = s.listed();
    let found = super::sunset_search::results(&s.modules, &s.gui.search);
    super::sunset_search::ranked(&found, (0..s.modules.len()).filter(|&i| listed(&s.modules[i]))).first().copied()
}

// A letter, digit or space typed in the click GUI's search.
pub(super) fn key_char(name: &str) -> Option<char> {
    let k = name.strip_prefix("key.keyboard.")?;
    if k == "space" {
        return Some(' ');
    }
    let mut chars = k.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii_alphanumeric()).then_some(c)
}

// A mouse button pressed while the GUI is open (its release is read from
// the held buttons each frame).
pub(super) fn gui_press(game: &mut Game<'_>, button: winit::event::MouseButton) {
    use winit::event::MouseButton;
    let at = gui_cursor(game);
    if with(|s| s.gui.profile_menu.is_some()) {
        if button == MouseButton::Left {
            let choice = with(|s| {
                let (w, h) = (s.gui.view_w, s.gui.view_h);
                let menu = s.gui.profile_menu.as_mut()?;
                if menu.closes(at, w, h) {
                    s.gui.profile_menu = None;
                    return None;
                }
                menu.click(at, w, h)
            });
            if let Some(choice) = choice {
                profile_menu_choice(game, choice);
            }
        }
        return;
    }
    // The Profiles module's row, either button: the list of saved profiles instead of a toggle or settings.
    if with(|s| {
        s.gui.editor.is_none()
            && s.binding.is_none()
            && hit_at(s, at).is_some_and(|(_, hit)| matches!(hit, Hit::Module(i) if s.modules[i].name == "Profiles"))
    }) {
        let result = super::sunset_profiles::Store::new(game.native_data_dir(ID)).and_then(|store| store.list());
        with(|s| {
            let names = result.as_ref().cloned().unwrap_or_default();
            let mut menu = super::sunset_profile_menu::Menu::new(names, s.selection.selected(), s.gui.view_w, s.gui.view_h);
            menu.error = result.err();
            s.gui.profile_menu = Some(menu);
            s.gui.drag = None;
        });
        return;
    }
    // A bind being chosen takes any button but the left one (which clicks as ever).
    let captured = with(|s| {
        let i = s.binding.filter(|_| button != MouseButton::Left && s.gui.editor.is_none())?;
        set_bind(s, i, Some(horizon_client::keybinds::mouse_name(button)));
        s.binding = None;
        Some(())
    });
    if captured.is_some() {
        save(game);
        return;
    }
    let right = match button {
        MouseButton::Left => false,
        MouseButton::Right => true,
        _ => return,
    };
    let mut toggled = None;
    let mut changed = false;
    with(|s| {
        // The editor is in front of everything: its buttons, nothing else; a picker's rows too.
        if let Some(Editor { module, setting, draft: Ok(draft), list: Some(picker), .. }) = &s.gui.editor {
            use super::sunset_picker as pick;
            let r = pick::rect(s.gui.view_w, s.gui.view_h);
            let area = pick::list_area(r);
            if !right && inside(pick::done_button(r), at) {
                s.gui.editor = None;
            } else if !right && inside(area, at) {
                let (i, j) = (*module, *setting);
                let row = ((at.1 - area[1] + super::sunset_smooth::shown(super::sunset_smooth::PICKER, picker.scroll)) / pick::ROW) as usize;
                if let Value::Text(t) = &s.modules[i].settings[j].value
                    && let Some((name, _)) = pick::rows(picker, t, draft.text()).get(row)
                {
                    let list = pick::clicked(picker, t, name);
                    s.modules[i].settings[j].value = Value::Text(list);
                    changed = true;
                }
            }
            return;
        }
        if s.gui.editor.is_some() {
            let (apply, cancel) = editor_buttons(editor_rect(s.gui.view_w, s.gui.view_h));
            if !right && inside(apply, at) {
                apply_editor(s);
                changed = true;
            } else if !right && inside(cancel, at) {
                s.gui.editor = None;
            }
            return;
        }
        let found = hit_at(s, at);
        // A number being typed: a click in its field leaves it be; anywhere else, it is left as it was.
        if let Some(Editor { module, setting, draft: Ok(draft), inline: true, .. }) = &s.gui.editor {
            let in_field = found.is_some_and(|(r, hit)| {
                hit == Hit::Setting(*module, *setting) && at.0 >= r[0] + r[2] - RIGHT_PAD - field_width(game, draft.text()) && at.1 < number_band(r)
            });
            if in_field {
                return;
            }
            s.gui.editor = None;
        }
        let Some((r, hit)) = found else {
            // Off the menu: a HUD element can be taken and moved.
            if let Some((name, dx, dy)) = super::sunset_hud_bars::grab(game.cursor).filter(|_| !right) {
                s.gui.drag = Some(Drag::Hud(name, dx, dy));
            }
            return;
        };
        // Modules is its switches: either button opens them.
        let right = right || matches!(hit, Hit::Module(i) if s.modules[i].name == super::sunset_extra_modules::NAME);
        match (hit, right) {
            (Hit::WinTitle, false) => {
                if let Some(w) = s.gui.window {
                    s.gui.drag = Some(Drag::Window(at.0 - w.x, at.1 - w.y));
                }
            }
            (Hit::WinClose, false) => s.gui.window = None,
            (Hit::WinBody | Hit::WinTitle | Hit::WinClose, _) => {}
            (Hit::Dash(super::sunset_dashboard::Part::Toggle), false) => toggled = super::sunset_dashboard::chosen(),
            (Hit::Dash(part), right) => super::sunset_dashboard::press(s, part, right),
            (Hit::Header(c), false) => {
                let p = s.gui.panels[c];
                s.gui.drag = Some(Drag::Panel(c, at.0 - p.x, at.1 - p.y));
            }
            (Hit::Header(c), true) => {
                s.gui.panels[c].collapsed = !s.gui.panels[c].collapsed;
                changed = true;
            }
            (Hit::Module(i), false) => toggled = Some(i),
            (Hit::Module(i), true) if super::sunset_dashboard::on() => {
                super::sunset_dashboard::choose(i);
                s.binding = None;
            }
            (Hit::Module(i), true) if s.module("ClickGUI").is_some_and(|m| m.p("Settings") == "Inline") => {
                if let Some(k) = s.gui.expanded.iter().position(|&e| e == i) {
                    s.gui.expanded.remove(k);
                } else {
                    s.gui.expanded.push(i);
                }
            }
            // Its own window, in the middle at first (the same module again closes it).
            (Hit::Module(i), true) => {
                s.gui.window = match s.gui.window {
                    Some(w) if w.module == i => None,
                    _ => Some(Win { module: i, x: (s.gui.view_w - win_w(s)) / 2.0, y: 40.0, scroll: 0.0, about_h: 0.0, content: 0.0, room: 0.0 }),
                };
                s.binding = None;
            }
            (Hit::Bind(i), false) => s.binding = if s.binding == Some(i) { None } else { Some(i) },
            (Hit::Bind(i), true) => {
                s.modules[i].key = None;
                s.binding = None;
                changed = true;
            }
            (Hit::Setting(i, j), right) => {
                let number = match s.modules[i].settings[j].value {
                    Value::Num { v, .. } => Some(v),
                    _ => None,
                };
                // A click on the number: it is typed instead (all of it selected, so typing replaces it).
                if let (Some(v), false) = (number, right)
                    && at.0 >= r[0] + r[2] - RIGHT_PAD - game.native_text_width(&num(v)) - 3.0
                    && at.1 < number_band(r)
                {
                    let mut draft = Draft::new(&num(v));
                    if let Ok(d) = &mut draft {
                        d.key(EditKey::SelectAll);
                    }
                    s.gui.editor = Some(Editor { module: i, setting: j, draft, inline: true, list: None });
                    s.gui.drag = None;
                    s.binding = None;
                } else if number.is_some() && !right {
                    s.gui.drag = Some(Drag::Slider(i, j, r, gui_scale()));
                    changed = slide(s, i, j, r, at.0);
                } else if number.is_some() {
                    // Right click on a slider: back to its default.
                    let fresh = fresh_module(s, i).settings.swap_remove(j);
                    s.modules[i].settings[j] = fresh;
                    changed = true;
                } else if let (Value::Text(t), false) = (&s.modules[i].settings[j].value, right) {
                    // A list of blocks, items or entities opens the picker (the draft is its search), other text the
                    // editor.
                    let m = &s.modules[i];
                    let list = super::sunset_picker::list_kind(m.name, m.settings[j].name).map(|k| {
                        if m.name == "Xray" && m.p("List") == "Simple" {
                            super::sunset_picker::short_list(super::sunset_picker::VALUABLES)
                        } else {
                            super::sunset_picker::open(game, k)
                        }
                    });
                    let draft = if list.is_some() { Draft::new("") } else { Draft::new(t) };
                    s.gui.editor = Some(Editor { module: i, setting: j, draft, inline: false, list });
                    s.gui.drag = None;
                    s.binding = None;
                } else {
                    s.modules[i].settings[j].click(right);
                    changed = true;
                }
            }
        }
    });
    if let Some(i) = toggled {
        toggle(game, i, false);
    } else if changed {
        save(game);
    }
}

// A HUD element dragged by the pointer, taken `dx`, `dy` into it: its position Custom, at where its corner is now.
fn move_hud(s: &mut State, game: &Game<'_>, name: &str, dx: f32, dy: f32) {
    let Some((fx, fy)) = super::sunset_hud_bars::drag_to(name, game.cursor.0 - dx, game.cursor.1 - dy) else {
        return;
    };
    let Some(m) = s.modules.iter_mut().find(|m| m.name == "HUD") else {
        return;
    };
    for st in &mut m.settings {
        match st.name.strip_prefix(name).map(str::trim) {
            Some("position") => {
                if let Value::Pick { i, of } = &mut st.value {
                    *i = of.iter().position(|o| *o == "Custom").unwrap_or(*i);
                }
            }
            Some("x") => {
                if let Value::Num { v, .. } = &mut st.value {
                    *v = fx;
                }
            }
            Some("y") => {
                if let Value::Num { v, .. } = &mut st.value {
                    *v = fy;
                }
            }
            _ => {}
        }
    }
}

// HUD's layout editor with the chat open: Coordinates, Compass and the Armor HUD are framed, and a press on one drags
// it until the button comes up (saved then). The chat closed, a drag still going ends there.
pub(super) fn chat_layout(game: &Game<'_>, out: &mut Vec<Shape>) {
    use super::sunset_hud_bars::ChatDrag;
    let dragging = with(|s| match s.gui.drag {
        Some(Drag::Hud(name, dx, dy)) => Some((name, dx, dy)),
        _ => None,
    });
    if game.screen != Screen::Chat || with(|s| s.gui.open) {
        super::sunset_hud_bars::chat_closed();
        if dragging.is_some() && !with(|s| s.gui.open) {
            with(|s| s.gui.drag = None);
            save(game);
        }
        return;
    }
    match super::sunset_hud_bars::chat_layout(game, dragging.map(|d| d.0), out) {
        Some(ChatDrag::Start(name, dx, dy)) => with(|s| s.gui.drag = Some(Drag::Hud(name, dx, dy))),
        Some(ChatDrag::End) => {
            with(|s| s.gui.drag = None);
            save(game);
        }
        None => {
            if let Some((name, dx, dy)) = dragging {
                with(|s| move_hud(s, game, name, dx, dy));
            }
        }
    }
}

// A drag follows the pointer each frame.
pub(super) fn follow_drag(game: &Game<'_>, w: f32, h: f32) {
    let at = gui_cursor(game);
    with(|s| match s.gui.drag {
        Some(Drag::Panel(c, dx, dy)) => {
            let p = &mut s.gui.panels[c];
            p.x = (at.0 - dx).clamp(0.0, (w - panel_w()).max(0.0));
            p.y = (at.1 - dy).clamp(0.0, (h - head_h()).max(0.0));
        }
        Some(Drag::Slider(i, j, r, scale)) => {
            // The look follows as it is dragged, but for what moves the panels: that waits for the release (save).
            let layout = s.modules[i].name == "ClickGUI" && matches!(s.modules[i].settings[j].name, "Panel width" | "GUI scale" | "Text indent");
            if slide(s, i, j, r, game.cursor.0 / scale) && !layout {
                sync_flags(s);
            }
        }
        Some(Drag::Window(dx, dy)) => {
            if let Some(win) = &mut s.gui.window {
                (win.x, win.y) = (at.0 - dx, at.1 - dy);
            }
        }
        Some(Drag::Hud(name, dx, dy)) => move_hud(s, game, name, dx, dy),
        None => {}
    });
    // The left button came up: the drag ends where the pointer is.
    if !game.physical_buttons.contains(&winit::event::MouseButton::Left)
        && with(|s| s.gui.drag.take().is_some())
    {
        save(game);
    }
}
