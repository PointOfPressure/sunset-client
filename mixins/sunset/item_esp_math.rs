// Bounded selection, exact registry ID filtering and label layout. No game state is retained.
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt::Write as _;

pub(super) const RESULT_LIMIT: usize = 1024;
pub(super) const LABEL_LIMIT: usize = 64;

#[derive(Default)]
pub(super) struct Filter {
    source: String,
    ids: Vec<String>,
}

impl Filter {
    // IDs are parsed only when the setting changes. Unqualified IDs mean minecraft,
    // never a suffix match in another namespace. Unknown valid IDs remain exact IDs.
    pub(super) fn update(&mut self, source: &str) {
        if self.source == source {
            return;
        }
        self.source.clear();
        self.source.push_str(source);
        self.ids.clear();
        for token in source.split(|c: char| c == ',' || c.is_whitespace()) {
            let token = token.to_ascii_lowercase();
            let (namespace, path) = token.split_once(':').unwrap_or(("minecraft", &token));
            let part = |s: &str, slash: bool| {
                !s.is_empty()
                    && s.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || b"_.-".contains(&b)
                            || (slash && b == b'/')
                    })
            };
            if part(namespace, false) && part(path, true) {
                self.ids.push(format!("{namespace}:{path}"));
            }
        }
        self.ids.sort_unstable();
        self.ids.dedup();
    }

    pub(super) fn allows(&self, mode: &str, id: &str) -> bool {
        if mode == "All" {
            return true;
        }
        let found = self.ids.binary_search_by(|s| s.as_str().cmp(id)).is_ok();
        match mode {
            "Include" => found,
            "Exclude" => !found,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Candidate {
    pub(super) distance_sq: f64,
    pub(super) index: usize,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance_sq
            .total_cmp(&other.distance_sq)
            .then_with(|| self.index.cmp(&other.index))
    }
}

#[derive(Default)]
pub(super) struct Nearest {
    heap: BinaryHeap<Candidate>,
    pub(super) sorted: Vec<Candidate>,
    limit: usize,
}

impl Nearest {
    pub(super) fn begin(&mut self, limit: usize) {
        self.heap.clear();
        self.sorted.clear();
        self.limit = limit.min(RESULT_LIMIT);
        // Reuse both allocations across frames, even when the world is empty or changes.
        if self.heap.capacity() < self.limit {
            self.heap.reserve(self.limit);
        }
        if self.sorted.capacity() < self.limit {
            self.sorted.reserve(self.limit);
        }
    }

    pub(super) fn consider(&mut self, distance_sq: f64, index: usize) {
        if !self.can_improve(distance_sq, index) {
            return;
        }
        let candidate = Candidate { distance_sq, index };
        if self.heap.len() < self.limit {
            self.heap.push(candidate);
        } else if self.heap.peek().is_some_and(|last| candidate < *last) {
            *self.heap.peek_mut().unwrap() = candidate;
        }
    }

    // Check before doing more expensive projection work for candidates which cannot enter
    // the bounded result set. Rejected geometry must never occupy the heap in the first place.
    pub(super) fn can_improve(&self, distance_sq: f64, index: usize) -> bool {
        self.limit > 0
            && distance_sq.is_finite()
            && distance_sq >= 0.0
            && (self.heap.len() < self.limit
                || self
                    .heap
                    .peek()
                    .is_some_and(|last| Candidate { distance_sq, index } < *last))
    }

    pub(super) fn finish(&mut self) {
        self.sorted.extend(self.heap.drain());
        self.sorted.sort_unstable();
    }
}

pub(super) fn live_stack(health: i32, count: u32, id: &str) -> bool {
    health > 0 && count > 0 && id != "minecraft:air" && !id.is_empty()
}

// Inclusive range, including the vertical axis. Invalid positions never reach projection/drawing.
pub(super) fn range_sq(at: [f64; 3], eye: [f64; 3], range: f64) -> Option<f64> {
    if !range.is_finite() || range <= 0.0 {
        return None;
    }
    let d = (at[0] - eye[0]).powi(2) + (at[1] - eye[1]).powi(2) + (at[2] - eye[2]).powi(2);
    (d.is_finite() && d <= range * range).then_some(d)
}

// Strip formatting/control characters before passing names to the HUD; bound custom names by
// Unicode characters, never by byte offsets. Count is separate and cannot be truncated away.
pub(super) fn label_into(name: &str, id: &str, count: u32, out: &mut String) {
    fn append(text: &str, out: &mut String) {
        let mut chars = text.chars();
        let mut length = 0;
        let mut space = false;
        while let Some(c) = chars.next() {
            if c == '\u{a7}' {
                chars.next();
                continue;
            }
            if c.is_whitespace() {
                space = !out.is_empty();
                continue;
            }
            if c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
            {
                continue;
            }
            if length >= 48 {
                out.push('\u{2026}');
                break;
            }
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
            length += 1;
        }
    }
    out.clear();
    append(name, out);
    if out.is_empty() {
        append(id, out);
    }
    if count > 1 {
        let _ = write!(out, " x{count}");
    }
}

// Keep the nearest labels first. Try at most four short upward offsets; omit the label if the
// space is occupied or the viewport is too small, rather than filling the screen with text.
pub(super) fn label_rect(
    anchor: [f32; 2],
    width: f32,
    viewport: [f32; 2],
    occupied: &[[f32; 4]],
) -> Option<[f32; 4]> {
    if !anchor
        .iter()
        .chain(viewport.iter())
        .chain([width].iter())
        .all(|v| v.is_finite())
        || width < 0.0
        || width + 8.0 > viewport[0]
        || viewport[1] < 18.0
    {
        return None;
    }
    let x = (anchor[0] - width / 2.0 - 2.0).clamp(2.0, viewport[0] - width - 6.0);
    for step in 0..4 {
        let r = [x, anchor[1] - 15.0 - step as f32 * 14.0, width + 4.0, 12.0];
        if r[1] < 2.0 || r[1] + r[3] > viewport[1] - 2.0 {
            continue;
        }
        if !occupied.iter().any(|b| {
            r[0] < b[0] + b[2] + 2.0
                && r[0] + r[2] + 2.0 > b[0]
                && r[1] < b[1] + b[3] + 2.0
                && r[1] + r[3] + 2.0 > b[1]
        }) {
            return Some(r);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_namespaces_unknown_ids_and_invalid_tokens_are_exact() {
        let mut f = Filter::default();
        f.update("Diamond, minecraft:diamond, example:ore/gem UNKNOWN:future bad:id:extra #tag * :empty ns:");
        assert_eq!(
            f.ids,
            ["example:ore/gem", "minecraft:diamond", "unknown:future"]
        );
        for id in ["minecraft:diamond", "example:ore/gem", "unknown:future"] {
            assert!(f.allows("Include", id));
            assert!(!f.allows("Exclude", id));
        }
        for id in [
            "mod:diamond",
            "minecraft:diamond_ore",
            "minecraft:future",
            "minecraft:ore/gem",
        ] {
            assert!(!f.allows("Include", id));
            assert!(f.allows("Exclude", id));
            assert!(f.allows("All", id));
        }
        f.update("bad:id:extra");
        assert!(!f.allows("Include", "minecraft:diamond"));
        assert!(f.allows("Exclude", "minecraft:diamond"));
        f.update("");
        assert!(f.ids.is_empty());
    }

    #[test]
    fn unchanged_filters_keep_the_parsed_allocation() {
        let mut f = Filter::default();
        f.update("diamond");
        let pointer = f.ids[0].as_ptr();
        for _ in 0..100 {
            f.update("diamond");
        }
        assert_eq!(f.ids[0].as_ptr(), pointer);
        f.update("stone");
        assert!(f.allows("Include", "minecraft:stone"));
        assert!(!f.allows("Include", "minecraft:diamond"));
    }

    #[test]
    fn selection_is_bounded_nearest_first_with_stable_ties() {
        let mut n = Nearest::default();
        n.begin(3);
        for (i, distance) in [9., 1., 4., 1., 0., 1.].into_iter().enumerate() {
            n.consider(distance, i);
        }
        n.consider(f64::NAN, 99);
        n.consider(-1., 100);
        assert_eq!(n.heap.len(), 3);
        n.finish();
        assert_eq!(
            n.sorted.iter().map(|c| c.index).collect::<Vec<_>>(),
            [4, 1, 3]
        );
        let capacities = (n.heap.capacity(), n.sorted.capacity());
        n.begin(3);
        n.consider(5., 8);
        n.finish();
        assert_eq!(n.sorted.len(), 1);
        assert_eq!(n.sorted[0].index, 8);
        assert_eq!(capacities, (n.heap.capacity(), n.sorted.capacity()));
        n.begin(0);
        n.consider(0., 0);
        n.finish();
        assert!(n.sorted.is_empty());
        n.begin(usize::MAX);
        for i in 0..10_000 {
            n.consider(i as f64, i);
        }
        n.finish();
        assert_eq!(n.sorted.len(), RESULT_LIMIT);
        assert_eq!(n.sorted.last().unwrap().index, RESULT_LIMIT - 1);
    }

    #[test]
    fn range_includes_boundary_and_height_but_rejects_nonfinite_input() {
        assert_eq!(range_sq([3., 4., 0.], [0.; 3], 5.), Some(25.));
        assert!(range_sq([0., 5.001, 0.], [0.; 3], 5.).is_none());
        assert!(range_sq([f64::NAN, 0., 0.], [0.; 3], 5.).is_none());
        assert!(range_sq([0.; 3], [f64::INFINITY; 3], 5.).is_none());
        assert!(range_sq([0.; 3], [0.; 3], 0.).is_none());
    }

    #[test]
    fn dead_empty_and_air_stacks_do_not_reach_selection() {
        assert!(live_stack(5, 1, "minecraft:diamond"));
        assert!(!live_stack(0, 64, "minecraft:diamond"));
        assert!(!live_stack(-1, 64, "minecraft:diamond"));
        assert!(!live_stack(5, 0, "minecraft:diamond"));
        assert!(!live_stack(5, 1, "minecraft:air"));
        assert!(!live_stack(5, 1, ""));
    }

    #[test]
    fn labels_are_bounded_single_line_unicode_and_keep_counts() {
        let mut text = String::new();
        label_into(
            "\u{a7}c  Diamond\n \tSword\u{202e}\0",
            "minecraft:diamond_sword",
            64,
            &mut text,
        );
        assert_eq!(text, "Diamond Sword x64");
        label_into("\u{a7}a\n", "example:unknown", 1, &mut text);
        assert_eq!(text, "example:unknown");
        label_into(&"界".repeat(200), "minecraft:stone", u32::MAX, &mut text);
        assert_eq!(text, format!("{}… x{}", "界".repeat(48), u32::MAX));
        label_into("Stone", "minecraft:stone", 1, &mut text);
        assert_eq!(text, "Stone");
    }

    #[test]
    fn labels_stagger_without_overlap_and_stay_on_screen() {
        let mut rects = Vec::new();
        for _ in 0..4 {
            rects.push(label_rect([100., 100.], 60., [200., 150.], &rects).unwrap());
        }
        assert!(label_rect([100., 100.], 60., [200., 150.], &rects).is_none());
        assert_eq!(rects[1][1], rects[0][1] - 14.);
        let edge = label_rect([199., 149.], 60., [200., 150.], &[]).unwrap();
        assert!(edge[0] + edge[2] <= 198. && edge[1] + edge[3] <= 148.);
        assert!(label_rect([10., 5.], 60., [200., 150.], &[]).is_none());
        assert!(label_rect([10., 50.], 300., [200., 150.], &[]).is_none());
        assert!(label_rect([f32::NAN, 50.], 60., [200., 150.], &[]).is_none());
    }
}
