// Kill Aura, ported from Meteor's source with its settings: which weapons it
// attacks with, how it rotates and switches, who it targets and how many at
// once, and its timing.

use super::sunset::{Canvas, Category, Module, health_colour, hold, look_at, toggle_named};
use super::sunset_combat_control as control;
use super::sunset_rotation::{direction, track};
use super::sunset_util::*;
use super::Game;
use horizon_client::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::Vec3;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Kill Aura", Category::Combat, "Attacks the entities you choose around you.")
            .pick("Attack when holding", &["Weapons", "All"])
            .text("Selected weapon types", "diamond_sword,diamond_axe,trident,mace,diamond_spear")
            .pick("Rotate", &["Always", "On hit", "None"])
            .pick("Rotation mode", &["Snap", "Normal", "Smooth", "Custom"])
            .pick("Rotation view", &["Silent", "Visible"])
            .num("Yaw speed", 60.0, 5.0, 180.0, 5.0)
            .num("Pitch speed", 60.0, 5.0, 180.0, 5.0)
            .num("Smoothing", 0.0, 0.0, 0.9, 0.05)
            .bool("Ray cast", false)
            .bool("Auto switch", false)
            .bool("Swap back", false)
            .bool("Only on click", false)
            .bool("Only on look", false)
            .bool("Auto disable", false)
            .text("Entities", "player")
            .pick("Priority", PRIORITIES)
            .bool("Hurt time priority", false)
            .num("Target FOV", 360.0, 10.0, 360.0, 5.0)
            // Loaded before Attack mode: a config older than it says Multi by a Max targets above 1 (see set_field).
            .num("Max targets", 1.0, 1.0, 5.0, 1.0)
            .pick("Attack mode", &["Single", "Switch", "Multi"])
            .bool("Target lock", false)
            .num("Target switch delay", 0.0, 0.0, 40.0, 1.0)
            .num("Range", 4.5, 1.0, 6.0, 0.1)
            .num("Walls range", 3.5, 1.0, 6.0, 0.1)
            .pick("Passive mob age filter", &["Adult", "Baby", "Both"])
            .pick("Hostile mob age filter", &["Both", "Adult", "Baby"])
            .bool("Ignore named", false)
            .bool("Ignore passive", true)
            .bool("Anti bot", false)
            .bool("Ignore teams", false)
            .bool("Ignore invisible", false)
            .num("Health threshold", 0.0, 0.0, 100.0, 1.0)
            .bool("Pause on use", false)
            .pick("Attack timing", &["Vanilla cooldown", "Custom delay", "Hit select"])
            .num("Hit delay", 11.0, 0.0, 60.0, 1.0)
            .num("Weapon switch delay", 0.0, 0.0, 10.0, 1.0),
    ]
}

// Who the shared hand ownership knows it by.
const HAND: &str = "Kill Aura";

#[derive(Default)]
struct As {
    hit_timer: u32,
    switch_timer: u32,
    last_selected: Option<usize>,
    // Swap back: the slot it switched from, and the one it switched to. You changing slot since makes it yours.
    swapped: bool,
    prev: usize,
    given: usize,
    // The hand, for the hit just made: it goes back with the hit.
    hit: Option<control::Finish>,
    // Why it did what it did last tick (.auradebug).
    reason: &'static str,
    attacking: bool,
    // Who it is hitting this tick (the Target HUD's).
    target: Option<u32>,
    // The target it stays on or is switching from, and the ticks before it may hit a new one.
    current: Option<u32>,
    target_timer: u32,
    // Switch: the last hit landed, so the next target's turn.
    switch_next: bool,
    // The look the rotation has reached.
    look: Option<(f32, f32)>,
    // Who it aimed at last tick and the look that would have met them.
    aimed: Option<(u32, (f32, f32))>,
    // Your entity id, which changes with the world (Auto disable).
    own_id: Option<u32>,
}

