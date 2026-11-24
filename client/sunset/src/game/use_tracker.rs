// Use Tracker (after Nuclear's Use Tracker and Arc's Pearl and Potion Prediction from the 1.16.5 FunTime clients, and
// Sky's Pearl Trajectory; behaviour only, written anew): over each other player within Range, what they are using now
// (eating, drinking, blocking, drawing a bow...) and for how long, their totem pops, and what they last ate or drank
// with the time its effect has left (a potion's own length, a golden apple's absorption). With Landings, every pearl
// and splash or lingering potion others throw is followed to where it comes down: a box there with the thrower and
// the seconds left. A server's potions are followed from their throw (the game keeps no flight of its own for them).
// With Chat, each finished drink or meal and each throw's landing is said in the chat too.

use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, own_id, say};
use super::sunset_compat::{SunsetExtra as _, SunsetGame as _};
use super::sunset_util::{self as util, short};
use super::sunset_visual::{Look, draw_box};
use horizon_client::projectiles::ProjectileKind;
use horizon_protocol::packets::play::Clientbound as C;
use std::collections::HashMap;
use std::sync::Mutex;

pub(super) const NAME: &str = "Use Tracker";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Render, "Shows what others use, their effects and pops, and where their throws land.")
            .bool("Using", true)
            .bool("Effects", true)
            .bool("Pops", true)
            .num("Range", 64.0, 8.0, 256.0, 8.0)
            .colour("Text color", "White")
            .bool("Landings", true)
            .bool("Own throws", false)
            .colour("Pearl color", "Purple")
            .colour("Potion color", "Pink")
            .bool("Chat", false),
    ]
}

// What a player is doing: the item in use and since when; the last thing they finished, when, and its effect's length.
#[derive(Clone, Default)]
struct Player {
    using: Option<(String, u64)>,
    done: Option<(String, u64, u64)>,
}

#[derive(Default)]
struct State {
    players: HashMap<u32, Player>,
    // Who threw each pearl and potion and whether it is a pearl (from the server's spawn).
    owners: HashMap<u32, (u32, bool)>,
    // Landings worked out this tick: where, the ticks left, whose, a pearl.
    landings: Vec<([f64; 3], u32, String, bool)>,
    // A server's potions in flight (the client keeps no projectile for them): from where and how fast they were
    // thrown, when, by whom, and where they come down once worked out.
    potions: HashMap<u32, Thrown>,
    // Chat: when each throw was first seen, and those told.
    seen: HashMap<u32, u64>,
    told: std::collections::HashSet<u32>,
}

#[derive(Clone)]
struct Thrown {
    from: [f64; 3],
    vel: [f64; 3],
    at: u64,
    owner: u32,
    land: Option<Option<([f64; 3], u32)>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

// A potion's or food's effect length in ticks (vanilla's), and the word for it; None: no timer (instant, or not one).
fn effect_of(item: &str, potion: Option<&str>) -> Option<(String, u64)> {
    let min = |m: f64| (m * 1200.0) as u64;
    match item {
        "golden_apple" => return Some(("absorption".into(), min(2.0))),
        "enchanted_golden_apple" => return Some(("resistance".into(), min(5.0))),
        "potion" => {}
        _ => return None,
    }
    let id = short(potion?);
    let (kind, base) = if let Some(k) = id.strip_prefix("long_") { (k, 1) } else if let Some(k) = id.strip_prefix("strong_") { (k, 2) } else { (id, 0) };
    // Normal, long, strong.
    let (t, name): ([f64; 3], &str) = match kind {
        "swiftness" => ([3.0, 8.0, 1.5], "speed"),
        "strength" => ([3.0, 8.0, 1.5], "strength"),
        "leaping" => ([3.0, 8.0, 1.5], "jump boost"),
        "fire_resistance" => ([3.0, 8.0, 3.0], "fire resistance"),
        "invisibility" => ([3.0, 8.0, 3.0], "invisibility"),
        "night_vision" => ([3.0, 8.0, 3.0], "night vision"),
        "water_breathing" => ([3.0, 8.0, 3.0], "water breathing"),
        "regeneration" => ([0.75, 1.5, 22.0 / 60.0], "regeneration"),
        "poison" => ([0.75, 1.5, 21.6 / 60.0], "poison"),
        "slow_falling" => ([1.5, 4.0, 1.5], "slow falling"),
        "turtle_master" => ([1.0 / 3.0, 2.0 / 3.0, 1.0 / 3.0], "turtle master"),
        "weakness" => ([1.5, 4.0, 1.5], "weakness"),
        "slowness" => ([1.5, 4.0, 1.0 / 3.0], "slowness"),
        "wind_charged" | "weaving" | "oozing" | "infested" => ([3.0, 3.0, 3.0], kind),
        "luck" => ([5.0, 5.0, 5.0], "luck"),
        _ => return None,
    };
    Some((format!("{}{}", name.replace('_', " "), if base == 2 { " II" } else { "" }), min(t[base])))
}

fn clock(ticks: u64) -> String {
    let s = ticks / 20;
    format!("{}:{:02}", s / 60, s % 60)
}

fn verb(item: &str) -> &'static str {
    match item {
        "potion" | "milk_bucket" | "honey_bottle" | "ominous_bottle" => "drinking",
        "shield" => "blocking",
        "bow" | "crossbow" | "trident" => "drawing",
        "spyglass" | "goat_horn" | "brush" => "using",
        _ => "eating",
    }
}

pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if let C::RemoveEntities(p) = packet {
        if STATE.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            st(|s| {
                for id in &p.ids {
                    s.potions.remove(&(*id as u32));
                }
            });
        }
        return;
    }
    let C::AddEntity(p) = packet else {
        return;
    };
    let kind = game.remote.as_ref().and_then(|r| r.builtin.get("minecraft:entity_type")).and_then(|t| t.name(p.kind));
    // Thrown potions: "potion" before 1.21.5, splash and lingering since.
    if matches!(kind, Some("minecraft:ender_pearl" | "minecraft:splash_potion" | "minecraft:lingering_potion" | "minecraft:potion"))
        && let (Ok(id), Ok(owner)) = (u32::try_from(p.id), u32::try_from(p.data))
        && STATE.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    {
        st(|s| {
            if s.owners.len() > 256 {
                s.owners.clear();
            }
            let pearl = kind == Some("minecraft:ender_pearl");
            s.owners.insert(id, (owner, pearl));
            if !pearl && s.potions.len() < 64 {
                s.potions.insert(id, Thrown { from: p.position, vel: p.velocity, at: game.ticks, owner, land: None });
            }
        });
    }
}

// Where a throw at `pos` going `vel` comes down: the point and the ticks. Each tick, as the game moves a throw:
// gravity `g` and drag 0.99 first, then the move.
fn landing(game: &Game<'_>, mut pos: [f64; 3], mut vel: [f64; 3], g: f64) -> Option<([f64; 3], u32)> {
    for tick in 1..=300 {
        vel = [vel[0] * 0.99, (vel[1] - g) * 0.99, vel[2] * 0.99];
        let next = [pos[0] + vel[0], pos[1] + vel[1], pos[2] + vel[2]];
        let d = util::dist(pos, next);
        if d > 1e-9 && let Some(hit) = game.clip_ray(pos, [(next[0] - pos[0]) / d, (next[1] - pos[1]) / d, (next[2] - pos[2]) / d], d) {
            return Some((hit.point, tick));
        }
        pos = next;
        if pos[1] < -128.0 {
            break;
        }
    }
    None
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| util::connected(game)) else {
        if m.is_none() {
            *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        return;
    };
    let now = game.ticks;
    let me = own_id(game);
    let here = game.player.as_ref().map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
    let range = m.n("Range");
    // Each player's use this tick: (id, the item in use, its potion).
    let uses: Vec<(u32, Option<(String, Option<String>)>)> = game
        .mobs
        .iter()
        .filter(|x| x.kind.id == "minecraft:player" && x.alive() && !x.removed && Some(x.id) != me)
        .filter(|x| util::dist(here, [x.position.x, x.position.y, x.position.z]) <= range)
        .map(|x| {
            let e = &x.extra;
            let item = (e.using_item > 0).then(|| {
                if e.using_off_hand {
                    e.sunset_off_hand().map(|id| (short(&id).to_owned(), None))
                } else {
                    e.main_hand_stack()
                        .map(|s| (short(&s.item).to_owned(), s.potion().map(str::to_owned)))
                        .or_else(|| e.sunset_main_hand().map(|id| (short(&id).to_owned(), None)))
                }
            });
            (x.id, item.flatten())
        })
        .collect();
    let mut finished: Vec<(u32, String)> = Vec::new();
    st(|s| {
        s.players.retain(|id, _| uses.iter().any(|u| u.0 == *id));
        for (id, item) in uses {
            // The item in use, with its potion after a "|".
            let key = item.map(|(i, po)| po.map_or(i.clone(), |po| format!("{i}|{po}")));
            let p = s.players.entry(id).or_default();
            if p.using.as_ref().map(|u| &u.0) == key.as_ref() {
                continue;
            }
            // A use that ran 30 ticks or more finished: its effect starts now.
            if let Some((was, since)) = p.using.take()
                && now >= since + 30
            {
                let (item, potion) = match was.split_once('|') {
                    Some((i, po)) => (i, Some(po)),
                    None => (was.as_str(), None),
                };
                if let Some((what, length)) = effect_of(item, potion) {
                    finished.push((id, format!("{what} {}", clock(length))));
                    p.done = Some((what, now, length));
                }
            }
            p.using = key.map(|k| (k, now));
        }
    });
    // Landings of others' (and with Own throws your) pearls and potions.
    let own = m.b("Own throws");
    let landings: Vec<(u32, ([f64; 3], u32, String, bool))> = if m.b("Landings") || m.b("Chat") {
        let (owners, potions) = st(|s| (s.owners.clone(), s.potions.clone()));
        let potion_ids: Vec<u32> = potions.keys().copied().collect();
        // A server's potions: each worked out once, from its throw.
        let mut out: Vec<(u32, ([f64; 3], u32, String, bool))> = Vec::new();
        let mut worked: Vec<(u32, Option<([f64; 3], u32)>)> = Vec::new();
        for (id, t) in potions {
            if !own && Some(t.owner) == me {
                continue;
            }
            let land = t.land.unwrap_or_else(|| {
                let l = landing(game, t.from, t.vel, 0.05);
                worked.push((id, l));
                l
            });
            if let Some((at, ticks)) = land {
                let left = (t.at + u64::from(ticks)).saturating_sub(now) as u32;
                let who = game.remote_player_name(t.owner).map(str::to_owned).unwrap_or_default();
                out.push((id, (at, left, who, false)));
            }
        }
        st(|s| {
            for (id, l) in worked {
                if let Some(t) = s.potions.get_mut(&id) {
                    t.land = Some(l);
                }
            }
            // Long since down: forget it.
            s.potions.retain(|_, t| now < t.at + 320);
        });
        out.extend(game.creatures
            .projectiles
            .iter()
            .filter(|p| !p.stuck && !potion_ids.contains(&p.id))
            .filter_map(|p| {
                // A server's potions come as plain thrown items: the spawn said which they are.
                let known = owners.get(&p.id).copied();
                let (g, pearl) = match (&p.kind, known) {
                    (ProjectileKind::EnderPearl, _) | (_, Some((_, true))) => (0.03, true),
                    (ProjectileKind::Potion(..), _) | (_, Some((_, false))) => (0.05, false),
                    _ => return None,
                };
                let owner = known.map(|k| k.0);
                if !own && owner.is_some() && owner == me {
                    return None;
                }
                let (at, ticks) = landing(game, [p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.y, p.velocity.z], g)?;
                let who = owner.and_then(|o| game.remote_player_name(o).map(str::to_owned)).unwrap_or_default();
                Some((p.id, (at, ticks, who, pearl)))
            }));
        out
    } else {
        Vec::new()
    };
    let mut lines: Vec<String> = Vec::new();
    if m.b("Chat") {
        for (id, what) in finished {
            if let Some(name) = game.remote_player_name(id) {
                lines.push(format!("Use Tracker: {name} took {what}"));
            }
        }
    }
    st(|s| {
        // A throw is told once, from its third tick (its first speed can be off).
        s.seen.retain(|id, _| game.creatures.projectiles.iter().any(|p| p.id == *id));
        s.told.retain(|id| s.seen.contains_key(id) || s.potions.contains_key(id));
        for p in game.creatures.projectiles.iter() {
            s.seen.entry(p.id).or_insert(now);
        }
        if m.b("Chat") {
            for (id, (at, ticks, who, pearl)) in &landings {
                // A server's potion is told at once (its throw's speed is exact).
                let ready = s.seen.get(id).is_some_and(|t| now >= t + 2) || s.potions.contains_key(id);
                if ready && s.told.insert(*id) {
                    let whose = if who.is_empty() { String::new() } else { format!("{who}'s ") };
                    lines.push(format!(
                        "Use Tracker: {whose}{} lands at {:.1} {:.1} {:.1} in {:.1}s",
                        if *pearl { "pearl" } else { "potion" },
                        at[0], at[1], at[2], f64::from(*ticks) / 20.0
                    ));
                }
            }
        }
        s.landings = landings.into_iter().map(|(_, l)| l).collect();
    });
    for line in lines {
        say(game, &line);
    }
}

