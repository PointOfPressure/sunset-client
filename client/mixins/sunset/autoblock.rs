// Server-use ownership for translated legacy sword blocking. No native shield/using-item state is fabricated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action { None, Block, Release }

#[derive(Debug, Default)]
pub(super) struct Legacy {
    slot: Option<usize>,
    ready: u64,
}

impl Legacy {
    pub(super) fn active(&self) -> bool { self.slot.is_some() }

    pub(super) fn update(&mut self, now: u64, wanted: Option<usize>, can_start: bool) -> Action {
        if self.slot.is_some() && self.slot != wanted {
            self.slot = None;
            return Action::Release;
        }
        if self.slot.is_none() && wanted.is_some() && can_start && now >= self.ready {
            self.slot = wanted;
            return Action::Block;
        }
        Action::None
    }

    pub(super) fn attack(&mut self, now: u64, delay: u64) -> bool {
        self.ready = now.saturating_add(delay);
        self.slot.take().is_some()
    }

    pub(super) fn interrupted(&mut self, now: u64) {
        self.slot = None;
        self.ready = now.saturating_add(1);
    }

    pub(super) fn stop(&mut self) -> bool {
        let active = self.slot.is_some();
        *self = Self::default();
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reblocking_does_not_change_the_existing_legacy_cps_clock() {
        use super::super::sunset_aura_timing::{Clicks, Pattern};
        let mut clock = Clicks::new(7);
        let mut block = Legacy::default();
        let mut attacks = 0;
        for tick in 0..60 {
            let due = clock.tick(10.0, 10.0, Pattern::Steady);
            if due { attacks += 1; block.attack(tick, 1); }
            let action = block.update(tick, Some(0), !due);
            if due { assert!(!block.active(), "release before a due attack"); }
            else { assert_eq!(action, Action::Block); }
        }
        assert_eq!(attacks, 30);
    }

    #[test]
    fn hold_release_for_actual_attack_then_reblock_once_after_delay() {
        let mut b = Legacy::default();
        assert_eq!(b.update(0, Some(0), true), Action::Block);
        assert_eq!(b.update(1, Some(0), true), Action::None);
        assert!(b.attack(2, 2));
        assert_eq!(b.update(2, Some(0), true), Action::None);
        assert_eq!(b.update(3, Some(0), true), Action::None);
        assert_eq!(b.update(4, Some(0), false), Action::None, "pending weapon/look operation owns the hand");
        assert_eq!(b.update(5, Some(0), true), Action::Block);
        assert!(b.stop());
        assert!(!b.stop());
    }

    #[test]
    fn target_loss_and_slot_changes_release_without_reusing_old_slot() {
        let mut b = Legacy::default();
        b.update(0, Some(0), true);
        assert_eq!(b.update(1, None, false), Action::Release);
        assert_eq!(b.update(2, None, false), Action::None);
        b.update(3, Some(1), true);
        assert_eq!(b.update(4, Some(2), true), Action::Release);
        assert_eq!(b.update(5, Some(2), true), Action::Block);
    }

    #[test]
    fn interrupted_use_or_native_slot_packet_does_not_release_someone_elses_use() {
        let mut b = Legacy::default();
        b.update(0, Some(0), true);
        b.interrupted(1);
        assert!(!b.stop(), "another module/native action owns the new use");
        b.interrupted(2);
        assert_eq!(b.update(2, Some(0), true), Action::None);
        assert_eq!(b.update(3, Some(0), true), Action::Block);
    }
}
