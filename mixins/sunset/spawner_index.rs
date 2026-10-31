// The resident native chunk map owns this sparse index.
use super::ChunkMap;
use super::sunset_spawner_data::Pos;
use crate::chunk::block_entity::BlockEntity;

pub fn state(this: &mut ChunkMap, pos: Pos, name: &str) {
    this.__horizon_mixins_sunset_spawner_registry_mut().observe(
        pos, matches!(name, "minecraft:spawner" | "minecraft:trial_spawner"),
    );
}

pub fn loaded(this: &mut ChunkMap, entities: &[(Pos, BlockEntity)]) {
    // One update per incoming entity alongside native chunk arrival. No walk
    // over unrelated entities already resident in this or another world.
    let index = this.__horizon_mixins_sunset_spawner_registry_mut();
    for (pos, entity) in entities {
        index.observe(*pos, matches!(entity, BlockEntity::Spawner(_) | BlockEntity::TrialSpawner(_)));
    }
}

pub fn remove(this: &mut ChunkMap, pos: Pos) {
    this.__horizon_mixins_sunset_spawner_registry_mut().observe(pos, false);
}

pub fn batch(this: &ChunkMap, after: Option<Pos>) -> (Vec<Pos>, bool) {
    let index = this.__horizon_mixins_sunset_spawner_registry();
    (index.batch(after), index.saturated)
}

pub fn scan(this: &ChunkMap, after: Option<Pos>, out: &mut Vec<Pos>) -> bool {
    let index = this.__horizon_mixins_sunset_spawner_registry();
    index.fill_batch(after, out);
    index.saturated
}

pub fn clear(this: &mut ChunkMap) {
    this.__horizon_mixins_sunset_spawner_registry_mut().clear();
}
