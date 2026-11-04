// More of Meteor's player and misc modules: Auto Fish, Auto Gap, Fast Use,
// Spam, and Anchor.

use super::sunset::{Category, Module, hold, look_at};
use super::sunset_util::*;
use super::{Game, Screen};
use std::sync::Mutex;
use horizon_protocol::packets::{Empty, play::{self, Serverbound as S}};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Auto Fish", Player, "Fishes for you.")
            .bool("Auto switch", true)
            .bool("Anti break", true)
            .bool("Auto cast", true)
            .num("Cast delay (ticks)", 14.0, 0.0, 60.0, 1.0)
            .num("Catch delay (ticks)", 6.0, 0.0, 60.0, 1.0),
        Module::new("Auto Gap", Player, "Eats golden apples.")
            .bool("Allow egap", true)
            .bool("Always", false)
            .bool("Potions regeneration", false)
            .bool("Potions fire resistance", true)
            .bool("Potions absorption", false)
            .bool("Health enabled", true)
            .num("Health threshold", 20.0, 1.0, 40.0, 1.0)
            .bool("Auto pot", false)
            .pick("Pot mode", &["Auto", "While holding"])
            .num("Pot health", 10.0, 1.0, 20.0, 0.5)
            .num("Pot pitch", 70.0, 50.0, 90.0, 1.0),
        Module::new("Chest Stealer", Player, "Empties containers you open.")
            .num("Items per tick", 3.0, 1.0, 27.0, 1.0)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0)
            .pick("Take", &["All", "Carts from shulkers"])
            .bool("Close when done", false)
            .text("Steal key", "None"),
        Module::new("Inventory Cleaner", Player, "Drops junk from your inventory.")
            .pick("Preset", &["Custom", "PvP", "Building", "Mining", "Survival"])
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Items", "rotten_flesh,spider_eye,poisonous_potato,gravel,cobblestone,netherrack,cobbled_deepslate,andesite,diorite,granite,tuff")
            .num("Blocks", 128.0, 0.0, 2304.0, 64.0)
            .num("Food", 64.0, 0.0, 576.0, 16.0)
            .num("Arrows", 64.0, 0.0, 576.0, 16.0)
            .num("Pearls", 16.0, 0.0, 64.0, 1.0)
            .text("Junk", PRESETS[0].5)
            .num("Delay (ticks)", 1.0, 0.0, 10.0, 1.0)
            .text("Loot key", "None")
            .num("Keep totems", 1.0, 1.0, 20.0, 1.0)
            .num("Keep pearl stacks", 5.0, 1.0, 10.0, 1.0),
        Module::new("Fast Use", Player, "Uses items faster.")
            .pick("Mode", &["All", "Some"])
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Items", "ender_pearl,experience_bottle,snowball,egg")
            .bool("Blocks", false)
            .num("Cooldown (ticks)", 0.0, 0.0, 4.0, 1.0)
            .bool("Throwables", false)
            .pick("Throw mode", &["Delay", "Every tick", "Burst"])
            .num("Throw delay", 2.0, 2.0, 5.0, 1.0)
            .num("Burst", 3.0, 1.0, 10.0, 1.0)
            .bool("Experience bottles", true)
            .bool("Ender pearls", true)
            .bool("Wind charges", true)
            .bool("Snowballs and eggs", false)
            .bool("Splash potions", false)
            .bool("Off hand", true),
        Module::new("Spam", Player, "Spams chat messages.")
            .text("Messages", "Sunset on top!")
            .num("Delay (ticks)", 20.0, 1.0, 400.0, 1.0)
            .bool("Randomise", false)
            .bool("Bypass", false)
            .num("Bypass length", 16.0, 4.0, 32.0, 1.0)
            .bool("Once", false),
        Module::new("Reach", Combat, "Longer reach.")
            .num("Extra block reach", 1.0, 0.0, 6.0, 0.5)
            .num("Entity reach", 4.5, 3.0, 6.0, 0.1),
        Module::new("Hitboxes", Combat, "Makes mobs' boxes bigger to aim at.")
            .text("Entities", "@player")
            .num("Expand", 0.5, 0.0, 2.0, 0.05)
            .bool("Ignore friends", true)
            .bool("Only on weapon", false)
            .text("Weapons", "sword,axe,pickaxe,shovel,hoe,mace,spear,trident"),
        Module::new("Ghost Hand", Player, "Uses containers through blocks.")
            .bool("Blocks", true)
            .bool("Hits", false)
            .bool("Cut through", false),
        Module::new("Air Place", Player, "Places blocks in the air.")
            .num("Range", 5.0, 2.0, 6.0, 0.5),
        Module::new("Speed Mine", Player, "Mines faster.")
            .pick("Mode", &["Damage", "Normal", "Haste"])
            .text("Blocks", "")
            .pick("Blocks filter", &["Blacklist", "Whitelist"])
            .num("Modifier", 1.4, 0.0, 10.0, 0.1)
            .num("Haste amplifier", 2.0, 1.0, 10.0, 1.0)
            .bool("Instamine", true),
        Module::new("No Break Reset", Player, "Keeps break progress on item swap.")
            .bool("Only to better tools", false),
    ]
}

#[derive(Default)]
struct Ps {
    fish_state: u8,
    cast_left: u32,
    catch_left: u32,
    gap_eating: bool,
    gap_prev: usize,
    spam_timer: u32,
    spam_index: usize,
    rng: u64,
    afk_jump: bool,
    afk_sneak: u32,
    afk_strafe: i32,
    afk_right: bool,
    afk_key: Option<KeyCode>,
    afk_yaw: Option<f32>,
    afk_message: i32,
    afk_index: usize,
    afk_wander: Wander,
    steal_wait: u32,
    steal_id: i32,
    steal_last: usize,
    steal_same: u32,
    steal_done: i32,
    clean_wait: u32,
    // Inventory Cleaner's Preset as last seen.
    clean_preset: String,
    // No Break Reset: the step the block's break took last tick, with the item held then.
    break_step: f32,
    // No Fall's Place: what it put down to land in, the item, the ticks since, the ticks you have stood still, and
    // whether the server's answer (the water there) has come.
    fall_placed: Option<([i32; 3], &'static str, u32, u32, bool)>,
    // Fast Place: the share of a block earned so far, in twentieths.
    place_credit: u32,
}

static PS: Mutex<Option<Ps>> = Mutex::new(None);

fn ps<R>(f: impl FnOnce(&mut Ps) -> R) -> R {
    let mut g = PS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Ps::default))
}

