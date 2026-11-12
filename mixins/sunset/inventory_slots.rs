// Inventory Manager's Slots mode: a fixed hotbar layout and a lean inventory, one action at a time, a random Delay
// apart. Written after how the classic 1.8 PvP inventory managers behave, not from any client's code:
// - the cursor emptied first; then your best armour worn, helmet to boots; then the best sword, a second sword, the
//   best pickaxe, axe, shovel, bow and a fishing rod each moved to its hotbar slot (0: none); the biggest block
//   stacks, the best potions and the most filling food to their slot ranges;
// - then what is over a limit thrown (blocks, arrows, buckets, snowballs and eggs, ender pearls), then anything not
//   worth keeping, then the worse duplicates (every sword past the one or two kept, every other tool, bow, rod and
//   armour piece but the best);
// - nothing for 10 ticks after you attack, place a block or have a container open, nor while Scaffold is on; off the
//   inventory screen only with GUI Move on and no click held (Only in inventory: on the screen only), and the
//   inventory counts as opened a tick before the first action and closed after two passes with nothing to do;
// - an item with a custom name (a server's menu item) is neither moved nor thrown unless Use / Drop custom items say.
// Slots in this file are numbered as the classic player inventory is: 0 to 8 the hotbar, 9 to 35 the rest, 36 to 39
// the boots, leggings, chestplate and helmet worn. Of two equal armour pieces the worn one stays.

use super::Game;
use super::sunset::Module;
use std::sync::Mutex;

pub(super) const SLOTS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Pickaxe,
    Axe,
    Shovel,
}

