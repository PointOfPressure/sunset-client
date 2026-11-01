#[path = "../mixins/sunset/packet_policy.rs"]
mod policy;
use policy::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Wire {
    Position(u8),
    OriginalEnd(u8),
    Action,
}
// Exact quota branch/reset independently verified from actual vanilla26.3
// ServerGamePacketListenerImpl bytecode by packet_engine_tests --server-jar.
fn vanilla_accepts(packets: &[Wire]) -> bool {
    let mut received_position = false;
    for packet in packets {
        match packet {
            Wire::Position(_) => {
                if received_position {
                    return false;
                }
                received_position = true;
            }
            Wire::OriginalEnd(_) => received_position = false,
            Wire::Action => {}
        }
    }
    true
}
#[test]
fn original_frame_order_prevents_the_reproduced_vanilla_disconnect() {
    let old = [
        Wire::OriginalEnd(0),
        Wire::OriginalEnd(1),
        Wire::Position(0),
        Wire::Position(1),
    ];
    assert!(!vanilla_accepts(&old));
    let original = [
        Wire::Position(0),
        Wire::OriginalEnd(0),
        Wire::Position(1),
        Wire::OriginalEnd(1),
    ];
    let mut queue = Queue::new(64, 4096, 1000);
    for (i, packet) in original.into_iter().enumerate() {
        queue.push(packet, i as u64, 500, 40).unwrap();
    }
    let wire = queue.drain(1000, true);
    assert_eq!(wire, original);
    assert!(vanilla_accepts(&wire));
}
#[test]
fn action_menu_disable_flushes_preserve_partial_frame_then_its_original_end() {
    for action in [false, true] {
        let mut queue = Queue::new(64, 4096, 1000);
        let partial = [Wire::Position(0), Wire::OriginalEnd(0), Wire::Position(1)];
        for packet in partial {
            queue.push(packet, 0, 500, 40).unwrap();
        }
        let mut wire = queue.drain(0, true);
        if action {
            wire.push(Wire::Action);
        }
        wire.extend([
            Wire::OriginalEnd(1),
            Wire::Position(2),
            Wire::OriginalEnd(2),
        ]);
        assert!(vanilla_accepts(&wire));
        assert_eq!(queue.released, 3);
    }
}
#[test]
fn correction_forwards_only_original_boundaries_before_new_position() {
    let mut queue = Queue::new(64, 4096, 1000);
    for packet in [
        Wire::OriginalEnd(9),
        Wire::Position(0),
        Wire::OriginalEnd(0),
        Wire::Position(1),
    ] {
        queue.push(packet, 0, 500, 40).unwrap();
    }
    let mut wire = vec![Wire::Position(9)];
    let boundaries = queue.discard_except(|p| matches!(p, Wire::OriginalEnd(_)));
    assert_eq!(boundaries, [Wire::OriginalEnd(9), Wire::OriginalEnd(0)]);
    wire.extend(boundaries);
    wire.extend([Wire::Action, Wire::Position(42), Wire::OriginalEnd(42)]);
    assert!(vanilla_accepts(&wire));
    assert!(!wire.contains(&Wire::Position(0)) && !wire.contains(&Wire::Position(1)));
    assert_eq!(
        (queue.released, queue.dropped, queue.bytes(), queue.len()),
        (2, 2, 0, 0)
    );
}
#[test]
fn disconnect_discards_boundaries_and_positions_and_both_consume_caps() {
    let mut queue = Queue::new(64, 4096, 1000);
    for tick in 0..32 {
        for packet in [Wire::Position(tick), Wire::OriginalEnd(tick)] {
            queue.push(packet, 0, 1000, 40).unwrap();
        }
    }
    assert_eq!(queue.len(), 64);
    assert_eq!(queue.bytes(), 2560);
    assert!(queue.push(Wire::OriginalEnd(32), 0, 1000, 40).is_err());
    queue.clear();
    assert_eq!(queue.dropped, 64);
    assert!(queue.drain(1000, true).is_empty());
    assert!(vanilla_accepts(&[
        Wire::Position(42),
        Wire::OriginalEnd(42)
    ]));
}

