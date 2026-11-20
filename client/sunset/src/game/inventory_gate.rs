// What Chest Stealer and Inventory Manager share about sending clicks: when the next may go.
//
// Old servers behind ViaVersion (seen on 1.8 servers with OldNCP): a 1.17+ client's ContainerClick carries the cursor
// after the click, where a pre-1.17 server wants the item the click took out, so ViaVersion writes the cursor there,
// or nothing for a Shift click (`ItemPacketRewriter1_17`). A 1.8 server compares that with its own result: a Shift
// click that moved anything, an item put on a stack and a split never match. It still makes the click, but answers
// "refused" with the whole menu, and takes no click at all until the client acknowledges (ViaVersion's Ping and the
// client's Pong, `packet_tap::refusal_ping`). Clicks sent in that round trip are dropped, and the menu sent back
// undoes them here: items flicker back and the actions are lost. So after a click such a server refuses, the next
// waits for the refusal's Ping (or for a round trip to pass without one), and on a server that has refused one, the
// modules click in ways it accepts (a pickup and a put-down into an empty slot, hotbar-key swaps, throws).
//
// NoCheatPlus's Inventory.FastClick cancels a click past 4 in its 200 ms bucket or 15 in its second, and a cancelled
// click is sent back too: the click budget keeps under both (sliding windows, a little stricter than NCP's buckets).
//
// Delays are in milliseconds with the time carried over: the next action is due Delay after the last was due, not
// after the tick it went in, so a 120 ms Delay averages 120 ms at 50 ms ticks, and under 50 ms several go in a tick.

use super::Game;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(50);

// ---- Delay

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Pacer {
    due: Option<Instant>,
}

impl Pacer {
    pub(super) const fn new() -> Self {
        Pacer { due: None }
    }

    pub(super) fn ready(&self, now: Instant) -> bool {
        self.due.is_none_or(|due| now >= due)
    }

    // An action went at `now`: the next is due `delay` after this one was due, when that was within the last tick;
    // after a longer wait (nothing to do, a server's answer), `delay` from now.
    pub(super) fn sent(&mut self, now: Instant, delay: Duration) {
        let on_time = |due: &Instant| now.checked_sub(TICK).is_none_or(|tick_ago| *due >= tick_ago);
        let base = self.due.filter(on_time).unwrap_or(now);
        self.due = Some(base + delay);
    }

    // The next action is due `delay` from now (a first item's delay).
    pub(super) fn wait(&mut self, now: Instant, delay: Duration) {
        self.due = Some(now + delay);
    }

    pub(super) fn reset(&mut self) {
        self.due = None;
    }
}

// A random delay between the two settings (either order), in whole milliseconds.
pub(super) fn roll(lo: f64, hi: f64, unit: f64) -> Duration {
    let (lo, hi) = (lo.min(hi).max(0.0), lo.max(hi).max(0.0));
    Duration::from_millis((lo + (hi - lo) * unit.clamp(0.0, 1.0)).round() as u64)
}

// ---- refused clicks and the click budget

#[derive(Clone, Copy, Debug)]
struct Wait {
    refusals: u64,
    until: Instant,
    // Unanswered, it shows the server takes such clicks (a Shift click that moved something: a 1.8 server always
    // refuses those).
    proves: bool,
}

#[derive(Debug, Default)]
struct Gate {
    login: u64,
    // The refusal count when this connection was first seen, and at the last look.
    base: u64,
    seen: u64,
    // Some(true): this server refused a click as described above; Some(false): it took a Shift click without.
    legacy: Option<bool>,
    waiting: Option<Wait>,
    // The game tick the refusal was seen in: the next click goes a tick later, after the game's Pong.
    answered: Option<u64>,
    clicks: VecDeque<Instant>,
}

static GATE: Mutex<Option<Gate>> = Mutex::new(None);

fn gate<R>(f: impl FnOnce(&mut Gate) -> R) -> R {
    let mut g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Gate::default))
}

