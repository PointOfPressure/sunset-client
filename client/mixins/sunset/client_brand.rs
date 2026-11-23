// The game thread selects the profile and attaches it to the connection before spawning its network worker.
// The counters cell is shared by that worker and Session, including before the socket has connected.
use horizon_protocol::connection::Counters;
use horizon_protocol::packets::play::Clientbound as C;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

// What a connection sends: its brand, the profile it came from, where it went, what could not be used, and the custom
// payloads (channel, data) its client sends once the server's Login packet is in.
pub struct Plan {
    pub brand: String,
    pub profile: &'static str,
    pub host: String,
    pub notes: Vec<&'static str>,
    pub joins: Vec<(&'static str, Vec<u8>)>,
}

pub(super) type CountersCell = Mutex<Option<Arc<Counters>>>;

thread_local! {
    static NEXT: RefCell<Option<(String, Plan)>> = const { RefCell::new(None) };
    static WORKER: RefCell<Option<Weak<CountersCell>>> = const { RefCell::new(None) };
}

struct Connection {
    owner: Weak<CountersCell>,
    plan: Arc<Plan>,
    prepared: bool,
    configuring: bool,
    horizon_brand: bool,
    report_due: bool,
    joins_due: bool,
}

static CONNECTIONS: Mutex<Vec<Connection>> = Mutex::new(Vec::new());

fn connections() -> MutexGuard<'static, Vec<Connection>> {
    CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner())
}

impl Connection {
    fn owns(&self, counters: &CountersCell) -> bool {
        std::ptr::eq(self.owner.as_ptr(), counters)
    }

    fn seen(&mut self, event: &super::Event) {
        use super::Event;
        match event {
            Event::Reconfiguring => {
                self.configuring = true;
                self.joins_due = false;
            }
            Event::Joined { .. } | Event::Reconfigured(_) => self.configuring = false,
            Event::Brand(brand) if self.configuring && brand == "horizon" => {
                // Horizon emits this configuration event after writing its compatibility brand.
                self.horizon_brand = true;
                self.report_due = true;
            }
            Event::Packet(packet) if !self.configuring && matches!(**packet, C::Login(_)) => {
                self.joins_due = !self.plan.joins.is_empty();
            }
            _ => {}
        }
    }
}

pub fn next(address: &str, plan: Option<Plan>) {
    NEXT.with(|next| *next.borrow_mut() = plan.map(|plan| (address.to_owned(), plan)));
}

pub fn clear() {
    NEXT.with(|next| *next.borrow_mut() = None);
    connections().clear();
}

pub(super) fn attach(address: &str, local: bool, counters: &Arc<CountersCell>) {
    let pending = NEXT.with(|next| next.take());
    let mut connections = connections();
    connections.retain(|connection| connection.owner.strong_count() > 0);
    if let Some((target, plan)) = pending
        && target == address
        && !local
    {
        connections.push(Connection {
            owner: Arc::downgrade(counters),
            plan: Arc::new(plan),
            prepared: false,
            configuring: true,
            horizon_brand: false,
            report_due: false,
            joins_due: false,
        });
    }
}

pub(super) fn forget(counters: &CountersCell) {
    connections().retain(|connection| !connection.owns(counters));
}

// Horizon's brand() has no connection arguments. Bind its call on this worker to the same cell as Session.
pub(super) fn begin(counters: &CountersCell) {
    let owner = connections()
        .iter()
        .find(|c| c.owns(counters))
        .map(|c| c.owner.clone());
    WORKER.with(|worker| *worker.borrow_mut() = owner);
}

pub(super) fn payload() -> Option<Vec<u8>> {
    let counters = WORKER.with(|worker| worker.take())?.upgrade()?;
    let mut connections = connections();
    let connection = connections.iter_mut().find(|c| c.owns(&counters))?;
    connection.prepared = true;
    connection.report_due = true;
    let mut writer = horizon_protocol::buf::Writer::new();
    writer.string(&connection.plan.brand);
    Some(writer.data)
}

pub(super) fn seen(session: &super::Session, events: &[super::Event]) {
    let mut connections = connections();
    if let Some(connection) = connections.iter_mut().find(|c| c.owns(&session.counters))
        && connection.prepared
    {
        for event in events {
            connection.seen(event);
        }
    }
}

// The plan whose join packets are due, once per Login from this session.
pub fn joins(session: &super::Session) -> Option<Arc<Plan>> {
    let mut connections = connections();
    let connection = connections.iter_mut().find(|c| c.owns(&session.counters))?;
    if session.configuring.load(Ordering::Acquire) || !std::mem::take(&mut connection.joins_due) {
        return None;
    }
    Some(connection.plan.clone())
}

