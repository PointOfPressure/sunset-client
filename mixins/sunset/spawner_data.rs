// Bounded spawner discovery state. No engine dependency: rustc --test this file.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};

pub type Pos = (i32, i32, i32);
pub const INDEX_LIMIT: usize = 16_384;
pub const SEEN_LIMIT: usize = 65_536;
pub const SCAN_BUDGET: usize = 256;
pub const LABEL_LIMIT: usize = 32;
pub const CHAT_EVERY: u64 = 40;

// Owned by BlockEntities, so server/client worlds never share an index.
#[derive(Default)]
pub struct Index {
    positions: BTreeSet<Pos>,
    pub saturated: bool,
}

impl Index {
    pub fn observe(&mut self, pos: Pos, spawner: bool) {
        if !spawner {
            self.positions.remove(&pos);
        } else if self.positions.len() < INDEX_LIMIT || self.positions.contains(&pos) {
            self.positions.insert(pos);
        } else {
            self.saturated = true;
        }
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.saturated = false;
    }

    // One logarithmic seek, then at most SCAN_BUDGET successors. No skip(n).
    pub fn batch(&self, after: Option<Pos>) -> Vec<Pos> {
        let mut out = Vec::new();
        self.fill_batch(after, &mut out);
        out
    }

    pub fn fill_batch(&self, after: Option<Pos>, out: &mut Vec<Pos>) {
        out.clear();
        match after {
            Some(pos) => out.extend(self.positions.range((Excluded(pos), Unbounded)).take(SCAN_BUDGET).copied()),
            None => out.extend(self.positions.iter().take(SCAN_BUDGET).copied()),
        }
    }
}

pub fn distance_squared(pos: Pos, at: [f64; 3]) -> f64 {
    [f64::from(pos.0) + 0.5, f64::from(pos.1) + 0.5, f64::from(pos.2) + 0.5]
        .iter().zip(at).map(|(p, a)| (p - a).powi(2)).sum()
}

pub fn within(pos: Pos, at: [f64; 3], range: f64) -> bool {
    range.is_finite() && range >= 0.0 && distance_squared(pos, at) <= range * range
}

// Only a supplied metadata identifier can name the mob. Never guess from nearby entities.
pub fn type_label(id: Option<&str>) -> String {
    let Some(id) = id.filter(|s| !s.is_empty() && s.len() <= 128) else { return "Unknown".into(); };
    let short = id.strip_prefix("minecraft:").unwrap_or(id);
    if short.is_empty() || !short.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b':' | b'/' | b'.' | b'-')) {
        return "Unknown".into();
    }
    let mut title = String::with_capacity(short.len());
    let mut capital = true;
    for c in short.chars() {
        if c == '_' {
            title.push(' ');
            capital = true;
        } else {
            title.push(if capital { c.to_ascii_uppercase() } else { c });
            capital = false;
        }
    }
    title
}

#[derive(Clone, Debug)]
pub struct Found {
    pub pos: Pos,
    pub name: String,
    mob: Option<Vec<u16>>,
    pub trial: bool,
    pub distance: f64,
}

impl Found {
    pub fn new(pos: Pos, mob: Option<&[u16]>, trial: bool, distance: f64) -> Self {
        let mut found = Self { pos, name: String::new(), mob: None, trial, distance };
        found.refresh(mob, trial);
        found
    }

    pub fn refresh(&mut self, mob: Option<&[u16]>, trial: bool) {
        let mob = mob.filter(|id| id.len() <= 128);
        if !self.name.is_empty() && self.mob.as_deref() == mob && self.trial == trial { return; }
        self.mob = mob.map(<[u16]>::to_vec);
        self.trial = trial;
        let id = mob.map(String::from_utf16_lossy);
        self.name = format!("{} {}Spawner", type_label(id.as_deref()), if trial { "Trial " } else { "" });
    }
}

#[derive(Default)]
pub struct Tracker {
    seen: BTreeMap<String, BTreeSet<Pos>>,
    seen_count: usize,
    dimension: Option<String>,
    pub cursor: Option<Pos>,
    next: Vec<Found>,
    pub labels: Vec<Found>,
    pending: Vec<Found>,
    pending_count: usize,
    last_chat: Option<u64>,
    pub seen_saturated: bool,
    pub warned: bool,
}

