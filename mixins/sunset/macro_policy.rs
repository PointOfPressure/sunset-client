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
    pub refilled: bool,
}
impl Sequence {
    pub(super) fn new(kind: Kind, first: Step, double: bool) -> Self {
        Self {
            kind,
            step: first,
            remaining: if kind == Kind::Anchor && double { 2 } else { 1 },
            refilled:false,
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
            Step::AnchorPlace if self.refilled => Step::Charge,
            Step::AnchorPlace => Step::Guard,
            Step::Guard => Step::Charge,
            Step::Charge => Step::Detonate,
            Step::Detonate if self.remaining > 1 => {
                self.remaining -= 1;
                self.refilled=true;
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
pub(super) fn wait(sent: u64, now: u64, observed: bool, timeout:u64) -> Wait {
    if now < sent {
        Wait::Expired
    } else if observed {
        Wait::Acknowledged
    } else if now - sent >= timeout {
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
pub(super) fn human_gap(sample:u64)->u64 {
    match sample%100 {0..=69=>2,70..=94=>3,_=>4}
}
pub(super) fn action_due(last:Option<u64>,now:u64,delay:f64,gap:u64)->bool {
    if !delay.is_finite() || !(0.0..=10.0).contains(&delay){return false;}
    last.is_none_or(|at|now>=at && now-at>delay as u64 && now-at>=gap)
}
pub(super) fn double_eligible(requested:bool,face:i32,anchor_y:i32,feet_floor:i32,
    replacement:bool,glowstone:bool)->bool {
    requested && face==1 && anchor_y>=feet_floor && replacement && glowstone
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

// An explicit-hand operation can still require borrowing another main slot.
// Facts are copied from the native catalog/stack before this bounded decision.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct UseRoute {pub select:Option<usize>,pub offhand:bool}
pub(super) fn placement_route(current:usize,preferred:usize,hotbar:[bool;9],
    through:[bool;9],offhand:bool)->Option<UseRoute> {
    if current>=9{return None;}
    if hotbar[current]{return Some(UseRoute{select:None,offhand:false});}
    if offhand && through[current]{return Some(UseRoute{select:None,offhand:true});}
    if let Some(slot)=hotbar.iter().position(|present|*present){
        return Some(UseRoute{select:Some(slot),offhand:false});
    }
    if !offhand{return None;}
    let selected=if preferred<9 && through[preferred]{Some(preferred)}else{
        through.iter().position(|allowed|*allowed)
    }?;
    Some(UseRoute{select:(selected!=current).then_some(selected),offhand:true})
}
#[derive(Clone,Copy,Debug,Default)]
pub(super) struct ClickFacts {
    pub empty:bool,pub known:bool,pub consuming:bool,pub blocking:bool,
    pub kinetic:bool,pub equip_swap:bool,pub sword:bool,pub axe:bool,
    pub mace:bool,pub totem:bool,
}
pub(super) fn click_through(f:ClickFacts,crystal_base:bool)->bool {
    if f.empty{return true;}
    f.known && !f.consuming && !f.blocking && !f.kinetic && !f.equip_swap
        && (f.sword || f.mace || f.totem || (f.axe && crystal_base))
}
pub(super) fn anchor_supplied(charged:bool,glowstone:bool,detonator:bool)->bool {
    detonator && (charged || glowstone)
}

// Counts are bounded by the native exposure sampler. Unknown geometry never
// enters either count; callers must abort its guard decision instead.
pub(super) fn guard_already_covered(total:usize,open:usize)->bool {
    total>0 && total<=256 && open<=total && open*100<total*15
}
pub(super) fn guard_shade_needed(open:usize)->Option<usize> {
    (open>0 && open<=256).then(||(open*15).div_ceil(100))
}
