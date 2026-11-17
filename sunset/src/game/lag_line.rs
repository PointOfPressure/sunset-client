// Fake Lag's outgoing line: the scheduler alone, with no game state, so its rules can be tested.
//
// Policy (why it is a line, not a merger): GrimAC (TickTimer, Timer), HorizonAC (prediction, timer, combat) and vanilla
// all read the packets between two ClientTickEnds as one client tick, and place the client's Pongs among them. Real
// latency delays that whole stream evenly and in order. So the line never merges, drops or reorders packets: what went
// in goes out in the same order, whole client ticks at a time. Packets are only ever thrown away when the connection
// they were written for is gone (`discard`), and that is counted.
//
// Phases:
// - Idle: nothing held; packets pass straight through.
// - Holding: the activation (the caller's mode) wants a delay; every packet waits for its due time (`Plan`).
// - Releasing: the activation ended with packets still held. They keep their due times (latency winds down as it
//   would on a recovering connection); new packets queue behind them and the line turns Idle once empty.
// - Resetting: the line was flushed or discarded for a lifecycle event (teleport, new life, session end). Nothing is
//   held again until a ClientTickEnd has passed, so the tick that answers the event goes out whole and undelayed.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// Bounds for any mode: HorizonAC's timer banks at most 1000 ms of quiet connection, GrimAC's NegativeTimer flags
// beyond its 1200 ms drift. Past either bound, what is held goes out (it is never dropped).
pub(super) const MOST: usize = 256;
pub(super) const OLDEST: Duration = Duration::from_millis(1000);
// A held tick whose ClientTickEnd never comes (the level went away mid-tick) still goes this long after its due time.
const TAIL_GRACE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    // ServerboundClientTickEnd: the end of one client tick.
    TickEnd,
    // A packet that releases everything before it and goes at once (a hit, a use, a click), when the mode says so.
    Trigger,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Idle,
    Holding,
    Releasing,
    Resetting,
}

impl Phase {
    pub(super) fn name(self) -> &'static str {
        match self {
            Phase::Idle => "Idle",
            Phase::Holding => "Holding",
            Phase::Releasing => "Releasing",
            Phase::Resetting => "Resetting",
        }
    }
}

// When what is held now becomes due.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Plan {
    // Each packet this long after it was sent (Delay, Dynamic).
    Latency(Duration),
    // Everything at this instant (Pulse, Knockback).
    Until(Instant),
}

// Why everything held went out at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Flush {
    Trigger,
    Overflow,
    // Another module took over the outgoing stream (Blink, a combat macro, GUI Move closing).
    Yield,
    // The server re-anchored the player in the same session (teleport, respawn, a new dimension).
    Reanchor,
    // Sunset stopped (Panic).
    Stopped,
}

pub(super) const FLUSHES: [Flush; 5] = [Flush::Trigger, Flush::Overflow, Flush::Yield, Flush::Reanchor, Flush::Stopped];

impl Flush {
    pub(super) fn name(self) -> &'static str {
        match self {
            Flush::Trigger => "action",
            Flush::Overflow => "overflow",
            Flush::Yield => "yield",
            Flush::Reanchor => "re-anchor",
            Flush::Stopped => "stopped",
        }
    }
}

// Why what was held was thrown away: the connection it was written for can no longer take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Discard {
    // Another session, or none: a disconnect, a transfer, a reconnect.
    Session,
    // The same connection went back to configuration (a proxy switching backends).
    Reconfigured,
}

pub(super) const DISCARDS: [Discard; 2] = [Discard::Session, Discard::Reconfigured];