impl Gate {
    fn sync(&mut self, (login, refusals): (u64, u64), now: Instant, tick: u64) {
        if login != self.login {
            *self = Gate { login, base: refusals, seen: refusals, clicks: std::mem::take(&mut self.clicks), ..Gate::default() };
        }
        if let Some(w) = self.waiting {
            if refusals > w.refusals {
                self.legacy = Some(true);
                self.waiting = None;
                self.answered = Some(tick);
            } else if now >= w.until {
                if w.proves && self.legacy.is_none() {
                    self.legacy = Some(false);
                }
                self.waiting = None;
            }
        } else if refusals > self.seen {
            // A refusal of a click that was not waited for (the player's, another module's, or one this server
            // refused unexpectedly): the next click goes after the game's Pong too.
            if refusals > self.base {
                self.legacy = Some(true);
            }
            self.answered = Some(tick);
        }
        self.seen = refusals;
    }

    fn ready(&self, tick: u64) -> bool {
        self.waiting.is_none() && self.answered.is_none_or(|t| tick > t)
    }

    fn refusable(&mut self, refusals: u64, now: Instant, round_trip: Duration, proves: bool) {
        if self.legacy == Some(false) {
            return;
        }
        let until = now + round_trip * 2 + Duration::from_millis(200);
        self.waiting = Some(Wait { refusals, until, proves });
    }

    fn room(&mut self, now: Instant, n: usize) -> bool {
        while self.clicks.front().is_some_and(|&t| now.duration_since(t) >= Duration::from_secs(1)) {
            self.clicks.pop_front();
        }
        let recent = self.clicks.iter().filter(|&&t| now.duration_since(t) < Duration::from_millis(200)).count();
        recent + n <= 4 && self.clicks.len() + n <= 15
    }

    fn clicked(&mut self, now: Instant, n: usize) {
        for _ in 0..n {
            if self.clicks.len() == 64 {
                self.clicks.pop_front();
            }
            self.clicks.push_back(now);
        }
    }
}

fn round_trip(game: &Game<'_>) -> Duration {
    let ms = super::remote::sunset_combat_remote::latency(game).unwrap_or(150).clamp(0, 2000);
    Duration::from_millis(ms as u64)
}

// No refusable click is waiting for its answer.
pub(super) fn ready(game: &Game<'_>) -> bool {
    let tick = game.ticks;
    gate(|g| {
        g.sync(super::sunset_packets::refusals(), Instant::now(), tick);
        g.ready(tick)
    })
}

// This server has refused a click the way old servers behind ViaVersion do.
pub(super) fn legacy(game: &Game<'_>) -> bool {
    let tick = game.ticks;
    gate(|g| {
        g.sync(super::sunset_packets::refusals(), Instant::now(), tick);
        g.legacy == Some(true)
    })
}

// A click such a server refuses went out: the next waits for its answer. `proves`: a Shift click that moved something.
pub(super) fn refusable(game: &Game<'_>, proves: bool) {
    let rt = round_trip(game);
    let (_, refusals) = super::sunset_packets::refusals();
    gate(|g| g.refusable(refusals, Instant::now(), rt, proves));
}

// Whether `n` more clicks now stay within NoCheatPlus's click limits.
pub(super) fn room(n: usize) -> bool {
    gate(|g| g.room(Instant::now(), n))
}

