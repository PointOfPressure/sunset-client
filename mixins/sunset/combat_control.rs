// Shared ownership only. Individual owners still validate their actions and
// release/restore their own state when cancelled, disabled or disconnected.
use super::sunset_combat_lease::Lease;
use super::{Dim, Game};
use std::sync::{Arc, Mutex};
type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
#[derive(Default)]
struct State {
    lease: Lease,
    session: Option<Session>,
    dim: Option<Dim>,
    entity: Option<i32>,
    live: bool,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}
fn alive(game: &Game<'_>) -> bool {
    game.remote.is_some()
        && !game.spectator()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
}
fn same(game: &Game<'_>, s: &State) -> bool {
    s.dim == Some(game.dim)
        && s.entity == game.remote.as_ref().map(|r| r.entity_id)
        && match (&s.session, &game.remote) {
            (Some(old), Some(remote)) => Arc::ptr_eq(old, &remote.session.counters),
            (None, None) => true,
            _ => false,
        }
}
// Returns an identity change or live deactivation. Callers cancel transient
// state AFTER this invalidates stale tickets, preventing old-scene restoration.
pub(super) fn begin_tick(game: &Game<'_>) -> bool {
    let live = game.native_on(super::sunset::ID) && alive(game);
    with(|s| {
        let session = game.remote.as_ref().map(|r| r.session.counters.clone());
        let changed = !same(game, s) || (s.live && !live);
        if changed || !live {
            s.lease.clear();
        }
        s.live = live;
        s.session = session;
        s.dim = Some(game.dim);
        s.entity = game.remote.as_ref().map(|r| r.entity_id);
        s.lease.begin(game.ticks);
        changed
    })
}
// Master-off/Panic must restore still-owned slots before clearing the ledger.
// Preserve an EXISTING live ticket only; never resurrect or adopt a scene here.
pub(super) fn prepare_cleanup(game: &Game<'_>) {
    with(|s| {
        if !s.live || !alive(game) || !same(game, s) {
            s.lease.clear();
            s.live = false;
        }
    });
}
pub(super) fn claim(owner: &'static str) -> bool {
    with(|s| s.live && s.lease.claim(owner))
}
pub(super) fn owns(owner: &str) -> bool {
    with(|s| s.live && s.lease.owner == Some(owner))
}
pub(super) fn available(owner: &str) -> bool {
    with(|s| s.live && s.lease.owner.is_none_or(|current| current == owner))
}
pub(super) fn release(owner: &str) {
    with(|s| s.lease.release(owner));
}
pub(super) fn owner() -> Option<&'static str> {
    with(|s| s.lease.owner)
}
pub(super) fn reset() {
    with(|s| {
        s.lease.clear();
        s.session = None;
        s.dim = None;
        s.entity = None;
        s.live = false;
    });
}
#[must_use]
pub(super) struct Finish {
    owner: &'static str,
    ticket: Option<u64>,
}
impl Finish {
    pub(super) fn current(&self) -> bool {
        self.ticket.is_some() && with(|s| s.live && s.lease.ticket(self.owner) == self.ticket)
    }
}
impl Drop for Finish {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket {
            with(|s| s.lease.finish(self.owner, ticket));
        }
    }
}
pub(super) fn finish(owner: &'static str) -> Finish {
    Finish {
        owner,
        ticket: with(|s| s.lease.ticket(owner)),
    }
}