// What the manager knows of a stack.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Stack {
    pub name: String,
    pub count: u32,
    pub custom: bool,
    pub useful: bool,
    // Any block item (the block limit and slots).
    pub block: bool,
    // Armour: its type (0 helmet, 1 chestplate, 2 leggings, 3 boots) and score.
    pub armor: Option<(usize, i32)>,
    pub sword: Option<f32>,
    pub tool: Option<(Tool, f32)>,
    pub bow: Option<f32>,
    pub rod: bool,
    // A potion with effects: splash, and its effect's rank.
    pub potion: Option<(bool, i32)>,
    // Food: its saturation modifier.
    pub food: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Cfg {
    // Hotbar slots, 1 to 9 (0: none).
    pub sword: usize,
    pub second_sword: usize,
    pub pickaxe: usize,
    pub axe: usize,
    pub shovel: usize,
    pub bow: usize,
    pub rod: usize,
    pub blocks: (usize, usize),
    pub potions: (usize, usize),
    pub food: (usize, usize),
    pub limits: Limits,
    pub drop_custom: bool,
    pub use_custom: bool,
    pub splash_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Limits {
    pub blocks: u32,
    pub arrows: u32,
    pub buckets: u32,
    pub throwables: u32,
    pub pearls: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    // The cursor's stack put in an empty slot, or thrown (None).
    Cursor(Option<usize>),
    // An armour piece worn (its slot, the armour type).
    Wear(usize, usize),
    // A stack swapped to a hotbar slot (0 to 8).
    Hotbar(usize, usize),
    Drop(usize),
    // A stack cut down to `keep`, the rest thrown.
    DropPart(usize, u32),
}

// The slots of armour type `t` worn: 39 the helmet down to 36 the boots.
fn worn(t: usize) -> usize {
    39 - t
}

// The inventory sorted into what is kept where and what is spare.
#[derive(Debug, Default)]
struct Scan {
    useless: Vec<usize>,
    blocks: Vec<usize>,
    potions: Vec<usize>,
    food: Vec<usize>,
    armor: [Option<usize>; 4],
    spare_armor: [Vec<usize>; 4],
    sword: Option<usize>,
    second_sword: Option<usize>,
    spare_swords: Vec<usize>,
    tools: [Option<usize>; 3],
    spare_tools: [Vec<usize>; 3],
    bow: Option<usize>,
    spare_bows: Vec<usize>,
    rod: Option<usize>,
    spare_rods: Vec<usize>,
}

// The best of `list` by `score` (the first of equals), the rest spare.
fn best(list: &[usize], spare: &mut Vec<usize>, score: impl Fn(usize) -> f32) -> Option<usize> {
    let (&first, rest) = list.split_first()?;
    let (mut top, mut value) = (first, score(first));
    for &i in rest {
        let v = score(i);
        if v > value {
            spare.push(top);
            (top, value) = (i, v);
        } else {
            spare.push(i);
        }
    }
    Some(top)
}

fn scan(inv: &[Option<Stack>; SLOTS], cfg: &Cfg) -> Scan {
    let mut s = Scan::default();
    let (mut armor, mut swords, mut tools, mut bows, mut rods): ([Vec<usize>; 4], Vec<usize>, [Vec<usize>; 3], Vec<usize>, Vec<usize>) =
        Default::default();
    for (i, stack) in inv.iter().enumerate() {
        let Some(st) = stack else { continue };
        if !st.useful {
            s.useless.push(i);
            continue;
        }
        if st.block {
            s.blocks.push(i);
        }
        if st.potion.is_some() {
            s.potions.push(i);
        }
        if st.food.is_some() {
            s.food.push(i);
        }
        if let Some((t, _)) = st.armor {
            armor[t].push(i);
        } else if st.sword.is_some() {
            swords.push(i);
        } else if let Some((tool, _)) = st.tool {
            tools[tool as usize].push(i);
        } else if st.bow.is_some() {
            bows.push(i);
        } else if st.rod {
            rods.push(i);
        }
    }
    let get = |i: usize| inv[i].as_ref().expect("scanned");
    // The piece worn first, so an equal one is the spare (not the worn one, which would be thrown and worn again).
    for (t, list) in armor.iter_mut().enumerate() {
        if let Some(at) = list.iter().position(|&i| i == worn(t)) {
            let w = list.remove(at);
            list.insert(0, w);
        }
    }
    for t in 0..4 {
        s.armor[t] = best(&armor[t], &mut s.spare_armor[t], |i| get(i).armor.map_or(0.0, |a| a.1 as f32));
    }
    // Swords by damage, best first (a stable sort keeps equals in slot order).
    swords.sort_by(|&a, &b| get(b).sword.unwrap_or(0.0).total_cmp(&get(a).sword.unwrap_or(0.0)));
    if let Some((&first, rest)) = swords.split_first() {
        s.sword = Some(first);
        let two = cfg.sword != 0 && cfg.second_sword != 0 && cfg.second_sword != cfg.sword;
        let rest = if two && !rest.is_empty() {
            s.second_sword = Some(rest[0]);
            &rest[1..]
        } else {
            rest
        };
        s.spare_swords.extend_from_slice(rest);
    }
    for t in 0..3 {
        s.tools[t] = best(&tools[t], &mut s.spare_tools[t], |i| get(i).tool.map_or(0.0, |x| x.1));
    }
    s.bow = best(&bows, &mut s.spare_bows, |i| get(i).bow.unwrap_or(0.0));
    if let Some((&first, rest)) = rods.split_first() {
        s.rod = Some(first);
        s.spare_rods.extend_from_slice(rest);
    }
    s
}

// A stack already in hotbar slot `target` (0 to 8).
fn at(slot: usize, target: usize) -> bool {
    slot <= 8 && slot == target
}

// Where a kept item goes: its hotbar slot (setting 1 to 9), unless it is there or none is set.
fn wants(slot: Option<usize>, setting: usize) -> Option<(usize, usize)> {
    let slot = slot?;
    (setting != 0 && !at(slot, setting - 1)).then_some((slot, setting - 1))
}

// A sword kept in the hotbar slot `setting` names.
fn sword_there(s: &Scan, cfg: &Cfg, setting: usize) -> bool {
    setting != 0 && ((s.sword.is_some() && cfg.sword == setting) || (s.second_sword.is_some() && cfg.second_sword == setting))
}

// The hotbar moves of the kept tools, in order.
fn tool_moves(s: &Scan, cfg: &Cfg) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    out.extend(wants(s.sword, cfg.sword));
    out.extend(wants(s.second_sword, cfg.second_sword));
    for (tool, setting) in [(s.tools[0], cfg.pickaxe), (s.tools[1], cfg.axe), (s.tools[2], cfg.shovel), (s.bow, cfg.bow)] {
        if !sword_there(s, cfg, setting) {
            out.extend(wants(tool, setting));
        }
    }
    if !sword_there(s, cfg, cfg.rod) && cfg.rod != 0 && (cfg.bow == 0 || cfg.rod != cfg.bow || s.bow.is_none()) {
        out.extend(wants(s.rod, cfg.rod));
    }
    out
}

