// Inventory Manager's (and Auto Armor's) decisions, apart from the inventory they run on: player.rs turns stacks into
// `Item`s (the snapshot) and runs one step at a time (inventory_exec.rs). Here:
//
// 1. Classification: what an item is for (`Kind`, `BlockUse`, `speciality`). Unknown and modded items are Other and are
//    never dropped as duplicates.
// 2. Evaluation, by role: weapons (a hit's damage and damage over time, by combat version), tools (mining speed; a
//    Fortune and a Silk Touch pickaxe are different tools), armour (vanilla's damage reduction, shared with Auto
//    Armor), and what a hotbar slot wants of consumables and blocks.
// 3. The hotbar: all nine wishes planned together (an assignment search), what already sits well left alone.
// 4. Retention before cleanup: the hotbar's picks, the best of each kind (and backups by policy), specialised and
//    protected items are kept; only the rest may go.
// 5. Merging and sorting steps.
//
// Every planner returns the next single step for the inventory as it is; an inventory already as wanted gives none.
// Slots are Sunset's: 0 to 8 the hotbar, 9 to 35 the rest.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Kind {
    Sword,
    Axe,
    Pickaxe,
    Shovel,
    Hoe,
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    Elytra,
    Mace,
    Spear,
    Trident,
    Bow,
    Crossbow,
    Other,
}

impl Kind {
    pub(super) fn of(name: &str) -> Self {
        let ends = |s: &str| name.ends_with(s);
        match name {
            "mace" => Self::Mace,
            "trident" => Self::Trident,
            "bow" => Self::Bow,
            "crossbow" => Self::Crossbow,
            "elytra" => Self::Elytra,
            _ if ends("_sword") => Self::Sword,
            _ if ends("_pickaxe") => Self::Pickaxe,
            _ if ends("_axe") => Self::Axe,
            _ if ends("_shovel") => Self::Shovel,
            _ if ends("_hoe") => Self::Hoe,
            _ if ends("_spear") => Self::Spear,
            _ if ends("_helmet") => Self::Helmet,
            _ if ends("_chestplate") => Self::Chestplate,
            _ if ends("_leggings") => Self::Leggings,
            _ if ends("_boots") => Self::Boots,
            _ => Self::Other,
        }
    }

    // Kinds where the best piece (and backups by policy) is enough, so worse ones may go. The rest each do something of
    // their own (maces, spears, tridents, bows, crossbows, elytras, and whatever Sunset does not know) and never count
    // as duplicates.
    fn replaceable(self) -> bool {
        matches!(self, Self::Sword | Self::Axe | Self::Pickaxe | Self::Shovel | Self::Hoe | Self::Helmet | Self::Chestplate
            | Self::Leggings | Self::Boots)
    }

    pub(super) fn armor(self) -> bool {
        matches!(self, Self::Helmet | Self::Chestplate | Self::Leggings | Self::Boots)
    }

    fn tool(self) -> bool {
        matches!(self, Self::Pickaxe | Self::Shovel | Self::Hoe)
    }
}

// The snapshot of one stack. Numbers are the game's: `attack` the item's attack damage bonus, `speed` its attacks a
// second (base included), `armor` and `toughness` its armour points, `mining` its best mining speed, `food` nutrition
// plus saturation. `meta` stands for its components beyond count and wear (0: none): stacks merge only when equal.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Item {
    pub(super) name: String,
    pub(super) count: u32,
    pub(super) max_stack: u32,
    pub(super) attack: f32,
    pub(super) speed: f32,
    pub(super) armor: f32,
    pub(super) toughness: f32,
    pub(super) mining: f32,
    pub(super) damage: u32,
    pub(super) max_damage: u32,
    pub(super) enchants: Vec<(String, u32)>,
    pub(super) named: bool,
    pub(super) block: bool,
    pub(super) food: f32,
    pub(super) meta: u64,
}

// What a full block is good for: building, or not, because it falls (sand, gravel, concrete powder...) or is too
// valuable to bridge with (obsidian and the like, Crystal PvP's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlockUse {
    Building,
    Falls,
    Valuable,
}

const FALLING: [&str; 7] = ["sand", "red_sand", "gravel", "anvil", "dragon_egg", "suspicious_sand", "suspicious_gravel"];
const VALUABLE: [&str; 7] = ["obsidian", "crying_obsidian", "respawn_anchor", "ender_chest", "beacon", "netherite_block", "diamond_block"];

impl Item {
    pub(super) fn kind(&self) -> Kind {
        Kind::of(&self.name)
    }

    fn level(&self, enchant: &str) -> u32 {
        self.enchants.iter().find(|(e, _)| e.strip_prefix("minecraft:").unwrap_or(e) == enchant).map_or(0, |e| e.1)
    }

    pub(super) fn block_use(&self) -> Option<BlockUse> {
        if !self.block {
            return None;
        }
        let n = self.name.as_str();
        Some(if FALLING.contains(&n) || n.ends_with("concrete_powder") || n.ends_with("_anvil") {
            BlockUse::Falls
        } else if VALUABLE.contains(&n) {
            BlockUse::Valuable
        } else {
            BlockUse::Building
        })
    }

    // Shulker boxes and bundles carry other items.
    fn container(&self) -> bool {
        self.name.ends_with("shulker_box") || self.name.ends_with("bundle")
    }

