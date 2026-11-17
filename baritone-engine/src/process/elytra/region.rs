// baritone.process.elytra.pathfinder.BaritoneRegion: reads the chunks of one of Baritone's cached region files
// (`r.X.Z.bcr`) into the octree chunks. The Java works out the dimension from the directory's name and gunzips the file
// itself; here the pathfinder knows its own dimension and a `RegionSource` hands over the file's bytes already gunzipped
// (the game's `FileStore` is one), since the cache directory of this port is laid out differently.
use super::baritone_cached_world::CACHED_REGION_MAGIC;
use super::baritone_elytra_chunk::Chunk;
use std::io::Read;

/// How a cached chunk is laid out: its height in blocks and the world y of its lowest block. The Java reads 384 for the
/// overworld and 256 for the other dimensions from a cache of its own dimensions; this port caches every dimension at the
/// game's own height from the game's own lowest block.
#[derive(Clone, Copy)]
pub struct Layout {
    pub height: usize,
    pub min_y: i32,
}

/// Where the region files are: the bytes of region (x, z) after the gunzip, or None if there is no such file.
pub trait RegionSource: Send + Sync {
    fn read_region(&self, region_x: i32, region_z: i32) -> Option<Vec<u8>>;
    fn layout(&self) -> Layout;
}

/// Reads the region (region_x, region_z) from the source, if there is one, and hands each chunk it holds to `sink`.
/// Returns whether a file was read.
/// `npf_min_y` is the world y that the pathfinder's octrees start at.
pub fn load(source: &dyn RegionSource, region_x: i32, region_z: i32, npf_min_y: i32, sink: &mut dyn FnMut(i32, i32, Chunk)) -> bool {
    let Some(bytes) = source.read_region(region_x, region_z) else { return false };
    let layout = source.layout();
    if let Err(e) = parse(&mut bytes.as_slice(), region_x, region_z, layout, layout.min_y - npf_min_y, sink) {
        eprintln!("[nether-pathfinder] exception thrown parsing region {region_x},{region_z}: {e}");
    }
    true
}

/// Parses an already decompressed region stream. A block at index y of the cached chunk is at y + `y_shift` in the octrees;
/// the ones that fall outside them are dropped.
pub fn parse(input: &mut dyn Read, region_x: i32, region_z: i32, layout: Layout, y_shift: i32, sink: &mut dyn FnMut(i32, i32, Chunk)) -> std::io::Result<()> {
    let mut int = [0u8; 4];
    input.read_exact(&mut int)?;
    if i32::from_be_bytes(int) != CACHED_REGION_MAGIC {
        eprintln!("[nether-pathfinder] bad magic for baritone region {region_x},{region_z}");
        return Ok(());
    }
    let height = layout.height;
    let mut data = vec![0u8; 2 * 16 * 16 * height / 8];
    for x in 0..32 {
        for z in 0..32 {
            let mut present = [0u8; 1];
            input.read_exact(&mut present)?;
            if present[0] == 1 {
                input.read_exact(&mut data)?;
                sink(x + 32 * region_x, z + 32 * region_z, parse_chunk(height, y_shift, &data));
            }
        }
    }
    Ok(())
}

fn position_index(x: usize, y: usize, z: usize) -> usize {
    (x << 1) | (z << 5) | (y << 9)
}

/// The two bits at bit index i, as `BitSet.toByteArray()` lays them out: bit n is in byte n / 8 at position n % 8, the
/// lowest bit first. The native reader took a byte's bits from the top down, which mirrored every aligned run of four
/// blocks along x.
fn get2_bits(i: usize, data: &[u8]) -> u8 {
    (data[i / 8] >> (i % 8)) & 0b11
}

fn parse_chunk(height: usize, y_shift: i32, data: &[u8]) -> Chunk {
    let mut chunk = Chunk::new();
    for y in 0..height {
        let target = y as i32 + y_shift;
        if !(0..super::baritone_elytra_chunk::HEIGHT).contains(&target) {
            continue;
        }
        for z in 0..16 {
            for x in 0..16 {
                if get2_bits(position_index(x, y, z), data) != 0 {
                    chunk.set_block(x as i32, target, z as i32, true);
                }
            }
        }
    }
    chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_stream_gives_its_chunks_and_their_solid_blocks() {
        let height = 256;
        let mut stream = CACHED_REGION_MAGIC.to_be_bytes().to_vec();
        for x in 0..32 {
            for z in 0..32 {
                if (x, z) == (3, 5) {
                    stream.push(1);
                    let mut data = vec![0u8; 2 * 16 * 16 * height / 8];
                    // Block (2, 70, 9) is something other than air (type bits 01), block (3, 70, 9) is air.
                    let bit = position_index(2, 70, 9);
                    data[bit / 8] |= 1 << (bit % 8);
                    stream.extend_from_slice(&data);
                } else {
                    stream.push(0);
                }
            }
        }
        let mut found = Vec::new();
        parse(&mut stream.as_slice(), 1, -1, Layout { height: 256, min_y: 0 }, 0, &mut |x, z, c| found.push((x, z, c))).unwrap();
        assert_eq!(found.len(), 1);
        let (x, z, chunk) = &found[0];
        assert_eq!((*x, *z), (3 + 32, 5 - 32));
        assert!(chunk.is_solid(2, 70, 9));
        assert!(!chunk.is_solid(3, 70, 9));
    }

    #[test]
    fn a_wrong_magic_reads_nothing() {
        let mut found = 0;
        parse(&mut [0u8, 0, 0, 1, 1].as_slice(), 0, 0, Layout { height: 256, min_y: 0 }, 0, &mut |_, _, _| found += 1).unwrap();
        assert_eq!(found, 0);
    }
}
