// The inventory modules' rules that need no world (Rust's PR #16): which slots a swap touches on the wire, when the
// server has confirmed a swap, a key's press edge, Double Hand's hold on the hand.

// The offhand's slot number in Sunset's hotbar/inventory numbering (util::OFFHAND).
pub(super) const OFFHAND: usize = 504;

// The player inventory menu's slot for an inventory slot.
pub(super) fn player_wire(slot: usize) -> Option<usize> {
    match slot {
        0..=8 => Some(36 + slot),
        9..=35 => Some(slot),
        OFFHAND => Some(45),
        _ => None,
    }
}

// The inventory slot a SetPlayerInventory packet's slot is.
pub(super) fn player_internal(slot: i32) -> Option<usize> {
    match slot {
        0..=35 => Some(slot as usize),
        40 => Some(OFFHAND),
        _ => None,
    }
}

// A swap of two slots sent to the server: done once the server says each holds what the other held.
#[derive(Clone, Debug)]
pub(super) struct Swap<T> {
    slots: [usize; 2],
    expected: [Option<T>; 2],
    sent: u64,
    seen: u8,
    rejected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Waiting,
    Confirmed,
    Rejected,
    Expired,
}

impl<T: Clone + PartialEq> Swap<T> {
    pub(super) fn new(slots: [usize; 2], before: [Option<T>; 2], sent: u64) -> Option<Self> {
        if slots[0] == slots[1] || before[0].is_none() || before[0] == before[1] {
            return None;
        }
        Some(Self { slots, expected: [before[1].clone(), before[0].clone()], sent, seen: 0, rejected: false })
    }
    // What the server says `slot` holds.
    pub(super) fn observe(&mut self, slot: usize, server_stack: &Option<T>) {
        for i in 0..2 {
            if self.slots[i] == slot {
                if server_stack == &self.expected[i] {
                    self.seen |= 1 << i;
                } else {
                    self.rejected = true;
                }
            }
        }
    }
    pub(super) fn reject(&mut self) {
        self.rejected = true;
    }
    pub(super) fn status(&self, now: u64, timeout: u64) -> Status {
        if self.rejected {
            Status::Rejected
        } else if now < self.sent || now.saturating_sub(self.sent) >= timeout {
            Status::Expired
        } else if self.seen == 3 {
            Status::Confirmed
        } else {
            Status::Waiting
        }
    }
}

// A swap the server refused: not sent again while the slots hold the same.
#[derive(Clone, Debug)]
pub(super) struct Failure<T> {
    pub slots: [usize; 2],
    pub before: [Option<T>; 2],
}

impl<T: PartialEq> Failure<T> {
    pub(super) fn blocks(&self, slots: [usize; 2], before: &[Option<T>; 2]) -> bool {
        self.slots == slots && &self.before == before
    }
}

// A key going down (not held).
#[derive(Default)]
pub(super) struct Edge {
    down: bool,
}

impl Edge {
    pub(super) fn update(&mut self, pressed: bool) -> bool {
        let rising = pressed && !self.down;
        self.down = pressed;
        rising
    }
}

// A delay setting in ticks, at most `maximum`.
pub(super) fn delay(value: f64, maximum: u64) -> u64 {
    if value.is_finite() && value >= 0.0 { (value as u64).min(maximum) } else { maximum }
}

// Key Throw's press lasts 10 ticks, the press tick included.
pub(super) fn press_alive(pressed: u64, now: u64) -> bool {
    now >= pressed && now - pressed < 10
}

// How long to wait for the server to confirm a swap: three times the ping plus 250 ms, from 0.75 to 5 s, in ticks.
pub(super) fn confirmation_ticks(latency_ms: Option<i32>) -> u64 {
    let millis = (u64::from(latency_ms.unwrap_or(0).max(0) as u32).saturating_mul(3).saturating_add(250)).clamp(750, 5000);
    millis.div_ceil(50)
}

// A menu's state id that is not older than the last one (it counts up and wraps at 32768).
pub(super) fn revision_current(previous: i32, incoming: i32) -> bool {
    if (0..32768).contains(&previous) && (0..32768).contains(&incoming) {
        (incoming - previous).rem_euclid(32768) < 16384
    } else {
        incoming >= previous
    }
}

// Double Hand: a hotbar totem held in the main hand while in danger, and the slot it came from, given back after 10
// safe ticks. Changing slot yourself ends it.
#[derive(Default)]
pub(super) struct Hand {
    pub held: Option<(usize, usize)>,
    safe: u8,
    overridden: bool,
}

impl Hand {
    pub(super) fn tick(&mut self, danger: bool, selected: usize, main_totem: bool, candidate: Option<usize>, can_change: bool) -> Option<usize> {
        self.safe = if danger { 0 } else { self.safe.saturating_add(1).min(10) };
        if self.held.is_some_and(|(_, ours)| selected != ours && !main_totem) {
            self.held = None;
            self.overridden = true;
        }
        if self.safe == 10 {
            self.overridden = false;
            if can_change {
                return self.restore(selected);
            }
        }
        if !danger || !can_change || self.overridden {
            return None;
        }
        if let Some((before, ours)) = self.held {
            if main_totem {
                return None;
            }
            let next = candidate.filter(|s| *s < 9)?;
            self.held = Some((before, next));
            return (ours != next).then_some(next);
        }
        let next = if main_totem { Some(selected) } else { candidate }.filter(|s| *s < 9)?;
        self.held = Some((selected, next));
        (next != selected).then_some(next)
    }
    pub(super) fn restore(&mut self, selected: usize) -> Option<usize> {
        self.held.take().and_then(|(before, ours)| (selected == ours && before != selected).then_some(before))
    }
}
