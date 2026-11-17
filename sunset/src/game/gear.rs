// Gear ESP (Mob Gear ESP before 1.0): what mobs and players carry, as far as it can be seen.
//
// Mob gear: mobs wearing or holding what players lose (a listed item, a listed enchantment, a named piece) are boxed,
// the pieces that gave them away shown above them, and with Chat named once.
//
// Player gear: each player near you, rebuilt from what the server shows of them. Two rows above their name (above
// Nametags' tag when it draws one):
// - Now: what they wear and hold this moment (head, chest, legs, feet, main hand, off hand), as the server sent it:
//   known.
// - Carried: what they are known to have had and not seen to have lost. Seen held or worn before, picked up (the
//   server tells who picks an item up, and the item is known), or given away by use (a pearl, potion, snowball, egg,
//   wind charge, arrow or trident they threw or shot) of something never seen in their hands. An item seen used since
//   it was last seen (thrown, shot, eaten to the last, a totem popped) may be gone and is marked so (dimmed, "?").
//   Then how many of their 36 inventory slots nothing is known of.
// What breaks (the server's item break event) or dies with them is forgotten; so is anything not seen for Forget
// after. Nothing guessed is shown as known: counts are those last seen, and only the Now row is the present.
//
// The server sends another player's armour and main hand with their components (enchantments, damage, count) and their
// off hand by id only, so an off hand piece has no count or enchantments here.

use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_util::*;
use super::sunset_visual::{Look, draw_box};
use super::Game;
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::Clientbound as C;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(super) const NAME: &str = "Gear ESP";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Render, "Shows what mobs and players around you carry.")
            .bool("Mob gear", true)
            .text("Items", "netherite_*,diamond_sword,diamond_axe,diamond_pickaxe,diamond_shovel,diamond_hoe,mace,elytra,totem_of_undying,shulker_box,*_shulker_box,enchanted_golden_apple,ender_pearl")
            .text("Enchantments", "mending,frost_walker,soul_speed,swift_sneak,binding_curse,vanishing_curse")
            .bool("Named items", true)
            .bool("Chat", true)
            .bool("Mob items", true)
            .colour("Color", "Pink")
            .num("Fill opacity", 0.25, 0.0, 1.0, 0.05)
            .bool("Player gear", true)
            .bool("Carried", true)
            .bool("Unknown slots", true)
            .num("Player range", 48.0, 8.0, 256.0, 8.0)
            .num("Icon size", 12.0, 8.0, 16.0, 1.0)
            .num("Forget after (s)", 300.0, 30.0, 1800.0, 30.0),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Items" | "Enchantments" | "Named items" | "Chat" | "Mob items" | "Color" | "Fill opacity" => m.b("Mob gear"),
        "Carried" | "Player range" | "Icon size" | "Forget after (s)" => m.b("Player gear"),
        "Unknown slots" => m.b("Player gear") && m.b("Carried"),
        _ => true,
    }
}

// ---- Player gear's book: what is known of each player's things (pure, unit tested)

// The slots a player shows: main hand, off hand, head, chest, legs, feet.
pub(super) const SHOWN: usize = 6;
const OFF_HAND: usize = 1;
// Their inventory slots, besides the off hand and armour.
const INVENTORY: u32 = 36;
const MOST_ITEMS: usize = 24;
const MOST_PLAYERS: usize = 128;

// How an item came to be known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum How {
    Held,
    Worn,
    Picked,
    // Only from using it (a pearl thrown, an arrow shot): they had one.
    Used,
}

#[derive(Clone, Debug)]
struct Known {
    key: String,
    stack: ItemStack,
    // The count last seen (the main hand's and picked up ones); None when the server never said.
    count: Option<u32>,
    how: How,
    // Seen used since it was last seen whole: it may be gone.
    spent: bool,
    seen: u64,
}

#[derive(Debug, Default)]
struct Player {
    items: Vec<Known>,
    // What each shown slot had last tick, and the main hand's count then.
    last: [Option<String>; SHOWN],
    last_count: Option<u32>,
    using: bool,
    // An item they threw or shot from their hand since the last sighting.
    threw: Option<String>,
    seen: u64,
}

// One item of the Carried row.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Carried {
    pub(super) stack: ItemStack,
    pub(super) count: Option<u32>,
    // Seen whole and not used since; false: it may be gone, or only its use was seen.
    pub(super) likely: bool,
}