#[test]
fn same_connection_respawn_preserves_original_ends_but_new_connection_drops_all() {
    for same_connection in [true, false] {
        let mut moves = Queue::new(64, 4096, 1000);
        let mut impulses = Queue::new(16, 1000, 1000);
        for packet in [
            Wire::OriginalEnd(9),
            Wire::Position(10),
            Wire::OriginalEnd(10),
        ] {
            moves.push(packet, 0, 1000, 40).unwrap();
        }
        impulses.push("old-world-knockback", 0, 800, 24).unwrap();
        let mut wire = if same_connection {
            vec![Wire::Position(9)]
        } else {
            Vec::new()
        };
        if same_connection {
            wire.extend(moves.discard_except(|p| matches!(p, Wire::OriginalEnd(_))));
        } else {
            moves.clear();
        }
        impulses.clear();
        wire.extend([Wire::Action, Wire::Position(42), Wire::OriginalEnd(42)]);
        assert!(vanilla_accepts(&wire));
        assert!(!wire.contains(&Wire::Position(10)));
        assert!(impulses.drain(1000, true).is_empty());
        assert_eq!(
            wire.iter()
                .filter(|p| matches!(p, Wire::OriginalEnd(9) | Wire::OriginalEnd(10)))
                .count(),
            if same_connection { 2 } else { 0 }
        );
    }
}

