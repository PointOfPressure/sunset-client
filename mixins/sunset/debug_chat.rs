// Debug Report's chat log: every line that comes into the chat (the server's, players' and the client's own), as
// plain text without its colour codes, with the time it came. Kept even when the Chat option hides it, and past the
// chat's own 100.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

const KEEP: usize = 500;

#[derive(Clone)]
pub struct Line {
    pub at: SystemTime,
    pub source: crate::chat::ChatSource,
    pub text: String,
}

static LOG: Mutex<VecDeque<Line>> = Mutex::new(VecDeque::new());
// Lines ever added: the plugin API's chat event reads the ones after the count it saw last.
static ADDED: AtomicU64 = AtomicU64::new(0);

pub fn added(content: &crate::text::Text, source: crate::chat::ChatSource) {
    let text: String = content.strip_legacy().plain().chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() == KEEP {
        log.pop_front();
    }
    log.push_back(Line { at: SystemTime::now(), source, text });
    ADDED.fetch_add(1, Ordering::Relaxed);
}

// The count of lines added so far, and the lines kept that came after `seen` of them (none the first time: u64::MAX).
pub fn since(seen: u64) -> (u64, Vec<Line>) {
    let log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let total = ADDED.load(Ordering::Relaxed);
    let new = usize::try_from(total.saturating_sub(seen)).unwrap_or(usize::MAX).min(log.len());
    (total, log.iter().skip(log.len() - new).cloned().collect())
}

// The lines kept, oldest first.
pub fn lines() -> Vec<Line> {
    LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}