#[derive(Debug, Default)]
pub(super) struct Book {
    players: HashMap<String, Player>,
}

// What tells two pieces apart: the item, its enchantments and its name (not its wear or count).
pub(super) fn key(stack: &ItemStack) -> String {
    let mut k = stack.item.to_string();
    let mut enchants: Vec<(String, u32)> = stack.enchantments().iter().map(|(e, l)| (e.to_string(), *l)).collect();
    enchants.sort();
    for (e, l) in enchants {
        let _ = write!(k, "|{e}{l}");
    }
    if let Some(name) = stack.custom_name() {
        let _ = write!(k, "|{name}");
    }
    k
}

// Consumables whose last one, used, leaves the hand empty.
fn consumable(item: &str) -> bool {
    let id = short(item);
    id.ends_with("potion") || id.ends_with("apple") || id.ends_with("pearl") || id.ends_with("_bottle") || id.ends_with("charge")
        || matches!(id, "snowball" | "egg" | "blue_egg" | "brown_egg" | "firework_rocket" | "totem_of_undying" | "chorus_fruit" | "golden_carrot"
            | "cooked_beef" | "cooked_porkchop" | "bread" | "end_crystal")
}

impl Book {
    fn player(&mut self, name: &str, now: u64, forget: u64) -> Option<&mut Player> {
        if !self.players.contains_key(name) {
            if self.players.len() >= MOST_PLAYERS {
                self.players.retain(|_, p| now.saturating_sub(p.seen) < forget);
                if self.players.len() >= MOST_PLAYERS {
                    return None;
                }
            }
            self.players.insert(name.to_owned(), Player::default());
        }
        self.players.get_mut(name)
    }

    fn note(p: &mut Player, stack: &ItemStack, count: Option<u32>, how: How, spent: bool, now: u64, by_id: bool) {
        let k = key(stack);
        // An off hand piece comes by id only: it is the one of that item already known, if any.
        let found = p.items.iter().position(|i| i.key == k).or_else(|| by_id.then(|| p.items.iter().position(|i| i.stack.item == stack.item)).flatten());
        match found {
            Some(i) => {
                let item = &mut p.items[i];
                if !by_id || item.key == k {
                    item.stack = stack.clone();
                }
                item.count = count.or(item.count);
                // What shows or was picked up is whole again; use marks it spent.
                if how != How::Used {
                    item.how = how;
                }
                item.spent = spent;
                item.seen = now;
            }
            None => {
                if p.items.len() >= MOST_ITEMS
                    && let Some(oldest) = (0..p.items.len()).min_by_key(|&i| p.items[i].seen)
                {
                    p.items.remove(oldest);
                }
                p.items.push(Known { key: k, stack: stack.clone(), count, how, spent, seen: now });
            }
        }
    }

    // A tick's sighting of a player: what each shown slot holds (the main hand with its count when the server sent
    // one), and whether they are using an item. `thrown`: items they threw or shot this tick.
    pub(super) fn observe(&mut self, name: &str, now: u64, forget: u64, shown: &[Option<ItemStack>; SHOWN], main_count: Option<u32>, using: bool) {
        let Some(p) = self.player(name, now, forget) else { return };
        p.seen = now;
        p.items.retain(|i| now.saturating_sub(i.seen) < forget);
        // The main hand's last one, used (eaten, drunk, thrown): a consumable's stack is gone, a thrown trident may be.
        let main_key = shown[0].as_ref().map(key);
        let threw = p.threw.take();
        if let Some(last) = p.last[0].clone()
            && main_key.as_deref() != Some(last.as_str())
            && p.last_count == Some(1)
            && (p.using || threw.as_deref() == last.split('|').next())
        {
            if p.items.iter().any(|i| i.key == last && consumable(&i.stack.item)) {
                p.items.retain(|i| i.key != last);
            } else if let Some(i) = p.items.iter_mut().find(|i| i.key == last) {
                i.spent = true;
            }
        }
        for (slot, stack) in shown.iter().enumerate() {
            let Some(stack) = stack else { continue };
            let (how, count) = match slot {
                0 => (How::Held, main_count),
                OFF_HAND => (How::Held, None),
                _ => (How::Worn, Some(stack.count)),
            };
            Self::note(p, stack, count, how, false, now, slot == OFF_HAND);
        }
        p.last = std::array::from_fn(|i| shown[i].as_ref().map(key));
        p.last_count = main_count;
        p.using = using;
    }

