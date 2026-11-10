// Inventory Manager's executor: when a planned step may go, and what became of the last one. Pure state, a tick at a
// time (player.rs gives it the inventory's fingerprint and does the clicking), so it is unit tested here.
//
// One step is in flight at a time. After a click the next one waits at least a round trip (Settle: the ping in ticks,
// and never less than Delay), so the server's answer to the last click (a correction, a full resync) is in before the
// next is planned: clicks are never stacked on an inventory the server may not share. A vanilla server answers a
// click it agrees with by sending nothing, so silence is not taken as failure; a step the inventory shows no trace of
// once it settled was refused, and the same step on the same inventory is not tried again. Nothing is planned while
// the inventory is as it was when the last plan found nothing to do, nor while you hold an item on the cursor or are
// clicking yourself.

use std::time::Duration;

// One click step: a hotbar-key swap, a stack thrown, or two slots changing places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Swap { from: usize, to: usize },
    Throw(usize),
    Exchange(usize, usize),
}

// What the last ticks cost and did, for the Debug Report.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stats {
    pub(super) plans: u64,
    pub(super) skipped: u64,
    pub(super) steps: u64,
    pub(super) refused: u64,
    pub(super) plan_total: Duration,
    pub(super) plan_most: Duration,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    step: Step,
    before: u64,
    until: u64,
}

#[derive(Debug, Default)]
pub(super) struct Exec {
    // The fingerprint the last plan was made on, and whether it found nothing to do.
    planned: Option<(u64, bool)>,
    pending: Option<Pending>,
    // Steps that left no trace, with the inventory they were tried on.
    refused: Vec<(Step, u64)>,
    quiet_until: u64,
    pub(super) stats: Stats,
}

// Whether to plan this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gate {
    Wait,
    Plan,
}

impl Exec {
    // You are using the inventory yourself (an item on the cursor, a button down): nothing for a moment after.
    pub(super) fn user(&mut self, now: u64) {
        self.quiet_until = now + 10;
        self.pending = None;
    }

    pub(super) fn gate(&mut self, now: u64, fingerprint: u64) -> Gate {
        if now < self.quiet_until {
            return Gate::Wait;
        }
        if let Some(p) = self.pending {
            if now < p.until {
                return Gate::Wait;
            }
            // Settled: no trace of it means the server refused it (or put it back).
            if fingerprint == p.before {
                self.refused.push((p.step, p.before));
                self.stats.refused += 1;
            }
            self.pending = None;
        }
        // What was refused on another inventory may work now.
        self.refused.retain(|(_, on)| *on == fingerprint);
        if self.planned == Some((fingerprint, true)) {
            self.stats.skipped += 1;
            return Gate::Wait;
        }
        Gate::Plan
    }

    // The plan made this tick on `fingerprint` (in `took` of time): the step to take, if any is worth taking.
    pub(super) fn planned(&mut self, fingerprint: u64, step: Option<Step>, took: Duration) -> Option<Step> {
        self.stats.plans += 1;
        self.stats.plan_total += took;
        self.stats.plan_most = self.stats.plan_most.max(took);
        let step = step.filter(|s| !self.refused.iter().any(|(r, on)| r == s && *on == fingerprint));
        self.planned = Some((fingerprint, step.is_none()));
        step
    }

    // The step went out at `now`; the next waits `settle` ticks (at least 1).
    pub(super) fn sent(&mut self, step: Step, fingerprint: u64, now: u64, settle: u64) {
        self.stats.steps += 1;
        self.pending = Some(Pending { step, before: fingerprint, until: now + settle.max(1) });
    }

    // A step is still settling, or you are at the inventory yourself.
    pub(super) fn busy(&self, now: u64) -> bool {
        now < self.quiet_until || self.pending.is_some_and(|p| now < p.until)
    }

    // Another part of the inventory modules (Auto Armor) clicked: wait until `until` before planning.
    pub(super) fn hold(&mut self, until: u64) {
        self.quiet_until = self.quiet_until.max(until);
    }

