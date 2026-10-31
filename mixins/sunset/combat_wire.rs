// Observe only final forwarding, after rotation transforms and delay barriers.
use super::Game;
use horizon_protocol::packets::play::Serverbound as S;
use std::sync::{Arc,Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
static LOOK: Mutex<Option<(Session, (f32,f32))>> = Mutex::new(None);
fn look(game: &Game<'_>) -> (f32,f32) {
    let state=LOOK.lock().unwrap_or_else(|e|e.into_inner());
    state.as_ref().filter(|(old,_)|game.remote.as_ref().is_some_and(|r|Arc::ptr_eq(old,&r.session.counters)))
        .map_or((game.yaw,game.pitch), |(_,look)|*look)
}
pub(super) fn allowed(game: &Game<'_>, packet: &S) -> bool {
    if !game.native_window() || !game.native_on(super::sunset::ID) { return true; }
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
    let rotation=match packet {
        S::MovePlayerPosRot(p)=>Some((p.yaw,p.pitch)),
        S::MovePlayerRot(p)=>Some((p.yaw,p.pitch)),
        _=>None,
    };
    if let (Some(rotation),Some(remote))=(rotation,&game.remote) {
        *LOOK.lock().unwrap_or_else(|e|e.into_inner())=Some((remote.session.counters.clone(),rotation));
    }
    if !game.native_window() || !game.native_on(super::sunset::ID) { return; }
    match packet {
        S::Attack(p) if p.id>=0=>super::sunset_melee_assist::note_attack(game,p.id as u32),
        S::PlayerAction(p) if p.action==8=>super::sunset_melee_assist::note_stab(game),
        _=>{},
    }
}
