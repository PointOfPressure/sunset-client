// Notifications: Sunset's messages as small toasts in a corner of the screen instead of lines in the chat, so nothing
// of Sunset's is in a screenshot of the chat. Newest nearest the corner, at most four shown, each fades after its time
// (longer for warnings and errors); a click on one in the open menu closes it. Settings are the Notifications module's
// (Client). Drawn last in the frame (gui_top), straight into the HUD batch: a frame with toasts showing allocates nothing,
// as their lines are wrapped once, when first drawn or when the window's width changes.

use super::Game;
use super::sunset::{Category, Module};
use horizon_client::hud::HudBatch;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) const NAME: &str = "Notifications";
const POSITIONS: &[&str] = &["Top right", "Top left", "Bottom right", "Bottom left"];
const SHOWN: usize = 4;
// Kept waiting beyond those shown (a burst of messages): the oldest go first.
const QUEUED: usize = 16;
const LINES: usize = 3;
const PAD: f32 = 5.0;
const LINE_H: f32 = 10.0;
const MARGIN: f32 = 4.0;
const GAP: f32 = 3.0;
const ENTER: Duration = Duration::from_millis(150);
const FADE: Duration = Duration::from_millis(300);

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Client, "Sunset's messages as toasts in a corner of the screen. Off: no messages.")
            .on()
            .pick("Position", POSITIONS)
            .num("Duration", 2.5, 1.0, 10.0, 0.5)
            .bool("Module toggles", true)
            .bool("Module messages", true),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Info,
    Success,
    Warning,
    Error,
}

impl Kind {
    fn accent(self) -> u32 {
        match self {
            Kind::Info => super::sunset::accent() & 0x00FF_FFFF,
            Kind::Success => 0x5CC97B,
            Kind::Warning => 0xE8B04A,
            Kind::Error => 0xE0564F,
        }
    }
    // Warnings and errors stay twice as long.
    fn weight(self) -> f32 {
        if matches!(self, Kind::Warning | Kind::Error) { 2.0 } else { 1.0 }
    }
}

struct Toast {
    kind: Kind,
    text: String,
    // Toasts with a key replace the one showing with it (a module turned on then off shows its last state once).
    key: Option<&'static str>,
    born: Instant,
    // Set when it starts to fade: its time ran out, it was clicked, or newer ones pushed it out.
    closing: Option<Instant>,
    // Its text wrapped for the width it was laid out for, and its box's size then.
    lines: Vec<String>,
    laid_for: f32,
    size: [f32; 2],
    // Where it was last drawn, for a click.
    rect: [f32; 4],
    // A notice that stays three times as long.
    long: bool,
}

static TOASTS: Mutex<VecDeque<Toast>> = Mutex::new(VecDeque::new());

fn toasts() -> std::sync::MutexGuard<'static, VecDeque<Toast>> {
    TOASTS.lock().unwrap_or_else(|e| e.into_inner())
}

// Whether messages show at all, and module toggles.
pub(super) fn enabled() -> bool {
    super::sunset::with(|s| s.module(NAME).is_none_or(|m| m.on))
}

pub(super) fn toggles_shown() -> bool {
    super::sunset::with(|s| s.module(NAME).is_none_or(|m| m.on && m.b("Module toggles")))
}

pub(super) fn module_messages() -> bool {
    super::sunset::with(|s| s.module(NAME).is_none_or(|m| m.on && m.b("Module messages")))
}

// A plain message's kind from its words: a failure is an error, the rest information.
pub(super) fn kind_of(msg: &str) -> Kind {
    let lower = msg.to_ascii_lowercase();
    if lower.starts_with("couldn't") || lower.contains(" failed") || lower.starts_with("error") { Kind::Error } else { Kind::Info }
}

pub(super) fn push(kind: Kind, text: &str) {
    push_keyed(kind, text, None);
}

pub(super) fn push_keyed(kind: Kind, text: &str, key: Option<&'static str>) {
    let text = text.trim();
    if text.is_empty() || !enabled() {
        return;
    }
    logged(text);
    add(&mut toasts(), Instant::now(), kind, text, key);
}

// Each toast shown is a line in the game's log (as Sunset's chat lines were), for bug reports and the test rig.
fn logged(text: &str) {
    horizon_client::logging::write_only(log::Level::Info, &format!("[Sunset toast] {text}"));
}

