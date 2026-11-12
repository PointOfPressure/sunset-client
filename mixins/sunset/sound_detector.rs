// Sound Detector: what players nearby are doing, from what the server sends of it. Sounds (a chest, barrel or door
// opening, a block placed, an anvil, an enchanting table, a brewing stand, a player eating or hurt), block events
// (chest and shulker lids, sent from 64 blocks where their sound carries 16; pistons, note blocks, bells), level events
// (a block broken, an anvil used, a dispenser), other players' mining cracks and pickups (both say who), explosions,
// and the far events every player hears from anywhere with the place they happened (a wither spawned, an end portal
// opened, a dragon killed). Mob, weather and ambient sounds are left out, and so is what a mob next to it more likely
// did (a villager's door, a sheep eating grass) when no player is there. Each heard thing is said with where it is from
// you; the same thing again close by within Cooldown adds to its count instead, said again once Cooldown has passed.
// Markers box where things were heard for Marker time. It reads only what the server already sends.
//
// After Donut Client 3.0's Sound Detector (MIT), written anew for Sunset.

use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_util::*;
use super::sunset_visual::{Look, draw_box};
use super::Game;
use horizon_protocol::packets::play::Clientbound as C;
use horizon_world::registry::BlockStateId;
use std::sync::Mutex;

pub(super) const NAME: &str = "Sound Detector";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::World, "Tells what players nearby are doing.")
            .group("General")
            .num("Range", 64.0, 4.0, 128.0, 4.0)
            .num("Ignore near you", 5.0, 0.0, 16.0, 1.0)
            .bool("Players only", false)
            .num("Cooldown", 10.0, 0.0, 120.0, 1.0)
            .num("Group radius", 6.0, 1.0, 32.0, 1.0)
            .bool("Ping", false)
            .group("Sounds")
            .bool("Containers", true)
            .bool("Doors", true)
            .bool("Building", true)
            .bool("Mining", true)
            .bool("Crafting", true)
            .bool("Player actions", true)
            .bool("Redstone", false)
            .bool("Explosions", true)
            .bool("Far events", true)
            .group("Markers")
            .bool("Markers", true)
            .num("Marker time", 30.0, 1.0, 300.0, 1.0)
            .colour("Color", "Yellow"),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Marker time" | "Color" => m.b("Markers"),
        _ => true,
    }
}

// The kinds of thing heard, by the switch that keeps them.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Containers,
    Doors,
    Building,
    Mining,
    Crafting,
    Player,
    Redstone,
    Explosions,
    Far,
}

impl Kind {
    fn switch(self) -> &'static str {
        match self {
            Kind::Containers => "Containers",
            Kind::Doors => "Doors",
            Kind::Building => "Building",
            Kind::Mining => "Mining",
            Kind::Crafting => "Crafting",
            Kind::Player => "Player actions",
            Kind::Redstone => "Redstone",
            Kind::Explosions => "Explosions",
            Kind::Far => "Far events",
        }
    }
    // What a mob would as likely have done: left out when a mob is next to it and no player.
    fn mob_made(self) -> bool {
        matches!(self, Kind::Doors | Kind::Building | Kind::Player)
    }
}

// Vanilla's SoundSource numbers the packets carry.
const SOURCE_BLOCKS: i32 = 4;
const SOURCE_PLAYERS: i32 = 7;

