#[path = "../mixins/sunset/target_order.rs"]
mod target_order;

#[test]
fn look_only_borrows_one_candidate_without_rescanning_the_population() {
    let population: Vec<_> = (0..10_000).collect();
    let viewed = target_order::candidates(&population, Some(9_999), true);
    assert_eq!(viewed, &[9_999]);
    assert_eq!(viewed.as_ptr(), population[9_999..].as_ptr());
    assert!(target_order::candidates(&population, None, true).is_empty());
    assert!(target_order::candidates(&population, Some(10_000), true).is_empty());
    assert_eq!(target_order::candidates(&population, None, false).as_ptr(), population.as_ptr());
    assert_eq!(target_order::candidates(&population, None, false).len(), 10_000);
}

#[test]
fn selection_matches_exhaustive_visibility_with_ties_and_occlusion() {
    let mut seed = 17u64;
    for count in 0..=256 {
        let candidates: Vec<_> = (0..count).map(|id| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed % 25) as f64, (id, seed & 7 != 0))
        }).collect();
        let expected = candidates.iter().filter(|(_, (_, visible))| *visible)
            .min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, (id, _))| *id);
        let actual = target_order::closest_visible(candidates, |(_, visible)| *visible)
            .map(|(id, _)| id);
        assert_eq!(actual, expected, "population={count}");
    }
}

#[test]
fn farther_candidates_skip_expensive_visibility_but_blocked_minima_do_not() {
    let mut rays = 0;
    let selected = target_order::closest_visible((0..1000).map(|id| (id as f64, id)), |_| {
        rays += 1;
        true
    });
    assert_eq!(selected, Some(0));
    assert_eq!(rays, 1);
    let selected = target_order::closest_visible([(0.0, false), (1.0, true)], |visible| *visible);
    assert_eq!(selected, Some(true));
}
