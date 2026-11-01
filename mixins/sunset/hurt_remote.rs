// Draft registration: child of game/remote.rs. Reads real decoded packets and
// synchronized registries. It never guesses a damage type's numerical id.
use crate::game::Game;
use crate::game::sunset_hurt_policy::{DamageKind, DamageNotice};
use horizon_protocol::packets::play::Clientbound as C;

pub(crate) fn damage_notice(game: &Game<'_>, packet: &C,
    enemy: impl FnOnce(u32) -> bool) -> Option<DamageNotice>
{
    let C::DamageEvent(p) = packet else { return None; };
    let victim = u32::try_from(p.id).ok()?;
    game.mobs.iter().find(|m| m.id == victim && !m.removed && m.alive())?;
    let remote = game.remote.as_ref()?;
    let kind = match remote.registries.name("minecraft:damage_type", p.source_type) {
        Some("minecraft:player_explosion") => DamageKind::PlayerExplosion,
        Some("minecraft:bad_respawn_point") => DamageKind::AnchorExplosion,
        Some("minecraft:player_attack") => DamageKind::PlayerMelee,
        Some("minecraft:mob_attack") => DamageKind::MobMelee,
        _ => DamageKind::Other,
    };
    Some(DamageNotice { victim, enemy_player: remote.level_players.contains_key(&p.id) && enemy(victim), kind,
        cause: p.cause.and_then(|v| u32::try_from(v).ok()), direct: p.direct.and_then(|v| u32::try_from(v).ok()),
        position: p.position.filter(|at| at.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0)) })
}
pub(crate) fn removed_ids(packet: &C) -> Option<Vec<u32>> {
    let C::RemoveEntities(p) = packet else { return None; };
    Some(p.ids.iter().take(4096).filter_map(|id| u32::try_from(*id).ok()).collect())
}
pub(crate) fn spawned_base(game: &Game<'_>, packet: &C) -> Option<[i32; 3]> {
    let C::AddEntity(p) = packet else { return None; };
    if game.remote.as_ref()?.builtin.get("minecraft:entity_type")?.name(p.kind)? != "minecraft:end_crystal"
        || !p.position.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0) { return None; }
    Some([p.position[0].floor() as i32, (p.position[1] - 1.0).floor() as i32, p.position[2].floor() as i32])
}
