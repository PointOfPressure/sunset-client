// Hand View: an item held in first person, moved, turned and sized, swung as Swing animation says, and lit at full
// brightness with No hand shadow.
fn sunset_hand_item(this: &Game<'_>, state: &ItemRenderState, pose: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    use horizon_client::first_person::sunset_hand_view as view;
    let left = match state.display {
        DisplayContext::FirstPersonLeftHand => true,
        DisplayContext::FirstPersonRightHand => false,
        _ => return std::ops::ControlFlow::Continue(()),
    };
    // Swing animation: only an item swung as the game swings it (not one in use, a crossbow, a spear, a stab or a spin).
    let hand = if left == this.settings.main_hand_left { horizon_client::first_person::Hand::Main } else { horizon_client::first_person::Hand::Off };
    let stack = if hand == horizon_client::first_person::Hand::Main { this.held() } else { this.offhand.as_ref() };
    let plain = this.using.is_none()
        && !this.player.as_ref().is_some_and(|p| p.auto_spin_attack_ticks > 0)
        && !this.swing.current.is_some_and(|c| c.animation.kind == 2)
        && stack.is_none_or(|s| &*s.item != "minecraft:crossbow" && horizon_client::spear::Kinetic::of(&s.item).is_none());
    let attack = if this.swing.attack_hand() == hand { this.swing.animation(this.clock.camera_fraction() as f32) } else { 0.0 };
    let sword = stack.is_some_and(|s| this.res.items().item_tags.get("minecraft:swords").is_some_and(|t| t.contains(&*s.item)));
    let pose = match view::swing_swap(left, attack, sword).filter(|_| plain) {
        Some(swap) => pose * swap,
        None => pose,
    };
    let posed = view::posed(left, pose);
    let lit = view::hand_light(light);
    if posed == pose && lit == light {
        return std::ops::ControlFlow::Continue(());
    }
    this.submit_item_lit(state, posed, lit, diffuse_lights(this.dim == Dim::Nether), out);
    std::ops::ControlFlow::Break(())
}