// What to write over a player: their use, last effect and pops; empty for nothing.
fn label(p: &Player, pops: u32, m: &Module, now: u64) -> Vec<String> {
    let mut out = Vec::new();
    if m.b("Using") && let Some((item, since)) = &p.using {
        let name = item.split('|').next().unwrap_or(item);
        out.push(format!("{} {} {:.1}s", verb(name), name.replace('_', " "), now.saturating_sub(*since) as f64 / 20.0));
    }
    if m.b("Effects") && let Some((what, at, length)) = &p.done && now < at + length {
        out.push(format!("{what} {}", clock(at + length - now)));
    }
    if m.b("Pops") && pops > 0 {
        out.push(format!("{pops} pop{}", if pops == 1 { "" } else { "s" }));
    }
    out
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else { return };
    let now = game.ticks;
    let text = m.c("Text color");
    let (players, landings) = match STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(s) => (s.players.clone(), s.landings.clone()),
        None => return,
    };
    for x in game.mobs.iter().filter(|x| players.contains_key(&x.id)) {
        let lines = label(&players[&x.id], super::sunset_effects::pops(x.id), m, now);
        let top = x.bounds().max_position().y + 0.9;
        for (i, line) in lines.iter().enumerate() {
            if let Some([sx, sy]) = cam.project([x.position.x, top, x.position.z]) {
                out.text(line, [sx - game.sunset_text_width(line) / 2.0, sy - 10.0 * (lines.len() - i) as f32], text);
            }
        }
    }
    for (at, ticks, who, pearl) in landings.into_iter().filter(|_| m.b("Landings")) {
        let argb = m.c(if pearl { "Pearl color" } else { "Potion color" });
        let r = if pearl { 0.3 } else { 0.5 };
        draw_box(cam, out, &Look::BOXED, [at[0] - r, at[1], at[2] - r], [at[0] + r, at[1] + 0.1, at[2] + r], argb, lw);
        if let Some([sx, sy]) = cam.project([at[0], at[1] + 0.6, at[2]]) {
            let what = if pearl { "pearl" } else { "potion" };
            let line = if who.is_empty() { format!("{what} {:.1}s", f64::from(ticks) / 20.0) } else { format!("{who}'s {what} {:.1}s", f64::from(ticks) / 20.0) };
            out.text(&line, [sx - game.sunset_text_width(&line) / 2.0, sy], argb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potion_lengths_follow_their_kind() {
        assert_eq!(effect_of("potion", Some("minecraft:strength")), Some(("strength".into(), 3600)));
        assert_eq!(effect_of("potion", Some("minecraft:long_strength")), Some(("strength".into(), 9600)));
        assert_eq!(effect_of("potion", Some("minecraft:strong_strength")), Some(("strength II".into(), 1800)));
        assert_eq!(effect_of("potion", Some("minecraft:healing")), None);
        assert_eq!(effect_of("golden_apple", None), Some(("absorption".into(), 2400)));
        assert_eq!(effect_of("bread", None), None);
        assert_eq!(clock(2400), "2:00");
    }
}
