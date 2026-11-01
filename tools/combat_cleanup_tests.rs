// Runs the ACTUAL production control/Finish code against a minimal native
// Game-interface fixture. No ownership or ticket algorithm is reimplemented.
extern crate self as horizon_protocol;
pub mod connection {
    pub struct Counters;
}
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dim {
    Overworld,
    Nether,
}
struct Health(f32);
impl Health {
    fn health(&self) -> f32 {
        self.0
    }
}
struct Player {
    health: Health,
}
struct Session {
    counters: Arc<Mutex<Option<Arc<connection::Counters>>>>,
}
struct Remote {
    entity_id: i32,
    session: Session,
}
struct Game<'a> {
    remote: Option<Remote>,
    player: Option<Player>,
    dim: Dim,
    ticks: u64,
    on: bool,
    spectator: bool,
    _l: PhantomData<&'a ()>,
}
impl Game<'_> {
    fn native_on(&self, _: &str) -> bool {
        self.on
    }
    fn spectator(&self) -> bool {
        self.spectator
    }
}
mod sunset {
    pub const ID: &str = "sunset";
}
#[path = "../mixins/sunset/combat_control.rs"]
mod control;
#[allow(dead_code)]
#[path = "../mixins/sunset/melee_policy.rs"]
mod policy;
#[path = "../mixins/sunset/combat_lease.rs"]
mod sunset_combat_lease;
static LOCK: Mutex<()> = Mutex::new(());
fn game() -> Game<'static> {
    Game {
        remote: Some(Remote {
            entity_id: 7,
            session: Session {
                counters: Arc::new(Mutex::new(None)),
            },
        }),
        player: Some(Player {
            health: Health(20.0),
        }),
        dim: Dim::Overworld,
        ticks: 10,
        on: true,
        spectator: false,
        _l: PhantomData,
    }
}
fn slot() -> policy::SlotLease {
    policy::SlotLease {
        owner: "Mace",
        previous: 0,
        selected: 2,
        until: 20,
        return_enabled: true,
    }
}

#[test]
fn same_scene_master_off_keeps_real_finish_ticket_for_owned_slot_cleanup() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    control::reset();
    let mut g = game();
    assert!(control::begin_tick(&g));
    assert!(control::claim("Mace"));
    let done = control::finish("Mace");
    g.on = false;
    control::prepare_cleanup(&g);
    assert!(done.current());
    assert_eq!(slot().restore(2, u64::MAX, done.current()), Some(0));
    drop(done);
    assert_eq!(control::owner(), None);
    control::reset();
}

#[test]
fn replacement_connection_cannot_restore_or_release_new_same_owner_ticket() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    control::reset();
    let mut g = game();
    control::begin_tick(&g);
    assert!(control::claim("Mace"));
    let old = control::finish("Mace");
    g.remote.as_mut().unwrap().session.counters = Arc::new(Mutex::new(None));
    g.on = false;
    control::prepare_cleanup(&g);
    assert!(!old.current());
    assert_eq!(slot().restore(2, u64::MAX, old.current()), None);
    g.on = true;
    assert!(control::begin_tick(&g));
    assert!(control::claim("Mace"));
    let new = control::finish("Mace");
    drop(old);
    assert!(new.current());
    drop(new);
    assert_eq!(control::owner(), None);
    control::reset();
}

#[test]
fn changed_dimension_player_death_spectator_and_nonfinite_health_reject_cleanup() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for changed in 0..6 {
        control::reset();
        let mut g = game();
        control::begin_tick(&g);
        assert!(control::claim("Mace"));
        let done = control::finish("Mace");
        match changed {
            0 => g.dim = Dim::Nether,
            1 => g.remote.as_mut().unwrap().entity_id = 8,
            2 => g.player.as_mut().unwrap().health.0 = 0.0,
            3 => g.spectator = true,
            4 => g.player.as_mut().unwrap().health.0 = f32::NAN,
            _ => g.player = None,
        }
        g.on = false;
        control::prepare_cleanup(&g);
        assert!(!done.current());
        assert_eq!(slot().restore(2, u64::MAX, done.current()), None);
        assert!(!control::claim("Mace"));
        drop(done);
    }
    control::reset();
}

#[test]
fn begin_tick_reports_identity_or_deactivation_and_invalidates_before_cancel() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    control::reset();
    let mut g = game();
    assert!(control::begin_tick(&g));
    assert!(!control::begin_tick(&g));
    assert!(control::available("Mace"));
    assert!(control::claim("Mace"));
    let done = control::finish("Mace");
    g.on = false;
    assert!(control::begin_tick(&g));
    assert!(!done.current());
    assert!(!control::begin_tick(&g));
    control::prepare_cleanup(&g);
    assert!(!done.current());
    assert!(!control::claim("Mace"));
    drop(done);
    g.on = true;
    assert!(!control::begin_tick(&g));
    assert!(control::claim("Mace"));
    assert!(control::owns("Mace"));
    control::release("Mace");
    control::reset();
}

#[test]
fn same_scene_user_slot_override_is_not_undone_during_cleanup() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    control::reset();
    let mut g = game();
    control::begin_tick(&g);
    control::claim("Mace");
    let done = control::finish("Mace");
    g.on = false;
    control::prepare_cleanup(&g);
    assert!(done.current());
    assert_eq!(slot().restore(3, u64::MAX, done.current()), None);
    drop(done);
    control::reset();
}
