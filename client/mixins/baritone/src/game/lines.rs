// Baritone's lines (horizon-baritone's render code makes them), handed over by the game side once a tick (src/baritone.rs) and added to the frame's gizmos by the
// debug_gizmos hook: the depth-tested ones, and the ones drawn over everything (`renderPathIgnoreDepth` and the like).
use horizon_client::gizmos::{Gizmos, Line};
use std::sync::Mutex;

static LINES: Mutex<(Vec<Line>, Vec<Line>)> = Mutex::new((Vec::new(), Vec::new()));

/// The depth-tested lines, and the lines drawn over everything.
pub fn set(depth: Vec<Line>, on_top: Vec<Line>) {
    if let Ok(mut held) = LINES.lock() {
        *held = (depth, on_top);
    }
}

/// Adds the lines held to a frame's gizmos.
pub fn add(gizmos: &mut Gizmos) {
    if let Ok(held) = LINES.lock() {
        gizmos.lines.extend(held.0.iter().cloned());
        gizmos.on_top.extend(held.1.iter().cloned());
    }
}
