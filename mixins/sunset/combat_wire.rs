// What went on the wire last (after Sunset changed it): the look and the sprint the server has.
use super::{Dim, Game};
use horizon_protocol::packets::play::Serverbound as S;
use std::sync::{Arc, Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
static LOOK: Mutex<Option<(Session, Dim, (f32, f32))>> = Mutex::new(None);
static SPRINT: Mutex<Option<(Session, Dim, bool)>> = Mutex::new(None);
pub(super) fn sent_look(game: &Game<'_>) -> Option<(f32, f32)> {
    LOOK.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(session, dim, _)| {
            *dim == game.dim
                && game.remote.as_ref().is_some_and(|remote| Arc::ptr_eq(session, &remote.session.counters))
        })
        .map(|(_, _, rotation)| *rotation)
}
pub(super) fn sent_sprinting(game: &Game<'_>) -> bool {
    SPRINT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(session, dim, _)| {
            *dim == game.dim
                && game.remote.as_ref().is_some_and(|remote| Arc::ptr_eq(session, &remote.session.counters))
        })
        .is_some_and(|(_, _, sprinting)| *sprinting)
}
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    if let (S::PlayerCommand(command), Some(remote)) = (packet, &game.remote)
        && command.id == remote.entity_id
        && matches!(command.action, 1 | 2)
    {
        *SPRINT.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((remote.session.counters.clone(), game.dim, command.action == 1));
    }
    let rotation = match packet {
        S::MovePlayerPosRot(p) => Some((p.yaw, p.pitch)),
        S::MovePlayerRot(p) => Some((p.yaw, p.pitch)),
        S::AcceptTeleportation(p) => Some((p.yaw, p.pitch)),
        _ => None,
    };
    if let (Some(rotation), Some(remote)) = (rotation, &game.remote) {
        *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some((remote.session.counters.clone(), game.dim, rotation));
    }
    if !game.native_window() || !game.native_on(super::sunset::ID) {
        return;
    }
    super::sunset_prediction::forwarded(game, packet);
    super::sunset_melee::forwarded(game, packet);
    super::sunset_combat_inventory::forwarded(game, packet);
}

pub(super) fn reset_look() {
    *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *SPRINT.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
