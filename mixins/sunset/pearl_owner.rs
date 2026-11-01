// Restore stock Projectile.recreateFromPacket owner metadata after a native
// accepted pearl spawn. AddEntity.data is the raw entity ID, not ID plus one.
use crate::game::Game;
use horizon_client::{mobs::Who, projectiles::ProjectileKind};

pub(super) fn accepted_spawn(
    game: &mut Game<'_>,
    accepted: bool,
    id: i32,
    kind: &str,
    data: i32,
) -> bool {
    if !game.native_window() || !game.native_on(super::sunset::ID) {
        return accepted;
    }
    if !accepted || kind != "minecraft:ender_pearl" || id < 0 || data < 0 {
        return accepted;
    }
    let Some(remote) = game.remote.as_ref() else {
        return accepted;
    };
    let owner = if remote.entity_id == data && game.player.is_some() {
        Some(Who::Player)
    } else if game.mobs.iter().any(|mob| mob.id == data as u32 && !mob.removed) {
        Some(Who::Mob(data as u32))
    } else {
        None
    };
    if let Some(owner) = owner
        && let Some(pearl) = game.creatures.projectiles.iter_mut().find(|pearl| {
            pearl.id == id as u32 && matches!(pearl.kind, ProjectileKind::EnderPearl)
        })
    {
        pearl.owner = Some(owner);
    }
    accepted
}
