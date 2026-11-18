// baritone.api.selection.ISelection and selection.SelectionManager: boxes of the world that #sel marks,
// to build into, clear, copy or replace. A selection keeps the two corners it was made with, because
// expand and contract move whichever of them lies further in the direction asked.
use super::baritone_utils::Pos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub pos1: Pos,
    pub pos2: Pos,
}

/// The axis a direction runs along, and whether it is the negative way.
fn axis_of(dir: [i32; 3]) -> (usize, bool) {
    let i = (0..3).find(|&i| dir[i] != 0).unwrap_or(0);
    (i, dir[i] < 0)
}

fn relative(p: Pos, dir: [i32; 3], blocks: i32) -> Pos {
    [p[0] + dir[0] * blocks, p[1] + dir[1] * blocks, p[2] + dir[2] * blocks]
}

impl Selection {
    pub fn new(pos1: Pos, pos2: Pos) -> Self {
        Self { pos1, pos2 }
    }

    pub fn min(&self) -> Pos {
        [0, 1, 2].map(|i| self.pos1[i].min(self.pos2[i]))
    }

    pub fn max(&self) -> Pos {
        [0, 1, 2].map(|i| self.pos1[i].max(self.pos2[i]))
    }

    /// Blocks along each axis.
    pub fn size(&self) -> [i32; 3] {
        let (lo, hi) = (self.min(), self.max());
        [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1]
    }

    pub fn volume(&self) -> i64 {
        let s = self.size();
        i64::from(s[0]) * i64::from(s[1]) * i64::from(s[2])
    }

    #[allow(dead_code)]
    pub fn contains(&self, p: Pos) -> bool {
        let (lo, hi) = (self.min(), self.max());
        (0..3).all(|i| p[i] >= lo[i] && p[i] <= hi[i])
    }

    /// Whether pos2 is the corner further in `dir` (ties do not matter).
    fn is_pos2(&self, dir: [i32; 3]) -> bool {
        let (i, negative) = axis_of(dir);
        (self.pos2[i] > self.pos1[i]) ^ negative
    }

    /// Grow `blocks` towards `dir`, a unit vector: the corner that is further that way moves.
    pub fn expand(&self, dir: [i32; 3], blocks: i32) -> Selection {
        if self.is_pos2(dir) {
            Selection::new(self.pos1, relative(self.pos2, dir, blocks))
        } else {
            Selection::new(relative(self.pos1, dir, blocks), self.pos2)
        }
    }

    /// Shrink: the corner that is further in `dir` stays, the other moves `blocks` that way.
    pub fn contract(&self, dir: [i32; 3], blocks: i32) -> Selection {
        if self.is_pos2(dir) {
            Selection::new(relative(self.pos1, dir, blocks), self.pos2)
        } else {
            Selection::new(self.pos1, relative(self.pos2, dir, blocks))
        }
    }

    /// Move the whole box `blocks` towards `dir`.
    pub fn shift(&self, dir: [i32; 3], blocks: i32) -> Selection {
        Selection::new(relative(self.pos1, dir, blocks), relative(self.pos2, dir, blocks))
    }
}

impl std::fmt::Display for Selection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let b = |p: Pos| format!("BetterBlockPos{{x={},y={},z={}}}", p[0], p[1], p[2]);
        write!(f, "Selection{{pos1={},pos2={}}}", b(self.pos1), b(self.pos2))
    }
}

/// The selections made so far, oldest first.
#[derive(Default, Debug)]
pub struct SelectionManager {
    pub selections: Vec<Selection>,
}

impl SelectionManager {
    pub fn add(&mut self, pos1: Pos, pos2: Pos) -> Selection {
        let s = Selection::new(pos1, pos2);
        self.selections.push(s);
        s
    }

    pub fn remove_all(&mut self) -> Vec<Selection> {
        std::mem::take(&mut self.selections)
    }

    /// Remove the last one made.
    pub fn remove_last(&mut self) -> Option<Selection> {
        self.selections.pop()
    }

    #[allow(dead_code)]
    pub fn last(&self) -> Option<Selection> {
        self.selections.last().copied()
    }

    /// Replace the selection at `index` with what `change` makes of it.
    pub fn transform(&mut self, index: usize, change: impl Fn(&Selection) -> Selection) {
        if let Some(s) = self.selections.get_mut(index) {
            *s = change(s);
        }
    }
}

/// Which selections expand, contract and shift act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformTarget {
    All,
    Newest,
    Oldest,
}

impl TransformTarget {
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "all" | "a" => Some(Self::All),
            "newest" | "n" => Some(Self::Newest),
            "oldest" | "o" => Some(Self::Oldest),
            _ => None,
        }
    }

    /// The indexes (of `count` selections) it picks.
    pub fn indexes(self, count: usize) -> Vec<usize> {
        match self {
            Self::All => (0..count).collect(),
            Self::Newest => count.checked_sub(1).into_iter().collect(),
            Self::Oldest => if count > 0 { vec![0] } else { Vec::new() },
        }
    }
}