impl Tracker {
    // Disabled: preserve discoveries, clear transient output. Dimension revisit
    // keeps that dimension's dedupe; a connection reset replaces the Tracker.
    pub fn context(&mut self, dimension: &str, enabled: bool) {
        let changed = self.dimension.as_deref() != Some(dimension);
        if changed || !enabled {
            self.cursor = None;
            self.next.clear();
            self.labels.clear();
            self.pending.clear();
            self.pending_count = 0;
        }
        if changed { self.dimension = Some(dimension.to_owned()); }
    }

    pub fn observe(&mut self, found: Found, chat: bool) {
        // A bounded nearest list is accumulated during each pass, never sorted
        // over all loaded spawners or all historic discoveries.
        let i = self.next.partition_point(|old| old.distance <= found.distance);
        if i < LABEL_LIMIT {
            self.next.insert(i, found.clone());
            self.next.truncate(LABEL_LIMIT);
        }
        if self.known(found.pos) { return; }
        if self.seen_count >= SEEN_LIMIT {
            self.seen_saturated = true;
            return; // Fail closed: never evict and re-alert old positions.
        }
        self.seen.entry(self.dimension.clone().unwrap_or_default()).or_default().insert(found.pos);
        self.seen_count += 1;
        if chat {
            self.pending_count = self.pending_count.saturating_add(1);
            if self.pending.len() < 3 { self.pending.push(found); }
        }
    }

    fn known(&self, pos: Pos) -> bool {
        self.seen.get(self.dimension.as_deref().unwrap_or_default()).is_some_and(|seen| seen.contains(&pos))
    }

    // Known candidates outside the nearest list need no decoded label.
    pub fn needs(&self, pos: Pos, distance: f64) -> bool {
        !self.known(pos) || self.next.len() < LABEL_LIMIT || self.next.last().is_some_and(|f| distance < f.distance)
    }

    pub fn silence(&mut self) {
        self.pending.clear();
        self.pending_count = 0;
    }

    pub fn finish_pass(&mut self) {
        std::mem::swap(&mut self.labels, &mut self.next);
        self.next.clear();
        self.cursor = None;
    }

