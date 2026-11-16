// The game as the pathfinder sees it: what each block state means for walking, the terrain a search
// reads, the hotbar, the entities, and a cache of where blocks are. This is the one place that
// touches the game's world; everything in pathing/ stays free of game types.
use super::baritone_utils::{feet_of, Pos};
use super::Game;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

/// `getMinY()` and `getMaxY()` of the dimension the player is in.
pub fn dimension_extent_of(game: &Game<'_>) -> (i32, i32) {
    let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension);
    let (min_y, height) = super::baritone_elytra_behavior::dimension_extent(dimension);
    (min_y, min_y + height - 1)
}

pub fn block_name(game: &Game<'_>, p: Pos) -> String {
    game.res.facts().get(game.world.block(p[0], p[1], p[2])).name.to_string()
}

pub fn is_air_at(game: &Game<'_>, p: Pos) -> bool {
    game.res.facts().is_air(game.world.block(p[0], p[1], p[2]))
}

pub fn loaded(game: &Game<'_>, p: Pos) -> bool {
    game.world.chunks.contains_key(&(p[0] >> 4, p[2] >> 4))
}

// ---------------------------------------------------------------------------------------
// The hotbar.

/// The first hotbar slot holding an item `wanted` accepts.
pub fn hotbar_slot(game: &Game<'_>, wanted: impl Fn(&str) -> bool) -> Option<usize> {
    (0..9).find(|&i| game.inventory.get(i).and_then(Option::as_ref).is_some_and(|s| s.count > 0 && wanted(&s.item)))
}

pub fn bare(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

// ---------------------------------------------------------------------------------------
// Entities.

pub fn player_pos(game: &Game<'_>) -> Option<[f64; 3]> {
    game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
}

/// `IPlayerContext.playerFeet`: inside a slab's block means standing on the slab.
pub fn player_feet(game: &Game<'_>) -> Option<Pos> {
    let feet = feet_of(player_pos(game)?);
    let in_slab = game.res.facts().get(game.world.block(feet[0], feet[1], feet[2])).name.ends_with("_slab");
    Some(if in_slab { [feet[0], feet[1] + 1, feet[2]] } else { feet })
}

/// A mob (or another player) the follow process can walk after.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub kind: String,
    pub name: Option<String>,
}

pub fn targets(game: &Game<'_>) -> Vec<Target> {
    game.mobs
        .iter()
        .filter(|m| m.alive())
        .map(|m| Target {
            id: m.id,
            pos: [m.position.x, m.position.y, m.position.z],
            yaw: m.yaw,
            kind: m.kind.id.to_string(),
            name: m.custom_name.clone(),
        })
        .collect()
}

/// Items lying on the ground: where, and what.
pub fn dropped_items(game: &Game<'_>) -> Vec<([f64; 3], String)> {
    game.dropped
        .iter()
        .map(|i| ([i.position.x, i.position.y, i.position.z], i.stack.item.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------------------
// Where blocks are.

/// Looks for blocks without reading the world twice: the chunks seen so far, and the block states a
/// name stands for.
#[derive(Default)]
pub struct WorldCache {
    /// Every chunk column that has been loaded since the game started.
    pub explored: HashSet<(i32, i32)>,
    revision: u64,
    ids: HashMap<String, Rc<HashSet<u32>>>,
}

impl WorldCache {
    /// Remember which chunks are loaded now (only when the set changed).
    pub fn note_chunks(&mut self, game: &Game<'_>) {
        let revision = game.world.chunks.membership_revision();
        if revision != self.revision {
            self.revision = revision;
            for (k, _) in &game.world.chunks {
                self.explored.insert(*k);
            }
        }
    }

    /// Every state id of the blocks called `names`.
    pub fn state_ids(&mut self, game: &Game<'_>, names: &HashSet<String>) -> Rc<HashSet<u32>> {
        let mut key: Vec<&str> = names.iter().map(String::as_str).collect();
        key.sort_unstable();
        let key = key.join(",");
        if let Some(ids) = self.ids.get(&key) {
            return ids.clone();
        }
        let ids: HashSet<u32> = game
            .res
            .facts()
            .states
            .iter()
            .enumerate()
            .filter(|(_, s)| names.contains(&*s.name))
            .map(|(i, _)| i as u32)
            .collect();
        let ids = Rc::new(ids);
        self.ids.insert(key, ids.clone());
        ids
    }

}

