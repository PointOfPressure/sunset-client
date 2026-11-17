// Which explosive action goes first, Crystal Aura's or Anchor Aura's, from what each would deal and cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    CrystalHit,
    AnchorHit,
    Charge,
    CrystalPlace,
    ObsidianPlace,
    AnchorPlace,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rating {
    pub kind: Kind,
    pub target_damage: f64,
    pub target_health: f64,
    pub self_damage: f64,
    pub held_totem: bool,
    pub ready_in: u16,
    pub look_credit: f64,
    pub full_damage: f64,
}

impl Rating {
    pub(super) fn new(
        kind: Kind,
        target_damage: f64,
        target_health: f64,
        self_damage: f64,
        held_totem: bool,
        ready_in: u16,
    ) -> Self {
        Self {
            kind,
            target_damage,
            target_health,
            self_damage,
            held_totem,
            ready_in,
            look_credit: 0.0,
            full_damage: target_damage,
        }
    }
    pub(super) fn planning_value(self) -> f64 {
        self.value() + self.look_credit
    }

    pub(super) fn valid(self) -> bool {
        [self.target_damage, self.target_health, self.self_damage].iter().all(|n| n.is_finite() && *n >= 0.0)
            && self.target_health > 0.0
    }
    pub(super) fn lethal(self) -> bool {
        self.target_damage >= self.target_health
    }
    pub(super) fn value(self) -> f64 {
        self.target_damage.min(self.target_health) - 0.5 * self.self_damage
            + if !self.lethal() {
                0.0
            } else if self.held_totem {
                500.0
            } else {
                1000.0
            }
    }
}