impl Discard {
    pub(super) fn name(self) -> &'static str {
        match self {
            Discard::Session => "session ended",
            Discard::Reconfigured => "reconfigured",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    // Send it now, natively: nothing is held before it.
    Pass,
    // Queue it (`push`), due at this instant.
    Hold(Instant),
    // Queue it behind everything held, then send everything (`pop` until empty).
    Flush(Flush),
}

#[derive(Clone, Debug, Default)]
pub(super) struct Stats {
    pub captured: u64,
    pub released: u64,
    pub flushes: [u64; FLUSHES.len()],
    pub discarded: [u64; DISCARDS.len()],
    pub max_depth: usize,
    // The longest a packet waited (ms), as it went out.
    pub max_wait_ms: u64,
    // Self-check: a packet went out before one queued ahead of it. Zero by construction.
    pub reordered: u64,
}

struct Entry<T> {
    item: T,
    kind: Kind,
    seq: u64,
    born: Instant,
    due: Instant,
}

pub(super) struct Line<T> {
    queue: VecDeque<Entry<T>>,
    phase: Phase,
    plan: Option<Plan>,
    seq: u64,
    last_out: u64,
    pub(super) stats: Stats,
}

impl<T> Default for Line<T> {
    fn default() -> Self {
        Self { queue: VecDeque::new(), phase: Phase::Idle, plan: None, seq: 0, last_out: 0, stats: Stats::default() }
    }
}

impl<T> Line<T> {
    pub(super) fn phase(&self) -> Phase {
        self.phase
    }

    pub(super) fn plan(&self) -> Option<Plan> {
        self.plan.filter(|_| self.phase == Phase::Holding)
    }

    pub(super) fn len(&self) -> usize {
        self.queue.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    // How long the oldest packet held has waited.
    pub(super) fn oldest(&self, now: Instant) -> Duration {
        self.queue.front().map_or(Duration::ZERO, |e| now.saturating_duration_since(e.born))
    }

    // The activation wants a delay this tick. A line resetting stays so until its tick ends.
    pub(super) fn hold(&mut self, plan: Plan) {
        if self.phase == Phase::Resetting {
            return;
        }
        self.phase = Phase::Holding;
        self.plan = Some(plan);
    }

    // The activation ended: what is held keeps its schedule and drains.
    pub(super) fn stop(&mut self) {
        if self.phase == Phase::Holding {
            self.phase = if self.queue.is_empty() { Phase::Idle } else { Phase::Releasing };
        }
        self.plan = None;
    }

    // What to do with the next packet, of `kind`, sent at `now`.
    pub(super) fn decide(&self, kind: Kind, now: Instant) -> Decision {
        let after = |at: Instant| self.queue.back().map_or(at, |b| b.due.max(at));
        match self.phase {
            Phase::Idle | Phase::Resetting | Phase::Releasing if self.queue.is_empty() => Decision::Pass,
            // Nothing held before it: a trigger simply goes.
            Phase::Holding if kind == Kind::Trigger && self.queue.is_empty() => Decision::Pass,
            Phase::Holding | Phase::Releasing if kind == Kind::Trigger => Decision::Flush(Flush::Trigger),
            _ if self.queue.len() >= MOST => Decision::Flush(Flush::Overflow),
            Phase::Holding => Decision::Hold(after(match self.plan {
                Some(Plan::Latency(d)) => now + d,
                Some(Plan::Until(at)) => at.max(now),
                None => now,
            })),
            // Behind what is held, as soon as it has gone.
            Phase::Idle | Phase::Resetting | Phase::Releasing => Decision::Hold(after(now)),
        }
    }

    pub(super) fn push(&mut self, item: T, kind: Kind, now: Instant, due: Instant) {
        self.seq += 1;
        // Due times never go backwards, so a changed delay or plan cannot reorder.
        let due = self.queue.back().map_or(due, |b| b.due.max(due));
        self.queue.push_back(Entry { item, kind, seq: self.seq, born: now, due });
        if self.phase == Phase::Idle {
            self.phase = Phase::Releasing;
        }
        self.stats.captured += 1;
        self.stats.max_depth = self.stats.max_depth.max(self.queue.len());
    }

    // How many packets from the front may go now: whole client ticks (up to the last due ClientTickEnd), except while
    // not holding, when everything due goes. Anything past the age bound goes regardless.
    pub(super) fn releasable(&self, now: Instant) -> usize {
        let mut due = 0;
        let mut whole = 0;
        let mut expired = 0;
        for (i, e) in self.queue.iter().enumerate() {
            let old = now.saturating_duration_since(e.born) >= OLDEST;
            if old {
                expired = i + 1;
            }
            if e.due > now && !old {
                break;
            }
            due = i + 1;
            if e.kind == Kind::TickEnd {
                whole = i + 1;
            }
        }
        let tail_late = self.queue.get(whole).is_some_and(|e| e.due + TAIL_GRACE <= now);
        let cut = if self.phase != Phase::Holding || tail_late { due } else { whole };
        cut.max(expired)
    }

    // The front packet, as it goes out.
    pub(super) fn pop(&mut self, now: Instant) -> Option<T> {
        let e = self.queue.pop_front()?;
        if e.seq < self.last_out {
            self.stats.reordered += 1;
        }
        self.last_out = e.seq;
        self.stats.released += 1;
        let waited = now.saturating_duration_since(e.born).as_millis() as u64;
        self.stats.max_wait_ms = self.stats.max_wait_ms.max(waited);
        if self.queue.is_empty() && self.phase == Phase::Releasing {
            self.phase = Phase::Idle;
        }
        Some(e.item)
    }

    // Everything held is to go now, in order (the caller pops until empty).
    pub(super) fn note_flush(&mut self, why: Flush) {
        let i = FLUSHES.iter().position(|f| *f == why).unwrap_or(0);
        self.stats.flushes[i] += 1;
    }

    // A lifecycle event: what is held has gone (flushed by the caller) or is thrown away; nothing is held again until
    // a ClientTickEnd passes.
    pub(super) fn reset(&mut self) {
        self.phase = Phase::Resetting;
        self.plan = None;
    }

    pub(super) fn discard(&mut self, why: Discard) -> usize {
        let n = self.queue.len();
        self.queue.clear();
        let i = DISCARDS.iter().position(|d| *d == why).unwrap_or(0);
        self.stats.discarded[i] += n as u64;
        self.reset();
        n
    }

    // A packet went out natively (Decision::Pass).
    pub(super) fn passed(&mut self, kind: Kind) {
        if self.phase == Phase::Resetting && kind == Kind::TickEnd && self.queue.is_empty() {
            self.phase = Phase::Idle;
        }
    }

    // Leave the line as new, keeping the totals.
    pub(super) fn clear_keeping_stats(&mut self) {
        let stats = std::mem::take(&mut self.stats);
        *self = Self { stats, ..Self::default() };
    }
}

// ---------------------------------------------------------------- activation (when to hold), apart from the line

// One client tick at 20 a second.
pub(super) const TICK: Duration = Duration::from_millis(50);

// Pulse and Knockback: a choke (everything due at its end), then a rest of at least a tick before the next one, so a
// pulse can neither hold forever nor restart in the tick it ended. Knockback starts one only on a fresh push; pushes
// during a choke or its rest neither extend it nor queue another.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Pulse {
    choke: Option<Instant>,
    rest: Option<Instant>,
}

impl Pulse {
    // This tick's plan. `start`: whether a new choke may begin now; `choke` rolls its length when it does.
    pub(super) fn step(&mut self, now: Instant, start: bool, choke: impl FnOnce() -> Duration, gap: Duration) -> Option<Plan> {
        if let Some(end) = self.choke {
            if now < end {
                return Some(Plan::Until(end));
            }
            self.choke = None;
            self.rest = Some(now + gap.max(TICK));
            return None;
        }
        if self.rest.is_some_and(|until| now < until) {
            return None;
        }
        self.rest = None;
        if !start {
            return None;
        }
        let end = now + choke().min(OLDEST);
        self.choke = Some(end);
        Some(Plan::Until(end))
    }

