// Inventory Manager's planning, apart from the inventory it runs on (player.rs turns stacks into `Item`s and carries
// out one step at a time): how good a weapon, tool or armour piece is, which duplicates are safe to drop, and the next
// move toward the hotbar layout or the sorted inventory. Slots are Sunset's: 0 to 8 the hotbar, 9 to 35 the rest.
//
// Each planner returns the next single step from the inventory as it is now, so a step the server refuses, or a
// change from elsewhere, is simply planned around next time; no step is taken when the inventory already is as wanted.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    // Kinds of which one best piece is all you need; extra ones may go. Maces, spears, tridents, bows and crossbows
    // each do something of their own and are never dropped as duplicates.
    fn replaceable(self) -> bool {
        matches!(self, Self::Sword | Self::Axe | Self::Pickaxe | Self::Shovel | Self::Hoe | Self::Helmet | Self::Chestplate
            | Self::Leggings | Self::Boots)
    }

    fn armor(self) -> bool {
        matches!(self, Self::Helmet | Self::Chestplate | Self::Leggings | Self::Boots)
    }

    fn tool(self) -> bool {
        matches!(self, Self::Pickaxe | Self::Shovel | Self::Hoe)
    }

    // An armour kind's slot in `worn` (head first, as the game's armor list).
    fn worn_slot(self) -> Option<usize> {
        match self {
            Self::Helmet => Some(0),
            Self::Chestplate => Some(1),
            Self::Leggings => Some(2),
            Self::Boots => Some(3),
            _ => None,
        }
    }
}

// What the planners need of a stack. Numbers are the game's: `attack` and `speed` the item's own attack damage and
// attacks a second, `armor` and `toughness` its armour points, `mining` its best mining speed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Item {
    pub(super) name: String,
    pub(super) count: u32,
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
    pub(super) food: bool,
}

impl Item {
    pub(super) fn kind(&self) -> Kind {
        Kind::of(&self.name)
    }

    fn level(&self, enchant: &str) -> u32 {
        self.enchants.iter().find(|(e, _)| e.strip_prefix("minecraft:").unwrap_or(e) == enchant).map_or(0, |e| e.1)
    }

    // How much of its durability is left, 0 to 1 (1 for an item that does not wear).
    fn left(&self) -> f32 {
        if self.max_damage == 0 { 1.0 } else { 1.0 - (self.damage as f32 / self.max_damage as f32).clamp(0.0, 1.0) }
    }

    // A piece about to break is worth little, however good it was; unbreaking and mending make wear matter less.
    fn wear(&self) -> f32 {
        let left = self.left();
        let lasting = 1.0 + 0.1 * self.level("unbreaking") as f32 + if self.level("mending") > 0 { 0.2 } else { 0.0 };
        if left < 0.05 { 0.5 } else { (0.85 + 0.15 * left).min(1.0) * lasting }
    }
}

// A weapon's worth. Legacy (1.8): the damage of a hit decides (there is no cooldown to wait out), and Sharpness adds
// 1.25 a level. Modern (1.9+): damage over time at full charge, a hit's damage times attacks a second, and Sharpness
// adds 0.5 a level plus 0.5.
pub(super) fn weapon_score(item: &Item, legacy: bool) -> f32 {
    let sharpness = item.level("sharpness") as f32;
    let bonus = if legacy { 1.25 * sharpness } else if sharpness > 0.0 { 0.5 * sharpness + 0.5 } else { 0.0 };
    let hit = 1.0 + item.attack.max(0.0) + bonus + 0.5 * item.level("fire_aspect") as f32 + 0.2 * item.level("knockback") as f32;
    let per_second = if legacy { hit } else { hit * item.speed.clamp(0.1, 4.0) };
    per_second * item.wear()
}

// An armour piece's worth: its points and half its toughness, Protection (4% less damage a level) and the special
// protections a little, then wear.
pub(super) fn armor_score(item: &Item) -> f32 {
    let special = ["fire_protection", "blast_protection", "projectile_protection"].iter().map(|e| item.level(e)).sum::<u32>();
    (item.armor + item.toughness * 0.5 + 0.75 * item.level("protection") as f32 + 0.25 * special as f32) * item.wear()
}

