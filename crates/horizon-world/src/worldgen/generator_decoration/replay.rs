//! Replays placed features of one chunk's decoration against a caller's view of the world, without generating
//! terrain: the generator's decoration seed, feature seeds, placement chains and features, with every block they
//! would set reported to the caller. For tools that predict what generation placed (ore finders, seed checks).
//!
//! Reads see the blocks this replay has already set over the caller's view; nothing is written to any level.

use super::{
    Decoration, FeatureIdentity, FeaturePlacer, FeatureWorld, GeneratorData, HeightmapType, PlacementContext,
    PlacementQueryError, WorldGenerationContext, WorldgenRandom, XoroshiroRandom,
};
use crate::registry::BlockStateId;
use std::collections::HashMap;
use std::sync::Arc;

/// What a replay reads of the world.
pub trait ReplayView {
    /// The block at a position; `None` where it is not known (no chunk loaded).
    fn block(&self, position: [i32; 3]) -> Option<BlockStateId>;
    /// The biome at a position, as an index into [`DecorationReplay::biome_names`].
    fn biome(&self, position: [i32; 3]) -> Option<u32>;
    /// One above the highest solid block of a column (`OCEAN_FLOOR_WG`).
    fn floor(&self, x: i32, z: i32) -> Option<i32>;
}

/// One dimension's decoration, ready to replay for any seed.
pub struct DecorationReplay {
    decoration: Arc<Decoration>,
    biome_names: Vec<Arc<str>>,
    build: (i32, i32),
    generation: WorldGenerationContext,
}

impl DecorationReplay {
    /// The decoration of `preset`'s `dimension` (built once and cached, as opening a world builds it). `build` is
    /// the level's build range (minimum y, height); `generation` the generator's (minimum y, height, sea level).
    /// `None` when the dimension has no feature decoration.
    pub fn new(
        data: &Arc<GeneratorData>,
        preset: &str,
        dimension: &str,
        build: (i32, i32),
        generation: (i32, i32, i32),
    ) -> Result<Option<Self>, String> {
        let decoration = data.decoration_of(preset, dimension).map_err(|e| e.to_string())?;
        Ok(decoration.map(|decoration| Self {
            decoration,
            biome_names: data.biome_names().to_vec(),
            build,
            generation: WorldGenerationContext::new(generation.0, generation.1, generation.2),
        }))
    }

    /// The placed-feature roots by identity (vanilla's feature index order).
    pub fn features(&self) -> &[String] {
        &self.decoration.names
    }

    /// Whether root `feature` can run on this build (its placement and feature are implemented).
    pub fn runs(&self, feature: u32) -> bool {
        self.decoration.features.get(feature as usize).is_some_and(|f| f.is_ok())
    }

    /// The biome names [`ReplayView::biome`] indexes.
    pub fn biome_names(&self) -> &[Arc<str>] {
        &self.biome_names
    }

    /// Whether `biome`'s generation settings list root `feature`.
    pub fn biome_has(&self, biome: u32, feature: u32) -> bool {
        self.decoration
            .biome_features
            .get(biome as usize)
            .and_then(|bits| bits.get(feature as usize / 64))
            .is_some_and(|word| word >> (feature % 64) & 1 != 0)
    }

    /// Replays roots `features` (identities, in this order) for chunk (`x`, `z`) of a world made from `seed`, each
    /// with its own feature seed. `set` gets each write: the index into `features`, the position and the state.
    pub fn replay(
        &self,
        seed: i64,
        x: i32,
        z: i32,
        features: &[u32],
        view: &impl ReplayView,
        set: &mut impl FnMut(usize, [i32; 3], BlockStateId),
    ) {
        let origin = [x.wrapping_shl(4), self.build.0, z.wrapping_shl(4)];
        let mut random = WorldgenRandom::new(XoroshiroRandom::new(0));
        let decoration_seed = random.set_decoration_seed(seed, origin[0], origin[2]);
        let mut world = Replay { replay: self, view, placed: HashMap::new(), root: 0, set };
        let mut placer = FeaturePlacer::new();
        let blocks = self.decoration.blocks;
        for (root, &identity) in features.iter().enumerate() {
            let Some(Ok(feature)) = self.decoration.features.get(identity as usize) else { continue };
            let Some((index, step)) = self.decoration.plan.seed(FeatureIdentity(identity)) else { continue };
            random.set_feature_seed(decoration_seed, index as i32, step as i32);
            world.root = root;
            let _ = placer.place_with_biome_check(
                &feature.placement,
                &mut world,
                &mut random,
                origin,
                FeatureIdentity(identity),
                |w, r, p| feature.feature.place(w, r, p, blocks),
            );
        }
    }
}

struct Replay<'a, V: ReplayView, S: FnMut(usize, [i32; 3], BlockStateId)> {
    replay: &'a DecorationReplay,
    view: &'a V,
    placed: HashMap<[i32; 3], BlockStateId>,
    root: usize,
    set: &'a mut S,
}

impl<V: ReplayView, S: FnMut(usize, [i32; 3], BlockStateId)> Replay<'_, V, S> {
    fn read(&self, p: [i32; 3]) -> Option<BlockStateId> {
        self.placed.get(&p).copied().or_else(|| self.view.block(p))
    }

    fn write(&mut self, p: [i32; 3], state: BlockStateId) {
        self.placed.insert(p, state);
        (self.set)(self.root, p, state);
    }
}

impl<V: ReplayView, S: FnMut(usize, [i32; 3], BlockStateId)> FeatureWorld for Replay<'_, V, S> {
    type Error = ();
    fn block(&mut self, p: [i32; 3]) -> Result<BlockStateId, ()> {
        self.read(p).ok_or(())
    }
    fn set_block(&mut self, p: [i32; 3], state: BlockStateId, _: i32) -> Result<bool, ()> {
        self.write(p, state);
        Ok(true)
    }
}

impl<V: ReplayView, S: FnMut(usize, [i32; 3], BlockStateId)> PlacementContext for Replay<'_, V, S> {
    fn generation(&self) -> WorldGenerationContext {
        self.replay.generation
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.replay.build.0)
    }
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.replay.build.0 + self.replay.build.1 - 1)
    }
    fn height(&mut self, _: HeightmapType, x: i32, z: i32) -> Result<i32, PlacementQueryError> {
        self.view.floor(x, z).ok_or(PlacementQueryError::Unavailable)
    }
    fn biome_allows_feature(&mut self, p: [i32; 3], feature: FeatureIdentity) -> Result<bool, PlacementQueryError> {
        let biome = self.view.biome(p).ok_or(PlacementQueryError::Unavailable)?;
        Ok(self.replay.biome_has(biome, feature.0))
    }
    fn block_state(&mut self, p: [i32; 3]) -> Result<BlockStateId, PlacementQueryError> {
        self.read(p).ok_or(PlacementQueryError::Unavailable)
    }
    fn set_section_block(&mut self, p: [i32; 3], state: BlockStateId) -> Result<(), PlacementQueryError> {
        self.write(p, state);
        Ok(())
    }
}
