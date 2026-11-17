// Hand View: the totem pop sized and moved on the screen.
fn sunset_totem_pose(value: Mat4, aspect: f32) -> Mat4 {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    let Some(pop) = ::sunset_core::hand_view::TOTEM.read().ok().and_then(|t| *t) else {
        return value;
    };
    let [x, y] = ::sunset_core::hand_view::totem_shift(pop, -value.w_axis.z, aspect);
    Mat4::from_translation(glam::Vec3::new(x, y, 0.0)) * value * Mat4::from_scale(glam::Vec3::splat(pop.scale))
}
