// baritone.api.schematic.format: reading a schematic from a file. Sponge `.schem` (WorldEdit, v2
// and v3), Litematica `.litematic` (any number of regions), MCEdit `.schematic` (numbered blocks, no data values, as the
// Java reads it) and vanilla structure `.nbt` are read.
use super::baritone_schematic::{BlockSpec, StaticSchematic};
use horizon_world::nbt::{read_named, Limits, Tag};
use std::io::Read;
use std::path::Path;

/// Read a schematic file. The error is a sentence for the player.
pub fn read(path: &Path) -> Result<StaticSchematic, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "schematic".into());
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out).map_err(|e| format!("{} is not a readable gzip file: {e}", path.display()))?;
        out
    } else {
        bytes
    };
    let (root, _) = read_named(&bytes, Limits::templates()).map_err(|e| format!("{} is not an NBT file: {e:?}", path.display()))?;
    let tag = root.tag;
    match extension.as_str() {
        "litematic" => litematic(&tag, name),
        "schem" => sponge(&tag, name),
        "nbt" => structure(&tag, name),
        "schematic" => mcedit(&tag, name),
        other => Err(format!("I do not know the .{other} format. .schematic, .schem, .litematic and .nbt are read.")),
    }
}

fn int(tag: Option<&Tag>) -> Option<i32> {
    match tag? {
        Tag::Int(v) => Some(*v),
        Tag::Short(v) => Some(i32::from(*v)),
        Tag::Byte(v) => Some(i32::from(*v)),
        Tag::Long(v) => Some(*v as i32),
        _ => None,
    }
}

fn text(tag: Option<&Tag>) -> Option<String> {
    match tag? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

fn compound_entries(tag: &Tag) -> Vec<(String, &Tag)> {
    match tag {
        Tag::Compound(map) => map.iter().map(|(k, v)| (k.to_string_lossy(), v)).collect(),
        _ => Vec::new(),
    }
}

/// `{Name: "minecraft:oak_stairs", Properties: {facing: "north"}}` as a block.
fn palette_entry(tag: &Tag) -> BlockSpec {
    let mut spec = BlockSpec::named(&text(tag.get("Name")).unwrap_or_else(|| "minecraft:air".into()));
    if let Some(props) = tag.get("Properties") {
        for (k, v) in compound_entries(props) {
            if let Some(v) = text(Some(v)) {
                spec.props.push((k, v));
            }
        }
    }
    spec
}

/// MCEdit `.schematic` (`MCEditSchematic`): a byte per block (`Blocks`, with the high nibbles in `AddBlocks`), looked up in the old
/// id table. The data values are not read, so every block is the default state of its block.
fn mcedit(root: &Tag, name: String) -> Result<StaticSchematic, String> {
    let materials = text(root.get("Materials")).unwrap_or_default();
    if materials != "Alpha" {
        return Err(format!("bad schematic {materials}"));
    }
    let (w, h, l) = (
        int(root.get("Width")).ok_or("The schematic has no Width.")?,
        int(root.get("Height")).ok_or("The schematic has no Height.")?,
        int(root.get("Length")).ok_or("The schematic has no Length.")?,
    );
    dimensions_check(w, h, l)?;
    let Some(Tag::ByteArray(blocks)) = root.get("Blocks") else { return Err("The schematic has no Blocks.".into()) };
    let additional: Option<&Vec<u8>> = match root.get("AddBlocks") {
        Some(Tag::ByteArray(add)) => Some(add),
        _ => None,
    };
    let names = super::baritone_schematic_legacy::mcedit_block_names((w * h * l) as usize, blocks, additional.map(Vec::as_slice));
    let mut out = StaticSchematic { size: [w, h, l], blocks: Vec::new(), name };
    out.blocks = names.into_iter().map(|n| Some(BlockSpec::named(n))).collect();
    Ok(out)
}

fn dimensions_check(w: i32, h: i32, l: i32) -> Result<(), String> {
    if w <= 0 || h <= 0 || l <= 0 || i64::from(w) * i64::from(h) * i64::from(l) > 50_000_000 {
        return Err(format!("A schematic of {w} x {h} x {l} is empty or too big to build."));
    }
    Ok(())
}

/// Sponge schematic, version 2 (fields at the top) or 3 (inside `Schematic`, blocks in `Blocks`).
fn sponge(root: &Tag, name: String) -> Result<StaticSchematic, String> {
    let body = root.get("Schematic").unwrap_or(root);
    let (w, h, l) = (
        int(body.get("Width")).ok_or("The schematic has no Width.")?,
        int(body.get("Height")).ok_or("The schematic has no Height.")?,
        int(body.get("Length")).ok_or("The schematic has no Length.")?,
    );
    dimensions_check(w, h, l)?;
    let (palette_tag, data_tag) = match body.get("Blocks") {
        Some(blocks) => (blocks.get("Palette"), blocks.get("Data")),
        None => (body.get("Palette"), body.get("BlockData")),
    };
    let palette_tag = palette_tag.ok_or("The schematic has no block palette.")?;
    let mut palette: Vec<BlockSpec> = Vec::new();
    for (state, id) in compound_entries(palette_tag) {
        let id = int(Some(id)).ok_or("A palette entry has no number.")? as usize;
        if palette.len() <= id {
            palette.resize(id + 1, BlockSpec::air());
        }
        palette[id] = BlockSpec::parse(&state);
    }
    let data = match data_tag {
        Some(Tag::ByteArray(b)) => b,
        _ => return Err("The schematic has no block data.".into()),
    };
    let volume = (w * h * l) as usize;
    let mut blocks = Vec::with_capacity(volume);
    let mut i = 0;
    while blocks.len() < volume {
        // A varint.
        let (mut value, mut shift) = (0usize, 0);
        loop {
            let byte = *data.get(i).ok_or("The block data ends early.")?;
            i += 1;
            value |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err("The block data is corrupt.".into());
            }
        }
        let index = blocks.len();
        blocks.push(Some(palette.get(value).cloned().ok_or_else(|| format!("Invalid Palette Index {index}"))?));
    }
    Ok(StaticSchematic { size: [w, h, l], blocks, name })
}

