// Debug Report's chat log: every line that comes into the chat (the server's, players' and the client's own), as
// plain text without its colour codes, with the time it came. Kept even when the Chat option hides it, and past the
// chat's own 100.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::SystemTime;

const KEEP: usize = 500;

#[derive(Clone)]
pub struct Line {
    pub at: SystemTime,
    pub source: crate::chat::ChatSource,
    pub text: String,
}

static LOG: Mutex<VecDeque<Line>> = Mutex::new(VecDeque::new());

pub fn added(content: &crate::text::Text, source: crate::chat::ChatSource) {
    let text: String = content.strip_legacy().plain().chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() == KEEP {
        log.pop_front();
    }
    log.push_back(Line { at: SystemTime::now(), source, text });
}

// The lines kept, oldest first.
pub fn lines() -> Vec<Line> {
    LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}
