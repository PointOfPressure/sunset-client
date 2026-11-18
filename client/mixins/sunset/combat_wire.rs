// What went on the wire last (after Sunset changed it): the look and the sprint the server has.
use super::{Dim, Game};
use horizon_protocol::packets::play::Serverbound as S;
use std::sync::{Arc, Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
static LOOK: Mutex<Option<(Session, Dim, (f32, f32))>> = Mutex::new(None);
static SPRINT: Mutex<Option<(Session, Dim, bool)>> = Mutex::new(None);
static INPUT: Mutex<Option<(Session, Dim, u8)>> = Mutex::new(None);
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
pub(super) fn sent_input(game: &Game<'_>) -> u8 {
    INPUT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(session, dim, _)| {
            *dim == game.dim
                && game.remote.as_ref().is_some_and(|remote| Arc::ptr_eq(session, &remote.session.counters))
        })
        .map_or(0, |(_, _, input)| *input)
}
// Lifecycle bookkeeping only: keep native sendChanges retry caches accurate
// while ordinary module observers are stopped.
pub(super) fn observe(game: &Game<'_>, packet: &S) {
    if let (S::PlayerInput(input), Some(remote)) = (packet, &game.remote) {
        *INPUT.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((remote.session.counters.clone(), game.dim, input.flags));
    }
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
}

// The one place that tells Sunset a packet is going on the wire now, whether the send hook let it through or Fake Lag
// let it go later, so every module that tracks what the server has hears of each packet once, at the time it goes.
pub(super) fn delivered(game: &Game<'_>, packet: &S) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
        observe(game, packet);
        return;
    }
    forwarded(game, packet);
    super::sunset_cart::sent(game, packet);
    super::sunset_render::sent(game, packet);
    super::sunset_drain::sent(game, packet);
    super::sunset_effects::sent(game, packet);
    super::sunset_lag::sent(game, packet);
    super::sunset_backtrack::sent(game, packet);
}

pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    observe(game, packet);
    if !game.native_window() || !game.native_on(super::sunset::ID) {
        if game.native_window() {
            super::sunset_gui_move::forwarded(game, packet);
        }
        return;
    }
    super::sunset_gui_move::forwarded(game, packet);
    super::sunset_inventory_slots::sent(packet);
    if let S::Attack(p) = packet {
        attack_slow(game, p.id);
    }
    super::sunset_prediction::forwarded(game, packet);
    super::sunset_melee::forwarded(game, packet);
    super::sunset_auto_mace::forwarded(game, packet);
    super::sunset_combat_inventory::forwarded(game, packet);
}

pub(super) fn reset_look() {
    *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *SPRINT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *INPUT.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

// Player.attack's own side, on the attacking client: a hit on a player with a sprint and over 0.9 of the charge
// knocks back, and the attacker's speed across is cut to 0.6 and the sprint stops. The game's remote attack (and every
// Sunset hit) sends the packet only, but Grim counts the cut as certain then (PacketPlayerAttack: minAttackSlow) and
// keeps no uncut move, so the next movement takes it, before it runs (motion_flags' attack slow).
// The charge is the game's own (getAttackStrengthScale): the attack speed of the item the last tick held
// (`attack_item`), not of one switched in for this hit. Its modifiers come with the player's tick, on the server too,
// so a sword charged long ago swapped for a mace still hits charged, and the cut is due.
fn attack_slow(game: &Game<'_>, id: i32) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let charge = game.hand_strength.scale(0.5, game.main_hand_attack_speed());
    let player = super::remote::sunset_combat_remote::is_player(game, id);
    let cut = p.sprinting && charge > 0.9 && player;
    if cut {
        horizon_client::motion::sunset_motion::add_attack_slow(game.ticks);
    }
}