// `n` clicks went out (counted whether or not a module keeps to the limits: NoCheatPlus counts them all).
pub(super) fn clicked(n: usize) {
    gate(|g| g.clicked(Instant::now(), n));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    // The times actions go at with ticks every 50 ms, over `span` ms.
    fn run(delay: u64, span: u64) -> Vec<u64> {
        let start = Instant::now();
        let mut p = Pacer::default();
        let mut out = Vec::new();
        for t in (0..span).step_by(50) {
            let now = start + ms(t);
            let mut n = 0;
            while p.ready(now) && n < 64 {
                p.sent(now, ms(delay));
                out.push(t);
                n += 1;
            }
        }
        out
    }

    #[test]
    fn every_ten_milliseconds_counts_on_average() {
        for (delay, expect) in [(100, 20), (120, 17), (150, 14), (70, 29), (60, 34)] {
            let n = run(delay, 2000).len();
            assert!(n.abs_diff(expect) <= 1, "{delay} ms: {n} actions in 2 s, expected about {expect}");
        }
        // Under a tick, several go in one.
        let n = run(20, 1000).len();
        assert!(n.abs_diff(50) <= 2, "20 ms: {n}");
        // No delay: everything at once, up to the cap.
        assert_eq!(run(0, 50).len(), 64);
    }

    #[test]
    fn an_idle_pacer_does_not_burst() {
        let start = Instant::now();
        let mut p = Pacer::default();
        p.sent(start, ms(100));
        // Long after: one action, the next a Delay later.
        let late = start + ms(5000);
        assert!(p.ready(late));
        p.sent(late, ms(100));
        assert!(!p.ready(late));
        assert!(!p.ready(late + ms(50)));
        assert!(p.ready(late + ms(100)));
    }

    #[test]
    fn rolls_stay_in_range_either_way_round() {
        assert_eq!(roll(100.0, 150.0, 0.0), ms(100));
        assert_eq!(roll(150.0, 100.0, 1.0), ms(150));
        assert_eq!(roll(0.0, 0.0, 0.7), ms(0));
    }

    #[test]
    fn a_refused_click_holds_the_next_until_its_ping_and_a_tick() {
        let now = Instant::now();
        let mut g = Gate::default();
        g.sync((1, 0), now, 10);
        g.refusable(0, now, ms(100), true);
        g.sync((1, 0), now + ms(60), 11);
        assert!(!g.ready(11));
        // The Ping came: legacy, and free a tick later (after the game's Pong).
        g.sync((1, 1), now + ms(110), 12);
        assert_eq!(g.legacy, Some(true));
        assert!(!g.ready(12));
        assert!(g.ready(13));
    }

    #[test]
    fn an_unanswered_shift_click_shows_a_modern_server() {
        let now = Instant::now();
        let mut g = Gate::default();
        g.sync((1, 0), now, 0);
        g.refusable(0, now, ms(50), true);
        g.sync((1, 0), now + ms(299), 5);
        assert!(!g.ready(5));
        g.sync((1, 0), now + ms(300), 6);
        assert!(g.ready(6));
        assert_eq!(g.legacy, Some(false));
        // Known modern: nothing waits any more.
        g.refusable(0, now, ms(50), true);
        assert!(g.ready(7));
        // A new connection starts over.
        g.sync((2, 0), now + ms(400), 8);
        assert_eq!(g.legacy, None);
    }

    #[test]
    fn the_click_budget_keeps_under_nocheatplus() {
        let now = Instant::now();
        let mut g = Gate::default();
        assert!(g.room(now, 4));
        assert!(!g.room(now, 5));
        g.clicked(now, 4);
        assert!(!g.room(now + ms(199), 1));
        assert!(g.room(now + ms(200), 1));
        // 15 a second.
        for i in 1..3 {
            g.clicked(now + ms(200 * i), 4);
        }
        g.clicked(now + ms(600), 3);
        assert!(!g.room(now + ms(900), 1));
        assert!(g.room(now + ms(1000), 4));
    }

    #[test]
    fn only_vias_refusal_pings_count() {
        use super::super::sunset_packets::refusal_ping;
        assert!(refusal_ping((1 << 30) | (3 << 16)));
        assert!(refusal_ping(1 << 30));
        assert!(!refusal_ping((1 << 30) | (3 << 16) | 5));
        assert!(!refusal_ping(-1));
        assert!(!refusal_ping(1234));
    }
}
