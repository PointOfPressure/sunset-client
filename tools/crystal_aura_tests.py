"""Compile/test the production pure state directly, without the native client.

Run: python tools/crystal_aura_tests.py
No fixtures duplicate the state implementation; the marked module is extracted
from the production mixin into a temporary Rust test crate.
"""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "mixins/sunset/crystal_aura.rs").read_text(encoding="utf-8")
state = source.split("// BEGIN PURE CRYSTAL STATE\n", 1)[1].split("// END PURE CRYSTAL STATE", 1)[0]
tests = r'''
use state::{Action, Clock};
#[test]
fn attack_reach_uses_first_box_contact_not_crystal_feet() {
    let eye: [f64;3] = [0.5, 122.62, 0.5];
    let at: [f64;3] = [5.5, 121.0, 0.5];
    let feet_distance = (0..3).map(|i| (at[i]-eye[i]).powi(2)).sum::<f64>().sqrt();
    assert!(feet_distance > 4.5);
    let hit = state::ray_box(eye, [1.0, 0.0, 0.0], [4.5,121.0,-0.5], [6.5,123.0,1.5], 4.5).unwrap();
    assert_eq!(hit, [4.5,122.62,0.5]);
    let end = state::ray_end(eye, hit);
    assert!(end[0] < hit[0] && end[0] > hit[0] - 0.0001);
    assert_eq!(end[1], hit[1]);
}
#[test]
fn slab_intersection_rejects_misses_and_invalid_geometry() {
    let lo = [3.0,0.0,0.0]; let hi = [4.0,2.0,2.0];
    assert!(state::ray_box([0.0,1.0,1.0], [-1.0,0.0,0.0],lo,hi,5.0).is_none());
    assert!(state::ray_box([0.0,3.0,1.0], [1.0,0.0,0.0],lo,hi,5.0).is_none());
    assert!(state::ray_box([0.0,1.0,1.0], [1.0,0.0,0.0],lo,hi,2.9).is_none());
    assert!(state::ray_box([0.0,1.0,1.0], [f64::NAN,0.0,0.0],lo,hi,5.0).is_none());
    assert_eq!(state::ray_box([3.5,1.0,1.0], [1.0,0.0,0.0],lo,hi,5.0),Some([3.5,1.0,1.0]));
}
#[test]
fn independent_place_clock_does_not_delay_acknowledged_crystal_break() {
    let mut c = Clock::default();
    c.sent(Action::Place([0;3]),100,20);
    c.observe(101,true);
    assert!(!c.due(Action::Place([0;3]),101));
    assert!(c.due(Action::Break(1,[0.0;3]),101));
    c.sent(Action::Break(1,[0.0;3]),101,2);
    c.observe(102,true);
    assert!(!c.due(Action::Break(1,[0.0;3]),102));
    assert!(c.due(Action::Break(1,[0.0;3]),103));
}
#[test]
fn score_caps_overkill_and_prefers_lower_self_damage_for_equal_target_value() {
    assert_eq!(state::candidate_score(90.0,20.0,4.0),1018.0);
    assert!(state::candidate_score(20.0,20.0,1.0) > state::candidate_score(90.0,20.0,4.0));
    assert!(state::candidate_score(19.0,20.0,0.0) < state::candidate_score(20.0,20.0,6.0));
    assert_eq!(state::candidate_score(f64::NAN,20.0,0.0),f64::NEG_INFINITY);
}
#[test]
fn placement_ray_stops_outside_top_face_without_hiding_the_base() {
    for y in [-64.0, -1.0, 0.0, 63.0, 317.0] {
        let top = [0.5, y + 1.0, 0.5];
        let end = state::place_ray_end(top);
        assert_eq!((end[0], end[2]), (top[0], top[2]));
        assert!(end[1] > top[1] && end[1] - top[1] < 1.0e-4);
        // An eye below the top on the neighboring side still crosses the
        // base's side before reaching the nudged endpoint: retain its shape.
        let eye = [0.5, y + 0.5, 1.5];
        let side_t = (1.0 - eye[2]) / (end[2] - eye[2]);
        let side_y = eye[1] + (end[1] - eye[1]) * side_t;
        assert!(side_y > y && side_y < top[1]);
    }
}
#[test]
fn placement_waits_for_server_then_retains_cooldown() {
    let mut c = Clock::default();
    c.sent(Action::Place([1,2,3]), 100, 8);
    assert_eq!(c.waiting.unwrap().action, Action::Place([1,2,3]));
    assert_eq!(c.crystals, 0);
    assert!(!c.ready(101));
    c.observe(101, false);
    assert!(!c.ready(108), "cooldown alone must not allow duplicate placement");
    c.observe(109, true);
    assert!(c.ready(109));
}
#[test]
fn attack_retry_requires_timeout_and_cooldown() {
    let mut c = Clock::default();
    c.sent(Action::Break(42, [1.0,2.0,3.0]), 10, 2);
    c.observe(39, false);
    assert!(!c.ready(39));
    c.observe(40, false);
    assert!(c.ready(40));
}
#[test]
fn acknowledged_attack_does_not_bypass_minimum_packet_gap() {
    let mut c = Clock::default();
    c.sent(Action::Break(42, [0.0;3]), 10, 0);
    c.observe(10, true);
    assert!(!c.ready(11));
    assert!(c.ready(12));
}
#[test]
fn queued_rotation_blocks_duplicate_work_and_reset_invalidates_it() {
    let mut c = Clock::default();
    let before = c.generation;
    c.queued = true;
    assert!(!c.ready(500));
    c.reset();
    assert_ne!(c.generation, before);
    assert!(!c.queued);
    assert!(c.waiting.is_none());
    assert!(c.ready(0));
}
#[test]
fn bounded_scan_visits_each_cell_once_and_wraps() {
    let mut c = Clock::default();
    let mut cells = std::collections::HashSet::new();
    let first = c.cell(5);
    cells.insert(first);
    for _ in 1..1331 {
        let p = c.cell(5);
        assert!(p.iter().all(|n| (-5..=5).contains(n)));
        assert!(cells.insert(p));
    }
    assert_eq!(cells.len(), 1331);
    assert_eq!(c.cell(5), first);
}
#[test]
fn clock_rollbacks_fail_closed_until_new_deadline() {
    let mut c = Clock::default();
    c.sent(Action::Place([0;3]), 100, 8);
    c.observe(0, false);
    assert!(!c.ready(0));
    assert!(c.waiting.is_some());
}

#[test]fn setup_wait_blocks_all_actions_until_payload_and_world_ack_or_timeout(){
 let mut c=Clock::default();c.sent(Action::Base([4,120,0]),10,20);
 assert!(!c.due(Action::Place([3,120,0]),11));assert!(!c.due(Action::Break(1,[0.0;3]),11));
 c.observe(39,false);assert!(!c.available());c.observe(40,false);assert!(c.available());
}
#[test]fn acknowledged_base_retains_setup_clock_without_delaying_ready_break(){
 let mut c=Clock::default();c.sent(Action::Base([4,120,0]),10,20);c.observe(11,true);
 assert!(!c.due(Action::Base([5,120,0]),29));assert!(c.due(Action::Base([5,120,0]),30));
 assert!(c.due(Action::Break(1,[0.0;3]),11));assert!(c.due(Action::Place([3,120,0]),11));
}
#[test]fn setup_reset_discards_pending_and_invalidates_accepted_generation(){
 let mut c=Clock::default();let old=c.generation;c.sent(Action::Base([4,120,0]),10,2);
 c.reset();assert!(c.waiting.is_none());assert_ne!(old,c.generation);assert!(c.available());
}

'''
with tempfile.TemporaryDirectory(prefix="sunset-crystal-tests-") as tmp:
    test_path = Path(tmp) / "crystal_state.rs"
    exe = Path(tmp) / "crystal_state_tests.exe"
    test_path.write_text(state + tests, encoding="utf-8")
    subprocess.run(["rustc", "--edition=2024", "--test", str(test_path), "-o", str(exe)], check=True)
    subprocess.run([str(exe)], check=True)
