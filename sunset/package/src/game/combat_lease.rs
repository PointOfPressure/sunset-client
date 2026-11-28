// The owner combat_control.rs keeps: claimed each tick it is used, dropped when it lapses.
#[derive(Default)]
pub(super) struct Lease {
    pub owner: Option<&'static str>,
    tick: u64,
    seen: u64,
    ticket: u64,
}
impl Lease {
    pub fn begin(&mut self, tick: u64) {
        if tick < self.tick || tick.saturating_sub(self.seen) > 10 {
            self.clear();
        }
        self.tick = tick;
    }
    pub fn claim(&mut self, owner: &'static str) -> bool {
        if self.owner.is_some_and(|current| current != owner) {
            return false;
        }
        if self.owner.is_none() {
            self.ticket = self.ticket.wrapping_add(1);
        }
        self.owner = Some(owner);
        self.seen = self.tick;
        true
    }
    pub fn release(&mut self, owner: &str) {
        if self.owner == Some(owner) {
            self.clear();
        }
    }
    pub fn clear(&mut self) {
        self.owner = None;
        self.ticket = self.ticket.wrapping_add(1);
    }
    pub fn ticket(&self, owner: &str) -> Option<u64> {
        (self.owner == Some(owner)).then_some(self.ticket)
    }
    pub fn finish(&mut self, owner: &str, ticket: u64) {
        if self.ticket(owner) == Some(ticket) {
            self.clear();
        }
    }
}