fn rand(n: usize) -> usize {
    ps(|p| {
        p.rng = p.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + n as u64);
        ((p.rng >> 33) as usize) % n.max(1)
    })
}

// Reach's extra, read by the reach hook (f64 bits; None when off).
static REACH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

pub(super) fn reach_extra() -> Option<f64> {
    let bits = REACH.load(std::sync::atomic::Ordering::Relaxed);
    (bits != u64::MAX).then(|| f64::from_bits(bits))
}

// How far the crosshair picks mobs (f64 bits; u64::MAX: the game's own), and Ghost Hand's Hits (1: through anything,
// 2: Cut through, only through blocks with nothing to collide with), read by the mob_hit and before_block hooks.
static ENTITY_REACH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static WALL_HITS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

// Hitboxes: how far the boxes of the mobs it picked this tick grow each way, and their ids (read by the mob_hit hook).
static HITBOXES: Mutex<(f64, Vec<u32>)> = Mutex::new((0.0, Vec::new()));
// Mine Through Mobs, at work this tick: the mobs it leaves to be hit as usual.
static NO_TRACE: Mutex<Option<String>> = Mutex::new(None);

// The mob under the crosshair with Reach's Entity reach or Auto Weapon's Spear reach, as the game's own pick finds it
// (the nearest box along the look), the boxes Hitboxes grows grown; none with Mine Through Mobs while a block is in
// reach (but for the mobs it lists). None: the game's own.
pub(super) fn mob_hit(game: &Game<'_>) -> Option<Option<(usize, f64)>> {
    let bits = ENTITY_REACH.load(std::sync::atomic::Ordering::Relaxed);
    let (grow, grown) = HITBOXES.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let trace = NO_TRACE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if bits == u64::MAX && grown.is_empty() && trace.is_none() {
        return None;
    }
    let hit = pick_mob(game, bits, grow, &grown);
    match (hit, trace) {
        (Some((i, _)), Some(kept)) if game.pick().is_some() && !listed(&kept, &game.mobs[i].kind.id) => Some(None),
        _ => Some(hit),
    }
}

