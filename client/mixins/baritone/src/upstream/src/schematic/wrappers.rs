// baritone.api.schematic.MirroredSchematic, RotatedSchematic, SubstituteSchematic and MaskSchematic (and the masks the
// builder puts on them: SelectionSchematic and MapArtSchematic), written from the Java. Each wraps another schematic
// and changes where its blocks are, which block is wanted, or which positions count at all.
//
// What a rotation or a mirror does to a block state, and what a block's default state is, belong to the game, so the
// wrappers ask a `StateOps`; the game gives the tables Horizon's own structure placement uses.
use super::baritone_block_state::BlockState;
use super::baritone_builder_logic::ISchematic;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub trait StateOps {
    /// `BlockState.rotate(rotation)`; `quarter_turns` is 0 none, 1 clockwise 90, 2 clockwise 180, 3 counterclockwise 90.
    fn rotate(&self, state: &Rc<BlockState>, quarter_turns: usize) -> Rc<BlockState>;
    /// `BlockState.mirror(mirror)`; `mirror` is 0 none, 1 left-right, 2 front-back.
    fn mirror(&self, state: &Rc<BlockState>, mirror: usize) -> Rc<BlockState>;
    /// The state of a block with the properties given, if there is one.
    fn resolve(&self, name: &str, props: &[(String, String)]) -> Option<Rc<BlockState>>;
    fn air(&self) -> Rc<BlockState>;
}

// ---------------------------------------------------------------------------------------
// Mirrored

pub struct MirroredSchematic {
    schematic: Rc<dyn ISchematic>,
    mirror: usize,
    ops: Rc<dyn StateOps>,
}

impl MirroredSchematic {
    pub fn new(schematic: Rc<dyn ISchematic>, mirror: usize, ops: Rc<dyn StateOps>) -> Self {
        Self { schematic, mirror, ops }
    }

    fn mirror_x(&self, x: i32) -> i32 {
        if self.mirror == 2 { self.width_x() - x - 1 } else { x }
    }

    fn mirror_z(&self, z: i32) -> i32 {
        if self.mirror == 1 { self.length_z() - z - 1 } else { z }
    }
}

impl ISchematic for MirroredSchematic {
    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        let current = self.ops.mirror(&Rc::new(current.clone()), self.mirror);
        self.schematic.in_schematic(self.mirror_x(x), y, self.mirror_z(z), &current)
    }

    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        let current = self.ops.mirror(&Rc::new(current.clone()), self.mirror);
        let placeable: Vec<Rc<BlockState>> = approx_placeable.iter().map(|s| self.ops.mirror(s, self.mirror)).collect();
        let desired = self.schematic.desired_state(self.mirror_x(x), y, self.mirror_z(z), &current, &placeable);
        self.ops.mirror(&desired, self.mirror)
    }

    fn reset(&self) {
        self.schematic.reset();
    }

    fn width_x(&self) -> i32 {
        self.schematic.width_x()
    }

    fn height_y(&self) -> i32 {
        self.schematic.height_y()
    }

    fn length_z(&self) -> i32 {
        self.schematic.length_z()
    }
}

// ---------------------------------------------------------------------------------------
// Rotated

pub struct RotatedSchematic {
    schematic: Rc<dyn ISchematic>,
    rotation: usize,
    inverse: usize,
    ops: Rc<dyn StateOps>,
}

impl RotatedSchematic {
    pub fn new(schematic: Rc<dyn ISchematic>, rotation: usize, ops: Rc<dyn StateOps>) -> Self {
        // rotation.getRotated(rotation).getRotated(rotation): three of them make the way back.
        Self { schematic, rotation, inverse: (rotation * 3) % 4, ops }
    }

    fn flips(&self) -> bool {
        self.rotation == 1 || self.rotation == 3
    }

    fn rotate_x(&self, x: i32, z: i32) -> i32 {
        match self.inverse {
            0 => x,
            1 => self.length_z() - z - 1,
            2 => self.width_x() - x - 1,
            _ => z,
        }
    }

    fn rotate_z(&self, x: i32, z: i32) -> i32 {
        match self.inverse {
            0 => z,
            1 => x,
            2 => self.length_z() - z - 1,
            _ => self.width_x() - x - 1,
        }
    }
}