// Preserve scan order for exact ties; ready-in is a tie break within one kind.
// This accepts a bounded caller slice; no world scan or packet side effects.
pub(super) fn select(candidates: &[Rating]) -> Option<usize> {
    let mut best: [Option<usize>; 6] = [None; 6];
    let slot = |k| match k {
        Kind::CrystalHit => 0,
        Kind::AnchorHit => 1,
        Kind::Charge => 2,
        Kind::CrystalPlace => 3,
        Kind::ObsidianPlace => 4,
        Kind::AnchorPlace => 5,
    };
    for (i, c) in candidates.iter().copied().enumerate() {
        if !c.valid() {
            continue;
        }
        let at = slot(c.kind);
        if best[at].is_none_or(|old| {
            c.value() > candidates[old].value()
                || c.value() == candidates[old].value() && c.ready_in < candidates[old].ready_in
        }) {
            best[at] = Some(i);
        }
    }
    match (best[0], best[1]) {
        (Some(crystal), Some(anchor)) => {
            let (c, a) = (candidates[crystal], candidates[anchor]);
            Some(if a.value() > c.value() + 1.0 || a.lethal() && !c.lethal() { anchor } else { crystal })
        }
        (Some(i), None) | (None, Some(i)) => Some(i),
        (None, None) => best[2..].iter().copied().flatten().next(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stage {
    pub immediate_crystal: bool,
    pub opening_crystal: bool,
    pub crystal_receipt_pending: bool,
    pub existing_anchor_now: bool,
    pub existing_anchor_opening: bool,
}
impl Stage {
    pub(super) fn new_crystal(self) -> bool {
        !(self.immediate_crystal || self.opening_crystal || self.crystal_receipt_pending)
    }
    pub(super) fn new_anchor(self) -> bool {
        !(self.existing_anchor_now || self.existing_anchor_opening)
    }
}

pub(super) fn opening_horizon(action_steps: u32, opens_in: u32) -> u32 {
    action_steps.max(opens_in)
}

// A block can be placeable but its future crystal can be outside attack reach.
// Check the entity shape, not its center or the clicked block top.
pub(super) fn future_crystal_breakable(eye: [f64; 3], base: [i32; 3], entity_range: f64) -> bool {
    if !eye.iter().all(|n| n.is_finite()) || !entity_range.is_finite() || entity_range <= 0.1 {
        return false;
    }
    let min = [f64::from(base[0]) - 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) - 0.5];
    let max = [min[0] + 2.0, min[1] + 2.0, min[2] + 2.0];
    let squared = (0..3)
        .map(|i| {
            let n = eye[i].clamp(min[i], max[i]) - eye[i];
            n * n
        })
        .sum::<f64>();
    squared <= (entity_range - 0.1).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(kind: Kind, target_damage: f64) -> Rating {
        Rating {
            kind,
            target_damage,
            target_health: 20.0,
            self_damage: 0.0,
            held_totem: false,
            ready_in: 0,
            look_credit: 0.0,
            full_damage: target_damage,
        }
    }
    #[test]
    fn setup_advances_existing_cycle_before_new_cycle() {
        assert_eq!(select(&[c(Kind::AnchorPlace, 19.0), c(Kind::CrystalPlace, 15.0), c(Kind::Charge, 6.0)]), Some(2));
    }
    #[test]
    fn crystal_wins_small_anchor_improvement_and_equal_margin() {
        assert_eq!(select(&[c(Kind::CrystalHit, 8.0), c(Kind::AnchorHit, 9.0)]), Some(0));
        assert_eq!(select(&[c(Kind::CrystalHit, 8.0), c(Kind::AnchorHit, 9.01)]), Some(1));
    }
    #[test]
    fn apparent_lethal_anchor_beats_nonlethal_crystal() {
        let mut anchor = c(Kind::AnchorHit, 3.0);
        anchor.target_health = 3.0;
        assert_eq!(select(&[c(Kind::CrystalHit, 19.0), anchor]), Some(1));
    }
    #[test]
    fn observed_totem_pop_and_kill_have_different_priority() {
        let kill = c(Kind::CrystalHit, 20.0);
        let mut pop = kill;
        pop.held_totem = true;
        assert_eq!(select(&[pop, kill]), Some(1));
    }
    #[test]
    fn readiness_breaks_only_equal_within_kind() {
        let mut a = c(Kind::CrystalPlace, 8.0);
        a.ready_in = 3;
        let mut b = a;
        b.ready_in = 1;
        assert_eq!(select(&[a, b, c(Kind::AnchorPlace, 19.0)]), Some(1));
    }
    #[test]
    fn rejects_nonfinite_and_negative_scores_inputs() {
        assert_eq!(select(&[c(Kind::CrystalHit, f64::NAN), c(Kind::AnchorHit, -1.0)]), None);
    }
    #[test]
    fn pending_or_opening_crystal_blocks_another_placement() {
        assert!(Stage::default().new_crystal());
        assert!(!Stage { opening_crystal: true, ..Stage::default() }.new_crystal());
        assert!(!Stage { crystal_receipt_pending: true, ..Stage::default() }.new_crystal());
    }
    #[test]
    fn waiting_existing_anchor_blocks_fresh_anchor() {
        assert!(!Stage { existing_anchor_opening: true, ..Stage::default() }.new_anchor());
        assert!(Stage::default().new_anchor());
        assert_eq!(opening_horizon(2, 8), 8);
    }
    #[test]
    fn placement_reach_alone_does_not_make_crystal_breakable() {
        assert!(!future_crystal_breakable([0.0, 2.0, 0.5], [4, 0, 0], 3.0));
        assert!(future_crystal_breakable([0.0, 2.0, 0.5], [3, 0, 0], 3.0));
    }
    #[test]
    fn box_boundary_is_used_rather_than_crystal_center() {
        assert!(future_crystal_breakable([0.0, 2.0, 0.5], [3, 0, 0], 3.0));
        assert!(!future_crystal_breakable([f64::INFINITY, 2.0, 0.5], [3, 0, 0], 3.0));
    }
}

pub(super) fn prefer_first(first: Rating, second: Rating, _tie_first: bool) -> bool {
    select(&[first, second]) == Some(0)
}
