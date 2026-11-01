// The menu's lists glide to where the wheel put them: their scroll values stay the targets, and what is drawn follows.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

// The panels come first, one for each category.
pub(super) const WINDOW: usize = 6;
pub(super) const DASHBOARD: usize = 7;
pub(super) const PICKER: usize = 8;
pub(super) const PANE: usize = 9;
pub(super) const SLOTS: usize = 10;

// ClickGUI's Animations: Off, and everything is at once where it is going.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub(super) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub(super) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

// How fast the rest of the way closes, per second: nine tenths of it in about 0.14 s.
const RATE: f32 = 16.0;
// This close, it is there.
const NEAR: f32 = 0.05;

struct Glide {
    shown: [Option<f32>; SLOTS],
    last: Option<Instant>,
}

static GLIDE: Mutex<Glide> = Mutex::new(Glide { shown: [None; SLOTS], last: None });

fn glide<R>(f: impl FnOnce(&mut Glide) -> R) -> R {
    f(&mut GLIDE.lock().unwrap_or_else(|e| e.into_inner()))
}

// One frame's step towards the target: the same glide at any frame rate.
fn step(shown: f32, target: f32, dt: f32) -> f32 {
    let next = shown + (target - shown) * (1.0 - (-dt * RATE).exp());
    if (target - next).abs() < NEAR { target } else { next }
}

// Once a frame, with each list's target (None for one not open: it starts at its target when it opens).
pub(super) fn frame(targets: [Option<f32>; SLOTS]) {
    glide(|g| {
        let now = Instant::now();
        let dt = if enabled() { g.last.map_or(0.0, |t| now.duration_since(t).as_secs_f32()) } else { f32::INFINITY };
        g.last = Some(now);
        for (shown, target) in g.shown.iter_mut().zip(targets) {
            *shown = target.map(|t| shown.map_or(t, |v| step(v, t, dt)));
        }
    });
}

// Where a list is drawn: `target` until the frame has seen it.
pub(super) fn shown(slot: usize, target: f32) -> f32 {
    glide(|g| g.shown[slot]).unwrap_or(target)
}

// The list shows new things (another page): no glide from the old place.
pub(super) fn jump(slot: usize) {
    glide(|g| g.shown[slot] = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_arrives_and_stays_without_passing_the_target() {
        let (mut v, mut last) = (0.0, 0.0);
        for _ in 0..120 {
            v = step(v, 100.0, 1.0 / 60.0);
            assert!(v >= last && v <= 100.0);
            last = v;
        }
        assert_eq!(v, 100.0);
    }

    #[test]
    fn the_frame_rate_does_not_change_the_speed() {
        let (mut fast, mut slow) = (0.0, 0.0);
        for _ in 0..24 {
            fast = step(fast, 100.0, 1.0 / 240.0);
        }
        for _ in 0..3 {
            slow = step(slow, 100.0, 1.0 / 30.0);
        }
        assert!((fast - slow).abs() < 0.001, "{fast} {slow}");
    }

    #[test]
    fn a_long_pause_lands_on_the_target() {
        assert_eq!(step(0.0, 100.0, 10.0), 100.0);
    }

    #[test]
    fn it_scrolls_back_up_too() {
        let v = step(100.0, 0.0, 1.0 / 60.0);
        assert!(v < 100.0 && v > 0.0);
    }
}