// A sound by its name and source: what it says and its kind. None for what is left out.
fn sound_kind(name: &str, source: i32) -> Option<(Kind, String)> {
    let name = short(name);
    let fixed = |kind: Kind, label: &str| Some((kind, label.to_owned()));
    if source != SOURCE_BLOCKS && source != SOURCE_PLAYERS {
        return None;
    }
    if let Some(block) = name.strip_prefix("block.").and_then(|n| n.strip_suffix(".open")) {
        return match block {
            b if b.ends_with("chest") || b == "barrel" || b == "shulker_box" => Some((Kind::Containers, format!("{} opened", b.replace('_', " ")))),
            b if b.ends_with("_trapdoor") => fixed(Kind::Doors, "trapdoor opened"),
            b if b.ends_with("_door") => fixed(Kind::Doors, "door opened"),
            b if b.ends_with("_fence_gate") => fixed(Kind::Doors, "fence gate opened"),
            _ => None,
        };
    }
    if let Some(block) = name.strip_prefix("block.").and_then(|n| n.strip_suffix(".place")) {
        return Some((Kind::Building, format!("block placed ({} sound)", block.replace('_', " "))));
    }
    match name {
        "block.anvil.use" => fixed(Kind::Crafting, "anvil used"),
        "block.enchantment_table.use" => fixed(Kind::Crafting, "enchanting table used"),
        "block.brewing_stand.brew" => fixed(Kind::Crafting, "potion brewed"),
        "block.smithing_table.use" => fixed(Kind::Crafting, "smithing table used"),
        "block.grindstone.use" => fixed(Kind::Crafting, "grindstone used"),
        "block.crafter.craft" => fixed(Kind::Crafting, "crafter crafted"),
        "block.composter.fill" | "block.composter.fill_success" => fixed(Kind::Crafting, "composter filled"),
        "block.respawn_anchor.charge" => fixed(Kind::Crafting, "respawn anchor charged"),
        "block.respawn_anchor.set_spawn" => fixed(Kind::Crafting, "respawn anchor set as spawn"),
        "block.beacon.activate" => fixed(Kind::Crafting, "beacon turned on"),
        "block.conduit.activate" => fixed(Kind::Crafting, "conduit turned on"),
        "block.end_portal_frame.fill" => fixed(Kind::Crafting, "eye of ender put in a portal frame"),
        "block.bell.use" => fixed(Kind::Crafting, "bell rung"),
        "block.piston.extend" | "block.piston.contract" => fixed(Kind::Redstone, "piston"),
        "block.dispenser.dispense" | "block.dispenser.launch" => fixed(Kind::Redstone, "dispenser"),
        "block.lever.click" => fixed(Kind::Redstone, "lever"),
        "block.comparator.click" => fixed(Kind::Redstone, "comparator"),
        "block.tripwire.click_on" => fixed(Kind::Redstone, "tripwire"),
        n if n.starts_with("block.note_block.") => fixed(Kind::Redstone, "note block"),
        n if n.ends_with("_button.click_on") => fixed(Kind::Redstone, "button pressed"),
        n if n.ends_with("_pressure_plate.click_on") => fixed(Kind::Redstone, "pressure plate"),
        "entity.generic.eat" | "entity.player.burp" => fixed(Kind::Player, "eating"),
        "entity.generic.drink" => fixed(Kind::Player, "drinking"),
        "entity.player.hurt" | "entity.player.hurt_on_fire" | "entity.player.hurt_drown" | "entity.player.hurt_freeze" => fixed(Kind::Player, "player hurt"),
        "entity.player.levelup" => fixed(Kind::Player, "level up"),
        "entity.ender_pearl.throw" => fixed(Kind::Player, "ender pearl thrown"),
        "entity.firework_rocket.launch" => fixed(Kind::Player, "firework launched"),
        "entity.experience_bottle.throw" => fixed(Kind::Player, "bottle o' enchanting thrown"),
        "item.trident.throw" => fixed(Kind::Player, "trident thrown"),
        "entity.fishing_bobber.throw" => fixed(Kind::Player, "fishing"),
        "item.bucket.fill" | "item.bucket.fill_lava" | "item.bucket.empty" | "item.bucket.empty_lava" => fixed(Kind::Player, "bucket used"),
        "item.flintandsteel.use" => fixed(Kind::Player, "flint and steel used"),
        "item.shield.block" => fixed(Kind::Player, "shield hit"),
        "item.hoe.till" => fixed(Kind::Player, "ground tilled"),
        "item.axe.strip" => fixed(Kind::Player, "log stripped"),
        n if n.starts_with("item.armor.equip_") => fixed(Kind::Player, "armor put on"),
        n if n.starts_with("entity.player.attack.") => fixed(Kind::Player, "player attacking"),
        _ => None,
    }
}

