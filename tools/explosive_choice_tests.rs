#[path = "../mixins/sunset/explosive_choice.rs"]
mod choice;
use choice::{Rating, prefer_first};

fn r(damage: f64, health: f64, own: f64, attack: bool) -> Rating {
    Rating { target_damage: damage, target_health: health, self_damage: own, attack }
}

#[test]
fn shared_turn_prefers_a_ready_hit_over_more_expensive_setup() {
    assert!(prefer_first(r(8.0, 20.0, 2.0, true), r(20.0, 20.0, 0.0, false), false));
    assert!(!prefer_first(r(20.0, 20.0, 0.0, false), r(8.0, 20.0, 2.0, true), true));
}
#[test]
fn estimated_finish_then_useful_damage_and_self_cost_break_ties() {
    assert!(prefer_first(r(6.0, 5.0, 2.0, true), r(12.0, 20.0, 0.0, true), false));
    assert!(prefer_first(r(8.0, 20.0, 2.0, true), r(8.0, 20.0, 4.0, true), false));
    assert!(prefer_first(r(8.0, 20.0, 2.0, true), r(8.0, 20.0, 2.0, true), true));
    assert!(!prefer_first(r(8.0, 20.0, 2.0, true), r(8.0, 20.0, 2.0, true), false));
}
#[test]
fn bad_ratings_cannot_win_the_shared_action_slot() {
    let good = r(8.0, 20.0, 2.0, true);
    for bad in [r(f64::NAN, 20.0, 2.0, true), r(8.0, 0.0, 2.0, true), r(8.0, 20.0, -1.0, true)] {
        assert!(!bad.valid());
        assert!(!prefer_first(bad, good, true));
        assert!(prefer_first(good, bad, false));
    }
}
