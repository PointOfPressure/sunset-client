// The buildStructure methods of the finders, written out from the Java by tools/gen_dsl.py, statement for statement.
// A PieceFinder compares the world with the block of each state it was given, so a state is the name of its block.
use super::sunset_scx_finder_piece::PieceFinder;

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn desert_pyramid(f: &mut PieceFinder) {
    let blockState_1 = "sandstone_stairs";
    let blockState_2 = "sandstone_stairs";
    let blockState_3 = "sandstone_stairs";
    let blockState_4 = "sandstone_stairs";
    f.fill_with_outline(0, 0, 0, 4, 9, 4, "sandstone", "air", false);
    f.fill_with_outline(1, 10, 1, 3, 10, 3, "sandstone", "sandstone", false);
    f.add_block(blockState_1, 2, 10, 0);
    f.add_block(blockState_2, 2, 10, 4);
    f.add_block(blockState_3, 0, 10, 2);
    f.add_block(blockState_4, 4, 10, 2);
    f.fill_with_outline(f.width - 5, 0, 0, f.width - 1, 9, 4, "sandstone", "air", false);
    f.fill_with_outline(f.width - 4, 10, 1, f.width - 2, 10, 3, "sandstone", "sandstone", false);
    f.add_block(blockState_1, f.width - 3, 10, 0);
    f.add_block(blockState_2, f.width - 3, 10, 4);
    f.add_block(blockState_3, f.width - 5, 10, 2);
    f.add_block(blockState_4, f.width - 1, 10, 2);
    f.fill_with_outline(8, 0, 0, 12, 4, 4, "sandstone", "air", false);
    f.fill_with_outline(9, 1, 0, 11, 3, 4, "air", "air", false);
    f.add_block("cut_sandstone", 9, 1, 1);
    f.add_block("cut_sandstone", 9, 2, 1);
    f.add_block("cut_sandstone", 9, 3, 1);
    f.add_block("cut_sandstone", 10, 3, 1);
    f.add_block("cut_sandstone", 11, 3, 1);
    f.add_block("cut_sandstone", 11, 2, 1);
    f.add_block("cut_sandstone", 11, 1, 1);
    f.fill_with_outline(4, 1, 1, 8, 3, 3, "sandstone", "air", false);
    f.fill_with_outline(4, 1, 2, 8, 2, 2, "air", "air", false);
    f.fill_with_outline(12, 1, 1, 16, 3, 3, "sandstone", "air", false);
    f.fill_with_outline(12, 1, 2, 16, 2, 2, "air", "air", false);
    f.fill_with_outline(5, 4, 5, f.width - 6, 4, f.depth - 6, "sandstone", "sandstone", false);
    f.fill_with_outline(9, 4, 9, 11, 4, 11, "air", "air", false);
    f.fill_with_outline(8, 1, 8, 8, 3, 8, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(12, 1, 8, 12, 3, 8, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(8, 1, 12, 8, 3, 12, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(12, 1, 12, 12, 3, 12, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(1, 1, 5, 4, 4, 11, "sandstone", "sandstone", false);
    f.fill_with_outline(f.width - 5, 1, 5, f.width - 2, 4, 11, "sandstone", "sandstone", false);
    f.fill_with_outline(6, 7, 9, 6, 7, 11, "sandstone", "sandstone", false);
    f.fill_with_outline(f.width - 7, 7, 9, f.width - 7, 7, 11, "sandstone", "sandstone", false);
    f.fill_with_outline(5, 5, 9, 5, 7, 11, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(f.width - 6, 5, 9, f.width - 6, 7, 11, "cut_sandstone", "cut_sandstone", false);
    f.add_block("air", 5, 5, 10);
    f.add_block("air", 5, 6, 10);
    f.add_block("air", 6, 6, 10);
    f.add_block("air", f.width - 6, 5, 10);
    f.add_block("air", f.width - 6, 6, 10);
    f.add_block("air", f.width - 7, 6, 10);
    f.fill_with_outline(2, 4, 4, 2, 6, 4, "air", "air", false);
    f.fill_with_outline(f.width - 3, 4, 4, f.width - 3, 6, 4, "air", "air", false);
    f.add_block(blockState_1, 2, 4, 5);
    f.add_block(blockState_1, 2, 3, 4);
    f.add_block(blockState_1, f.width - 3, 4, 5);
    f.add_block(blockState_1, f.width - 3, 3, 4);
    f.fill_with_outline(1, 1, 3, 2, 2, 3, "sandstone", "sandstone", false);
    f.fill_with_outline(f.width - 3, 1, 3, f.width - 2, 2, 3, "sandstone", "sandstone", false);
    f.add_block("sandstone", 1, 1, 2);
    f.add_block("sandstone", f.width - 2, 1, 2);
    f.add_block("sandstone_slab", 1, 2, 2);
    f.add_block("sandstone_slab", f.width - 2, 2, 2);
    f.add_block(blockState_4, 2, 1, 2);
    f.add_block(blockState_3, f.width - 3, 1, 2);
    f.fill_with_outline(4, 3, 5, 4, 3, 17, "sandstone", "sandstone", false);
    f.fill_with_outline(f.width - 5, 3, 5, f.width - 5, 3, 17, "sandstone", "sandstone", false);
    f.fill_with_outline(3, 1, 5, 4, 2, 16, "air", "air", false);
    f.fill_with_outline(f.width - 6, 1, 5, f.width - 5, 2, 16, "air", "air", false);
    let mut int_7: i32 = 0;
    let mut int_7: i32 = 5;
    while int_7 <= 17 {
        f.add_block("cut_sandstone", 4, 1, int_7);
        f.add_block("chiseled_sandstone", 4, 2, int_7);
        f.add_block("cut_sandstone", f.width - 5, 1, int_7);
        f.add_block("chiseled_sandstone", f.width - 5, 2, int_7);
        int_7 += 2;
    }
    f.add_block("orange_terracotta", 10, 0, 7);
    f.add_block("orange_terracotta", 10, 0, 8);
    f.add_block("orange_terracotta", 9, 0, 9);
    f.add_block("orange_terracotta", 11, 0, 9);
    f.add_block("orange_terracotta", 8, 0, 10);
    f.add_block("orange_terracotta", 12, 0, 10);
    f.add_block("orange_terracotta", 7, 0, 10);
    f.add_block("orange_terracotta", 13, 0, 10);
    f.add_block("orange_terracotta", 9, 0, 11);
    f.add_block("orange_terracotta", 11, 0, 11);
    f.add_block("orange_terracotta", 10, 0, 12);
    f.add_block("orange_terracotta", 10, 0, 13);
    f.add_block("blue_terracotta", 10, 0, 10);
    let mut int_7: i32 = 0;
    while int_7 <= f.width - 1 {
        f.add_block("cut_sandstone", int_7, 2, 1);
        f.add_block("orange_terracotta", int_7, 2, 2);
        f.add_block("cut_sandstone", int_7, 2, 3);
        f.add_block("cut_sandstone", int_7, 3, 1);
        f.add_block("orange_terracotta", int_7, 3, 2);
        f.add_block("cut_sandstone", int_7, 3, 3);
        f.add_block("orange_terracotta", int_7, 4, 1);
        f.add_block("chiseled_sandstone", int_7, 4, 2);
        f.add_block("orange_terracotta", int_7, 4, 3);
        f.add_block("cut_sandstone", int_7, 5, 1);
        f.add_block("orange_terracotta", int_7, 5, 2);
        f.add_block("cut_sandstone", int_7, 5, 3);
        f.add_block("orange_terracotta", int_7, 6, 1);
        f.add_block("chiseled_sandstone", int_7, 6, 2);
        f.add_block("orange_terracotta", int_7, 6, 3);
        f.add_block("orange_terracotta", int_7, 7, 1);
        f.add_block("orange_terracotta", int_7, 7, 2);
        f.add_block("orange_terracotta", int_7, 7, 3);
        f.add_block("cut_sandstone", int_7, 8, 1);
        f.add_block("cut_sandstone", int_7, 8, 2);
        f.add_block("cut_sandstone", int_7, 8, 3);
        int_7 += f.width - 1;
    }
    let mut int_7: i32 = 2;
    while int_7 <= f.width - 3 {
        f.add_block("cut_sandstone", int_7 - 1, 2, 0);
        f.add_block("orange_terracotta", int_7, 2, 0);
        f.add_block("cut_sandstone", int_7 + 1, 2, 0);
        f.add_block("cut_sandstone", int_7 - 1, 3, 0);
        f.add_block("orange_terracotta", int_7, 3, 0);
        f.add_block("cut_sandstone", int_7 + 1, 3, 0);
        f.add_block("orange_terracotta", int_7 - 1, 4, 0);
        f.add_block("chiseled_sandstone", int_7, 4, 0);
        f.add_block("orange_terracotta", int_7 + 1, 4, 0);
        f.add_block("cut_sandstone", int_7 - 1, 5, 0);
        f.add_block("orange_terracotta", int_7, 5, 0);
        f.add_block("cut_sandstone", int_7 + 1, 5, 0);
        f.add_block("orange_terracotta", int_7 - 1, 6, 0);
        f.add_block("chiseled_sandstone", int_7, 6, 0);
        f.add_block("orange_terracotta", int_7 + 1, 6, 0);
        f.add_block("orange_terracotta", int_7 - 1, 7, 0);
        f.add_block("orange_terracotta", int_7, 7, 0);
        f.add_block("orange_terracotta", int_7 + 1, 7, 0);
        f.add_block("cut_sandstone", int_7 - 1, 8, 0);
        f.add_block("cut_sandstone", int_7, 8, 0);
        f.add_block("cut_sandstone", int_7 + 1, 8, 0);
        int_7 += f.width - 3 - 2;
    }
    f.fill_with_outline(8, 4, 0, 12, 6, 0, "cut_sandstone", "cut_sandstone", false);
    f.add_block("air", 8, 6, 0);
    f.add_block("air", 12, 6, 0);
    f.add_block("orange_terracotta", 9, 5, 0);
    f.add_block("chiseled_sandstone", 10, 5, 0);
    f.add_block("orange_terracotta", 11, 5, 0);
    f.fill_with_outline(8, -14, 8, 12, -11, 12, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(8, -10, 8, 12, -10, 12, "chiseled_sandstone", "chiseled_sandstone", false);
    f.fill_with_outline(8, -9, 8, 12, -9, 12, "cut_sandstone", "cut_sandstone", false);
    f.fill_with_outline(8, -8, 8, 12, -1, 12, "sandstone", "sandstone", false);
    f.fill_with_outline(9, -11, 9, 11, -1, 11, "air", "air", false);
    f.add_block("stone_pressure_plate", 10, -11, 10);
    f.fill_with_outline(9, -13, 9, 11, -13, 11, "tnt", "air", false);
    f.add_block("air", 8, -11, 10);
    f.add_block("air", 8, -10, 10);
    f.add_block("chiseled_sandstone", 7, -10, 10);
    f.add_block("cut_sandstone", 7, -11, 10);
    f.add_block("air", 12, -11, 10);
    f.add_block("air", 12, -10, 10);
    f.add_block("chiseled_sandstone", 13, -10, 10);
    f.add_block("cut_sandstone", 13, -11, 10);
    f.add_block("air", 10, -11, 8);
    f.add_block("air", 10, -10, 8);
    f.add_block("chiseled_sandstone", 10, -10, 7);
    f.add_block("cut_sandstone", 10, -11, 7);
    f.add_block("air", 10, -11, 12);
    f.add_block("air", 10, -10, 12);
    f.add_block("chiseled_sandstone", 10, -10, 13);
    f.add_block("cut_sandstone", 10, -11, 13);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn jungle_pyramid(f: &mut PieceFinder) {
    let eastStairs = "cobblestone_stairs";
    let westStairs = "cobblestone_stairs";
    let southStairs = "cobblestone_stairs";
    let northStairs = "cobblestone_stairs";
    f.add_block(northStairs, 5, 9, 6);
    f.add_block(northStairs, 6, 9, 6);
    f.add_block(southStairs, 5, 9, 8);
    f.add_block(southStairs, 6, 9, 8);
    f.add_block(northStairs, 4, 0, 0);
    f.add_block(northStairs, 5, 0, 0);
    f.add_block(northStairs, 6, 0, 0);
    f.add_block(northStairs, 7, 0, 0);
    f.add_block(northStairs, 4, 1, 8);
    f.add_block(northStairs, 4, 2, 9);
    f.add_block(northStairs, 4, 3, 10);
    f.add_block(northStairs, 7, 1, 8);
    f.add_block(northStairs, 7, 2, 9);
    f.add_block(northStairs, 7, 3, 10);
    f.add_block(eastStairs, 4, 4, 5);
    f.add_block(westStairs, 7, 4, 5);
    f.add_block("tripwire_hook", 1, -3, 8);
    f.add_block("tripwire_hook", 4, -3, 8);
    f.add_block("tripwire", 2, -3, 8);
    f.add_block("tripwire", 3, -3, 8);
    let blockState_5 = "redstone_wire";
    f.add_block("redstone_wire", 5, -3, 7);
    f.add_block(blockState_5, 5, -3, 6);
    f.add_block(blockState_5, 5, -3, 5);
    f.add_block(blockState_5, 5, -3, 4);
    f.add_block(blockState_5, 5, -3, 3);
    f.add_block(blockState_5, 5, -3, 2);
    f.add_block("redstone_wire", 5, -3, 1);
    f.add_block("redstone_wire", 4, -3, 1);
    f.add_block("mossy_cobblestone", 3, -3, 1);
    f.add_block("vine", 3, -2, 2);
    f.add_block("tripwire_hook", 7, -3, 1);
    f.add_block("tripwire_hook", 7, -3, 5);
    f.add_block("tripwire", 7, -3, 2);
    f.add_block("tripwire", 7, -3, 3);
    f.add_block("tripwire", 7, -3, 4);
    f.add_block("redstone_wire", 8, -3, 6);
    f.add_block("redstone_wire", 9, -3, 6);
    f.add_block("redstone_wire", 9, -3, 5);
    f.add_block("mossy_cobblestone", 9, -3, 4);
    f.add_block("redstone_wire", 9, -2, 4);
    f.add_block("vine", 8, -1, 3);
    f.add_block("vine", 8, -2, 3);
    f.add_block("mossy_cobblestone", 9, -3, 2);
    f.add_block("mossy_cobblestone", 8, -3, 1);
    f.add_block("mossy_cobblestone", 4, -3, 5);
    f.add_block("mossy_cobblestone", 5, -2, 5);
    f.add_block("mossy_cobblestone", 5, -1, 5);
    f.add_block("mossy_cobblestone", 6, -3, 5);
    f.add_block("mossy_cobblestone", 7, -2, 5);
    f.add_block("mossy_cobblestone", 7, -1, 5);
    f.add_block("mossy_cobblestone", 8, -3, 5);
    f.add_block("chiseled_stone_bricks", 8, -2, 11);
    f.add_block("chiseled_stone_bricks", 9, -2, 11);
    f.add_block("chiseled_stone_bricks", 10, -2, 11);
    let blockState_6 = "lever";
    f.add_block(blockState_6, 8, -2, 12);
    f.add_block(blockState_6, 9, -2, 12);
    f.add_block(blockState_6, 10, -2, 12);
    f.add_block("mossy_cobblestone", 10, -2, 9);
    f.add_block("redstone_wire", 8, -2, 9);
    f.add_block("redstone_wire", 8, -2, 10);
    f.add_block("redstone_wire", 10, -1, 9);
    f.add_block("sticky_piston", 9, -2, 8);
    f.add_block("sticky_piston", 10, -2, 8);
    f.add_block("sticky_piston", 10, -1, 8);
    f.add_block("repeater", 10, -2, 10);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn swamp_hut(f: &mut PieceFinder) {
    f.fill_with_outline(1, 1, 1, 5, 1, 7, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(1, 4, 2, 5, 4, 7, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(2, 1, 0, 4, 1, 0, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(2, 2, 2, 3, 3, 2, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(1, 2, 3, 1, 3, 6, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(5, 2, 3, 5, 3, 6, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(2, 2, 7, 4, 3, 7, "spruce_planks", "spruce_planks", false);
    f.fill_with_outline(1, 0, 2, 1, 3, 2, "oak_log", "oak_log", false);
    f.fill_with_outline(5, 0, 2, 5, 3, 2, "oak_log", "oak_log", false);
    f.fill_with_outline(1, 0, 7, 1, 3, 7, "oak_log", "oak_log", false);
    f.fill_with_outline(5, 0, 7, 5, 3, 7, "oak_log", "oak_log", false);
    f.add_block("oak_fence", 2, 3, 2);
    f.add_block("oak_fence", 3, 3, 7);
    f.add_block("air", 1, 3, 4);
    f.add_block("air", 5, 3, 4);
    f.add_block("air", 5, 3, 5);
    f.add_block("potted_red_mushroom", 1, 3, 5);
    f.add_block("crafting_table", 3, 2, 6);
    f.add_block("cauldron", 4, 2, 6);
    f.add_block("oak_fence", 1, 2, 1);
    f.add_block("oak_fence", 5, 2, 1);
    let northStairs = "spruce_stairs";
    let eastStairs = "spruce_stairs";
    let westStairs = "spruce_stairs";
    let southStairs = "spruce_stairs";
    f.fill_with_outline(0, 4, 1, 6, 4, 1, northStairs, northStairs, false);
    f.fill_with_outline(0, 4, 2, 0, 4, 7, eastStairs, eastStairs, false);
    f.fill_with_outline(6, 4, 2, 6, 4, 7, westStairs, westStairs, false);
    f.fill_with_outline(0, 4, 8, 6, 4, 8, southStairs, southStairs, false);
    f.add_block(northStairs, 0, 4, 1);
    f.add_block(northStairs, 6, 4, 1);
    f.add_block(southStairs, 0, 4, 8);
    f.add_block(southStairs, 6, 4, 8);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn igloo(f: &mut PieceFinder) {
    let snow = "snow_block";
    let ice = "ice";
    let workBench = "crafting_table";
    f.add_block(workBench, 1, 1, 5);
    let mut y: i32 = 0;
    while y < 3 {
        f.add_block(snow, 2, y, 0);
        f.add_block(snow, 2, y, 1);
        f.add_block(snow, 1, y, 2);
        f.add_block(snow, 0, y, 3);
        f.add_block(snow, 0, y, 4);
        f.add_block(ice, 0, 1, 4);
        f.add_block(snow, 0, y, 5);
        f.add_block(snow, 1, y, 6);
        f.add_block(snow, 2, y, 7);
        f.add_block(snow, 3, y, 7);
        f.add_block(snow, 4, y, 0);
        f.add_block(snow, 4, y, 1);
        f.add_block(snow, 5, y, 2);
        f.add_block(snow, 6, y, 3);
        f.add_block(snow, 6, y, 4);
        f.add_block(ice, 6, 1, 4);
        f.add_block(snow, 6, y, 5);
        f.add_block(snow, 5, y, 6);
        f.add_block(snow, 4, y, 7);
        y += 1;
    }
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn monument(f: &mut PieceFinder) {
    let prismarine = "prismarine";
    let prismarineBricks = "prismarine_bricks";
    let darkPrismarine = "dark_prismarine";
    let seaLantern = "sea_lantern";
    let water = "water";
    let mut i: i32 = 0;
    while i < 4 {
        let x: i32 = if i >= 2 { 7 } else { 0 };
        let z: i32 = if i % 2 == 0 { 0 } else { 7 };
        let mut y: i32 = 0;
        while y < 3 {
            f.add_block(prismarineBricks, x, y, z);
            y += 1;
        }
        i += 1;
    }
    let mut i: i32 = 0;
    while i < 4 {
        let x: i32 = if i >= 2 { 6 } else { 1 };
        let z: i32 = if i % 2 == 0 { 1 } else { 6 };
        f.add_block(prismarineBricks, x, 3, z);
        i += 1;
    }
    let mut x: i32 = 2;
    while x <= 5 {
        let mut z: i32 = 2;
        while z <= 5 {
            if x == 2 || x == 5 || z == 2 || z == 5 {
                f.add_block(prismarine, x, 4, z);
            }
            z += 1;
        }
        x += 1;
    }
    let mut i: i32 = 0;
    while i < 4 {
        let x: i32 = if i >= 2 { 5 } else { 2 };
        let z: i32 = if i % 2 == 0 { 2 } else { 5 };
        f.add_block(prismarineBricks, x, 4, z);
        f.add_block(seaLantern, x, 3, z);
        i += 1;
    }
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn end_city(f: &mut PieceFinder) {
    let air = "air";
    let endstoneBricks = "end_stone_bricks";
    let purpur = "purpur_block";
    let purpurPillar = "purpur_pillar";
    let purpleGlass = "magenta_stained_glass";
    f.fill_with_outline_opt(0, 0, 0, 7, 4, 7, Some(endstoneBricks), None, false);
    f.fill_with_outline(0, 0, 0, 0, 3, 0, purpurPillar, purpurPillar, false);
    f.fill_with_outline(7, 0, 0, 7, 3, 0, purpurPillar, purpurPillar, false);
    f.fill_with_outline(0, 0, 7, 0, 3, 7, purpurPillar, purpurPillar, false);
    f.fill_with_outline(7, 0, 7, 7, 3, 7, purpurPillar, purpurPillar, false);
    f.fill_with_outline(0, 0, 0, 7, 0, 7, purpur, purpur, false);
    f.fill_with_outline(3, 1, 0, 4, 3, 0, air, air, false);
    f.fill_with_outline(0, 2, 2, 0, 3, 2, purpleGlass, purpleGlass, false);
    f.fill_with_outline(0, 2, 5, 0, 3, 5, purpleGlass, purpleGlass, false);
    f.fill_with_outline(7, 2, 2, 7, 3, 2, purpleGlass, purpleGlass, false);
    f.fill_with_outline(7, 2, 5, 7, 3, 5, purpleGlass, purpleGlass, false);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn outpost(f: &mut PieceFinder) {
    let chest = "chest";
    let birchwood = "birch_planks";
    f.add_block(chest, 9, 14, 10);
    f.fill_with_outline(4, 0, 4, 10, 0, 10, birchwood, birchwood, false);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn trial_chambers_ominous_vault(f: &mut PieceFinder, x: i32, y: i32, z: i32) {
    let air = "air";
    let polishedTuff = "polished_tuff";
    let polishedTuffSlabTop = "polished_tuff_slab";
    let ominousVault = "vault";
    let tuffBricks = "tuff_bricks";
    let chiseledTuff = "chiseled_tuff";
    let redGlazedTerracotta = "red_glazed_terracotta";
    let waxedCopperBulbOn = "waxed_copper_bulb";
    let redCandle3 = "red_candle";
    let redCandle4 = "red_candle";
    let waxedOxidizedCopperGrate = "waxed_oxidized_copper_grate";
    f.fill_with_outline(x, y, z, x + 2, y, z + 1, waxedOxidizedCopperGrate, waxedOxidizedCopperGrate, false);
    f.add_block(redCandle3, x, y + 1, z);
    f.add_block(redCandle4, x + 2, y + 1, z);
    f.fill_with_outline(x, y + 1, z + 1, x + 2, y + 3, z + 1, polishedTuff, polishedTuff, false);
    f.fill_with_outline(x, y + 2, z + 1, x + 2, y + 2, z + 1, polishedTuffSlabTop, polishedTuffSlabTop, false);
    f.add_block(ominousVault, x + 1, y + 1, z + 1);
    f.add_block(air, x + 1, y + 2, z + 1);
    f.add_block(waxedCopperBulbOn, x + 1, y + 3, z + 1);
    f.fill_with_outline(x, y, z + 2, x + 2, y, z + 2, polishedTuff, polishedTuff, false);
    f.fill_with_outline(x, y + 1, z + 2, x + 2, y + 3, z + 2, tuffBricks, tuffBricks, false);
    f.add_block(waxedCopperBulbOn, x + 1, y + 1, z + 2);
    f.fill_with_outline(x, y + 2, z + 2, x + 2, y + 2, z + 2, redGlazedTerracotta, redGlazedTerracotta, false);
    f.add_block(chiseledTuff, x + 1, y + 2, z + 2);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn trial_chambers_end1(f: &mut PieceFinder) {
    let air = "air";
    let tuffBricks = "tuff_bricks";
    let waxedOxidizedCutCopper = "waxed_oxidized_cut_copper";
    let waxedCopperBulbOn = "waxed_copper_bulb";
    let waxedCopperBlock = "waxed_copper_block";
    let chiseledTuff = "chiseled_tuff";
    let chiseledTuffBricks = "chiseled_tuff_bricks";
    let polishedTuff = "polished_tuff";
    f.fill_with_outline(0, 0, 0, 0, 18, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(18, 0, 0, 18, 18, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(0, 19, 0, 18, 19, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(0, 0, 0, 18, 1, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(6, 6, 11, 12, 18, 12, tuffBricks, tuffBricks, false);
    f.fill_with_outline(6, 3, 11, 12, 3, 12, waxedOxidizedCutCopper, waxedOxidizedCutCopper, false);
    f.fill_with_outline(6, 10, 11, 12, 10, 12, waxedCopperBlock, waxedCopperBlock, false);
    f.fill_with_outline(6, 15, 11, 12, 15, 12, waxedCopperBlock, waxedCopperBlock, false);
    f.fill_with_outline(6, 5, 12, 12, 5, 12, chiseledTuff, chiseledTuff, false);
    f.fill_with_outline(6, 5, 11, 7, 5, 12, waxedCopperBlock, waxedCopperBlock, false);
    f.fill_with_outline(11, 5, 11, 12, 5, 11, waxedCopperBlock, waxedCopperBlock, false);
    f.fill_with_outline(7, 5, 12, 12, 5, 12, chiseledTuffBricks, chiseledTuffBricks, false);
    f.add_block(chiseledTuff, 8, 5, 12);
    f.add_block(chiseledTuff, 10, 5, 12);
    f.fill_with_outline(6, 7, 11, 12, 7, 12, chiseledTuffBricks, chiseledTuffBricks, false);
    f.fill_with_outline(6, 12, 11, 12, 12, 12, chiseledTuffBricks, chiseledTuffBricks, false);
    f.fill_with_outline(6, 17, 11, 12, 17, 12, chiseledTuffBricks, chiseledTuffBricks, false);
    f.fill_with_outline(8, 3, 11, 10, 17, 11, air, air, false);
    f.fill_with_outline(8, 18, 11, 10, 18, 12, polishedTuff, polishedTuff, false);
    trial_chambers_ominous_vault(f, 8, 13, 10);
}

#[allow(clippy::all, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_parens)]
pub fn trial_chambers_end2(f: &mut PieceFinder) {
    let water = "water";
    let tuffBricks = "tuff_bricks";
    let waxedOxidizedCopper = "waxed_oxidized_copper";
    let waxedCopperGrate = "waxed_copper_grate";
    let diamondChest = "chest";
    f.fill_with_outline(0, 0, 0, 0, 18, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(18, 0, 0, 18, 18, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(0, 19, 0, 18, 19, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(0, 0, 0, 18, 0, 18, tuffBricks, tuffBricks, false);
    f.fill_with_outline(8, 0, 13, 10, 0, 17, waxedOxidizedCopper, waxedOxidizedCopper, false);
    f.fill_with_outline(4, 2, 8, 14, 2, 9, water, water, false);
    f.fill_with_outline(3, 2, 11, 4, 2, 15, water, water, false);
    f.fill_with_outline(14, 2, 11, 15, 2, 15, water, water, false);
    f.fill_with_outline(8, 3, 11, 10, 5, 11, waxedCopperGrate, waxedCopperGrate, false);
    f.fill_with_outline(7, 3, 13, 7, 5, 16, waxedCopperGrate, waxedCopperGrate, false);
    f.fill_with_outline(11, 3, 13, 11, 5, 16, waxedCopperGrate, waxedCopperGrate, false);
    f.add_block(diamondChest, 9, 3, 6);
    trial_chambers_ominous_vault(f, 8, 1, 15);
}