// A level event by its number (LevelEvent's constants): what it says and its kind.
fn level_kind(game: &Game<'_>, kind: i32, data: i32, global: bool) -> Option<(Kind, String)> {
    let fixed = |k: Kind, label: &str| Some((k, label.to_owned()));
    if global {
        return match kind {
            1023 => fixed(Kind::Far, "wither spawned"),
            1028 => fixed(Kind::Far, "ender dragon killed"),
            1038 => fixed(Kind::Far, "end portal opened"),
            _ => None,
        };
    }
    match kind {
        2001 => {
            let name = &game.res.facts().get(BlockStateId(data as u32)).name;
            Some((Kind::Building, format!("{} broken", short(name).replace('_', " "))))
        }
        1030 => fixed(Kind::Crafting, "anvil used"),
        1042 => fixed(Kind::Crafting, "grindstone used"),
        1044 => fixed(Kind::Crafting, "smithing table used"),
        1043 => fixed(Kind::Crafting, "lectern page turned"),
        1503 => fixed(Kind::Crafting, "eye of ender put in a portal frame"),
        1000 | 1002 => fixed(Kind::Redstone, "dispenser"),
        3003 => fixed(Kind::Building, "copper waxed"),
        3004 | 3005 => fixed(Kind::Building, "copper scraped"),
        _ => None,
    }
}

// Something heard: where, what, its kind and who (when the packet says).
struct Heard {
    at: [f64; 3],
    kind: Kind,
    label: String,
    actor: Option<String>,
}

// A thing heard and its repeats: where, what, how many times, who, and when it was last heard and last said.
struct Incident {
    at: [f64; 3],
    label: String,
    actor: Option<String>,
    count: u32,
    seen: u64,
    said: u64,
    told: u32,
}

#[derive(Default)]
struct Sd {
    incidents: Vec<Incident>,
}

static SD: Mutex<Option<Sd>> = Mutex::new(None);

fn sd<R>(f: impl FnOnce(&mut Sd) -> R) -> R {
    f(SD.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Sd::default))
}

fn block_at(p: [i32; 3]) -> [f64; 3] {
    center_of(p)
}

