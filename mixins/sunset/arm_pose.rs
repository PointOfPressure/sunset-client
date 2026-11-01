// Hand View: the bare arm, moved, turned and sized (drawn again with the new pose, once).
fn sunset_arm_pose(this: &Game<'_>, left: bool, m: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    use horizon_client::first_person::sunset_hand_view as view;
    let posed = view::posed(left, m);
    if posed == m || view::posing() {
        return std::ops::ControlFlow::Continue(());
    }
    view::set_posing(true);
    this.first_person_arm(left, posed, light, out);
    view::set_posing(false);
    std::ops::ControlFlow::Break(())
}
