// Hand View's switches, read where the first-person hands are posed and set by the game: a transform for each arm
// (right, left) put after the game's own pose, and whether the hands keep still when the view turns.
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

pub static NO_SWAY: AtomicBool = AtomicBool::new(false);
pub static VIEW: RwLock<Option<[glam::Mat4; 2]>> = RwLock::new(None);

// A pose with the arm's own transform after it.
pub fn posed(left: bool, pose: glam::Mat4) -> glam::Mat4 {
    match VIEW.read() {
        Ok(view) => view.map_or(pose, |v| pose * v[usize::from(left)]),
        Err(_) => pose,
    }
}

thread_local! {
    // An arm is being drawn again in its new pose.
    static POSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub fn posing() -> bool {
    POSING.with(std::cell::Cell::get)
}
pub fn set_posing(v: bool) {
    POSING.with(|p| p.set(v));
}