/// Vanilla structure template: `size`, `palette`, `blocks[{pos, state}]`. Spaces it does not list
/// are left alone.
fn structure(root: &Tag, name: String) -> Result<StaticSchematic, String> {
    let size = match root.get("size") {
        Some(Tag::List(v)) if v.len() == 3 => [int(v.first()).unwrap_or(0), int(v.get(1)).unwrap_or(0), int(v.get(2)).unwrap_or(0)],
        _ => return Err("The structure has no size.".into()),
    };
    dimensions_check(size[0], size[1], size[2])?;
    let palette: Vec<BlockSpec> = match root.get("palette") {
        Some(Tag::List(v)) => v.iter().map(palette_entry).collect(),
        _ => return Err("The structure has no palette.".into()),
    };
    let mut out = StaticSchematic { size, blocks: vec![None; (size[0] * size[1] * size[2]) as usize], name };
    if let Some(Tag::List(list)) = root.get("blocks") {
        for b in list {
            let (Some(Tag::List(pos)), Some(state)) = (b.get("pos"), int(b.get("state"))) else {
                continue;
            };
            let p = [int(pos.first()).unwrap_or(-1), int(pos.get(1)).unwrap_or(-1), int(pos.get(2)).unwrap_or(-1)];
            if (0..3).all(|i| p[i] >= 0 && p[i] < size[i]) {
                let at = out.index(p);
                out.blocks[at] = palette.get(state as usize).cloned();
            }
        }
    }
    Ok(out)
}

/// Litematica: regions with a position, a size (negative when drawn backwards), a palette and a
/// bit-packed array of palette indices.
fn litematic(root: &Tag, name: String) -> Result<StaticSchematic, String> {
    let regions = root.get("Regions").ok_or("The litematic has no regions.")?;
    struct Region {
        origin: [i32; 3],
        size: [i32; 3],
        palette: Vec<BlockSpec>,
        states: Vec<i64>,
    }
    let mut parsed: Vec<Region> = Vec::new();
    for (_, region) in compound_entries(regions) {
        let vec3 = |key: &str| -> Option<[i32; 3]> {
            let t = region.get(key)?;
            Some([int(t.get("x"))?, int(t.get("y"))?, int(t.get("z"))?])
        };
        let (Some(pos), Some(size)) = (vec3("Position"), vec3("Size")) else {
            continue;
        };
        let palette: Vec<BlockSpec> = match region.get("BlockStatePalette") {
            Some(Tag::List(v)) => v.iter().map(palette_entry).collect(),
            _ => continue,
        };
        let states = match region.get("BlockStates") {
            Some(Tag::LongArray(v)) => v.clone(),
            _ => continue,
        };
        // A negative size means the region was drawn the other way: its corner is at the far end.
        let origin = [
            if size[0] < 0 { pos[0] + size[0] + 1 } else { pos[0] },
            if size[1] < 0 { pos[1] + size[1] + 1 } else { pos[1] },
            if size[2] < 0 { pos[2] + size[2] + 1 } else { pos[2] },
        ];
        parsed.push(Region { origin, size: [size[0].abs(), size[1].abs(), size[2].abs()], palette, states });
    }
    if parsed.is_empty() {
        return Err("The litematic holds no region I can read.".into());
    }
    let lo = [0, 1, 2].map(|i| parsed.iter().map(|r| r.origin[i]).min().unwrap_or(0));
    let hi = [0, 1, 2].map(|i| parsed.iter().map(|r| r.origin[i] + r.size[i]).max().unwrap_or(0));
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    dimensions_check(size[0], size[1], size[2])?;
    let mut out = StaticSchematic { size, blocks: vec![None; (size[0] * size[1] * size[2]) as usize], name };
    for r in &parsed {
        let bits = (32 - (r.palette.len().max(2) as u32 - 1).leading_zeros()).max(2) as usize;
        let mask = (1u64 << bits) - 1;
        let volume = (r.size[0] * r.size[1] * r.size[2]) as usize;
        for idx in 0..volume {
            let start = idx * bits;
            let (word, offset) = (start >> 6, start & 63);
            let Some(&first) = r.states.get(word) else {
                break;
            };
            let mut value = (first as u64 >> offset) & mask;
            if offset + bits > 64 {
                if let Some(&second) = r.states.get(word + 1) {
                    value |= ((second as u64) << (64 - offset)) & mask;
                }
            }
            let x = (idx as i32) % r.size[0];
            let z = ((idx as i32) / r.size[0]) % r.size[2];
            let y = (idx as i32) / (r.size[0] * r.size[2]);
            let p = [r.origin[0] - lo[0] + x, r.origin[1] - lo[1] + y, r.origin[2] - lo[2] + z];
            let at = out.index(p);
            // CompositeSchematic: where regions overlap, the first one put in holds the block.
            if out.blocks[at].is_none() {
                out.blocks[at] = r.palette.get(value as usize).cloned();
            }
        }
    }
    Ok(out)
}
