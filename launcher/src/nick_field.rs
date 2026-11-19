//! The nickname box of the "Add offline name" dialog. A Minecraft name is at most 16 ASCII letters, digits and
//! underscores, so the box only ever holds those: other characters are refused as they are typed or pasted, and
//! every position is a byte and a character at once. Editing is what a short name needs: the caret, Backspace and
//! Delete, Home and End, select all (Ctrl/Cmd-A) and the clipboard.

use gpui::prelude::FluentBuilder as _;
use gpui::{div, px, rgb, App, ClipboardItem, Div, Hsla, KeyDownEvent, ParentElement as _, SharedString, Styled as _};

pub const MAX: usize = 16;

/// What a key press did to the box.
#[derive(Debug, PartialEq)]
pub enum Key {
    Enter,
    Escape,
    /// Not the box's (Tab, other shortcuts): the dialog may handle it.
    Pass,
    /// Only the caret or the selection changed.
    Moved,
    /// The name changed.
    Edited,
    /// A character a name can't hold (or a full box): nothing changed.
    Refused,
}

pub fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[derive(Default)]
pub struct NickField {
    text: String,
    caret: usize,
    /// The whole name is selected (Ctrl/Cmd-A): typing replaces it, Backspace clears it.
    all: bool,
}

impl NickField {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Puts `s` at the caret (over the selection), keeping only what a name can hold, up to [`MAX`].
    /// Returns whether anything was refused.
    pub fn insert(&mut self, s: &str) -> bool {
        let room = if self.all { MAX } else { MAX - self.text.len() };
        let keep: String = s.chars().filter(|&c| allowed(c)).take(room).collect();
        let refused = keep.chars().count() < s.chars().count();
        // A refused key leaves a selected name as it was.
        if self.all && !keep.is_empty() {
            self.clear();
        }
        self.text.insert_str(self.caret, &keep);
        self.caret += keep.len();
        refused
    }

    fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.all = false;
    }

    /// Applies one key: `key` is gpui's key name, `typed` the character it makes, `command` Ctrl (Cmd on macOS).
    /// Clipboard keys are [`handle`]'s.
    pub fn key(&mut self, key: &str, typed: Option<&str>, command: bool) -> Key {
        match (key, command) {
            ("enter", _) => Key::Enter,
            ("escape", _) => Key::Escape,
            ("a", true) => {
                self.all = !self.text.is_empty();
                self.caret = self.text.len();
                Key::Moved
            }
            (_, true) if !matches!(key, "backspace" | "delete" | "left" | "right") => Key::Pass,
            ("backspace" | "delete", _) if self.all => {
                self.clear();
                Key::Edited
            }
            ("backspace", _) if self.caret > 0 => {
                // Ctrl-Backspace takes everything before the caret: a name is one word.
                let from = if command { 0 } else { self.caret - 1 };
                self.text.replace_range(from..self.caret, "");
                self.caret = from;
                Key::Edited
            }
            ("delete", _) if self.caret < self.text.len() => {
                let to = if command { self.text.len() } else { self.caret + 1 };
                self.text.replace_range(self.caret..to, "");
                Key::Edited
            }
            ("backspace" | "delete", _) => Key::Moved,
            ("left" | "home" | "right" | "end", _) => {
                let start = key == "home" || (key == "left" && (command || self.all));
                let end = key == "end" || (key == "right" && (command || self.all));
                self.caret = if start {
                    0
                } else if end {
                    self.text.len()
                } else if key == "left" {
                    self.caret.saturating_sub(1)
                } else {
                    (self.caret + 1).min(self.text.len())
                };
                self.all = false;
                Key::Moved
            }
            ("space", _) => Key::Refused,
            _ => match typed.filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) {
                Some(t) => {
                    let before = self.text.len();
                    let refused = self.insert(t);
                    if refused || self.text.len() == before { Key::Refused } else { Key::Edited }
                }
                None => Key::Pass,
            },
        }
    }
}

