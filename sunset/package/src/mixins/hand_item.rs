// Hand View: an item held in first person, moved, turned and sized, swung (and a sword blocked) as its Animations say,
// 1.9+ or 1.8, and lit at full brightness with No hand shadow.
fn sunset_hand_item(this: &Game<'_>, state: &ItemRenderState, pose: glam::Mat4, light: u32, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use crate::game::sunset_hand_view as view;
    let left = match state.display {
        DisplayContext::FirstPersonLeftHand => true,
        DisplayContext::FirstPersonRightHand => false,
        _ => return std::ops::ControlFlow::Continue(()),
    };
    // Swing animation: only an item swung as the game swings it (not a crossbow, a spear, a stab or a spin, nor one in
    // use but a blocking sword: `plain` below).
    let hand = if left == this.settings.main_hand_left { horizon_client::first_person::Hand::Main } else { horizon_client::first_person::Hand::Off };
    let stack = if hand == horizon_client::first_person::Hand::Main { this.held() } else { this.offhand.as_ref() };
    let swung = !this.player.as_ref().is_some_and(|p| p.auto_spin_attack_ticks > 0)
        && !this.swing.current.is_some_and(|c| c.animation.kind == 2)
        && stack.is_none_or(|s| &*s.item != "minecraft:crossbow" && horizon_client::spear::Kinetic::of(&s.item).is_none());
    let attack = if this.swing.attack_hand() == hand { this.swing.animation(this.clock.camera_fraction() as f32) } else { 0.0 };
    let sword = stack.is_some_and(|s| this.res.items().item_tags.get("minecraft:swords").is_some_and(|t| t.contains(&*s.item)));
    let game_pose = pose;
    let main = hand == horizon_client::first_person::Hand::Main;
    // The main hand's sword in a real use: a 1.8 server behind ViaVersion makes its swords blocking items, so right click
    // (or Kill Aura's AutoBlock) starts a use and the game turns the sword across the body, its own block. Where the
    // block is Hand View's to show, that is the same block as before the use began and between two hits, shown the
    // same way: the sword jumped between the game's block and Hand View's each time a use started or stopped.
    let used_block = main && sword && view::OWN_BLOCK.load(std::sync::atomic::Ordering::Relaxed)
        && this.using.is_some_and(|(slot, _)| slot != crate::game::sunset_util::OFFHAND);
    let plain = swung && (this.using.is_none() || used_block);
    // A flat item with the game's own display (a resource pack's own keeps the game's pose), and whether Horizon's
    // Combat feel Legacy already drew it the 1.8.9 way (blocking while use is held with a sword, as it does).
    let flat = !state.layers.is_empty()
        && state.layers.iter().all(|l| horizon_client::first_person::is_modern_flat_display(&l.matrix, left));
    let native = this.settings.extras.pvp.combat.legacy && flat;
    let native_block = native && main && this.right && this.screen == super::Screen::Playing
        && stack.is_some_and(|s| s.item.ends_with("_sword"));
    // A blocking sword (Kill Aura's Legacy AutoBlock, Block on right click, or Horizon's own block): the main hand's. It
    // goes into the block at once, as 1.8's did (a blend between the two poses turned the sword a long way round).
    let block = main && sword && (view::SWORD_BLOCK.load(std::sync::atomic::Ordering::Relaxed) || native_block || used_block);
    let time = (this.ticks as f64 + this.clock.camera_fraction()) as f32 / 20.0;
    // The swing Hand View's animations play: its own clock for the main hand (a swing plays out before the next), the
    // game's for the other.
    let s = if main && this.swing.attack_hand() == hand {
        view::swing_clock(this.swings_started as u64, this.swing.current.map_or(6, |c| c.duration_ticks), time)
    } else {
        attack
    };
    let how = match (native && !left, used_block) {
        (true, _) => view::Drawn::Legacy { block: native_block },
        (false, true) => view::Drawn::Block,
        (false, false) => view::Drawn::Modern,
    };
    // Combat feel Legacy draws a sword in use at rest (no swing) unless right click holds it in its block.
    let drawn_attack = if used_block && !native_block { 0.0 } else { attack };
    // The pose blocking or not: 1.8's for a flat item in the right main hand (hand_view_flags), else the 1.9+ swing
    // in the game's swing's place. Horizon's 1.8.9 pose of a left hand is left as it is.
    let shape = |block: bool| -> glam::Mat4 {
        let legacy = (plain && main && !left && flat)
            .then(|| view::legacy_pose(pose, drawn_attack, s, block, time, how, view::view_of(left)))
            .flatten();
        if let Some(legacy) = legacy {
            return legacy;
        }
        let swapped = match view::swing_swap(left, attack, s, sword, block, used_block).filter(|_| plain && !native) {
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