impl ISchematic for RotatedSchematic {
    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        let current = self.ops.rotate(&Rc::new(current.clone()), self.inverse);
        self.schematic.in_schematic(self.rotate_x(x, z), y, self.rotate_z(x, z), &current)
    }

    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        let current = self.ops.rotate(&Rc::new(current.clone()), self.inverse);
        let placeable: Vec<Rc<BlockState>> = approx_placeable.iter().map(|s| self.ops.rotate(s, self.inverse)).collect();
        let desired = self.schematic.desired_state(self.rotate_x(x, z), y, self.rotate_z(x, z), &current, &placeable);
        self.ops.rotate(&desired, self.rotation)
    }

    fn reset(&self) {
        self.schematic.reset();
    }

    fn width_x(&self) -> i32 {
        if self.flips() { self.schematic.length_z() } else { self.schematic.width_x() }
    }

    fn height_y(&self) -> i32 {
        self.schematic.height_y()
    }

    fn length_z(&self) -> i32 {
        if self.flips() { self.schematic.width_x() } else { self.schematic.length_z() }
    }
}

// ---------------------------------------------------------------------------------------
// Substitute

pub struct SubstituteSchematic {
    schematic: Rc<dyn ISchematic>,
    /// The block wanted, and the blocks to build instead, best first.
    substitutions: Vec<(String, Vec<String>)>,
    ops: Rc<dyn StateOps>,
    /// `blockStateCache`: a state with another block's name, keyed by the state and the block.
    cache: RefCell<HashMap<(u32, String), Rc<BlockState>>>,
}

fn is_air_name(name: &str) -> bool {
    matches!(name.strip_prefix("minecraft:").unwrap_or(name), "air" | "cave_air" | "void_air")
}

impl SubstituteSchematic {
    pub fn new(schematic: Rc<dyn ISchematic>, substitutions: Vec<(String, Vec<String>)>, ops: Rc<dyn StateOps>) -> Self {
        Self { schematic, substitutions, ops, cache: RefCell::new(HashMap::new()) }
    }

    fn bare(state: &BlockState) -> &str {
        state.bare_name()
    }

    /// `withBlock`: the block's default state with every property the state has and the block accepts.
    fn with_block(&self, state: &Rc<BlockState>, block: &str) -> Rc<BlockState> {
        let key = (state.id, block.to_owned());
        if state.id != 0 {
            if let Some(hit) = self.cache.borrow().get(&key) {
                return hit.clone();
            }
        }
        let Some(default) = self.ops.resolve(block, &[]) else { return state.clone() };
        let mut props: Vec<(String, String)> = Vec::new();
        for (k, v) in state.props.iter() {
            // A property the other block does not have, or a value it does not allow, is left out.
            if default.has_prop(k) {
                let mut candidate = props.clone();
                candidate.push((k.to_string(), v.to_string()));
                if self.ops.resolve(block, &candidate).is_some_and(|r| r.prop(k) == Some(&**v)) {
                    props = candidate;
                }
            }
        }
        let new_state = self.ops.resolve(block, &props).unwrap_or(default);
        if state.id != 0 {
            self.cache.borrow_mut().insert(key, new_state.clone());
        }
        new_state
    }
}

impl ISchematic for SubstituteSchematic {
    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        self.schematic.in_schematic(x, y, z, current)
    }

    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        let desired = self.schematic.desired_state(x, y, z, current, approx_placeable);
        let Some((_, substitutes)) = self.substitutions.iter().find(|(source, _)| source == Self::bare(&desired)) else {
            return desired;
        };
        let current_name = Self::bare(current);
        // Do not preserve air: it is almost always there and almost never wanted.
        if substitutes.iter().any(|s| s == current_name) && !current.is_air() {
            return self.with_block(&desired, &current.name);
        }
        for substitute in substitutes {
            if is_air_name(substitute) {
                // Air can always be "placed".
                return if current.is_air() { Rc::new(current.clone()) } else { self.ops.air() };
            }
            if let Some(placeable) = approx_placeable.iter().find(|p| Self::bare(p) == substitute) {
                return self.with_block(&desired, &placeable.name);
            }
        }
        match substitutes.first().and_then(|first| self.ops.resolve(first, &[])) {
            Some(first) => first,
            None => desired,
        }
    }

    fn reset(&self) {
        self.schematic.reset();
    }

    fn width_x(&self) -> i32 {
        self.schematic.width_x()
    }

    fn height_y(&self) -> i32 {
        self.schematic.height_y()
    }

    fn length_z(&self) -> i32 {
        self.schematic.length_z()
    }
}