    // How much of its durability is left, 0 to 1 (1 for an item that does not wear).
    fn left(&self) -> f32 {
        if self.max_damage == 0 { 1.0 } else { 1.0 - (self.damage as f32 / self.max_damage as f32).clamp(0.0, 1.0) }
    }

    // Wear: a piece about to break is worth half; Unbreaking and Mending make wear matter less.
    fn wear(&self) -> f32 {
        let left = self.left();
        let lasting = 1.0 + 0.1 * self.level("unbreaking") as f32 + if self.level("mending") > 0 { 0.2 } else { 0.0 };
        if left < 0.05 { 0.5 } else { (0.85 + 0.15 * left).min(1.0) * lasting }
    }

    // What sets a piece apart from others of its kind: Fortune, Silk Touch, Looting, Fire Aspect, Riptide... Two pieces
    // with different specialities serve different purposes. Plain improvements (Sharpness, Efficiency, Protection,
    // Unbreaking, Mending, Power) and curses are no speciality.
    fn speciality(&self) -> Vec<&str> {
        const PLAIN: [&str; 6] = ["unbreaking", "mending", "sharpness", "efficiency", "protection", "power"];
        let mut s: Vec<&str> = self.enchants.iter().map(|(e, _)| e.strip_prefix("minecraft:").unwrap_or(e))
            .filter(|e| !PLAIN.contains(e) && !e.ends_with("_curse")).collect();
        s.sort_unstable();
        s.dedup();
        s
    }
}

// ---- evaluation

// A weapon's damage: a hit's (what counts in Legacy 1.8, where nothing waits on a cooldown) and over time (a hit at full
// charge times attacks a second, Modern 1.9+). Sharpness adds 1.25 a level in Legacy, 0.5 a level plus 0.5 in Modern;
// Fire Aspect and Knockback a little. Both scaled by wear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Damage {
    pub(super) hit: f32,
    pub(super) sustained: f32,
}

pub(super) fn weapon_damage(item: &Item, legacy: bool) -> Damage {
    let sharpness = item.level("sharpness") as f32;
    let bonus = if legacy { 1.25 * sharpness } else if sharpness > 0.0 { 0.5 * sharpness + 0.5 } else { 0.0 };
    let hit = (1.0 + item.attack.max(0.0) + bonus + 0.5 * item.level("fire_aspect") as f32 + 0.2 * item.level("knockback") as f32)
        * item.wear();
    let sustained = if legacy { hit } else { hit * item.speed.clamp(0.1, 4.0) };
    Damage { hit, sustained }
}

// How a weapon ranks for its combat model.
pub(super) fn weapon_score(item: &Item, legacy: bool) -> f32 {
    let d = weapon_damage(item, legacy);
    if legacy { d.hit } else { d.sustained }
}

// A tool's worth: its mining speed with Efficiency (level² + 1, when it is faster than a hand), then wear.
pub(super) fn tool_score(item: &Item) -> f32 {
    let efficiency = item.level("efficiency");
    let speed = item.mining + if item.mining > 1.0 && efficiency > 0 { (efficiency * efficiency + 1) as f32 } else { 0.0 };
    speed * item.wear()
}

// The hits armour is judged against: a light one, a sword crit, a heavy one.
const REFERENCE_HITS: [f32; 3] = [6.0, 12.0, 20.0];

// Vanilla's damage reduction for a set with `armor` points and `toughness` against a `damage` hit (points less damage
// over 2 + toughness / 4, at least a fifth of them, at most 20, each 4%), then `epf` (enchantment protection, 4% a point,
// at most 20) on what is left: the share of the hit taken away.
pub(super) fn reduction(armor: f32, toughness: f32, epf: f32, damage: f32) -> f32 {
    let points = (armor - damage / (2.0 + toughness / 4.0)).max(armor / 5.0).min(20.0);
    let after_armor = 1.0 - points / 25.0;
    let after_enchant = 1.0 - epf.clamp(0.0, 20.0) * 0.04;
    1.0 - after_armor * after_enchant
}

// An armour piece's worth: the reduction a set of four like it gives, averaged over the reference hits (so points,
// toughness and Protection count as vanilla counts them), then wear. Shared with Auto Armor, so what Inventory Manager
// keeps and what Auto Armor wears never disagree. Pieces are only ever compared with pieces for the same slot.
pub(super) fn armor_score(item: &Item) -> f32 {
    let special = ["fire_protection", "blast_protection", "projectile_protection"].iter().map(|e| item.level(e)).sum::<u32>() as f32;
    // Protection is 1 EPF a level against everything; the special ones count half (each covers one kind of hit).
    let epf = 4.0 * (item.level("protection") as f32 + 0.5 * special);
    let mean = REFERENCE_HITS.iter().map(|&d| reduction(4.0 * item.armor, 4.0 * item.toughness, epf, d)).sum::<f32>() / REFERENCE_HITS.len() as f32;
    mean * item.wear()
}

pub(super) fn score(item: &Item, legacy: bool) -> f32 {
    let kind = item.kind();
    if kind.armor() {
        armor_score(item)
    } else if kind.tool() {
        tool_score(item)
    } else if kind == Kind::Axe {
        // An axe is a weapon and a tool: ranked by the better of the two.
        weapon_score(item, legacy).max(tool_score(item))
    } else {
        weapon_score(item, legacy)
    }
}

