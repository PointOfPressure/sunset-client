// World modules ported from Meteor's source: Auto Shearer, Auto Nametag,
// Auto Mount, Flamethrower; and Bow Aimbot.

use super::sunset::{Category, Module, hold};
use super::sunset_util::*;
use super::{Game, Screen};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Auto Shearer", World, "Shears sheep that haven't been.")
            .num("Distance", 5.0, 1.0, 6.0, 0.5)
            .bool("Anti break", false)
            .bool("Rotate", true),
        Module::new("Auto Nametag", World, "Uses name tags on every named-less mob in range. It WILL tag all of them.")
            .text("Entities", "")
            .num("Range", 5.0, 1.0, 6.0, 0.5)
            .bool("Renametag", true)
            .bool("Rotate", true),
        Module::new("Auto Mount", World, "Mounts rideable mobs next to you.")
            .bool("Check saddle", false)
            .bool("Rotate", true)
            .text("Entities", "horse,donkey,mule,pig,strider,llama,camel,skeleton_horse,zombie_horse"),
        Module::new("Flamethrower", World, "Sets alive food animals on fire, so they drop cooked food.")
            .num("Distance", 5.0, 1.0, 6.0, 0.5)
            .bool("Anti break", false)
            .bool("Target babies", false)
            .num("Tick interval", 5.0, 1.0, 20.0, 1.0)
            .bool("Rotate", true)
            .text("Entities", "pig,cow,sheep,chicken,rabbit"),
        Module::new("Liquid Filler", World, "Places blocks inside water and lava within range of you.")
            .pick("Place in", &["Both", "Water", "Lava"])
            .pick("Shape", &["Sphere", "Uniform cube"])
            .num("Place range", 4.5, 1.0, 6.0, 0.5)
            .num("Walls range", 4.5, 1.0, 6.0, 0.5)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0)
            .num("Max blocks per tick", 1.0, 1.0, 10.0, 1.0)
            .pick("Sort", &["Furthest", "Closest", "Top down", "Bottom up", "None"])
            .bool("Rotate", true)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Whitelist", "dirt,cobblestone,stone,netherrack,diorite,granite,andesite")
            .text("Blacklist", ""),
        Module::new("Spawn Proofer", World, "Places torches or slabs on the spots where mobs could spawn.")
            .num("Place delay", 1.0, 0.0, 10.0, 1.0)
            .num("Place range", 4.5, 1.0, 6.0, 0.5)
            .num("Walls range", 4.5, 1.0, 6.0, 0.5)
            .num("Blocks per tick", 1.0, 1.0, 10.0, 1.0)
            .num("Light level", 0.0, 0.0, 15.0, 1.0)
            .text("Blocks", "torch,stone_button,stone_slab")
            .pick("Mode", &["Both", "Always", "Potential"])
            .bool("Rotate", true),
        Module::new("Bow Aimbot", Category::Combat, "Aims your bow at the nearest target while you draw it.")
            .num("Range", 20.0, 5.0, 60.0, 1.0)
            .pick("Priority", PRIORITIES)
            .bool("Babies", true)
            .bool("Nametagged", false)
            .text("Entities", "player"),
    ]
}

#[derive(Default)]
struct Ws {
    cooldowns: Vec<(u32, u32)>,
    flame_ticks: u32,
    fill_timer: u32,
    proof_timer: u32,
}

static WS: Mutex<Option<Ws>> = Mutex::new(None);

fn ws<R>(f: impl FnOnce(&mut Ws) -> R) -> R {
    let mut g = WS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Ws::default))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    ws(|w| {
        w.cooldowns.retain_mut(|(_, t)| {
            *t = t.saturating_sub(1);
            *t > 0
        });
        w.flame_ticks += 1;
    });
    if !in_world(game) {
        return;
    }
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    if let Some(m) = get("Auto Shearer") {
        auto_shearer(game, m);
    }
    if let Some(m) = get("Auto Nametag") {
        auto_nametag(game, m);
    }
    if let Some(m) = get("Auto Mount") {
        auto_mount(game, m);
    }
    if let Some(m) = get("Flamethrower") {
        flamethrower(game, m);
    }
    if let Some(m) = get("Bow Aimbot") {
        bow_aimbot(game, m);
    }
    if let Some(m) = get("Liquid Filler") {
        liquid_filler(game, m);
    }
    if let Some(m) = get("Spawn Proofer") {
        spawn_proofer(game, m);
    }
}