    pub fn chat(&mut self, tick: u64) -> Option<String> {
        if self.pending_count == 0 || self.last_chat.is_some_and(|last| tick.saturating_sub(last) < CHAT_EVERY) {
            return None;
        }
        self.last_chat = Some(tick);
        let mut message = self.pending.iter().map(|f| {
            format!("{} at {} {} {} ({:.0}m)", f.name, f.pos.0, f.pos.1, f.pos.2, f.distance)
        }).collect::<Vec<_>>().join("; ");
        if self.pending_count > self.pending.len() {
            message.push_str(&format!("; +{} more new spawners", self.pending_count - self.pending.len()));
        }
        self.pending.clear();
        self.pending_count = 0;
        Some(format!("Discovered {message}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn found(x: i32) -> Found {
        Found::new((x, 64, 0), None, false, f64::from(x.abs()))
    }
    #[test]
    fn range_uses_block_centers_and_three_dimensions() {
        assert!(within((0, 0, 0), [0.5, 0.5, 0.5], 0.0));
        assert!(within((3, 4, 0), [0.5, 0.5, 0.5], 5.0));
        assert!(!within((3, 4, 1), [0.5, 0.5, 0.5], 5.0));
        assert!(!within((0, 0, 0), [f64::NAN, 0.0, 0.0], 32.0));
        assert!(!within((0, 0, 0), [0.0; 3], -1.0));
        assert!(distance_squared((i32::MIN, 0, i32::MAX), [0.0; 3]).is_finite());
    }
    #[test]
    fn metadata_is_required_and_sanitized() {
        assert_eq!(type_label(None), "Unknown");
        assert_eq!(type_label(Some("")), "Unknown");
        assert_eq!(type_label(Some("minecraft:")), "Unknown");
        assert_eq!(type_label(Some("minecraft:spider")), "Spider");
        assert_eq!(type_label(Some("minecraft:cave_spider")), "Cave Spider");
        assert_eq!(type_label(Some("zombie\nSpoof")), "Unknown");
        assert_eq!(found(1).name, "Unknown Spawner");
    }
    #[test]
    fn index_budget_replacement_removal_and_cap() {
        let mut index = Index::default();
        for x in 0..INDEX_LIMIT as i32 + 10 { index.observe((x, 0, 0), true); }
        assert_eq!(index.positions.len(), INDEX_LIMIT);
        assert!(index.saturated);
        let mut all = Vec::new();
        let mut cursor = None;
        loop {
            let batch = index.batch(cursor);
            assert!(batch.len() <= SCAN_BUDGET);
            if batch.is_empty() { break; }
            cursor = batch.last().copied();
            all.extend(batch);
        }
        assert_eq!(all.len(), INDEX_LIMIT);
        assert_eq!(all.iter().copied().collect::<BTreeSet<_>>().len(), INDEX_LIMIT);
        index.observe((0, 0, 0), false);
        index.observe((INDEX_LIMIT as i32, 0, 0), true);
        assert_eq!(index.positions.len(), INDEX_LIMIT);
        index.clear();
        assert!(index.positions.is_empty());
        assert!(!index.saturated);
    }
    #[test]
    fn session_dimension_and_disabled_policy() {
        let mut tracker = Tracker::default();
        tracker.context("overworld", true);
        tracker.observe(found(1), true);
        tracker.observe(found(1), true);
        assert_eq!(tracker.pending_count, 1);
        assert!(tracker.chat(1).unwrap().contains("Unknown Spawner at 1 64 0 (1m)"));
        tracker.context("overworld", false);
        tracker.context("overworld", true);
        tracker.observe(found(1), true);
        assert!(tracker.chat(50).is_none());
        tracker.context("nether", true);
        tracker.observe(found(1), true);
        assert!(tracker.chat(50).is_some());
        tracker.context("overworld", true);
        tracker.observe(found(1), true);
        assert!(tracker.chat(100).is_none());
        tracker = Tracker::default();
        tracker.context("overworld", true);
        tracker.observe(found(1), true);
        assert!(tracker.chat(100).is_some());
    }
    #[test]
    fn nearest_labels_and_grouped_chat_stay_bounded() {
        let mut tracker = Tracker::default();
        tracker.context("overworld", true);
        for x in (1..=1000).rev() { tracker.observe(found(x), true); }
        assert_eq!(tracker.pending.len(), 3);
        tracker.finish_pass();
        assert_eq!(tracker.labels.len(), LABEL_LIMIT);
        assert_eq!(tracker.labels[0].pos.0, 1);
        assert_eq!(tracker.labels[LABEL_LIMIT - 1].pos.0, LABEL_LIMIT as i32);
        assert!(tracker.chat(10).unwrap().contains("+997 more new spawners"));
        tracker.observe(found(1001), true);
        assert!(tracker.chat(49).is_none());
        assert!(tracker.chat(50).is_some());
    }
    #[test]
    fn seen_limit_never_evicts_and_spams_old_positions() {
        let mut tracker = Tracker::default();
        tracker.context("overworld", true);
        for x in 0..=SEEN_LIMIT as i32 { tracker.observe(found(x), false); }
        assert_eq!(tracker.seen_count, SEEN_LIMIT);
        assert!(tracker.seen_saturated);
        tracker.observe(found(0), true);
        assert!(tracker.chat(10).is_none());
        tracker.observe(found(SEEN_LIMIT as i32), true);
        assert!(tracker.chat(100).is_none());
    }

    #[test]
    fn disabled_chat_discards_pending_and_label_refresh_uses_real_metadata() {
        let mut tracker = Tracker::default();
        tracker.context("overworld", true);
        tracker.observe(found(1), true);
        tracker.silence();
        tracker.observe(found(1), true);
        assert!(tracker.chat(100).is_none());
        let spider: Vec<_> = "minecraft:spider".encode_utf16().collect();
        let mut label = found(1);
        label.refresh(Some(&spider), false);
        assert_eq!(label.name, "Spider Spawner");
        label.refresh(None, false);
        assert_eq!(label.name, "Unknown Spawner");
        label.refresh(Some(&spider), true);
        assert_eq!(label.name, "Spider Trial Spawner");
    }
}
