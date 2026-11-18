// baritone.process.FarmProcess, written from the Java: scan the chunks round the player for crops, farmland and
// the like; break the ripe ones in reach, plant seeds on bare farmland, soul sand and jungle logs, use bone meal,
// and otherwise path to the nearest work (GoalBreak for a ripe crop, a block above bare farmland, the dropped
// seeds and crops).
use super::baritone::Core;
use super::baritone_goals::{rc, GoalBlock, GoalBreak, GoalComposite, GoalGetToBlock, GoalRef};
use super::baritone_movement_state::Input;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_player_context::PlayerContext;
use super::baritone_scanner::scan_chunk_radius;
use super::baritone_utils::{dist_sq, Pos};
use super::Game;
use std::collections::HashSet;

const FARMLAND_PLANTABLE: &[&str] = &["beetroot_seeds", "melon_seeds", "wheat_seeds", "pumpkin_seeds", "potato", "carrot"];

const PICKUP_DROPPED: &[&str] = &[
    "beetroot_seeds", "beetroot", "melon_seeds", "melon_slice", "melon", "wheat_seeds", "wheat", "pumpkin_seeds", "pumpkin", "potato", "carrot",
    "nether_wart", "cocoa_beans", "sugar_cane", "bamboo", "cactus",
];

/// The blocks of `Harvest`, in its order.
const HARVEST: &[&str] = &["wheat", "carrots", "potatoes", "beetroots", "pumpkin", "melon", "nether_wart", "cocoa", "sugar_cane", "bamboo", "cactus"];

const HORIZONTALS: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

pub struct FarmProcess {
    active: bool,
    locations: Option<Vec<Pos>>,
    tick_count: i32,
    range: i32,
    center: Pos,
}

