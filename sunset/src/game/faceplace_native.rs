// The facts Face Place needs about a target: health, the blocks round its feet, its armour's wear.
use super::{
    sunset_durability_patch::{self as durability_patch, Prototype},
    sunset_faceplace_policy::ArmorCondition,
};
use horizon_client::{blocks::BlockFacts, items::ItemStack, world::World};
use horizon_core::{geometry::Vec3, position::BlockPos};
pub fn horizontal_resistance(world: &World, facts: &BlockFacts, feet: Vec3) -> [Option<f32>; 4] {
    if !feet.is_finite() || feet.components().iter().any(|v| v.abs() > 30_000_000.0) {
        return [None; 4];
    }
    let pos = BlockPos::containing(feet);
    [[1, 0], [-1, 0], [0, 1], [0, -1]].map(|[x, z]| {
        let p = pos.offset(x, 0, z);
        if !world.loaded(p.x, p.z) {
            return None;
        }
        let resistance = facts.get(world.block(p.x, p.y, p.z)).explosion_resistance;
        resistance.is_finite().then_some(resistance)
    })
}
pub fn armor(stack: &ItemStack, prototype: Prototype) -> Option<ArmorCondition> {
    if stack.count == 0 {
        return None;
    }
    let patch = stack.wire_patch();
    // A changed locally decoded stack can lose its exact wire patch. Saved
    // unmodelled components must not silently fall back to ordinary prototypes.
    if patch.is_none() && stack.extra.as_ref().is_some_and(|e| !e.other.is_empty()) {
        return None;
    }
    durability_patch::resolve(prototype, stack.damage, patch.as_deref())
}