// The moves filling a slot range (first, last; 1 to 9) from `list`, already in order.
fn range_moves(list: &[usize], (first, last): (usize, usize)) -> Vec<(usize, usize)> {
    if first == 0 || first > last {
        return Vec::new();
    }
    list.iter()
        .take(last - first + 1)
        .enumerate()
        .map(|(l, &slot)| (slot, first - 1 + l))
        .filter(|&(slot, target)| !at(slot, target))
        .collect()
}

fn ordered_blocks(inv: &[Option<Stack>; SLOTS], s: &Scan) -> Vec<usize> {
    let mut list = s.blocks.clone();
    list.sort_by_key(|&i| std::cmp::Reverse(inv[i].as_ref().map_or(0, |x| x.count)));
    list
}

fn ordered_potions(inv: &[Option<Stack>; SLOTS], s: &Scan, cfg: &Cfg) -> Vec<usize> {
    let mut list = s.potions.clone();
    let potion = |i: usize| inv[i].as_ref().and_then(|x| x.potion).unwrap_or((false, -1));
    list.sort_by(|&a, &b| {
        let ((sa, ra), (sb, rb)) = (potion(a), potion(b));
        if cfg.splash_first && sa != sb {
            return sb.cmp(&sa);
        }
        rb.cmp(&ra)
    });
    list
}

fn ordered_food(inv: &[Option<Stack>; SLOTS], s: &Scan) -> Vec<usize> {
    let mut list = s.food.clone();
    let food = |i: usize| inv[i].as_ref().and_then(|x| x.food).unwrap_or(0.0);
    list.sort_by(|&a, &b| food(b).total_cmp(&food(a)));
    list
}

// The stacks of each limited kind, in slot order, and the limit.
fn limited(inv: &[Option<Stack>; SLOTS], cfg: &Cfg) -> [(Vec<usize>, u32); 5] {
    let of = |is: &dyn Fn(&Stack) -> bool| -> Vec<usize> {
        inv.iter().enumerate().filter(|(_, x)| x.as_ref().is_some_and(is)).map(|(i, _)| i).collect()
    };
    let l = cfg.limits;
    [
        (of(&|x| x.block), l.blocks),
        (of(&|x| x.name == "arrow"), l.arrows),
        (of(&|x| matches!(x.name.as_str(), "bucket" | "water_bucket" | "lava_bucket" | "milk_bucket")), l.buckets),
        (of(&|x| matches!(x.name.as_str(), "snowball" | "egg")), l.throwables),
        (of(&|x| x.name == "ender_pearl"), l.pearls),
    ]
}

fn total(inv: &[Option<Stack>; SLOTS], list: &[usize]) -> u32 {
    list.iter().filter_map(|&i| inv[i].as_ref()).map(|x| x.count).sum()
}

// Worn armour to change: the best piece of type `t`, when it is not the one worn and beats it.
fn better_armor(inv: &[Option<Stack>; SLOTS], s: &Scan, t: usize) -> Option<usize> {
    let best = s.armor[t]?;
    let score = inv[best].as_ref()?.armor?.1;
    let worn_score = inv[worn(t)].as_ref().and_then(|x| x.armor).map_or(i32::MIN, |a| a.1);
    (best != worn(t) && score > worn_score).then_some(best)
}

