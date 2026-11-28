// baritone.process.elytra.UnpackedSegment and NetherPath: a stretch of flight path, and whether it reaches the goal. The Java
// builds a lazy Stream and collects it once; here a segment holds its blocks, since joining two is a copy either way.
use std::collections::HashMap;
use std::sync::Arc;

type Pos = [i32; 3];

/// `PathCalculationException`: the search found nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathCalculationError(pub String);

impl std::fmt::Display for PathCalculationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpackedSegment {
    path: Vec<Pos>,
    finished: bool,
}

impl UnpackedSegment {
    pub fn new(path: Vec<Pos>, finished: bool) -> Self {
        Self { path, finished }
    }

    /// `append`: the new segment is only finished if the one getting added on is.
    pub fn append(mut self, other: &[Pos], other_finished: bool) -> Self {
        self.path.extend_from_slice(other);
        self.finished = other_finished;
        self
    }

    pub fn prepend(self, other: &[Pos]) -> Self {
        let mut path = other.to_vec();
        path.extend(self.path);
        Self { path, finished: self.finished }
    }

    /// `collect`. Remove backtracks: coming back to a spot we've been means everything since then was a loop, cut it out.
    pub fn collect(&self) -> Vec<Pos> {
        let mut out: Vec<Pos> = Vec::with_capacity(self.path.len());
        let mut index: HashMap<Pos, usize> = HashMap::new();
        for pos in &self.path {
            match index.get(pos) {
                None => {
                    index.insert(*pos, out.len());
                    out.push(*pos);
                }
                Some(&j) => {
                    while out.len() > j + 1 {
                        if let Some(removed) = out.pop() {
                            index.remove(&removed);
                        }
                    }
                }
            }
        }
        out
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

/// `NetherPath`: a path that is never changed once made, so it is compared by reference.
#[derive(Clone, Debug)]
pub struct NetherPath {
    blocks: Arc<Vec<Pos>>,
}

impl NetherPath {
    pub fn new(blocks: Vec<Pos>) -> Self {
        Self { blocks: Arc::new(blocks) }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn get(&self, i: usize) -> Pos {
        self.blocks[i]
    }

    /// The last position in the path, or None if empty.
    pub fn last(&self) -> Option<Pos> {
        self.blocks.last().copied()
    }

    pub fn blocks(&self) -> &[Pos] {
        &self.blocks
    }

    /// The shared storage, for a solver context to hold and compare by reference.
    pub fn shared(&self) -> Arc<Vec<Pos>> {
        self.blocks.clone()
    }

    #[allow(dead_code)]
    pub fn same_as(&self, other: &NetherPath) -> bool {
        Arc::ptr_eq(&self.blocks, &other.blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32) -> Pos {
        [x, 0, 0]
    }

    #[test]
    fn a_loop_in_the_path_is_cut_out() {
        // A B C B D C came out as A B D in the first version: the stale C -> 2 stayed in the index and ate the real end of
        // the path. Cutting the loop B C B forgets C, so the C at the end is a new spot.
        let (a, b, c, d) = (p(0), p(1), p(2), p(3));
        let s = UnpackedSegment::new(vec![a, b, c, b, d, c], true);
        assert_eq!(s.collect(), vec![a, b, d, c]);
        let s = UnpackedSegment::new(vec![a, b, c, b, d], true);
        assert_eq!(s.collect(), vec![a, b, d]);
        assert_eq!(UnpackedSegment::new(vec![a, a, a], false).collect(), vec![a]);
    }

    #[test]
    fn append_and_prepend_keep_the_right_finished_flag() {
        let s = UnpackedSegment::new(vec![p(1), p(2)], false);
        let appended = s.clone().append(&[p(3)], true);
        assert!(appended.is_finished());
        assert_eq!(appended.collect(), vec![p(1), p(2), p(3)]);
        // Prepending keeps this segment's flag.
        let prepended = s.prepend(&[p(0)]);
        assert!(!prepended.is_finished());
        assert_eq!(prepended.collect(), vec![p(0), p(1), p(2)]);
    }

    #[test]
    fn a_nether_path_is_compared_by_reference() {
        let a = NetherPath::new(vec![p(1), p(2)]);
        let b = a.clone();
        let c = NetherPath::new(vec![p(1), p(2)]);
        assert!(a.same_as(&b));
        assert!(!a.same_as(&c));
        assert_eq!(a.last(), Some(p(2)));
        assert_eq!(NetherPath::empty().last(), None);
    }
}