    // They threw or shot `item`: they had one. Held as it went, the hand's count tells the rest; else what is known of
    // it may be gone now (and was not known whole if it was never seen).
    pub(super) fn used(&mut self, name: &str, now: u64, forget: u64, item: &str) {
        let Some(p) = self.player(name, now, forget) else { return };
        let held = |slot: usize| p.last[slot].as_deref().is_some_and(|k| k.split('|').next() == Some(item));
        if held(0) || held(OFF_HAND) {
            p.threw = Some(item.to_owned());
            return;
        }
        match p.items.iter_mut().find(|i| &*i.stack.item == item) {
            Some(i) => {
                i.spent = true;
                i.count = None;
                i.seen = now;
            }
            None => Self::note(p, &ItemStack::new(item, 1), None, How::Used, true, now, false),
        }
    }

    // They picked up `stack` (`amount` of it).
    pub(super) fn picked(&mut self, name: &str, now: u64, forget: u64, stack: &ItemStack, amount: u32) {
        let Some(p) = self.player(name, now, forget) else { return };
        let k = key(stack);
        let before = p.items.iter().find(|i| i.key == k).and_then(|i| i.count).unwrap_or(0);
        Self::note(p, stack, Some(before + amount.max(1)), How::Picked, false, now, false);
    }

    // A totem of theirs went off: that one is gone, whether they have another the off hand will tell.
    pub(super) fn popped(&mut self, name: &str, now: u64, forget: u64) {
        let Some(p) = self.player(name, now, forget) else { return };
        if let Some(i) = p.items.iter_mut().find(|i| i.stack.item.ends_with("totem_of_undying")) {
            i.spent = true;
            i.seen = now;
        } else {
            Self::note(p, &ItemStack::new("minecraft:totem_of_undying", 1), None, How::Used, true, now, false);
        }
    }

    // What a shown slot held broke.
    pub(super) fn broke(&mut self, name: &str, slot: usize) {
        if let Some(p) = self.players.get_mut(name)
            && let Some(k) = p.last.get(slot).cloned().flatten()
        {
            p.items.retain(|i| i.key != k);
        }
    }

    // They died: what they carried is not theirs any more.
    pub(super) fn died(&mut self, name: &str) {
        if let Some(p) = self.players.get_mut(name) {
            p.items.clear();
            p.last = Default::default();
            p.last_count = None;
        }
    }

    // The Carried row for a player now (what they show now left out), the likeliest and most telling first, and how
    // many inventory slots nothing is known of (the main hand's stack is in one).
    pub(super) fn carried(&self, name: &str, now: u64, forget: u64, shown: &[Option<ItemStack>; SHOWN]) -> (Vec<Carried>, u32) {
        let Some(p) = self.players.get(name) else {
            return (Vec::new(), INVENTORY - u32::from(shown[0].is_some()));
        };
        let showing: HashSet<String> = shown.iter().flatten().map(key).collect();
        // An off hand piece known by id only stands for the one of that item.
        let off_id = shown[OFF_HAND].as_ref().map(|s| s.item.clone());
        let mut out: Vec<(&Known, u8)> = p
            .items
            .iter()
            .filter(|i| now.saturating_sub(i.seen) < forget && !showing.contains(&i.key))
            .filter(|i| off_id.as_ref() != Some(&i.stack.item))
            .map(|i| (i, rank(&i.stack.item)))
            .collect();
        out.sort_by(|(a, ra), (b, rb)| (a.spent || a.how == How::Used).cmp(&(b.spent || b.how == How::Used)).then(ra.cmp(rb)).then(b.seen.cmp(&a.seen)));
        let held = u32::from(shown[0].is_some());
        let unknown = INVENTORY.saturating_sub(held + out.len() as u32);
        let row = out
            .into_iter()
            .map(|(i, _)| Carried { stack: i.stack.clone(), count: i.count, likely: !i.spent && i.how != How::Used })
            .collect();
        (row, unknown)
    }

    pub(super) fn clear(&mut self) {
        self.players.clear();
    }
}