fn spares(s: &Scan) -> impl Iterator<Item = usize> + '_ {
    s.spare_swords
        .iter()
        .chain(s.spare_tools.iter().flatten())
        .chain(&s.spare_bows)
        .chain(&s.spare_rods)
        .chain(s.spare_armor.iter().flatten())
        .copied()
}

// Whether the inventory has anything to do (the inventory is opened for it).
pub(super) fn needs_work(inv: &[Option<Stack>; SLOTS], cursor: bool, cfg: &Cfg) -> bool {
    let s = scan(inv, cfg);
    cursor
        || (0..4).any(|t| better_armor(inv, &s, t).is_some())
        || !tool_moves(&s, cfg).is_empty()
        || !range_moves(&ordered_blocks(inv, &s), cfg.blocks).is_empty()
        || !range_moves(&ordered_potions(inv, &s, cfg), cfg.potions).is_empty()
        || !range_moves(&ordered_food(inv, &s), cfg.food).is_empty()
        || limited(inv, cfg).iter().any(|(list, limit)| total(inv, list) > *limit)
        || !s.useless.is_empty()
        || spares(&s).next().is_some()
}

// This pass's action: the first of the manager's order that may go. `may(slot, drop)`: whether the stack in `slot`
// may be clicked (moved or, with `drop`, thrown); `empty`: the first empty inventory slot (not armour).
pub(super) fn next_action(inv: &[Option<Stack>; SLOTS], cursor: bool, cfg: &Cfg, may: impl Fn(usize, bool) -> bool) -> Option<Action> {
    if cursor {
        return Some(Action::Cursor((0..36).find(|&i| inv[i].is_none())));
    }
    let s = scan(inv, cfg);
    for t in 0..4 {
        if let Some(from) = better_armor(inv, &s, t) {
            return Some(Action::Wear(from, t));
        }
    }
    let custom_ok = |slot: usize, drop: bool| {
        inv[slot].as_ref().is_none_or(|x| !x.custom || if drop { cfg.drop_custom } else { cfg.use_custom }) && may(slot, drop)
    };
    for (from, to) in tool_moves(&s, cfg) {
        if custom_ok(from, false) {
            return Some(Action::Hotbar(from, to));
        }
    }
    for moves in [
        range_moves(&ordered_blocks(inv, &s), cfg.blocks),
        range_moves(&ordered_potions(inv, &s, cfg), cfg.potions),
        range_moves(&ordered_food(inv, &s), cfg.food),
    ] {
        if let Some(&(from, to)) = moves.iter().find(|(from, _)| custom_ok(*from, false)) {
            return Some(Action::Hotbar(from, to));
        }
    }
    for (list, limit) in limited(inv, cfg) {
        let count = total(inv, &list);
        if count <= limit {
            continue;
        }
        let excess = count - limit;
        // From the last stack back.
        if let Some(&i) = list.iter().rev().find(|&&i| custom_ok(i, true)) {
            let size = inv[i].as_ref().map_or(0, |x| x.count);
            return Some(if excess >= size { Action::Drop(i) } else { Action::DropPart(i, size - excess) });
        }
    }
    if let Some(i) = s.useless.first().copied().filter(|&i| custom_ok(i, true)) {
        return Some(Action::Drop(i));
    }
    spares(&s).find(|&i| custom_ok(i, true)).map(Action::Drop)
}

// ---- the game side

// The manager's own state: its pause after an action, the counts since an attack, a block placed and a container,
// and whether the inventory counts as open.
struct State {
    next_click: std::time::Duration,
    since_click: std::time::Instant,
    attack: u32,
    place: u32,
    container: u32,
    alive: u32,
    entity: Option<i32>,
    open: bool,
    open_delay: u32,
    idle: u32,
    rng: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| State {
        next_click: std::time::Duration::ZERO,
        since_click: std::time::Instant::now(),
        attack: 100,
        place: 100,
        container: 100,
        alive: 0,
        entity: None,
        open: false,
        open_delay: 0,
        idle: 0,
        rng: 0x2545_f491_4f6c_dd1d,
    }))
}

