// Sunset's lifecycle for this game process, shared by the game, network and renderer crates:
// Running, then (Panic) Stopping, then Dormant, never back. Nothing is saved: the next game
// process starts Running. While Stopping, dispatch is closed and only cleanup runs; Dormant
// begins once no plugin callback is still executing. A token taken while Running stops being
// current at the first stop, so detached work started before Panic drops its result.
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering::SeqCst};

const RUNNING: u8 = 0;
const STOPPING: u8 = 1;
const DORMANT: u8 = 2;

pub struct Lifecycle {
    state: AtomicU8,
    callbacks: AtomicUsize,
}

pub static RUNTIME: Lifecycle = Lifecycle::new();

impl Lifecycle {
    pub const fn new() -> Self {
        Self { state: AtomicU8::new(RUNNING), callbacks: AtomicUsize::new(0) }
    }

    pub fn running(&self) -> bool { self.state.load(SeqCst) == RUNNING }
    pub fn stopping(&self) -> bool { self.state.load(SeqCst) == STOPPING }
    pub fn dormant(&self) -> bool { self.state.load(SeqCst) == DORMANT }
    pub fn idle(&self) -> bool { self.callbacks.load(SeqCst) == 0 }
    pub fn token(&self) -> u64 { u64::from(self.state.load(SeqCst)) }
    pub fn current(&self, token: u64) -> bool { token == u64::from(RUNNING) && self.running() }

    // Running -> Stopping. True for the one call that started the stop.
    pub fn request_stop(&self) -> bool {
        self.state.compare_exchange(RUNNING, STOPPING, SeqCst, SeqCst).is_ok()
    }

    // Stopping -> Dormant once no callback is in flight. True for the one call that finished it.
    pub fn finish_stop(&self) -> bool {
        self.idle() && self.state.compare_exchange(STOPPING, DORMANT, SeqCst, SeqCst).is_ok()
    }

    // A callback may run only while Running. The count is taken before the state is checked
    // again, so a stop requested in between either sees the callback or the callback sees it.
    pub fn enter(&self) -> Option<Callback<'_>> {
        if !self.running() { return None; }
        self.callbacks.fetch_add(1, SeqCst);
        let callback = Callback(self);
        self.running().then_some(callback)
    }
}

pub struct Callback<'a>(&'a Lifecycle);

impl Drop for Callback<'_> {
    fn drop(&mut self) { self.0.callbacks.fetch_sub(1, SeqCst); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_blocks_dispatch_and_waits_for_in_flight_callback() {
        let life = Lifecycle::new();
        let callback = life.enter().unwrap();
        assert!(life.request_stop());
        assert!(!life.request_stop());
        assert!(life.stopping() && !life.running());
        assert!(life.enter().is_none());
        assert!(!life.finish_stop());
        drop(callback);
        assert!(life.finish_stop());
        assert!(!life.finish_stop());
        assert!(life.dormant());
    }

    #[test]
    fn dormant_is_final() {
        let life = Lifecycle::new();
        let work = life.token();
        assert!(life.current(work));
        assert!(life.request_stop());
        assert!(!life.current(work));
        assert!(life.finish_stop());
        for _ in 0..4 {
            assert!(!life.request_stop());
            assert!(!life.finish_stop());
            assert!(life.enter().is_none());
            assert!(!life.current(work) && !life.current(life.token()));
            assert!(life.dormant() && !life.running() && !life.stopping());
        }
        assert!(life.idle());
    }

    #[test]
    fn only_one_of_many_threads_starts_or_finishes_the_stop() {
        let life = std::sync::Arc::new(Lifecycle::new());
        let wins: usize = (0..8)
            .map(|_| {
                let life = life.clone();
                std::thread::spawn(move || {
                    let entered = life.enter();
                    let started = usize::from(life.request_stop());
                    drop(entered);
                    started + 10 * usize::from(life.finish_stop())
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .sum();
        // Every thread's callback is gone: if no thread got to finish, the last caller does.
        let finished_late = usize::from(life.finish_stop());
        assert_eq!(wins % 10, 1, "exactly one thread starts the stop");
        assert_eq!(wins / 10 + finished_late, 1, "exactly one call finishes it");
        assert!(life.dormant() && life.idle());
    }
}
