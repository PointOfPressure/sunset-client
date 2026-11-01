// The look a tick turns toward a target (Rust's rotation step): a stride that grows with the distance and the speed, a
// little shorter at random, at most 25 degrees, in whole steps of the mouse.
pub fn rotation_step(base: [f32; 2], desired: [f32; 2], quantum: f64, speed: f64, random: f64) -> Option<[f32; 2]> {
    if !base.iter().chain(desired.iter()).all(|n| n.is_finite())
        || !quantum.is_finite()
        || quantum <= 0.
        || !speed.is_finite()
        || speed < 0.
        || !random.is_finite()
    {
        return None;
    }
    let yaw = f64::from((desired[0] - base[0] + 180.).rem_euclid(360.) - 180.);
    let pitch = f64::from(desired[1] - base[1]);
    let distance = yaw.hypot(pitch);
    let mut scale = 1.;
    if distance >= 1e-4 && speed > 0. {
        let mut stride = (0.313 * distance + 1.56 * speed).min(distance);
        if stride < distance {
            stride *= 0.85 + 0.15 * random.clamp(0., 1.);
        }
        scale = stride / distance;
    }
    let axis = yaw.abs().max(pitch.abs()) * scale;
    if axis > 25. {
        scale *= 25. / axis;
    }
    let cap = (25. / quantum).floor().max(1.);
    let counts = |delta: f64| ((delta * scale / quantum) + 0.5).floor().clamp(-cap, cap);
    Some([
        (f64::from(base[0]) + counts(yaw) * quantum) as f32,
        (f64::from(base[1]) + counts(pitch) * quantum).clamp(-90., 90.) as f32,
    ])
}