// An attack or a block placed (not a water bucket's) went out: the manager waits 10 ticks.
pub(super) fn sent(packet: &horizon_protocol::packets::play::Serverbound) {
    use horizon_protocol::packets::play::Serverbound as S;
    match packet {
        S::Attack(_) => st(|s| s.attack = 0),
        S::UseItemOn(_) => st(|s| s.place = 0),
        _ => {}
    }
}

fn close(s: &mut State) {
    s.open = false;
    s.open_delay = 0;
    s.idle = 0;
}

// The classic slot number's place in Sunset's numbering (armour 500 and on, helmet first).
fn internal(slot: usize) -> usize {
    if slot < 36 { slot } else { 500 + (39 - slot) }
}

fn stack_at<'a>(game: &'a Game<'_>, slot: usize) -> Option<&'a horizon_client::items::ItemStack> {
    if slot < 36 { game.inventory.get(slot).and_then(Option::as_ref) } else { game.armor.get(39 - slot).and_then(Option::as_ref) }
}

// The 1.8 potion ranks (instant health 1 ... water breathing 12) of a modern potion's effect, -1 for none ranked.
fn potion_rank(potion: &str) -> i32 {
    let p = super::sunset_util::short(potion).trim_start_matches("long_").trim_start_matches("strong_");
    match p {
        "healing" => 1,
        "regeneration" => 2,
        "turtle_master" => 3,
        "strength" => 7,
        "swiftness" => 8,
        "fire_resistance" => 9,
        "invisibility" => 10,
        "water_breathing" => 12,
        _ => -1,
    }
}

const FALLS: [&str; 6] = ["sand", "gravel", "concrete_powder", "anvil", "dragon_egg", "pointed_dripstone"];
// Full blocks that hold something (a block entity) or bounce: not building blocks.
const NOT_BUILDING: [&str; 14] = [
    "furnace", "blast_furnace", "smoker", "barrel", "dispenser", "dropper", "jukebox", "beacon", "spawner", "note_block",
    "slime_block", "honey_block", "crafter", "trial_spawner",
];
// Items a classic manager keeps besides weapons, tools, armour, food, potions and building blocks.
const KEPT: [&str; 10] = ["fishing_rod", "water_bucket", "bucket", "arrow", "bow", "snowball", "egg", "ender_pearl", "clay_ball", "fire_charge"];

