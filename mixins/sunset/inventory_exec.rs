// Inventory Manager's executor: when a planned step may go, and what became of the last one. Pure state, a tick at a
// time (player.rs gives it the inventory's fingerprint and does the clicking), so it is unit tested here.
//
// Predicted clicks use Delay as their cadence, with at most one logical operation per tick. Ping controls how long
// recent operations are watched for a rollback, not how long each successful prediction stalls the next action.
// A vanilla server acknowledges correct prediction with silence. An operation that leaves no trace, or whose
// original contents return during its observation window, is suppressed on those contents. Recent history is
// bounded. Other inventory owners can explicitly hold dispatch; player input always takes priority.

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
    watch_until: u64,
    revision: u64,
}

#[derive(Debug, Default)]
pub(super) struct Exec {
    context: Option<u64>,
    revision: u64,
    // The fingerprint the last plan was made on, and whether it found nothing to do.
    planned: Option<(u64, bool)>,
    pending: Option<Pending>,
    recent: Vec<Pending>,
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
    // Settings, locks and menu context can change without an inventory packet. Invalidate decisions, but keep the
    // outstanding click's settle deadline: a configuration edit is not permission to overlap transactions.
    pub(super) fn context(&mut self, context: u64) {
        if self.context != Some(context) {
            self.context = Some(context);
            self.revision = self.revision.wrapping_add(1);
            self.planned = None;
            self.refused.clear();
        }
    }

    pub(super) fn blocked(&self, fingerprint: u64) -> Vec<Step> {
        self.refused.iter().filter_map(|&(step, on)| (on == fingerprint).then_some(step)).collect()
    }

    // Later actions must not depend on unconfirmed slot contents: a rejected swap followed by a throw could drop
    // the valuable source item still present on the server. Independent slots can proceed at full cadence.
    pub(super) fn reserved(&self, now: u64) -> Vec<Step> {
        self.pending.iter().chain(self.recent.iter()).filter(|p| now < p.watch_until).map(|p| p.step).collect()
    }

    fn remember(&mut self, p: Pending) {
        if self.recent.len() == 64 { self.recent.remove(0); }
        self.recent.push(p);
    }

    // You are using the inventory yourself (an item on the cursor, a button down): nothing for a moment after.
    pub(super) fn user(&mut self, now: u64) {
        self.quiet_until = self.quiet_until.max(now + 1).max(self.pending.map_or(0, |p| p.until));
        if let Some(p) = self.pending.take().filter(|p| now < p.watch_until) { self.remember(p); }
        // The user can legitimately recreate old contents; keep the slot reservations, but abandon attribution.
        self.revision = self.revision.wrapping_add(1);
        self.planned = None;
    }

