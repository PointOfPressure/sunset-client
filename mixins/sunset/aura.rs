// Kill Aura, Rust's from PR #16: one target, hit at full cooldown with the look the server has (a turn of at most
// 25 degrees a tick, in whole mouse steps, never moving your camera), only when that look really meets the target's
// box with nothing in the way; Crits waits for the fall that makes a hit critical; Mace, Shield Breaker and spears
// use their own mechanics. Kept from the Meteor port before it: the entity list and filters, Range, Walls range,
// Anti bot, Ignore teams, Only on click/look, Pause on use and the weapon check.
use super::Game;
use super::sunset::{Canvas, Category, Module, health_colour, hold, look_at};
use super::sunset_combat_control as control;
use super::sunset_melee as melee;
use super::sunset_melee_policy as policy;
use super::sunset_util::*;
use horizon_client::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::{Aabb, Vec3};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Kill Aura", Category::Combat, "Attacks the entities you choose around you at full cooldown, turning the look the server has (not your camera) onto them and hitting only when it meets them. Crits: Wait holds a hit while you rise so it lands falling, Jump jumps for it; with Criticals on, Criticals does it instead.")
            .text("Entities", "player")
            .pick("Priority", &["Distance", "Health", "Angle"])
            .pick("Crits", &["Wait", "Off", "Jump"])
            .num("Range", 4.5, 1.0, 6.0, 0.1)
            .num("Walls range", 3.5, 0.0, 6.0, 0.1)
            .pick("Attack when holding", &["Weapons", "All"])
            .text("Selected weapon types", "diamond_sword,diamond_axe,trident,mace,iron_spear")
            .bool("Only on click", false)
            .bool("Only on look", false)
            .pick("Passive mob age filter", &["Adult", "Baby", "Both"])
            .pick("Hostile mob age filter", &["Both", "Adult", "Baby"])
            .bool("Ignore named", false)
            .bool("Ignore passive", true)
            .bool("Anti bot", false)
            .bool("Ignore teams", false)
            .bool("Pause on use", false),
    ]
}

type Connection = Arc<dyn std::any::Any + Send + Sync>;

// The world a target was picked in: the connection, the dimension and your entity id.
#[derive(Clone)]
struct Scene {
    connection: Connection,
    dimension: super::Dim,
    own: i32,
}

fn scene(game: &Game<'_>) -> Option<Scene> {
    game.remote.as_ref().map(|r| Scene { connection: r.session.counters.clone() as Connection, dimension: game.dim, own: r.entity_id })
}

fn same(game: &Game<'_>, old: &Scene) -> bool {
    old.dimension == game.dim
        && game.remote.as_ref().is_some_and(|r| {
            r.entity_id == old.own && Arc::as_ptr(&old.connection) as *const () == Arc::as_ptr(&r.session.counters) as *const ()
        })
}

#[derive(Default)]
struct As {
    target: Option<u32>,
    scene: Option<Scene>,
    critical: super::sunset_aura_policy::CriticalWindow,
    // A hit waiting for its look to reach the server.
    pending: Option<Instant>,
}

static AS: Mutex<Option<As>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut As) -> R) -> R {
    let mut s = AS.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(As::default))
}

pub(super) fn reset() {
    super::sunset::cancel_turns("Kill Aura");
    st(|s| {
        s.target = None;
        s.pending = None;
        s.scene = None;
        s.critical.reset();
    });
}

pub(super) fn disabled(game: &mut Game<'_>) {
    reset();
    melee::cancel_owner(game, "Kill Aura");
}

// Who Kill Aura is hitting (Target ESP marks them).
pub(super) fn target_id() -> Option<u32> {
    st(|s| s.target)
}

// Kill Aura is on with these very settings still.
fn current(m: &Module) -> bool {
    super::sunset::with(|s| s.module("Kill Aura").is_some_and(|now| now.on && now == m))
}

fn allowed(game: &Game<'_>, m: &Module) -> bool {
    melee::attack_allowed(game, "Kill Aura")
        && game.attack_clicks == 0
        && !super::sunset_render::camera_takes_mouse()
        && !(m.b("Pause on use") && (game.using.is_some() || game.mining.is_some()))
        && (!m.b("Only on click") || game.left || game.physical_buttons.contains(&winit::event::MouseButton::Left))
}

