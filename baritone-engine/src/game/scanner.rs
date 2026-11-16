// baritone.cache.FasterWorldScanner (the scanning of the loaded chunks) and what BlockOptionalMeta asks of the
// game: which block states a lookup stands for, and what its blocks drop.
use super::baritone_bom::{BlockOptionalMeta, BlockOptionalMetaLookup};
use super::baritone_utils::Pos;
use super::baritone_world_scanner::{chunk_range, section_order};
use super::Game;
use horizon_core::random::LegacyRandom;
use horizon_client::items::ItemStack;
use std::collections::HashSet;
use std::sync::Arc;

/// Every block state id the lookup stands for (`getAllBlockStates` of each of its blocks).
pub fn lookup_state_ids(game: &Game<'_>, lookup: &BlockOptionalMetaLookup) -> HashSet<u32> {
    let mut ids = HashSet::new();
    for bom in lookup.blocks() {
        let with_properties = !bom.properties().is_empty();
        for (i, s) in game.res.facts().states.iter().enumerate() {
            if *s.name != *bom.block() {
                continue;
            }
            if with_properties {
                let text = game.state_string(horizon_world::registry::BlockStateId(i as _));
                let props = super::baritone_block_state::parse_props(&text);
                let has = |k: &str, v: &str| props.iter().any(|(pk, pv)| &**pk == k && &**pv == v);
                if !bom.properties().iter().all(|(k, v)| has(k, v)) {
                    continue;
                }
            }
            ids.insert(i as u32);
        }
    }
    ids
}

/// `scanChunkRadius`: the positions of the wanted blocks in the loaded chunks around `feet`, in the order the
/// scanner finds them (chunks spiralling out, each chunk's sections from the player's level outward), at most
/// `max` of them. `max_search_radius` is in chunks.
pub fn scan_chunk_radius(game: &Game<'_>, ids: &HashSet<u32>, feet: Pos, max: usize, max_search_radius: i32) -> Vec<Pos> {
    let mut found: Vec<Pos> = Vec::new();
    if ids.is_empty() {
        return found;
    }
    let player_section = (feet[1] - horizon_client::chunk::MIN_Y) >> 4;
    for (cx, cz) in chunk_range(feet[0] >> 4, feet[2] >> 4, max_search_radius) {
        let Some(chunk) = game.world.chunks.get(&(cx, cz)) else { continue };
        let mut in_chunk = 0usize;
        for s in section_order(player_section, chunk.sections.len() as i32) {
            if in_chunk >= max {
                break;
            }
            let section = &chunk.sections[s as usize];
            if !section.blocks.maybe_has(|v| ids.contains(&v)) {
                continue;
            }
            let base = horizon_client::chunk::MIN_Y + s * 16;
            'section: for y in base..base + 16 {
                for lz in 0..16 {
                    for lx in 0..16 {
                        if ids.contains(&chunk.get(lx, y, lz)) {
                            if in_chunk >= max {
                                break 'section;
                            }
                            found.push([cx * 16 + lx, y, cz * 16 + lz]);
                            in_chunk += 1;
                        }
                    }
                }
            }
        }
        if found.len() >= max {
            break;
        }
    }
    found.truncate(max);
    found
}

/// `BlockOptionalMeta.drops`: the items a block gives when a netherite pickaxe breaks it, from its loot table.
pub fn block_drops(game: &Game<'_>, block: &str) -> HashSet<String> {
    let Some(state) = game.res.facts().states.iter().position(|s| &*s.name == block) else { return HashSet::new() };
    let id = horizon_world::registry::BlockStateId(state as _);
    let pickaxe = ItemStack { item: Arc::from("minecraft:netherite_pickaxe"), count: 1, damage: 0, extra: None };
    let mut random = LegacyRandom::new(1);
    game.res
        .items()
        .block_drops_by_player(game.pack.assets.registry, id, Some(&pickaxe), &[], &mut random)
        .into_iter()
        .map(|stack| stack.item.to_string())
        .collect()
}

/// A lookup of the named blocks (with their drops filled in), `selector` being `id` or `id[properties]`.
pub fn make_bom(game: &Game<'_>, selector: &str, resolve: &mut dyn FnMut(&str) -> Option<String>) -> Result<BlockOptionalMeta, String> {
    let mut bom = BlockOptionalMeta::parse(selector, resolve)?;
    // A property that the block does not have, or a value it cannot take, is refused as the Java refuses it.
    if !bom.properties().is_empty() {
        let probe = BlockOptionalMetaLookup::new(vec![bom.clone()]);
        if lookup_state_ids(game, &probe).is_empty() {
            let (k, v) = &bom.properties()[0];
            return Err(format!("\"{v}\" is not a valid value for {k} on Block{{{}}}", bom.block()));
        }
    }
    bom.set_drops(block_drops(game, bom.block()));
    Ok(bom)
}
