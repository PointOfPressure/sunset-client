// baritone.utils.ToolSet, written from the Java: which of the nine hotbar slots breaks a block best, how fast that is, and what
// the settings change about it (autoTool, useSwordToMine, itemSaver, considerPotionEffects, blocksToAvoidBreaking and its
// multiplier, preferSilkTouch). The slot is chosen on the speed against the block's default state, ties go to the cheaper tool
// material, and a hand (an empty slot) counts like any other stack.
use super::baritone_settings::Settings;
use super::Game;
use horizon_client::items::ItemStack;
use horizon_world::registry::BlockStateId;

/// What the tool choice reads of the settings.
#[derive(Clone)]
pub struct ToolSettings {
    pub auto_tool: bool,
    pub use_sword_to_mine: bool,
    /// `itemSaver` with its threshold, or None when it is off.
    pub item_saver: Option<i32>,
    pub consider_potion_effects: bool,
    pub avoid_breaking: Vec<String>,
    pub avoid_breaking_multiplier: f64,
    pub prefer_silk_touch: bool,
}

impl ToolSettings {
    pub fn of(s: &Settings) -> Self {
        Self {
            auto_tool: s.flag("autoTool"),
            use_sword_to_mine: s.flag("useSwordToMine"),
            item_saver: s.flag("itemSaver").then(|| s.int("itemSaverThreshold")),
            consider_potion_effects: s.flag("considerPotionEffects"),
            avoid_breaking: s.list("blocksToAvoidBreaking"),
            avoid_breaking_multiplier: s.float("avoidBreakingMultiplier"),
            prefer_silk_touch: s.flag("preferSilkTouch"),
        }
    }
}

/// `materialTagsPriorityList`: the cheaper the material, the lower the index. (In 1.21.4 these are the tags of what REPAIRS
/// a tool, planks and ingots, so a tool itself is in none of them and every tool costs -1; kept as the Java has it.)
const MATERIAL_TAGS: [&str; 6] = [
    "minecraft:wooden_tool_materials",
    "minecraft:stone_tool_materials",
    "minecraft:iron_tool_materials",
    "minecraft:gold_tool_materials",
    "minecraft:diamond_tool_materials",
    "minecraft:netherite_tool_materials",
];

fn in_tag(game: &Game<'_>, tag: &str, stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|s| game.res.items().item_tags.get(tag).is_some_and(|t| t.contains(&*s.item)))
}

/// `getMaterialCost`.
fn material_cost(game: &Game<'_>, stack: Option<&ItemStack>) -> i32 {
    MATERIAL_TAGS.iter().position(|tag| in_tag(game, tag, stack)).map_or(-1, |i| i as i32)
}

/// `hasSilkTouch`.
fn has_silk_touch(stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|s| s.enchantment_level("minecraft:silk_touch") > 0)
}

fn hotbar(game: &Game<'_>, slot: usize) -> Option<ItemStack> {
    game.inventory.get(slot).and_then(Option::as_ref).filter(|s| s.count > 0).cloned()
}

/// `calculateSpeedVsBlock`: the progress one tick of this item makes against the block (1 / ticks); negative for what cannot
/// be broken.
pub fn calculate_speed_vs_block(game: &Game<'_>, stack: Option<&ItemStack>, state: BlockStateId) -> f64 {
    let facts = game.res.facts().get(state);
    let hardness = f64::from(facts.hardness);
    if hardness < 0.0 {
        return -1.0;
    }
    let items = game.res.items();
    let mut speed = f64::from(items.mining_speed(stack, &facts.name));
    if speed > 1.0 {
        // The first enchantment with a mining efficiency attribute adds `levels_squared + 1` of itself (Efficiency).
        if let Some(s) = stack {
            let level = f64::from(s.enchantment_level("minecraft:efficiency"));
            if level > 0.0 {
                speed += level * level + 1.0;
            }
        }
    }
    speed /= hardness;
    if !facts.requires_tool || (stack.is_some() && items.correct_for_drops(stack, &facts.name)) { speed / 30.0 } else { speed / 100.0 }
}

/// `getBestSlot`: the hotbar slot to break the block with. `path_calc` is `pathingCalculation`: with autoTool off the tool
/// that is held is the one the path will use.
pub fn best_slot(game: &Game<'_>, state: BlockStateId, prefer_silk_touch: bool, path_calc: bool, ts: &ToolSettings) -> usize {
    if !ts.auto_tool && path_calc {
        return game.selected;
    }
    let mut best = 0;
    let mut highest_speed = f64::NEG_INFINITY;
    let mut lowest_cost = i32::MIN;
    let mut best_silk_touch = false;
    for i in 0..9 {
        let stack = hotbar(game, i);
        let stack = stack.as_ref();
        if !ts.use_sword_to_mine && in_tag(game, "minecraft:swords", stack) {
            continue;
        }
        if let (Some(threshold), Some(s)) = (ts.item_saver, stack) {
            let max = game.res.items().get(&s.item).map_or(0, |info| info.max_damage as i32);
            if s.damage as i32 + threshold >= max && max > 1 {
                continue;
            }
        }
        let speed = calculate_speed_vs_block(game, stack, state);
        let silk_touch = has_silk_touch(stack);
        if speed > highest_speed {
            highest_speed = speed;
            best = i;
            lowest_cost = material_cost(game, stack);
            best_silk_touch = silk_touch;
        } else if speed == highest_speed {
            let cost = material_cost(game, stack);
            if (cost < lowest_cost && (silk_touch || !best_silk_touch)) || (prefer_silk_touch && !best_silk_touch && silk_touch) {
                highest_speed = speed;
                best = i;
                lowest_cost = cost;
                best_silk_touch = silk_touch;
            }
        }
    }
    best
}

/// `potionAmplifier`: what haste and mining fatigue do to the time a block takes.
fn potion_amplifier(game: &Game<'_>) -> f64 {
    let effects = &game.potions.effects;
    let mut speed = 1.0;
    if let Some(amplifier) = effects.amplifier("haste") {
        speed *= 1.0 + f64::from(amplifier + 1) * 0.2;
    }
    if let Some(amplifier) = effects.amplifier("mining_fatigue") {
        speed *= match amplifier {
            0 => 0.3,
            1 => 0.09,
            2 => 0.0027, // not 0.09 * 0.3 = 0.027, as the Java's own comment says
            _ => 0.00081,
        };
    }
    speed
}

/// `avoidanceMultiplier`.
fn avoidance_multiplier(game: &Game<'_>, state: BlockStateId, ts: &ToolSettings) -> f64 {
    let name = game.res.facts().get(state).name.clone();
    let bare = name.strip_prefix("minecraft:").unwrap_or(&name);
    if ts.avoid_breaking.iter().any(|a| a == bare) { ts.avoid_breaking_multiplier } else { 1.0 }
}

/// `getStrVsBlock`: how fast the best tool at hand breaks the block, as 1 / ticks.
pub fn str_vs_block(game: &Game<'_>, state: BlockStateId, ts: &ToolSettings) -> f64 {
    let slot = best_slot(game, state, false, true, ts);
    let stack = hotbar(game, slot);
    let best = calculate_speed_vs_block(game, stack.as_ref(), state) * avoidance_multiplier(game, state, ts);
    if ts.consider_potion_effects { potion_amplifier(game) * best } else { best }
}

/// `switchToBestToolFor`: the slot to hold to break the block, when autoTool picks it.
pub fn tool_slot_for(game: &Game<'_>, state: BlockStateId, ts: &ToolSettings) -> Option<usize> {
    (ts.auto_tool).then(|| best_slot(game, state, ts.prefer_silk_touch, false, ts))
}
