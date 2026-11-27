// Prediction filtering and a bounded per-job ledger. No engine, input or world access belongs here.

use super::sunset_ore_sim::{MiningQuery, PredictedOre, PredictionKey, PredictionSnapshot};
use std::collections::{BTreeMap, HashMap, HashSet};

const KINDS: [&str; 10] = ["coal", "copper", "iron", "lapis", "redstone", "gold", "diamond", "emerald", "quartz", "debris"];
pub(super) const MAX_TARGETS: usize = 4096;

pub(super) fn kind_name(kind: u8) -> Option<&'static str> {
    KINDS.get(usize::from(kind)).copied()
}

pub(super) fn matches_kind(kind: u8, state_name: &str) -> bool {
    let name = state_name.strip_prefix("minecraft:").unwrap_or(state_name);
    match kind {
        0 => matches!(name, "coal_ore" | "deepslate_coal_ore"),
        1 => matches!(name, "copper_ore" | "deepslate_copper_ore"),
        2 => matches!(name, "iron_ore" | "deepslate_iron_ore"),
        3 => matches!(name, "lapis_ore" | "deepslate_lapis_ore"),
        4 => matches!(name, "redstone_ore" | "deepslate_redstone_ore"),
        5 => matches!(name, "gold_ore" | "deepslate_gold_ore" | "nether_gold_ore"),
        6 => matches!(name, "diamond_ore" | "deepslate_diamond_ore"),
        7 => matches!(name, "emerald_ore" | "deepslate_emerald_ore"),
        8 => name == "nether_quartz_ore",
        9 => name == "ancient_debris",
        _ => false,
    }
}

// Geological cover may hide an ore. Fluids, air, another ore family and constructed blocks cannot verify it.
pub(super) fn plausible_state(kind: u8, state_name: &str) -> bool {
    if matches_kind(kind, state_name) { return true; }
    let name = state_name.strip_prefix("minecraft:").unwrap_or(state_name);
    match kind {
        0..=7 => matches!(name, "stone" | "deepslate" | "granite" | "diorite" | "andesite" | "tuff" | "netherrack"),
        8 | 9 => matches!(name, "netherrack" | "basalt" | "blackstone"),
        _ => false,
    }
}

pub(super) fn parse_kinds(text: &str) -> Result<u16, String> {
    let mut mask = 0;
    for word in text.split(',') {
        let word = word.trim().to_ascii_lowercase();
        if word.is_empty() { return Err("Choose at least one ore kind; separate names with commas.".into()); }
        let name = word.strip_prefix("minecraft:").unwrap_or(&word);
        let kind = KINDS.iter().position(|&kind| kind == name)
            .or_else(|| (0..KINDS.len()).find(|&kind| matches_kind(kind as u8, name)));
        let Some(kind) = kind else { return Err(format!("Unknown ore kind '{word}'. Use {}.", KINDS.join(","))); };
        mask |= 1 << kind;
    }
    Ok(mask)
}

pub(super) fn validated_query(
    anchor: [f64; 3], kind_mask: u16, min_y: i32, max_y: i32,
    radius: f64, limit: usize, build_min: i32, build_max: i32,
) -> Result<MiningQuery, String> {
    if !anchor.iter().all(|n| n.is_finite()) || !radius.is_finite() {
        return Err("Mining position and radius must be finite.".into());
    }
    if kind_mask == 0 || kind_mask & !0x03ff != 0 { return Err("Choose valid ore kinds first.".into()); }
    if min_y > max_y || build_min > build_max { return Err("Mining minimum Y must not exceed maximum Y.".into()); }
    let (min_y, max_y) = (min_y.max(build_min), max_y.min(build_max));
    if min_y > max_y { return Err("Mining Y range is outside this dimension's build range.".into()); }
    Ok(MiningQuery { anchor, kind_mask, min_y, max_y, radius: radius.clamp(8.0, 256.0), limit: limit.clamp(1, MAX_TARGETS) })
}

fn distance(pos: [i32; 3], feet: [f64; 3]) -> f64 {
    pos.into_iter().zip(feet).map(|(p, f)| (f64::from(p) + 0.5 - f).powi(2)).sum()
}