fn describe(game: &Game<'_>, m: &Module, stack: &horizon_client::items::ItemStack) -> Stack {
    let item = super::sunset_player::plan_item(game, stack);
    let name = item.name.clone();
    let level = |e: &str| item.enchants.iter().find(|(x, _)| super::sunset_util::short(x) == e).map_or(0, |x| x.1);
    let info = game.res.items().get(&stack.item);
    let block = info.as_ref().and_then(|i| i.block).is_some();
    let building = item.block
        && !FALLS.iter().any(|f| name.ends_with(f))
        && !NOT_BUILDING.contains(&name.as_str())
        && !name.contains("shulker_box");
    let armor_type = if name.ends_with("_helmet") {
        Some(0)
    } else if name.ends_with("_chestplate") {
        Some(1)
    } else if name.ends_with("_leggings") {
        Some(2)
    } else if name.ends_with("_boots") {
        Some(3)
    } else {
        None
    };
    let tier = |n: &str| match n.split('_').next().unwrap_or("") {
        "netherite" => 6,
        "diamond" => 5,
        "iron" => 4,
        "chainmail" => 3,
        "golden" => 2,
        "leather" => 1,
        _ => 0,
    };
    let armor = armor_type.map(|t| {
        let points = item.armor.round() as i32;
        let other = level("projectile_protection") + level("fire_protection") + level("blast_protection");
        (t, points * 100 + tier(&name) * 10 + level("protection") as i32 * 6 + other as i32 * 2)
    });
    let efficiency = level("efficiency") as f32;
    let tool_score = item.mining + if efficiency > 0.0 { efficiency * efficiency + 1.0 } else { 0.0 };
    let tool = if name.ends_with("_pickaxe") {
        Some((Tool::Pickaxe, tool_score))
    } else if name.ends_with("_axe") {
        Some((Tool::Axe, tool_score))
    } else if name.ends_with("_shovel") {
        Some((Tool::Shovel, tool_score))
    } else {
        None
    };
    let sword = name.ends_with("_sword").then(|| item.attack + level("sharpness") as f32 * 1.25 + level("fire_aspect") as f32 * 2.5);
    let potion = matches!(name.as_str(), "potion" | "splash_potion" | "lingering_potion")
        .then(|| stack.potion())
        .flatten()
        .filter(|p| !matches!(super::sunset_util::short(p), "water" | "awkward" | "mundane" | "thick"))
        .map(|p| (name != "potion", potion_rank(p)));
    let food = info.as_ref().and_then(|i| i.food).map(|(nutrition, saturation)| {
        if nutrition > 0 { saturation as f32 / (nutrition as f32 * 2.0) } else { 0.0 }
    });
    let kept = super::sunset_util::listed(m.t("Protected items"), &name);
    let useful = building
        || name == "tnt"
        || name.ends_with("glass")
        || sword.is_some()
        || tool.is_some()
        || armor.is_some()
        || food.is_some()
        || matches!(name.as_str(), "potion" | "splash_potion" | "lingering_potion")
        || KEPT.contains(&name.as_str())
        || kept;
    Stack {
        name,
        count: stack.count,
        custom: item.named,
        useful,
        block,
        armor,
        sword,
        tool,
        bow: (item.name == "bow").then(|| level("power") as f32),
        rod: item.name == "fishing_rod",
        potion,
        food,
    }
}

fn cfg(m: &Module) -> Cfg {
    let n = |name: &str| m.n(name).max(0.0) as usize;
    let range = |from: &str, to: &str| (n(from), n(to));
    Cfg {
        sword: n("Sword slot"),
        second_sword: n("Second sword slot"),
        pickaxe: n("Pickaxe slot"),
        axe: n("Axe slot"),
        shovel: n("Shovel slot"),
        bow: n("Bow slot"),
        rod: n("Rod slot"),
        blocks: range("Block slots from", "Block slots to"),
        potions: range("Potion slots from", "Potion slots to"),
        food: range("Food slots from", "Food slots to"),
        limits: Limits {
            blocks: m.n("Block limit") as u32,
            arrows: m.n("Arrow limit") as u32,
            buckets: m.n("Bucket limit") as u32,
            throwables: m.n("Snowball and egg limit") as u32,
            pearls: m.n("Ender pearl limit") as u32,
        },
        drop_custom: m.b("Drop custom items"),
        use_custom: m.b("Use custom items"),
        splash_first: m.b("Prioritize splash potions"),
    }
}

fn unit(rng: &mut u64) -> f64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    (*rng >> 11) as f64 / (1u64 << 53) as f64
}

