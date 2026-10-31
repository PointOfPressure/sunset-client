#[path = "../mixins/sunset/city_policy.rs"]
mod policy;
use policy::*;

#[test]
fn single_footprint_has_complete_feet_head_and_roof() {
    let cells = trap_cells([0.2, 64.0, 0.2], [0.8, 65.8, 0.8], true).unwrap();
    assert_eq!(cells.len(), 9);
    for y in [64, 65] {
        for side in SIDES {
            assert!(cells.contains(&[side[0], y, side[2]]));
        }
    }
    assert!(cells.contains(&[0, 66, 0]));
    assert!(!cells.contains(&[0, 64, 0]));
}
#[test]
fn straddling_footprint_has_no_interior_or_duplicate_cells() {
    let cells = trap_cells([-0.3, 64.0, -0.3], [0.3, 65.8, 0.3], true).unwrap();
    assert_eq!(cells.len(), 20);
    for (i, cell) in cells.iter().enumerate() {
        assert!(!cells[..i].contains(cell));
        if cell[1] < 66 {
            assert!(!(cell[0] >= -1 && cell[0] <= 0 && cell[2] >= -1 && cell[2] <= 0));
        }
    }
    assert_eq!(
        trap_cells([-0.3, 64.0, -0.3], [0.3, 65.8, 0.3], false)
            .unwrap()
            .len(),
        16
    );
}
#[test]
fn geometry_rejects_invalid_and_unbounded_input() {
    for value in [f64::NAN, f64::INFINITY, 30_000_001.0] {
        assert!(trap_cells([value, 0.0, 0.0], [value, 1.8, 0.6], true).is_none());
    }
    assert!(trap_cells([0.0, 64.0, 0.0], [3.0, 65.8, 0.6], true).is_none());
    assert!(trap_cells([0.2, -65.0, 0.2], [0.8, -63.2, 0.8], true).is_none());
    assert!(trap_cells([0.2, 318.0, 0.2], [0.8, 319.8, 0.8], true).is_none());
    assert!(trap_cells([0.2, -64.0, 0.2], [0.8, -62.2, 0.8], true).is_some());
    assert!(trap_cells([0.2, 317.0, 0.2], [0.8, 318.8, 0.8], true).is_some());
    assert!(box_distance([0.0; 3], [1.0; 3], [0.0; 3]).is_none());
    assert_eq!(
        box_distance([0.0; 3], [1.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        Some(1.0)
    );
    assert!(near([0.0; 3], [3.0, 4.0, 0.0], 5.0));
    assert!(!near([0.0; 3], [3.0, 4.0, 0.0], 4.99));
}
#[test]
fn placement_waits_for_payload_or_timeout_and_caps_attempts() {
    let mut ledger = Ledger::default();
    let p = [1, 64, 0];
    assert!(ledger.sent(p, 0, 10));
    assert_eq!(ledger.waiting(), 1);
    assert!(!ledger.sent(p, 0, 11));
    ledger.expire(39);
    assert!(!ledger.can_try(p, 0));
    ledger.expire(40);
    assert!(ledger.sent(p, 0, 40));
    ledger.observe(p);
    assert_eq!(ledger.waiting(), 0);
    assert!(ledger.sent(p, 0, 41));
    ledger.observe(p);
    assert!(!ledger.can_try(p, 0));
    assert!(ledger.can_try(p, 1));
    assert!(ledger.sent(p, 1, 42));
    ledger.clear();
    assert!(ledger.can_try(p, 0));
}
#[test]
fn placement_ledger_has_fixed_capacity() {
    let mut ledger = Ledger::default();
    for x in 0..64 {
        assert!(ledger.sent([x, 64, 0], 0, 1));
    }
    assert!(!ledger.sent([64, 64, 0], 0, 1));
    ledger.expire(31);
    assert!(ledger.sent([0, 64, 0], 0, 31));
    assert_eq!(ledger.waiting(), 1);
}
#[test]
fn damage_history_rechecks_old_hands_with_current_weakness() {
    let mut h = History::default();
    for tick in 0..=20 {
        h.sample(tick, Some(if tick == 0 { 1.0 } else { 8.0 }));
    }
    assert!(h.positive(20, 20, 0.0));
    assert!(!h.positive(20, 20, -4.0));
    h.sample(21, Some(8.0));
    assert!(h.positive(21, 20, -4.0));
    assert!(!h.positive(21, 20, -8.0));
    assert!(!h.positive(21, 20, f32::NAN));
}
#[test]
fn unknown_hand_warmup_rewind_and_slot_spam_cannot_fake_history() {
    let mut h = History::default();
    h.sample(10, Some(8.0));
    assert!(!h.positive(10, 10, 0.0));
    for tick in 11..=20 {
        h.sample(tick, Some(8.0));
    }
    assert!(h.positive(20, 10, 0.0));
    h.sample(20, None);
    assert!(!h.positive(20, 10, 0.0));
    for _ in 0..65 {
        h.sample(20, Some(8.0));
    }
    assert!(!h.positive(20, 10, 0.0));
    h.sample(1, Some(8.0));
    assert!(!h.positive(1, 10, 0.0));
}
#[test]
fn visual_mask_times_out_without_repeating_a_rejected_prediction() {
    let mut masks = Masks::default();
    let pos = [1.5, 64.0, 0.5];
    assert!(masks.add(7, pos, 1));
    assert!(masks.hidden(7, pos));
    assert_eq!(masks.at(pos), Some(7));
    assert!(!masks.add(7, pos, 2));
    masks.expire(20, 20);
    assert!(masks.hidden(7, pos));
    masks.expire(21, 20);
    assert!(!masks.hidden(7, pos));
    assert_eq!(masks.stubborn(), 1);
    assert!(!masks.add(7, pos, 22));
    assert!(!masks.confirm(7));
    assert!(masks.add(7, pos, 23));
    assert!(masks.confirm(7));
    assert_eq!(masks.count(), 0);
}
#[test]
fn visual_masks_and_stubborn_ids_are_bounded() {
    let mut m = Masks::default();
    for id in 1..=64 {
        assert!(m.add(id, [f64::from(id), 64.0, 0.0], 0));
    }
    assert!(!m.add(65, [0.0; 3], 0));
    m.expire(10, 10);
    assert_eq!(m.stubborn(), 64);
    for id in 65..=128 {
        assert!(m.add(id, [f64::from(id), 64.0, 0.0], 10));
    }
    m.expire(20, 10);
    assert_eq!(m.stubborn(), 128);
    assert!(m.add(129, [0.0; 3], 20));
    m.expire(30, 10);
    assert!(!m.add(130, [0.0; 3], 30));
    assert_eq!(m.count(), 0);
    m.clear();
    assert!(m.add(130, [0.0; 3], 31));
}
#[test]
fn section_updates_decode_negative_sections_and_reject_overflow() {
    let wire = (123_i64 << 12) | (15 << 8) | (7 << 4) | 3;
    assert_eq!(
        unpack_section([-2, -4, 3], wire),
        Some(([-17, -61, 55], 123))
    );
    assert!(unpack_section([i32::MAX, 0, 0], wire).is_none());
    assert!(unpack_section([0; 3], -1).is_none());
}