// ---- retention

// How many of each kind to keep: the best only, the best and a backup, or (Keep specialized) the best of each
// speciality too. In every policy a piece with an enchantment the kept ones lack is kept as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Retention {
    BestOnly,
    BestPlusBackup,
    KeepSpecialized,
}

impl Retention {
    pub(super) fn named(name: &str) -> Self {
        match name {
            "Best only" => Self::BestOnly,
            "Keep specialized" => Self::KeepSpecialized,
            _ => Self::BestPlusBackup,
        }
    }
}

// Each enchantment `weaker` has is also on `better` at the same level or higher, and it has no name of its own.
fn dominated(weaker: &Item, better: &Item) -> bool {
    !weaker.named && weaker.enchants.iter().all(|(e, l)| better.enchants.iter().any(|(b, bl)| b == e && bl >= l))
}

type Group<'a> = ((Kind, Vec<&'a str>), Vec<(Option<usize>, &'a Item)>);

// The slots whose items are excess under `retention`: of each replaceable kind (and, with Keep specialized, each
// speciality within it), the ones past those kept that a kept piece dominates. Worn armour takes part (and is never
// excess). Never a `keep` slot (protected, Loadouts', the hotbar plan's), a named piece, or anything not replaceable.
pub(super) fn excess<'a>(slots: &'a [Option<Item>], worn: &'a [Option<Item>; 4], keep: &[bool], legacy: bool, retention: Retention) -> Vec<usize> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    let members = worn.iter().flatten().map(|w| (None, w))
        .chain(slots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| (Some(i), s))));
    for (at, item) in members {
        if !item.kind().replaceable() {
            continue;
        }
        let key = (item.kind(), if retention == Retention::KeepSpecialized { item.speciality() } else { Vec::new() });
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push((at, item)),
            None => groups.push((key, vec![(at, item)])),
        }
    }
    let keep_n = if retention == Retention::BestPlusBackup { 2 } else { 1 };
    let mut out = Vec::new();
    for (_, mut members) in groups {
        // Best first; ties: what you wear, then the lower slot.
        members.sort_by(|a, b| {
            score(b.1, legacy).total_cmp(&score(a.1, legacy))
                .then_with(|| a.0.map_or(-1, |i| i as i64).cmp(&b.0.map_or(-1, |i| i as i64)))
        });
        let kept: Vec<&Item> = members.iter().take(keep_n).map(|m| m.1).collect();
        for &(at, item) in members.iter().skip(keep_n) {
            let Some(i) = at else { continue };
            if keep.get(i).copied().unwrap_or(true) || never_drop(item) {
                continue;
            }
            if kept.iter().any(|k| dominated(item, k)) {
                out.push(i);
            }
        }
    }
    out.sort_unstable();
    out
}

// Items no automatic cleanup drops, whatever lists and limits say: containers, named items and elytras.
pub(super) fn never_drop(item: &Item) -> bool {
    item.container() || item.named || item.kind() == Kind::Elytra
}

// ---- the hotbar

// One hotbar slot's wish.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Want {
    Kind(Kind),
    // A building block (none that falls, none too valuable), the biggest stack.
    Blocks,
    // Food that is no golden apple, the most filling.
    Food,
    // Golden apples, enchanted ones first.
    GoldenApple,
    // Potions, splash ones first.
    Potion,
    // One item by id.
    Named(String),
}

// The choices a Slot setting offers.
pub(super) const SLOT_CHOICES: &[&str] = &["None", "Sword", "Axe", "Pickaxe", "Shovel", "Mace", "Spear", "Trident", "Bow",
    "Crossbow", "Blocks", "Food", "Golden apple", "Pearl", "Potion", "Totem", "Crystal", "Obsidian", "Anchor", "Glowstone",
    "Water bucket", "Lava bucket", "Rockets", "Wind charge", "Cobweb", "Arrows"];

// A wish from a word: a Slot choice or a Hotbar layout word (an item id works too). None for an empty word or None.
pub(super) fn want(word: &str) -> Option<Want> {
    let w: String = word.trim().to_ascii_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    let w = w.strip_prefix("minecraft:").unwrap_or(&w).to_owned();
    Some(match w.as_str() {
        "" | "none" => return None,
        "blocks" => Want::Blocks,
        "food" => Want::Food,
        "goldenapple" | "goldenapples" | "gapple" => Want::GoldenApple,
        "potion" | "potions" => Want::Potion,
        "pearl" | "pearls" => Want::Named("ender_pearl".into()),
        "totem" => Want::Named("totem_of_undying".into()),
        "crystal" | "crystals" => Want::Named("end_crystal".into()),
        "anchor" => Want::Named("respawn_anchor".into()),
        "waterbucket" | "water" => Want::Named("water_bucket".into()),
        "lavabucket" | "lava" => Want::Named("lava_bucket".into()),
        "rockets" | "rocket" => Want::Named("firework_rocket".into()),
        "windcharge" => Want::Named("wind_charge".into()),
        "arrows" => Want::Named("arrow".into()),
        "sword" | "axe" | "pickaxe" | "shovel" | "hoe" | "spear" => Want::Kind(Kind::of(&format!("x_{w}"))),
        "mace" | "trident" | "bow" | "crossbow" => Want::Kind(Kind::of(&w)),
        other => Want::Named(other.to_owned()),
    })
}

