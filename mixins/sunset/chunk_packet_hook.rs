// New Chunks and Player Activity Detector: each chunk packet's sections as the server wrote them, palettes in order,
// as the decoder thread starts on them.
fn sunset_chunk_packet(format: &ChunkFormat, p: &play::LevelChunkWithLight) {
    crate::game::sunset_new_chunks::arrived(p.x, p.z, &p.chunk.buffer, format.sections, format.biome_bits);
}
