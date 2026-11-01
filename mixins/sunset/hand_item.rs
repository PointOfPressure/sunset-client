// Hand View: an item held in first person, moved, turned and sized.
fn sunset_hand_item(this: &Game<'_>, state: &ItemRenderState, pose: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    let left = match state.display {
        DisplayContext::FirstPersonLeftHand => true,
        DisplayContext::FirstPersonRightHand => false,
        _ => return std::ops::ControlFlow::Continue(()),
    };
    let posed = horizon_client::first_person::sunset_hand_view::posed(left, pose);
    if posed == pose {
        return std::ops::ControlFlow::Continue(());
    }
    this.submit_item_lit(state, posed, light, diffuse_lights(this.dim == Dim::Nether), out);
    std::ops::ControlFlow::Break(())
}