fn add(q: &mut VecDeque<Toast>, now: Instant, kind: Kind, text: &str, key: Option<&'static str>) {
    // The same message again while it shows, or the same key: that one starts over, with the new text, nearest the corner.
    if let Some(i) = q.iter().position(|t| t.closing.is_none() && (t.text == text || key.is_some() && t.key == key)) {
        let mut t = q.remove(i).expect("index from position");
        if t.text != text {
            t.text = text.to_owned();
            t.laid_for = -1.0;
        }
        t.kind = kind;
        t.born = now;
        q.push_back(t);
    } else {
        q.push_back(Toast {
            kind,
            text: text.to_owned(),
            key,
            born: now,
            closing: None,
            lines: Vec::new(),
            laid_for: -1.0,
            size: [0.0; 2],
            rect: [0.0; 4],
            long: false,
        });
    }
    while q.len() > QUEUED {
        q.pop_front();
    }
}

// A one-off notice (Sunset Presence's first launch): information that stays three times as long.
pub(super) fn push_notice(text: &str) {
    if !enabled() {
        return;
    }
    logged(text);
    let mut q = toasts();
    add(&mut q, Instant::now(), Kind::Info, text, None);
    if let Some(t) = q.back_mut() {
        t.long = true;
    }
}

// Every toast gone: Panic, or Notifications turned off.
pub(super) fn clear() {
    toasts().clear();
}

// A click at a point (the HUD's units): the toast there starts to fade. True if one was hit.
pub(super) fn click(at: (f32, f32)) -> bool {
    let now = Instant::now();
    let mut q = toasts();
    for t in q.iter_mut().rev() {
        let [x, y, w, h] = t.rect;
        if w > 0.0 && t.closing.is_none() && at.0 >= x && at.0 < x + w && at.1 >= y && at.1 < y + h {
            t.closing = Some(now);
            return true;
        }
    }
    false
}