// Within `range` of the player as the shape says: a ball, or a cube.
fn in_shape(shape: &str, me: [f64; 3], p: [i32; 3], range: f64) -> bool {
    let c = center_of(p);
    if shape == "Uniform cube" {
        (c[0] - me[0]).abs() <= range && (c[1] - me[1]).abs() <= range && (c[2] - me[2]).abs() <= range
    } else {
        dist(me, c) <= range
    }
}

fn liquid_filler(game: &mut Game<'_>, m: &Module) {
    if ws(|w| {
        if w.fill_timer < m.n("Delay (ticks)") as u32 {
            w.fill_timer += 1;
            true
        } else {
            w.fill_timer = 0;
            false
        }
    }) {
        return;
    }
    let list = if m.p("List mode") == "Whitelist" { m.t("Whitelist") } else { m.t("Blacklist") };
    let white = m.p("List mode") == "Whitelist";
    let slot = find_hotbar(game, |n| {
        let is_block = game.res.items().get(&format!("minecraft:{n}")).is_some_and(|i| i.block.is_some());
        is_block && (listed(&list, n) == white)
    });
    let Some(slot) = slot else {
        return;
    };
    let (Some(eye), Some(at)) = (game.eye_position(), block_pos(game)) else {
        return;
    };
    let (range, walls) = (m.n("Place range"), m.n("Walls range"));
    let shape = m.p("Shape");
    let which = m.p("Place in");
    let (rh, rv) = ((range + 1.0).ceil() as i32, range.ceil() as i32);
    let mut found: Vec<[i32; 3]> = Vec::new();
    for x in at[0] - rh..=at[0] + rh {
        for y in at[1] - rv..=at[1] + rv {
            for z in at[2] - rh..=at[2] + rh {
                let p = [x, y, z];
                // Reject the unused corners before any world/entity lookup.
                if !in_shape(shape, eye, p, range) {
                    continue;
                }
                let name: &str = &facts(game, p).name;
                let ok = match which {
                    "Water" => name == "minecraft:water",
                    "Lava" => name == "minecraft:lava",
                    _ => name == "minecraft:water" || name == "minecraft:lava",
                };
                if !ok || !can_place(game, p, true) {
                    continue;
                }
                let c = center_of(p);
                let out = !in_shape(shape, eye, p, walls) && !clear_line(game, c, p);
                if !out {
                    found.push(p);
                }
            }
        }
    }
    match m.p("Sort") {
        "Top down" => found.sort_by_key(|p| -p[1]),
        "Bottom up" => found.sort_by_key(|p| p[1]),
        "Closest" => found.sort_by(|a, b| dist(eye, center_of(*a)).total_cmp(&dist(eye, center_of(*b)))),
        "Furthest" => found.sort_by(|a, b| dist(eye, center_of(*b)).total_cmp(&dist(eye, center_of(*a)))),
        _ => {}
    }
    let mut count = 0;
    for p in found {
        if count >= m.n("Max blocks per tick") as u32 {
            break;
        }
        if place_block(game, p, slot, m.b("Rotate"), true) {
            count += 1;
        }
    }
}

