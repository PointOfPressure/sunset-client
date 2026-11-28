// baritone.api.schematic: what a build wants the world to be, block by block. A schematic only says
// what block belongs at a position (relative to its own corner) and what to leave alone; the builder
// compares that with the world.

use super::baritone_schematic_mask::{CylinderMask, SphereMask, StaticMask};

/// A block the build wants: its name, and the properties that matter (`facing=north`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSpec {
    #[allow(dead_code)]
    pub name: String,
    pub props: Vec<(String, String)>,
}

impl BlockSpec {
    pub fn named(name: &str) -> Self {
        let name = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
        Self { name, props: Vec::new() }
    }

    pub fn air() -> Self {
        Self::named("air")
    }

    pub fn is_air(&self) -> bool {
        matches!(self.name.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
    }

    /// From `minecraft:oak_stairs[facing=north,half=bottom]`.
    pub fn parse(text: &str) -> Self {
        let (name, rest) = match text.split_once('[') {
            Some((n, r)) => (n, r.trim_end_matches(']')),
            None => (text, ""),
        };
        let mut spec = Self::named(name.trim());
        for pair in rest.split(',').filter(|p| !p.is_empty()) {
            if let Some((k, v)) = pair.split_once('=') {
                spec.props.push((k.trim().to_owned(), v.trim().to_owned()));
            }
        }
        spec
    }

    /// Whether a world block (`name` and its `state[...]` text) is this one.
    #[allow(dead_code)]
    pub fn matches(&self, name: &str, state: &str) -> bool {
        if self.is_air() {
            return matches!(name, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air");
        }
        name == self.name && self.props.iter().all(|(k, v)| state.contains(&format!("{k}={v}")))
    }
}

pub trait Schematic {
    /// Blocks along x, y and z.
    fn size(&self) -> [i32; 3];
    /// The block that belongs at `p` (inside the size), given the name of the block that is there
    /// now. None: leave it as it is.
    fn desired(&self, p: [i32; 3], current: &str) -> Option<BlockSpec>;
    #[allow(dead_code)]
    fn label(&self) -> String;
}

/// Every block in the box.
pub struct FillSchematic {
    pub size: [i32; 3],
    pub spec: BlockSpec,
}

impl Schematic for FillSchematic {
    fn size(&self) -> [i32; 3] {
        self.size
    }
    fn desired(&self, _p: [i32; 3], _current: &str) -> Option<BlockSpec> {
        Some(self.spec.clone())
    }
    fn label(&self) -> String {
        format!("{} filling {} x {} x {}", self.spec.name, self.size[0], self.size[1], self.size[2])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The four sides, no floor or roof.
    Walls,
    /// All six sides.
    Shell,
    Sphere { hollow: bool },
    /// Round about an axis (0 x, 1 y, 2 z).
    Cylinder { hollow: bool, axis: usize },
}

/// A shape in the box, made of one block; the rest of the box is left alone.
pub struct ShapeSchematic {
    pub size: [i32; 3],
    pub shape: Shape,
    pub spec: BlockSpec,
}

impl ShapeSchematic {
    fn inside(&self, p: [i32; 3]) -> bool {
        let s = self.size;
        let edge = |v: i32, n: i32| v == 0 || v == n - 1;
        match self.shape {
            Shape::Walls => edge(p[0], s[0]) || edge(p[2], s[2]),
            Shape::Shell => edge(p[0], s[0]) || edge(p[1], s[1]) || edge(p[2], s[2]),
            Shape::Sphere { hollow } => SphereMask::new(s[0], s[1], s[2], !hollow).part_of_mask(p[0], p[1], p[2]),
            Shape::Cylinder { hollow, axis } => CylinderMask::new(s[0], s[1], s[2], !hollow, axis).part_of_mask(p[0], p[1], p[2]),
        }
    }
}

impl Schematic for ShapeSchematic {
    fn size(&self) -> [i32; 3] {
        self.size
    }
    fn desired(&self, p: [i32; 3], _current: &str) -> Option<BlockSpec> {
        self.inside(p).then(|| self.spec.clone())
    }
    fn label(&self) -> String {
        format!("{:?} of {}", self.shape, self.spec.name)
    }
}

/// Swap some blocks for another and leave the rest.
pub struct ReplaceSchematic {
    pub size: [i32; 3],
    pub from: Vec<String>,
    pub to: BlockSpec,
}

impl Schematic for ReplaceSchematic {
    fn size(&self) -> [i32; 3] {
        self.size
    }
    fn desired(&self, _p: [i32; 3], current: &str) -> Option<BlockSpec> {
        let bare = current.strip_prefix("minecraft:").unwrap_or(current);
        self.from.iter().any(|f| f == bare || f == current).then(|| self.to.clone())
    }
    fn label(&self) -> String {
        format!("replacing {} with {}", self.from.join(", "), self.to.name)
    }
}

/// A schematic read from a file, or copied from the world: every position is set or left alone.
pub struct StaticSchematic {
    pub size: [i32; 3],
    /// Indexed `(y * length + z) * width + x`.
    pub blocks: Vec<Option<BlockSpec>>,
    pub name: String,
}

impl StaticSchematic {
    pub fn index(&self, p: [i32; 3]) -> usize {
        ((p[1] * self.size[2] + p[2]) * self.size[0] + p[0]) as usize
    }
}

impl Schematic for StaticSchematic {
    fn size(&self) -> [i32; 3] {
        self.size
    }
    fn desired(&self, p: [i32; 3], _current: &str) -> Option<BlockSpec> {
        self.blocks.get(self.index(p)).cloned().flatten()
    }
    fn label(&self) -> String {
        format!("{} ({} x {} x {})", self.name, self.size[0], self.size[1], self.size[2])
    }
}

/// Several schematics laid out in one box: what a build of more than one selection is. Where two
/// overlap, the first listed wins.
pub struct CompositeSchematic {
    pub size: [i32; 3],
    /// Each part, and where its corner is in the box.
    pub parts: Vec<([i32; 3], std::rc::Rc<dyn Schematic>)>,
    pub name: String,
}

impl Schematic for CompositeSchematic {
    fn size(&self) -> [i32; 3] {
        self.size
    }
    fn desired(&self, p: [i32; 3], current: &str) -> Option<BlockSpec> {
        self.parts.iter().find_map(|(off, part)| {
            let rel = [p[0] - off[0], p[1] - off[1], p[2] - off[2]];
            let s = part.size();
            let inside = (0..3).all(|i| rel[i] >= 0 && rel[i] < s[i]);
            if inside { part.desired(rel, current) } else { None }
        })
    }
    fn label(&self) -> String {
        format!("{} ({} parts)", self.name, self.parts.len())
    }
}
