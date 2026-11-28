// Read the native prediction handler rather than treating client-predicted air as a confirmed break.
use crate::game::Game;

// Methods on the game, as an extension trait: a support module can't add inherent methods to the game's own type.
pub(crate) trait SunsetBaritone {
    fn sunset_baritone_tool_strength(&self, state_id: u32, slot: usize) -> Option<(f64, bool)>;
    fn sunset_baritone_click_counts(&self) -> [u32; 3];
    fn sunset_baritone_cancel_clicks(&mut self, clicks: [u32; 3]);
    fn sunset_baritone_observation( &self, pos: [i32; 3], ) -> Option<(u32, Option<(i32, u32)>)>;
}

impl SunsetBaritone for Game<'_> {
    // Read-only counterpart of the native mining formula. It never changes the held slot or dispatches mutable
    // native break-speed hooks; the actual dig still runs those hooks through Horizon's mapped input tick.
    #[allow(dead_code)] // Optional API v3 host read: the companion is not required to be installed.
    fn sunset_baritone_tool_strength(&self, state_id: u32, slot: usize) -> Option<(f64, bool)> {
        use horizon_items::mining::{DestroySpeedModifiers, destroy_progress, destroy_speed};
        use horizon_client::blocks::Fluid;
        let id = horizon_world::registry::BlockStateId(state_id);
        self.pack.assets.registry.state(id)?;
        if slot >= 9 { return None; }
        let facts = self.res.facts().get(id);
        let p = self.player.as_ref()?;
        let held = self.inventory.get(slot)?.as_ref();
        let items = self.items();
        let harvest = items.has_correct_tool_for_drops(held, facts.requires_tool, &facts.name);
        let speed = items.mining_speed(held, &facts.name);
        let attribute = |name, fallback| p.attributes.computed_value_of(name).unwrap_or(fallback) as f32;
        let mut efficiency = attribute("minecraft:mining_efficiency", 0.0);
        if slot != self.selected {
            let contribution = |stack: Option<&horizon_client::items::ItemStack>| -> f32 {
                stack.into_iter().flat_map(|s| self.stations().enchantments.attributes(s))
                    .filter(|(name, _, _, op)| name == "minecraft:mining_efficiency" && op == "add_value")
                    .map(|(_, _, amount, _)| amount as f32).sum()
            };
            efficiency = (efficiency - contribution(self.held()) + contribution(held)).max(0.0);
        }
        let under_water = self.eye_position().is_some_and(|eye| {
            let [x, y, z] = eye.map(|v| v.floor() as i32);
            let inside = |y| {
                let f = self.res.facts().get(self.world.block(x, y, z));
                if f.fluid != Fluid::Water { return false; }
                let above = self.res.facts().get(self.world.block(x, y + 1, z));
                let source = f.fluid_level == 8 && !f.fluid_falling;
                let height = if above.fluid == f.fluid || source && above.sturdy_full & 1 != 0 { 1.0 }
                    else { f64::from(f32::from(f.fluid_level) / 9.0) };
                eye[1] <= f64::from(y) + height
            };
            inside(y) || eye[1] == f64::from(y) && inside(y - 1)
        });
        let modifiers = DestroySpeedModifiers { mining_efficiency: efficiency,
            effects: self.potions.effects.mining_speed_factor(), block_break_speed: attribute("minecraft:block_break_speed", 1.0),
            submerged_mining_speed: under_water.then(|| self.submerged_mining_speed()), on_ground: p.ground.on_ground };
        let progress = if facts.hardness == 0.0 { 1.0 } else { destroy_progress(facts.hardness, harvest, destroy_speed(speed, &modifiers)) };
        Some((f64::from(progress.max(0.0)), harvest))
    }

    fn sunset_baritone_click_counts(&self) -> [u32; 3] {
        self.remote.as_ref().map_or([self.attack_clicks.count(), self.use_clicks.count(), 0], |r|
            crate::game::sunset_owned_clicks::counts(&r.input))
    }

    fn sunset_baritone_cancel_clicks(&mut self, clicks: [u32; 3]) {
        if let Some(r) = self.remote.as_mut() {
            crate::game::sunset_owned_clicks::cancel(&mut r.input, clicks);
        } else {
            crate::game::sunset_owned_clicks::trim(&mut self.attack_clicks, clicks[0]);
            crate::game::sunset_owned_clicks::trim(&mut self.use_clicks, clicks[1]);
        }
    }

    fn sunset_baritone_observation(
        &self, pos: [i32; 3],
    ) -> Option<(u32, Option<(i32, u32)>)> {
        if !self.world.loaded(pos[0], pos[2]) { return None; }
        let effective = self.world.block(pos[0], pos[1], pos[2]).0;
        let predicted = self.remote.as_ref().and_then(|r| r.predictions.get(&pos))
            .map(|p| (p.sequence, p.state as u32));
        Some((effective, predicted))
    }
}