fn spawn_proofer(game: &mut Game<'_>, m: &Module) {
    if ws(|w| {
        if w.proof_timer < m.n("Place delay") as u32 {
            w.proof_timer += 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let list = m.t("Blocks");
    let Some(slot) = find_hotbar(game, |n| listed(&list, n)) else {
        report(game, "None of the chosen blocks are in your hotbar.");
        toggle_off(game, "Spawn Proofer");
        return;
    };
    let (Some(eye), Some(at)) = (game.eye_position(), block_pos(game)) else {
        return;
    };
    let (range, walls) = (m.n("Place range"), m.n("Walls range"));
    let limit = m.n("Light level") as u8;
    let mode = m.p("Mode");
    let r = range.ceil() as i32;
    let mut spots: Vec<([i32; 3], u8)> = Vec::new();
    for x in at[0] - r..=at[0] + r {
        for y in at[1] - r..=at[1] + r {
            for z in at[2] - r..=at[2] + r {
                let p = [x, y, z];
                let c = center_of(p);
                let distance = dist(eye, c);
                if distance > range {
                    continue;
                }
                if !is_air(game, p) {
                    continue;
                }
                let below = [x, y - 1, z];
                let all = game.res.facts();
                let state = game.world.block(below[0], below[1], below[2]);
                let f = all.get(state);
                if all.is_air(state) || !f.solid_render || f.name.ends_with("bedrock") || f.name.ends_with("barrier") {
                    continue;
                }
                let light = game.world.light(x, y, z);
                let (block, sky) = (light & 0x0F, light >> 4);
                if block > limit {
                    continue;
                }
                let always = sky <= limit;
                if (always && mode == "Potential") || (!always && mode == "Always") {
                    continue;
                }
                if !can_place(game, p, true) {
                    continue;
                }
                if distance > walls && !clear_line(game, c, p) {
                    continue;
                }
                spots.push((p, light));
            }
        }
    }
    if spots.is_empty() {
        return;
    }
    // A light source goes in one a tick, the darkest first.
    let light_source = game
        .inventory
        .get(slot)
        .and_then(|s| s.as_ref())
        .and_then(|s| game.res.items().get(&s.item))
        .and_then(|i| i.block)
        .is_some_and(|b| game.res.facts().get(b).emission > 0);
    let mut placed = 0;
    if light_source {
        spots.sort_by_key(|(_, l)| (l & 0x0F).max(l >> 4));
        placed = m.n("Blocks per tick") as u32 - 1;
    }
    for (p, _) in spots {
        if placed >= m.n("Blocks per tick") as u32 {
            break;
        }
        if place_block(game, p, slot, m.b("Rotate"), true) {
            placed += 1;
        }
    }
    ws(|w| w.proof_timer = 0);
}

// The tool in the hotbar for entity `want`, switched to now.
fn use_on(game: &mut Game<'_>, slot: usize, id: u32, centre: [f64; 3], rotate: bool) {
    turned_to(game, "Use on", rotate, centre, move |game| {
        let before = game.selected;
        hold(game, slot);
        game.send(S::Interact(play::Interact { id: id as i32, hand: 0, location: [0.0, 1.0, 0.0], secondary: false }));
        game.swing_arm();
        hold(game, before);
    });
}

fn cooling(id: u32) -> bool {
    ws(|w| w.cooldowns.iter().any(|(i, _)| *i == id))
}

fn cool(id: u32, ticks: u32) {
    ws(|w| w.cooldowns.push((id, ticks)));
}

fn centre(t: &Target) -> [f64; 3] {
    [(t.lo[0] + t.hi[0]) / 2.0, (t.lo[1] + t.hi[1]) / 2.0, (t.lo[2] + t.hi[2]) / 2.0]
}

fn auto_shearer(game: &mut Game<'_>, m: &Module) {
    let range = m.n("Distance");
    let anti = m.b("Anti break");
    let slot = find_hotbar(game, |n| n == "shears");
    let Some(slot) = slot else {
        return;
    };
    if anti {
        if let Some(s) = game.inventory.get(slot).and_then(|s| s.as_ref()) {
            let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
            if max > 0 && s.damage + 1 >= max {
                return;
            }
        }
    }
    let eye = game.eye_position().unwrap_or([0.0; 3]);
    let target = game
        .mobs
        .iter()
        .find(|s| {
            s.kind.id == "minecraft:sheep"
                && s.alive()
                && !s.removed
                && !s.extra.sheared
                && !s.is_baby()
                && !cooling(s.id)
                && dist(eye, [s.position.x, s.position.y, s.position.z]) <= range
        })
        .map(mob_target);
    if let Some(t) = target {
        use_on(game, slot, t.id, centre(&t), m.b("Rotate"));
        cool(t.id, 20);
    }
}

fn auto_nametag(game: &mut Game<'_>, m: &Module) {
    let Some(slot) = find_hotbar(game, |n| n == "name_tag") else {
        report(game, "No name tag in your hotbar.");
        toggle_off(game, "Auto Nametag");
        return;
    };
    let range = m.n("Range");
    let list = m.t("Entities");
    let eye = game.eye_position().unwrap_or([0.0; 3]);
    let rename = m.b("Renametag");
    // Inspect the mob itself: no Target allocation or second entity scan
    // for each candidate. min_by keeps the first entity for equal distances.
    let target = game.mobs.iter()
        .filter(|mob| mob.alive() && !mob.removed && mob.kind.id != "minecraft:player")
        .filter(|mob| (rename || mob.custom_name.is_none()) && listed(list, &mob.kind.id))
        .filter_map(|mob| {
            let distance = dist(eye, [mob.position.x, mob.position.y, mob.position.z]);
            (distance <= range && !cooling(mob.id)).then_some((distance, mob))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, mob)| mob_target(mob));
    if let Some(t) = target {
        use_on(game, slot, t.id, centre(&t), m.b("Rotate"));
        cool(t.id, 20);
    }
}

fn auto_mount(game: &mut Game<'_>, m: &Module) {
    if game.riding.is_some() || game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return;
    }
    if game.held().is_some_and(|s| short(&s.item).ends_with("_spawn_egg")) {
        return;
    }
    let list = m.t("Entities");
    let eye = game.eye_position().unwrap_or([0.0; 3]);
    let check_saddle = m.b("Check saddle");
    let pick = game.mobs.iter().filter(|mob| mob.alive() && !mob.removed).find(|mob| {
        if mob.kind.id == "minecraft:player" || dist(eye, [mob.position.x, mob.position.y, mob.position.z]) > 4.0 || cooling(mob.id) || !listed(list, &mob.kind.id) {
            return false;
        }
        // Saddled-only rides.
        let rideable_needs_saddle = ["pig", "skeleton_horse", "strider", "zombie_horse"].iter().any(|k| short(&mob.kind.id) == *k);
        if rideable_needs_saddle && !mob.extra.saddled {
            return false;
        }
        !(short(&mob.kind.id) != "llama" && check_saddle && !mob.extra.saddled)
    }).map(mob_target);
    if let Some(t) = pick {
        let id = t.id as i32;
        turned_to(game, "Auto Mount", m.b("Rotate"), centre(&t), move |game| {
            game.send(S::Interact(play::Interact { id, hand: 0, location: [0.0, 1.0, 0.0], secondary: false }));
            game.swing_arm();
        });
        cool(t.id, 20);
    }
}

fn flamethrower(game: &mut Game<'_>, m: &Module) {
    let range = m.n("Distance");
    let list = m.t("Entities");
    let anti = m.b("Anti break");
    let eye = game.eye_position().unwrap_or([0.0; 3]);
    let ticks = ws(|w| w.flame_ticks);
    if ticks < m.n("Tick interval") as u32 {
        return;
    }
    let babies = m.b("Target babies");
    let target = game.mobs.iter().find(|mob| {
        mob.alive() && !mob.removed && mob.kind.id != "minecraft:player"
            && dist(eye, [mob.position.x, mob.position.y, mob.position.z]) <= range
            && listed(list, &mob.kind.id)
            && (babies || !mob.is_baby()) && !mob.is_on_fire()
    }).map(mob_target);
    let Some(t) = target else {
        return;
    };
    let slot = find_hotbar(game, |n| n == "flint_and_steel" || n == "fire_charge");
    let Some(slot) = slot else {
        return;
    };
    if anti {
        if let Some(s) = game.inventory.get(slot).and_then(|s| s.as_ref()) {
            let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
            if max > 0 && s.damage + 1 >= max {
                return;
            }
        }
    }
    let Some((below, point)) = fire_spot(game, eye, &t) else {
        return;
    };
    // The look goes to the point clicked, as a real click's does: a look through the mob is a click on the mob, and
    // Horizon's server leaves a block use alone while you look at one.
    let cursor = [(point[0] - f64::from(below[0])) as f32, 1.0, (point[2] - f64::from(below[2])) as f32];
    turned_to(game, "Flamethrower", m.b("Rotate"), point, move |game| {
        let before = game.selected;
        hold(game, slot);
        let seq = sequence(game);
        game.send(S::UseItemOn(play::UseItemOn { hand: 0, pos: below, face: 1, cursor, inside: false, world_border: false, sequence: seq }));
        game.swing_arm();
        hold(game, before);
    });
    ws(|w| w.flame_ticks = 0);
}

// Where to click to set `t` alight: the top of a block under its feet with air above (where the fire goes), at the
// point nearest your eyes that the mob does not hide and that is within reach. The block and the point.
fn fire_spot(game: &Game<'_>, eye: [f64; 3], t: &Target) -> Option<([i32; 3], [f64; 3])> {
    let y = t.lo[1].floor() as i32;
    let grown = (t.lo.map(|v| v - 0.05), t.hi.map(|v| v + 0.05));
    let mut best: Option<(f64, [i32; 3], [f64; 3])> = None;
    for x in t.lo[0].floor() as i32..=(t.hi[0] - 1e-6).floor() as i32 {
        for z in t.lo[2].floor() as i32..=(t.hi[2] - 1e-6).floor() as i32 {
            let below = [x, y - 1, z];
            if !is_air(game, [x, y, z]) || is_air(game, below) || !facts(game, below).full_collision {
                continue;
            }
            let (fx, fz) = (f64::from(x), f64::from(z));
            let nearest = [eye[0].clamp(fx + 0.01, fx + 0.99), eye[2].clamp(fz + 0.01, fz + 0.99)];
            let points = [0.01, 0.5, 0.99].into_iter().flat_map(|a| [0.01, 0.5, 0.99].map(|b| [fx + a, fz + b]));
            for [px, pz] in points.chain([nearest]) {
                let point = [px, f64::from(y), pz];
                let d = dist(eye, point);
                if d > 4.5 || best.is_some_and(|b| b.0 <= d) || crosses(eye, point, grown.0, grown.1) {
                    continue;
                }
                best = Some((d, below, point));
            }
        }
    }
    best.map(|(_, below, point)| (below, point))
}

// Whether the segment from `a` to `b` passes through the box from `lo` to `hi`.
fn crosses(a: [f64; 3], b: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> bool {
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for i in 0..3 {
        let d = b[i] - a[i];
        if d.abs() < 1e-12 {
            if a[i] < lo[i] || a[i] > hi[i] {
                return false;
            }
            continue;
        }
        let (u, v) = ((lo[i] - a[i]) / d, (hi[i] - a[i]) / d);
        t0 = t0.max(u.min(v));
        t1 = t1.min(u.max(v));
        if t0 > t1 {
            return false;
        }
    }
    true
}

// ---- Bow Aimbot

fn bow_aimbot(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !(game.physical_buttons.contains(&MouseButton::Right) || game.right) || !game.native_playing() {
        return;
    }
    let Some(stack) = game.held() else {
        return;
    };
    let item = short(&stack.item);
    if item != "bow" && item != "crossbow" {
        return;
    }
    if !game.creative && find_inventory(game, |n| n.ends_with("arrow")).is_none() {
        return;
    }
    let eye = match game.eye_position() {
        Some(e) => e,
        None => return,
    };
    let range = m.n("Range");
    let list = m.t("Entities");
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let friends = super::sunset::with(|s| s.friends.clone());
    let (nametagged, babies) = (m.b("Nametagged"), m.b("Babies"));
    let mut cands: Vec<Target> = game.mobs.iter()
        .filter(|mob| mob.alive() && !mob.removed && Some(mob.id) != own)
        .filter(|mob| dist(eye, [mob.position.x, mob.position.y, mob.position.z]) <= range && listed(list, &mob.kind.id))
        .filter(|mob| (nametagged || mob.custom_name.is_none()) && (babies || !mob.is_baby()))
        .filter(|mob| !(mob.kind.id == "minecraft:player" && game.remote_player_name(mob.id).is_some_and(|n| friends.iter().any(|f| f.eq_ignore_ascii_case(n)))))
        .map(mob_target)
        .filter(|t| clear_line(game, centre(t), [i32::MIN; 3]))
        .collect();
    sort_targets(game, &mut cands, m.p("Priority"));
    let Some(t) = cands.into_iter().next() else {
        return;
    };
    let ticks = game.using.map_or(0, |(_, t)| t);
    let power = {
        let f = f64::from(ticks) / 20.0;
        ((f * f + f * 2.0) / 3.0).min(1.0) as f32
    };
    let (rx, rz) = (t.pos[0] - eye[0], t.pos[2] - eye[2]);
    let ry = t.pos[1] + (t.hi[1] - t.lo[1]) / 2.0 - eye[1];
    let h = (rx * rx + rz * rz).sqrt();
    // Meteor's ballistic pitch: g = 0.006 per tick per tick squared, speed in blocks.
    let g = 0.006;
    let v = f64::from(power);
    let v2 = v * v;
    let pitch = -((v2 - (v2 * v2 - g * (g * h * h + 2.0 * ry * v2)).sqrt()) / (g * h)).atan().to_degrees();
    let (yaw, flat_pitch) = super::sunset::look_at(eye, [t.pos[0], t.pos[1] + (t.hi[1] - t.lo[1]) / 2.0, t.pos[2]]);
    game.yaw = yaw;
    game.pitch = if pitch.is_nan() { flat_pitch } else { pitch as f32 };
}

#[allow(dead_code)]
fn unused(_: Screen) {}
