// Restore stock Projectile.recreateFromPacket owner metadata after a native
// accepted pearl spawn. AddEntity.data is the raw entity ID, not ID plus one.
use crate::game::Game;
use horizon_client::{mobs::Who, projectiles::ProjectileKind};
use std::{any::Any, sync::{Arc, Mutex}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OwnerReason {
    Rejected,
    InvalidSpawn,
    InvalidOwner,
    UnknownOwner,
    MissingNative,
    BoundLocal,
    BoundMob,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct OwnerSnapshot {
    pub tick: u64,
    pub own_id: i32,
    pub spawn_id: i32,
    pub raw_data: i32,
    pub resolved: Option<Who>,
    pub reason: OwnerReason,
}
struct Scene {
    dim: crate::game::Dim,
    own_id: i32,
    counters: Arc<dyn Any + Send + Sync>,
}
static LAST: Mutex<Option<(Scene, OwnerSnapshot)>> = Mutex::new(None);

// No scan, logging or formatting: only a single bounded diagnostic record.
fn remember(scene: Scene, snapshot: OwnerSnapshot) {
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = Some((scene, snapshot));
}
pub(super) fn snapshot(game: &Game<'_>) -> Option<OwnerSnapshot> {
    let remote = game.remote.as_ref()?;
    let last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    last.as_ref().filter(|(scene, _)| {
        scene.dim == game.dim && scene.own_id == remote.entity_id
            && Arc::as_ptr(&scene.counters) as *const ()
                == Arc::as_ptr(&remote.session.counters) as *const ()
    }).map(|(_, snapshot)| *snapshot)
}

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
    if kind != "minecraft:ender_pearl" {
        return accepted;
    }
    let Some(remote) = game.remote.as_ref() else {
        return accepted;
    };
    let scene = Scene {
        dim: game.dim,
        own_id: remote.entity_id,
        counters: remote.session.counters.clone() as Arc<dyn Any + Send + Sync>,
    };
    let mut trace = OwnerSnapshot {
        tick: game.ticks, own_id: remote.entity_id, spawn_id: id, raw_data: data,
        resolved: None, reason: OwnerReason::Rejected,
    };
    if !accepted || id < 0 || data < 0 {
        trace.reason = if !accepted { OwnerReason::Rejected }
            else if id < 0 { OwnerReason::InvalidSpawn } else { OwnerReason::InvalidOwner };
        remember(scene, trace);
        return accepted;
    }
    let owner = if remote.entity_id == data && game.player.is_some() {
        Some(Who::Player)
    } else if game.mobs.iter().any(|mob| mob.id == data as u32 && !mob.removed) {
        Some(Who::Mob(data as u32))
    } else {
        None
    };
    trace.resolved = owner;
    trace.reason = OwnerReason::UnknownOwner;
    if let Some(owner) = owner {
        trace.reason = OwnerReason::MissingNative;
        if let Some(pearl) = game.creatures.projectiles.iter_mut().find(|pearl| {
            pearl.id == id as u32 && matches!(pearl.kind, ProjectileKind::EnderPearl)
        }) {
            pearl.owner = Some(owner);
            trace.reason = if owner == Who::Player { OwnerReason::BoundLocal }
                else { OwnerReason::BoundMob };
        }
    }
    remember(scene, trace);
    accepted
}