    pub(super) fn choking(&self) -> bool {
        self.choke.is_some()
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

// Dynamic: on as soon as its condition holds, off only after it has failed `exit_after` ticks running, so an enemy at
// the edge of the range cannot flip the line every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Hysteresis {
    on: bool,
    off_ticks: u32,
}

impl Hysteresis {
    pub(super) fn step(&mut self, condition: bool, exit_after: u32) -> bool {
        if condition {
            self.on = true;
            self.off_ticks = 0;
        } else if self.on {
            self.off_ticks += 1;
            if self.off_ticks >= exit_after.max(1) {
                *self = Self::default();
            }
        }
        self.on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn a_pulse_rests_a_tick_before_the_next_and_never_holds_forever() {
        let (mut p, t) = (Pulse::default(), Instant::now());
        assert_eq!(p.step(t, true, || 200 * MS, Duration::ZERO), Some(Plan::Until(t + 200 * MS)));
        assert_eq!(p.step(t + 100 * MS, true, || unreachable!(), Duration::ZERO), Some(Plan::Until(t + 200 * MS)));
        // It ends, and the very next tick does not start another.
        assert_eq!(p.step(t + 200 * MS, true, || unreachable!(), Duration::ZERO), None);
        assert_eq!(p.step(t + 220 * MS, true, || unreachable!(), Duration::ZERO), None);
        assert!(p.step(t + 250 * MS, true, || 200 * MS, Duration::ZERO).is_some());
        // A choke longer than the line's age bound is cut to it.
        let mut long = Pulse::default();
        assert_eq!(long.step(t, true, || 5000 * MS, Duration::ZERO), Some(Plan::Until(t + OLDEST)));
    }

    #[test]
    fn a_push_during_a_choke_does_not_extend_it() {
        let (mut p, t) = (Pulse::default(), Instant::now());
        assert!(p.step(t, false, || unreachable!(), Duration::ZERO).is_none());
        assert_eq!(p.step(t, true, || 150 * MS, Duration::ZERO), Some(Plan::Until(t + 150 * MS)));
        assert_eq!(p.step(t + 100 * MS, true, || 150 * MS, Duration::ZERO), Some(Plan::Until(t + 150 * MS)));
        assert_eq!(p.step(t + 150 * MS, true, || 150 * MS, 100 * MS), None);
        // Inside the rest a push is dropped, not saved for later.
        assert_eq!(p.step(t + 200 * MS, true, || 150 * MS, 100 * MS), None);
        assert_eq!(p.step(t + 250 * MS, false, || unreachable!(), 100 * MS), None);
        assert!(!p.choking());
    }

    #[test]
    fn dynamic_waits_before_it_lets_go() {
        let mut h = Hysteresis::default();
        assert!(!h.step(false, 3));
        assert!(h.step(true, 3));
        assert!(h.step(false, 3) && h.step(false, 3));
        assert!(h.step(true, 3));
        assert!(h.step(false, 3) && h.step(false, 3));
        assert!(!h.step(false, 3));
    }

    fn offer(line: &mut Line<u32>, item: u32, kind: Kind, now: Instant, out: &mut Vec<u32>) {
        match line.decide(kind, now) {
            Decision::Pass => {
                out.push(item);
                line.passed(kind);
            }
            Decision::Hold(due) => line.push(item, kind, now, due),
            Decision::Flush(why) => {
                line.push(item, kind, now, now);
                line.note_flush(why);
                while let Some(p) = line.pop(now) {
                    out.push(p);
                }
            }
        }
        release(line, now, out);
    }

    fn release(line: &mut Line<u32>, now: Instant, out: &mut Vec<u32>) {
        for _ in 0..line.releasable(now) {
            out.extend(line.pop(now));
        }
    }

    // One client tick: input, move, tick end, numbered from `base`.
    fn tick(line: &mut Line<u32>, base: u32, now: Instant, out: &mut Vec<u32>) {
        offer(line, base, Kind::Other, now, out);
        offer(line, base + 1, Kind::Other, now, out);
        offer(line, base + 2, Kind::TickEnd, now, out);
    }

    #[test]
    fn idle_line_passes_everything_in_order() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        tick(&mut line, 0, t, &mut out);
        assert_eq!(out, [0, 1, 2]);
        assert_eq!(line.phase(), Phase::Idle);
    }

