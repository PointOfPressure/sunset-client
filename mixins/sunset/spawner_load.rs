fn sunset_spawner_load(world: &mut World, chunk: &Chunk) {
    crate::chunk_map::sunset_spawner_index::loaded(&mut world.chunks, &chunk.block_entities);
}
