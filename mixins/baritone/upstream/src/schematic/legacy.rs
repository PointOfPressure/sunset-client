// The old numeric block ids of an MCEdit `.schematic`, as `MCEditSchematic` reads them: a byte a block (`Blocks`), the high bits of
// ids past 255 in `AddBlocks`, and the item name the game keeps for the id (`ItemIdFix`). The data values are not read, so the
// block is the default state of whatever the name is; a name that is no block now (`grass`, `log`: the names of before the
// flattening) is air, as the Java's registry lookup makes it.

/// `ItemIdFix`: the item name the game keeps for each old block id, for ids 1 to 255 that have one (read from the game's own
/// class). Many are the names of before the flattening and name no block now; those come out as air, as the Java's registry
/// lookup does.
const LEGACY_BLOCKS: &[(u8, &str)] = &[
    (1, "stone"),
    (2, "grass"),
    (3, "dirt"),
    (4, "cobblestone"),
    (5, "planks"),
    (6, "sapling"),
    (7, "bedrock"),
    (8, "flowing_water"),
    (9, "water"),
    (10, "flowing_lava"),
    (11, "lava"),
    (12, "sand"),
    (13, "gravel"),
    (14, "gold_ore"),
    (15, "iron_ore"),
    (16, "coal_ore"),
    (17, "log"),
    (18, "leaves"),
    (19, "sponge"),
    (20, "glass"),
    (21, "lapis_ore"),
    (22, "lapis_block"),
    (23, "dispenser"),
    (24, "sandstone"),
    (25, "noteblock"),
    (27, "golden_rail"),
    (28, "detector_rail"),
    (29, "sticky_piston"),
    (30, "web"),
    (31, "tallgrass"),
    (32, "deadbush"),
    (33, "piston"),
    (35, "wool"),
    (37, "yellow_flower"),
    (38, "red_flower"),
    (39, "brown_mushroom"),
    (40, "red_mushroom"),
    (41, "gold_block"),
    (42, "iron_block"),
    (43, "double_stone_slab"),
    (44, "stone_slab"),
    (45, "brick_block"),
    (46, "tnt"),
    (47, "bookshelf"),
    (48, "mossy_cobblestone"),
    (49, "obsidian"),
    (50, "torch"),
    (51, "fire"),
    (52, "mob_spawner"),
    (53, "oak_stairs"),
    (54, "chest"),
    (56, "diamond_ore"),
    (57, "diamond_block"),
    (58, "crafting_table"),
    (60, "farmland"),
    (61, "furnace"),
    (62, "lit_furnace"),
    (65, "ladder"),
    (66, "rail"),
    (67, "stone_stairs"),
    (69, "lever"),
    (70, "stone_pressure_plate"),
    (72, "wooden_pressure_plate"),
    (73, "redstone_ore"),
    (76, "redstone_torch"),
    (77, "stone_button"),
    (78, "snow_layer"),
    (79, "ice"),
    (80, "snow"),
    (81, "cactus"),
    (82, "clay"),
    (84, "jukebox"),
    (85, "fence"),
    (86, "pumpkin"),
    (87, "netherrack"),
    (88, "soul_sand"),
    (89, "glowstone"),
    (90, "portal"),
    (91, "lit_pumpkin"),
    (95, "stained_glass"),
    (96, "trapdoor"),
    (97, "monster_egg"),
    (98, "stonebrick"),
    (99, "brown_mushroom_block"),
    (100, "red_mushroom_block"),
    (101, "iron_bars"),
    (102, "glass_pane"),
    (103, "melon_block"),
    (106, "vine"),
    (107, "fence_gate"),
    (108, "brick_stairs"),
    (109, "stone_brick_stairs"),
    (110, "mycelium"),
    (111, "waterlily"),
    (112, "nether_brick"),
    (113, "nether_brick_fence"),
    (114, "nether_brick_stairs"),
    (116, "enchanting_table"),
    (119, "end_portal"),
    (120, "end_portal_frame"),
    (121, "end_stone"),
    (122, "dragon_egg"),
    (123, "redstone_lamp"),
    (125, "double_wooden_slab"),
    (126, "wooden_slab"),
    (127, "cocoa"),
    (128, "sandstone_stairs"),
    (129, "emerald_ore"),
    (130, "ender_chest"),
    (131, "tripwire_hook"),
    (133, "emerald_block"),
    (134, "spruce_stairs"),
    (135, "birch_stairs"),
    (136, "jungle_stairs"),
    (137, "command_block"),
    (138, "beacon"),
    (139, "cobblestone_wall"),
    (141, "carrots"),
    (142, "potatoes"),
    (143, "wooden_button"),
    (145, "anvil"),
    (146, "trapped_chest"),
    (147, "light_weighted_pressure_plate"),
    (148, "heavy_weighted_pressure_plate"),
    (151, "daylight_detector"),
    (152, "redstone_block"),
    (153, "quartz_ore"),
    (154, "hopper"),
    (155, "quartz_block"),
    (156, "quartz_stairs"),
    (157, "activator_rail"),
    (158, "dropper"),
    (159, "stained_hardened_clay"),
    (160, "stained_glass_pane"),
    (161, "leaves2"),
    (162, "log2"),
    (163, "acacia_stairs"),
    (164, "dark_oak_stairs"),
    (170, "hay_block"),
    (171, "carpet"),
    (172, "hardened_clay"),
    (173, "coal_block"),
    (174, "packed_ice"),
    (175, "double_plant"),
];


/// `ItemIdFix.getItem(id)` as a block name: the name in the table, or air.
pub fn legacy_block_name(id: i32) -> &'static str {
    u8::try_from(id).ok().and_then(|id| LEGACY_BLOCKS.binary_search_by_key(&id, |(i, _)| *i).ok()).map_or("air", |at| LEGACY_BLOCKS[at].1)
}

/// The name of each of `count` blocks. `additional` holds two nibbles a byte, the first block's in the high one (the Java's own
/// reading of it), and each is the part of the id above 255.
pub fn mcedit_block_names(count: usize, blocks: &[u8], additional: Option<&[u8]>) -> Vec<&'static str> {
    (0..count)
        .map(|index| {
            let mut id = i32::from(blocks.get(index).copied().unwrap_or(0));
            if let Some(add) = additional {
                let byte = add.get(index / 2).copied().unwrap_or(0);
                let nibble = if index % 2 == 0 { byte >> 4 } else { byte & 0xF };
                id |= i32::from(nibble) << 8;
            }
            legacy_block_name(id)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_ids_name_the_blocks_the_table_keeps() {
        let names = mcedit_block_names(4, &[1, 3, 8, 200], None);
        assert_eq!(names, ["stone", "dirt", "flowing_water", "air"]);
    }

    #[test]
    fn the_extra_nibbles_make_an_id_past_255_which_names_nothing() {
        // 0x10: the first block gets 1 << 8 added, the second 0.
        assert_eq!(mcedit_block_names(2, &[1, 3], Some(&[0x10])), ["air", "dirt"]);
    }

    #[test]
    fn the_table_is_in_id_order_so_it_can_be_searched() {
        assert!(LEGACY_BLOCKS.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
