// Hand View: the bare arm, moved, turned and sized, and lit at full brightness with No hand shadow (drawn again
// with the new pose and light, once).
fn sunset_arm_pose(this: &Game<'_>, left: bool, m: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use horizon_client::first_person::sunset_hand_view as view;
    if view::posing() {
        return std::ops::ControlFlow::Continue(());
    }
    let posed = view::posed(left, m);
    let lit = view::hand_light(light);
    if posed == m && lit == light {
        return std::ops::ControlFlow::Continue(());
    }
    view::set_posing(true);
    this.first_person_arm(left, posed, lit, out);
    view::set_posing(false);
    std::ops::ControlFlow::Break(())
}
