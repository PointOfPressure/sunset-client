// The columns the client forgets (horizon-baritone's ChunkEvent UNLOAD): each chunk still held is kept for its packer.
fn baritone_unload(game: &crate::game::Game<'_>, gone: &horizon_client::fast_hash::FxHashSet<(i32, i32)>) {
    for &(x, z) in gone {
        if let Some(chunk) = game.world.chunks.get(&(x, z)) {
            crate::game::baritone_unload::push(x, z, chunk.clone());
        }
    }
}
