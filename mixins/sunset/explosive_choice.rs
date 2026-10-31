// World-independent scheduling for two already safety-checked opportunities.
// Health is the observed health value, not proof of a kill or totem pop.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rating {
    pub target_damage: f64,
    pub target_health: f64,
    pub self_damage: f64,
    pub attack: bool,
}

impl Rating {
    pub(super) fn valid(self) -> bool {
        [self.target_damage, self.target_health, self.self_damage].iter()
            .all(|n| n.is_finite() && *n >= 0.0) && self.target_health > 0.0
    }
    fn value(self) -> f64 {
        self.target_damage.min(self.target_health) - self.self_damage * 0.5
    }
}

pub(super) fn prefer_first(first: Rating, second: Rating, tie_first: bool) -> bool {
    match (first.valid(), second.valid()) {
        (false, _) => return false,
        (true, false) => return true,
        _ => {}
    }
    // A ready hit takes precedence over spending a turn on setup. Otherwise
    // prefer a possible finish, then useful damage relative to self damage.
    if first.attack != second.attack { return first.attack; }
    let finish = |r: Rating| r.target_damage >= r.target_health;
    if finish(first) != finish(second) { return finish(first); }
    match first.value().total_cmp(&second.value()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => tie_first,
    }
}