// Which sort of tool an item is, so "diamond sword" in the list means any sword.
fn weapon_type(item: &str) -> Option<&'static str> {
    let n = short(item);
    Some(if n.ends_with("_sword") {
        "sword"
    } else if n.ends_with("_axe") {
        "axe"
    } else if n.ends_with("_pickaxe") {
        "pickaxe"
    } else if n.ends_with("_shovel") {
        "shovel"
    } else if n.ends_with("_hoe") {
        "hoe"
    } else if n.ends_with("_spear") {
        "spear"
    } else if n == "mace" {
        "mace"
    } else if n == "trident" {
        "trident"
    } else {
        return None;
    })
}

fn acceptable(m: &Module, item: Option<&str>) -> bool {
    if m.p("Attack when holding") == "All" {
        return true;
    }
    let Some(t) = item.and_then(weapon_type) else {
        return false;
    };
    m.t("Selected weapon types").split(',').any(|w| weapon_type(w.trim()) == Some(t))
}

pub(super) fn is_hostile(kind: &str) -> bool {
    const H: [&str; 24] = [
        "zombie", "husk", "drowned", "zombie_villager", "skeleton", "stray", "bogged", "wither_skeleton", "creeper", "spider", "cave_spider",
        "witch", "blaze", "ghast", "slime", "magma_cube", "phantom", "pillager", "vindicator", "evoker", "ravager", "vex", "guardian", "silverfish",
    ];
    let k = short(kind);
    H.contains(&k) || k == "piglin_brute" || k == "hoglin" || k == "zoglin" || k == "endermite" || k == "warden"
}

// The entity list setting: names, plus @all, @player, @hostile and @passive.
pub(super) fn entity_listed(list: &str, t: &Target) -> bool {
    list.split(',').map(str::trim).any(|tok| match tok {
        "@all" => true,
        "@player" => t.player,
        "@hostile" => is_hostile(&t.kind),
        "@passive" => !t.player && !is_hostile(&t.kind),
        _ => short(tok) == short(&t.kind),
    })
}

fn ageable(kind: &str) -> bool {
    const A: [&str; 22] = [
        "pig", "cow", "sheep", "chicken", "horse", "donkey", "mule", "llama", "rabbit", "wolf", "cat", "fox", "panda", "polar_bear", "goat",
        "camel", "mooshroom", "villager", "turtle", "axolotl", "bee", "sniffer",
    ];
    A.contains(&short(kind))
}

fn age_ok(filter: &str, baby: bool) -> bool {
    match filter {
        "Adult" => !baby,
        "Baby" => baby,
        _ => true,
    }
}

// From the eyes to the nearest point of the target's box: what Range measures.
fn reach(eye: [f64; 3], t: &Target) -> f64 {
    policy::box_distance(eye, t.lo, t.hi).unwrap_or(f64::INFINITY)
}

// Nothing solid between two points, through loaded chunks only.
fn clear(game: &Game<'_>, from: Vec3, to: Vec3) -> bool {
    if ![from.x, from.y, from.z, to.x, to.y, to.z].iter().all(|v| v.is_finite()) {
        return false;
    }
    let unknown = horizon_core::geometry::traverse_blocks(from, to, 512, |p: horizon_core::position::BlockPos| {
        (!game.world.loaded(p.x, p.z)).then_some(())
    });
    if !matches!(unknown, Ok(None)) {
        return false;
    }
    super::spear::sunset_combat_collision::clip(game, from, to)
        .is_none_or(|hit| from.distance_squared(hit) >= from.distance_squared(to) - 0.001)
}

// A point of the box to aim at that the eyes see: the nearest point of its core, else its middle high, half way or
// low. None: walled off.
fn point(game: &Game<'_>, eye: Vec3, raw: Aabb) -> Option<Vec3> {
    let size = raw.size();
    let core = raw.deflate(Vec3::new(0.15f64.min(size.x * 0.3), 0.2f64.min(size.y * 0.25), 0.15f64.min(size.z * 0.3)));
    let (a, b) = (core.min_position(), core.max_position());
    let near = Vec3::new(eye.x.clamp(a.x, b.x), eye.y.clamp(a.y, b.y), eye.z.clamp(a.z, b.z));
    if clear(game, eye, near) {
        return Some(near);
    }
    let (mid, lo) = (raw.center(), raw.min_position());
    [0.85, 0.5, 0.15].into_iter().map(|height| Vec3::new(mid.x, lo.y + size.y * height, mid.z)).find(|at| clear(game, eye, *at))
}

