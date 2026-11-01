// Protect Loot: no explosion near where a player just died, so their drops aren't blown up.
use super::remote::sunset_loot_remote::{self, Death};
use super::sunset_explosive_damage::Blast;
use super::sunset_loot_policy::{Dimension, Guard, Reason};
use super::{Game, sunset};
use horizon_protocol::{connection::Counters, packets::play::Clientbound as C};
use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
type Session = Arc<Mutex<Option<Arc<Counters>>>>;
struct State {
    session: Option<Session>,
    dimension: Option<Dimension>,
    guard: Guard,
    last: Reason,
    blocked: u64,
}
static STATE: Mutex<State> =
    Mutex::new(State { session: None, dimension: None, guard: Guard::new(), last: Reason::Clear, blocked: 0 });
static START: OnceLock<Instant> = OnceLock::new();
fn now() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
fn same_session(game: &Game<'_>, s: &State) -> bool {
    game.remote.as_ref().zip(s.session.as_ref()).is_some_and(|(r, old)| Arc::ptr_eq(&r.session.counters, old))
}
fn scene(game: &Game<'_>, name: &str, login: bool) {
    let dimension = Dimension::of(name);
    state(|s| {
        if login || !same_session(game, s) || s.dimension != dimension {
            s.guard.clear();
            s.last = Reason::Clear;
        }
        s.session = game.remote.as_ref().map(|r| r.session.counters.clone());
        s.dimension = dimension;
    });
}
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if !game.native_window() {
        return;
    }
    match packet {
        C::Login(p) => scene(game, &p.spawn.dimension, true),
        // Same-connection, same-dimension respawn deliberately retains death
        // zones. A changed local entity id is not a different world.
        C::Respawn(p) => scene(game, &p.spawn.dimension, false),
        C::Disconnect(_) => state(|s| {
            s.session = None;
            s.dimension = None;
            s.guard.clear();
            s.last = Reason::Clear;
        }),
        C::EntityEvent(p) if p.event == 3 && game.native_on(sunset::ID) => {
            // Event3 also belongs to egg/snowball particles. The native identity
            // bridge must recognize an actual current player before recording.
            let death = sunset_loot_remote::player_death(game, p.id);
            if matches!(death, Death::Other) {
                return;
            }
            state(|s| {
                if !same_session(game, s) || s.dimension.is_none() {
                    return;
                }
                match death {
                    Death::Player(Some(at)) => s.guard.record(now(), p.id, at),
                    Death::Player(None) => s.guard.unknown_death(now()),
                    Death::Other => {}
                }
            });
        }
        _ => {}
    }
}
pub(super) fn allows(game: &Game<'_>, blast: Blast, protect: bool) -> bool {
    if !protect {
        return true;
    }
    state(|s| {
        s.last = if !same_session(game, s) || s.dimension.is_none() {
            Reason::UnknownScene
        } else {
            s.guard.check(now(), blast.center, blast.power)
        };
        let allowed = s.last == Reason::Clear;
        if !allowed {
            s.blocked = s.blocked.saturating_add(1);
        }
        allowed
    })
}