// How telling an item is in a fight, most first: totems, golden apples, pearls, crystals and anchors, potions,
// weapons, armour, the rest.
fn rank(item: &str) -> u8 {
    let id = short(item);
    match id {
        "totem_of_undying" => 0,
        "enchanted_golden_apple" => 1,
        "golden_apple" => 2,
        "ender_pearl" | "wind_charge" => 3,
        "end_crystal" | "respawn_anchor" | "glowstone" | "obsidian" => 4,
        _ if id.ends_with("potion") => 5,
        _ if id.ends_with("_sword") || id.ends_with("_axe") || id.ends_with("_spear") || matches!(id, "mace" | "trident" | "bow" | "crossbow") => 6,
        _ if id.ends_with("_helmet") || id.ends_with("_chestplate") || id.ends_with("_leggings") || id.ends_with("_boots") || id == "elytra" => 7,
        _ => 8,
    }
}

// ---- in the game

#[derive(Default)]
struct Gs {
    book: Book,
    // Mobs giving themselves away, and the pieces that did; the ones already told in chat.
    mobs: Vec<(u32, Vec<ItemStack>)>,
    said: HashSet<u32>,
    // The item entities about, by id: a pickup's item is gone from the world by the time its packet is read.
    items: HashMap<u32, ItemStack>,
    world: (u64, &'static str),
}

static GS: Mutex<Option<Gs>> = Mutex::new(None);
// Player gear is on: the packets are read for it, and Forget after in ticks.
static WATCH: AtomicBool = AtomicBool::new(false);
static FORGET: AtomicU64 = AtomicU64::new(6000);

fn gs<R>(f: impl FnOnce(&mut Gs) -> R) -> R {
    f(GS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Gs::default))
}

fn forget(m: &Module) -> u64 {
    (m.n("Forget after (s)") * 20.0) as u64
}

fn is_player(mob: &horizon_client::mobs::Mob) -> bool {
    &*mob.kind.id == "minecraft:player"
}

fn own(game: &Game<'_>, id: u32) -> bool {
    game.remote.as_ref().is_some_and(|r| r.entity_id == id as i32)
}

// What a player shows: main hand (and its count when the server sent it with its stack), off hand, head to feet.
fn shown(mob: &horizon_client::mobs::Mob) -> ([Option<ItemStack>; SHOWN], Option<u32>) {
    let e = &mob.extra;
    let by_id = |id: &Option<std::sync::Arc<str>>| id.as_ref().map(|id| ItemStack::new(id, 1));
    let main_stack = e.main_hand_stack().cloned();
    let count = main_stack.as_ref().map(|s| s.count);
    let main = main_stack.or_else(|| by_id(&e.main_hand));
    let worn = |a: usize, id: &Option<std::sync::Arc<str>>| e.armor[a].clone().filter(|s| Some(&s.item) == id.as_ref()).or_else(|| by_id(id));
    ([main, by_id(&e.off_hand), worn(0, &e.head), worn(1, &e.chest), worn(2, &e.legs), worn(3, &e.feet)], count)
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let m = mods.iter().find(|m| m.name == NAME && m.on);
    // A new world or dimension starts over.
    let world = (world_hash(game).unwrap_or(0), game.dim.id());
    if gs(|g| g.world != world) {
        gs(|g| *g = Gs { world, ..Gs::default() });
    }
    let Some(m) = m else {
        WATCH.store(false, Ordering::Relaxed);
        gs(|g| {
            g.mobs.clear();
            g.said.clear();
            g.items.clear();
        });
        return;
    };
    mob_gear(game, m);
    let watch = m.b("Player gear");
    WATCH.store(watch, Ordering::Relaxed);
    FORGET.store(forget(m), Ordering::Relaxed);
    if !watch {
        gs(|g| {
            g.book.clear();
            g.items.clear();
        });
        return;
    }
    let (now, forget, range) = (game.ticks, forget(m), m.n("Player range"));
    let me = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]);
    let near = |at: [f64; 3]| me.is_some_and(|me| dist(me, at) <= range + 16.0);
    let seen: Vec<(String, [Option<ItemStack>; SHOWN], Option<u32>, bool)> = game
        .mobs
        .iter()
        .filter(|x| is_player(x) && x.alive() && !x.removed && !own(game, x.id) && near([x.position.x, x.position.y, x.position.z]))
        .filter_map(|x| {
            let name = game.remote_player_name(x.id)?.to_owned();
            let (shown, count) = shown(x);
            Some((name, shown, count, x.extra.using_item > 0))
        })
        .collect();
    // The item entities about, kept for pickups.
    let items: Vec<(u32, &ItemStack)> =
        game.dropped.iter().filter(|d| near([d.position.x, d.position.y, d.position.z])).map(|d| (d.id, &d.stack)).collect();
    gs(|g| {
        for (name, shown, count, using) in &seen {
            g.book.observe(name, now, forget, shown, *count, *using);
        }
        g.items.retain(|id, _| items.iter().any(|(i, _)| i == id));
        for (id, stack) in items {
            g.items.entry(id).or_insert_with(|| stack.clone());
        }
    });
}

