// Observe only final forwarding, after rotation transforms and delay barriers.
use super::{Game, Dim};
use horizon_protocol::packets::play::Serverbound as S;
use std::sync::{Arc,Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
static LOOK: Mutex<Option<(Session, Dim, (f32,f32))>> = Mutex::new(None);
static SPRINT: Mutex<Option<(Session, Dim, bool)>> = Mutex::new(None);
pub(super) fn sent_look(game: &Game<'_>) -> Option<(f32,f32)> {
    LOOK.lock().unwrap_or_else(|e|e.into_inner()).as_ref()
        .filter(|(session,dim,_)|*dim==game.dim && game.remote.as_ref()
            .is_some_and(|remote|Arc::ptr_eq(session,&remote.session.counters)))
        .map(|(_,_,rotation)|*rotation)
}
pub(super) fn sent_sprinting(game: &Game<'_>) -> bool {
    SPRINT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .filter(|(session,dim,_)| *dim==game.dim && game.remote.as_ref()
            .is_some_and(|remote| Arc::ptr_eq(session,&remote.session.counters)))
        .is_some_and(|(_,_,sprinting)| *sprinting)
}
fn look(game: &Game<'_>) -> (f32,f32) {
    let state=LOOK.lock().unwrap_or_else(|e|e.into_inner());
    state.as_ref().filter(|(old,dim,_)|*dim == game.dim && game.remote.as_ref().is_some_and(|r|Arc::ptr_eq(old,&r.session.counters)))
        .map_or((game.yaw,game.pitch), |(_,_,look)|*look)
}
pub(super) fn allowed(game: &Game<'_>, packet: &S) -> bool {
    if !game.native_window() || !game.native_on(super::sunset::ID) { return true; }
    if super::sunset_crystal_optimizer::duplicate(game, packet) { return false; }
    match packet {
        S::Attack(p) => p.id < 0 || !super::sunset_melee_assist::guard_attack(game,p.id as u32),
        S::PlayerAction(p) if p.action == 8 => {
            let (yaw,pitch)=look(game);
            !super::sunset_melee_assist::guard_stab_at(game,yaw,pitch)
        }
        S::UseItem(p) => {
            let slot=if p.hand==1 {super::sunset_util::OFFHAND} else {game.selected};
            !game.stack_in(slot).is_some_and(|s|horizon_client::spear::Kinetic::of(&s.item).is_some()
                && super::sunset_melee_assist::guard_stab_at(game,p.yaw,p.pitch))
        }
        _=>true,
    }
}
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    if let (S::PlayerCommand(command),Some(remote))=(packet,&game.remote)
        && command.id==remote.entity_id && matches!(command.action,1|2) {
        *SPRINT.lock().unwrap_or_else(|e| e.into_inner())=
            Some((remote.session.counters.clone(),game.dim,command.action==1));
    }
    let rotation=match packet {
        S::MovePlayerPosRot(p)=>Some((p.yaw,p.pitch)),
        S::MovePlayerRot(p)=>Some((p.yaw,p.pitch)),
        S::AcceptTeleportation(p)=>Some((p.yaw,p.pitch)),
        _=>None,
    };
    if let (Some(rotation),Some(remote))=(rotation,&game.remote) {
        *LOOK.lock().unwrap_or_else(|e|e.into_inner())=Some((remote.session.counters.clone(),game.dim,rotation));
    }
    if !game.native_window() || !game.native_on(super::sunset::ID) { return; }
    super::sunset_crystal_optimizer::sent(game, packet);
    super::sunset_prediction::forwarded(game, packet);
    super::sunset_combat_inventory::forwarded(game, packet);
    if matches!(packet,S::UseItem(_)|S::UseItemOn(_)) {
        super::sunset_melee_assist::note_action(game);
    }
    match packet {
        S::Attack(p) if p.id>=0=>{
            super::sunset_melee_assist::note_attack(game,p.id as u32);
            super::sunset_crystal_bot::attack_sent(game,p.id as u32);
        },
        S::PlayerAction(p) if p.action==8=>super::sunset_melee_assist::note_stab(game),
        _=>{},
    }
}

pub(super) fn reset_look() {
    *LOOK.lock().unwrap_or_else(|e|e.into_inner())=None;
    *SPRINT.lock().unwrap_or_else(|e|e.into_inner())=None;
}
