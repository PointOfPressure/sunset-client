// baritone.process.BuilderProcess, the part that needs no game: whether a block is the one the schematic wants
// (`valid`, `sameBlockstate`), the schematic as the builder reads it (ISchematic with block states), the layers
// wrapper, and BuilderCalculationContext's costs. The process itself is in build.rs.
use super::baritone_block_state::{BlockState, Class};
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_costs::COST_INF;
use super::baritone_settings::Settings;
use super::baritone_utils::Pos;
use std::rc::Rc;

/// BuilderProcess.ORIENTATION_PROPS, by property name: the properties that say which way a block faces.
const ORIENTATION_PROPS: &[&str] = &["axis", "facing", "half", "shape", "north", "east", "south", "west", "up", "open"];

/// baritone.api.schematic.ISchematic, with the block states the builder compares.
pub trait ISchematic {
    /// The state wanted at a position relative to the schematic's corner. `current` is what is there now and
    /// `approx_placeable` what the inventory would place (some schematics choose a block from it).
    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState>;

    fn width_x(&self) -> i32;
    fn height_y(&self) -> i32;
    fn length_z(&self) -> i32;

    /// Whether the position is something the schematic has an opinion about.
    fn in_schematic(&self, x: i32, y: i32, z: i32, _current: &BlockState) -> bool {
        x >= 0 && x < self.width_x() && y >= 0 && y < self.height_y() && z >= 0 && z < self.length_z()
    }

    /// Forget any state the schematic keeps (a repeated build).
    fn reset(&self) {}
}

/// What the builder reads of the settings, taken once per use.
#[derive(Clone)]
pub struct ValidSettings {
    pub ok_if_water: bool,
    pub ok_if_air: Vec<String>,
    pub build_ignore_blocks: Vec<String>,
    pub build_ignore_existing: bool,
    pub build_ignore_direction: bool,
    pub build_ignore_properties: Vec<String>,
    /// `buildValidSubstitutes`: the block a schematic asks for, and the blocks accepted in its place.
    pub valid_substitutes: Vec<(String, Vec<String>)>,
}

impl ValidSettings {
    pub fn of(s: &Settings) -> Self {
        let bare = |list: Vec<String>| list.into_iter().map(|n| n.trim_start_matches("minecraft:").to_owned()).collect();
        Self {
            ok_if_water: s.flag("okIfWater"),
            ok_if_air: bare(s.list("okIfAir")),
            build_ignore_blocks: bare(s.list("buildIgnoreBlocks")),
            build_ignore_existing: s.flag("buildIgnoreExisting"),
            build_ignore_direction: s.flag("buildIgnoreDirection"),
            build_ignore_properties: s.list("buildIgnoreProperties"),
            valid_substitutes: s.block_map("buildValidSubstitutes"),
        }
    }
}

/// `BuilderProcess.sameBlockstate`.
pub fn same_blockstate(first: &BlockState, second: &BlockState, s: &ValidSettings) -> bool {
    if first.name != second.name {
        return false;
    }
    if !s.build_ignore_direction && s.build_ignore_properties.is_empty() {
        return first.props == second.props; // early return if no properties are being ignored
    }
    for (key, value) in first.props.iter() {
        let other = second.prop(key);
        if other != Some(&**value) && !(s.build_ignore_direction && ORIENTATION_PROPS.contains(&&**key)) && !s.build_ignore_properties.iter().any(|p| p == &**key) {
            return false;
        }
    }
    true
}

/// `BuilderProcess.containsBlockState`.
pub fn contains_blockstate(states: &[Rc<BlockState>], state: &BlockState, s: &ValidSettings) -> bool {
    states.iter().any(|t| same_blockstate(t, state, s))
}

fn is_liquid_block(state: &BlockState) -> bool {
    state.is(Class::Liquid)
}

