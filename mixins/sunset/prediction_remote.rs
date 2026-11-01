// What prediction.rs reads off the connection: equipment changes and which entity kind is a player.
use crate::game::Game;
use horizon_core::geometry::{Aabb, Vec3};
#[derive(Clone, Copy, Debug)]
pub(crate) struct RemoteMotion {
    pub rendered: Vec3,
    pub server: Vec3,
    pub bounds: Aabb,
    pub update_interval: u32,
    pub floating: bool,
}

pub(crate) fn is_player(game: &Game<'_>, kind: i32) -> bool {
    game.remote.as_ref().and_then(|r| r.builtin.get("minecraft:entity_type")).and_then(|registry| registry.name(kind))
        == Some("minecraft:player")
}
pub(crate) fn equipment(
    game: &Game<'_>,
    packet: &horizon_protocol::packets::play::Clientbound,
) -> Option<(u32, Vec<(usize, Option<horizon_client::items::ItemStack>)>)> {
    let horizon_protocol::packets::play::Clientbound::SetEquipment(p) = packet else {
        return None;
    };
    Some((
        u32::try_from(p.id).ok()?,
        p.slots
            .iter()
            .filter(|(slot, _)| *slot < 2)
            .map(|(slot, stack)| (usize::from(*slot), game.to_stack(stack)))
            .collect(),
    ))
}
pub(crate) fn motion(game: &Game<'_>, id: u32) -> Option<RemoteMotion> {
    let remote = game.remote.as_ref()?;
    let mob = game.mobs.iter().find(|m| m.id == id)?;
    let tracked = remote.entities.get(&i32::try_from(id).ok()?)?;
    let [x, y, z] = tracked.position;
    let server = Vec3::new(x, y, z);
    let interval = horizon_client::lan::tracking::settings(&mob.kind.id).1;
    if interval == 0 || !server.is_finite() {
        return None;
    }
    Some(RemoteMotion {
        rendered: mob.position,
        server,
        bounds: mob.bounds(),
        update_interval: interval,
        floating: mob.extra.fall_flying
            || mob.fluids.in_water
            || mob.fluids.in_lava
            || mob.climbing
            || mob.no_gravity
            || mob.extra.mount.is_some(),
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalMotion {
    pub feet: Vec3,
    pub previous: Vec3,
    pub bounds: Aabb,
    pub floating: bool,
}
/// The native previous-tick field is old_position, NOT the most recent movement
/// record (multiple records may belong to one tick).
pub(crate) fn local_motion(game: &Game<'_>) -> Option<LocalMotion> {
    use horizon_core::position::BlockPos;
    use horizon_world::{
        climbing::{self, ClimbingWorld},
        registry::BlockStateId,
    };
    struct ReadClimb<'a, 'g>(&'a Game<'g>);
    impl ClimbingWorld for ReadClimb<'_, '_> {
        type Error = ();
        fn is_spectator(&mut self) -> Result<bool, ()> {
            Ok(self.0.player.as_ref().ok_or(())?.spectator)
        }
        fn block_position(&mut self) -> Result<BlockPos, ()> {
            Ok(BlockPos::containing(self.0.player.as_ref().ok_or(())?.position))
        }
        fn in_block_state(&mut self) -> Result<BlockStateId, ()> {
            let p = self.block_position()?;
            self.block_state(p)
        }
        fn is_fall_flying(&mut self) -> Result<bool, ()> {
            Ok(self.0.player.as_ref().ok_or(())?.fall_flying)
        }
        fn block_state(&mut self, p: BlockPos) -> Result<BlockStateId, ()> {
            if !self.0.world.loaded(p.x, p.z) {
                return Err(());
            }
            Ok(self.0.world.block(p.x, p.y, p.z))
        }
        fn set_last_climbable_position(&mut self, _: BlockPos) {} // Read-only forecast.
    }
    let p = game.player.as_ref()?;
    let climbing =
        if p.flying { false } else { climbing::on_climbable(&mut ReadClimb(game), &game.motion.climbing).ok()? };
    Some(LocalMotion {
        feet: p.position,
        previous: p.old_position,
        bounds: p.bounds(),
        // Native local LivingMotionWorld::no_gravity is always false.
        floating: p.fall_flying || p.fluids.was_touching_water || p.is_in_lava() || climbing || game.riding.is_some(),
    })
}