// One pass, each tick before the game's (so its clicks go before the tick's move packet). `m`: Inventory Manager, on,
// in Slots mode.
pub(super) fn pre_tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        st(close);
        return;
    };
    let entity = game.remote.as_ref().map(|r| r.entity_id);
    let menu = game.remote_menu();
    let player_menu = menu.0 == 0 && menu.1 == "inventory";
    let in_inventory = game.screen == super::Screen::Inventory && player_menu;
    let foreign = !player_menu || !matches!(game.screen, super::Screen::Inventory | super::Screen::Playing);
    let ready = st(|s| {
        if s.entity != entity {
            (s.entity, s.alive) = (entity, 0);
            close(s);
        }
        s.alive = s.alive.saturating_add(1);
        s.attack = s.attack.saturating_add(1);
        s.place = s.place.saturating_add(1);
        s.container = if foreign { 0 } else { s.container.saturating_add(1) };
        s.alive > 40
    });
    if !ready || game.player.is_none() || game.remote.is_none() {
        return;
    }
    let scaffold = super::sunset::with(|s| s.is_on("Scaffold"));
    let gui_move = super::sunset::with(|s| s.is_on("GUI Move"));
    // Only in inventory: on the inventory screen only, the pause counting from when it opened.
    if m.b("Only in inventory") && !in_inventory {
        st(|s| {
            s.since_click = std::time::Instant::now();
            close(s);
        });
        return;
    }
    let paused = st(|s| s.since_click.elapsed() < s.next_click || s.container < 10 || s.attack < 10 || s.place < 10);
    if paused || scaffold || foreign || (!gui_move && !in_inventory) || super::sunset_gui_move::inventory_busy() {
        st(close);
        return;
    }
    let locked = super::sunset_loadouts::locked_slots();
    let inv: [Option<Stack>; SLOTS] = std::array::from_fn(|i| stack_at(game, i).map(|s| describe(game, m, s)));
    let cursor = game.carried.is_some();
    let c = cfg(m);
    let work = needs_work(&inv, cursor, &c);
    // The inventory counts as opened a tick before the first action (at once on its screen).
    let can_open = gui_move
        && game.screen == super::Screen::Playing
        && !scaffold
        && !game.left
        && !game.right
        && !game.physical_buttons.contains(&winit::event::MouseButton::Left)
        && !game.physical_buttons.contains(&winit::event::MouseButton::Right);
    let go = st(|s| {
        if !s.open {
            if !work {
                return false;
            }
            s.idle = 0;
            if in_inventory {
                (s.open, s.open_delay) = (true, 0);
            } else if can_open {
                (s.open, s.open_delay) = (true, 1);
            } else {
                return false;
            }
        } else if !work {
            close(s);
            return false;
        }
        if s.open_delay > 0 {
            s.open_delay -= 1;
            return false;
        }
        true
    });
    if !go || super::sunset_combat_control::owner().is_some() {
        return;
    }
    let may = |slot: usize, _drop: bool| {
        let i = internal(slot);
        !(i < 36 && locked[i]) && !super::sunset_loadouts::locked(i)
    };
    let action = next_action(&inv, cursor, &c, may);
    let moved = action.is_some_and(|a| act(game, a, &locked));
    let (lo, hi) = (m.n("Delay min (ms)"), m.n("Delay max (ms)"));
    st(|s| {
        if moved {
            let roll = unit(&mut s.rng);
            s.next_click = std::time::Duration::from_millis((lo.min(hi) + (hi - lo).abs() * roll).round() as u64);
            s.since_click = std::time::Instant::now();
            s.idle = 0;
        } else {
            // Two passes with nothing done: the inventory counts as closed.
            s.idle += 1;
            if s.idle >= 2 {
                close(s);
            }
        }
    });
}