// ---------------------------------------------------------------------------------------
// Mask

/// `partOfMask(x, y, z, currentState)`.
pub type MaskFn = Rc<dyn Fn(i32, i32, i32, &BlockState) -> bool>;

pub struct MaskSchematic {
    schematic: Rc<dyn ISchematic>,
    part_of_mask: MaskFn,
}

impl MaskSchematic {
    pub fn new(schematic: Rc<dyn ISchematic>, part_of_mask: MaskFn) -> Self {
        Self { schematic, part_of_mask }
    }

    /// `SelectionSchematic`: only what lies inside one of the selections (given as the corners relative to the schematic) is
    /// part of it.
    pub fn selection(schematic: Rc<dyn ISchematic>, boxes: Vec<([i32; 3], [i32; 3])>) -> Self {
        Self::new(
            schematic,
            Rc::new(move |x, y, z, _| boxes.iter().any(|(min, max)| x >= min[0] && y >= min[1] && z >= min[2] && x <= max[0] && y <= max[1] && z <= max[2])),
        )
    }

    /// `MapArtSchematic`: only the top block of each column counts, and what is under it does not matter. `height_map` is
    /// the y of the highest block of each column, `x * length + z`.
    pub fn map_art(schematic: Rc<dyn ISchematic>, height_map: Vec<i32>, length_z: i32) -> Self {
        Self::new(schematic, Rc::new(move |x, y, z, _| y >= height_map.get((x * length_z + z) as usize).copied().unwrap_or(i32::MAX)))
    }
}

impl ISchematic for MaskSchematic {
    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        self.schematic.in_schematic(x, y, z, current) && (self.part_of_mask)(x, y, z, current)
    }

    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        self.schematic.desired_state(x, y, z, current, approx_placeable)
    }

    fn reset(&self) {
        self.schematic.reset();
    }

    fn width_x(&self) -> i32 {
        self.schematic.width_x()
    }

    fn height_y(&self) -> i32 {
        self.schematic.height_y()
    }

    fn length_z(&self) -> i32 {
        self.schematic.length_z()
    }
}

