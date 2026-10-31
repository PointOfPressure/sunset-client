// Original bounded server-observation ledger for optional crystal-base setup.
// A sequence acknowledgement or predicted block is deliberately not an ACK.
pub const CAPACITY: usize = 64;
pub const MAX_SENDS: u8 = 3;
pub const WAIT_TICKS: u64 = 30;
#[derive(Clone, Debug)]
struct Cell {
    pos: [i32; 3], observed: u32, sends: u8, pending: Option<u64>,
    obsidian_payload: bool, ready_after: u64,
}
#[derive(Default)]
pub struct Ledger { cells: Vec<Cell> }
impl Ledger {
    pub const fn new() -> Self { Self { cells: Vec::new() } }
    pub fn clear(&mut self) { self.cells.clear(); }
    pub fn len(&self) -> usize { self.cells.len() }
    pub fn can_send(&self, pos: [i32; 3], tick: u64) -> bool {
        self.cells.iter().find(|c| c.pos == pos).map_or(self.cells.len() < CAPACITY, |c|
            c.sends < MAX_SENDS && c.pending.is_none_or(|sent|
                tick >= sent && tick - sent >= WAIT_TICKS))
    }
    // Call only immediately before the accepted real send, not when queuing.
    pub fn sent(&mut self, pos: [i32; 3], observed: u32, tick: u64, delay: u64) -> bool {
        if !self.can_send(pos, tick) { return false; }
        let index = match self.cells.iter().position(|c| c.pos == pos) {
            Some(index) => index,
            None => { self.cells.push(Cell { pos, observed, sends: 0, pending: None,
                obsidian_payload: false, ready_after: 0 }); self.cells.len() - 1 }
        };
        let cell = &mut self.cells[index];
        cell.sends += 1; cell.pending = Some(tick); cell.obsidian_payload = false;
        cell.ready_after = tick.saturating_add(delay.max(2));
        true
    }
    // The caller validates the registry id and the connection/dimension. An
    // unchanged rejection-air payload cannot replenish the three-send budget.
    pub fn payload(&mut self, pos: [i32; 3], id: u32, obsidian: bool) {
        let Some(cell) = self.cells.iter_mut().find(|c| c.pos == pos) else { return; };
        if id != cell.observed {
            cell.observed = id;
            if !obsidian { cell.sends = 0; cell.pending = None; }
        }
        cell.obsidian_payload = obsidian;
    }
    // Unknown cells are ordinary existing bases. A cell we attempted requires
    // its actual obsidian payload even if its pending timer has expired.
    pub fn crystal_allowed(&self, pos: [i32; 3], tick: u64, world_obsidian: bool) -> bool {
        self.cells.iter().find(|c| c.pos == pos).is_none_or(|c|
            c.obsidian_payload && world_obsidian && tick >= c.ready_after)
    }
    pub fn acknowledged(&mut self, pos: [i32; 3], world_obsidian: bool) -> bool {
        let Some(cell) = self.cells.iter_mut().find(|c| c.pos == pos) else { return false; };
        if !cell.obsidian_payload || !world_obsidian { return false; }
        cell.pending = None;
        true
    }
}

// Protocol SectionBlocksUpdate packing, used only for actual received payloads.
pub fn unpack_section(section: [i32; 3], change: i64) -> Option<([i32; 3], u32)> {
    let id = u32::try_from((change as u64) >> 12).ok()?;
    Some(([section[0].checked_mul(16)?.checked_add(((change >> 8) & 15) as i32)?,
        section[1].checked_mul(16)?.checked_add((change & 15) as i32)?,
        section[2].checked_mul(16)?.checked_add(((change >> 4) & 15) as i32)?], id))
}