// The map never grows beyond the returned cap. Exclusions and live-state checks by the caller happen first.
pub(super) fn bounded_candidates(
    ores: impl IntoIterator<Item = PredictedOre>, query: &MiningQuery,
    feet: [f64; 3], excluded: &HashSet<[i32; 3]>, remaining: usize,
) -> Vec<PredictedOre> {
    let cap = query.limit.min(remaining).min(MAX_TARGETS);
    if cap == 0 || query.min_y > query.max_y || query.kind_mask == 0
        || !query.radius.is_finite() || query.radius <= 0.0
        || !query.anchor.iter().chain(feet.iter()).all(|n| n.is_finite())
    { return Vec::new(); }
    let radius_squared = query.radius.min(256.0).powi(2);
    let mut selected: BTreeMap<(u64, [i32; 3]), u8> = BTreeMap::new();
    for ore in ores {
        if ore.kind >= 10 || query.kind_mask & (1 << ore.kind) == 0 || excluded.contains(&ore.pos)
            || !(query.min_y..=query.max_y).contains(&ore.pos[1])
            || distance(ore.pos, query.anchor) > radius_squared
        { continue; }
        // Squared finite nonnegative f64 values sort in the same order as their bits. No i32 arithmetic can overflow.
        let d = distance(ore.pos, feet);
        if !d.is_finite() { continue; }
        selected.entry((d.to_bits(), ore.pos)).and_modify(|kind| *kind = (*kind).min(ore.kind)).or_insert(ore.kind);
        if selected.len() > cap { selected.pop_last(); }
    }
    selected.into_iter().map(|((_, pos), kind)| PredictedOre { pos, kind }).collect()
}

