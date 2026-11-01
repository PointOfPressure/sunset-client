// Original inventory transaction policy. Predictions are never acknowledgements.

pub(super) const OFFHAND: usize = 504;

pub(super) fn player_wire(slot: usize) -> Option<usize> {
    match slot { 0..=8 => Some(36 + slot), 9..=35 => Some(slot), OFFHAND => Some(45), _ => None }
}

pub(super) fn player_internal(slot: i32) -> Option<usize> {
    match slot { 0..=35 => Some(slot as usize), 40 => Some(OFFHAND), _ => None }
}

#[derive(Clone, Debug)]
pub(super) struct Swap<T> {
    pub slots: [usize; 2],
    pub expected: [Option<T>; 2],
    pub sent: u64,
    seen: u8,
    rejected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status { Waiting, Confirmed, Rejected, Expired }

impl<T: Clone + PartialEq> Swap<T> {
    pub(super) fn new(slots: [usize; 2], before: [Option<T>; 2], sent: u64) -> Option<Self> {
        if slots[0] == slots[1] || before[0].is_none() || before[0] == before[1] { return None; }
        Some(Self { slots, expected: [before[1].clone(), before[0].clone()], sent, seen: 0, rejected: false })
    }

    pub(super) fn observe(&mut self, slot: usize, server_stack: &Option<T>) {
        for i in 0..2 {
            if self.slots[i] != slot { continue; }
            if server_stack == &self.expected[i] { self.seen |= 1 << i; }
            else { self.rejected = true; }
        }
    }

    pub(super) fn reject(&mut self) { self.rejected = true; }
    pub(super) fn confirmations(&self) -> u8 { self.seen }