/// `BuilderProcess.valid`: whether `current` is as good as `desired` (None: no opinion).
pub fn valid(current: &BlockState, desired: Option<&BlockState>, item_verify: bool, s: &ValidSettings) -> bool {
    let Some(desired) = desired else { return true };
    if is_liquid_block(current) && s.ok_if_water {
        return true;
    }
    if current.is_air() && desired.is_air() {
        return true;
    }
    if current.is_air() && s.ok_if_air.iter().any(|b| b == desired.bare_name()) {
        return true;
    }
    if desired.is_air() && s.build_ignore_blocks.iter().any(|b| b == current.bare_name()) {
        return true;
    }
    if !current.is_air() && s.build_ignore_existing && !item_verify {
        return true;
    }
    if !item_verify && s.valid_substitutes.iter().any(|(source, accepted)| source == desired.bare_name() && accepted.iter().any(|b| b == current.bare_name())) {
        return true;
    }
    if current.name == desired.name && current.props == desired.props {
        return true;
    }
    same_blockstate(current, desired, s)
}

/// BuilderCalculationContext: the search placing a block where the schematic wants it for free, and breaking a
/// block that is wrong for no more than a block would cost.
pub struct BuilderHooks {
    pub placeable: Vec<Rc<BlockState>>,
    pub schematic: Rc<dyn ISchematic>,
    pub origin: Pos,
    pub valid: ValidSettings,
    pub place_incorrect_penalty: f64,
    pub break_correct_penalty: f64,
}

impl BuilderHooks {
    /// `getSchematic`: the state wanted at a world position, None where the schematic has no opinion.
    pub fn get_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> Option<Rc<BlockState>> {
        let (rx, ry, rz) = (x - self.origin[0], y - self.origin[1], z - self.origin[2]);
        if self.schematic.in_schematic(rx, ry, rz, current) {
            Some(self.schematic.desired_state(rx, ry, rz, current, &self.placeable))
        } else {
            None
        }
    }
}

impl CostHooks for BuilderHooks {
    fn configure(&self, ctx: &mut CalculationContext) {
        ctx.jump_penalty += 10.0;
        ctx.backtrack_cost_favoring_coefficient = 1.0;
    }

    fn cost_of_placing_at(&self, base: &CalculationContext, x: i32, y: i32, z: i32, current: &BlockState) -> Option<f64> {
        if !base.bsi.can_place_at(x, z) {
            // Make calculation fail properly if we can't build.
            return Some(COST_INF);
        }
        let place_block_cost = base.place_block_cost();
        match self.get_schematic(x, y, z, current) {
            Some(sch) => {
                // TODO this can return true even when allowPlace is off.... is that an issue?
                if sch.is_air() {
                    // We want this to be air, but they're asking if they can place here. This won't be a schematic
                    // block, this will be a throwaway.
                    return Some(place_block_cost * self.place_incorrect_penalty); // we're going to have to break it eventually
                }
                if self.placeable.iter().any(|p| p.name == sch.name && p.props == sch.props) {
                    return Some(0.0); // that's right we gonna make it FREE to place a block where it should go in a structure
                }
                if !base.has_throwaway {
                    return Some(COST_INF);
                }
                // We want it to be something that we don't have: even more of a pain to place something wrong.
                Some(place_block_cost * 1.5 * self.place_incorrect_penalty)
            }
            None => Some(if base.has_throwaway { place_block_cost } else { COST_INF }),
        }
    }

    fn break_cost_multiplier_at(&self, base: &CalculationContext, x: i32, y: i32, z: i32, current: &BlockState) -> Option<f64> {
        if !base.allow_break && !base.allow_break_anyway.iter().any(|b| b == current.bare_name()) {
            return Some(COST_INF);
        }
        match self.get_schematic(x, y, z, current) {
            Some(sch) => {
                if sch.is_air() {
                    // It should be air: regardless of current contents, we can break it.
                    return Some(1.0);
                }
                // It should be a real block. Is it already that block?
                if valid(&base.bsi.get0(x, y, z), Some(&sch), false, &self.valid) {
                    Some(self.break_correct_penalty)
                } else {
                    // Can break if it's wrong. Would be great to return less than 1 here, but that would actually make
                    // the cost calculation messed up since we're breaking a block, if we underestimate the cost, then
                    // it'll fail when it really takes the correct amount of time.
                    Some(1.0)
                }
            }
            None => Some(1.0), // why not lol
        }
    }
}

/// The layers wrapper `onTick` puts round the schematic when buildInLayers is on.
pub struct LayeredSchematic {
    pub real: Rc<dyn ISchematic>,
    pub min_y_inclusive: i32,
    pub max_y_inclusive: i32,
}