    // Turned off, a new world: nothing pending, nothing remembered.
    pub(super) fn reset(&mut self) {
        let stats = self.stats;
        *self = Self { stats, ..Self::default() };
    }
}

// The ticks a step settles for: a round trip at the server's ping plus a tick, or Delay if that is longer.
pub(super) fn settle_ticks(latency_ms: Option<i32>, delay_ticks: u64) -> u64 {
    let rtt = u64::from(latency_ms.unwrap_or(100).clamp(0, 2000) as u32).div_ceil(50) + 1;
    rtt.max(delay_ticks).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Duration = Duration::ZERO;

    #[test]
    fn one_step_in_flight_and_it_settles_before_the_next() {
        let mut e = Exec::default();
        assert_eq!(e.gate(0, 1), Gate::Plan);
        let s = e.planned(1, Some(Step::Throw(4)), D).unwrap();
        e.sent(s, 1, 0, 3);
        // The inventory changed at once (the client's prediction), but the step still settles.
        assert_eq!(e.gate(1, 2), Gate::Wait);
        assert_eq!(e.gate(2, 2), Gate::Wait);
        assert_eq!(e.gate(3, 2), Gate::Plan);
        assert_eq!(e.stats.refused, 0);
    }

    #[test]
    fn a_refused_step_is_not_tried_again_on_the_same_inventory() {
        let mut e = Exec::default();
        assert_eq!(e.gate(0, 7), Gate::Plan);
        let s = e.planned(7, Some(Step::Exchange(9, 12)), D).unwrap();
        e.sent(s, 7, 0, 2);
        // Settled with the inventory as before: refused.
        assert_eq!(e.gate(2, 7), Gate::Plan);
        assert_eq!(e.stats.refused, 1);
        assert_eq!(e.planned(7, Some(Step::Exchange(9, 12)), D), None, "not again");
        // Unchanged and nothing to do: no more planning at all.
        assert_eq!(e.gate(3, 7), Gate::Wait);
        // The inventory changes: it may be tried again.
        assert_eq!(e.gate(4, 8), Gate::Plan);
        assert_eq!(e.planned(8, Some(Step::Exchange(9, 12)), D), Some(Step::Exchange(9, 12)));
    }

    #[test]
    fn an_unchanged_inventory_with_nothing_to_do_is_not_planned_again() {
        let mut e = Exec::default();
        assert_eq!(e.gate(0, 5), Gate::Plan);
        assert_eq!(e.planned(5, None, D), None);
        for t in 1..100 {
            assert_eq!(e.gate(t, 5), Gate::Wait);
        }
        assert_eq!(e.stats.plans, 1);
        assert_eq!(e.stats.skipped, 99);
        assert_eq!(e.gate(100, 6), Gate::Plan);
    }

    #[test]
    fn your_own_clicks_come_first() {
        let mut e = Exec::default();
        e.user(10);
        assert_eq!(e.gate(15, 1), Gate::Wait);
        assert_eq!(e.gate(20, 1), Gate::Plan);
    }

    #[test]
    fn auto_armor_and_the_manager_take_turns() {
        let mut e = Exec::default();
        e.hold(5);
        assert!(e.busy(4));
        assert_eq!(e.gate(4, 1), Gate::Wait);
        assert!(!e.busy(5));
        assert_eq!(e.gate(5, 1), Gate::Plan);
        let s = e.planned(1, Some(Step::Throw(2)), D).unwrap();
        e.sent(s, 1, 5, 3);
        assert!(e.busy(7), "Auto Armor waits for the manager's step");
        assert!(!e.busy(8));
    }

    #[test]
    fn settling_follows_the_ping() {
        assert_eq!(settle_ticks(Some(0), 0), 1);
        assert_eq!(settle_ticks(Some(120), 1), 4);
        assert_eq!(settle_ticks(Some(40), 6), 6, "Delay when longer");
        assert_eq!(settle_ticks(None, 1), 3, "an unknown ping counts as 100 ms");
        assert_eq!(settle_ticks(Some(100_000), 1), 41, "capped at 2 s");
    }
}