static AS: Mutex<Option<As>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut As) -> R) -> R {
    let mut g = AS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(As::default))
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
    let Some(item) = item else {
        return false;
    };
    let Some(t) = weapon_type(item) else {
        return false;
    };
    m.t("Selected weapon types").split(',').any(|w| weapon_type(w.trim()) == Some(t))
}

// A hotbar slot it may hit with: not an item about to break for nothing, nor one that is broken.
fn fit_slot(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    match game.inventory.get(slot).and_then(Option::as_ref) {
        Some(s) => {
            acceptable(m, Some(&s.item))
                && s.count > 0
                && game.res.items().get(&s.item).is_some_and(|i| i.max_damage == 0 || s.damage < i.max_damage)
        }
        None => acceptable(m, None),
    }
}

// What to hit with: what you hold if it will do, else with Auto switch the first in the hotbar that will.
fn weapon_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    if fit_slot(game, m, game.selected) {
        return Some(game.selected);
    }
    let held = |slot: usize| game.inventory.get(slot).is_some_and(Option::is_some);
    m.b("Auto switch").then(|| (0..9).find(|&slot| held(slot) && fit_slot(game, m, slot))).flatten()
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

// How far the eyes are from the nearest point of the target's box: what Range and Walls range measure.
fn reach(eye: [f64; 3], t: &Target) -> f64 {
    dist(eye, [eye[0].clamp(t.lo[0], t.hi[0]), eye[1].clamp(t.lo[1], t.hi[1]), eye[2].clamp(t.lo[2], t.hi[2])])
}

fn centre(t: &Target) -> [f64; 3] {
    [(t.lo[0] + t.hi[0]) / 2.0, (t.lo[1] + t.hi[1]) / 2.0, (t.lo[2] + t.hi[2]) / 2.0]
}

// In plain sight: a clear line to its head, middle or feet, so someone half behind a block still counts.
fn in_sight(game: &Game<'_>, t: &Target) -> bool {
    let c = centre(t);
    [c, [c[0], t.hi[1] - 0.1, c[2]], [c[0], t.lo[1] + 0.1, c[2]]].iter().any(|&p| clear_line(game, p, [i32::MIN; 3]))
}

// Within Target FOV of where you are really looking.
fn in_fov(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3]) -> bool {
    let fov = m.n("Target FOV");
    if fov >= 360.0 {
        return true;
    }
    let c = centre(t);
    let to = [c[0] - eye[0], c[1] - eye[1], c[2] - eye[2]];
    let d = dist(eye, c).max(1e-6);
    let view = direction((game.yaw, game.pitch));
    let cos = (view[0] * to[0] + view[1] * to[1] + view[2] * to[2]) / d;
    cos.clamp(-1.0, 1.0).acos().to_degrees() <= fov / 2.0
}

// Does a ray along `look` meet the target's box within Range, with nothing solid before it beyond Walls range?
fn ray_hits(game: &Game<'_>, m: &Module, eye: [f64; 3], look: (f32, f32), t: &Target) -> bool {
    use horizon_core::geometry::{Aabb, Vec3};
    let dir = direction(look);
    let b = Aabb::new(Vec3::new(t.lo[0], t.lo[1], t.lo[2]), Vec3::new(t.hi[0], t.hi[1], t.hi[2]));
    let Some(d) = super::ray_box(eye, dir, m.n("Range"), b) else {
        return false;
    };
    d <= m.n("Walls range") || clear_line(game, [eye[0] + dir[0] * d, eye[1] + dir[1] * d, eye[2] + dir[2] * d], [i32::MIN; 3])
}

