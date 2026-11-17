// Client Spoofer's hand-off to the network thread. The plan for a connection is made as it starts (game thread),
// taken when that connection sends its brand (network thread), then kept for the connection's life: its join packets
// go out after each play Login (the first join, and again after every reconfiguration), and Diagnostics reads it.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

// What a connection sends: its brand, the profile it came from, where it went, what could not be used, and the custom
// payloads (channel, data) its client sends once the server's Login packet is in.
pub struct Plan {
    pub brand: String,
    pub profile: &'static str,
    pub host: String,
    pub notes: Vec<&'static str>,
    pub joins: Vec<(&'static str, Vec<u8>)>,
}

static NEXT: Mutex<Option<Plan>> = Mutex::new(None);
static CURRENT: Mutex<Option<Arc<Plan>>> = Mutex::new(None);
static REPORT: AtomicBool = AtomicBool::new(false);
// A connection with join packets is live, and a Login packet came in since they last went out.
static JOINS: AtomicBool = AtomicBool::new(false);
static LOGIN: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// A connection starting: its plan (None: Horizon's own brand and nothing else). The last connection's is dropped.
pub fn next(plan: Option<Plan>) {
    *lock(&NEXT) = plan;
    *lock(&CURRENT) = None;
    REPORT.store(false, Ordering::Relaxed);
    JOINS.store(false, Ordering::Relaxed);
    LOGIN.store(false, Ordering::Relaxed);
}

// The brand to send (the game writes it as one protocol string), or None to send Horizon's own.
pub fn payload() -> Option<String> {
    let plan = lock(&NEXT).take()?;
    let brand = plan.brand.clone();
    JOINS.store(!plan.joins.is_empty(), Ordering::Release);
    *lock(&CURRENT) = Some(Arc::new(plan));
    REPORT.store(true, Ordering::Release);
    Some(brand)
}

// The game took in a play Login packet: the play phase starts (or restarts, after reconfiguration).
pub fn login_seen() {
    if JOINS.load(Ordering::Acquire) {
        LOGIN.store(true, Ordering::Release);
    }
}

// The plan whose join packets are due, once per Login.
pub fn joins() -> Option<Arc<Plan>> {
    if !LOGIN.swap(false, Ordering::Acquire) {
        return None;
    }
    lock(&CURRENT).clone()
}

// The plan of a connection whose brand went out, once.
pub fn sent() -> Option<Arc<Plan>> {
    if !REPORT.swap(false, Ordering::Acquire) {
        return None;
    }
    lock(&CURRENT).clone()
}
