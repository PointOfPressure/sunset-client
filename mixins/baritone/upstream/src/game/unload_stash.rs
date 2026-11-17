// baritone.api.event.events.ChunkEvent, the unload half: the chunk the server is about to drop, kept for the packer. It sits in the
// streaming module of the game, where the chunk is removed, and the mod takes what it holds once a tick. The Java packs a chunk
// "pre unload" for the same reason: what was built or broken in it since it was loaded is only seen now.
use crate::chunk::Chunk;
use std::cell::RefCell;
use std::sync::Arc;

/// More than this waiting and the oldest are dropped: a teleport can unload hundreds at once, and each chunk kept is the
/// memory of the chunk. The Java's queue holds 2000 (`chunkPackerQueueMaxSize`).
const LIMIT: usize = 512;

thread_local! {
    static UNLOADING: RefCell<Vec<(i32, i32, Arc<Chunk>)>> = const { RefCell::new(Vec::new()) };
}

/// A chunk about to leave the world (`Streaming::forget`).
pub fn push(x: i32, z: i32, chunk: Arc<Chunk>) {
    UNLOADING.with_borrow_mut(|list| {
        if list.len() >= LIMIT {
            list.remove(0);
        }
        list.push((x, z, chunk));
    });
}

/// Everything held since the last call.
pub fn take() -> Vec<(i32, i32, Arc<Chunk>)> {
    UNLOADING.with_borrow_mut(std::mem::take)
}
