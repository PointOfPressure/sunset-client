// The game thread snapshots a profile before Horizon starts the network worker.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

pub struct Plan {
    pub brand: String,
    pub profile: &'static str,
    pub host: String,
    pub notes: Vec<&'static str>,
    pub joins: Vec<(&'static str, Vec<u8>)>,
}

thread_local! {
    static NEXT: RefCell<Option<(String, Plan)>> = const { RefCell::new(None) };
}

struct Connection {
    // The counters cell is shared by the worker and the session. It lets us
    // prune abandoned attempts even if the game never polls a disconnect.
    owner: Weak<dyn Send + Sync>,
    plan: Arc<Plan>,
    prepared: bool,
    configuring: bool,
    horizon_brand: bool,
    report_due: bool,
    joins_due: bool,
}

static CONNECTIONS: Mutex<BTreeMap<u64, Connection>> = Mutex::new(BTreeMap::new());

fn connections() -> MutexGuard<'static, BTreeMap<u64, Connection>> {
    CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn next(address: &str, plan: Option<Plan>) {
    NEXT.with(|next| *next.borrow_mut() = plan.map(|plan| (address.to_owned(), plan)));
}

pub fn clear() {
    NEXT.with(|next| *next.borrow_mut() = None);
    connections().clear();
}

pub fn attach(address: &str, serial: u64, owner: Weak<dyn Send + Sync>) -> bool {
    let Some((target, plan)) = NEXT.with(|next| next.take()) else {
        return false;
    };
    if target != address {
        return false;
    }
    let mut connections = connections();
    connections.retain(|_, connection| connection.owner.strong_count() > 0);
    connections.insert(
        serial,
        Connection {
            owner,
            plan: Arc::new(plan),
            prepared: false,
            configuring: true,
            horizon_brand: false,
            report_due: false,
            joins_due: false,
        },
    );
    true
}

// Called once from the worker at the initial configuration brand.
pub fn payload(serial: u64) -> Option<String> {
    let mut connections = connections();
    let connection = connections.get_mut(&serial)?;
    connection.prepared = true;
    connection.report_due = true;
    Some(connection.plan.brand.clone())
}

pub enum Event {
    Configuring,
    Play,
    HorizonBrand,
    Login,
    Closed,
}

pub fn seen(serial: u64, event: Event) {
    let mut connections = connections();
    if matches!(event, Event::Closed) {
        connections.remove(&serial);
        return;
    }
    let Some(connection) = connections.get_mut(&serial).filter(|c| c.prepared) else {
        return;
    };
    match event {
        Event::Configuring => {
            connection.configuring = true;
            connection.joins_due = false;
        }
        Event::Play => connection.configuring = false,
        Event::HorizonBrand if connection.configuring => {
            // Horizon emits this configuration event after writing its compatibility brand.
            connection.horizon_brand = true;
            connection.report_due = true;
        }
        Event::Login if !connection.configuring => {
            connection.joins_due = !connection.plan.joins.is_empty();
        }
        _ => {}
    }
}

pub fn joins(serial: u64) -> Option<Arc<Plan>> {
    let mut connections = connections();
    let connection = connections.get_mut(&serial)?;
    std::mem::take(&mut connection.joins_due).then(|| connection.plan.clone())
}

pub fn report(serial: u64) -> Option<(Arc<Plan>, bool)> {
    let mut connections = connections();
    let connection = connections.get_mut(&serial)?;
    (connection.report_due && !connection.configuring).then(|| {
        connection.report_due = false;
        (connection.plan.clone(), connection.horizon_brand)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attach_profile(serial: u64, brand: &str) -> Arc<dyn Send + Sync> {
        let owner: Arc<dyn Send + Sync> = Arc::new(());
        next(
            "same.test",
            Some(Plan {
                brand: brand.into(),
                profile: "Custom",
                host: "same.test".into(),
                notes: Vec::new(),
                joins: vec![("test:register", Vec::new())],
            }),
        );
        assert!(attach("same.test", serial, Arc::downgrade(&owner)));
        owner
    }

    #[test]
    fn delayed_worker_keeps_its_own_profile_on_same_host_reconnect() {
        let _first = attach_profile(10, "first");
        let _second = attach_profile(11, "second");
        let first = std::thread::spawn(|| payload(10)).join().unwrap();
        assert_eq!(first.as_deref(), Some("first"));
        assert_eq!(payload(11).as_deref(), Some("second"));

        seen(10, Event::Closed);
        assert!(payload(10).is_none());
        assert_eq!(payload(11).as_deref(), Some("second"));
    }

    #[test]
    fn login_only_releases_its_own_sessions_join_packets() {
        let _first = attach_profile(20, "first");
        let _second = attach_profile(21, "second");
        assert!(payload(20).is_some());
        assert!(payload(21).is_some());
        seen(20, Event::Play);
        seen(21, Event::Play);
        seen(20, Event::Login);
        assert!(joins(21).is_none());
        assert_eq!(joins(20).unwrap().brand, "first");
        assert!(joins(20).is_none());

        seen(20, Event::Login);
        seen(20, Event::Configuring);
        assert!(joins(20).is_none());
        seen(20, Event::Play);
        seen(20, Event::Login);
        assert_eq!(joins(20).unwrap().brand, "first");
    }

    #[test]
    fn reports_horizon_compatibility_brand_during_configuration() {
        let _owner = attach_profile(30, "selected");
        assert!(payload(30).is_some());
        seen(30, Event::Play);
        seen(30, Event::HorizonBrand);
        assert!(!report(30).unwrap().1);
        assert!(report(30).is_none());

        seen(30, Event::Configuring);
        seen(30, Event::HorizonBrand);
        assert!(report(30).is_none());
        seen(30, Event::Play);
        assert!(report(30).unwrap().1);
        assert!(report(30).is_none());
    }
}