// The look meets the box within range (and, unless `walls`, before anything solid).
fn hits(game: &Game<'_>, eye: Vec3, raw: Aabb, range: f64, look: (f32, f32), walls: bool) -> bool {
    let v = super::view_vector(look.0, look.1);
    let end = eye.add_vector(Vec3::new(v[0], v[1], v[2]).scale(range));
    raw.clip(eye, end)
        .or_else(|| raw.contains(eye).then_some(eye))
        .is_some_and(|at| eye.distance_squared(at) <= range * range && (walls || clear(game, eye, at)))
}

// The box the look must meet: a little inside the real one.
fn hitbox(raw: Aabb) -> Aabb {
    let size = raw.size();
    raw.deflate(Vec3::new(0.03f64.min(size.x * 0.15), 0.01, 0.03f64.min(size.z * 0.15)))
}

fn wanted(game: &Game<'_>, m: &Module, mob: &horizon_client::mobs::Mob) -> bool {
    if !mob.alive()
        || mob.removed
        || !mob.health.is_finite()
        || game.riding == Some(mob.id)
        || mob.kind.id == "minecraft:armor_stand"
        || melee::wasted_on_shield(game, mob.id)
    {
        return false;
    }
    let t = mob_target(mob);
    if !entity_listed(m.t("Entities"), &t) || spared(game, m, mob.id, t.player) {
        return false;
    }
    if t.player {
        return player_enemy(game, mob);
    }
    if m.b("Ignore named") && mob.custom_name.is_some() {
        return false;
    }
    let k = short(&t.kind);
    if m.b("Ignore passive") && matches!(k, "enderman" | "piglin" | "zombified_piglin" | "wolf") && !mob.ai.aggressive {
        return false;
    }
    let baby = mob.is_baby();
    if matches!(k, "zombie" | "piglin" | "hoglin" | "zoglin") {
        return age_ok(m.p("Hostile mob age filter"), baby);
    }
    !ageable(k) || age_ok(m.p("Passive mob age filter"), baby)
}

// No point of `mob` in sight, but its feet within Walls range: aimed at its middle, hit through the wall.
fn through_walls(game: &Game<'_>, m: &Module, eye: Vec3, mob: &horizon_client::mobs::Mob) -> bool {
    point(game, eye, mob.bounds()).is_none() && eye.distance_squared(mob.position).sqrt() <= m.n("Walls range")
}

fn aim_point(game: &Game<'_>, m: &Module, eye: Vec3, mob: &horizon_client::mobs::Mob, raw: Aabb) -> Option<Vec3> {
    point(game, eye, raw).or_else(|| through_walls(game, m, eye, mob).then(|| raw.center()))
}

// The target: the one already hit if still in reach, else the best by Priority, the unhurt and the nearest first on
// a tie. With Only on look, only who the crosshair is on.
fn pick(game: &Game<'_>, m: &Module, eye: Vec3, range: f64) -> Option<u32> {
    if game.mobs.len() > policy::POPULATION_LIMIT {
        return None;
    }
    let look = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let previous = st(|s| s.target);
    let crosshair = m.b("Only on look").then(|| game.mob_hit().map(|(i, _)| game.mobs[i].id));
    let mut best: Option<(u32, f64, bool, f64)> = None;
    for mob in &game.mobs {
        if crosshair.is_some_and(|on| on != Some(mob.id)) || !wanted(game, m, mob) {
            continue;
        }
        let target = mob_target(mob);
        let d = reach([eye.x, eye.y, eye.z], &target);
        if d > range {
            continue;
        }
        let Some(at) = aim_point(game, m, eye, mob, mob.bounds()) else {
            continue;
        };
        if previous == Some(mob.id) {
            return previous;
        }
        let score = match m.p("Priority") {
            "Health" => super::sunset_prediction::effective_health(game, &target).unwrap_or(f64::from(mob.health)),
            "Angle" => {
                let a = look_at([eye.x, eye.y, eye.z], [at.x, at.y, at.z]);
                f64::from((a.0 - look.0 + 180.0).rem_euclid(360.0) - 180.0).hypot(f64::from(a.1 - look.1))
            }
            _ => d,
        };
        let candidate = (mob.id, score, mob.hurt_time > 0, d);
        if best.is_none_or(|old| score.total_cmp(&old.1).then(candidate.2.cmp(&old.2)).then(d.total_cmp(&old.3)).is_lt()) {
            best = Some(candidate);
        }
    }
    best.map(|x| x.0)
}

