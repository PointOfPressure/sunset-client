// Streamer Mode's names, read wherever text is drawn or measured: each name (lower case) and what shows in its place.
// A name is matched as a whole word of name characters (letters, digits and _, all a Minecraft name can hold), any
// case, across the styled pieces a text is made of; the replacement takes the style of the name's first character.
use crate::text::{Span, Style, Text};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

static ON: AtomicBool = AtomicBool::new(false);
static NAMES: RwLock<Option<Arc<HashMap<String, String>>>> = RwLock::new(None);
const LONGEST: usize = 16;

thread_local! {
    // A replaced text being drawn or measured: it is not looked at again.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

// The names to hide (None: Streamer Mode off).
pub fn set(names: Option<HashMap<String, String>>) {
    ON.store(names.as_ref().is_some_and(|n| !n.is_empty()), Ordering::Release);
    *NAMES.write().unwrap_or_else(|e| e.into_inner()) = names.map(Arc::new);
}

fn names() -> Option<Arc<HashMap<String, String>>> {
    if !ON.load(Ordering::Acquire) || INSIDE.with(Cell::get) {
        return None;
    }
    NAMES.read().unwrap_or_else(|e| e.into_inner()).clone()
}

// Draws or measures a replaced text without looking at it again.
pub fn guarded<R>(f: impl FnOnce() -> R) -> R {
    INSIDE.with(|inside| inside.set(true));
    let out = f();
    INSIDE.with(|inside| inside.set(false));
    out
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// The replacement for the word chars[from..to], if it is a name.
fn lookup<'a>(names: &'a HashMap<String, String>, chars: &[char], from: usize, to: usize) -> Option<&'a str> {
    if to - from > LONGEST {
        return None;
    }
    let mut buf = [0u8; LONGEST];
    for (i, c) in chars[from..to].iter().enumerate() {
        buf[i] = c.to_ascii_lowercase() as u8;
    }
    names.get(std::str::from_utf8(&buf[..to - from]).ok()?).map(String::as_str)
}

// Each name in `chars`: where it starts and ends, and its replacement. A legacy section-sign code (the sign and the
// character after it) is formatting, not part of a word.
fn find<'a>(names: &'a HashMap<String, String>, chars: &[char]) -> Vec<(usize, usize, &'a str)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{a7}' {
            i += 2;
            continue;
        }
        if !name_char(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && name_char(chars[i]) {
            i += 1;
        }
        if let Some(to) = lookup(names, chars, start, i) {
            found.push((start, i, to));
        }
    }
    found
}

// Plain text with the names replaced, or None when it holds none.
pub fn protect_str(s: &str) -> Option<String> {
    let names = names()?;
    if !s.chars().any(name_char) {
        return None;
    }
    let chars: Vec<char> = s.chars().collect();
    let found = find(&names, &chars);
    if found.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    for (from, to, with) in found {
        out.extend(&chars[at..from]);
        out.push_str(with);
        at = to;
    }
    out.extend(&chars[at..]);
    Some(out)
}

// A styled text with the names replaced (its § codes resolved first), or None when it holds none.
pub fn protect(text: &Text) -> Option<Text> {
    let names = names()?;
    let text = text.resolved();
    let styled: Vec<(char, &Style)> = text.chars().collect();
    let chars: Vec<char> = styled.iter().map(|&(c, _)| c).collect();
    let found = find(&names, &chars);
    if found.is_empty() {
        return None;
    }
    let mut out = Text::default();
    let mut push = |s: &str, style: &Style| match out.spans.last_mut() {
        Some(last) if last.style == *style => last.text.push_str(s),
        _ => out.spans.push(Span { text: s.to_owned(), style: style.clone() }),
    };
    let mut at = 0;
    let mut one = [0u8; 4];
    for (from, to, with) in found {
        for &(c, style) in &styled[at..from] {
            push(c.encode_utf8(&mut one), style);
        }
        push(with, styled[from].1);
        at = to;
    }
    for &(c, style) in &styled[at..] {
        push(c.encode_utf8(&mut one), style);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn names_are_whole_words_in_any_case() {
        let names = map(&[("vstxx", "You"), ("notch", "Player 1")]);
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        let f = |s: &str| find(&names, &chars(s)).into_iter().map(|(a, b, w)| (a, b, w.to_owned())).collect::<Vec<_>>();
        assert_eq!(f("<VSTXX> hi Notch!"), vec![(1, 6, "You".into()), (11, 16, "Player 1".into())]);
        assert!(f("vstxx2 notched xvstxx").is_empty(), "a longer word is another name");
        assert_eq!(f("[VIP] vstxx's"), vec![(6, 11, "You".into())]);
        assert!(f("aaaaaaaaaaaaaaaaaaaaaaa").is_empty());
        assert_eq!(f("\u{a7}cvstxx\u{a7}r"), vec![(2, 7, "You".into())]);
    }

    #[test]
    fn styled_names_keep_the_first_characters_style_and_split_spans_are_caught() {
        set(Some(map(&[("vstxx", "You")])));
        let red = Style::color(0xFF0000);
        let mut t = Text::default();
        t.spans.push(Span { text: "<vs".into(), style: Style::default() });
        t.spans.push(Span { text: "txx".into(), style: red.clone() });
        t.spans.push(Span { text: "> hi".into(), style: Style::default() });
        let out = protect(&t).expect("a name split over two spans");
        assert_eq!(out.plain(), "<You> hi");
        assert_eq!(protect_str("\u{a7}avstxx joined").as_deref(), Some("\u{a7}aYou joined"), "a colour code is not part of the name");
        assert_eq!(protect_str("VSTXX joined").as_deref(), Some("You joined"));
        assert_eq!(protect_str("nobody here"), None);
        assert_eq!(guarded(|| protect_str("vstxx")), None, "a replaced text is not looked at again");
        set(None);
        assert_eq!(protect_str("vstxx"), None);
    }
}