// The nine wishes: each Slot setting, or where it says None, that slot's word in the Hotbar layout text (how layouts
// were written before the Slot settings, and a way to name any item id).
pub(super) fn wants(slots: [&str; 9], layout_text: &str) -> [Option<Want>; 9] {
    let words: Vec<&str> = layout_text.split(',').collect();
    std::array::from_fn(|i| want(slots[i]).or_else(|| words.get(i).and_then(|w| want(w))))
}

fn fits(want: &Want, item: &Item) -> bool {
    match want {
        Want::Kind(k) => item.kind() == *k,
        Want::Blocks => item.block_use() == Some(BlockUse::Building),
        Want::Food => item.food > 0.0 && !item.name.ends_with("golden_apple"),
        Want::GoldenApple => item.name.ends_with("golden_apple"),
        Want::Potion => item.name.ends_with("potion"),
        Want::Named(n) => item.name == *n,
    }
}

// How well an item answers a wish, the higher the better.
fn worth(want: &Want, item: &Item, legacy: bool) -> f32 {
    match want {
        Want::Kind(_) => score(item, legacy),
        Want::Food => item.food * 100.0 + item.count as f32,
        Want::GoldenApple => (if item.name == "enchanted_golden_apple" { 1000.0 } else { 0.0 }) + item.count as f32,
        Want::Potion => (if item.name == "splash_potion" { 1000.0 } else { 0.0 }) + item.count as f32,
        _ => item.count as f32,
    }
}

// A piece already in its slot stays unless another is this much better: no swapping for a hair of difference.
const STAY: f32 = 1.05;

type Pick = [Option<usize>; 9];

// The nine slots planned together: for each wish all candidates (what already sits there first when it is
// within STAY of the best), then the assignment (each item once) that fills the most slots, then uses the better
// candidates, then moves the fewest items. Slots without a wish, and locked slots or items, are left as they are.
// Ties go the same way every time.
pub(super) fn plan_hotbar(slots: &[Option<Item>], wants: &[Option<Want>; 9], locked: &[bool], legacy: bool) -> Pick {
    let is_locked = |i: usize| locked.get(i).copied().unwrap_or(true);
    let options: Vec<Vec<usize>> = (0..9)
        .map(|h| {
            let Some(want) = wants[h].as_ref().filter(|_| !is_locked(h)) else { return Vec::new() };
            let mut c: Vec<(usize, f32)> = (0..slots.len())
                .filter(|&i| !is_locked(i))
                .filter_map(|i| slots[i].as_ref().filter(|s| fits(want, s)).map(|s| (i, worth(want, s, legacy))))
                .collect();
            c.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            if let Some(best) = c.first().map(|c| c.1)
                && let Some(at) = c.iter().position(|&(i, w)| i == h && w * STAY >= best)
            {
                let here = c.remove(at);
                c.insert(0, here);
            }
            c.into_iter().map(|(i, _)| i).collect()
        })
        .collect();
    // Assignment over nine destination bits, visiting each source once. This considers every item without the
    // exponential search (or a candidate cutoff that leaves repeated roles unfilled).
    let mut assignments = vec![None; 1 << 9];
    assignments[0] = Some(((0, 0), [None; 9]));
    for source in 0..slots.len() {
        let ranks: [Option<usize>; 9] = std::array::from_fn(|h| options[h].iter().position(|&i| i == source));
        for mask in (0usize..1 << 9).rev() {
            let Some((score, pick)) = assignments[mask] else { continue };
            for h in 0..9 {
                let Some(rank) = ranks[h].filter(|_| mask & (1 << h) == 0) else { continue };
                let next = mask | (1 << h);
                let score = (score.0 - rank as i32, score.1 - i32::from(source != h));
                let mut pick = pick;
                pick[h] = Some(source);
                if assignments[next].is_none_or(|(old, old_pick)| score > old || (score == old && pick < old_pick)) {
                    assignments[next] = Some((score, pick));
                }
            }
        }
    }
    let mut best = ((0, i32::MIN, i32::MIN), [None; 9]);
    for (mask, assignment) in assignments.into_iter().enumerate() {
        if let Some(((rank, moves), pick)) = assignment {
            let score = (mask.count_ones(), rank, moves);
            if score > best.0 { best = (score, pick); }
        }
    }
    best.1
}

// The next hotbar-key swap toward the plan: (the slot to take from, the hotbar slot it goes to); none when every planned
// item is in place. An item put in place is not moved by the next steps (no other slot plans it, and a slot in place is
// never a swap's source), so the steps end, nine at most.
#[cfg(test)]
pub(super) fn hotbar_step(slots: &[Option<Item>], wants: &[Option<Want>; 9], locked: &[bool], legacy: bool) -> Option<(usize, usize)> {
    let plan = plan_hotbar(slots, wants, locked, legacy);
    (0..9).find_map(|h| plan[h].filter(|&from| from != h).map(|from| (from, h)))
}

// ---- merging and sorting