// The slot to hit with (Shield Breaker's axe or Mace's mace, else the hand) and whether it goes back after.
fn weapon(game: &Game<'_>, m: &Module, id: u32, spear: bool) -> Option<(usize, bool)> {
    if spear {
        return Some((game.selected, false));
    }
    let assist = melee::preferred_weapon(game, id);
    let slot = assist.map_or(game.selected, |p| p.slot);
    melee::slot_strength(game, slot, 0.5)?;
    let item = game.inventory.get(slot).and_then(|s| s.as_ref()).map(|s| &*s.item);
    acceptable(m, item).then_some((slot, assist.is_some_and(|p| p.restore)))
}

// A spear's stab along `look` passes through `id` and no player who is not an enemy.
fn spear_safe(game: &Game<'_>, id: u32, look: (f32, f32)) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let v = super::view_vector(look.0, look.1);
    let boxes: Vec<_> = game
        .mobs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.alive() && !m.removed && game.riding != Some(m.id))
        .map(|(i, m)| (i, m.bounds()))
        .collect();
    let range = if game.creative {
        (REACH.min_creative, REACH.max_creative, REACH.hitbox_margin)
    } else {
        (REACH.min, REACH.max, REACH.hitbox_margin)
    };
    let selected = hits_in_reach(
        &boxes,
        Vec3::new(eye[0], eye[1], eye[2]),
        Vec3::new(v[0], v[1], v[2]),
        game.known_player_movement(),
        range,
        |from, to| super::spear::sunset_combat_collision::clip(game, from, to),
    );
    selected.iter().any(|(i, _)| game.mobs[*i].id == id)
        && selected.iter().all(|(i, _)| {
            let m = &game.mobs[*i];
            m.kind.id != "minecraft:player" || player_enemy(game, m)
        })
}

fn airborne(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| {
        !p.ground.on_ground && !p.fluids.was_touching_water && p.last_climbable.is_none() && !p.flying && !p.fall_flying
    }) && game.riding.is_none()
        && game.potions.effects.get("blindness").is_none()
}

// Crits Wait: the hit waits (see CriticalWindow). Off, riding, or the Criticals module doing it: never.
fn critical_hold(game: &mut Game<'_>, m: &Module, jump: bool, crits: bool) -> bool {
    if crits || m.p("Crits") == "Off" || game.riding.is_some() {
        return false;
    }
    let air = airborne(game);
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let ground_jump = p.ground.on_ground && (jump || game.keys.contains(&winit::keyboard::KeyCode::Space));
    let result = st(|s| s.critical.plan(game.ticks, air, p.sprinting, p.fall_distance > 0.0, ground_jump));
    if result.drop_sprint {
        melee::drop_sprint(game);
    }
    result.hold
}

// How far a hit reaches: your entity interaction range (a spear's own reach), riding adds 2.5.
fn range(game: &mut Game<'_>, m: &Module, spear: bool) -> Option<f64> {
    let reach = if spear {
        f64::from(if game.creative { REACH.max_creative } else { REACH.max })
    } else {
        game.player.as_mut()?.attribute("minecraft:entity_interaction_range").ok()? + if game.riding.is_some() { 2.5 } else { 0.0 }
    } - 0.01;
    let reach = reach.min(m.n("Range"));
    (reach.is_finite() && reach > 0.0 && reach <= 64.0).then_some(reach)
}

