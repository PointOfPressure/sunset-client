// Looking at one entity needs no second walk over the population.
pub(super) fn candidates<T>(items: &[T], looked_at: Option<usize>, only_look: bool) -> &[T] {
    if only_look {
        looked_at.and_then(|i| items.get(i..=i)).unwrap_or(&[])
    } else {
        items
    }
}

// Candidates arrive in world order. Only a new angular minimum can replace
// the visible winner, so farther candidates need no collision raycast.
pub(super) fn closest_visible<T>(
    candidates: impl IntoIterator<Item = (f64, T)>,
    mut visible: impl FnMut(&T) -> bool,
) -> Option<T> {
    let mut best: Option<(f64, T)> = None;
    for (angle, target) in candidates {
        if best.as_ref().is_some_and(|(previous, _)| !angle.total_cmp(previous).is_lt()) {
            continue;
        }
        if visible(&target) {
            best = Some((angle, target));
        }
    }
    best.map(|(_, target)| target)
}
