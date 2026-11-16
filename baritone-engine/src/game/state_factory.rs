// The game's block states as the pathfinder's BlockState: built from the game's facts once per state id and kept
// for as long as the instance lives (a state never changes), and the lookup of a state from a block name and the
// properties typed for it (a schematic's palette entry).
use super::baritone_block_state::{BlockState, Class, FluidKind};
use super::{Game, Resources};
use horizon_client::blocks::Fluid;
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub struct StateFactory {
    res: Arc<Resources>,
    registry: &'static BlockRegistry,
    cache: RefCell<HashMap<u32, Rc<BlockState>>>,
    by_name: RefCell<HashMap<String, Option<Rc<BlockState>>>>,
}

impl StateFactory {
    pub fn new(game: &Game<'_>) -> Self {
        Self { res: game.res.clone(), registry: game.pack.assets.registry, cache: RefCell::new(HashMap::new()), by_name: RefCell::new(HashMap::new()) }
    }

    /// The state with this id.
    pub fn state(&self, id: BlockStateId) -> Rc<BlockState> {
        let key = id.0 as u32;
        if let Some(s) = self.cache.borrow().get(&key) {
            return s.clone();
        }
        let built = Rc::new(self.build(id));
        self.cache.borrow_mut().insert(key, built.clone());
        built
    }

    /// The default state of the block with this name (`minecraft:stone`, or bare `stone`), remembered.
    pub fn default_state_of(&self, name: &str) -> Option<Rc<BlockState>> {
        if let Some(s) = self.by_name.borrow().get(name) {
            return s.clone();
        }
        let full = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
        let state = self.registry.default_state(&full).map(|id| self.state(id));
        self.by_name.borrow_mut().insert(name.to_owned(), state.clone());
        state
    }

    /// The state a block of this name has with the properties given (the others at their defaults), or the
    /// default state when they do not fit.
    pub fn resolve(&self, name: &str, props: &[(String, String)]) -> Option<Rc<BlockState>> {
        let mut descriptor = horizon_world::chunk::BlockState { name: name.into(), properties: Default::default() };
        for (k, v) in props {
            descriptor.properties.insert(k.as_str().into(), v.as_str().into());
        }
        let id = self.registry.resolve(&descriptor).ok().or_else(|| self.registry.default_state(name))?;
        Some(self.state(id))
    }

    fn build(&self, state: BlockStateId) -> BlockState {
        let facts = self.res.facts().get(state);
        let fluid_source = facts.fluid_level == 8 && !facts.fluid_falling;
        let fluid = match facts.fluid {
            Fluid::None => FluidKind::None,
            Fluid::Water if fluid_source => FluidKind::Water,
            Fluid::Water => FluidKind::FlowingWater,
            Fluid::Lava if fluid_source => FluidKind::Lava,
            Fluid::Lava => FluidKind::FlowingLava,
        };
        let props: Vec<(Arc<str>, Arc<str>)> = self.registry.state(state).map(|s| s.properties.iter().map(|(k, v)| (Arc::from(k.as_str()), Arc::from(v.as_str()))).collect()).unwrap_or_default();
        BlockState {
            id: state.0 as u32,
            name: facts.name.clone(),
            classes: Class::flags_of(facts.classes.iter().map(|c| &**c)),
            props: props.into(),
            fluid,
            fluid_amount: facts.fluid_level,
            hardness: facts.hardness,
            replaceable: facts.replaceable,
            // Block.isShapeFullBlock(getCollisionShape).
            full_collision: facts.full_collision,
            sturdy_top: (facts.sturdy_full >> 1) & 1 == 1,
            // isPathfindable(PathComputationType.LAND), from the game's own table of it.
            pathfindable: self.res.mob_data().state(state).flags & horizon_client::mobs::data::LAND != 0,
        }
    }
}
