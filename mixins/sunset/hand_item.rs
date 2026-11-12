// Hand View: an item held in first person, moved, turned and sized, swung (and a sword blocked) as its Animations say,
// 1.9+ or 1.8, and lit at full brightness with No hand shadow.
fn sunset_hand_item(this: &Game<'_>, state: &ItemRenderState, pose: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

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
    let game_pose = pose;
    let main = hand == horizon_client::first_person::Hand::Main;
    // A flat item with the game's own display (a resource pack's own keeps the game's pose), and whether Horizon's
    // Combat feel Legacy already drew it the 1.8.9 way (blocking while use is held with a sword, as it does).
    let flat = !state.layers.is_empty()
        && state.layers.iter().all(|l| horizon_client::first_person::is_modern_flat_display(&l.matrix, left));
    let native = this.settings.extras.pvp.combat.legacy && flat;
    let native_block = native && main && this.right && this.screen == super::Screen::Playing
        && stack.is_some_and(|s| s.item.ends_with("_sword"));
    // A blocking sword (Kill Aura's Legacy AutoBlock, Block on right click, or Horizon's own block): the main hand's. It
    // goes into the block at once, as 1.8's did (a blend between the two poses turned the sword a long way round).
    let block = main && sword && (view::SWORD_BLOCK.load(std::sync::atomic::Ordering::Relaxed) || native_block);
    let time = (this.ticks as f64 + this.clock.camera_fraction()) as f32 / 20.0;
    // The swing Hand View's animations play: its own clock for the main hand (a swing plays out before the next), the
    // game's for the other.
    let s = if main && this.swing.attack_hand() == hand {
        view::swing_clock(this.swings_started as u64, this.swing.current.map_or(6, |c| c.duration_ticks), time)
    } else {
        attack
    };
    let how = if native && !left { view::Drawn::Legacy { block: native_block } } else { view::Drawn::Modern };
    // The pose blocking or not: 1.8's for a flat item in the right main hand (hand_view_flags), else the 1.9+ swing
    // in the game's swing's place. Horizon's 1.8.9 pose of a left hand is left as it is.
    let shape = |block: bool| -> glam::Mat4 {
        let legacy = (plain && main && !left && flat)
            .then(|| view::legacy_pose(pose, attack, s, block, time, how, view::view_of(left)))
            .flatten();
        if let Some(legacy) = legacy {
            return legacy;
        }
        let swapped = match view::swing_swap(left, attack, s, sword, block).filter(|_| plain && !native) {
            Some(swap) => pose * swap,
            None => pose,
        };
        view::posed(left, swapped)
    };
    let posed = shape(block);
    let lit = view::hand_light(light);
    // Nothing changed from the game's own pose and light: the game draws it.
    if posed == game_pose && lit == light {
        return std::ops::ControlFlow::Continue(());
    }
    this.submit_item_lit(state, posed, lit, diffuse_lights(this.dim == Dim::Nether), out);
    std::ops::ControlFlow::Break(())
}