// Asks for the look; once the server has it, checks everything again (under 200 ms, the same world, settings, slot
// and item, the target still wanted) and, with `attack`, hits.
fn queue(game: &mut Game<'_>, m: &Module, id: u32, look: (f32, f32), attack: bool, spear: bool, crits: bool) {
    if !control::claim("Kill Aura") {
        return;
    }
    let done = control::finish("Kill Aura");
    let Some(connection) = scene(game) else {
        return;
    };
    let expected = m.clone();
    let original = game.selected;
    let held = game.held().cloned();
    let queued = Instant::now();
    st(|s| s.pending = Some(queued));
    let accepted = super::sunset::try_turn_then("Kill Aura", look.0, look.1, move |game| {
        let done = done;
        if st(|s| s.pending) == Some(queued) {
            st(|s| s.pending = None);
        }
        let m = expected;
        if !attack
            || !done.current()
            || queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !current(&m)
            || !allowed(game, &m)
            || game.selected != original
            || game.held() != held.as_ref()
            || !game.mobs.iter().any(|mob| mob.id == id && wanted(game, &m, mob))
        {
            return;
        }
        let Some((slot, restore)) = weapon(game, &m, id, spear) else {
            return;
        };
        let smash = !spear && melee::smash_ready(game, id);
        if spear && (game.cannot_attack_with_item(game.hand_strength.ticker, 0) || !spear_safe(game, id, look)) {
            return;
        }
        if !spear && !smash && !melee::slot_strength(game, slot, 0.5).is_some_and(|scale| scale >= 1.0) {
            return;
        }
        if smash && melee::holds_smash(game, id) {
            melee::drop_sprint(game);
            return;
        }
        if !smash && !spear && critical_hold(game, &m, false, crits) {
            return;
        }
        let (Some(range), Some(eye)) = (range(game, &m, spear), game.eye_position()) else {
            return;
        };
        let eye = Vec3::new(eye[0], eye[1], eye[2]);
        let Some(target) = game.mobs.iter().find(|t| t.id == id) else {
            return;
        };
        let walls = through_walls(game, &m, eye, target);
        if !hits(game, eye, hitbox(target.bounds()), range, look, walls) || !super::remote::sunset_combat_remote::sprint_synced(game) {
            return;
        }
        // Criticals' hop or jump makes the hit, a tick or more on, with the slot as it is then.
        if crits && !spear && !smash {
            hold(game, slot);
            super::sunset::attack(game, id, true, Some(look));
            st(|s| s.critical.reset());
            return;
        }
        if slot != original && restore {
            if !melee::borrow_slot(game, "Kill Aura", slot, true, done) {
                return;
            }
        } else {
            hold(game, slot);
        }
        if melee::send_hit_at(game, id, look.0, look.1) {
            if smash {
                melee::note_smash();
            }
            st(|s| s.critical.reset());
        }
    });
    if !accepted {
        st(|s| s.pending = None);
    }
}