    #[test]
    fn latency_delays_whole_ticks_and_never_reorders_or_drops() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(100 * MS));
        for n in 0..10 {
            tick(&mut line, n * 3, t + 50 * MS * n, &mut out);
            // Never part of a tick: what went out ends on a tick end.
            assert!(out.len() % 3 == 0, "{out:?}");
        }
        // After 100 ms the first ticks are out; everything sent is out or held, in order.
        assert!(!out.is_empty());
        line.stop();
        release(&mut line, t + 2000 * MS, &mut out);
        assert_eq!(out, (0..30).collect::<Vec<_>>());
        assert_eq!(line.phase(), Phase::Idle);
        assert_eq!(line.stats.reordered, 0);
    }

    #[test]
    fn a_trigger_sends_what_is_held_first_and_holding_continues() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(300 * MS));
        tick(&mut line, 0, t, &mut out);
        offer(&mut line, 3, Kind::Other, t + 10 * MS, &mut out);
        assert!(out.is_empty());
        offer(&mut line, 4, Kind::Trigger, t + 20 * MS, &mut out);
        assert_eq!(out, [0, 1, 2, 3, 4]);
        assert_eq!(line.phase(), Phase::Holding);
        offer(&mut line, 5, Kind::TickEnd, t + 30 * MS, &mut out);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn stopping_drains_on_schedule_with_new_packets_behind() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(200 * MS));
        tick(&mut line, 0, t, &mut out);
        line.stop();
        assert_eq!(line.phase(), Phase::Releasing);
        // Not yet due: the new tick waits behind the old one, rather than overtaking it.
        tick(&mut line, 3, t + 50 * MS, &mut out);
        assert!(out.is_empty());
        release(&mut line, t + 200 * MS, &mut out);
        assert_eq!(out, [0, 1, 2, 3, 4, 5]);
        assert_eq!(line.phase(), Phase::Idle);
        tick(&mut line, 6, t + 250 * MS, &mut out);
        assert_eq!(out.len(), 9);
    }

    #[test]
    fn a_pulse_goes_at_once_at_its_end() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Until(t + 300 * MS));
        for n in 0..5 {
            tick(&mut line, n * 3, t + 50 * MS * n, &mut out);
        }
        release(&mut line, t + 299 * MS, &mut out);
        assert!(out.is_empty());
        release(&mut line, t + 300 * MS, &mut out);
        assert_eq!(out, (0..15).collect::<Vec<_>>());
    }

    #[test]
    fn age_and_size_bounds_release_and_never_drop() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Until(t + 10_000 * MS));
        offer(&mut line, 0, Kind::Other, t, &mut out);
        release(&mut line, t + OLDEST, &mut out);
        assert_eq!(out, [0]);
        out.clear();
        for n in 0..MOST as u32 + 1 {
            offer(&mut line, n, Kind::Other, t + OLDEST, &mut out);
        }
        assert_eq!(out, (0..MOST as u32 + 1).collect::<Vec<_>>());
        assert_eq!(line.stats.flushes[1], 1);
    }

    #[test]
    fn reset_holds_nothing_until_the_answering_tick_ends() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(300 * MS));
        tick(&mut line, 0, t, &mut out);
        // A teleport: everything held goes (in order), then the line resets.
        line.note_flush(Flush::Reanchor);
        while let Some(p) = line.pop(t) {
            out.push(p);
        }
        line.reset();
        line.hold(Plan::Latency(300 * MS));
        assert_eq!(line.phase(), Phase::Resetting);
        // The accept and the confirming move go straight out, then the tick end ends the reset.
        tick(&mut line, 3, t + MS, &mut out);
        assert_eq!(out, [0, 1, 2, 3, 4, 5]);
        assert_eq!(line.phase(), Phase::Idle);
        line.hold(Plan::Latency(300 * MS));
        tick(&mut line, 6, t + 2 * MS, &mut out);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn a_discard_counts_what_it_threw_away() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(300 * MS));
        tick(&mut line, 0, t, &mut out);
        assert_eq!(line.discard(Discard::Reconfigured), 3);
        assert_eq!(line.stats.discarded, [0, 3]);
        assert!(line.is_empty());
        assert_eq!(line.phase(), Phase::Resetting);
    }

    #[test]
    fn a_shorter_delay_never_overtakes_what_is_held() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(500 * MS));
        tick(&mut line, 0, t, &mut out);
        line.hold(Plan::Latency(10 * MS));
        tick(&mut line, 3, t + 50 * MS, &mut out);
        release(&mut line, t + 100 * MS, &mut out);
        assert!(out.is_empty());
        release(&mut line, t + 500 * MS, &mut out);
        assert_eq!(out, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn holding_waits_for_the_tick_end_but_not_forever() {
        let (mut line, t, mut out) = (Line::default(), Instant::now(), Vec::new());
        line.hold(Plan::Latency(100 * MS));
        offer(&mut line, 0, Kind::Other, t, &mut out);
        release(&mut line, t + 150 * MS, &mut out);
        assert!(out.is_empty());
        release(&mut line, t + 100 * MS + TAIL_GRACE, &mut out);
        assert_eq!(out, [0]);
    }
}