// The thrown and shot things that give an item away, by entity type.
fn thrown_item(kind: &str) -> Option<&'static str> {
    Some(match short(kind) {
        "ender_pearl" => "minecraft:ender_pearl",
        "snowball" => "minecraft:snowball",
        "egg" => "minecraft:egg",
        "splash_potion" => "minecraft:splash_potion",
        "lingering_potion" => "minecraft:lingering_potion",
        "experience_bottle" => "minecraft:experience_bottle",
        "wind_charge" => "minecraft:wind_charge",
        "trident" => "minecraft:trident",
        "arrow" => "minecraft:arrow",
        "spectral_arrow" => "minecraft:spectral_arrow",
        _ => return None,
    })
}

// The packets since the last tick, for Player gear: who threw or shot what, picked what up, popped a totem, broke a
// piece or died.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if !WATCH.load(Ordering::Relaxed) {
        return;
    }
    let (now, forget) = (game.ticks, FORGET.load(Ordering::Relaxed));
    let name = |id: i32| u32::try_from(id).ok().and_then(|id| game.remote_player_name(id)).map(str::to_owned);
    match packet {
        C::AddEntity(p) => {
            let kind = game.remote.as_ref().and_then(|r| r.builtin.get("minecraft:entity_type")).and_then(|t| t.name(p.kind));
            if let (Some(item), Some(owner)) = (kind.and_then(thrown_item), name(p.data)) {
                gs(|g| g.book.used(&owner, now, forget, item));
            }
        }
        C::TakeItemEntity(p) => {
            if let Some(who) = name(p.player)
                && let Ok(id) = u32::try_from(p.item)
                && let Some(stack) = gs(|g| g.items.get(&id).cloned())
            {
                gs(|g| g.book.picked(&who, now, forget, &stack, u32::try_from(p.amount).unwrap_or(1)));
            }
        }
        C::EntityEvent(p) => {
            let Some(who) = name(p.id) else { return };
            match p.event {
                35 => gs(|g| g.book.popped(&who, now, forget)),
                3 => gs(|g| g.book.died(&who)),
                // LivingEntity.entityEventForEquipmentBreak: main hand, off hand, head, chest, legs, feet.
                47..=52 => gs(|g| g.book.broke(&who, (p.event - 47) as usize)),
                _ => {}
            }
        }
        _ => {}
    }
}

// ---- Mob gear

// What on a mob gives it away: the listed items it wears or holds, a piece with a listed enchantment, or a named
// piece. The text for chat and the pieces.
fn mob_find(m: &Module, mob: &horizon_client::mobs::Mob) -> Option<(String, Vec<ItemStack>)> {
    let list = m.t("Items");
    let listed_item = |id: &str| {
        let id = short(id);
        list.split(',').map(str::trim).filter(|x| !x.is_empty()).any(|x| {
            x.strip_prefix('*').map_or(x == id, |end| id.ends_with(end)) || x.strip_suffix('*').is_some_and(|start| !start.is_empty() && id.starts_with(start))
        })
    };
    let (shown, _) = shown(mob);
    let mut pieces: Vec<ItemStack> = Vec::new();
    let mut said: Option<String> = None;
    for stack in shown.iter().flatten() {
        let label = short(&stack.item).replace('_', " ");
        let why = if listed_item(&stack.item) {
            Some(label)
        } else if let Some((name, _)) = stack.enchantments().iter().find(|(n, _)| listed(m.t("Enchantments"), n)) {
            Some(format!("{label} with {}", short(name).replace('_', " ")))
        } else if m.b("Named items") && let Some(name) = stack.custom_name() {
            Some(format!("{label} named {name}"))
        } else {
            None
        };
        if let Some(why) = why {
            said.get_or_insert(why);
            pieces.push(stack.clone());
        }
    }
    said.map(|s| (s, pieces))
}