fn bare(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl FarmProcess {
    pub fn new() -> Self {
        Self { active: false, locations: None, tick_count: 0, range: 0, center: [0, 0, 0] }
    }

    /// `farm(range, pos)`.
    pub fn farm(&mut self, range: i32, pos: Pos) {
        self.center = pos;
        self.range = range;
        self.active = true;
        self.locations = None;
    }

    fn state_age(state: &super::baritone_block_state::BlockState) -> i32 {
        state.prop("age").and_then(|a| a.parse().ok()).unwrap_or(0)
    }
}

impl Process for FarmProcess {
    fn name(&self) -> &'static str {
        "farm"
    }

    fn display(&self) -> String {
        "Farming".to_owned()
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, is_safe_to_cancel: bool) -> PathingCommand {
        let interval = core.settings.int("mineGoalUpdateInterval") as i32;
        if interval != 0 {
            let due = self.tick_count % interval == 0;
            self.tick_count += 1;
            if due {
                let mut scan: Vec<String> = HARVEST.iter().map(|b| format!("minecraft:{b}")).collect();
                if core.settings.flag("replantCrops") {
                    scan.push("minecraft:farmland".into());
                    scan.push("minecraft:jungle_log".into());
                    if core.settings.flag("replantNetherWart") {
                        scan.push("minecraft:soul_sand".into());
                    }
                }
                let names: HashSet<String> = scan.into_iter().collect();
                let ids = core.cache.state_ids(game, &names);
                let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
                self.locations = Some(scan_chunk_radius(game, &ids, feet, core.settings.int("farmMaxScanSize").max(0) as usize, 10));
            }
        }
        let Some(locations) = self.locations.clone() else { return PathingCommand::RequestPause };

        let mut to_break: Vec<Pos> = Vec::new();
        let mut open_farmland: Vec<Pos> = Vec::new();
        let mut bonemealable: Vec<Pos> = Vec::new();
        let mut open_soulsand: Vec<Pos> = Vec::new();
        let mut open_log: Vec<Pos> = Vec::new();
        let mut has_immature_crop = false;
        let (range, center) = (self.range, self.center);
        core.drive(game, |_, ctx, s| {
            let air = |p: Pos| ctx.bsi().get0(p[0], p[1], p[2]).is_air();
            for pos in &locations {
                // Check if the target block is out of range.
                if range != 0 && dist_sq(*pos, center) > i64::from(range) * i64::from(range) {
                    continue;
                }
                let state = ctx.bsi().get0(pos[0], pos[1], pos[2]);
                let air_above = air([pos[0], pos[1] + 1, pos[2]]);
                let name = bare(&state.name).to_owned();
                if name == "farmland" {
                    if air_above {
                        open_farmland.push(*pos);
                    }
                    continue;
                }
                if name == "soul_sand" {
                    if air_above {
                        open_soulsand.push(*pos);
                    }
                    continue;
                }
                if name == "jungle_log" {
                    if HORIZONTALS.iter().any(|d| air([pos[0] + d[0], pos[1], pos[2] + d[2]])) {
                        open_log.push(*pos);
                    }
                    continue;
                }
                let _ = s;
                if HARVEST.contains(&name.as_str()) && Self::ready_for_harvest_in(&name, &state, ctx, *pos, &core_flags(s)) {
                    to_break.push(*pos);
                    continue;
                }
                if HARVEST.contains(&name.as_str()) {
                    has_immature_crop = true;
                }
                // BonemealableBlock, of the blocks scanned: a crop that is not ripe, a cocoa pod that is not full
                // grown, a bamboo stalk that can still grow.
                let valid_target = match name.as_str() {
                    "wheat" | "carrots" | "potatoes" => Self::state_age(&state) < 7,
                    "beetroots" => Self::state_age(&state) < 3,
                    "cocoa" => Self::state_age(&state) < 2,
                    "bamboo" => Self::bamboo_can_grow(ctx, *pos),
                    _ => false,
                };
                if valid_target {
                    bonemealable.push(*pos);
                }
            }
        });

        core.input.clear_all_keys();
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
        let reach = core.drive(game, |_, ctx, _| ctx.block_reach_distance());
        let in_reach = |pos: Pos| (dist_sq(feet, pos) as f64) <= reach * reach;

        for pos in &to_break {
            if !in_reach(*pos) {
                continue;
            }
            let rot = core.drive(game, |_, ctx, _| ctx.reachable(*pos, ctx.block_reach_distance(), false));
            if let (Some(rot), true) = (rot, is_safe_to_cancel) {
                let mut looking = false;
                core.drive(game, |_, ctx, _| {
                    ctx.update_look_target(rot, true);
                    ctx.switch_to_best_tool_for(*pos);
                    looking = ctx.is_looking_at(*pos);
                });
                if looking {
                    core.input.set_input_force_state(Input::ClickLeft, true);
                }
                return PathingCommand::RequestPause;
            }
        }
        let mut both = open_farmland.clone();
        both.extend(open_soulsand.iter().copied());
        for pos in &both {
            if !in_reach(*pos) {
                continue;
            }
            let soulsand = open_soulsand.contains(pos);
            let mut click: Option<bool> = None;
            core.drive(game, |_, ctx, _| {
                let offset = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
                let Some(rot) = ctx.reachable_offset(*pos, offset, reach, false) else { return };
                if !is_safe_to_cancel || !ctx.select_item(true, &|item| if soulsand { bare(item) == "nether_wart" } else { FARMLAND_PLANTABLE.contains(&bare(item)) }) {
                    return;
                }
                if matches!(ctx.ray_trace_towards(rot, reach, false), Some((_, [0, 1, 0]))) {
                    ctx.update_look_target(rot, true);
                    click = Some(ctx.is_looking_at(*pos));
                }
            });
            if let Some(looking) = click {
                if looking {
                    core.input.set_input_force_state(Input::ClickRight, true);
                }
                return PathingCommand::RequestPause;
            }
        }
        for pos in &open_log {
            if !in_reach(*pos) {
                continue;
            }
            for dir in HORIZONTALS {
                let side = [pos[0] + dir[0], pos[1], pos[2] + dir[2]];
                let is_air = core.with_calc(game, |calc, _, _| calc.bsi.get0(side[0], side[1], side[2]).is_air());
                if !is_air {
                    continue;
                }
                let mut click: Option<bool> = None;
                core.drive(game, |_, ctx, _| {
                    // Vec3.atCenterOf(pos).add(dir * 0.5)
                    let face_center = [f64::from(pos[0]) + 0.5 + f64::from(dir[0]) * 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5 + f64::from(dir[2]) * 0.5];
                    let Some(rot) = ctx.reachable_offset(*pos, face_center, reach, false) else { return };
                    if !is_safe_to_cancel || !ctx.select_item(true, &|item| bare(item) == "cocoa_beans") {
                        return;
                    }
                    if ctx.ray_trace_towards(rot, reach, false).is_some_and(|(_, face)| face == dir) {
                        ctx.update_look_target(rot, true);
                        click = Some(ctx.is_looking_at(*pos));
                    }
                });
                if let Some(looking) = click {
                    if looking {
                        core.input.set_input_force_state(Input::ClickRight, true);
                    }
                    return PathingCommand::RequestPause;
                }
            }
        }
        for pos in &bonemealable {
            if !in_reach(*pos) {
                continue;
            }
            let mut click: Option<bool> = None;
            core.drive(game, |_, ctx, _| {
                let Some(rot) = ctx.reachable(*pos, ctx.block_reach_distance(), false) else { return };
                if is_safe_to_cancel && ctx.select_item(true, &|item| bare(item) == "bone_meal") {
                    ctx.update_look_target(rot, true);
                    click = Some(ctx.is_looking_at(*pos));
                }
            });
            if let Some(looking) = click {
                if looking {
                    core.input.set_input_force_state(Input::ClickRight, true);
                }
                return PathingCommand::RequestPause;
            }
        }

        if calc_failed {
            core.say(game, "Farm failed");
            if core.settings.flag("notificationOnFarmFail") {
                core.notify("Farm failed", true);
            }
            self.active = false;
            return PathingCommand::RequestPause;
        }

        let have = |core: &mut Core, game: &mut Game<'_>, wanted: &dyn Fn(&str) -> bool| core.drive(game, |_, ctx, _| ctx.select_item(false, wanted));
        let mut goalz: Vec<GoalRef> = Vec::new();
        for pos in &to_break {
            goalz.push(rc(GoalBreak(*pos)));
        }
        if have(core, game, &|item| FARMLAND_PLANTABLE.contains(&bare(item))) {
            for pos in &open_farmland {
                goalz.push(rc(GoalBlock([pos[0], pos[1] + 1, pos[2]])));
            }
        }
        if have(core, game, &|item| bare(item) == "nether_wart") {
            for pos in &open_soulsand {
                goalz.push(rc(GoalBlock([pos[0], pos[1] + 1, pos[2]])));
            }
        }
        if have(core, game, &|item| bare(item) == "cocoa_beans") {
            for pos in &open_log {
                for dir in HORIZONTALS {
                    let side = [pos[0] + dir[0], pos[1], pos[2] + dir[2]];
                    if core.with_calc(game, |calc, _, _| calc.bsi.get0(side[0], side[1], side[2]).is_air()) {
                        goalz.push(rc(GoalGetToBlock(side)));
                    }
                }
            }
        }
        if have(core, game, &|item| bare(item) == "bone_meal") {
            for pos in &bonemealable {
                goalz.push(rc(GoalBlock(*pos)));
            }
        }
        for item in game.dropped.iter() {
            if item.on_ground && PICKUP_DROPPED.contains(&bare(&item.stack.item)) {
                // +0.1 because of farmland's 0.9375 dummy height lol
                goalz.push(rc(GoalBlock([item.position.x.floor() as i32, (item.position.y + 0.1).floor() as i32, item.position.z.floor() as i32])));
            }
        }
        if goalz.is_empty() {
            if has_immature_crop && core.settings.flag("farmWaitForGrowth") {
                return PathingCommand::RequestPause;
            }
            core.say(game, "Farm failed");
            if core.settings.flag("notificationOnFarmFail") {
                core.notify("Farm failed", true);
            }
            self.active = false;
            return PathingCommand::RequestPause;
        }
        PathingCommand::SetGoalAndPath(rc(GoalComposite(goalz)))
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.active = false;
    }

    fn cancel(&mut self) {
        self.active = false;
    }
}