// Limits trim whole stacks, without throwing away the last stack needed to meet a nonzero quota. The remaining
// amount can exceed a limit by part of one stack; avoid a burst of partial-drop clicks or touching the cursor.
// Protected/assigned stacks count toward the quota but are never selected for removal.
pub(super) fn quota_excess(slots: &[Option<Item>], keep: &[bool], limits: [u32; 4]) -> Vec<usize> {
    let class = |item: &Item| {
        if item.block { Some(0) }
        else if item.food > 0.0 { Some(1) }
        else if item.name.ends_with("arrow") { Some(2) }
        else if item.name == "ender_pearl" { Some(3) }
        else { None }
    };
    let mut out = Vec::new();
    for (category, limit) in limits.into_iter().enumerate() {
        let mut total: u64 = 0;
        let mut candidates = Vec::new();
        for (i, item) in slots.iter().enumerate() {
            if let Some(item) = item.as_ref().filter(|item| class(item) == Some(category)) {
                total += u64::from(item.count);
                if !keep.get(i).copied().unwrap_or(true) { candidates.push((i, item.count)); }
            }
        }
        // Main inventory first, smallest stacks first, stable ties from the end of the inventory.
        candidates.sort_by_key(|&(i, count)| (i < 9, count, std::cmp::Reverse(i)));
        for (i, count) in candidates {
            if total.saturating_sub(u64::from(count)) >= u64::from(limit) {
                total -= u64::from(count);
                out.push(i);
            }
        }
    }
    out
}

// Two part stacks of the same item and components: (from, into), a main-inventory stack into an earlier slot.
// Hotbar stacks are destinations only: draining one fights repeated layout wishes and the player's arrangement.
// As three clicks (pick up `from`, click `into`, click `from`) whatever does not fit goes back: nothing stays on the
// cursor, nothing is lost.
pub(super) fn merge_step(slots: &[Option<Item>], locked: &[bool]) -> Option<(usize, usize)> {
    let part = |i: usize| slots[i].as_ref().filter(|s| s.max_stack > 1 && s.count < s.max_stack && !locked.get(i).copied().unwrap_or(true));
    for into in 0..slots.len() {
        let Some(a) = part(into) else { continue };
        if let Some(from) = ((into + 1).max(9)..slots.len()).find(|&j| part(j).is_some_and(|b| b.name == a.name && b.meta == a.meta && b.damage == a.damage)) {
            return Some((from, into));
        }
    }
    None
}

// Sorting's order: weapons, tools, armour, blocks, food, potions, the rest; then by name, bigger stacks first.
fn rank(item: &Item) -> u8 {
    let kind = item.kind();
    match kind {
        Kind::Sword | Kind::Axe | Kind::Mace | Kind::Spear | Kind::Trident | Kind::Bow | Kind::Crossbow => 0,
        _ if kind.tool() => 1,
        _ if kind.armor() || kind == Kind::Elytra => 2,
        _ if item.block => 3,
        _ if item.food > 0.0 => 4,
        _ if item.name.ends_with("potion") => 5,
        _ => 6,
    }
}

fn key(item: &Option<Item>) -> (u8, &str) {
    item.as_ref().map_or((u8::MAX, ""), |s| (rank(s), s.name.as_str()))
}