// A tool's worth: its mining speed with Efficiency (level² + 1 when it is faster than a hand), then wear.
pub(super) fn tool_score(item: &Item) -> f32 {
    let efficiency = item.level("efficiency");
    let speed = item.mining + if item.mining > 1.0 && efficiency > 0 { (efficiency * efficiency + 1) as f32 } else { 0.0 };
    let extra = if item.level("silk_touch") > 0 || item.level("fortune") > 0 { 0.5 } else { 0.0 };
    (speed + extra) * item.wear()
}

pub(super) fn score(item: &Item, legacy: bool) -> f32 {
    let kind = item.kind();
    if kind.armor() {
        armor_score(item)
    } else if kind.tool() {
        tool_score(item)
    } else if kind == Kind::Axe {
        // An axe is a weapon and a tool: the better of the two, so a fast mining axe is not dropped for a sharp one.
        weapon_score(item, legacy).max(tool_score(item))
    } else {
        weapon_score(item, legacy)
    }
}

// `weaker` can go for `better` only when nothing about it is lost: no name of its own, and each of its enchantments
// on `better` too at the same level or higher. Anything else is kept (its worth is not plain).
fn dominated(weaker: &Item, better: &Item) -> bool {
    !weaker.named && weaker.enchants.iter().all(|(e, l)| better.enchants.iter().any(|(b, bl)| b == e && bl >= l))
}

// The duplicates that may be dropped: for each replaceable kind, every piece another (in the inventory or worn) beats
// in worth and dominates. `protected` slots are never chosen.
pub(super) fn duplicates(slots: &[Option<Item>], worn: &[Option<Item>; 4], protected: &[bool], legacy: bool) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, item) in slots.iter().enumerate() {
        let Some(item) = item else { continue };
        let kind = item.kind();
        if !kind.replaceable() || protected.get(i).copied().unwrap_or(true) {
            continue;
        }
        let mine = score(item, legacy);
        let others = slots.iter().enumerate().filter(|(j, _)| *j != i).filter_map(|(j, s)| s.as_ref().map(|s| (Some(j), s)));
        let worn = kind.worn_slot().and_then(|w| worn[w].as_ref()).map(|w| (None, w));
        let beaten = others.chain(worn).any(|(j, other)| {
            if other.kind() != kind {
                return false;
            }
            let theirs = score(other, legacy);
            // Equal worth: the lower slot stays, so exactly one of two equal pieces is kept.
            let better = theirs > mine + 1e-4 || ((theirs - mine).abs() <= 1e-4 && j.is_none_or(|j| j < i));
            better && dominated(item, other)
        });
        if beaten {
            out.push(i);
        }
    }
    out
}

// One hotbar slot's wish: a kind (its best piece), blocks (the biggest stack), food, a potion, or one item by name.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Want {
    Kind(Kind),
    Blocks,
    Food,
    Potion,
    Named(String),
}

// The Hotbar layout setting: nine comma-separated wishes, one a slot from the left, empty for a slot left alone.
// `sword`, `axe`, `pickaxe`, `shovel`, `hoe`, `mace`, `spear`, `trident`, `bow`, `crossbow`, `blocks`, `food`,
// `potion`, or an item's id (`golden_apple`, `ender_pearl`, `water_bucket`...).
pub(super) fn layout(text: &str) -> [Option<Want>; 9] {
    let mut out: [Option<Want>; 9] = Default::default();
    for (slot, word) in text.split(',').take(9).enumerate() {
        let w = word.trim().to_ascii_lowercase();
        let w = w.strip_prefix("minecraft:").unwrap_or(&w);
        out[slot] = match w {
            "" => None,
            "blocks" => Some(Want::Blocks),
            "food" => Some(Want::Food),
            "potion" | "potions" => Some(Want::Potion),
            "sword" | "axe" | "pickaxe" | "shovel" | "hoe" | "mace" | "spear" | "trident" | "bow" | "crossbow" => {
                let probe = match w {
                    "mace" | "trident" | "bow" | "crossbow" => w.to_owned(),
                    _ => format!("x_{w}"),
                };
                Some(Want::Kind(Kind::of(&probe)))
            }
            other => Some(Want::Named(other.to_owned())),
        };
    }
    out
}

fn fits(want: &Want, item: &Item) -> bool {
    match want {
        Want::Kind(k) => item.kind() == *k,
        Want::Blocks => item.block,
        Want::Food => item.food,
        Want::Potion => item.name.ends_with("potion"),
        Want::Named(n) => item.name == *n,
    }
}