/// What `readyForHarvest` reads of the settings.
struct Flags {
    replant_crops: bool,
}

fn core_flags(s: &super::baritone_settings::Settings) -> Flags {
    Flags { replant_crops: s.flag("replantCrops") }
}

impl FarmProcess {
    /// BambooStalkBlock.isValidBonemealTarget: under 16 tall, and the top is not done growing.
    fn bamboo_can_grow(ctx: &dyn PlayerContext, pos: Pos) -> bool {
        let bamboo = |y: i32| bare(&ctx.bsi().get0(pos[0], y, pos[2]).name) == "bamboo";
        let above = (1..=16).take_while(|i| bamboo(pos[1] + i)).count() as i32;
        let below = (1..=16).take_while(|i| bamboo(pos[1] - i)).count() as i32;
        above + below + 1 < 16 && ctx.bsi().get0(pos[0], pos[1] + above, pos[2]).prop("stage") != Some("1")
    }

    fn ready_for_harvest_in(name: &str, state: &super::baritone_block_state::BlockState, ctx: &dyn PlayerContext, pos: Pos, flags: &Flags) -> bool {
        let below = ctx.bsi().get0(pos[0], pos[1] - 1, pos[2]);
        match name {
            "wheat" | "carrots" | "potatoes" => Self::state_age(state) >= 7,
            "beetroots" => Self::state_age(state) >= 3,
            "pumpkin" | "melon" => true,
            "nether_wart" => Self::state_age(state) >= 3,
            "cocoa" => Self::state_age(state) >= 2,
            "sugar_cane" | "bamboo" | "cactus" => !flags.replant_crops || bare(&below.name) == name,
            _ => false,
        }
    }
}