    pub(super) fn gate(&mut self, now: u64, fingerprint: u64) -> Gate {
        if now < self.quiet_until {
            return Gate::Wait;
        }
        // A delayed correction may restore an inventory from before an earlier predicted operation. Keep watching
        // after its cadence deadline, including when the latest plan has already found nothing else to do.
        let recent = std::mem::take(&mut self.recent);
        for p in recent {
            if p.revision == self.revision && now <= p.watch_until && fingerprint == p.before {
                self.refuse(p);
                self.planned = None;
            } else if now < p.watch_until {
                self.recent.push(p);
            } else {
                // Work skipped due to a reserved slot may now be possible even without changed contents.
                self.planned = None;
            }
        }
        if let Some(p) = self.pending {
            if now < p.until {
                return Gate::Wait;
            }
            // Settled: no trace of it means the server refused it (or put it back).
            if fingerprint == p.before && p.revision == self.revision {
                self.refuse(p);
            } else if now < p.watch_until {
                self.remember(p);
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

    fn refuse(&mut self, p: Pending) {
        if !self.refused.contains(&(p.step, p.before)) {
            self.refused.push((p.step, p.before));
            self.stats.refused += 1;
        }
    }

    // The plan made this tick on `fingerprint` (in `took` of time): the step to take, if any is worth taking.
    pub(super) fn planned(&mut self, fingerprint: u64, step: Option<Step>, took: Duration) -> Option<Step> {
        self.stats.plans += 1;
        self.stats.plan_total += took;
        self.stats.plan_most = self.stats.plan_most.max(took);
        self.planned = Some((fingerprint, step.is_none()));
        step.filter(|s| !self.refused.iter().any(|(r, on)| r == s && *on == fingerprint))
    }

    // The step went out at `now`; the next waits `settle` ticks (at least 1).
    pub(super) fn sent(&mut self, step: Step, fingerprint: u64, now: u64, settle: u64) {
        self.stats.steps += 1;
        let until = now + settle.max(1);
        self.pending = Some(Pending { step, before: fingerprint, until, watch_until: until, revision: self.revision });
    }

    pub(super) fn sent_predicted(&mut self, step: Step, fingerprint: u64, now: u64, delay: u64, latency_ms: Option<i32>) {
        self.sent(step, fingerprint, now, delay.max(1));
        if let Some(p) = &mut self.pending {
            p.watch_until = now + settle_ticks(latency_ms, delay);
        }
    }

    // A step is still settling, or you are at the inventory yourself.
    pub(super) fn busy(&self, now: u64) -> bool {
        now < self.quiet_until || self.pending.iter().chain(self.recent.iter()).any(|p| now < p.watch_until)
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
    fn zero_delay_uses_prediction_without_waiting_a_round_trip() {
        for ping in [None, Some(0), Some(100), Some(2000)] {
            let mut e = Exec::default();
            e.sent_predicted(Step::Swap { from: 9, to: 0 }, 1, 0, 0, ping);
            assert_eq!(e.gate(0, 2), Gate::Wait, "one operation per tick");
            assert_eq!(e.gate(1, 2), Gate::Plan, "ping must not override zero delay: {ping:?}");
        }
    }

    #[test]
    fn mouse_release_has_no_half_second_delay() {
        let mut e = Exec::default();
        e.user(10);
        assert_eq!(e.gate(10, 1), Gate::Wait);
        assert_eq!(e.gate(11, 1), Gate::Plan);
    }

    #[test]
    fn delayed_rollbacks_are_detected_after_another_predicted_step() {
        let mut e = Exec::default();
        let first = Step::Swap { from: 9, to: 0 };
        e.sent_predicted(first, 1, 0, 0, Some(500));
        assert_eq!(e.gate(1, 2), Gate::Plan);
        e.sent_predicted(Step::Throw(10), 2, 1, 0, Some(500));
        assert_eq!(e.gate(2, 3), Gate::Plan);
        e.planned(3, None, D);
        assert_eq!(e.gate(3, 3), Gate::Wait);
        assert_eq!(e.gate(4, 1), Gate::Plan, "a late correction restores the first operation's input");
        assert_eq!(e.blocked(1), [first]);
        assert_eq!(e.planned(1, Some(first), D), None, "do not loop the corrected operation");
        assert_eq!(e.stats.refused, 1);
    }

    #[test]
    fn configured_delays_and_external_holds_are_still_respected() {
        for delay in [1, 2, 6] {
            let mut e = Exec::default();
            e.sent_predicted(Step::Throw(9), 1, 0, delay, Some(2000));
            assert_eq!(e.gate(delay - 1, 2), Gate::Wait);
            assert_eq!(e.gate(delay, 2), Gate::Plan, "delay is not replaced by ping");
            e.hold(50);
            assert_eq!(e.gate(49, 2), Gate::Wait, "Auto Armor retains its explicit deadline");
            assert_eq!(e.gate(50, 2), Gate::Plan);
        }
    }

    #[test]
    fn dependent_cleanup_waits_for_slots_and_replans_when_they_are_released() {
        let mut e = Exec::default();
        e.context(1);
        let swap = Step::Swap { from: 9, to: 0 };
        e.sent_predicted(swap, 1, 0, 0, Some(200)); // Observe through tick 5.
        assert_eq!(e.gate(1, 2), Gate::Plan, "independent slots can work next tick");
        assert_eq!(e.reserved(1), [swap], "do not throw the displaced sword at slot 9 yet");
        assert!(e.busy(1), "Auto Armor must not conflict with the reserved slots either");
        e.planned(2, None, D); // Only dependent work remains.
        assert_eq!(e.gate(4, 2), Gate::Wait);
        assert_eq!(e.gate(5, 2), Gate::Plan, "reservation expiry invalidates the finished-plan cache");
        assert!(e.reserved(5).is_empty());
        assert!(!e.busy(5));

        e.sent_predicted(swap, 2, 5, 0, Some(200));
        e.context(2);
        assert_eq!(e.gate(6, 3), Gate::Plan);
        e.user(7);
        assert_eq!(e.reserved(8), [swap], "settings/input cannot expose uncertain slots to cleanup");
        assert_eq!(e.gate(8, 2), Gate::Plan);
        assert!(e.blocked(2).is_empty(), "old context/manual input cannot be attributed to a refusal");
    }

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
        // The caller excludes refused steps before asking for a new plan.
        assert_eq!(e.blocked(7), [Step::Exchange(9, 12)]);
        e.planned(7, None, D);
        // Unchanged and genuinely nothing left to do: no more planning at all.
        assert_eq!(e.gate(3, 7), Gate::Wait);
        // The inventory changes: it may be tried again.
        assert_eq!(e.gate(4, 8), Gate::Plan);
        assert_eq!(e.planned(8, Some(Step::Exchange(9, 12)), D), Some(Step::Exchange(9, 12)));
    }

    #[test]
    fn refusing_one_step_does_not_mark_the_whole_inventory_finished() {
        let mut e = Exec::default();
        let step = Step::Throw(9);
        e.sent(step, 7, 0, 1);
        assert_eq!(e.gate(1, 7), Gate::Plan);
        assert_eq!(e.planned(7, Some(step), D), None);
        assert_eq!(e.gate(2, 7), Gate::Plan, "the planner must still be able to try another slot");
        assert_eq!(e.planned(7, Some(Step::Throw(10)), D), Some(Step::Throw(10)));
    }

    #[test]
    fn changed_settings_replan_but_do_not_interrupt_a_pending_click() {
        let mut e = Exec::default();
        e.context(1);
        e.planned(7, None, D);
        assert_eq!(e.gate(1, 7), Gate::Wait);
        e.context(2);
        assert_eq!(e.gate(1, 7), Gate::Plan);
        e.sent(Step::Swap { from: 9, to: 0 }, 7, 1, 4);
        e.context(3);
        assert_eq!(e.gate(2, 8), Gate::Wait);
        assert_eq!(e.gate(5, 7), Gate::Plan);
        assert!(e.blocked(7).is_empty(), "an old context's failed step cannot block the new layout");
        e.reset();
        assert!(e.blocked(7).is_empty());
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
        assert_eq!(e.gate(10, 1), Gate::Wait);
        e.user(11); // Still held: every tick continues yielding to the player.
        assert_eq!(e.gate(11, 1), Gate::Wait);
        assert_eq!(e.gate(12, 1), Gate::Plan);
        e.sent(Step::Throw(9), 1, 20, 41);
        e.user(21);
        assert!(e.busy(60), "manual input must not shorten an outstanding transaction's deadline");
        assert_eq!(e.gate(31, 2), Gate::Wait);
        assert_eq!(e.gate(61, 2), Gate::Plan);
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