// How well an item answers a wish, the higher the better.
fn worth(want: &Want, item: &Item, legacy: bool) -> f32 {
    match want {
        Want::Kind(_) => score(item, legacy),
        _ => item.count as f32,
    }
}

// The next hotbar-key swap toward the layout: (the slot to take from, the hotbar slot it goes to). Slots are filled
// left to right; a slot already holding as good a piece as any is left alone, and what one slot took is not taken
// for a later one, so the steps end. `locked` slots (Loadouts', protected ones) are neither taken from nor filled.
pub(super) fn hotbar_step(slots: &[Option<Item>], wants: &[Option<Want>; 9], locked: &[bool], legacy: bool) -> Option<(usize, usize)> {
    let mut taken = vec![false; slots.len()];
    let is_locked = |i: usize| locked.get(i).copied().unwrap_or(true);
    for (h, want) in wants.iter().enumerate() {
        let Some(want) = want else { continue };
        if is_locked(h) {
            continue;
        }
        let best = (0..slots.len())
            .filter(|&i| !taken[i] && !is_locked(i))
            .filter_map(|i| slots[i].as_ref().filter(|s| fits(want, s)).map(|s| (i, worth(want, s, legacy))))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        let Some((from, best_worth)) = best else { continue };
        let here = slots[h].as_ref().filter(|s| !taken[h] && fits(want, s)).map(|s| worth(want, s, legacy));
        if from == h || here.is_some_and(|w| w + 1e-4 >= best_worth) {
            taken[h] = true;
            continue;
        }
        return Some((from, h));
    }
    None
}

// Sorting's order: weapons, tools, armour, blocks, food, potions, the rest; then by name, bigger stacks first.
fn rank(item: &Item) -> u8 {
    let kind = item.kind();
    match kind {
        Kind::Sword | Kind::Axe | Kind::Mace | Kind::Spear | Kind::Trident | Kind::Bow | Kind::Crossbow => 0,
        _ if kind.tool() => 1,
        _ if kind.armor() => 2,
        _ if item.block => 3,
        _ if item.food => 4,
        _ if item.name.ends_with("potion") => 5,
        _ => 6,
    }
}

fn key(item: &Option<Item>) -> (u8, &str) {
    item.as_ref().map_or((u8::MAX, ""), |s| (rank(s), s.name.as_str()))
}