// `crits`: the Criticals module is on (and not just for your clicks).
pub(super) fn tick(game: &mut Game<'_>, m: &Module, crits: bool) {
    if !current(m) {
        disabled(game);
        return;
    }
    if !allowed(game, m) {
        super::sunset::cancel_turns("Kill Aura");
        st(|s| {
            s.target = None;
            s.pending = None;
            s.critical.reset();
        });
        return;
    }
    if !st(|s| s.scene.as_ref().is_some_and(|old| same(game, old))) {
        reset();
        st(|s| s.scene = scene(game));
    }
    if let Some(at) = st(|s| s.pending) {
        if at.elapsed() > Duration::from_millis(200) {
            reset();
        } else {
            control::claim("Kill Aura");
        }
        return;
    }
    let spear = game.held().is_some_and(|s| Kinetic::of(&s.item).is_some());
    let (Some(range), Some(at)) = (range(game, m, spear), game.eye_position()) else {
        return;
    };
    let eye = Vec3::new(at[0], at[1], at[2]);
    let Some(id) = pick(game, m, eye, range) else {
        st(|s| {
            s.target = None;
            s.critical.reset();
        });
        return;
    };
    st(|s| s.target = Some(id));
    let Some((slot, _)) = weapon(game, m, id, spear) else {
        return;
    };
    let scale = melee::slot_strength(game, slot, 0.5).unwrap_or(0.0);
    let smash = !spear && melee::smash_ready(game, id);
    let ready = if spear { !game.cannot_attack_with_item(game.hand_strength.ticker, 0) } else { smash || scale >= 1.0 };
    let delay = horizon_client::attack_strength::delay(game.main_hand_attack_speed());
    let ticks_left = ((1.0 - game.hand_strength.scale(0.5, game.main_hand_attack_speed())) * delay).ceil().max(0.0) as i32;
    let jump = !spear
        && !crits
        && m.p("Crits") == "Jump"
        && game.riding.is_none()
        && game.player.as_ref().is_some_and(|p| p.ground.on_ground && !p.fluids.was_touching_water)
        && ticks_left <= 7;
    if jump {
        melee::request_jump("Kill Aura");
    }
    let Some(target) = game.mobs.iter().find(|t| t.id == id) else {
        return;
    };
    let mut raw = target.bounds();
    // A tick from ready: aim where it will be.
    let aiming = !ready && ticks_left <= 1;
    if aiming {
        let delta = super::sunset_aura_policy::stride([
            target.position.x - target.previous.x,
            target.position.y - target.previous.y,
            target.position.z - target.previous.z,
        ]);
        raw = raw.move_by(Vec3::new(delta[0], delta[1], delta[2]));
    }
    let walls = through_walls(game, m, eye, target);
    let Some(at) = aim_point(game, m, eye, target, raw) else {
        return;
    };
    let desired = look_at([eye.x, eye.y, eye.z], [at.x, at.y, at.z]);
    let next = (!super::sunset_explosive_tactics::fixed_look(game) && !super::sunset::turn_pending())
        .then(|| super::sunset_explosive_tactics::next_look(game, desired, 0.0));
    let next_hits = next.is_some_and(|look| hits(game, eye, hitbox(raw), range, look, walls));
    if !ready {
        if let Some(look) = next.filter(|_| aiming && next_hits) {
            queue(game, m, id, look, false, spear, crits);
        }
        return;
    }
    let sent = if game.riding.is_some() {
        (game.yaw, game.pitch)
    } else {
        super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch))
    };
    let at_sent = hits(game, eye, hitbox(raw), range, sent, walls);
    // A big turn takes a few ticks, a look alone each; the hit only goes once the look meets the target.
    if !at_sent && !next_hits {
        if let Some(next) = next {
            queue(game, m, id, next, false, spear, crits);
        }
        return;
    }
    let hold = if smash { melee::holds_smash(game, id) } else { !spear && critical_hold(game, m, jump, crits) };
    if hold {
        if smash {
            melee::drop_sprint(game);
        }
        if let Some(look) = next.filter(|_| next_hits && !game.left && game.attack_clicks == 0) {
            queue(game, m, id, look, false, spear, crits);
        }
        return;
    }
    let look = if at_sent { sent } else { next.unwrap_or(sent) };
    if spear && !spear_safe(game, id, look) {
        return;
    }
    queue(game, m, id, look, true, spear, crits);
}

// The Target HUD: a box by the crosshair with the name, distance and health of who Kill Aura is hitting.
// Potato: the health bar alone, no text.
pub(super) fn draw_target(game: &Game<'_>, w: f32, h: f32, potato: bool, out: &mut impl Canvas) {
    let Some(id) = st(|s| s.target) else {
        return;
    };
    let (Some(mob), Some(eye)) = (game.mobs.iter().find(|x| x.id == id), game.eye_position()) else {
        return;
    };
    let fraction = (mob.health / mob.kind.max_health.max(1.0)).clamp(0.0, 1.0);
    let (x, y) = (w / 2.0 + 12.0, h / 2.0 + 12.0);
    if potato {
        out.fill([x, y, 60.0, 4.0], 0xFF30_3030);
        out.fill([x, y, 60.0 * fraction, 4.0], 0xFF00_0000 | health_colour(fraction));
        return;
    }
    let name = display_name(game, mob);
    let away = format!("{:.1}m", reach(eye, &mob_target(mob)));
    let health = format!("{:.1}", (mob.health * 10.0).round() / 10.0);
    let (nw, aw, hw) = (game.native_text_width(&name), game.native_text_width(&away), game.native_text_width(&health));
    let width = (nw + aw + 16.0).max(90.0);
    out.fill([x, y, width, 26.0], 0x9000_0000);
    out.text(&name, [x + 4.0, y + 3.0], 0xFFFFFF);
    out.text(&away, [x + width - 4.0 - aw, y + 3.0], 0xFFFFFF);
    let bar = width - 12.0 - hw;
    out.fill([x + 4.0, y + 17.0, bar, 4.0], 0xFF30_3030);
    out.fill([x + 4.0, y + 17.0, bar * fraction, 4.0], 0xFF00_0000 | health_colour(fraction));
    out.text(&health, [x + width - 4.0 - hw, y + 14.0], health_colour(fraction));
}
