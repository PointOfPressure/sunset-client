#[path = "../mixins/sunset/city_policy.rs"]
mod policy;

#[test]
fn roof_has_one_reachable_outside_scaffold_and_a_native_face_chain() {
    for (lo, hi, expected) in [
        ([3.2, 121.0, 0.2], [3.8, 122.8, 0.8], 10),
        ([-0.3, 121.0, -0.3], [0.3, 122.8, 0.3], 21),
    ] {
        let base = policy::trap_cells(lo, hi, true).unwrap();
        let cells = policy::trap_cells_from(lo, hi, true, [-4.0, 122.62, 0.5]).unwrap();
        assert_eq!(cells.len(), expected);
        assert_eq!(cells.len(), base.len() + 1);
        let extra: Vec<_> = cells
            .iter()
            .filter(|p| !base.contains(p))
            .copied()
            .collect();
        assert_eq!(extra.len(), 1);
        let scaffold = extra[0];
        assert_eq!(scaffold[1], 123);
        assert!(cells.contains(&[scaffold[0], 122, scaffold[2]]));
        // The added block stands outside the occupied footprint.
        assert!(
            (scaffold[0] as f64 + 1.0 <= lo[0] || scaffold[0] as f64 >= hi[0])
                || (scaffold[2] as f64 + 1.0 <= lo[2] || scaffold[2] as f64 >= hi[2])
        );
        let mut supported = vec![scaffold];
        for _ in 0..4 {
            for roof in base.iter().filter(|p| p[1] == 123) {
                if supported
                    .iter()
                    .any(|p| (p[0] - roof[0]).abs() + (p[2] - roof[2]).abs() == 1)
                    && !supported.contains(roof)
                {
                    supported.push(*roof);
                }
            }
        }
        assert!(
            base.iter()
                .filter(|p| p[1] == 123)
                .all(|roof| supported.contains(roof))
        );
    }
}

#[test]
fn roof_off_spends_no_scaffold_and_hidden_inward_faces_are_rejected() {
    let lo = [0.2, 64.0, 0.2];
    let hi = [0.8, 65.8, 0.8];
    assert_eq!(
        policy::trap_cells_from(lo, hi, false, [0.0, 65.0, 0.0]).unwrap(),
        policy::trap_cells(lo, hi, false).unwrap()
    );
    for (eye, hidden) in [
        ([-4.0, 65.6, 0.5], [-1, 66, 0]),
        ([4.0, 65.6, 0.5], [1, 66, 0]),
        ([0.5, 65.6, -4.0], [0, 66, -1]),
        ([0.5, 65.6, 4.0], [0, 66, 1]),
    ] {
        let cells = policy::trap_cells_from(lo, hi, true, eye).unwrap();
        let scaffold = *cells.last().unwrap();
        assert_ne!(scaffold, hidden);
        let normal = [-scaffold[0], 0, -scaffold[2]];
        assert!((0..3).any(|axis| normal[axis] != 0
            && (eye[axis]
                - (f64::from(scaffold[axis]) + if normal[axis] > 0 { 1.0 } else { 0.0 }))
                * f64::from(normal[axis])
                > 0.0));
    }
    assert!(policy::trap_cells_from(lo, hi, true, [f64::NAN; 3]).is_none());
}

#[test]
fn roof_jump_requests_have_cooldown_and_a_two_request_ceiling() {
    let mut jumps = policy::RoofJumps::default();
    assert!(jumps.request(10));
    assert!(!jumps.request(10));
    assert!(!jumps.request(29));
    assert!(jumps.request(30));
    assert!(!jumps.request(100));
    jumps.clear();
    assert!(jumps.request(5));
    assert!(!jumps.request(4));
}

#[test]
fn trap_order_keeps_far_floor_and_head_faces_ahead_of_the_occluding_near_column() {
    let eye = [0.5, 122.62, 0.5];
    let mut cells = policy::trap_cells_from([3.2,121.0,0.2],[3.8,122.8,0.8],true,eye).unwrap();
    policy::order_trap(&mut cells, eye);
    let index = |cell| cells.iter().position(|p| *p == cell).unwrap();
    assert!(index([4,121,0]) < index([2,121,0]));
    assert!(index([4,122,0]) < index([2,122,0]));
    assert!(index([2,121,0]) < index([4,122,0]));
    assert!(cells.windows(2).all(|pair| pair[0][1] <= pair[1][1]));
    // Removing only observed ACKs preserves priority for the remaining cells.
    let remaining = cells[1..].to_vec();
    let mut next = remaining.clone();
    policy::order_trap(&mut next, eye);
    assert_eq!(next, remaining);
}