// Reports local preparation and Horizon negotiation without claiming server receipt.
pub fn report(session: &super::Session) -> Option<(Arc<Plan>, bool)> {
    let mut connections = connections();
    let connection = connections.iter_mut().find(|c| c.owns(&session.counters))?;
    (connection.report_due && !connection.configuring).then(|| {
        connection.report_due = false;
        (connection.plan.clone(), connection.horizon_brand)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Event, Session};
    use super::*;
    use horizon_protocol::packets::play::{Login, SpawnInfo};

    fn attach_profile(session: &Session, brand: &str, local: bool) {
        next(
            "example.test",
            Some(Plan {
                brand: brand.into(),
                profile: "Custom",
                host: "example.test".into(),
                notes: Vec::new(),
                joins: vec![("test:register", Vec::new())],
            }),
        );
        attach("example.test", local, &session.counters);
    }

    fn joined() -> Event {
        Event::Joined {
            registries: Default::default(),
            uuid: 1,
            name: "Tester".into(),
        }
    }

    fn initial_brand(counters: &CountersCell) -> Option<Vec<u8>> {
        begin(counters);
        payload()
    }

    #[test]
    fn late_worker_keeps_its_own_brand_on_same_host_reconnect() {
        let first = Session::replay("Tester");
        let second = Session::replay("Tester");
        let local = Session::replay("Tester");
        attach_profile(&first, "first", false);
        attach_profile(&second, "second", false);
        attach_profile(&local, "local", true);

        let pending = first.counters.clone();
        assert_eq!(
            std::thread::spawn(move || initial_brand(&pending))
                .join()
                .unwrap()
                .as_deref(),
            Some(b"\x05first".as_slice())
        );
        assert_eq!(
            initial_brand(&second.counters).as_deref(),
            Some(b"\x06second".as_slice())
        );
        assert!(initial_brand(&local.counters).is_none());
        forget(&first.counters);
        assert!(initial_brand(&first.counters).is_none());
    }

    #[test]
    fn join_work_belongs_to_the_session_that_received_login() {
        let first = Session::replay("Tester");
        let second = Session::replay("Tester");
        attach_profile(&first, "first", false);
        attach_profile(&second, "second", false);
        assert!(initial_brand(&first.counters).is_some());
        assert!(initial_brand(&second.counters).is_some());
        let login = || {
            Event::Packet(Box::new(C::Login(Login {
                player_id: 1,
                hardcore: false,
                levels: Vec::new(),
                max_players: 1,
                chunk_radius: 2,
                simulation_distance: 2,
                reduced_debug_info: false,
                show_death_screen: true,
                limited_crafting: false,
                spawn: SpawnInfo {
                    dimension_type: 0,
                    dimension: "minecraft:overworld".into(),
                    seed: 0,
                    game_type: 0,
                    previous_game_type: None,
                    debug: false,
                    flat: false,
                    last_death: None,
                    portal_cooldown: 0,
                    sea_level: 63,
                },
                online_mode: false,
                enforces_secure_chat: false,
            })))
        };
        seen(&first, &[joined(), login()]);
        seen(&second, &[joined()]);
        assert!(joins(&second).is_none());
        first.configuring.store(true, Ordering::Relaxed);
        assert!(joins(&first).is_none());
        first.configuring.store(false, Ordering::Relaxed);
        assert_eq!(joins(&first).unwrap().brand, "first");
        assert!(joins(&first).is_none());
        seen(&first, &[login(), Event::Reconfiguring]);
        assert!(joins(&first).is_none());
        seen(&first, &[Event::Reconfigured(Default::default()), login()]);
        assert_eq!(joins(&first).unwrap().brand, "first");
    }

    #[test]
    fn report_distinguishes_configuration_brand_reply_from_a_play_brand() {
        let session = Session::replay("Tester");
        attach_profile(&session, "selected", false);
        assert!(initial_brand(&session.counters).is_some());
        seen(&session, &[joined(), Event::Brand("horizon".into())]);
        let (plan, compatibility_brand) = report(&session).unwrap();
        assert_eq!(plan.brand, "selected");
        assert!(!compatibility_brand);
        assert!(report(&session).is_none());

        seen(
            &session,
            &[
                Event::Reconfiguring,
                Event::Brand("horizon".into()),
                Event::Reconfigured(Default::default()),
            ],
        );
        assert!(report(&session).unwrap().1);
        assert!(report(&session).is_none());
    }
}