// The words of a text in lines no wider than max, at most LINES of them (the last cut with "..." if it runs over).
fn wrap(text: &str, max: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let joined = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if width(&joined) <= max || line.is_empty() && width(word) <= max {
            line = joined;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // A word wider than a line on its own is cut where it fills one.
        let mut rest = word;
        while width(rest) > max {
            let cut = rest.char_indices().map(|(i, _)| i).skip(1).take_while(|&i| width(&rest[..i]) <= max).last().unwrap_or_else(|| rest.chars().next().map_or(rest.len(), char::len_utf8));
            lines.push(rest[..cut].to_owned());
            rest = &rest[cut..];
        }
        line = rest.to_owned();
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > LINES {
        lines.truncate(LINES);
        let last = &mut lines[LINES - 1];
        while !last.is_empty() && width(&format!("{last}...")) > max {
            last.pop();
        }
        last.push_str("...");
    }
    lines
}

// How far into its fade-out a toast is (0 none, 1 gone), and its slide in (0 at the edge, 1 in place).
fn motion(t: &Toast, now: Instant, life: Duration) -> (f32, f32) {
    let age = now.saturating_duration_since(t.born);
    let enter = (age.as_secs_f32() / ENTER.as_secs_f32()).min(1.0);
    let ease = 1.0 - (1.0 - enter) * (1.0 - enter);
    let fading_from = t.closing.unwrap_or(t.born + life.mul_f32(t.kind.weight() * if t.long { 3.0 } else { 1.0 }));
    let out = (now.saturating_duration_since(fading_from).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
    (out, ease)
}

pub(super) fn draw(game: &Game<'_>, batch: &mut HudBatch) {
    if game.ui.hide_gui {
        return;
    }
    let mut q = toasts();
    if q.is_empty() {
        return;
    }
    let Some((on, corner, life)) = super::sunset::with(|s| {
        s.module(NAME).map(|m| (m.on, POSITIONS.iter().position(|p| *p == m.p("Position")).unwrap_or(0), m.n("Duration")))
    }) else {
        return;
    };
    if !on {
        q.clear();
        return;
    }
    let now = Instant::now();
    let life = Duration::from_secs_f32(life.clamp(0.5, 30.0) as f32);
    q.retain(|t| motion(t, now, life).0 < 1.0);
    // Beyond those shown, the oldest fade out to make room.
    let excess = q.iter().filter(|t| t.closing.is_none()).count().saturating_sub(SHOWN);
    for t in q.iter_mut().filter(|t| t.closing.is_none()).take(excess) {
        t.closing = Some(now);
    }
    let (w, h) = (batch.width, batch.height);
    let max = (w * 0.4).clamp(80.0, 240.0);
    let (right, bottom) = (corner % 2 == 0, corner >= 2);
    let px = 1.0 / batch.scale;
    let mut y = if bottom { h - MARGIN } else { MARGIN };
    for t in q.iter_mut().rev() {
        if t.laid_for != max {
            t.lines = wrap(&t.text, max - 2.0 * PAD - 3.0, &|s| game.native_text_width(s));
            let text_w = t.lines.iter().map(|l| game.native_text_width(l)).fold(0.0, f32::max);
            t.size = [text_w + 2.0 * PAD + 3.0, t.lines.len() as f32 * LINE_H + 2.0 * PAD - 2.0];
            t.laid_for = max;
        }
        let (out, ease) = motion(t, now, life);
        let [tw, th] = t.size;
        let slide = (1.0 - ease) * (tw + MARGIN);
        let x = if right { w - MARGIN - tw + slide } else { MARGIN - slide };
        let top = if bottom { y - th } else { y };
        let start = batch.vertices.len();
        super::sunset::round_fill(game, batch, [x, top, tw, th], 3.0, [true; 4], 0xE814_1418, px);
        super::sunset::round_fill(game, batch, [x, top, 2.0, th], 1.0, [true, false, false, true], 0xFF00_0000 | t.kind.accent(), px);
        for (i, line) in t.lines.iter().enumerate() {
            game.native_text(batch, line, [x + PAD + 2.0, top + PAD + i as f32 * LINE_H], 0xF0F0F0, false);
        }
        let alpha = (1.0 - out) * ease.max(0.25);
        if alpha < 1.0 {
            for v in &mut batch.vertices[start..] {
                v.color = (v.color & 0x00FF_FFFF) | ((((v.color >> 24) as f32) * alpha) as u32) << 24;
            }
        }
        t.rect = [x, top, tw, th];
        // A fading toast gives up its place as it goes, so those after it close the gap.
        let step = (th + GAP) * (1.0 - out);
        y = if bottom { y - step } else { y + step };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &str) -> f32 {
        s.chars().count() as f32 * 6.0
    }

    #[test]
    fn wraps_words_and_cuts_long_ones() {
        assert_eq!(wrap("Kill Aura on", 200.0, &width), vec!["Kill Aura on"]);
        assert_eq!(wrap("aaaa bbbb cccc", 60.0, &width), vec!["aaaa bbbb", "cccc"]);
        // A 20-letter word at 10 letters a line.
        assert_eq!(wrap("abcdefghijklmnopqrst", 60.0, &width), vec!["abcdefghij", "klmnopqrst"]);
        assert!(wrap("", 60.0, &width).is_empty());
    }

    #[test]
    fn long_text_stops_at_three_lines() {
        let lines = wrap(&"word ".repeat(40), 60.0, &width);
        assert_eq!(lines.len(), LINES);
        assert!(lines[2].ends_with("..."));
        assert!(lines.iter().all(|l| width(l) <= 60.0));
    }

    #[test]
    fn same_key_or_text_replaces_and_queue_is_bounded() {
        let mut q = VecDeque::new();
        let now = Instant::now();
        add(&mut q, now, Kind::Success, "Kill Aura on", Some("Kill Aura"));
        add(&mut q, now, Kind::Info, "Speed on", Some("Speed"));
        add(&mut q, now, Kind::Info, "Kill Aura off", Some("Kill Aura"));
        assert_eq!(q.len(), 2);
        assert_eq!(q.back().unwrap().text, "Kill Aura off");
        assert_eq!(q.back().unwrap().kind, Kind::Info);
        add(&mut q, now, Kind::Info, "Speed on", None);
        assert_eq!(q.len(), 2);
        assert_eq!(q.back().unwrap().text, "Speed on");
        for i in 0..40 {
            add(&mut q, now, Kind::Info, &format!("message {i}"), None);
        }
        assert_eq!(q.len(), QUEUED);
        assert_eq!(q.back().unwrap().text, "message 39");
    }

    #[test]
    fn fades_after_its_time_and_errors_last_longer() {
        let born = Instant::now();
        let mut q = VecDeque::new();
        add(&mut q, born, Kind::Info, "a", None);
        add(&mut q, born, Kind::Error, "b", None);
        let life = Duration::from_secs(2);
        let at = born + Duration::from_millis(2_000) + FADE;
        assert_eq!(motion(&q[0], at, life).0, 1.0);
        assert_eq!(motion(&q[1], at, life).0, 0.0);
        assert_eq!(motion(&q[0], born, life), (0.0, 0.0));
        assert_eq!(motion(&q[0], born + ENTER, life).1, 1.0);
    }
}