/// `MapArtSchematic.generateHeightMap`: the y of the highest block that is not air in each column, `x * length + z`, or
/// `i32::MAX` for a column with none (it is let be whatever). `column(x, z)` lists the block of each y, lowest first.
pub fn map_art_heights(width: i32, length: i32, column: &dyn Fn(i32, i32) -> Vec<bool>) -> Vec<i32> {
    let mut heights = Vec::with_capacity((width * length) as usize);
    for x in 0..width {
        for z in 0..length {
            let solid = column(x, z);
            heights.push(solid.iter().rposition(|filled| *filled).map_or(i32::MAX, |y| y as i32));
        }
    }
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, props: &[(&str, &str)]) -> Rc<BlockState> {
        use super::super::baritone_block_state::{parse_props, Class, FluidKind};
        let text = props.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",");
        let air = matches!(name, "air" | "cave_air");
        Rc::new(BlockState {
            id: 0,
            name: std::sync::Arc::from(format!("minecraft:{name}")),
            classes: if air { Class::flags_of(["AirBlock"]) } else { 0 },
            props: parse_props(&format!("minecraft:{name}[{text}]")),
            fluid: FluidKind::None,
            fluid_amount: 0,
            hardness: 1.0,
            replaceable: air,
            full_collision: !air,
            sturdy_top: true,
            pathfindable: air,
        })
    }

    /// Facing turns clockwise by `quarter_turns`; a mirror swaps east and west for left-right.
    struct Ops;

    const FACES: [&str; 4] = ["north", "east", "south", "west"];

    impl StateOps for Ops {
        fn rotate(&self, s: &Rc<BlockState>, quarter_turns: usize) -> Rc<BlockState> {
            match s.prop("facing").and_then(|f| FACES.iter().position(|x| *x == f)) {
                Some(i) => state(s.bare_name(), &[("facing", FACES[(i + quarter_turns) % 4])]),
                None => s.clone(),
            }
        }
        fn mirror(&self, s: &Rc<BlockState>, mirror: usize) -> Rc<BlockState> {
            match (s.prop("facing"), mirror) {
                (Some("east"), 1) => state(s.bare_name(), &[("facing", "west")]),
                (Some("west"), 1) => state(s.bare_name(), &[("facing", "east")]),
                (Some("north"), 2) => state(s.bare_name(), &[("facing", "south")]),
                (Some("south"), 2) => state(s.bare_name(), &[("facing", "north")]),
                _ => s.clone(),
            }
        }
        fn resolve(&self, name: &str, props: &[(String, String)]) -> Option<Rc<BlockState>> {
            let bare = name.strip_prefix("minecraft:").unwrap_or(name);
            if bare == "bogus" {
                return None;
            }
            // Stairs and furnaces face; nothing else has a property.
            let has_facing = bare.ends_with("_stairs") || bare == "furnace";
            if props.iter().any(|(k, _)| k != "facing" || !has_facing) {
                return None;
            }
            let refs: Vec<(&str, &str)> = props.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            if has_facing && refs.is_empty() {
                return Some(state(bare, &[("facing", "north")]));
            }
            Some(state(bare, &refs))
        }
        fn air(&self) -> Rc<BlockState> {
            state("air", &[])
        }
    }

    /// A 3 x 1 x 2 schematic whose block says where it is: `b{x}{z}`, facing east.
    struct Grid;

    impl ISchematic for Grid {
        fn desired_state(&self, x: i32, _y: i32, z: i32, _current: &BlockState, _p: &[Rc<BlockState>]) -> Rc<BlockState> {
            state(&format!("b{x}{z}"), &[("facing", "east")])
        }
        fn width_x(&self) -> i32 {
            3
        }
        fn height_y(&self) -> i32 {
            1
        }
        fn length_z(&self) -> i32 {
            2
        }
    }

    fn grid() -> Rc<dyn ISchematic> {
        Rc::new(Grid)
    }

    fn ops() -> Rc<dyn StateOps> {
        Rc::new(Ops)
    }

    #[test]
    fn a_clockwise_quarter_turn_moves_the_corner_and_turns_the_blocks() {
        let r = RotatedSchematic::new(grid(), 1, ops());
        assert_eq!((r.width_x(), r.length_z()), (2, 3));
        let air = state("air", &[]);
        // The block at the origin corner goes to the far x edge, and the block that faced east faces south.
        let at = |x, z| r.desired_state(x, 0, z, &air, &[]);
        let corner = at(1, 0);
        assert_eq!(corner.bare_name(), "b00");
        assert_eq!(corner.prop("facing"), Some("south"));
        assert_eq!(at(0, 0).bare_name(), "b01");
        assert_eq!(at(1, 2).bare_name(), "b20");
    }

    #[test]
    fn a_rotation_followed_by_its_inverse_is_the_same_schematic() {
        let air = state("air", &[]);
        for rotation in 0..4 {
            let r = RotatedSchematic::new(grid(), rotation, ops());
            let mut seen = Vec::new();
            for z in 0..r.length_z() {
                for x in 0..r.width_x() {
                    seen.push(r.desired_state(x, 0, z, &air, &[]).bare_name().to_owned());
                }
            }
            seen.sort();
            assert_eq!(seen, ["b00", "b01", "b10", "b11", "b20", "b21"], "rotation {rotation}");
        }
    }

    #[test]
    fn a_half_turn_reverses_both_axes() {
        let r = RotatedSchematic::new(grid(), 2, ops());
        let air = state("air", &[]);
        assert_eq!(r.desired_state(0, 0, 0, &air, &[]).bare_name(), "b21");
        assert_eq!(r.desired_state(2, 0, 1, &air, &[]).bare_name(), "b00");
    }

    #[test]
    fn a_mirror_flips_one_axis_and_the_blocks_that_face_along_it() {
        let air = state("air", &[]);
        let front_back = MirroredSchematic::new(grid(), 2, ops());
        assert_eq!(front_back.desired_state(0, 0, 0, &air, &[]).bare_name(), "b20");
        let left_right = MirroredSchematic::new(grid(), 1, ops());
        let d = left_right.desired_state(0, 0, 0, &air, &[]);
        assert_eq!(d.bare_name(), "b01");
        assert_eq!(d.prop("facing"), Some("west"));
        let none = MirroredSchematic::new(grid(), 0, ops());
        assert_eq!(none.desired_state(1, 0, 1, &air, &[]).bare_name(), "b11");
    }

    struct Wants(&'static str);

    impl ISchematic for Wants {
        fn desired_state(&self, _x: i32, _y: i32, _z: i32, _c: &BlockState, _p: &[Rc<BlockState>]) -> Rc<BlockState> {
            state(self.0, &[("facing", "south")])
        }
        fn width_x(&self) -> i32 {
            1
        }
        fn height_y(&self) -> i32 {
            1
        }
        fn length_z(&self) -> i32 {
            1
        }
    }

    fn substitutes(wants: &'static str, map: &[(&str, &[&str])]) -> SubstituteSchematic {
        let map = map.iter().map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect())).collect();
        SubstituteSchematic::new(Rc::new(Wants(wants)), map, ops())
    }

    #[test]
    fn a_substitute_keeps_what_is_already_there_when_it_is_listed() {
        let s = substitutes("oak_stairs", &[("oak_stairs", &["spruce_stairs", "furnace"])]);
        let d = s.desired_state(0, 0, 0, &state("furnace", &[("facing", "west")]), &[]);
        // The furnace is accepted, and takes the stairs' facing where it has the property.
        assert_eq!((d.bare_name(), d.prop("facing")), ("furnace", Some("south")));
    }

    #[test]
    fn a_substitute_takes_the_first_one_that_can_be_placed() {
        let s = substitutes("oak_stairs", &[("oak_stairs", &["spruce_stairs", "furnace"])]);
        let air = state("air", &[]);
        let d = s.desired_state(0, 0, 0, &air, &[state("furnace", &[("facing", "north")])]);
        assert_eq!(d.bare_name(), "furnace");
        // Nothing placeable: the first of the list.
        let d = s.desired_state(0, 0, 0, &air, &[]);
        assert_eq!((d.bare_name(), d.prop("facing")), ("spruce_stairs", Some("north")));
    }

    #[test]
    fn air_among_the_substitutes_clears_the_block_and_leaves_air_alone() {
        let s = substitutes("oak_stairs", &[("oak_stairs", &["air"])]);
        assert!(s.desired_state(0, 0, 0, &state("dirt", &[]), &[]).is_air());
        assert_eq!(s.desired_state(0, 0, 0, &state("cave_air", &[]), &[]).bare_name(), "cave_air");
        // Not on the list at all: unchanged.
        let other = substitutes("oak_stairs", &[("stone", &["dirt"])]);
        assert_eq!(other.desired_state(0, 0, 0, &state("dirt", &[]), &[]).bare_name(), "oak_stairs");
    }

    #[test]
    fn a_property_the_new_block_does_not_have_is_left_out() {
        let s = substitutes("oak_stairs", &[("oak_stairs", &["dirt"])]);
        let d = s.desired_state(0, 0, 0, &state("air", &[]), &[state("dirt", &[])]);
        assert_eq!((d.bare_name(), d.props.len()), ("dirt", 0));
    }

    #[test]
    fn a_selection_mask_and_a_map_art_mask_decide_which_positions_count() {
        let air = state("air", &[]);
        let selected = MaskSchematic::selection(grid(), vec![([1, 0, 0], [2, 0, 0])]);
        assert!(!selected.in_schematic(0, 0, 0, &air));
        assert!(selected.in_schematic(1, 0, 0, &air) && selected.in_schematic(2, 0, 0, &air));
        assert!(!selected.in_schematic(2, 0, 1, &air));

        // Column (x 0, z 0) tops out at y 0, the others have nothing in them.
        let heights = map_art_heights(3, 2, &|x, z| vec![x == 0 && z == 0]);
        assert_eq!(heights[0], 0);
        assert_eq!(heights[1], i32::MAX);
        let art = MaskSchematic::map_art(grid(), heights, 2);
        assert!(art.in_schematic(0, 0, 0, &air));
        assert!(!art.in_schematic(1, 0, 0, &air), "an empty column is let be");
    }
}