fn mob_gear(game: &mut Game<'_>, m: &Module) {
    if !m.b("Mob gear") {
        gs(|g| {
            g.mobs.clear();
            g.said.clear();
        });
        return;
    }
    if game.ticks % 10 != 0 {
        return;
    }
    let found: Vec<(u32, String, Vec<ItemStack>, [f64; 3], String)> = game
        .mobs
        .iter()
        .filter(|x| x.alive() && !x.removed && !is_player(x))
        .filter_map(|x| mob_find(m, x).map(|(said, pieces)| (x.id, said, pieces, [x.position.x, x.position.y, x.position.z], short(&x.kind.id).replace('_', " "))))
        .collect();
    let fresh: Vec<(String, [f64; 3], String)> = gs(|g| {
        g.mobs = found.iter().map(|f| (f.0, f.2.clone())).collect();
        found.into_iter().filter(|f| g.said.insert(f.0)).map(|f| (f.1, f.3, f.4)).collect()
    });
    if m.b("Chat") {
        for (gear, p, kind) in fresh {
            report(game, &format!("A {kind} with {gear} at {:.0} {:.0} {:.0}", p[0], p[1], p[2]));
        }
    }
}

// ---- drawing

// A row of icons centred on `x`, its bottom at `bottom`; returns its top. Dimmed icons are drawn under a shade with a
// "?" (may be gone); counts over 1 in the corner, wear as a bar under damageable pieces.
fn row(game: &Game<'_>, out: &mut impl Canvas, icons: &[(&ItemStack, Option<u32>, bool)], tail: Option<&str>, x: f32, bottom: f32, size: f32) -> f32 {
    let gap = 1.0;
    let tail_w = tail.map_or(0.0, |t| game.native_text_width(t) * 0.6 + 2.0);
    let width = icons.len() as f32 * (size + gap) - gap + tail_w;
    let (left, top) = (x - width / 2.0, bottom - size - 2.0);
    out.fill([left - 2.0, top - 1.0, width + 4.0, size + 3.0], 0x7000_0000);
    for (i, (stack, count, sure)) in icons.iter().enumerate() {
        let at = [left + i as f32 * (size + gap), top];
        out.item(stack, at, size);
        if let Some(max) = game.res.items().get(&stack.item).map(|i| i.max_damage).filter(|m| *m > 0)
            && stack.damage > 0
        {
            let left_frac = 1.0 - (stack.damage as f32 / max as f32).min(1.0);
            let rgb = super::sunset_hud_bars::wear_rgb(left_frac);
            out.fill([at[0] + 1.0, at[1] + size, (size - 2.0) * left_frac, 1.0], 0xFF00_0000 | rgb);
        }
        if !sure {
            out.fill([at[0], at[1], size, size], 0x9000_0000);
            out.scaled("?", [at[0] + size - 4.0, at[1]], 0xFFFF55, 0.6);
        }
        if let Some(n) = count.filter(|n| *n > 1) {
            let t = n.to_string();
            let w = game.native_text_width(&t) * 0.6;
            out.scaled(&t, [at[0] + size - w, at[1] + size - 5.0], if *sure { 0xFFFFFF } else { 0xAAAAAA }, 0.6);
        }
    }
    if let Some(t) = tail {
        out.scaled(t, [left + width - tail_w + 2.0, top + size / 2.0 - 2.5], 0xAAAAAA, 0.6);
    }
    top - 1.0
}

