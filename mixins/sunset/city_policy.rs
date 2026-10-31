// Original bounded geometry, placement acknowledgements and optimizer proof.
use std::collections::VecDeque;
pub(super) const SIDES: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

pub(super) fn valid(p: [f64; 3]) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0)
}
pub(super) fn near(a: [f64; 3], b: [f64; 3], range: f64) -> bool {
    valid(a)
        && valid(b)
        && range.is_finite()
        && range >= 0.0
        && (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>() <= range * range
}
pub(super) fn box_distance(eye: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    if !valid(eye) || !valid(lo) || !valid(hi) || !(0..3).all(|i| lo[i] <= hi[i]) {
        return None;
    }
    Some(
        (0..3)
            .map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2))
            .sum::<f64>()
            .sqrt(),
    )
}

pub(super) fn trap_cells(lo: [f64; 3], hi: [f64; 3], top: bool) -> Option<Vec<[i32; 3]>> {
    if !valid(lo) || !valid(hi) || !(0..3).all(|i| hi[i] >= lo[i]) {
        return None;
    }
    let minx = lo[0].floor() as i32;
    let maxx = (hi[0] - 1e-4).floor() as i32;
    let minz = lo[2].floor() as i32;
    let maxz = (hi[2] - 1e-4).floor() as i32;
    if maxx < minx || maxz < minz || maxx - minx > 1 || maxz - minz > 1 || hi[1] - lo[1] > 4.0 {
        return None;
    }
    let feet = (lo[1] + 0.01).floor() as i32;
    if feet < -64 || feet + 2 >= 320 {
        return None;
    }
    let mut columns = Vec::with_capacity(4);
    for x in minx..=maxx {
        for z in minz..=maxz {
            columns.push([x, z]);
        }
    }
    let mut result = Vec::with_capacity(36);
    for dy in 0..=1 {
        for column in &columns {
            for d in SIDES {
                let x = column[0] + d[0];
                let z = column[1] + d[2];
                if columns.contains(&[x, z]) {
                    continue;
                }
                let cell = [x, feet + dy, z];
                if !result.contains(&cell) {
                    result.push(cell);
                }
            }
        }
    }
    if top {
        for c in columns {
            result.push([c[0], feet + 2, c[1]]);
        }
    }
    Some(result)
}

// A flat-floor roof needs a third-height outside support. Choose one head-ring
// column nearest the observer; its upper face supports the scaffold, whose
// side supports the first roof cell. Remaining roof cells form a connected grid.
pub(super) fn trap_cells_from(
    lo: [f64; 3],
    hi: [f64; 3],
    top: bool,
    eye: [f64; 3],
) -> Option<Vec<[i32; 3]>> {
    if !valid(eye) {
        return None;
    }
    let mut cells = trap_cells(lo, hi, top)?;
    if top {
        let roof_y = cells.iter().map(|p| p[1]).max()?;
        let scaffold = cells
            .iter()
            .filter(|p| p[1] == roof_y - 1)
            .map(|p| [p[0], roof_y, p[2]])
            .filter(|p| {
                cells.iter().any(|roof| {
                    roof[1] == roof_y
                        && (i64::from(roof[0]) - i64::from(p[0])).abs()
                            + (i64::from(roof[2]) - i64::from(p[2])).abs()
                            == 1
                        && (0..3).any(|axis| {
                            let normal = roof[axis] - p[axis];
                            normal != 0
                                && (eye[axis]
                                    - (f64::from(p[axis]) + if normal > 0 { 1.0 } else { 0.0 }))
                                    * f64::from(normal)
                                    > 1e-6
                        })
                })
            })
            .min_by(|a, b| {
                let distance = |p: [i32; 3]| {
                    (0..3)
                        .map(|i| (f64::from(p[i]) + 0.5 - eye[i]).powi(2))
                        .sum::<f64>()
                };
                distance(*a).total_cmp(&distance(*b))
            })?;
        cells.push(scaffold);
    }
    Some(cells)
}

#[derive(Default)]
pub(super) struct RoofJumps {
    last: Option<u64>,
    count: u8,
}
impl RoofJumps {
    pub(super) fn clear(&mut self) {
        self.last = None;
        self.count = 0;
    }
    pub(super) fn request(&mut self, tick: u64) -> bool {
        if self.count >= 2
            || self
                .last
                .is_some_and(|last| tick < last || tick - last < 20)
        {
            return false;
        }
        self.last = Some(tick);
        self.count += 1;
        true
    }
}