pub(super) fn ordered_candidates(
    snapshot: &PredictionSnapshot, query: &MiningQuery, feet: [f64; 3],
    excluded: &HashSet<[i32; 3]>, remaining: usize,
) -> Vec<PredictedOre> {
    bounded_candidates(snapshot.ores.iter().copied(), query, feet, excluded, remaining)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome { Mined, Skipped, Mismatch, Unconfirmed }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Counts {
    pub mined: usize, pub skipped: usize, pub mismatch: usize, pub unconfirmed: usize,
}

pub(super) struct TargetLedger {
    key: PredictionKey,
    cap: usize,
    attempted: HashSet<[i32; 3]>,
    terminal: HashMap<[i32; 3], Outcome>,
}

impl TargetLedger {
    pub(super) fn new(key: PredictionKey, cap: usize) -> Self {
        Self { key, cap: cap.clamp(1, MAX_TARGETS), attempted: HashSet::new(), terminal: HashMap::new() }
    }
    pub(super) fn key(&self) -> PredictionKey { self.key }
    pub(super) fn exclusions(&self) -> &HashSet<[i32; 3]> { &self.attempted }
    pub(super) fn remaining(&self) -> usize { self.cap.saturating_sub(self.attempted.len()) }
    pub(super) fn attempt(&mut self, pos: [i32; 3]) -> bool {
        self.remaining() > 0 && self.attempted.insert(pos)
    }
    // Late callbacks from an old replay cannot consume the new job's budget or change its counts.
    pub(super) fn finish(&mut self, key: PredictionKey, pos: [i32; 3], outcome: Outcome) -> bool {
        if key != self.key || !self.attempted.contains(&pos) || self.terminal.contains_key(&pos) { return false; }
        self.terminal.insert(pos, outcome);
        true
    }
    pub(super) fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for outcome in self.terminal.values() {
            match outcome {
                Outcome::Mined => counts.mined += 1,
                Outcome::Skipped => counts.skipped += 1,
                Outcome::Mismatch => counts.mismatch += 1,
                Outcome::Unconfirmed => counts.unconfirmed += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> MiningQuery { validated_query([0.5; 3], 0x03ff, -64, 319, 32.0, 64, -64, 319).unwrap() }
    fn key() -> PredictionKey { PredictionKey { world: 1, replay: 1, dimension: "minecraft:overworld" } }

    #[test]
    fn baritone_all_kind_families_are_exact() {
        for (kind, name) in KINDS.iter().enumerate() {
            assert_eq!(kind_name(kind as u8), Some(*name));
            assert_eq!(parse_kinds(name).unwrap(), 1 << kind);
        }
        for name in ["coal", "copper", "iron", "lapis", "redstone", "gold", "diamond", "emerald"] {
            assert_eq!(parse_kinds(&format!("minecraft:deepslate_{name}_ore")).unwrap(), parse_kinds(name).unwrap());
        }
        assert_eq!(parse_kinds("nether_gold_ore,nether_quartz_ore,ancient_debris").unwrap(), (1 << 5) | (1 << 8) | (1 << 9));
        for invalid in ["", ",", "diamond,", "diamond,typo", "other:diamond_ore", "diamond_ore[lit=true]"] { assert!(parse_kinds(invalid).is_err()); }
        assert!(!matches_kind(6, "minecraft:iron_ore"));
        assert!(!matches_kind(6, "other:diamond_ore"));
        assert!(plausible_state(6, "minecraft:deepslate"));
        for name in ["air", "water", "lava", "oak_planks", "iron_ore"] { assert!(!plausible_state(6, name)); }
    }

    #[test]
    fn baritone_candidates_dedupe_filter_before_cap_and_keep_anchor() {
        let mut query = query();
        query.limit = 2;
        let excluded = HashSet::from([[-1, 0, 0], [0, 0, 0]]);
        let ores = [([-1, 0, 0], 6), ([0, 0, 0], 6), ([1, 0, 0], 6), ([1, 0, 0], 2), ([2, 0, 0], 6), ([40, 0, 0], 6)];
        let snapshot = PredictionSnapshot { key: key(), revision: 1, pending: false, completed_chunks: 2,
            ores: ores.into_iter().map(|(pos, kind)| PredictedOre { pos, kind }).collect() };
        let chosen = ordered_candidates(&snapshot, &query, [40.5, 0.5, 0.5], &excluded, 2);
        assert_eq!(chosen, [PredictedOre { pos: [2, 0, 0], kind: 6 }, PredictedOre { pos: [1, 0, 0], kind: 2 }]);
        assert_eq!(ordered_candidates(&snapshot, &query, [0.5; 3], &excluded, 1).len(), 1);
    }

    #[test]
    fn baritone_order_is_stable_at_negative_coords_and_extreme_values() {
        let query = query();
        let ores = [[1, 0, 0], [0, 0, -1], [-1, 0, 0], [0, 0, 1], [i32::MAX, 0, i32::MIN]];
        let selected = bounded_candidates(ores.map(|pos| PredictedOre { pos, kind: 6 }), &query, [0.5; 3], &HashSet::new(), 64);
        assert_eq!(selected.iter().map(|o| o.pos).collect::<Vec<_>>(), [[-1, 0, 0], [0, 0, -1], [0, 0, 1], [1, 0, 0]]);
        assert_eq!(selected, bounded_candidates(ores.into_iter().rev().map(|pos| PredictedOre { pos, kind: 6 }), &query, [0.5; 3], &HashSet::new(), 64));
        let query = validated_query([f64::MAX; 3], 1, 0, 1, 8.0, 1, -64, 319).unwrap();
        assert!(bounded_candidates([PredictedOre { pos: [i32::MAX; 3], kind: 0 }], &query, [0.0; 3], &HashSet::new(), 1).is_empty());
    }

    #[test]
    fn baritone_limits_are_absolute_and_dimension_bounded() {
        let q = validated_query([0.0; 3], 64, -64, 319, 9999.0, usize::MAX, 0, 255).unwrap();
        assert_eq!((q.min_y, q.max_y, q.radius, q.limit), (0, 255, 256.0, 4096));
        assert!(validated_query([0.0; 3], 64, 100, 0, 32.0, 1, -64, 319).is_err());
        assert!(validated_query([0.0; 3], 64, -64, -1, 32.0, 1, 0, 255).is_err());
        assert!(validated_query([f64::NAN; 3], 64, 0, 1, 32.0, 1, 0, 255).is_err());
        assert!(validated_query([0.0; 3], 0, 0, 1, 32.0, 1, 0, 255).is_err());
        let query = validated_query([0.5, 15.5, 0.5], 64, 10, 20, 8.0, 64, -64, 319).unwrap();
        let positions = [[0, 10, 0], [0, 20, 0], [0, 9, 0], [0, 21, 0], [8, 15, 0], [8, 16, 0]];
        let actual = bounded_candidates(positions.map(|pos| PredictedOre { pos, kind: 6 }), &query, query.anchor, &HashSet::new(), 64);
        assert_eq!(actual.into_iter().map(|ore| ore.pos).collect::<Vec<_>>(), [[0, 10, 0], [0, 20, 0], [8, 15, 0]]);
    }

    #[test]
    fn baritone_ledger_bounds_whole_run_and_rejects_late_results() {
        let mut ledger = TargetLedger::new(key(), 2);
        assert_eq!(ledger.key(), key());
        assert!(ledger.attempt([0; 3]));
        assert!(!ledger.attempt([0; 3]));
        assert!(!ledger.finish(PredictionKey { replay: 2, ..key() }, [0; 3], Outcome::Mined));
        assert!(ledger.finish(key(), [0; 3], Outcome::Mismatch));
        assert!(!ledger.finish(key(), [0; 3], Outcome::Mined));
        assert!(ledger.attempt([1; 3]));
        assert!(ledger.finish(key(), [1; 3], Outcome::Mined));
        assert!(!ledger.attempt([2; 3]));
        assert_eq!(ledger.remaining(), 0);
        assert_eq!(ledger.exclusions().len(), 2);
        assert_eq!(ledger.counts(), Counts { mined: 1, mismatch: 1, ..Counts::default() });
        let mut ledger = TargetLedger::new(key(), 2);
        for (i, outcome) in [Outcome::Skipped, Outcome::Unconfirmed].into_iter().enumerate() {
            assert!(ledger.attempt([i as i32; 3]));
            assert!(ledger.finish(key(), [i as i32; 3], outcome));
        }
        assert_eq!(ledger.counts(), Counts { skipped: 1, unconfirmed: 1, ..Counts::default() });
    }
}