// What the packets of this tick say was heard.
fn hear(game: &Game<'_>, packets: Vec<C>) -> Vec<Heard> {
    let me = super::sunset::own_id(game);
    let player = |id: u32| (Some(id) != me).then(|| game.remote_player_name(id).map(str::to_owned)).flatten();
    let mob_at = |id: u32| game.mobs.iter().find(|x| x.id == id).map(|x| [x.position.x, x.position.y, x.position.z]);
    let mut out = Vec::new();
    for p in packets {
        let heard = match p {
            C::Sound(s) => game.remote_sound_name(&s.sound).and_then(|n| sound_kind(&n, s.source)).map(|(kind, label)| Heard {
                at: [f64::from(s.x) / 8.0, f64::from(s.y) / 8.0, f64::from(s.z) / 8.0],
                kind,
                label,
                actor: None,
            }),
            C::SoundEntity(s) => {
                let id = s.id as u32;
                let kind = game.remote_sound_name(&s.sound).and_then(|n| sound_kind(&n, s.source));
                match (kind, mob_at(id)) {
                    (Some((kind, label)), Some(at)) if Some(id) != me => Some(Heard { at, kind, label, actor: player(id) }),
                    _ => None,
                }
            }
            C::BlockEvent(e) => {
                let name = short(&game.res.facts().get(game.world.block(e.pos[0], e.pos[1], e.pos[2])).name).to_owned();
                let what = if (name.ends_with("chest") || name.ends_with("shulker_box")) && e.b0 == 1 && e.b1 > 0 {
                    let label = if name.ends_with("shulker_box") { "shulker box".to_owned() } else { name.replace('_', " ") };
                    Some((Kind::Containers, format!("{label} opened")))
                } else if name == "piston" || name == "sticky_piston" {
                    Some((Kind::Redstone, "piston".to_owned()))
                } else if name == "note_block" {
                    Some((Kind::Redstone, "note block".to_owned()))
                } else if name == "bell" {
                    Some((Kind::Crafting, "bell rung".to_owned()))
                } else {
                    None
                };
                what.map(|(kind, label)| Heard { at: block_at(e.pos), kind, label, actor: None })
            }
            C::LevelEvent(e) => level_kind(game, e.kind, e.data, e.global).map(|(kind, label)| Heard { at: block_at(e.pos), kind, label, actor: None }),
            // A crack's start (stage 0) only; its stages after are the same digging.
            C::BlockDestruction(d) if d.progress == 0 && Some(d.id as u32) != me => {
                let name = short(&game.res.facts().get(game.world.block(d.pos[0], d.pos[1], d.pos[2])).name).replace('_', " ");
                Some(Heard { at: block_at(d.pos), kind: Kind::Mining, label: format!("mining {name}"), actor: player(d.id as u32) })
            }
            C::TakeItemEntity(t) if Some(t.player as u32) != me => {
                mob_at(t.player as u32).zip(player(t.player as u32)).map(|(at, name)| Heard { at, kind: Kind::Player, label: "items picked up".into(), actor: Some(name) })
            }
            C::Explode(x) => Some(Heard { at: x.center, kind: Kind::Explosions, label: "explosion".into(), actor: None }),
            _ => None,
        };
        out.extend(heard);
    }
    out
}