fn pick_mob(game: &Game<'_>, bits: u64, grow: f64, grown: &[u32]) -> Option<(usize, f64)> {
    let ordinary = if game.creative { 5.0 } else { 3.0 };
    let reach = if bits == u64::MAX { ordinary } else { f64::from_bits(bits).max(ordinary) };
    let (eye, dir) = game.aim()?;
    let boxed = |m: &horizon_client::mobs::Mob| {
        let b = m.bounds();
        if grown.contains(&m.id) { b.inflate(horizon_core::geometry::Vec3::new(grow, grow, grow)) } else { b }
    };
    game.mobs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.alive() && Some(m.id) != game.riding)
        .filter_map(|(i, m)| Some((i, super::ray_box(eye, dir, reach, boxed(m))?)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

// Hitboxes (Meteor's), for the crosshair's pick: the mobs whose boxes grow by Expand, those the Entities list names
// (with @all, @player, @hostile, @passive), not friends with Ignore friends, and none unless what Weapons lists is
// in hand with Only on weapon.
fn hitboxes(game: &Game<'_>, m: Option<&Module>) -> (f64, Vec<u32>) {
    let Some(m) = m.filter(|m| m.n("Expand") > 0.0) else {
        return (0.0, Vec::new());
    };
    if m.b("Only on weapon") {
        let held = game.held().map_or("", |s| &*s.item);
        let tagged = |tag: &str| game.res.items().item_tags.get(tag).is_some_and(|items| items.contains(held));
        let kind = match short(held) {
            "mace" => "mace",
            "trident" => "trident",
            _ if horizon_client::spear::Kinetic::of(held).is_some() => "spear",
            _ => ["sword", "axe", "pickaxe", "shovel", "hoe"].into_iter().find(|k| tagged(&format!("minecraft:{k}s"))).unwrap_or(""),
        };
        if kind.is_empty() || !listed(m.t("Weapons"), kind) {
            return (0.0, Vec::new());
        }
    }
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let (friends, names) = (super::sunset::with(|s| s.friends.clone()), game.remote_player_names());
    let ids = game
        .mobs
        .iter()
        .filter(|mob| Some(mob.id) != own)
        .filter(|mob| {
            let player = mob.kind.id == "minecraft:player";
            let hostile = super::sunset_aura::is_hostile(&mob.kind.id);
            m.t("Entities").split(',').map(str::trim).any(|tok| match tok {
                "@all" => true,
                "@player" => player,
                "@hostile" => hostile,
                "@passive" => !player && !hostile,
                _ => !tok.is_empty() && short(tok) == short(&mob.kind.id),
            }) && !(player && m.b("Ignore friends") && super::sunset::is_friend(&friends, &names, mob.id))
        })
        .map(|mob| mob.id)
        .collect();
    (m.n("Expand"), ids)
}

// Ghost Hand's Hits: the mob under the crosshair counts as before any block. Cut through: only when no block between
// you and it has a collision box (grass, flowers, torches, cobwebs and the like are passed).
pub(super) fn hit_through_walls(game: &Game<'_>, distance: f64) -> bool {
    let mode = WALL_HITS.load(std::sync::atomic::Ordering::Relaxed);
    if mode == 0 || !game.mob_hit().is_some_and(|(_, d)| d == distance) {
        return false;
    }
    mode == 1
        || game.aim().is_some_and(|(eye, dir)| {
            let to = std::array::from_fn(|i| eye[i] + dir[i] * distance);
            super::sunset_combat::clip_clear(game, eye, to, distance)
        })
}

// What Ghost Hand opens or uses: containers, doors, trapdoors, gates, buttons, levers and work blocks.
fn usable(game: &Game<'_>, p: [i32; 3]) -> bool {
    let name = short(&facts(game, p).name);
    name != "iron_door"
        && name != "iron_trapdoor"
        && ["chest", "barrel", "shulker_box", "hopper", "dispenser", "dropper", "furnace", "smoker", "brewing_stand", "crafting_table",
            "crafter", "anvil", "enchanting_table", "beacon", "smithing_table", "loom", "stonecutter", "cartography_table",
            "grindstone", "button", "lever", "door", "trapdoor", "fence_gate"]
            .iter()
            .any(|k| name == *k || name.ends_with(&format!("_{k}")))
}

// Ghost Hand (Meteor's): your right click is not on a usable block, so the first one along the look within reach is
// used instead, through whatever is in between (servers check the distance only).
fn ghost_hand(game: &mut Game<'_>) {
    if !super::sunset::with(|s| s.module("Ghost Hand").is_some_and(|m| m.on && m.b("Blocks"))) {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    if game.pick().is_some_and(|h| usable(game, h.pos)) || game.mob_hit().is_some() {
        return;
    }
    let look = super::view_vector(game.yaw, game.pitch);
    let reach = game.reach();
    let mut last = None;
    for i in 1..(reach * 10.0) as i32 {
        let t = f64::from(i) / 10.0;
        let p = [0, 1, 2].map(|a| (eye[a] + look[a] * t).floor() as i32);
        if last == Some(p) {
            continue;
        }
        last = Some(p);
        if usable(game, p) {
            let sequence = sequence(game);
            game.send(S::UseItemOn(play::UseItemOn { hand: 0, pos: p, face: 1, cursor: [0.5, 1.0, 0.5], inside: true, world_border: false, sequence }));
            game.swing_arm();
            return;
        }
    }
}

// Right mouse button down while playing: Ghost Hand, Air Place.
pub(super) fn right_click(game: &mut Game<'_>) {
    ghost_hand(game);
    let Some(m) = super::sunset::with(|s| s.module("Air Place").filter(|m| m.on).cloned()) else {
        return;
    };
    if !in_world(game) || game.pick().is_some() || game.entity_targeted() {
        return;
    }
    let held = game.held().is_some_and(|s| game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()));
    if !held {
        return;
    }
    let (Some(eye), yaw, pitch) = (game.eye_position(), f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians()) else {
        return;
    };
    let d = m.n("Range");
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let at = [
        (eye[0] + look[0] * d).floor() as i32,
        (eye[1] + look[1] * d).floor() as i32,
        (eye[2] + look[2] * d).floor() as i32,
    ];
    let slot = game.selected;
    place_block(game, at, slot, false, true);
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    REACH.store(
        get("Reach").map_or(u64::MAX, |m| m.n("Extra block reach").to_bits()),
        std::sync::atomic::Ordering::Relaxed,
    );
    let spear = super::sunset_melee::spear_reach_slot(game).map(|_| {
        let reach = horizon_client::spear::REACH;
        f64::from(if game.creative { reach.max_creative } else { reach.max })
    });
    let entity = match (get("Reach").map(|m| m.n("Entity reach")), spear) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    ENTITY_REACH.store(entity.map_or(u64::MAX, f64::to_bits), std::sync::atomic::Ordering::Relaxed);
    *HITBOXES.lock().unwrap_or_else(|e| e.into_inner()) = hitboxes(game, get("Hitboxes"));
    // Mine Through Mobs (Meteor's): with a pickaxe in either hand (or anything, with Only with a pickaxe off).
    let pickaxe = [game.held(), game.offhand.as_ref()].into_iter().flatten().any(|s| s.item.ends_with("_pickaxe"));
    *NO_TRACE.lock().unwrap_or_else(|e| e.into_inner()) =
        get("Mine Through Mobs").filter(|m| pickaxe || !m.b("Only with a pickaxe")).map(|m| m.t("Entities").to_owned());
    let hits = get("Ghost Hand").map_or(0, |m| if m.b("Hits") { 1 } else { 2 * u8::from(m.b("Cut through")) });
    WALL_HITS.store(hits, std::sync::atomic::Ordering::Relaxed);
    // These work on the inventory screen, which in_world leaves out.
    if game.remote.is_some() && game.player.is_some() {
        if let Some(m) = get("Chest Stealer") {
            auto_steal(game, m);
        }
        if let Some(m) = get("Inventory Cleaner") {
            inventory_cleaner(game, m);
        }
    }
    if !in_world(game) {
        ps(|p| p.place_credit = 0);
        return;
    }
    if let Some(m) = get("Auto Fish") {
        auto_fish(game, m);
    } else {
        ps(|p| p.fish_state = 0);
    }
    if let Some(m) = get("Auto Gap") {
        auto_gap(game, m);
    } else if ps(|p| std::mem::take(&mut p.gap_eating)) {
        let _ = game.play_mouse(MouseButton::Right, false);
    }
    if let Some(m) = get("Fast Place") {
        fast_place(game, m);
    } else {
        ps(|p| p.place_credit = 0);
    }
    if let Some(m) = get("Fast Use") {
        fast_use(game, m);
    }
    if let Some(m) = get("Anti AFK") {
        anti_afk(game, m);
    } else {
        ps(|p| {
            p.afk_yaw = None;
            p.afk_message = 0;
            p.afk_wander = Wander::default();
        });
    }
    if let Some(m) = get("Spam") {
        spam(game, m);
    } else {
        ps(|p| p.spam_timer = 0);
    }
    match get("No Fall") {
        Some(m) => no_fall(game, m),
        None => ps(|p| p.fall_placed = None),
    }
}

// Auto Gap is eating: Auto Eat waits.
pub(super) fn gap_eating() -> bool {
    ps(|p| p.gap_eating)
}

// ---- Auto Fish

fn rod_ok(game: &Game<'_>, slot: usize, anti_break: bool) -> Option<u32> {
    let s = game.inventory.get(slot)?.as_ref()?;
    if short(&s.item) != "fishing_rod" {
        return None;
    }
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    if anti_break && max > 0 && s.damage + 1 >= max {
        return None;
    }
    let lvl = |e: &str| s.enchantment_level(e);
    Some(lvl("minecraft:luck_of_the_sea") + lvl("minecraft:lure") + lvl("minecraft:mending") + lvl("minecraft:unbreaking"))
}

fn use_rod(game: &mut Game<'_>, m: &Module) -> bool {
    let anti = m.b("Anti break");
    let selected = game.selected;
    let mut best: Option<(usize, u32)> = rod_ok(game, selected, anti).map(|s| (selected, s));
    for i in 0..9 {
        if i == selected {
            continue;
        }
        if let Some(score) = rod_ok(game, i, anti) {
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
    }
    let Some((slot, _)) = best else {
        return false;
    };
    if slot != selected {
        if !m.b("Auto switch") {
            return false;
        }
        hold(game, slot);
    }
    let _ = game.play_mouse(MouseButton::Right, true);
    let _ = game.play_mouse(MouseButton::Right, false);
    ps(|p| p.cast_left = m.n("Cast delay (ticks)") as u32);
    true
}

fn auto_fish(game: &mut Game<'_>, m: &Module) {
    match super::fishing::sunset_fishing::state(game) {
        None => {
            // Idle: cast again after the delay.
            ps(|p| p.fish_state = 0);
            if !m.b("Auto cast") {
                return;
            }
            if ps(|p| {
                if p.cast_left > 0 {
                    p.cast_left -= 1;
                    true
                } else {
                    false
                }
            }) {
                return;
            }
            if use_rod(game, m) {
                ps(|p| p.fish_state = 1);
            }
        }
        Some((bobbing, biting, hooked)) => {
            let state = ps(|p| p.fish_state);
            match state {
                0 => ps(|p| p.fish_state = 1),
                1 => {
                    if hooked {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    } else if bobbing && biting {
                        ps(|p| {
                            p.catch_left = m.n("Catch delay (ticks)") as u32;
                            p.fish_state = 2;
                        });
                    }
                }
                _ => {
                    let wait = ps(|p| {
                        if p.catch_left > 0 {
                            p.catch_left -= 1;
                            true
                        } else {
                            false
                        }
                    });
                    if hooked || !wait {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    }
                }
            }
        }
    }
}

// ---- Auto Gap

fn gap_slot(game: &Game<'_>, m: &Module, need_egap: bool) -> Option<usize> {
    let plain = if need_egap { None } else { find_hotbar(game, |n| n == "golden_apple") };
    plain.or_else(|| if m.b("Allow egap") || need_egap { find_hotbar(game, |n| n == "enchanted_golden_apple") } else { None })
}

fn auto_gap(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let effects = &game.potions.effects;
    let missing = |name: &str| effects.amplifier(name).is_none();
    let mut need_egap = false;
    let mut should = m.b("Always");
    if !should && m.b("Potions regeneration") && missing("regeneration") {
        should = true;
    }
    if !should && m.b("Potions fire resistance") && missing("fire_resistance") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Potions absorption") && missing("absorption") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Health enabled") {
        let health = (p.health.health() + p.health.absorption()).round();
        should = f64::from(health) < m.n("Health threshold");
    }
    let eating = ps(|s| s.gap_eating);
    if eating {
        if should {
            match gap_slot(game, m, need_egap) {
                Some(slot) => {
                    hold(game, slot);
                    if game.using.is_none() {
                        let _ = game.play_mouse(MouseButton::Right, true);
                    }
                }
                None => stop_eating(game),
            }
        } else {
            stop_eating(game);
        }
    } else if should {
        if let Some(slot) = gap_slot(game, m, need_egap) {
            let prev = game.selected;
            ps(|s| {
                s.gap_prev = prev;
                s.gap_eating = true;
            });
            hold(game, slot);
            let _ = game.play_mouse(MouseButton::Right, true);
        }
    }
}

fn stop_eating(game: &mut Game<'_>) {
    let _ = game.play_mouse(MouseButton::Right, false);
    let prev = ps(|s| {
        s.gap_eating = false;
        s.gap_prev
    });
    hold(game, prev);
}

// ---- Fast Place

// Blocks a second at each speed. The game's own is one every 4 ticks (5 a second), and a tick places one at most (20).
const PLACE_RATES: [u32; 5] = [6, 8, 10, 14, 20];

fn holds_block(game: &Game<'_>) -> bool {
    [game.held(), game.offhand.as_ref()]
        .into_iter()
        .flatten()
        .any(|s| game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()))
}

fn place_ready(credit: &mut u32, speed: usize) -> bool {
    let rate = PLACE_RATES[speed.clamp(1, PLACE_RATES.len()) - 1];
    *credit += rate;
    if *credit >= 20 {
        *credit -= 20;
        true
    } else {
        false
    }
}

// Runs after the game counts its delay down and before it reads it: each tick earns a share of a block, and the delay
// opens when a whole one is earned. So speeds between two whole delays average out to their exact rate.
fn fast_place(game: &mut Game<'_>, m: &Module) {
    if !game.right || game.using.is_some() || !holds_block(game) {
        ps(|p| p.place_credit = 0);
        return;
    }
    let speed = m.n("Speed") as usize;
    let ready = ps(|p| place_ready(&mut p.place_credit, speed));
    game.use_delay = if ready { 0 } else { game.use_delay.max(1) };
}

// ---- Fast Use

fn fast_use(game: &mut Game<'_>, m: &Module) {
    // Blocks are Fast Place's while it is on, or its speed would not hold; throwables are Throwables' when it is on.
    if holds_block(game) && super::sunset::with(|s| s.is_on("Fast Place")) || super::sunset_combat_inventory::thrown_in_hand(game, m) {
        return;
    }
    let item = game.held().map(|s| short(&s.item).to_owned());
    let applies = m.p("Mode") == "All"
        || item.as_ref().is_some_and(|i| {
            passes(m, "Items", i)
                || m.b("Blocks") && game.res.items().get(&format!("minecraft:{i}")).is_some_and(|inf| inf.block.is_some())
        });
    if applies && game.use_delay > m.n("Cooldown (ticks)") as u32 {
        game.use_delay = m.n("Cooldown (ticks)") as u32;
    }
}

#[cfg(test)]
mod fast_place_tests {
    use super::place_ready;

    #[test]
    fn each_speed_places_its_configured_average_per_second() {
        let expected = [6, 8, 10, 14, 20];
        for (index, count) in expected.into_iter().enumerate() {
            let mut credit = 0;
            let placed = (0..20).filter(|_| place_ready(&mut credit, index + 1)).count();
            assert_eq!(placed, count);
            assert_eq!(credit, 0);
        }
    }

    #[test]
    fn speed_is_clamped_to_the_supported_range() {
        let mut low = 0;
        let low_count = (0..20).filter(|_| place_ready(&mut low, 0)).count();
        let mut high = 0;
        let high_count = (0..20).filter(|_| place_ready(&mut high, 99)).count();
        assert_eq!(low_count, 6);
        assert_eq!(high_count, 20);
    }
}

// ---- Chest Stealer and Inventory Cleaner

// How many slots a server's container menu has, for the ones that are just a grid of items.
fn container_size(menu: &str) -> Option<usize> {
    let name = menu.strip_prefix("minecraft:").unwrap_or(menu);
    match name {
        "shulker_box" | "generic_9x3" => Some(27),
        "hopper" => Some(5),
        "generic_3x3" => Some(9),
        _ => name.strip_prefix("generic_9x")?.parse::<usize>().ok().map(|rows| rows * 9),
    }
}

fn auto_steal(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::QuickMove;
    if game.screen != Screen::Inventory {
        // Closed: the next chest starts afresh.
        ps(|p| p.steal_done = i32::MIN);
        return;
    }
    // Carts from shulkers (a cart PvP restock): only TNT minecarts, only out of shulker boxes.
    let carts = m.p("Take") == "Carts from shulkers";
    let shulker = match game.container {
        Some(key) => super::sunset_util::facts(game, [key.0, key.1, key.2]).name.ends_with("shulker_box"),
        None => game.remote_menu().1.strip_prefix("minecraft:") == Some("shulker_box"),
    };
    if carts && !shulker {
        return;
    }
    // A chest of your own world, or the menu a server opened.
    let (id, size) = match game.container {
        Some(key) if matches!(game.container_kind(), Some(super::BlockEntity::Chest(_))) => {
            (key.0 ^ key.1.rotate_left(10) ^ key.2.rotate_left(20) | 1, game.container_size())
        }
        Some(_) => return,
        None => {
            let (id, menu) = game.remote_menu();
            match container_size(&menu) {
                Some(n) => (id, n),
                None => return,
            }
        }
    };
    // With a Steal key, only while it is held.
    let key = m.t("Steal key") != "None";
    if ps(|p| p.steal_done == id) || (key && !super::sunset_util::setting_key_down(game, m, "Steal key")) || ps(|p| {
        p.steal_wait = p.steal_wait.saturating_sub(1);
        p.steal_wait > 0
    }) {
        return;
    }
    let items: Vec<usize> =
        (0..size).filter(|&i| game.slot_mut(300 + i).as_ref().is_some_and(|s| !carts || &*s.item == "minecraft:tnt_minecart")).collect();
    // Done when there is nothing to take, or when taking stopped changing it (your inventory is full). Close when
    // done shuts the container then.
    let stuck = ps(|p| {
        p.steal_same = if p.steal_id == id && p.steal_last == items.len() { p.steal_same + 1 } else { 0 };
        (p.steal_id, p.steal_last) = (id, items.len());
        p.steal_same >= 6
    });
    if items.is_empty() || stuck {
        ps(|p| p.steal_done = id);
        if m.b("Close when done") {
            super::sunset_combat_inventory::close_inventory(game);
        }
        return;
    }
    let ops: Vec<_> = items.iter().take(m.n("Items per tick") as usize).map(|&i| (Some(300 + i), 0, QuickMove)).collect();
    game.menu_ops(&ops);
    ps(|p| p.steal_wait = m.n("Delay (ticks)") as u32);
}

// Inventory Cleaner's presets: name, then the blocks, food, arrows and pearls kept, and the junk.
const PRESETS: [(&str, f64, f64, f64, f64, &str); 4] = [
    ("PvP", 128.0, 64.0, 64.0, 16.0, "dirt,gravel,sand,rotten_flesh,spider_eye,poisonous_potato,wheat_seeds,bone,string,feather,flint,stick,egg"),
    ("Building", 1728.0, 32.0, 0.0, 16.0, "rotten_flesh,spider_eye,poisonous_potato,wheat_seeds,bone,string,feather,flint,egg"),
    ("Mining", 64.0, 64.0, 0.0, 0.0, "dirt,gravel,cobblestone,cobbled_deepslate,netherrack,andesite,diorite,granite,tuff,rotten_flesh,spider_eye,poisonous_potato,wheat_seeds"),
    ("Survival", 256.0, 128.0, 128.0, 16.0, "rotten_flesh,spider_eye,poisonous_potato"),
];

// A tool, weapon or armour piece: its kind (the word after the material) and how good its material is.
fn gear(item: &str) -> Option<(&str, u32)> {
    const KINDS: [&str; 13] = ["sword", "axe", "pickaxe", "shovel", "hoe", "spear", "helmet", "chestplate", "leggings", "boots", "bow", "crossbow", "mace"];
    let (material, kind) = item.rsplit_once('_').unwrap_or(("", item));
    if !KINDS.contains(&kind) || (material.is_empty() && !matches!(kind, "bow" | "crossbow" | "mace")) {
        return None;
    }
    let tier = match material {
        "netherite" => 7,
        "diamond" => 6,
        "iron" => 5,
        "copper" => 4,
        "stone" | "chainmail" | "turtle" => 3,
        "golden" => 2,
        _ => 1,
    };
    Some((kind, tier))
}

// The inventory slots (0-35) a preset drops: its junk; a tool, weapon or armour piece when a better one or the same
// one is kept (worn armour counts); building blocks, food, arrows and pearls past its limits.
fn junk_slots(game: &Game<'_>, m: &Module) -> Vec<usize> {
    let item = |i: usize| game.inventory.get(i).and_then(|s| s.as_ref());
    let mut out = Vec::new();
    // What you wear is the best so far of its kind, and never dropped.
    const WORN: usize = usize::MAX;
    let mut best: Vec<(&str, u32, usize)> =
        game.armor.iter().flatten().filter_map(|s| gear(short(&s.item)).map(|(kind, tier)| (kind, tier, WORN))).collect();
    for i in 0..36 {
        let Some((kind, tier)) = item(i).and_then(|s| gear(short(&s.item))) else {
            continue;
        };
        match best.iter_mut().find(|b| b.0 == kind) {
            Some(b) if tier > b.1 => {
                if b.2 != WORN {
                    out.push(b.2);
                }
                *b = (kind, tier, i);
            }
            Some(_) => out.push(i),
            None => best.push((kind, tier, i)),
        }
    }
    let mut kept = [0.0; 4];
    for i in 0..36 {
        let Some(stack) = item(i) else {
            continue;
        };
        let name = short(&stack.item);
        if listed(m.t("Junk"), name) {
            out.push(i);
            continue;
        }
        let info = game.res.items().get(&stack.item);
        let class = if info.as_ref().and_then(|i| i.block).is_some_and(|b| game.res.facts().get(b).full_collision) {
            0
        } else if info.as_ref().is_some_and(|i| i.food.is_some()) {
            1
        } else if name.ends_with("arrow") {
            2
        } else if name == "ender_pearl" {
            3
        } else {
            continue;
        };
        let limit = m.n(["Blocks", "Food", "Arrows", "Pearls"][class]);
        if kept[class] + stack.count as f64 > limit {
            out.push(i);
        } else {
            kept[class] += stack.count as f64;
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn inventory_cleaner(game: &mut Game<'_>, m: &Module) {
    // A preset just picked fills in its limits and junk (not on the first look, so a change you made stays).
    let preset = m.p("Preset");
    let before = ps(|p| std::mem::replace(&mut p.clean_preset, preset.to_owned()));
    if !before.is_empty() && before != preset
        && let Some(&(_, blocks, food, arrows, pearls, junk)) = PRESETS.iter().find(|p| p.0 == preset)
    {
        for (field, value) in [("blocks", blocks.to_string()), ("food", food.to_string()), ("arrows", arrows.to_string()),
            ("pearls", pearls.to_string()), ("junk", junk.to_owned())]
        {
            super::sunset::set_setting(game, &format!("inventorycleaner.{field}"), &value);
        }
        return;
    }

    use horizon_client::menu::Input::Throw;
    if game.screen != Screen::Inventory || game.remote_menu().1 != "inventory" || super::sunset_refill::loot(game, m) {
        return;
    }
    if ps(|p| {
        p.clean_wait = p.clean_wait.saturating_sub(1);
        p.clean_wait > 0
    }) {
        return;
    }
    // Never from a slot Loadouts locks.
    let locked = super::sunset_loadouts::locked_slots();
    let junk = if preset == "Custom" {
        (0..36).find(|&i| !locked[i] && game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| passes(m, "Items", short(&s.item))))
    } else {
        junk_slots(game, m).into_iter().find(|&i| !locked[i])
    };
    if let Some(i) = junk {
        game.menu_ops(&[(Some(i), 1, Throw)]);
        ps(|p| p.clean_wait = m.n("Delay (ticks)") as u32 + 1);
    }
}

// ---- Anti AFK (Meteor's AntiAFK)

// Wander, vape-rs's Anti-AFK (github.com/rdbtCVS/vape-rs, MIT): once none of your move keys has been held for a
// random Start delay, every few seconds at random (Frequency) a small visible turn at a random speed (at most Max yaw
// and Max pitch change) and a short walk, a tenth to a fifth of a second, toward a point within a block. Keep close
// walks back and turns back once it has drifted from where it started. Times in ticks.
#[derive(Default)]
struct Wander {
    idle_since: Option<u64>,
    delay: u64,
    next_walk: u64,
    next_turn: u64,
    walked_at: u64,
    turned_at: u64,
    // Where you were and how you looked when it began.
    origin: Option<([f64; 2], f32, f32)>,
    // The point it walks to, from when, for how long.
    walk: Option<([f64; 2], u64, u64)>,
    // The look it turns to and its speed, degrees a tick.
    turn: Option<(f32, f32, f32)>,
}

fn unit() -> f64 {
    rand(1_000_000) as f64 / 1e6
}

fn wrap180(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

// The move keys from `at` toward `to` with this yaw (vape-rs's keys_towards).
fn keys_towards(at: [f64; 2], yaw: f32, to: [f64; 2]) -> Vec<KeyCode> {
    let (dx, dz) = (to[0] - at[0], to[1] - at[1]);
    if dx.hypot(dz) < 0.1 {
        return Vec::new();
    }
    let rad = f64::from(yaw).to_radians();
    let forward = -rad.sin() * dx + rad.cos() * dz;
    let left = rad.cos() * dx + rad.sin() * dz;
    let edge = 0.3 * dx.hypot(dz);
    [(forward > edge, KeyCode::KeyW), (forward < -edge, KeyCode::KeyS), (left > edge, KeyCode::KeyA), (left < -edge, KeyCode::KeyD)]
        .into_iter()
        .filter_map(|(held, k)| held.then_some(k))
        .collect()
}

// One tick of Wander: the keys to hold, and the view turned a step.
fn wander(game: &mut Game<'_>, m: &Module) -> Vec<KeyCode> {
    let Some(p) = game.player.as_ref() else {
        return Vec::new();
    };
    let at = [p.position.x, p.position.z];
    let now = game.ticks;
    let yours = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD].iter().any(|k| game.physical_keys.contains(k));
    let mut w = ps(|p| std::mem::take(&mut p.afk_wander));
    if yours || w.idle_since.is_none() {
        let (lo, hi) = (m.n("Shortest start delay (s)"), m.n("Longest start delay (s)"));
        let delay = ((lo + unit() * (hi - lo).max(0.0)) * 20.0) as u64;
        w = Wander { idle_since: Some(now), delay, next_walk: delay, next_turn: delay, walked_at: now, turned_at: now, ..Wander::default() };
        ps(|p| p.afk_wander = w);
        return Vec::new();
    }
    let mut keys = Vec::new();
    if now.saturating_sub(w.idle_since.unwrap_or(now)) >= w.delay {
        // Up to a second plus the Frequency's interval between one and the next.
        let interval = 20 + (20.0 / m.n("Frequency").max(0.1)) as usize;
        let (yaw, pitch) = (game.yaw, game.pitch);
        if m.b("Turn") && in_world(game) && now.saturating_sub(w.turned_at) >= w.next_turn {
            w.origin.get_or_insert((at, yaw, pitch));
            w.next_turn = rand(interval) as u64;
            w.turned_at = now;
            let (most_yaw, most_pitch) = (m.n("Max yaw change"), m.n("Max pitch change"));
            let drifted = match (m.b("Keep close"), w.origin) {
                (true, Some((_, y, p))) => f64::from(wrap180(yaw - y).hypot(wrap180(pitch - p))) >= most_yaw.hypot(most_pitch),
                _ => false,
            };
            let mut dy = unit() * most_yaw;
            let mut dp = unit() * most_pitch;
            if unit() > 0.6 {
                dy = -dy;
            }
            if unit() > 0.5 {
                dp = -dp;
            }
            let to = match w.origin {
                Some((_, y, p)) if drifted => (y, p),
                _ => (yaw + dy as f32, (pitch + dp as f32).clamp(-90.0, 90.0)),
            };
            w.turn = Some((to.0, to.1, (unit() * 10.0).max(1.0) as f32));
        }
        if let Some((ty, tp, speed)) = w.turn {
            let (dy, dp) = (wrap180(ty - yaw), tp - pitch);
            game.yaw += dy.clamp(-speed, speed);
            game.pitch = (game.pitch + dp.clamp(-speed, speed)).clamp(-90.0, 90.0);
            if dy.abs() <= speed && dp.abs() <= speed {
                w.turn = None;
            }
        }
        match w.walk {
            None if now.saturating_sub(w.walked_at) >= w.next_walk => {
                w.origin.get_or_insert((at, yaw, pitch));
                w.next_walk = rand(interval) as u64;
                let mut to = [at[0] + unit() * 2.0 - 1.0, at[1] + unit() * 2.0 - 1.0];
                if let (true, Some((origin, _, _))) = (m.b("Keep close"), w.origin)
                    && (origin[0] - at[0]).hypot(origin[1] - at[1]) >= 0.75
                {
                    to = origin;
                }
                w.walk = Some((to, now, 2 + rand(3) as u64));
                w.walked_at = now;
            }
            Some((_, since, ticks)) if now.saturating_sub(since) >= ticks => w.walk = None,
            _ => {}
        }
        if let Some((to, _, _)) = w.walk {
            keys = keys_towards(at, game.yaw, to);
        }
    }
    ps(|p| p.afk_wander = w);
    keys
}

// The keys it holds this tick: Wander's walk; or, in Meteor mode, a jump now and then, sneaking, and strafing left
// and right.
pub(super) fn afk_keys(game: &mut Game<'_>, m: &Module) -> Vec<KeyCode> {
    if m.p("Mode") == "Wander" {
        return wander(game, m);
    }
    let mut keys = Vec::new();
    if m.b("Jump") {
        // rand takes the same lock as ps: roll first, or the game freezes.
        let roll = rand(99) == 0;
        let jump = ps(|p| {
            p.afk_jump = !p.afk_jump && roll;
            p.afk_jump
        });
        if jump {
            keys.push(KeyCode::Space);
        }
    }
    if m.b("Sneak") {
        let sneak = ps(|p| {
            let held = p.afk_sneak < m.n("Sneak time") as u32;
            p.afk_sneak += 1;
            held
        });
        if sneak {
            keys.push(KeyCode::ShiftLeft);
        } else if rand(99) == 0 {
            ps(|p| p.afk_sneak = 0);
        }
    }
    if m.b("Strafe") {
        let key = ps(|p| {
            p.afk_strafe -= 1;
            if p.afk_strafe <= 0 {
                p.afk_key = Some(if p.afk_right { KeyCode::KeyD } else { KeyCode::KeyA });
                p.afk_right = !p.afk_right;
                p.afk_strafe = 20;
            }
            p.afk_key
        });
        keys.extend(key);
    }
    keys
}

fn anti_afk(game: &mut Game<'_>, m: &Module) {
    if !in_world(game) {
        return;
    }
    let meteor = m.p("Mode") == "Meteor";
    if meteor && m.b("Swing") && rand(99) == 0 {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
    if meteor && m.b("Spin") {
        let yaw = ps(|p| {
            let y = p.afk_yaw.unwrap_or(game.yaw) + m.n("Speed") as f32;
            p.afk_yaw = Some(y);
            y
        });
        if m.p("Spin mode") == "Client" {
            game.yaw = yaw;
        } else {
            super::sunset::turn("Anti AFK", yaw, m.n("Pitch") as f32);
        }
    }
    if m.b("Send messages") {
        let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
        if messages.is_empty() {
            return;
        }
        let due = ps(|p| {
            p.afk_message -= 1;
            p.afk_message <= 0
        });
        if due {
            let i = if m.b("Random") {
                rand(messages.len())
            } else {
                ps(|p| {
                    p.afk_index = (p.afk_index + 1) % messages.len();
                    p.afk_index
                })
            };
            game.send_chat(messages[i]);
            ps(|p| p.afk_message = m.n("Delay (seconds)") as i32 * 20);
        }
    }
}

// ---- Spam

fn spam(game: &mut Game<'_>, m: &Module) {
    if game.remote.is_none() {
        toggle_off(game, "Spam");
        return;
    }
    if ps(|p| {
        if p.spam_timer > 0 {
            p.spam_timer -= 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
    if messages.is_empty() {
        return;
    }
    let i = if m.b("Randomise") {
        rand(messages.len())
    } else {
        ps(|p| {
            let i = p.spam_index % messages.len();
            p.spam_index = i + 1;
            i
        })
    };
    let mut text = messages[i].to_owned();
    if m.b("Bypass") {
        const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        text.push(' ');
        for _ in 0..m.n("Bypass length") as usize {
            text.push(LETTERS[rand(LETTERS.len())] as char);
        }
    }
    text.truncate(256);
    game.send_chat(&text);
    ps(|p| p.spam_timer = m.n("Delay (ticks)") as u32);
    // Once: one message (or /command) as it turns on, then off (a kit or warp on a key).
    if m.b("Once") {
        toggle_off(game, "Spam");
    }
}

// ---- No Fall (Meteor's NoFall): Air place and Place, and Clutch's place under Packet and No ground. Those two change
// the move packets (runtime.rs's outgoing). This runs after the tick, once its move packet has gone, where Meteor uses
// the item.

// Place's choices: the item, and whether it is a bucket (used, and picked back up) or a block (placed).
const FALL_ITEMS: [(&str, &str, bool); 5] = [
    ("Bucket", "water_bucket", true),
    ("Powder snow", "powder_snow_bucket", true),
    ("Hay bale", "hay_block", false),
    ("Cobweb", "cobweb", false),
    ("Slime block", "slime_block", false),
];

fn no_fall(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if game.creative || p.flying || (m.b("Pause on mace") && game.held().is_some_and(|s| short(&s.item) == "mace")) {
        return;
    }
    match m.p("Mode") {
        "Air place" => fall_air_place(game, m),
        "Place" => fall_place(game, m),
        _ if m.b("Clutch") => fall_place(game, m),
        _ => {}
    }
}

// Air place: a block under you once the fall would hurt (Before damage) or kill (Before death).
fn fall_air_place(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let fallen = p.fall_distance;
    let health = f64::from(p.health.health() + p.health.absorption());
    let due = if m.p("Air place mode") == "Before damage" { fallen > 2.0 } else { fallen > health.max(2.0) };
    if !due {
        return;
    }
    let Some(slot) = (0..9).find(|&i| super::sunset::item_info(game, i).is_some_and(|info| info.block.is_some())) else {
        return;
    };
    if m.b("Anchor") {
        center_player(game);
    }
    if let Some(at) = block_pos(game) {
        place_block(game, [at[0], at[1] - 1, at[2]], slot, m.b("Rotate"), true);
    }
}

// Place: water (or what Placed item says) where the fall will land, a moment before it does; once you have stood
// still for two ticks, the water or powder snow goes back in the bucket.
fn fall_place(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (vy, fallen, still) = (p.velocity.y, p.fall_distance, p.fall_distance == 0.0 && p.ground.on_ground);
    if let Some((at, item, timer, stood, seen)) = ps(|s| s.fall_placed) {
        let stood = if still { (stood + 1).min(2) } else { 0 };
        let there = facts(game, at);
        let present = if item == "water_bucket" { there.fluid_level == 8 && there.name.ends_with("water") } else { there.name.ends_with("powder_snow") };
        // Gone once it was there (it was picked up, or flowed away), or never there in a second.
        let mut done = (seen && !present) || timer > 20;
        if stood >= 2 && present {
            if let (Some(slot), Some(eye)) = (find_hotbar(game, |n| n == "bucket"), game.eye_position()) {
                let (yaw, pitch) = look_at(eye, center_of(at));
                use_held(game, slot, yaw, pitch);
            }
            done = true;
        }
        ps(|s| s.fall_placed = (!done).then_some((at, item, timer + 1, stood, seen || present)));
        return;
    }
    if vy >= 0.0 || (fallen <= 3.0 && vy > -0.6) || p.fluids.was_touching_water {
        return;
    }
    let chosen = m.p("Placed item");
    // Water boils away in the Nether: powder snow instead.
    let chosen = if chosen == "Bucket" && game.dim.id() == "minecraft:the_nether" { "Powder snow" } else { chosen };
    let Some(&(_, item, bucket)) = FALL_ITEMS.iter().find(|i| i.0 == chosen) else {
        return;
    };
    let Some(slot) = find_hotbar(game, |n| n == item) else {
        return;
    };
    if m.b("Anchor") {
        center_player(game);
    }
    let (Some((ground, point)), Some(eye)) = (landing(game), game.eye_position()) else {
        return;
    };
    // The use reaches as far as a click does (4.5 blocks); further up, it waits for a later tick.
    if dist(eye, point) > 4.5 {
        return;
    }
    let above = [ground[0], ground[1] + 1, ground[2]];
    if bucket {
        let (yaw, pitch) = look_at(eye, point);
        use_held(game, slot, yaw, pitch);
        ps(|s| s.fall_placed = Some((above, item, 0, 0, false)));
    } else {
        place_block(game, above, slot, m.b("Rotate"), true);
    }
}

// Uses the item in a hotbar slot looking at (yaw, pitch), as a right click into the air does, then holds what you
// held before.
fn use_held(game: &mut Game<'_>, slot: usize, yaw: f32, pitch: f32) {
    let before = game.selected;
    hold(game, slot);
    let seq = sequence(game);
    game.send(S::UseItem(play::UseItem { hand: 0, sequence: seq, yaw, pitch }));
    game.swing_arm();
    hold(game, before);
}

// Where the fall lands within the next 8 ticks (Meteor's findLandingSurface, straight down): the block it lands on and
// the point on its top below you; nothing if it lands in water or later.
fn landing(game: &Game<'_>) -> Option<([i32; 3], [f64; 3])> {
    let p = game.player.as_ref()?;
    let (x, z) = (p.position.x, p.position.z);
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let feet = p.position.y.floor() as i32;
    let below = (0..64).map(|d| feet - 1 - d).find(|&y| !is_air(game, [bx, y, bz]))?;
    let ground = [bx, below, bz];
    if facts(game, ground).fluid_level > 0 || !facts(game, ground).full_collision {
        return None;
    }
    let top = f64::from(below + 1);
    let (mut y, mut vy) = (p.position.y, p.velocity.y);
    for _ in 0..8 {
        y += vy;
        vy = (vy - 0.08) * 0.98;
        if y <= top {
            return Some((ground, [x, top, z]));
        }
    }
    None
}

// ---- Speed Mine (Meteor's SpeedMine), before the tick, where the game goes on breaking the block

pub(super) fn speed_mine(game: &mut Game<'_>, m: Option<&Module>) {
    use super::remote::sunset_remote as remote;
    // Haste mode's own Haste, taken off when it is no longer wanted.
    let haste = m.filter(|m| m.p("Mode") == "Haste").map(|m| m.n("Haste amplifier") as i32 - 1);
    let ours = game.potions.effects.get("haste").is_some_and(|e| !e.visible);
    match haste {
        Some(amplifier) if game.potions.effects.get("haste").is_none_or(|e| e.amplifier < amplifier) => {
            let mut effect = horizon_client::effects::EffectInstance::new("haste", -1, amplifier);
            effect.visible = false;
            game.potions.effects.add(effect);
        }
        None if ours => {
            game.potions.effects.list.retain(|e| e.id != "haste" || e.visible);
        }
        _ => {}
    }
    let (Some(m), Some((pos, done))) = (m, remote::breaking(game)) else {
        return;
    };
    let name = &game.res.facts().get(game.world.block(pos[0], pos[1], pos[2])).name;
    if listed(m.t("Blocks"), name) != (m.p("Blocks filter") == "Whitelist") {
        return;
    }
    let step = game.destroy_progress(pos);
    match m.p("Mode") {
        // The server takes the stop once its count reaches 0.7 (counting the tick the break began in): be done then.
        "Damage" if (done > 0.0 && done + step >= 0.7) || (done == 0.0 && step > 0.5 && m.b("Instamine")) => {
            remote::set_breaking(game, 1.0);
        }
        // Modifier times the game's own step (the game adds one step itself).
        "Normal" if done > 0.0 => remote::set_breaking(game, done + step * (m.n("Modifier") as f32 - 1.0)),
        _ => {}
    }
}

// ---- No Break Reset, before the tick, where the game would see the new item and start the break again

pub(super) fn no_break_reset(game: &mut Game<'_>, m: Option<&Module>) {
    use super::remote::sunset_remote as remote;
    let (Some(m), Some((pos, _))) = (m, remote::breaking(game)) else {
        return;
    };
    let step = game.destroy_progress(pos);
    if remote::breaking_item_changed(game) && (!m.b("Only to better tools") || step >= ps(|p| p.break_step)) {
        remote::keep_breaking(game);
    }
    ps(|p| p.break_step = step);
}
