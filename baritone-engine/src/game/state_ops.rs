// What a rotation or a mirror does to a block state, and what state a block has with some properties: the answer the schematic
// wrappers (`baritone_schematic_wrappers`) ask of the game. Rotating and mirroring use the same tables Horizon's own structure
// placement does (`worldgen/blocks/structure-blocks.json`), so a stair turned here faces where `/place` would turn it.
use super::baritone_block_state::BlockState;
use super::baritone_schematic_wrappers::StateOps;
use super::baritone_state_factory::StateFactory;
use super::Game;
use horizon_world::registry::BlockStateId;
use horizon_world::worldgen::structure::{Mirror, Rotation, StateTransforms};
use std::rc::Rc;
use std::sync::OnceLock;

static TRANSFORMS: OnceLock<Option<StateTransforms>> = OnceLock::new();

pub struct GameStateOps {
    factory: Rc<StateFactory>,
    /// None when the game's tables could not be read: nothing is turned then.
    transforms: Option<&'static StateTransforms>,
}

impl GameStateOps {
    pub fn new(game: &Game<'_>, factory: Rc<StateFactory>) -> Self {
        let transforms = TRANSFORMS
            .get_or_init(|| {
                let json = std::fs::read_to_string(game.options.resources.join("worldgen/blocks/structure-blocks.json")).ok()?;
                StateTransforms::from_json(&json, game.pack.assets.registry.len()).ok()
            })
            .as_ref();
        Self { factory, transforms }
    }
}

impl StateOps for GameStateOps {
    fn rotate(&self, state: &Rc<BlockState>, quarter_turns: usize) -> Rc<BlockState> {
        match (self.transforms, Rotation::ALL.get(quarter_turns)) {
            (Some(t), Some(&rotation)) if quarter_turns != 0 => self.factory.state(t.rotate(BlockStateId(state.id), rotation)),
            _ => state.clone(),
        }
    }

    fn mirror(&self, state: &Rc<BlockState>, mirror: usize) -> Rc<BlockState> {
        let kind = match mirror {
            1 => Mirror::LeftRight,
            2 => Mirror::FrontBack,
            _ => return state.clone(),
        };
        match self.transforms {
            Some(t) => self.factory.state(t.mirror(BlockStateId(state.id), kind)),
            None => state.clone(),
        }
    }

    fn resolve(&self, name: &str, props: &[(String, String)]) -> Option<Rc<BlockState>> {
        let full = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
        self.factory.resolve(&full, props)
    }

    fn air(&self) -> Rc<BlockState> {
        self.factory.default_state_of("minecraft:air").expect("the game has air")
    }
}