// The next swap of two slots of `range` (the main inventory) toward the sorted order: the first slot holding the
// wrong sort of item takes the next one that belongs there. Items of the same sort count as in place, so equal
// stacks are never shuffled among themselves. `locked` slots stay out of it.
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

    fn item(name: &str) -> Item {
        let (attack, speed) = match name {
            "diamond_sword" => (6.0, 1.6),
            "iron_sword" => (5.0, 1.6),
            "netherite_sword" => (7.0, 1.6),
            "diamond_axe" => (8.0, 1.0),
            _ => (0.0, 4.0),
        };
        let mining = match name {
            "iron_pickaxe" => 6.0,
            "diamond_pickaxe" => 8.0,
            "golden_pickaxe" => 12.0,
            _ => 1.0,
        };
        let armor = match name {
            "diamond_chestplate" => 8.0,
            "iron_chestplate" => 6.0,
            _ => 0.0,
        };
        Item { name: name.into(), count: 1, attack, speed, armor, mining, max_damage: 1000, ..Item::default() }
    }

    fn with(mut i: Item, e: &str, l: u32) -> Item {
        i.enchants.push((format!("minecraft:{e}"), l));
        i
    }

    #[test]
    fn weapons_go_by_the_combat_model() {
        let sword = item("diamond_sword");
        let axe = item("diamond_axe");
        // Modern: the sword's speed beats the axe's bigger hit; Legacy: the hit decides.
        assert!(weapon_score(&sword, false) > weapon_score(&axe, false));
        assert!(weapon_score(&axe, true) > weapon_score(&sword, true));
        // Sharpness V on iron beats a plain diamond sword in Legacy (1.25 a level).
        assert!(weapon_score(&with(item("iron_sword"), "sharpness", 5), true) > weapon_score(&sword, true));
        // A sword about to break is worth less than a worse one that is whole.
        let broken = Item { damage: 990, ..item("netherite_sword") };
        assert!(weapon_score(&broken, false) < weapon_score(&item("iron_sword"), false));
    }

    #[test]
    fn a_better_material_is_not_always_the_better_tool() {
        let gold = item("golden_pickaxe");
        let iron = with(item("iron_pickaxe"), "efficiency", 5);
        assert!(tool_score(&iron) > tool_score(&gold), "Efficiency V iron outdigs plain gold");
        assert!(armor_score(&with(item("iron_chestplate"), "protection", 4)) > armor_score(&item("diamond_chestplate")));
    }

    #[test]
    fn only_dominated_duplicates_go() {
        let slots = vec![
            Some(item("diamond_sword")),
            Some(item("iron_sword")),
            Some(with(item("iron_sword"), "fire_aspect", 2)),
            Some(Item { named: true, ..item("iron_sword") }),
            Some(item("mace")),
            Some(item("mace")),
            Some(item("diamond_sword")),
        ];
        let worn: [Option<Item>; 4] = Default::default();
        let out = duplicates(&slots, &worn, &[false; 7], false);
        // The plain iron sword and the second diamond one go; the enchanted one (Fire Aspect the diamond lacks), the
        // named one and the maces stay.
        assert_eq!(out, [1, 6]);
        assert!(duplicates(&slots, &worn, &[false, true, false, false, false, false, true], false).is_empty());
    }

    #[test]
    fn worn_armour_counts_as_the_one_to_beat() {
        let slots = vec![Some(item("iron_chestplate")), Some(item("diamond_chestplate"))];
        let mut worn: [Option<Item>; 4] = Default::default();
        worn[1] = Some(with(item("diamond_chestplate"), "protection", 4));
        assert_eq!(duplicates(&slots, &worn, &[false; 2], false), [0, 1]);
        // Better than what you wear: it stays (Auto Armor puts it on), and the worse one goes.
        worn[1] = Some(item("iron_chestplate"));
        assert_eq!(duplicates(&slots, &worn, &[false; 2], false), [0]);
    }

    #[test]
    fn the_hotbar_fills_left_to_right_and_then_stops() {
        let mut slots: Vec<Option<Item>> = vec![None; 36];
        slots[0] = Some(item("diamond_pickaxe"));
        slots[12] = Some(item("diamond_sword"));
        slots[20] = Some(item("iron_sword"));
        slots[25] = Some(Item { block: true, count: 64, ..item("stone") });
        let wants = layout("sword, pickaxe,, blocks");
        let locked = vec![false; 36];
        let mut steps = 0;
        while let Some((from, to)) = hotbar_step(&slots, &wants, &locked, false) {
            slots.swap(from, to);
            steps += 1;
            assert!(steps < 10, "it ends");
        }
        assert_eq!(slots[0].as_ref().unwrap().name, "diamond_sword");
        assert_eq!(slots[1].as_ref().unwrap().name, "diamond_pickaxe");
        assert!(slots[2].is_none());
        assert_eq!(slots[3].as_ref().unwrap().name, "stone");
        assert_eq!(steps, 3);
        // A locked slot is left as it is.
        let mut locked = vec![false; 36];
        locked[0] = true;
        let mut fresh: Vec<Option<Item>> = vec![None; 36];
        fresh[12] = Some(item("diamond_sword"));
        assert_eq!(hotbar_step(&fresh, &layout("sword"), &locked, false), None);
    }

    #[test]
    fn sorting_reaches_its_order_and_leaves_equal_items_be() {
        let mut slots: Vec<Option<Item>> = vec![None; 36];
        slots[9] = Some(Item { food: true, ..item("bread") });
        slots[11] = Some(item("diamond_sword"));
        slots[12] = Some(Item { block: true, ..item("stone") });
        slots[14] = Some(Item { block: true, ..item("stone") });
        slots[15] = Some(item("iron_pickaxe"));
        let locked = vec![false; 36];
        let mut steps = 0;
        while let Some((a, b)) = sort_step(&slots, 9..36, &locked) {
            slots.swap(a, b);
            steps += 1;
            assert!(steps < 20, "it ends");
        }
        let names: Vec<&str> = (9..15).map(|i| slots[i].as_ref().map_or("-", |s| s.name.as_str())).collect();
        assert_eq!(names, ["diamond_sword", "iron_pickaxe", "stone", "stone", "bread", "-"]);
        assert!(steps <= 5, "{steps}: one swap per slot put right, at most");
        assert_eq!(sort_step(&slots, 9..36, &locked), None);
    }
}
