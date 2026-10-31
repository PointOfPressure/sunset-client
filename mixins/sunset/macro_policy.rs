// Original deterministic ordering for manual explosive macros. Observations
// acknowledge transactions; sending a packet never advances the sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Crystal,
    Anchor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Base,
    CrystalPlace,
    CrystalBreak,
    AnchorPlace,
    Guard,
    Charge,
    Detonate,
    Done,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Sequence {
    pub kind: Kind,
    pub step: Step,
    pub remaining: u8,
}
impl Sequence {
    pub(super) fn new(kind: Kind, first: Step, double: bool) -> Self {
        Self {
            kind,
            step: first,
            remaining: if kind == Kind::Anchor && double { 2 } else { 1 },
        }
    }
    pub(super) fn acknowledged(&mut self, sent: Step) -> bool {
        if self.step != sent || sent == Step::Done {
            return false;
        }
        self.step = match sent {
            Step::Base => Step::CrystalPlace,
            Step::CrystalPlace => Step::CrystalBreak,
            Step::CrystalBreak => Step::Done,
            Step::AnchorPlace => Step::Guard,
            Step::Guard => Step::Charge,
            Step::Charge => Step::Detonate,
            Step::Detonate if self.remaining > 1 => {
                self.remaining -= 1;
                Step::AnchorPlace
            }
            Step::Detonate | Step::Done => Step::Done,
        };
        true
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wait {
    Pending,
    Acknowledged,
    Expired,
}
pub(super) fn wait(sent: u64, now: u64, observed: bool) -> Wait {
    if now < sent {
        Wait::Expired
    } else if observed {
        Wait::Acknowledged
    } else if now - sent >= 30 {
        Wait::Expired
    } else {
        Wait::Pending
    }
}
pub(super) fn restore(
    previous: usize,
    owned: usize,
    current: usize,
    requested: bool,
) -> Option<usize> {
    (requested && previous < 9 && owned < 9 && current == owned && previous != owned)
        .then_some(previous)
}

#[derive(Default)]
pub(super) struct Retries {
    entries: Vec<(u32, u8, u64)>,
}
impl Retries {
    pub(super) const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    pub(super) fn allows(&self, id: u32, tick: u64) -> bool {
        id != 0
            && self
                .entries
                .iter()
                .find(|(cid, _, _)| *cid == id)
                .map_or(self.entries.len() < 256, |(_, _, until)| tick >= *until)
    }
    pub(super) fn sent(&mut self, id: u32, tick: u64) {
        if !self.allows(id, tick) {
            return;
        }
        let index = match self.entries.iter().position(|(cid, _, _)| *cid == id) {
            Some(index) => index,
            None => {
                self.entries.push((id, 0, 0));
                self.entries.len() - 1
            }
        };
        let (_, count, until) = &mut self.entries[index];
        *count += 1;
        if *count >= 3 {
            *count = 0;
            *until = tick.saturating_add(100);
        }
    }
    pub(super) fn retain(&mut self, mut exists: impl FnMut(u32) -> bool) {
        self.entries.retain(|(id, _, _)| exists(*id));
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
}
pub(super) fn face(face: [i32; 3]) -> Option<i32> {
    match face {
        [0, -1, 0] => Some(0),
        [0, 1, 0] => Some(1),
        [0, 0, -1] => Some(2),
        [0, 0, 1] => Some(3),
        [-1, 0, 0] => Some(4),
        [1, 0, 0] => Some(5),
        _ => None,
    }
}
pub(super) fn outside(hit: [f64; 3], face: i32) -> Option<[f64; 3]> {
    let d = match face {
        0 => [0., -1., 0.],
        1 => [0., 1., 0.],
        2 => [0., 0., -1.],
        3 => [0., 0., 1.],
        4 => [-1., 0., 0.],
        5 => [1., 0., 0.],
        _ => return None,
    };
    hit.iter()
        .all(|n| n.is_finite())
        .then(|| std::array::from_fn(|i| hit[i] + d[i] * 1.0e-5))
}
pub(super) fn ray(
    eye: [f64; 3],
    direction: [f64; 3],
    lo: [f64; 3],
    hi: [f64; 3],
    range: f64,
) -> Option<[f64; 3]> {
    if !eye
        .iter()
        .chain(direction.iter())
        .chain(lo.iter())
        .chain(hi.iter())
        .chain([range].iter())
        .all(|n| n.is_finite())
        || range < 0.
        || (0..3).any(|i| lo[i] > hi[i])
    {
        return None;
    }
    let norm = direction.iter().map(|n| n * n).sum::<f64>().sqrt();
    if norm < 1.0e-12 {
        return None;
    }
    let d = direction.map(|n| n / norm);
    let (mut a, mut b) = (0f64, range);
    for i in 0..3 {
        if d[i].abs() < 1.0e-12 {
            if eye[i] < lo[i] || eye[i] > hi[i] {
                return None;
            }
        } else {
            let p = (lo[i] - eye[i]) / d[i];
            let q = (hi[i] - eye[i]) / d[i];
            a = a.max(p.min(q));
            b = b.min(p.max(q));
            if a > b {
                return None;
            }
        }
    }
    Some(std::array::from_fn(|i| eye[i] + d[i] * a))
}