impl ISchematic for LayeredSchematic {
    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        self.real.desired_state(x, y, z, current, approx_placeable)
    }

    fn width_x(&self) -> i32 {
        self.real.width_x()
    }

    fn height_y(&self) -> i32 {
        self.real.height_y()
    }

    fn length_z(&self) -> i32 {
        self.real.length_z()
    }

    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        x >= 0 && x < self.width_x() && y >= 0 && y < self.height_y() && z >= 0 && z < self.length_z() && y >= self.min_y_inclusive && y <= self.max_y_inclusive && self.real.in_schematic(x, y, z, current)
    }

    fn reset(&self) {
        self.real.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::baritone_block_state::FluidKind;
    use super::super::baritone_block_state::parse_props;
    use std::sync::Arc;

    fn state(name: &str, props: &str, classes: &[&str]) -> BlockState {
        BlockState {
            id: 0,
            name: Arc::from(format!("minecraft:{name}")),
            classes: Class::flags_of(classes.iter().copied()),
            props: parse_props(&format!("minecraft:{name}[{props}]")),
            fluid: FluidKind::None,
            fluid_amount: 0,
            hardness: 1.0,
            replaceable: name == "air",
            full_collision: name != "air",
            sturdy_top: true,
            pathfindable: name == "air",
        }
    }

    fn settings() -> ValidSettings {
        ValidSettings::of(&Settings::load(|_| None))
    }

    #[test]
    fn a_valid_substitute_is_accepted_in_place_of_the_block_asked_for() {
        let mut s = settings();
        s.valid_substitutes = vec![("stone".to_owned(), vec!["cobblestone".to_owned(), "andesite".to_owned()])];
        let stone = state("stone", "", &[]);
        assert!(valid(&state("andesite", "", &[]), Some(&stone), false, &s));
        assert!(!valid(&state("dirt", "", &[]), Some(&stone), false, &s));
        // A stack that has to come out as stone itself is not satisfied by a substitute.
        assert!(!valid(&state("andesite", "", &[]), Some(&stone), true, &s));
    }

    #[test]
    fn the_same_block_with_other_properties_is_wrong_unless_the_property_is_ignored() {
        let a = state("oak_stairs", "facing=north,half=bottom", &[]);
        let b = state("oak_stairs", "facing=south,half=bottom", &[]);
        let mut s = settings();
        assert!(!valid(&a, Some(&b), false, &s));
        s.build_ignore_direction = true;
        assert!(valid(&a, Some(&b), false, &s));
        s.build_ignore_direction = false;
        s.build_ignore_properties = vec!["facing".into()];
        assert!(valid(&a, Some(&b), false, &s));
        let c = state("oak_stairs", "facing=south,half=top", &[]);
        assert!(!valid(&a, Some(&c), false, &s));
    }

    #[test]
    fn air_matches_air_and_nothing_wanted_matches_anything() {
        let air = state("air", "", &["AirBlock"]);
        let stone = state("stone", "", &[]);
        let s = settings();
        assert!(valid(&air, Some(&air), false, &s));
        assert!(valid(&stone, None, false, &s));
        assert!(!valid(&air, Some(&stone), false, &s));
        assert!(!valid(&stone, Some(&air), false, &s));
    }

    #[test]
    fn existing_blocks_are_ok_when_ignored_but_not_when_checking_an_item() {
        let dirt = state("dirt", "", &[]);
        let stone = state("stone", "", &[]);
        let mut s = settings();
        s.build_ignore_existing = true;
        assert!(valid(&dirt, Some(&stone), false, &s));
        assert!(!valid(&dirt, Some(&stone), true, &s));
    }

    #[test]
    fn ok_if_air_and_build_ignore_blocks() {
        let air = state("air", "", &["AirBlock"]);
        let torch = state("torch", "", &[]);
        let dirt = state("dirt", "", &[]);
        let mut s = settings();
        s.ok_if_air = vec!["torch".into()];
        assert!(valid(&air, Some(&torch), false, &s));
        s.build_ignore_blocks = vec!["dirt".into()];
        assert!(valid(&dirt, Some(&air), false, &s));
    }
}
