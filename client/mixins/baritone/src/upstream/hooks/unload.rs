fn baritone_unload(world: &World, gone: &FxHashSet<(i32, i32)>) {
    for &(x, z) in gone {
        if let Some(chunk) = world.chunks.get(&(x, z)) {
            baritone_unload::push(x, z, chunk.clone());
        }
    }
}
