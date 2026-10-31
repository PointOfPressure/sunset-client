// The real adapter compiled against a minimal representation of the native API.
#[path = "../mixins/sunset/spawner_data.rs"]
mod sunset_spawner_data;

mod chunk {
    pub mod block_entity {
        pub enum BlockEntity { Spawner(()), TrialSpawner(()), Chest }
    }
}
use chunk::block_entity::BlockEntity;
use sunset_spawner_data::{Index, Pos};
use std::collections::HashMap;

#[derive(Default)]
struct World {
    block_entities: HashMap<Pos, BlockEntity>,
}
#[derive(Default)]
struct ChunkMap { index: Index }
impl ChunkMap {
    fn __horizon_mixins_sunset_spawner_registry(&self) -> &Index { &self.index }
    fn __horizon_mixins_sunset_spawner_registry_mut(&mut self) -> &mut Index { &mut self.index }
}
#[path = "../mixins/sunset/spawner_index.rs"]
mod sunset_spawner_index;

#[test]
fn incoming_chunks_and_sync_are_instance_owned_and_only_track_spawners() {
    let mut client = ChunkMap::default();
    let server = ChunkMap::default();
    let mut world = World::default();
    let entries = vec![((1, 64, 1), BlockEntity::Spawner(())),
        ((2, 64, 2), BlockEntity::TrialSpawner(())), ((3, 64, 3), BlockEntity::Chest)];
    sunset_spawner_index::loaded(&mut client, &entries);
    assert_eq!(sunset_spawner_index::batch(&client, None).0, vec![(1, 64, 1), (2, 64, 2)]);
    assert!(sunset_spawner_index::batch(&server, None).0.is_empty());
    // Moving the native world carries its index, without pointer identity.
    let mut moved = client;
    world.block_entities.insert((1, 64, 1), BlockEntity::Chest);
    sunset_spawner_index::state(&mut moved, (1, 64, 1), "minecraft:chest");
    assert_eq!(sunset_spawner_index::batch(&moved, None).0, vec![(2, 64, 2)]);
    world.block_entities.insert((4, 64, 4), BlockEntity::Spawner(()));
    sunset_spawner_index::state(&mut moved, (4, 64, 4), "minecraft:spawner");
    assert_eq!(sunset_spawner_index::batch(&moved, None).0, vec![(2, 64, 2), (4, 64, 4)]);
    sunset_spawner_index::clear(&mut moved);
    assert!(sunset_spawner_index::batch(&moved, None).0.is_empty());
    moved = ChunkMap::default();
    assert!(sunset_spawner_index::batch(&moved, None).0.is_empty());
}

#[test]
fn unload_prunes_stale_candidates_with_bounded_forward_progress() {
    let mut world = World::default();
    let mut streaming = ChunkMap::default();
    for x in 0..1000 {
        let pos = (x, 64, 0);
        world.block_entities.insert(pos, BlockEntity::Spawner(()));
        sunset_spawner_index::state(&mut streaming, pos, "minecraft:spawner");
    }
    // The production tick also checks actual loaded chunk and block state.
    world.block_entities.retain(|pos, _| pos.0 >= 500);
    let mut cursor = None;
    let mut visits = 0;
    loop {
        let batch = sunset_spawner_index::batch(&streaming, cursor).0;
        assert!(batch.len() <= sunset_spawner_data::SCAN_BUDGET);
        if batch.is_empty() { break; }
        for pos in batch {
            cursor = Some(pos);
            visits += 1;
            if !world.block_entities.contains_key(&pos) { sunset_spawner_index::remove(&mut streaming, pos); }
        }
    }
    assert_eq!(visits, 1000);
    let mut survivors = Vec::new();
    let mut cursor = None;
    loop {
        let batch = sunset_spawner_index::batch(&streaming, cursor).0;
        if batch.is_empty() { break; }
        cursor = batch.last().copied();
        survivors.extend(batch);
    }
    assert_eq!(survivors.len(), 500);
    assert!(survivors.iter().all(|p| p.0 >= 500));
}

#[test]
fn reusable_scan_buffer_is_bounded_and_known_far_candidates_need_no_label() {
    let mut streaming = ChunkMap::default();
    let mut world = World::default();
    for x in 0..1000 {
        world.block_entities.insert((x, 0, 0), BlockEntity::Spawner(()));
        sunset_spawner_index::state(&mut streaming, (x, 0, 0), "minecraft:spawner");
    }
    let mut out = Vec::new();
    assert!(!sunset_spawner_index::scan(&streaming, None, &mut out));
    assert_eq!(out.len(), sunset_spawner_data::SCAN_BUDGET);
    let capacity = out.capacity();
    sunset_spawner_index::scan(&streaming, Some((255, 0, 0)), &mut out);
    assert_eq!(out[0], (256, 0, 0));
    assert_eq!(out.capacity(), capacity);
    let mut tracker = sunset_spawner_data::Tracker::default();
    tracker.context("overworld", true);
    for x in 0..100 {
        tracker.observe(sunset_spawner_data::Found::new((x, 0, 0), None, false, f64::from(x)), false);
    }
    assert!(!tracker.needs((99, 0, 0), 99.0));
    assert!(tracker.needs((99, 0, 0), 0.0));
    assert!(tracker.needs((100, 0, 0), 100.0));
    assert!(!tracker.warned);
}
