// Client Spoofer's hand-off to the network thread: the brand chosen as a connection starts (game thread), taken when
// that connection sends its brand, then kept for Diagnostics.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

// The brand a connection sends, the profile it came from, where it went, and what could not be used.
pub struct Plan {
    pub brand: String,
    pub profile: &'static str,
    pub host: String,
    pub notes: Vec<&'static str>,
}

static NEXT: Mutex<Option<Plan>> = Mutex::new(None);
static SENT: Mutex<Option<Plan>> = Mutex::new(None);
static REPORT: AtomicBool = AtomicBool::new(false);

fn lock(m: &Mutex<Option<Plan>>) -> MutexGuard<'_, Option<Plan>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// A connection starting: its brand (None: Horizon's own). What the last one sent is no longer news.
pub fn next(plan: Option<Plan>) {
    *lock(&NEXT) = plan;
    *lock(&SENT) = None;
    REPORT.store(false, Ordering::Relaxed);
}

// The brand payload (one protocol string), or None to send Horizon's own.
pub fn payload() -> Option<Vec<u8>> {
    let plan = lock(&NEXT).take()?;
    let mut w = horizon_protocol::buf::Writer::new();
    w.string(&plan.brand);
    *lock(&SENT) = Some(plan);
    REPORT.store(true, Ordering::Release);
    Some(w.data)
}

// The brand that went out, once.
pub fn sent() -> Option<Plan> {
    if !REPORT.swap(false, Ordering::Acquire) {
        return None;
    }
    lock(&SENT).take()
}
