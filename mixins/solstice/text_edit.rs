// SPDX-License-Identifier: MIT
// Original implementation assisted by OpenAI Codex (GPT-6.1 Sol).
// Host-free single-line draft. Only the caller's explicit Apply commits it.

pub(super) const MAX_BYTES: usize = 4096;

pub(super) fn validate(text: &str) -> Result<(), &'static str> {
    if text.len() > MAX_BYTES {
        Err("Maximum 4096 UTF-8 bytes")
    } else if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        Err("Single line only; control characters are not allowed")
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum EditKey {
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    SelectAll,
}

#[derive(Clone, Debug)]
pub(super) struct Draft {
    text: String,
    // Always UTF-8 scalar boundaries. This is not grapheme-cluster editing.
    caret: usize,
    selected: bool,
    pub(super) error: Option<&'static str>,
}

impl Draft {
    pub(super) fn new(text: &str) -> Result<Self, &'static str> {
        validate(text)?;
        Ok(Self {
            text: text.to_owned(),
            caret: text.len(),
            selected: false,
            error: None,
        })
    }
    pub(super) fn text(&self) -> &str {
        &self.text
    }
    pub(super) fn caret(&self) -> usize {
        self.caret
    }
    pub(super) fn selected(&self) -> bool {
        self.selected && !self.text.is_empty()
    }
    pub(super) fn value(&self) -> Result<&str, &'static str> {
        validate(&self.text)?;
        Ok(&self.text)
    }
    fn previous(&self) -> usize {
        self.text[..self.caret]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.caret
            + self.text[self.caret..]
                .chars()
                .next()
                .map_or(0, char::len_utf8)
    }
    pub(super) fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let remaining = if self.selected { 0 } else { self.text.len() };
        let result = validate(text).and_then(|()| {
            if remaining + text.len() > MAX_BYTES {
                Err("Maximum 4096 UTF-8 bytes")
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.error = Some(error);
            return;
        }
        if self.selected {
            self.text.clear();
            self.caret = 0;
        }
        self.text.insert_str(self.caret, text);
        self.caret += text.len();
        self.selected = false;
        self.error = None;
    }
    pub(super) fn key(&mut self, key: EditKey) {
        use EditKey::*;
        match key {
            SelectAll => {
                self.selected = true;
            }
            Left => {
                self.caret = if self.selected { 0 } else { self.previous() };
                self.selected = false;
            }
            Right => {
                self.caret = if self.selected {
                    self.text.len()
                } else {
                    self.next()
                };
                self.selected = false;
            }
            Home => {
                self.caret = 0;
                self.selected = false;
            }
            End => {
                self.caret = self.text.len();
                self.selected = false;
            }
            Backspace | Delete if self.selected => {
                self.text.clear();
                self.caret = 0;
                self.selected = false;
            }
            Backspace => {
                let previous = self.previous();
                self.text.replace_range(previous..self.caret, "");
                self.caret = previous;
            }
            Delete => {
                self.text.replace_range(self.caret..self.next(), "");
            }
        }
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditKey::*;

    #[test]
    fn draft_and_cancel_do_not_change_the_original() {
        let original = "  hello, world | /msg friend Hi!  ".to_owned();
        let mut draft = Draft::new(&original).unwrap();
        draft.key(SelectAll);
        draft.insert("replacement");
        drop(draft); // Cancel: no reference to or mutation of the setting.
        assert_eq!(original, "  hello, world | /msg friend Hi!  ");
        assert_eq!(
            Draft::new(&original).unwrap().value(),
            Ok(original.as_str())
        );
    }

    #[test]
    fn edits_both_sides_of_caret_and_selection() {
        let mut draft = Draft::new("abc").unwrap();
        draft.key(Home);
        draft.key(Right);
        draft.key(Delete);
        draft.insert("!");
        assert_eq!(draft.value(), Ok("a!c"));
        draft.key(End);
        draft.key(Backspace);
        draft.key(Left);
        draft.key(Backspace);
        assert_eq!(draft.value(), Ok("!"));
        draft.key(SelectAll);
        draft.insert(" a,b|c ");
        assert_eq!(draft.value(), Ok(" a,b|c "));
        draft.key(SelectAll);
        draft.key(Delete);
        assert_eq!(draft.value(), Ok(""));
        draft.key(Backspace);
        draft.key(Delete);
        assert_eq!(draft.caret(), 0);
    }

    #[test]
    fn unicode_scalar_boundaries_survive_navigation_and_deletion() {
        let mut draft = Draft::new("aé🦀ب").unwrap();
        draft.key(Left);
        draft.key(Backspace);
        assert_eq!(draft.value(), Ok("aéب"));
        draft.key(Home);
        draft.key(Right);
        draft.key(Delete);
        assert_eq!(draft.value(), Ok("aب"));
        draft.insert("e\u{301}");
        assert_eq!(draft.value(), Ok("ae\u{301}ب"));
        // Explicitly scalar-based: delete the combining mark separately.
        draft.key(Backspace);
        assert_eq!(draft.value(), Ok("aeب"));
        for _ in 0..20 {
            draft.key(Right);
        }
        assert_eq!(draft.caret(), draft.text().len());
    }

    #[test]
    fn rejects_invalid_or_oversized_input_atomically() {
        for bad in [
            "\0", "\n", "\r", "\t", "\u{7f}", "\u{85}", "\u{2028}", "\u{2029}",
        ] {
            assert!(Draft::new(bad).is_err());
            let mut draft = Draft::new("valid").unwrap();
            draft.key(SelectAll);
            draft.insert(bad);
            assert!(draft.error.is_some());
            assert_eq!(draft.value(), Ok("valid"));
            assert!(draft.selected());
        }
        let full = "é".repeat(MAX_BYTES / 2);
        let mut draft = Draft::new(&full).unwrap();
        draft.insert("x");
        assert_eq!(draft.value(), Ok(full.as_str()));
        assert!(draft.error.is_some());
        draft.key(SelectAll);
        draft.insert("|empty is valid|");
        assert_eq!(draft.value(), Ok("|empty is valid|"));
        assert!(draft.error.is_none());
        assert!(Draft::new(&"x".repeat(MAX_BYTES + 1)).is_err());
    }
}