#[test]
fn due_times_never_reorder_newer_impulses() {
    let mut q = Queue::new(16, 1000, 1000);
    q.push("first", 100, 250, 10).unwrap();
    q.push("second", 150, 0, 10).unwrap();
    assert!(q.drain(349, false).is_empty());
    assert_eq!(q.drain(350, false), ["first", "second"]);
    assert_eq!((q.accepted, q.released, q.bytes()), (2, 2, 0));
}
#[test]
fn count_and_byte_caps_are_absolute_and_fail_without_stealing_packets() {
    let mut q = Queue::new(2, 20, 1000);
    q.push(1, 0, 100, 10).unwrap();
    q.push(2, 0, 100, 10).unwrap();
    assert_eq!(q.push(3, 0, 100, 1), Err(3));
    assert_eq!(q.len(), 2);
    assert_eq!(q.drain(0, true), [1, 2]);
    assert_eq!(q.bytes(), 0);
    assert_eq!(q.push(4, 0, 0, 21), Err(4));
    assert!(q.is_empty());
    assert!(Queue::new(0, 20, 1000).push(1, 0, 0, 1).is_err());
}
#[test]
fn old_age_ceiling_does_not_release_younger_packets_before_their_deadline() {
    let mut q = Queue::new(64, 4096, 1000);
    q.push(1, 0, 1000, 1).unwrap();
    q.push(2, 999, 1000, 1).unwrap();
    assert_eq!(q.oldest_age(999), 999);
    assert_eq!(q.drain(1000, false), [1]);
    assert_eq!(q.oldest_age(1000), 1);
    assert_eq!(q.bytes(), 1);
    assert!(q.drain(1998, false).is_empty());
    assert_eq!(q.drain(1999, false), [2]);
}
#[test]
fn aged_idle_end_prefix_does_not_force_a_fresh_movement_batch_out_early() {
    let mut q = Queue::new(64, 4096, 1000);
    q.push(Wire::OriginalEnd(0), 0, 1000, 40).unwrap();
    q.push(Wire::Position(1), 900, 1000, 40).unwrap();
    q.push(Wire::OriginalEnd(1), 901, 1000, 40).unwrap();
    let mut wire = vec![Wire::Position(0)];
    // Production outgoing drains this prefix before needs_flush/push.
    wire.extend(q.drain(1000, false));
    assert!(!q.needs_flush(1000, 40));
    q.push(Wire::Position(2), 1000, 1000, 40).unwrap();
    q.push(Wire::OriginalEnd(2), 1001, 1000, 40).unwrap();
    assert_eq!(wire, [Wire::Position(0), Wire::OriginalEnd(0)]);
    assert!(q.drain(1899, false).is_empty());
    wire.extend(q.drain(1900, false));
    assert_eq!(wire.last(), Some(&Wire::Position(1)));
    wire.extend(q.drain(1901, false));
    assert_eq!(wire.last(), Some(&Wire::OriginalEnd(1)));
    wire.extend(q.drain(2001, false));
    assert!(vanilla_accepts(&wire));
    assert_eq!((q.len(), q.bytes()), (0, 0));
}
#[test]
fn clock_rewind_and_disconnect_clear_do_not_replay_into_a_new_epoch() {
    let mut q = Queue::new(16, 1000, 1000);
    q.push(1, 500, 100, 1).unwrap();
    assert_eq!(q.drain(499, false), [1]);
    q.push(2, 600, 100, 1).unwrap();
    q.clear();
    assert!(q.drain(u64::MAX, true).is_empty());
    assert_eq!(q.dropped, 1);
}
#[test]
fn correction_discards_queued_coordinates_without_forwarding_them() {
    let mut q = Queue::new(64, 4096, 1000);
    q.push([5.0, 70.0, 2.0], 0, 500, 40).unwrap();
    q.push([6.0, 70.0, 2.0], 50, 500, 40).unwrap();
    q.clear();
    assert!(q.drain(5000, true).is_empty());
    assert_eq!((q.released, q.dropped), (0, 2));
}
#[test]
fn delay_and_queue_limits_saturate_at_explicit_hard_ceilings() {
    let mut q = Queue::new(usize::MAX, usize::MAX, u64::MAX);
    for i in 0..64 {
        q.push(i, 10, u64::MAX, 1).unwrap();
    }
    assert!(q.push(99, 10, 1, 1).is_err());
    assert!(q.drain(1009, false).is_empty());
    assert_eq!(q.drain(1010, false).len(), 64);
    let mut q = Queue::new(1, 10, 1000);
    q.push(7, u64::MAX, u64::MAX, 1).unwrap();
    assert_eq!(q.drain(u64::MAX, false), [7]);
}
#[test]
fn chance_setting_has_exact_boundaries_and_no_nonfinite_enablement() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
        assert!(!(0..10_000).any(|n| probability(invalid, n)));
    }
    assert_eq!((0..10_000).filter(|n| probability(40.0, *n)).count(), 4000);
    assert_eq!(
        (0..10_000).filter(|n| probability(100.0, *n)).count(),
        10_000
    );
}
#[test]
fn reversed_delay_settings_are_inclusive_finite_and_bounded() {
    for n in 0..1000 {
        assert_eq!(
            uniform_between(100.0, 50.0, n),
            uniform_between(50.0, 100.0, n)
        );
        assert!((50..=100).contains(&uniform_between(100.0, 50.0, n)));
    }
    assert_eq!(uniform_between(10.0, 10.0, 1000), 10);
    assert_eq!(uniform_between(f64::NAN, f64::INFINITY, 12), 0);
    assert_eq!(bounded(99999.0, 10, 1000), 1000);
}
#[test]
fn dynamic_lag_requires_approach_aim_range_and_finite_vectors() {
    let eye = [0.0; 3];
    let look = [0.0, 0.0, 1.0];
    let towards = [0.0, 0.0, 0.1];
    assert!(approaching(eye, look, towards, [0.0, 0.0, 5.0], 5.0, 6.0));
    assert!(!approaching(
        eye,
        look,
        [0.0, 0.0, -0.1],
        [0.0, 0.0, 5.0],
        5.0,
        6.0
    ));
    assert!(!approaching(
        eye,
        look,
        [0.0, 0.0, 0.02],
        [0.0, 0.0, 5.0],
        5.0,
        6.0
    ));
    assert!(!approaching(eye, look, towards, [0.0, 0.0, 7.0], 7.0, 6.0));
    assert!(!approaching(eye, look, towards, [5.0, 0.0, 0.0], 5.0, 6.0));
    assert!(!approaching(
        eye,
        look,
        towards,
        [f64::NAN, 0.0, 0.0],
        5.0,
        6.0
    ));
    assert!(!approaching(eye, look, towards, eye, 0.0, 6.0));
    for angle in [29.9f64, 30.1] {
        let a = angle.to_radians();
        assert_eq!(
            approaching(
                eye,
                look,
                [0.0, 0.0, 0.1],
                [a.sin() * 5.0, 0.0, a.cos() * 5.0],
                5.0,
                6.0
            ),
            angle < 30.0
        );
    }
}