// Boxes and pieces of Mob gear; the rows of Player gear above each player's name, or above Nametags' tag (`tags`: the
// players Nametags drew this frame, with the middle and top of their tag).
pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32, tags: &[(u32, [f32; 2])]) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else { return };
    let size = m.n("Icon size") as f32;
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let top_of = |mob: &horizon_client::mobs::Mob| {
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let off = [lerp(mob.previous.x, mob.position.x) - mob.position.x, lerp(mob.previous.y, mob.position.y) - mob.position.y,
            lerp(mob.previous.z, mob.position.z) - mob.position.z];
        ([lo.x + off[0], lo.y + off[1], lo.z + off[2]], [hi.x + off[0], hi.y + off[1], hi.z + off[2]])
    };
    let mob_items = m.b("Mob items");
    gs(|g| {
        if m.b("Mob gear") {
            let look = Look::of(m);
            for (id, pieces) in &g.mobs {
                let Some(mob) = game.mobs.iter().find(|x| x.id == *id) else { continue };
                let (lo, hi) = top_of(mob);
                draw_box(cam, out, &look, lo, hi, m.c("Color"), lw);
                let above = [(lo[0] + hi[0]) / 2.0, hi[1] + 0.3, (lo[2] + hi[2]) / 2.0];
                if mob_items
                    && cam.sees(above, 1.0)
                    && let Some([x, y]) = cam.project(above)
                {
                    let icons: Vec<(&ItemStack, Option<u32>, bool)> = pieces.iter().map(|s| (s, None, true)).collect();
                    row(game, out, &icons, None, x, y, size);
                }
            }
        }
        if !m.b("Player gear") {
            return;
        }
        let (now, forget, range) = (game.ticks, forget(m), m.n("Player range"));
        let (carried_on, unknown_on) = (m.b("Carried"), m.b("Unknown slots"));
        for mob in game.mobs.iter().filter(|x| is_player(x) && x.alive() && !x.removed && !own(game, x.id)) {
            let (lo, hi) = top_of(mob);
            let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
            if dist(mid, cam.pos()) > range {
                continue;
            }
            let Some(name) = game.remote_player_name(mob.id) else { continue };
            // Above Nametags' tag, else above the game's own name over the head.
            let anchor = match tags.iter().find(|(id, _)| *id == mob.id) {
                Some((_, at)) => Some(*at),
                None => {
                    let above = [mid[0], hi[1] + 0.75, mid[2]];
                    cam.sees(above, 1.0).then(|| cam.project(above)).flatten()
                }
            };
            let Some([x, mut bottom]) = anchor else { continue };
            let (now_row, _) = shown(mob);
            if carried_on {
                let (carried, unknown) = g.book.carried(name, now, forget, &now_row);
                let tail = (unknown_on && unknown > 0).then(|| format!("+{unknown}?"));
                if !carried.is_empty() || tail.is_some() {
                    let icons: Vec<(&ItemStack, Option<u32>, bool)> = carried.iter().map(|c| (&c.stack, c.count, c.likely)).collect();
                    bottom = row(game, out, &icons, tail.as_deref(), x, bottom, (size * 0.8).round());
                }
            }
            let main_count = mob.extra.main_hand_stack().map(|s| s.count);
            let order = [2, 3, 4, 5, 0, 1];
            let icons: Vec<(&ItemStack, Option<u32>, bool)> = order
                .iter()
                .filter_map(|&i| now_row[i].as_ref().map(|s| (s, if i == 0 { main_count } else { None }, true)))
                .collect();
            if !icons.is_empty() {
                row(game, out, &icons, None, x, bottom, size);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: u64 = 6000;

    fn st(id: &str, count: u32) -> Option<ItemStack> {
        Some(ItemStack::new(&format!("minecraft:{id}"), count))
    }

    fn hands(main: Option<ItemStack>, off: Option<ItemStack>, chest: Option<ItemStack>) -> [Option<ItemStack>; SHOWN] {
        [main, off, None, chest, None, None]
    }

    fn names(row: &[Carried]) -> Vec<(String, bool)> {
        row.iter().map(|c| (short(&c.stack.item).to_owned(), c.likely)).collect()
    }

    #[test]
    fn what_was_seen_is_carried_and_only_the_present_is_known() {
        let mut b = Book::default();
        let sword = hands(st("diamond_sword", 1), st("totem_of_undying", 1), st("netherite_chestplate", 1));
        b.observe("Rival", 100, F, &sword, Some(1), false);
        let pearls = hands(st("ender_pearl", 16), st("totem_of_undying", 1), st("elytra", 1));
        b.observe("Rival", 101, F, &pearls, Some(16), false);
        let (row, unknown) = b.carried("Rival", 102, F, &pearls);
        // The sword and the chestplate no longer show: carried, likely; the totem shows, so it is not in the row.
        assert_eq!(names(&row), [("diamond_sword".into(), true), ("netherite_chestplate".into(), true)]);
        assert_eq!(unknown, 36 - 1 - 2);
        // Back to the sword: the pearls are carried, 16 last seen.
        b.observe("Rival", 103, F, &sword, Some(1), false);
        let (row, _) = b.carried("Rival", 103, F, &sword);
        let pearl = row.iter().find(|c| c.stack.item.ends_with("ender_pearl")).unwrap();
        assert_eq!((pearl.count, pearl.likely), (Some(16), true));
        // Unknown players have nothing known but the slot in their hand.
        assert_eq!(b.carried("Nobody", 103, F, &sword), (Vec::new(), 35));
        // Forgotten after Forget after.
        assert!(b.carried("Rival", 103 + F, F, &sword).0.is_empty());
    }

    #[test]
    fn use_and_loss_make_items_uncertain_or_gone() {
        let mut b = Book::default();
        let sword = hands(st("diamond_sword", 1), None, None);
        b.observe("Rival", 1, F, &sword, Some(1), false);
        // A pearl thrown from the hand: the hand's count tells the rest. The last one thrown is gone.
        b.observe("Rival", 1, F, &hands(st("ender_pearl", 1), None, None), Some(1), false);
        b.used("Rival", 1, F, "minecraft:ender_pearl");
        b.observe("Rival", 1, F, &sword, Some(1), false);
        assert!(b.carried("Rival", 1, F, &sword).0.is_empty());
        // A pearl thrown, never seen held (a silent swap): they had one, which may be gone.
        b.used("Rival", 2, F, "minecraft:ender_pearl");
        let (row, _) = b.carried("Rival", 2, F, &sword);
        assert_eq!(names(&row), [("ender_pearl".into(), false)]);
        // A golden apple eaten to the last: gone.
        b.observe("Rival", 3, F, &hands(st("golden_apple", 1), None, None), Some(1), true);
        b.observe("Rival", 4, F, &sword, Some(1), false);
        assert!(!names(&b.carried("Rival", 4, F, &sword).0).iter().any(|(n, _)| n == "golden_apple"));
        // Switched away from a stack of them: carried.
        b.observe("Rival", 5, F, &hands(st("golden_apple", 5), None, None), Some(5), true);
        b.observe("Rival", 6, F, &sword, Some(1), false);
        assert!(names(&b.carried("Rival", 6, F, &sword).0).contains(&("golden_apple".into(), true)));
        // A totem popped with none shown after: may be gone. A piece that broke: gone.
        b.observe("Rival", 7, F, &hands(st("diamond_sword", 1), st("totem_of_undying", 1), st("diamond_chestplate", 1)), Some(1), false);
        b.popped("Rival", 8, F);
        b.observe("Rival", 8, F, &sword, Some(1), false);
        assert!(names(&b.carried("Rival", 8, F, &sword).0).contains(&("totem_of_undying".into(), false)));
        b.observe("Rival", 9, F, &hands(st("diamond_sword", 1), None, st("diamond_chestplate", 1)), Some(1), false);
        b.broke("Rival", 3);
        b.observe("Rival", 10, F, &sword, Some(1), false);
        assert!(!names(&b.carried("Rival", 10, F, &sword).0).iter().any(|(n, _)| n == "diamond_chestplate"));
        // A pickup is known with its count; death forgets everything.
        b.picked("Rival", 11, F, &ItemStack::new("minecraft:obsidian", 1), 12);
        let (row, _) = b.carried("Rival", 11, F, &sword);
        assert_eq!(row.iter().find(|c| c.stack.item.ends_with("obsidian")).and_then(|c| c.count), Some(12));
        b.died("Rival");
        assert!(b.carried("Rival", 12, F, &sword).0.is_empty());
    }

    #[test]
    fn enchanted_pieces_are_told_apart_and_the_book_stays_bounded() {
        let mut plain = ItemStack::new("minecraft:diamond_sword", 1);
        let mut sharp = plain.clone();
        sharp.enchant("minecraft:sharpness", 5);
        assert_ne!(key(&plain), key(&sharp));
        plain.damage = 100;
        assert_eq!(key(&plain), key(&ItemStack::new("minecraft:diamond_sword", 1)), "wear is not identity");
        let mut b = Book::default();
        for i in 0..60 {
            b.observe("Rival", i, F, &hands(st(&format!("item_{i}"), 1), None, None), Some(1), false);
        }
        assert!(b.players["Rival"].items.len() <= MOST_ITEMS);
        for i in 0..300 {
            b.observe(&format!("p{i}"), 50, F, &Default::default(), None, false);
        }
        assert!(b.players.len() <= MOST_PLAYERS);
        // The unknown count never goes below none.
        let (_, unknown) = b.carried("Rival", 60, F, &hands(st("stick", 1), None, None));
        assert!(unknown <= 36);
    }
}