// The next swap of two slots of `range` toward the sorted order: the first slot holding the wrong sort of item takes the
// next one that belongs there. Items of the same sort count as in place, so equal stacks are never shuffled.
pub(super) fn sort_step(slots: &[Option<Item>], range: std::ops::Range<usize>, locked: &[bool]) -> Option<(usize, usize)> {
    let free: Vec<usize> = range.filter(|&i| i < slots.len() && !locked.get(i).copied().unwrap_or(true)).collect();
    let mut wanted: Vec<&Option<Item>> = free.iter().map(|&i| &slots[i]).collect();
    wanted.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| {
        let count = |s: &Option<Item>| s.as_ref().map_or(0, |s| s.count);
        count(b).cmp(&count(a))
    }));
    for (at, &p) in free.iter().enumerate() {
        let target = key(wanted[at]);
        if key(&slots[p]) == target {
            continue;
        }
        let q = free[at + 1..].iter().copied().find(|&q| key(&slots[q]) == target)?;
        return Some((p, q));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vanilla's numbers for the items the fixtures use: attack damage bonus, attacks a second, armour points,
    // toughness, mining speed.
    fn item(name: &str) -> Item {
        let (attack, speed) = match name {
            "wooden_sword" => (3.0, 1.6),
            "iron_sword" => (5.0, 1.6),
            "diamond_sword" => (6.0, 1.6),
            "netherite_sword" => (7.0, 1.6),
            "diamond_axe" => (8.0, 1.0),
            "mace" => (5.0, 0.6),
            _ => (0.0, 4.0),
        };
        let mining = match name {
            "iron_pickaxe" => 6.0,
            "diamond_pickaxe" => 8.0,
            "netherite_pickaxe" => 9.0,
            "golden_pickaxe" => 12.0,
            "diamond_axe" => 8.0,
            _ => 1.0,
        };
        let (armor, toughness) = match name {
            "iron_chestplate" => (6.0, 0.0),
            "diamond_chestplate" => (8.0, 2.0),
            "netherite_chestplate" => (8.0, 3.0),
            _ => (0.0, 0.0),
        };
        let block = matches!(name, "stone" | "oak_planks" | "sand" | "obsidian" | "cobblestone");
        let food = match name {
            "bread" => 11.0,
            "golden_apple" | "enchanted_golden_apple" => 13.6,
            _ => 0.0,
        };
        let gear = attack > 0.0 || armor > 0.0 || mining > 1.0 || name == "elytra";
        let max_stack = if gear { 1 } else if name == "ender_pearl" { 16 } else { 64 };
        Item { name: name.into(), count: 1, max_stack, attack, speed, armor, toughness, mining, max_damage: 1500, block, food, ..Item::default() }
    }

    fn with(mut i: Item, e: &str, l: u32) -> Item {
        i.enchants.push((format!("minecraft:{e}"), l));
        i
    }

    fn n(mut i: Item, count: u32) -> Item {
        i.count = count;
        i
    }

    fn inv(items: &[(usize, Item)]) -> Vec<Option<Item>> {
        let mut v = vec![None; 36];
        for (i, it) in items {
            v[*i] = Some(it.clone());
        }
        v
    }

    fn names(slots: &[Option<Item>], range: std::ops::Range<usize>) -> Vec<String> {
        range.map(|i| slots[i].as_ref().map_or("-".to_owned(), |s| s.name.clone())).collect()
    }

    fn slots(words: [&str; 9]) -> [Option<Want>; 9] {
        wants(words, "")
    }

    // Runs the hotbar plan to its end, as the executor would, and counts the swaps.
    fn run_hotbar(inv: &mut [Option<Item>], wants: &[Option<Want>; 9], locked: &[bool]) -> usize {
        let mut steps = 0;
        while let Some((from, to)) = hotbar_step(inv, wants, locked, false) {
            inv.swap(from, to);
            steps += 1;
            assert!(steps <= 9, "a hotbar plan ends in at most nine swaps");
        }
        steps
    }

    #[test]
    fn sharpness_iron_against_plain_diamond_by_combat_model() {
        let iron = with(item("iron_sword"), "sharpness", 5);
        let diamond = item("diamond_sword");
        // Legacy: 1 + 5 + 6.25 = 12.25 a hit against 7. Modern: (1 + 5 + 3) × 1.6 = 14.4 against 11.2.
        assert!(weapon_score(&iron, true) > weapon_score(&diamond, true));
        assert!(weapon_score(&iron, false) > weapon_score(&diamond, false));
        // Modern: a diamond axe hits hardest (9 against 7) but the sword does more over time (11.2 against 9).
        let axe = item("diamond_axe");
        assert!(weapon_damage(&axe, false).hit > weapon_damage(&diamond, false).hit);
        assert!(weapon_damage(&axe, false).sustained < weapon_damage(&diamond, false).sustained);
    }

    #[test]
    fn protection_four_iron_beats_plain_diamond_by_vanilla_reduction() {
        let iron4 = with(item("iron_chestplate"), "protection", 4);
        let diamond = item("diamond_chestplate");
        let iron = item("iron_chestplate");
        // A set like it against 6, 12 and 20 damage: Protection IV iron 92.8%, 89.9%, 84.2%; plain diamond 80% each
        // (its 32 points cap at 20); plain iron 80%, 72%, 56%.
        assert!((armor_score(&iron4) - 0.8896).abs() < 0.002, "{}", armor_score(&iron4));
        assert!((armor_score(&diamond) - 0.80).abs() < 0.002, "{}", armor_score(&diamond));
        assert!((armor_score(&iron) - 0.6933).abs() < 0.002, "{}", armor_score(&iron));
        assert!(armor_score(&iron4) > armor_score(&diamond) && armor_score(&diamond) > armor_score(&iron));
    }

    #[test]
    fn fortune_and_silk_touch_pickaxes_both_stay() {
        let inv = inv(&[
            (0, with(item("diamond_pickaxe"), "fortune", 3)),
            (9, with(item("iron_pickaxe"), "silk_touch", 1)),
            (10, item("iron_pickaxe")),
            (11, with(item("diamond_pickaxe"), "efficiency", 5)),
        ]);
        let worn = Default::default();
        let keep = vec![false; 36];
        for retention in [Retention::BestOnly, Retention::BestPlusBackup, Retention::KeepSpecialized] {
            // The Efficiency V pickaxe is the best; Fortune and Silk Touch each do what it can't; the plain iron one
            // adds nothing.
            assert_eq!(excess(&inv, &worn, &keep, false, retention), [10], "{retention:?}");
        }
    }

    #[test]
    fn an_efficiency_lower_tier_tool_beats_a_higher_tier_plain_one() {
        assert!(tool_score(&with(item("iron_pickaxe"), "efficiency", 5)) > tool_score(&item("netherite_pickaxe")));
        assert!(tool_score(&item("golden_pickaxe")) > tool_score(&item("diamond_pickaxe")), "gold digs fastest, if briefly");
    }

    #[test]
    fn backups_follow_the_retention_policy() {
        let inv1 = inv(&[(9, item("diamond_sword")), (10, item("iron_sword")), (11, item("wooden_sword"))]);
        let worn = Default::default();
        let keep = vec![false; 36];
        assert_eq!(excess(&inv1, &worn, &keep, false, Retention::BestOnly), [10, 11]);
        assert_eq!(excess(&inv1, &worn, &keep, false, Retention::BestPlusBackup), [11], "the iron sword is the backup");
        // A sword with Fire Aspect the best lacks is not a plain duplicate, under any policy.
        let inv2 = inv(&[(9, item("diamond_sword")), (10, with(item("iron_sword"), "fire_aspect", 2))]);
        assert!(excess(&inv2, &worn, &keep, false, Retention::BestOnly).is_empty());
    }

    #[test]
    fn named_protected_unknown_and_special_items_are_never_excess() {
        let inv = inv(&[
            (9, item("netherite_sword")),
            (10, Item { named: true, ..item("iron_sword") }),
            (11, item("iron_sword")),
            (12, item("modded_widget")),
            (13, item("shulker_box")),
            (14, item("elytra")),
            (15, item("mace")),
            (16, item("mace")),
        ]);
        let worn = Default::default();
        let mut keep = vec![false; 36];
        keep[11] = true;
        assert!(excess(&inv, &worn, &keep, false, Retention::BestOnly).is_empty());
        assert_eq!(inv[12].as_ref().unwrap().kind(), Kind::Other, "an unknown item is not replaceable");
        assert!(never_drop(inv[13].as_ref().unwrap()) && never_drop(inv[14].as_ref().unwrap()) && never_drop(inv[10].as_ref().unwrap()));
    }

    #[test]
    fn worn_armour_is_the_one_to_beat() {
        let inv = inv(&[(9, item("iron_chestplate")), (10, item("diamond_chestplate"))]);
        let mut worn: [Option<Item>; 4] = Default::default();
        worn[1] = Some(with(item("diamond_chestplate"), "protection", 4));
        let keep = vec![false; 36];
        assert_eq!(excess(&inv, &worn, &keep, false, Retention::BestOnly), [9, 10]);
        assert_eq!(excess(&inv, &worn, &keep, false, Retention::BestPlusBackup), [9], "the diamond one is the backup");
    }

    #[test]
    fn an_already_right_hotbar_takes_no_step() {
        let mut inv1 = inv(&[(0, item("diamond_sword")), (1, item("diamond_pickaxe")), (2, n(item("stone"), 64)), (3, n(item("golden_apple"), 8))]);
        let w = slots(["Sword", "Pickaxe", "Blocks", "Golden apple", "None", "None", "None", "None", "None"]);
        let keep = vec![false; 36];
        assert_eq!(run_hotbar(&mut inv1, &w, &keep), 0);
        // A barely better sword in the inventory does not displace the one in place (within STAY)...
        inv1[20] = Some(Item { attack: 6.2, ..item("diamond_sword") });
        assert_eq!(run_hotbar(&mut inv1, &w, &keep), 0);
        // ...a clearly better one does, in one swap.
        inv1[21] = Some(item("netherite_sword"));
        assert_eq!(run_hotbar(&mut inv1, &w, &keep), 1);
        assert_eq!(names(&inv1, 0..1), ["netherite_sword"]);
    }

    #[test]
    fn conflicting_wishes_are_planned_together() {
        // Two Blocks slots, a Pickaxe slot whose pickaxe sits in a Blocks slot, sand (falls) and obsidian (its own slot).
        let mut inv1 = inv(&[(0, item("diamond_pickaxe")), (5, n(item("stone"), 64)), (9, n(item("oak_planks"), 32)),
            (10, n(item("sand"), 64)), (11, n(item("obsidian"), 64)), (12, item("netherite_sword"))]);
        let w = slots(["Blocks", "Blocks", "Pickaxe", "Sword", "Obsidian", "None", "None", "None", "None"]);
        let keep = vec![false; 36];
        let steps = run_hotbar(&mut inv1, &w, &keep);
        assert_eq!(names(&inv1, 0..5), ["stone", "oak_planks", "diamond_pickaxe", "netherite_sword", "obsidian"]);
        assert_eq!(steps, 5);
        assert_eq!(hotbar_step(&inv1, &w, &keep, false), None, "and then nothing more to do");
    }

    #[test]
    fn the_hotbar_leaves_locked_slots_and_items_alone() {
        let w = slots(["Sword", "Sword", "None", "None", "None", "None", "None", "None", "None"]);
        let mut inv1 = inv(&[(12, item("diamond_sword")), (0, item("bread"))]);
        let mut locked = vec![false; 36];
        locked[0] = true;
        run_hotbar(&mut inv1, &w, &locked);
        assert_eq!(names(&inv1, 0..2), ["bread", "diamond_sword"]);
        let mut inv2 = inv(&[(12, item("diamond_sword"))]);
        let mut locked = vec![false; 36];
        locked[12] = true;
        assert_eq!(run_hotbar(&mut inv2, &w, &locked), 0);
    }

    #[test]
    fn crystal_pvp_hotbar_keeps_each_tool_of_the_trade() {
        let mut inv1 = inv(&[(9, item("netherite_sword")), (10, n(item("end_crystal"), 64)), (11, n(item("obsidian"), 64)),
            (12, n(item("respawn_anchor"), 16)), (13, n(item("glowstone"), 64)), (14, n(item("enchanted_golden_apple"), 8)),
            (15, n(item("golden_apple"), 32)), (16, n(item("ender_pearl"), 16)), (17, item("totem_of_undying")), (18, n(item("stone"), 64))]);
        let w = slots(["Sword", "Crystal", "Obsidian", "Anchor", "Glowstone", "Golden apple", "Pearl", "Totem", "Blocks"]);
        run_hotbar(&mut inv1, &w, &vec![false; 36]);
        assert_eq!(names(&inv1, 0..9), ["netherite_sword", "end_crystal", "obsidian", "respawn_anchor", "glowstone",
            "enchanted_golden_apple", "ender_pearl", "totem_of_undying", "stone"]);
    }

    #[test]
    fn the_layout_text_still_works_and_slot_settings_win() {
        let w = wants(["None", "Pickaxe", "None", "None", "None", "None", "None", "None", "None"], "sword,axe,,blocks");
        assert_eq!(w[0], Some(Want::Kind(Kind::Sword)));
        assert_eq!(w[1], Some(Want::Kind(Kind::Pickaxe)), "the slot setting over the text's axe");
        assert_eq!(w[2], None);
        assert_eq!(w[3], Some(Want::Blocks));
        assert_eq!(want("Water bucket"), Some(Want::Named("water_bucket".into())));
        for choice in SLOT_CHOICES.iter().skip(1) {
            assert!(want(choice).is_some(), "{choice}");
        }
    }

    #[test]
    fn repeated_roles_can_fill_all_nine_hotbar_slots() {
        let mut inventory = inv(&(9..18).map(|i| (i, n(item("stone"), 64))).collect::<Vec<_>>());
        let w = slots(["Blocks"; 9]);
        let locked = vec![false; 36];
        run_hotbar(&mut inventory, &w, &locked);
        assert!(inventory[..9].iter().all(Option::is_some));
        assert_eq!(hotbar_step(&inventory, &w, &locked, false), None);
    }

    #[test]
    fn quotas_keep_the_last_useful_stack_and_count_protected_stacks() {
        let inventory = inv(&[(0, n(item("stone"), 64)), (9, n(item("stone"), 64)), (10, n(item("stone"), 64)),
            (11, n(item("bread"), 8)), (12, n(item("arrow"), 64))]);
        let mut keep = vec![false; 36];
        keep[0] = true;
        assert_eq!(quota_excess(&inventory, &keep, [80, 4, 0, 16]), [10, 12]);
        // A protected stack already meets the quota: ordinary copies can go, but that stack cannot.
        assert_eq!(quota_excess(&inventory, &keep, [32, 4, 0, 16]), [10, 9, 12]);
    }

    #[test]
    fn merging_does_not_empty_another_hotbar_slot() {
        let inventory = inv(&[(0, n(item("stone"), 10)), (8, n(item("stone"), 20))]);
        assert_eq!(merge_step(&inventory, &vec![false; 36]), None,
            "merging hotbar stacks fights the layout and the player's own arrangement");
        let mut inventory = inventory;
        inventory[9] = Some(n(item("stone"), 5));
        assert_eq!(merge_step(&inventory, &vec![false; 36]), Some((9, 0)));
    }

    #[test]
    fn part_stacks_merge_only_with_their_like() {
        let mut inv1 = inv(&[(2, n(item("stone"), 40)), (9, n(item("stone"), 30)), (10, Item { meta: 7, ..n(item("stone"), 10) }),
            (11, n(item("ender_pearl"), 16))]);
        let locked = vec![false; 36];
        assert_eq!(merge_step(&inv1, &locked), Some((9, 2)), "the later stack into the hotbar's");
        // After it (64 and 6): the 6 has no plain stone to join; the stack with components stays apart.
        inv1[2] = Some(n(item("stone"), 64));
        inv1[9] = Some(n(item("stone"), 6));
        assert_eq!(merge_step(&inv1, &locked), None);
    }

    #[test]
    fn sorting_reaches_its_order_and_stays_there_over_many_ticks() {
        let mut inv1 = inv(&[(9, item("bread")), (11, item("diamond_sword")), (12, n(item("stone"), 3)), (14, n(item("stone"), 9)),
            (15, item("iron_pickaxe"))]);
        let locked = vec![false; 36];
        let mut steps = 0;
        for _ in 0..150 {
            if let Some((a, b)) = sort_step(&inv1, 9..36, &locked) {
                inv1.swap(a, b);
                steps += 1;
            }
        }
        assert!(steps <= 5, "{steps} swaps over 150 ticks: no loop");
        assert_eq!(names(&inv1, 9..15), ["diamond_sword", "iron_pickaxe", "stone", "stone", "bread", "-"]);
    }

    #[test]
    fn planning_a_full_inventory_is_cheap() {
        let kinds = ["diamond_sword", "iron_sword", "diamond_pickaxe", "iron_pickaxe", "diamond_chestplate", "iron_chestplate", "bread", "stone"];
        let full: Vec<Option<Item>> = (0..36).map(|i| Some(n(with(item(kinds[i % kinds.len()]), "unbreaking", (i % 3) as u32), 1 + i as u32))).collect();
        let worn: [Option<Item>; 4] = Default::default();
        let w = slots(["Sword", "Pickaxe", "Blocks", "Food", "Sword", "Blocks", "Food", "Pickaxe", "Blocks"]);
        let locked = vec![false; 36];
        let runs = 500;
        let started = std::time::Instant::now();
        for _ in 0..runs {
            std::hint::black_box(plan_hotbar(&full, &w, &locked, false));
            std::hint::black_box(excess(&full, &worn, &locked, false, Retention::BestPlusBackup));
            std::hint::black_box(merge_step(&full, &locked));
            std::hint::black_box(sort_step(&full, 9..36, &locked));
        }
        let each = started.elapsed() / runs;
        println!("inventory plan (hotbar search, retention, merge, sort; 36 full slots): {} us a plan", each.as_micros());
        assert!(each < std::time::Duration::from_millis(5), "{each:?}");
    }
}