// A spear stabs along the look the server has, and through everyone on it: does that reach the target, within its own
// reach and Range, without going through a friend too?
fn stab_hits(game: &Game<'_>, m: &Module, eye: [f64; 3], look: (f32, f32), t: &Target) -> bool {
    let dir = direction(look);
    let (min, max) = if game.creative { (REACH.min_creative, REACH.max_creative) } else { (REACH.min, REACH.max) };
    let hits = hits_in_reach(
        &game.hittable_mob_boxes(),
        Vec3::new(eye[0], eye[1], eye[2]),
        Vec3::new(dir[0], dir[1], dir[2]),
        game.known_player_movement(),
        (min, max.min(m.n("Range") as f32), REACH.hitbox_margin),
        |from, to| super::spear::sunset_combat_collision::clip(game, from, to),
    );
    let friends = super::sunset::with(|s| s.friends.clone());
    let friend = |id: u32| game.remote_player_name(id).is_some_and(|n| friends.iter().any(|f| f.eq_ignore_ascii_case(n)));
    hits.iter().any(|(i, _)| game.mobs[*i].id == t.id) && !hits.iter().any(|(i, _)| friend(game.mobs[*i].id))
}

fn entity_check(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3]) -> bool {
    if t.health <= 0.0 || spared(game, m, t.id, t.player) {
        return false;
    }
    if reach(eye, t) > m.n("Range") || !entity_listed(m.t("Entities"), t) || !in_fov(game, m, t, eye) {
        return false;
    }
    let threshold = m.n("Health threshold");
    if threshold > 0.0 && f64::from(t.health) > threshold {
        return false;
    }
    let mob = game.mobs.iter().find(|x| x.id == t.id);
    if m.b("Ignore named") && mob.is_some_and(|x| x.custom_name.is_some()) {
        return false;
    }
    if m.b("Ignore invisible") && mob.is_some_and(|x| x.invisible) {
        return false;
    }
    if reach(eye, t) > m.n("Walls range") && !in_sight(game, t) {
        return false;
    }
    let k = short(&t.kind);
    if m.b("Ignore passive") && matches!(k, "enderman" | "piglin" | "zombified_piglin" | "wolf") {
        return false;
    }
    let baby = mob.is_some_and(|x| x.is_baby());
    if matches!(k, "zombie" | "piglin" | "hoglin" | "zoglin") {
        return age_ok(m.p("Hostile mob age filter"), baby);
    }
    if ageable(k) {
        return age_ok(m.p("Passive mob age filter"), baby);
    }
    true
}

fn stop(game: &mut Game<'_>, m: &Module) {
    let (was, swapped, prev, given) = st(|s| {
        s.current = None;
        s.target_timer = 0;
        s.switch_next = false;
        s.look = None;
        s.aimed = None;
        (std::mem::take(&mut s.attacking), std::mem::take(&mut s.swapped), s.prev, s.given)
    });
    if was && swapped && m.b("Swap back") && game.selected == given && control::owns(HAND) {
        hold(game, prev);
    }
    control::release(HAND);
}

// Turned off: back to the weapon it swapped from, and nothing kept for next time.
pub(super) fn disabled(game: &mut Game<'_>, swap_back: bool) {
    let (swapped, prev, given) = st(|s| {
        let was = (s.swapped, s.prev, s.given);
        *s = As::default();
        was
    });
    if swap_back && swapped && game.selected == given {
        hold(game, prev);
    }
    control::release(HAND);
}

// Who to hit this tick: Multi all it may, Switch each in turn, Single one (the same one with Target lock).
fn choose(m: &Module, mut list: Vec<Target>) -> Vec<Target> {
    let mode = m.p("Attack mode");
    if mode == "Multi" {
        list.truncate(m.n("Max targets") as usize);
        return list;
    }
    let (current, next) = st(|s| (s.current, std::mem::take(&mut s.switch_next)));
    let at = current.and_then(|id| list.iter().position(|t| t.id == id));
    let pick = match (mode, at) {
        ("Switch", Some(i)) if next => (i + 1) % list.len(),
        ("Switch", Some(i)) => i,
        ("Single", Some(i)) if m.b("Target lock") => i,
        _ => 0,
    };
    vec![list.swap_remove(pick)]
}