    pub(super) fn status(&self, now: u64, timeout: u64) -> Status {
        if self.rejected { Status::Rejected }
        else if now < self.sent || now.saturating_sub(self.sent) >= timeout { Status::Expired }
        else if self.seen == 3 { Status::Confirmed }
        else { Status::Waiting }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Failure<T> { pub slots: [usize; 2], pub before: [Option<T>; 2] }
impl<T: PartialEq> Failure<T> {
    pub(super) fn blocks(&self, slots: [usize; 2], before: &[Option<T>; 2]) -> bool {
        self.slots == slots && &self.before == before
    }
}

#[derive(Default)]
pub(super) struct Edge { down: bool }
impl Edge {
    pub(super) fn update(&mut self, pressed: bool) -> bool {
        let rising = pressed && !self.down;
        self.down = pressed;
        rising
    }
}

pub(super) fn delay(value: f64, maximum: u64) -> u64 {
    if value.is_finite() && value >= 0.0 { (value as u64).min(maximum) } else { maximum }
}

/// Ten native ticks, including the press tick; a paused/cooldown attempt is
/// retained, while selection/world changes are handled by its owner.
pub(super) fn press_alive(pressed: u64, now: u64) -> bool {
    now >= pressed && now - pressed < 10
}

pub(super) fn finish_delay(hotbar: bool, close: f64, switch_back: f64) -> u64 {
    delay(if hotbar { switch_back } else { close }, 10)
}
pub(super) fn confirmation_ticks(latency_ms:Option<i32>)->u64 {
    let millis=(u64::from(latency_ms.unwrap_or(0).max(0) as u32).saturating_mul(3).saturating_add(250)).clamp(750,5000);
    millis.div_ceil(50)
}
pub(super) fn revision_current(previous:i32,incoming:i32)->bool {
    if (0..32768).contains(&previous) && (0..32768).contains(&incoming){
        (incoming-previous).rem_euclid(32768)<16384
    }else{incoming>=previous}
}
#[derive(Default)]
pub(super) struct RefillRetry {pub attempts:u8,retry_at:u64}
impl RefillRetry {
    pub(super) fn ready(&self,now:u64)->bool{self.attempts<3 && now>=self.retry_at}
    pub(super) fn sent(&mut self){self.attempts=self.attempts.saturating_add(1).min(3);}
    pub(super) fn failed(&mut self,now:u64,latency:Option<i32>){
        self.retry_at=now.saturating_add(u64::from(latency.unwrap_or(0).clamp(100,500) as u32)*1000);
    }
    pub(super) fn reset(&mut self){*self=Self::default();}
}

#[derive(Clone, Copy, Debug, Default)]
enum RefillStage { #[default] Idle, Notice(u64), Target(u64, usize) }
#[derive(Default)]
pub(super) struct RefillPace { stage: RefillStage, tempo: f64 }
impl RefillPace {
    pub(super) fn clear(&mut self) { self.stage = RefillStage::Idle; }
    /// Millisecond timing is sampled in microseconds, independent of tick rate.
    /// The two uniforms produce a central bias, rather than fixed tick sleeps.
    pub(super) fn step(&mut self, now: u64, speed: u64, candidate: Option<usize>,
        valid: impl Fn(usize)->bool, random: [u64;3]) -> Option<usize> {
        let speed=speed.clamp(1,10);
        if speed==1 {self.clear();return candidate.filter(|s|valid(*s));}
        let unit=|n:u64|(n%1_000_000) as f64/1_000_000.0;
        let sample=|lo:f64,hi:f64,tempo:f64|((lo+(hi-lo)*(unit(random[1])+unit(random[2]))*0.5)*tempo*1000.0).round() as u64;
        // Observed notice and target timing envelopes in milliseconds.
        const NOTICE:[[f64;2];9]=[[18.,33.],[26.,46.],[34.,59.],[42.,72.],[50.,85.],[58.,98.],[66.,111.],[74.,124.],[82.,137.]];
        const TARGET:[[f64;2];9]=[[8.5,16.5],[11.,21.],[13.5,25.5],[16.,30.],[18.5,34.5],[21.,39.],[23.5,43.5],[26.,48.],[28.5,52.5]];
        match self.stage {
            RefillStage::Idle=>{
                candidate?;self.tempo=0.94+unit(random[0])*0.12;
                let [lo,hi]=NOTICE[(speed-2) as usize];
                self.stage=RefillStage::Notice(now.saturating_add(sample(lo,hi,self.tempo)));None
            }
            RefillStage::Notice(due) if now>=due=>{
                let Some(source)=candidate else {self.clear();return None;};
                let [lo,hi]=TARGET[(speed-2) as usize];
                self.stage=RefillStage::Target(now.saturating_add(sample(lo,hi,self.tempo)),source);None
            }
            RefillStage::Target(due,source) if now>=due=>{
                if valid(source){self.clear();Some(source)}else{
                    self.stage=RefillStage::Notice(now.saturating_add(sample(6.,14.,self.tempo)));None
                }
            }
            _=>None,
        }
    }
}

pub(super) fn nearby_source(candidates:&mut [usize],previous:Option<usize>,sample:u64)->Option<usize>{
    if candidates.is_empty(){return None;}
    let anchor=previous.filter(|i|(9..36).contains(i)).map_or((sample%27) as usize,|i|i-9);
    candidates.sort_by_key(|i|{let at=i-9;(at%9).abs_diff(anchor%9)+(at/9).abs_diff(anchor/9)});
    Some(candidates[(sample as usize)%candidates.len().min(3)])
}

#[derive(Default)]
pub(super) struct Hand {
    pub held: Option<(usize, usize)>,
    safe: u8,
    overridden: bool,
}
impl Hand {
    pub(super) fn tick(&mut self, danger: bool, selected: usize, main_totem: bool,
        candidate: Option<usize>, can_change: bool) -> Option<usize> {
        self.safe = if danger { 0 } else { self.safe.saturating_add(1).min(10) };
        if self.held.is_some_and(|(_, ours)| selected != ours && !main_totem) {
            self.held = None; self.overridden = true;
        }
        if self.safe == 10 {
            self.overridden = false;
            if can_change { return self.restore(selected); }
        }
        if !danger || !can_change || self.overridden { return None; }
        if let Some((before, ours)) = self.held {
            if main_totem { return None; }
            if let Some(next) = candidate.filter(|s| *s < 9) {
                self.held = Some((before, next));
                return (ours != next).then_some(next);
            }
            return None;
        }
        let next = if main_totem { Some(selected) } else { candidate }.filter(|s| *s < 9)?;
        self.held = Some((selected, next));
        (next != selected).then_some(next)
    }

    pub(super) fn restore(&mut self, selected: usize) -> Option<usize> {
        self.held.take().and_then(|(before, ours)| (selected == ours && before != selected).then_some(before))
    }
}