// The clicks of an action. True when they went.
fn act(game: &mut Game<'_>, action: Action, locked: &[bool; 36]) -> bool {
    use horizon_client::menu::Input::{Pickup, Swap, Throw};
    let ops: Vec<(Option<usize>, i32, horizon_client::menu::Input)> = match action {
        Action::Cursor(Some(slot)) => vec![(Some(internal(slot)), 0, Pickup)],
        Action::Cursor(None) => vec![(None, 0, Pickup)],
        Action::Wear(from, t) => {
            super::sunset_gui_move::direct(|| super::sunset::wear(game, internal(from), t));
            return true;
        }
        Action::Hotbar(from, to) => {
            if locked[to] {
                return false;
            }
            vec![(Some(internal(from)), to as i32, Swap)]
        }
        Action::Drop(slot) => vec![(Some(internal(slot)), 1, Throw)],
        Action::DropPart(slot, keep) => {
            let mut ops = vec![(Some(internal(slot)), 0, Pickup)];
            ops.extend((0..keep).map(|_| (Some(internal(slot)), 1, Pickup)));
            ops.push((None, 0, Pickup));
            ops
        }
    };
    super::sunset_gui_move::direct(|| game.remote_menu_ops(&ops))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: Cfg = Cfg {
        sword: 1,
        second_sword: 2,
        pickaxe: 2,
        axe: 3,
        shovel: 4,
        bow: 7,
        rod: 8,
        blocks: (5, 5),
        potions: (6, 6),
        food: (9, 9),
        limits: Limits { blocks: 512, arrows: 128, buckets: 1, throwables: 16, pearls: 16 },
        drop_custom: false,
        use_custom: false,
        splash_first: false,
    };

    fn empty() -> [Option<Stack>; SLOTS] {
        std::array::from_fn(|_| None)
    }

    fn sword(damage: f32) -> Option<Stack> {
        Some(Stack { name: "iron_sword".into(), count: 1, useful: true, sword: Some(damage), ..Stack::default() })
    }

    fn blocks(count: u32) -> Option<Stack> {
        Some(Stack { name: "oak_planks".into(), count, useful: true, block: true, ..Stack::default() })
    }

    fn helmet(score: i32) -> Option<Stack> {
        Some(Stack { name: "iron_helmet".into(), count: 1, useful: true, armor: Some((0, score)), ..Stack::default() })
    }

    #[test]
    fn the_order_is_cursor_armour_tools_then_ranges_then_drops() {
        let mut inv = empty();
        assert_eq!(next_action(&inv, true, &CFG, |_, _| true), Some(Action::Cursor(Some(0))));
        inv[20] = helmet(500);
        inv[12] = sword(7.0);
        inv[30] = blocks(64);
        // A better helmet first, then the sword to slot 1, then blocks to slot 5.
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Wear(20, 0)));
        inv[39] = inv[20].take();
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Hotbar(12, 0)));
        inv[0] = inv[12].take();
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Hotbar(30, 4)));
        inv[4] = inv[30].take();
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), None);
        assert!(!needs_work(&inv, false, &CFG));
    }

    #[test]
    fn two_swords_take_both_slots_and_the_rest_are_thrown() {
        let mut inv = empty();
        inv[0] = sword(6.0);
        inv[1] = sword(7.0);
        inv[9] = sword(5.0);
        // The best (slot 1) to slot 1 first: swords sorted by damage.
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Hotbar(1, 0)));
        inv.swap(0, 1);
        // The second best is in slot 2 (index 1) already; the worst goes.
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Drop(9)));
    }

    #[test]
    fn limits_throw_from_the_last_stack_and_cut_one_down() {
        let mut inv = empty();
        let cfg = Cfg { limits: Limits { blocks: 100, ..CFG.limits }, blocks: (0, 0), ..CFG };
        inv[9] = blocks(64);
        inv[10] = blocks(64);
        // 128 over 100: the last stack keeps 36.
        assert_eq!(next_action(&inv, false, &cfg, |_, _| true), Some(Action::DropPart(10, 36)));
        inv[11] = blocks(64);
        // 192 over 100: the last stack (64 <= 92) goes whole.
        assert_eq!(next_action(&inv, false, &cfg, |_, _| true), Some(Action::Drop(11)));
    }

    #[test]
    fn junk_goes_and_custom_items_are_left_alone() {
        let mut inv = empty();
        inv[15] = Some(Stack { name: "rotten_flesh".into(), count: 3, ..Stack::default() });
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Drop(15)));
        inv[15].as_mut().unwrap().custom = true;
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), None, "a menu item stays");
        let cfg = Cfg { drop_custom: true, ..CFG };
        assert_eq!(next_action(&inv, false, &cfg, |_, _| true), Some(Action::Drop(15)));
    }

    #[test]
    fn a_worn_piece_as_good_as_the_spare_stays_on_and_the_spare_goes() {
        let mut inv = empty();
        inv[39] = helmet(500);
        inv[20] = helmet(500);
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Drop(20)));
        inv[20] = helmet(600);
        assert_eq!(next_action(&inv, false, &CFG, |_, _| true), Some(Action::Wear(20, 0)));
    }
}
