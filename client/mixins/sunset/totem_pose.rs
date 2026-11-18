// Hand View: the totem pop sized and moved on the screen.
fn sunset_totem_pose(value: Mat4, aspect: f32) -> Mat4 {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    crate::first_person::sunset_hand_view::totem_pose(value, aspect)
}
