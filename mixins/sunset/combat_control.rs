// Shared ownership only. Individual owners still validate their actions and
// release/restore their own state when cancelled, disabled or disconnected.
use super::{Dim, Game};
use super::sunset_combat_lease::Lease;
use std::sync::{Arc, Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
#[derive(Default)]
struct State { lease: Lease, session: Option<Session>, dim: Option<Dim>, live: bool }
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}
pub(super) fn begin_tick(game: &Game<'_>) {
    let live = game.native_on(super::sunset::ID) && game.remote.is_some()
        && !game.spectator() && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    with(|s| {
        let session = game.remote.as_ref().map(|r| r.session.counters.clone());
        let same = match (&s.session, &session) {
            (Some(a), Some(b)) => Arc::ptr_eq(a,b),
            (None,None) => true,
            _ => false,
        };
        if !live || !same || s.dim != Some(game.dim) { s.lease.clear(); }
        s.live = live;
        s.session = session;
        s.dim = Some(game.dim);
        s.lease.begin(game.ticks);
    });
}
pub(super) fn claim(owner: &'static str) -> bool {
    with(|s| s.live && s.lease.claim(owner))
}
pub(super) fn owns(owner: &str) -> bool { with(|s| s.live && s.lease.owner == Some(owner)) }
pub(super) fn available(owner: &str) -> bool {
    with(|s| s.live && s.lease.owner.is_none_or(|current| current == owner))
}
pub(super) fn release(owner: &str) { with(|s| s.lease.release(owner)); }
pub(super) fn owner() -> Option<&'static str> { with(|s| s.lease.owner) }
pub(super) fn reset() { with(|s| { s.lease.clear(); s.session=None; s.dim=None; s.live=false; }); }
#[must_use]
pub(super) struct Finish { owner: &'static str, ticket: Option<u64> }
impl Finish {
    pub(super) fn current(&self) -> bool {
        self.ticket.is_some() && with(|s| s.live && s.lease.ticket(self.owner) == self.ticket)
    }
}
impl Drop for Finish {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket { with(|s| s.lease.finish(self.owner,ticket)); }
    }
}
pub(super) fn finish(owner: &'static str) -> Finish {
    Finish { owner, ticket: with(|s| s.lease.ticket(owner)) }
}
