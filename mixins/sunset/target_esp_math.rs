// Target ESP's marker on screen (fixed-size corner brackets) and which targets it marks.

// Unit ray and bounded distance for the native block-outline clipper. Invalid
// or zero-length geometry must never enter voxel traversal.
pub(super) fn sightline(eye: [f64; 3], target: [f64; 3], range: f64) -> Option<([f64; 3], f64)> {
    if !eye
        .iter()
        .chain(target.iter())
        .chain([range].iter())
        .all(|v| v.is_finite())
        || range <= 0.0
    {
        return None;
    }
    let delta = std::array::from_fn::<_, 3, _>(|i| target[i] - eye[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    (distance.is_finite() && distance > 0.001 && distance <= range)
        .then(|| (delta.map(|v| v / distance), distance))
}

pub(super) fn eligible(
    id: u32,
    own: Option<u32>,
    health: f32,
    removed: bool,
    invisible: bool,
    player: bool,
    players_only: bool,
) -> bool {
    Some(id) != own
        && health.is_finite()
        && health > 0.0
        && !removed
        && !invisible
        && (!players_only || player)
}

// Bounds are [left, top, right, bottom]. The arms stay at the actual projected
// corners: clipping must not turn an offscreen target into a screen-edge box.
pub(super) fn brackets(
    bounds: [f32; 4],
    thickness: f32,
    padding: f32,
    length: f32,
) -> Option<[[f32; 4]; 8]> {
    if !bounds
        .iter()
        .chain([thickness, padding, length].iter())
        .all(|v| v.is_finite())
        || thickness <= 0.0
        || padding < 0.0
        || length <= 0.0
        || bounds[2] - bounds[0] < 2.0
        || bounds[3] - bounds[1] < 2.0
    {
        return None;
    }
    let [l, t, r, b] = [
        bounds[0] - padding,
        bounds[1] - padding,
        bounds[2] + padding,
        bounds[3] + padding,
    ];
    let thick = thickness.min((r - l) / 4.0).min((b - t) / 4.0);
    let arm = length.min((r - l) / 4.0).min((b - t) / 4.0).max(thick);
    Some([
        [l, t, arm, thick],
        [l, t + thick, thick, arm - thick],
        [r - arm, t, arm, thick],
        [r - thick, t + thick, thick, arm - thick],
        [l, b - thick, arm, thick],
        [l, b - arm, thick, arm - thick],
        [r - arm, b - thick, arm, thick],
        [r - thick, b - arm, thick, arm - thick],
    ])
}

pub(super) fn clip(rect: [f32; 4], viewport: [f32; 2], grow: f32) -> Option<[f32; 4]> {
    if !rect
        .iter()
        .chain(viewport.iter())
        .chain([grow].iter())
        .all(|v| v.is_finite())
        || viewport[0] <= 0.0
        || viewport[1] <= 0.0
        || rect[2] <= 0.0
        || rect[3] <= 0.0
        || grow < 0.0
    {
        return None;
    }
    let l = (rect[0] - grow).max(0.0);
    let t = (rect[1] - grow).max(0.0);
    let r = (rect[0] + rect[2] + grow).min(viewport[0]);
    let b = (rect[1] + rect[3] + grow).min(viewport[1]);
    (r > l && b > t).then_some([l, t, r - l, b - t])
}
