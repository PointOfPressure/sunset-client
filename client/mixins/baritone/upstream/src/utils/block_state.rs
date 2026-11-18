// What Baritone asks of a net.minecraft.world.level.block.state.BlockState: the block's class chain
// (`state.getBlock() instanceof SlabBlock`), its name (`block == Blocks.LADDER`), its properties
// (`state.getValue(SlabBlock.TYPE)`), its fluid, its hardness and the shape of its collision box.
// The game fills one of these per block state; the pathfinder only reads it.
use std::sync::Arc;

/// The block classes the pathfinder tests with `instanceof`. Horizon exports each block's Java class
/// chain, so a block has the flag when any class of its chain has the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    Air,
    BaseFire,
    AbstractSkull,
    ShulkerBox,
    Slab,
    TrapDoor,
    Door,
    FenceGate,
    Carpet,
    SnowLayer,
    Cauldron,
    Azalea,
    AmethystCluster,
    Stair,
    StainedGlass,
    Leaves,
    Falling,
    Liquid,
    Infested,
    BambooStalk,
    MovingPiston,
    Scaffolding,
    PointedDripstone,
    Waterlily,
    EndPortal,
    Skull,
}

impl Class {
    /// The Java class name each flag stands for.
    pub const NAMES: &'static [(&'static str, Class)] = &[
        ("AirBlock", Class::Air),
        ("BaseFireBlock", Class::BaseFire),
        ("AbstractSkullBlock", Class::AbstractSkull),
        ("ShulkerBoxBlock", Class::ShulkerBox),
        ("SlabBlock", Class::Slab),
        ("TrapDoorBlock", Class::TrapDoor),
        ("DoorBlock", Class::Door),
        ("FenceGateBlock", Class::FenceGate),
        ("CarpetBlock", Class::Carpet),
        ("SnowLayerBlock", Class::SnowLayer),
        ("CauldronBlock", Class::Cauldron),
        ("AzaleaBlock", Class::Azalea),
        ("AmethystClusterBlock", Class::AmethystCluster),
        ("StairBlock", Class::Stair),
        ("StainedGlassBlock", Class::StainedGlass),
        ("LeavesBlock", Class::Leaves),
        ("FallingBlock", Class::Falling),
        ("LiquidBlock", Class::Liquid),
        ("InfestedBlock", Class::Infested),
        ("BambooStalkBlock", Class::BambooStalk),
        ("MovingPistonBlock", Class::MovingPiston),
        ("ScaffoldingBlock", Class::Scaffolding),
        ("PointedDripstoneBlock", Class::PointedDripstone),
        ("WaterlilyBlock", Class::Waterlily),
        ("EndPortalBlock", Class::EndPortal),
        ("SkullBlock", Class::Skull),
    ];

    pub fn bit(self) -> u64 {
        1 << (self as u8)
    }

    /// The flags of a class chain, most specific class first.
    pub fn flags_of<'a>(chain: impl IntoIterator<Item = &'a str>) -> u64 {
        let mut flags = 0;
        for name in chain {
            for (n, class) in Self::NAMES {
                if *n == name {
                    flags |= class.bit();
                }
            }
        }
        flags
    }
}

/// Which fluid is in a block, if any: `state.getFluidState().getType()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidKind {
    None,
    /// Still water, `Fluids.WATER`.
    Water,
    /// Flowing water, `Fluids.FLOWING_WATER`.
    FlowingWater,
    Lava,
    FlowingLava,
}

#[derive(Clone, Debug)]
pub struct BlockState {
    /// The game's id of the state (0 in tests).
    pub id: u32,
    /// `minecraft:oak_slab`.
    pub name: Arc<str>,
    pub classes: u64,
    /// `type=bottom`, `waterlogged=false` and so on.
    pub props: Arc<[(Arc<str>, Arc<str>)]>,
    pub fluid: FluidKind,
    /// `FluidState.getAmount`: 8 for a source, 1 to 7 flowing, 0 with no fluid.
    pub fluid_amount: u8,
    #[allow(dead_code)]
    pub hardness: f32,
    /// `state.canBeReplaced()`.
    pub replaceable: bool,
    /// `Block.isShapeFullBlock(state.getCollisionShape(...))`.
    pub full_collision: bool,
    /// `state.isFaceSturdy(.., Direction.UP)`.
    pub sturdy_top: bool,
    /// `state.isPathfindable(PathComputationType.LAND)`.
    pub pathfindable: bool,
}

impl BlockState {
    pub fn is(&self, class: Class) -> bool {
        self.classes & class.bit() != 0
    }

    /// `block == Blocks.X`, with the name given without the namespace.
    pub fn is_block(&self, bare: &str) -> bool {
        self.name.strip_prefix("minecraft:").is_some_and(|n| n == bare)
    }

    pub fn bare_name(&self) -> &str {
        self.name.strip_prefix("minecraft:").unwrap_or(&self.name)
    }

    /// `state.getValue(property)` as text.
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| &**k == key).map(|(_, v)| &**v)
    }

    pub fn has_prop(&self, key: &str) -> bool {
        self.prop(key).is_some()
    }

    pub fn flag(&self, key: &str) -> bool {
        self.prop(key) == Some("true")
    }

    /// `!state.getFluidState().isEmpty()`.
    pub fn has_fluid(&self) -> bool {
        self.fluid != FluidKind::None
    }

    /// Air as the common case: a block of the AirBlock class (air, cave air, void air).
    pub fn is_air(&self) -> bool {
        self.is(Class::Air)
    }

    /// Whether this is a source (a still fluid, not flowing).
    pub fn fluid_is_source(&self) -> bool {
        matches!(self.fluid, FluidKind::Water | FluidKind::Lava)
    }

    /// `state == other` for the states the pathfinder compares: the same block and the same properties.
    #[allow(dead_code)]
    pub fn same_as(&self, other: &BlockState) -> bool {
        self.name == other.name && self.props == other.props
    }
}

/// A block state from its text form, `minecraft:oak_slab[type=bottom,waterlogged=false]`: the way the
/// tests and the game both describe one.
pub fn parse_props(state_text: &str) -> Arc<[(Arc<str>, Arc<str>)]> {
    let inside = match state_text.split_once('[') {
        Some((_, rest)) => rest.trim_end_matches(']'),
        None => "",
    };
    inside
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (Arc::from(k.trim()), Arc::from(v.trim())))
        .collect::<Vec<_>>()
        .into()
}