// Where `at` is from `from`: how far, which way and how far up or down.
fn bearing(from: [f64; 3], at: [f64; 3]) -> String {
    const WAYS: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    let (dx, dz) = (at[0] - from[0], at[2] - from[2]);
    let way = WAYS[((dx.atan2(-dz).to_degrees() + 360.0 + 22.5) / 45.0) as usize % 8];
    let dy = (at[1] - from[1]).round() as i64;
    let height = match dy {
        0 => String::new(),
        d if d > 0 => format!(", {d} up"),
        d => format!(", {} down", -d),
    };
    format!("{:.0} m {way}{height}", dx.hypot(dz))
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| in_world(game)) else {
        horizon_client::net::sunset_packets::sounds_on(false);
        sd(|s| s.incidents.clear());
        return;
    };
    horizon_client::net::sunset_packets::sounds_on(true);
    let packets = horizon_client::net::sunset_packets::take_sounds();
    let Some(here) = player_pos(game) else { return };
    let now = game.ticks;
    let (cooldown, group, keep) = ((m.n("Cooldown") * 20.0) as u64, m.n("Group radius"), (m.n("Marker time") * 20.0) as u64);
    let me = super::sunset::own_id(game);
    let mut lines = Vec::new();
    for h in hear(game, packets) {
        if !m.b(h.kind.switch()) {
            continue;
        }
        let d = dist(here, h.at);
        if h.kind != Kind::Far && (d > m.n("Range") || d < m.n("Ignore near you")) {
            continue;
        }
        // Who is there: a player right by it, and whether a mob is closer to it than any player.
        let mut player_near: Option<(f64, String)> = None;
        let mut mob_near = f64::MAX;
        for x in game.mobs.iter().filter(|x| x.alive() && !x.removed && Some(x.id) != me) {
            let e = dist([x.position.x, x.position.y, x.position.z], h.at);
            if x.kind.id == "minecraft:player" {
                if e <= 4.0 && player_near.as_ref().is_none_or(|(b, _)| e < *b) && let Some(name) = game.remote_player_name(x.id) {
                    player_near = Some((e, name.to_owned()));
                }
            } else {
                mob_near = mob_near.min(e);
            }
        }
        if h.actor.is_none() && player_near.is_none() && h.kind.mob_made() && mob_near <= 2.5 {
            continue;
        }
        let actor = h.actor.clone().or(player_near.map(|(_, n)| n));
        if m.b("Players only") && actor.is_none() && h.kind != Kind::Far {
            continue;
        }
        let said = sd(|s| {
            if let Some(i) = s.incidents.iter_mut().find(|i| i.label == h.label && now - i.seen <= cooldown.max(20) && dist(i.at, h.at) <= group) {
                i.count += 1;
                i.seen = now;
                i.actor = actor.clone().or(i.actor.take());
                if now - i.said < cooldown {
                    return None;
                }
                i.said = now;
                let more = i.count - i.told;
                i.told = i.count;
                return Some((i.label.clone(), i.at, i.actor.clone(), more));
            }
            s.incidents.push(Incident { at: h.at, label: h.label.clone(), actor: actor.clone(), count: 1, seen: now, said: now, told: 1 });
            Some((h.label.clone(), h.at, actor.clone(), 1))
        });
        if let Some((label, at, actor, n)) = said {
            let times = if n > 1 { format!(" x{n}") } else { String::new() };
            let who = actor.map_or(String::new(), |a| format!(", {a}"));
            let [x, y, z] = at.map(|v| v.floor() as i64);
            lines.push(format!("Heard {label}{times} {} ({x} {y} {z}){who}", bearing(here, at)));
        }
    }
    sd(|s| s.incidents.retain(|i| now - i.seen <= keep.max(cooldown)));
    if !lines.is_empty() && m.b("Ping") {
        game.play_at_feet("minecraft:block.note_block.pling");
    }
    for line in lines {
        report(game, &line);
    }
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on && m.b("Markers")) else { return };
    let (argb, keep) = (m.c("Color"), (m.n("Marker time") * 20.0).max(1.0));
    let now = game.ticks;
    sd(|s| {
        for i in &s.incidents {
            let age = (now - i.seen) as f64;
            if age > keep {
                continue;
            }
            let alpha = (f64::from(argb >> 24) * (1.0 - age / keep).max(0.15)) as u32;
            let [x, y, z] = i.at;
            let (lo, hi) = ([x - 0.5, y - 0.5, z - 0.5], [x + 0.5, y + 0.5, z + 0.5]);
            draw_box(cam, out, &Look::BOXED, lo, hi, alpha << 24 | (argb & 0x00FF_FFFF), lw);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{Kind, bearing, sound_kind};

    #[test]
    fn player_made_sounds_are_named_and_mob_sounds_left_out() {
        assert!(sound_kind("minecraft:block.chest.open", 4).is_some_and(|(k, l)| k == Kind::Containers && l == "chest opened"));
        assert!(sound_kind("minecraft:block.oak_door.open", 4).is_some_and(|(k, _)| k == Kind::Doors));
        assert!(sound_kind("minecraft:block.stone.place", 4).is_some_and(|(k, _)| k == Kind::Building));
        assert!(sound_kind("minecraft:entity.generic.eat", 7).is_some_and(|(k, _)| k == Kind::Player));
        assert!(sound_kind("minecraft:entity.zombie.ambient", 5).is_none());
        assert!(sound_kind("minecraft:block.chest.close", 4).is_none());
    }

    #[test]
    fn bearings_name_the_way_north_is_minus_z() {
        assert!(bearing([0.0, 64.0, 0.0], [0.0, 64.0, -20.0]).starts_with("20 m north"));
        assert!(bearing([0.0, 64.0, 0.0], [10.0, 60.0, 0.0]).ends_with("east, 4 down"));
    }
}