pub(super) fn tick(game: &mut Game<'_>, m: &Module, crits: bool) {
    st(|s| s.target = None);
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    // A hit on its way: nothing else may take the hand before it goes.
    if super::sunset::CRIT_STAGE.load(Ordering::Relaxed) != 0 || super::sunset::strike_waiting() {
        let _ = control::claim(HAND);
        st(|s| s.reason = "a hit is on its way");
        return;
    }
    // Auto disable: after you die, or in a new world.
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let moved = st(|s| s.own_id.replace(own.unwrap_or(0))).is_some_and(|was| Some(was) != own);
    if m.b("Auto disable") && (moved || game.player.as_ref().is_some_and(|p| p.health.health() <= 0.0)) {
        toggle_named(game, "Kill Aura", false);
        return;
    }
    // A change of hotbar slot starts the weapon switch delay.
    let sel = game.selected;
    if st(|s| s.last_selected.replace(sel)) != Some(sel) {
        st(|s| s.switch_timer = m.n("Weapon switch delay") as u32);
    }
    // The hit has gone, and the hand with it, unless it was lost on the way (a new world, say).
    if st(|s| s.hit.take()).is_some_and(|hit| !hit.current()) {
        st(|s| s.reason = "lost the hand before the hit went");
    }
    // The hand: kept only for a weapon to swap back from, and left alone while another module has it.
    if !st(|s| s.swapped) {
        control::release(HAND);
    }
    if !control::available(HAND) || !super::sunset::inventory_idle(game) {
        stop(game, m);
        st(|s| s.reason = "another module has the hand, or a menu is open");
        return;
    }
    if game.player.as_ref().is_none_or(|p| p.health.health() <= 0.0) || game.spectator() {
        stop(game, m);
        return;
    }
    if m.b("Pause on use") && (game.using.is_some() || game.mining.is_some()) {
        stop(game, m);
        return;
    }
    if m.b("Only on click") && !(game.left || game.physical_buttons.contains(&winit::event::MouseButton::Left)) {
        stop(game, m);
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let friends = super::sunset::with(|s| s.friends.clone());
    let names = game.remote_player_names();
    let mut list: Vec<Target> = if m.b("Only on look") {
        game.mob_hit().map(|(i, _)| mob_target(&game.mobs[i])).into_iter().collect()
    } else {
        every_living(game)
    };
    list.retain(|t| {
        Some(t.id) != own
            && !(t.player && names.iter().any(|(i, n)| *i == t.id && friends.iter().any(|f| f.eq_ignore_ascii_case(n))))
            && entity_check(game, m, t, eye)
    });
    sort_targets(game, &mut list, m.p("Priority"));
    if m.b("Hurt time priority") {
        // Who can take a hit soonest; the sort keeps Priority's order among equals.
        let hurt = |t: &Target| game.mobs.iter().find(|x| x.id == t.id).map_or(0, |x| x.hurt_time);
        list.sort_by_key(hurt);
    }
    if list.is_empty() {
        stop(game, m);
        st(|s| s.reason = "no target");
        return;
    }
    // Pick the weapon: never out of a hand eating, blocking or mining.
    let Some(slot) = weapon_slot(game, m) else {
        stop(game, m);
        st(|s| s.reason = "no usable weapon");
        return;
    };
    if slot != game.selected {
        if game.using.is_some() || game.mining.is_some() || !control::claim(HAND) {
            st(|s| s.reason = "the hand is busy");
            return;
        }
        if m.b("Swap back") {
            st(|s| {
                if !s.swapped {
                    s.prev = game.selected;
                    s.swapped = true;
                }
                s.given = slot;
            });
        }
        hold(game, slot);
    }
    if st(|s| s.swapped) && !control::claim(HAND) {
        st(|s| s.reason = "the hand is busy");
        return;
    }
    let list = choose(m, list);
    let primary = list[0].clone();
    let delay = m.n("Target switch delay") as u32;
    let waiting = st(|s| {
        s.attacking = true;
        s.target = Some(primary.id);
        if s.current.replace(primary.id).is_some_and(|was| was != primary.id) && m.p("Attack mode") != "Multi" {
            s.target_timer = delay;
        }
        let wait = s.target_timer > 0;
        s.target_timer = s.target_timer.saturating_sub(1);
        wait
    });
    let rotate_mode = m.p("Rotate");
    // The look this tick: from where the last tick's got to (or your own), a step toward the target. Silent tells only
    // the server; Visible turns your view. A hit made now goes out before that look does, so it counts the last one.
    let sent = st(|s| s.look);
    if rotate_mode == "Always" {
        let from = sent.unwrap_or((game.yaw, game.pitch));
        // The look is checked next tick: aim at where you and the target will be by then, each moving as this tick.
        let yours = game.player.as_ref().filter(|_| game.riding.is_none()).map_or([0.0; 3], |p| {
            [p.position.x - game.previous_position.x, p.position.y - game.previous_position.y, p.position.z - game.previous_position.z]
        });
        let c = centre(&primary);
        let to = look_at(
            [eye[0] + yours[0], eye[1] + yours[1], eye[2] + yours[2]],
            [c[0] + primary.pos[0] - primary.prev[0], c[1] + primary.pos[1] - primary.prev[1], c[2] + primary.pos[2] - primary.prev[2]],
        );
        let was = st(|s| s.aimed.replace((primary.id, to))).filter(|a| a.0 == primary.id).map(|a| a.1);
        let next = track(m.p("Rotation mode"), from, to, was, m.n("Yaw speed") as f32, m.n("Pitch speed") as f32, m.n("Smoothing") as f32);
        if m.p("Rotation view") == "Visible" {
            (game.yaw, game.pitch) = next;
        } else {
            super::sunset::turn("Kill Aura", next.0, next.1);
        }
        st(|s| s.look = Some(next));
    }
    // Just found a target: its look has not gone out yet.
    if waiting || (rotate_mode == "Always" && sent.is_none()) {
        st(|s| s.reason = "turning to a new target");
        return;
    }
    // The delay: the game's cooldown, or a fixed number of ticks. Hit select also waits out the target's hurt time.
    if st(|s| {
        if s.switch_timer > 0 {
            s.switch_timer -= 1;
            true
        } else {
            false
        }
    }) {
        st(|s| s.reason = "weapon switch delay");
        return;
    }
    let timing = m.p("Attack timing");
    let ready = if timing == "Custom delay" {
        st(|s| {
            if s.hit_timer < m.n("Hit delay") as u32 {
                s.hit_timer += 1;
                false
            } else {
                true
            }
        })
    } else {
        game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 1.0
    };
    let spear = game.held().is_some_and(|s| Kinetic::of(&s.item).is_some());
    if !ready || (spear && game.cannot_attack_with_item(game.hand_strength.ticker, 0)) {
        st(|s| s.reason = "cooling down");
        return;
    }
    if !control::claim(HAND) {
        st(|s| s.reason = "the hand is busy");
        return;
    }
    // With Always a hit waits until the look the server has meets the target, whether Ray cast is on or not.
    let check = m.b("Ray cast") || rotate_mode == "Always";
    let mut hit = false;
    st(|s| s.reason = "the look does not meet the target");
    // A stab is one action, whoever it goes through.
    for t in list.iter().take(if spear { 1 } else { usize::MAX }) {
        if timing == "Hit select" && game.mobs.iter().any(|x| x.id == t.id && x.hurt_time > 0) {
            continue;
        }
        let on_hit = (rotate_mode == "On hit").then(|| look_at(eye, centre(t)));
        let aim = on_hit.or(sent).unwrap_or((game.yaw, game.pitch));
        if if spear { !stab_hits(game, m, eye, aim, t) } else { check && !ray_hits(game, m, eye, aim, t) } {
            continue;
        }
        super::sunset::attack(game, t.id, crits, on_hit);
        hit = true;
    }
    if hit {
        st(|s| {
            s.hit_timer = 0;
            s.switch_next = true;
            s.reason = "hit sent";
            s.hit = Some(control::finish(HAND));
        });
    }
}

pub(super) fn diagnostics() -> String {
    st(|s| format!("KillAura reason={} target={:?} hand={:?}", s.reason, s.target, control::owner()))
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
