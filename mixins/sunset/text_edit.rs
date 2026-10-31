// A text setting's draft while the GUI's editor is open (Rust's pull request #4): one line, edited at its caret,
// and the setting only changes when Apply is clicked.

pub(super) const MAX_BYTES: usize = 4096;

// One line, of at most MAX_BYTES bytes.
pub(super) fn check(text: &str) -> Result<(), &'static str> {
    if text.len() > MAX_BYTES {
        Err("4096 bytes at most")
    } else if text.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')) {
        Err("one line, no control characters")
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
    // A byte offset on a character boundary.
    caret: usize,
    selected: bool,
    pub(super) error: Option<&'static str>,
}

impl Draft {
    pub(super) fn new(text: &str) -> Result<Self, &'static str> {
        check(text)?;
        Ok(Self { text: text.to_owned(), caret: text.len(), selected: false, error: None })
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
    fn previous(&self) -> usize {
        self.text[..self.caret].char_indices().next_back().map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.caret + self.text[self.caret..].chars().next().map_or(0, char::len_utf8)
    }
    // Typed text at the caret (over the whole text when it is selected); refused whole if it would break the rules.
    pub(super) fn insert(&mut self, text: &str) {
        let kept = if self.selected { 0 } else { self.text.len() };
        if let Err(e) = check(text).and_then(|()| if kept + text.len() > MAX_BYTES { Err("4096 bytes at most") } else { Ok(()) }) {
            self.error = Some(e);
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
        let selected = std::mem::take(&mut self.selected);
        match key {
            SelectAll => self.selected = true,
            Left => self.caret = if selected { 0 } else { self.previous() },
            Right => self.caret = if selected { self.text.len() } else { self.next() },
            Home => self.caret = 0,
            End => self.caret = self.text.len(),
            Backspace | Delete if selected => {
                self.text.clear();
                self.caret = 0;
            }
            Backspace => {
                let at = self.previous();
                self.text.replace_range(at..self.caret, "");
                self.caret = at;
            }
            Delete => {
                let end = self.next();
                self.text.replace_range(self.caret..end, "");
            }
        }
        self.error = None;
    }
}