/// One key down in the box, clipboard included (Ctrl/Cmd-C, -X, -V). Never returns Pass for those.
pub fn handle(f: &mut NickField, ev: &KeyDownEvent, cx: &mut App) -> Key {
    let k = &ev.keystroke;
    let command = k.modifiers.control || k.modifiers.platform;
    match (k.key.as_str(), command) {
        ("c" | "x", true) => {
            let whole = f.all || k.key == "x";
            if whole && !f.text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(f.text.clone()));
            }
            if k.key == "x" && !f.text.is_empty() {
                f.clear();
                return Key::Edited;
            }
            Key::Moved
        }
        ("v", true) => {
            let clip = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default();
            let line = clip.trim().lines().next().unwrap_or_default().trim().to_string();
            if f.insert(&line) { Key::Refused } else { Key::Edited }
        }
        _ => f.key(&k.key, k.key_char.as_deref(), command),
    }
}

/// What the box draws: the placeholder while empty and unfocused, else the name with the caret (the whole name
/// tinted when selected). Colours are `0xRRGGBB`.
pub fn content(f: &NickField, focused: bool, placeholder: SharedString, ink: u32, muted: u32, accent: u32) -> Div {
    if f.text.is_empty() && !focused {
        return div().text_color(rgb(muted)).child(placeholder);
    }
    let piece = |s: &str| div().flex_none().whitespace_nowrap().child(s.to_string());
    let caret = div().flex_none().w(px(1.)).h(px(15.)).bg(rgb(ink));
    let (before, after) = f.text.split_at(f.caret);
    div()
        .flex()
        .items_center()
        .text_color(rgb(ink))
        .when(f.all, |d| d.child(piece(&f.text).rounded(px(2.)).bg(Hsla::from(rgb(accent)).opacity(0.3))))
        .when(!f.all, |d| d.child(piece(before)))
        .when(focused, |d| d.child(caret))
        .when(!f.all, |d| d.child(piece(after)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(f: &mut NickField, s: &str) -> Vec<Key> {
        s.chars().map(|c| f.key(&c.to_ascii_lowercase().to_string(), Some(&c.to_string()), false)).collect()
    }

    #[test]
    fn keeps_only_what_a_name_can_hold() {
        let mut f = NickField::default();
        let keys = typed(&mut f, "Ab_9-é");
        assert_eq!(f.text(), "Ab_9");
        assert_eq!(keys[4..], [Key::Refused, Key::Refused]);
        assert_eq!(f.key("space", Some(" "), false), Key::Refused);
        typed(&mut f, "0123456789abcdef");
        assert_eq!(f.text().len(), MAX);
        assert!(f.insert("Steve Two!"));
    }

    #[test]
    fn edits_at_the_caret() {
        let mut f = NickField::default();
        typed(&mut f, "Stve");
        f.key("left", None, false);
        f.key("left", None, false);
        typed(&mut f, "e");
        assert_eq!(f.text(), "Steve");
        f.key("home", None, false);
        assert_eq!(f.key("backspace", None, false), Key::Moved);
        assert_eq!(f.key("delete", None, false), Key::Edited);
        assert_eq!(f.text(), "teve");
        f.key("end", None, false);
        f.key("backspace", None, true);
        assert_eq!(f.text(), "");
    }

    #[test]
    fn select_all_then_type_replaces() {
        let mut f = NickField::default();
        typed(&mut f, "Old_Name");
        assert_eq!(f.key("a", Some("a"), true), Key::Moved);
        typed(&mut f, "New");
        assert_eq!(f.text(), "New");
        f.key("a", Some("a"), true);
        assert_eq!(f.key("-", Some("-"), false), Key::Refused);
        assert_eq!(f.text(), "New");
        assert_eq!(f.key("backspace", None, false), Key::Edited);
        assert_eq!(f.text(), "");
        assert_eq!(f.key("t", Some("t"), true), Key::Pass);
    }
}