#[derive(Clone, Copy)]
struct Attempt {
    pos: [i32; 3],
    before: u32,
    count: u8,
    sent: Option<u64>,
}
#[derive(Default)]
pub(super) struct Ledger {
    entries: Vec<Attempt>,
}
impl Ledger {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
    pub(super) fn can_try(&self, pos: [i32; 3], before: u32) -> bool {
        self.entries
            .iter()
            .find(|e| e.pos == pos)
            .is_none_or(|e| e.before != before || (e.count < 3 && e.sent.is_none()))
            && (self.entries.len() < 64 || self.entries.iter().any(|e| e.pos == pos))
    }
    pub(super) fn sent(&mut self, pos: [i32; 3], before: u32, tick: u64) -> bool {
        if !self.can_try(pos, before) {
            return false;
        }
        if let Some(e) = self.entries.iter_mut().find(|e| e.pos == pos) {
            if e.before != before {
                e.before = before;
                e.count = 0;
            }
            e.count += 1;
            e.sent = Some(tick);
        } else {
            self.entries.push(Attempt {
                pos,
                before,
                count: 1,
                sent: Some(tick),
            });
        }
        true
    }
    pub(super) fn observe(&mut self, pos: [i32; 3]) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.pos == pos) {
            e.sent = None;
        }
    }
    pub(super) fn expire(&mut self, tick: u64) {
        for e in &mut self.entries {
            if e.sent
                .is_some_and(|t| tick < t || tick.saturating_sub(t) >= 30)
            {
                e.sent = None;
            }
        }
    }
    pub(super) fn waiting(&self) -> usize {
        self.entries.iter().filter(|e| e.sent.is_some()).count()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Sample {
    tick: u64,
    base: Option<f32>,
}
#[derive(Default)]
pub(super) struct History {
    samples: VecDeque<Sample>,
    started: Option<u64>,
}
impl History {
    pub(super) fn clear(&mut self) {
        self.samples.clear();
        self.started = None;
    }
    pub(super) fn sample(&mut self, tick: u64, base: Option<f32>) {
        if self.samples.back().is_some_and(|p| tick < p.tick) {
            self.clear();
        }
        self.started.get_or_insert(tick);
        if self.samples.len() == 64 {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { tick, base });
    }
    pub(super) fn positive(&self, tick: u64, window: u64, effect: f32) -> bool {
        let window = window.clamp(10, 40);
        if !effect.is_finite()
            || self
                .started
                .is_none_or(|start| tick < start || tick - start < window)
            || self
                .samples
                .front()
                .is_none_or(|p| p.tick > tick.saturating_sub(window))
        {
            return false;
        }
        self.samples
            .iter()
            .filter(|p| tick.saturating_sub(p.tick) <= window)
            .all(|p| {
                p.base.is_some_and(|base| {
                    base.is_finite() && (base + effect).is_finite() && base + effect > 0.0
                })
            })
    }
}

#[derive(Clone, Copy)]
struct Mask {
    id: u32,
    pos: [f64; 3],
    sent: u64,
}
#[derive(Default)]
pub(super) struct Masks {
    hidden: Vec<Mask>,
    stubborn: Vec<u32>,
    disabled: bool,
}
impl Masks {
    pub(super) fn clear(&mut self) {
        self.hidden.clear();
        self.stubborn.clear();
        self.disabled = false;
    }
    pub(super) fn add(&mut self, id: u32, pos: [f64; 3], tick: u64) -> bool {
        if id == 0
            || !valid(pos)
            || self.disabled
            || self.stubborn.contains(&id)
            || self.hidden.iter().any(|m| m.id == id)
            || self.hidden.len() >= 64
        {
            return false;
        }
        self.hidden.push(Mask {
            id,
            pos,
            sent: tick,
        });
        true
    }
    pub(super) fn hidden(&self, id: u32, pos: [f64; 3]) -> bool {
        self.hidden.iter().any(|m| m.id == id && m.pos == pos)
    }
    pub(super) fn at(&self, pos: [f64; 3]) -> Option<u32> {
        self.hidden.iter().find(|m| m.pos == pos).map(|m| m.id)
    }
    pub(super) fn confirm(&mut self, id: u32) -> bool {
        let found = self.hidden.iter().any(|m| m.id == id);
        self.hidden.retain(|m| m.id != id);
        self.stubborn.retain(|n| *n != id);
        found
    }
    pub(super) fn expire(&mut self, tick: u64, window: u64) {
        let mut expired = Vec::with_capacity(self.hidden.len());
        self.hidden.retain(|m| {
            let old = tick < m.sent || tick.saturating_sub(m.sent) >= window.clamp(10, 40);
            if old {
                expired.push(m.id);
            }
            !old
        });
        for id in expired {
            if !self.stubborn.contains(&id) {
                if self.stubborn.len() == 128 {
                    self.disabled = true;
                    self.hidden.clear();
                    break;
                }
                self.stubborn.push(id);
            }
        }
    }
    pub(super) fn count(&self) -> usize {
        self.hidden.len()
    }
    pub(super) fn stubborn(&self) -> usize {
        self.stubborn.len()
    }
}

pub(super) fn unpack_section(section: [i32; 3], change: i64) -> Option<([i32; 3], u32)> {
    let local = (change & 0xfff) as i32;
    let pos = [
        section[0].checked_mul(16)?.checked_add(local >> 8 & 15)?,
        section[1].checked_mul(16)?.checked_add(local & 15)?,
        section[2].checked_mul(16)?.checked_add(local >> 4 & 15)?,
    ];
    let state = u32::try_from(change >> 12).ok()?;
    Some((pos, state))
}
