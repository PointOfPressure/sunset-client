// Original bounded timing policy; independent of networking and rendering.
use std::collections::VecDeque;

pub(super) struct Entry<T> {
    pub packet: T,
    pub born: u64,
    pub due: u64,
    bytes: usize,
}
pub(super) struct Queue<T> {
    items: VecDeque<Entry<T>>,
    bytes: usize,
    count_limit: usize,
    byte_limit: usize,
    age_limit: u64,
    pub accepted: u64,
    pub released: u64,
    pub dropped: u64,
}
impl<T> Queue<T> {
    pub(super) fn new(count: usize, bytes: usize, age: u64) -> Self {
        Self {
            items: VecDeque::new(),
            bytes: 0,
            count_limit: count.min(64),
            byte_limit: bytes.min(262_144),
            age_limit: age.min(1000),
            accepted: 0,
            released: 0,
            dropped: 0,
        }
    }
    pub(super) fn len(&self) -> usize {
        self.items.len()
    }
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub(super) fn oldest_age(&self, now: u64) -> u64 {
        self.items.front().map_or(0, |e| now.saturating_sub(e.born))
    }
    pub(super) fn needs_flush(&self, now: u64, bytes: usize) -> bool {
        self.items.len() >= self.count_limit
            || bytes > self.byte_limit
            || self.bytes.saturating_add(bytes) > self.byte_limit
            || self
                .items
                .front()
                .is_some_and(|e| now < e.born || now.saturating_sub(e.born) >= self.age_limit)
    }
    pub(super) fn push(&mut self, packet: T, now: u64, wait: u64, bytes: usize) -> Result<(), T> {
        if self.needs_flush(now, bytes) || self.count_limit == 0 || self.age_limit == 0 {
            return Err(packet);
        }
        let due = now.saturating_add(wait.min(self.age_limit));
        let due = self.items.back().map_or(due, |last| last.due.max(due));
        self.items.push_back(Entry {
            packet,
            born: now,
            due,
            bytes,
        });
        self.bytes += bytes;
        self.accepted = self.accepted.saturating_add(1);
        Ok(())
    }
    pub(super) fn drain(&mut self, now: u64, all: bool) -> Vec<T> {
        let rewound = self
            .items
            .front()
            .is_some_and(|e| now < e.born);
        let mut result = Vec::new();
        while self
            .items
            .front()
            .is_some_and(|e| all || rewound || e.due <= now || now.saturating_sub(e.born) >= self.age_limit)
        {
            if result.is_empty() {
                result.reserve(self.items.len());
            }
            let item = self.items.pop_front().expect("front exists");
            self.bytes -= item.bytes;
            result.push(item.packet);
            self.released = self.released.saturating_add(1);
        }
        result
    }
    pub(super) fn clear(&mut self) {
        self.dropped = self.dropped.saturating_add(self.items.len() as u64);
        self.items.clear();
        self.bytes = 0;
    }
    // A correction invalidates old coordinates, while original frame markers
    // must still close the server's current position quota. No packet is made.
    pub(super) fn discard_except(&mut self, mut keep: impl FnMut(&T) -> bool) -> Vec<T> {
        let mut kept = Vec::with_capacity(self.items.len());
        while let Some(item) = self.items.pop_front() {
            self.bytes -= item.bytes;
            if keep(&item.packet) {
                kept.push(item.packet);
                self.released = self.released.saturating_add(1);
            } else {
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        kept
    }
}

pub(super) fn bounded(value: f64, minimum: u64, maximum: u64) -> u64 {
    if !value.is_finite() {
        return minimum;
    }
    (value.round().max(minimum as f64).min(maximum as f64)) as u64
}

pub(super) fn probability(chance: f64, sample: u64) -> bool {
    chance.is_finite()
        && chance > 0.0
        && (chance >= 100.0 || ((sample % 10_000) as f64) < chance * 100.0)
}

pub(super) fn uniform_between(first: f64, second: f64, sample: u64) -> u64 {
    let a = bounded(first, 0, 1000);
    let b = bounded(second, 0, 1000);
    let lo = a.min(b);
    let hi = a.max(b);
    lo + sample % (hi - lo + 1)
}

pub(super) fn approaching(
    eye: [f64; 3],
    look: [f64; 3],
    velocity: [f64; 3],
    target: [f64; 3],
    feet_distance: f64,
    range: f64,
) -> bool {
    if !eye
        .iter()
        .chain(look.iter())
        .chain(velocity.iter())
        .chain(target.iter())
        .all(|n| n.is_finite())
        || !feet_distance.is_finite()
        || !range.is_finite()
        || range < 0.0
        || feet_distance < 0.0
        || feet_distance > range
    {
        return false;
    }
    let delta: [f64; 3] = std::array::from_fn(|i| target[i] - eye[i]);
    let distance = delta.iter().map(|n| n * n).sum::<f64>().sqrt();
    let length = look.iter().map(|n| n * n).sum::<f64>().sqrt();
    if distance <= 1e-9 || !distance.is_finite() || length <= 1e-9 || !length.is_finite() {
        return false;
    }
    let direction = delta.map(|n| n / distance);
    let dot = (0..3).map(|i| look[i] / length * direction[i]).sum::<f64>();
    dot >= 30f64.to_radians().cos()
        && velocity[0] * direction[0] + velocity[2] * direction[2] > 0.02
}
