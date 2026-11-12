// Kill Aura, implemented for Sunset with its settings: which weapons it
// attacks with, how it rotates and switches, who it targets and how many at
// once, and its timing. Its default Rotation mode, Silent aura, uses a silent look turned in whole
// mouse counts that the game's own move packet carries (see sunset_rotation). A hit goes within Attack range; with
// Ray cast, only once the look the server has, or the one sent right after the hit, meets the target. It leaves the
// look and the hand to Scaffold while Scaffold places.

use super::sunset::{Canvas, Category, Module, health_colour, hold, look_at, toggle_named};
use super::sunset_aura_aim as aim;
use super::sunset_aura_timing as timing;
use super::sunset_combat_control as control;
use super::sunset_rotation::{Managed, direction, track};
use super::sunset_util::*;
use super::Game;
use horizon_client::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::Vec3;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Kill Aura", Category::Combat, "Attacks nearby entities. Legacy AutoBlock is experimental and requires translated sword use.")
            // The menu shows these in sections (General, Targeting, Rotations, Timing, Inventory, Blocking, Movement);
            // saved settings go by name. Two orders matter on loading: Max targets before Attack mode, the Shield alias
            // before AutoBlock (set_field).
            .group("General")
            // Settings for a way of fighting, filled in when picked (preset); Custom leaves them.
            .pick("Preset", &["Custom", "HvH", "Blatant", "Ghost"])
            // How hits work on this server.
            .pick("Combat version", &["Auto", "Legacy (1.8)", "Modern (1.9+)"])
            // Range: who it may target and track. Attack range: how far a hit may go (the server's reach: Horizon's,
            // vanilla's, takes 3 blocks to the target's box).
            .num("Range", 4.5, 1.0, 6.0, 0.1)
            .num("Attack range", 3.0, 1.0, 6.0, 0.05)
            .num("Walls range", 3.5, 1.0, 6.0, 0.1)
            .bool("Auto disable", false)
            .group("Targeting")
            // Loaded before Attack mode: a config older than it says Multi by a Max targets above 1 (see set_field).
            .num("Max targets", 1.0, 1.0, 5.0, 1.0)
            .pick("Attack mode", &["Single", "Switch", "Multi"])
            .bool("Target lock", false)
            .num("Target switch delay", 0.0, 0.0, 40.0, 1.0)
            .pick("Priority", PRIORITY_CHOICES)
            .bool("Hurt time priority", false)
            .num("Target FOV", 360.0, 10.0, 360.0, 5.0)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Entities", "player")
            .num("Health threshold", 0.0, 0.0, 100.0, 1.0)
            .bool("Anti bot", true)
            .pick("Anti bot sensitivity", &["Balanced", "Conservative", "Sensitive"])
            .bool("Ignore teams", false)
            .bool("Ignore invisible", false)
            .bool("Ignore named", false)
            .bool("Ignore passive", true)
            .pick("Passive mob age filter", &["Adult", "Baby", "Both"])
            .pick("Hostile mob age filter", &["Both", "Adult", "Baby"])
            .group("Rotations")
            .pick("Rotate", &["Always", "On hit", "None"])
            // Snap turns at once (Instant); Normal, Smooth and Custom follow at their speeds (Tracked); Adaptive turns
            // fast when far, settles near the point, holds inside a deadzone and lands as a hit is due.
            .pick("Rotation mode", &["Silent aura", "Snap", "Normal", "Smooth", "Custom", "Adaptive"])
            .pick("Rotation view", &["Silent", "Visible"])
            // Aiming starts within Rotation range; where in the box (aura_aim.rs), and how far ahead of its movement.
            .num("Rotation range", 4.5, 1.0, 8.0, 0.1)
            .pick("Aim point", &["Dynamic", "Center", "Nearest", "Custom"])
            .num("Aim height (%)", 50.0, 0.0, 100.0, 5.0)
            .pick("Prediction", &["Basic", "Adaptive", "Off"])
            .num("Prediction strength", 1.0, 0.0, 2.0, 0.1)
            .num("Yaw speed", 60.0, 5.0, 180.0, 5.0)
            .num("Pitch speed", 60.0, 5.0, 180.0, 5.0)
            .num("Smoothing", 0.0, 0.0, 0.9, 0.05)
            // Silent aura's turning speed.
            .num("Aim speed", 7.0, 1.0, 10.0, 0.1)
            .bool("Ray cast", false)
            // When it attacks (aura_timing.rs). Modern: the cooldown (or a fixed delay, or Hit select's wait for the hurt
            // time to pass); Legacy: a click rate.
            .group("Timing")
            .bool("Only on click", false)
            .bool("Only on look", false)
            .bool("Pause on use", false)
            .pick("Attack timing", &["Vanilla cooldown", "Custom delay", "Hit select"])
            .num("Cooldown", 1.0, 0.5, 1.0, 0.05)
            .num("Hit delay", 11.0, 0.0, 60.0, 1.0)
            .num("Min CPS", 9.0, 1.0, 20.0, 0.5)
            .num("Max CPS", 13.0, 1.0, 20.0, 0.5)
            .pick("CPS pattern", &["Random", "Drift", "Steady"])
            // What it hits with.
            .group("Inventory")
            .pick("Attack when holding", &["Weapons", "All"])
            .text("Selected weapon types", "diamond_sword,diamond_axe,trident,mace,diamond_spear")
            .bool("Auto switch", false)
            .bool("Swap back", false)
            .num("Weapon switch delay", 0.0, 0.0, 10.0, 1.0)
            .group("Blocking")
            // The retained Shield alias (hidden) comes before AutoBlock, so an explicit new profile value overrides the
            // old bool's migration; Modern's shield settings follow.
            .bool("Shield", false)
            .pick("AutoBlock", &["Off", "Legacy", "Modern"])
            .num("Legacy reblock delay", 1.0, 0.0, 10.0, 1.0)
            .bool("Shield only with a weapon", true)
            .bool("Shield only near a target", true)
            .num("Shield range", 6.0, 2.0, 16.0, 0.5)
            .num("Shield re-raise delay", 0.0, 0.0, 10.0, 1.0)
            // Target strafe (target_strafe.rs): your move goes round the target at Strafe radius.
            .group("Movement")
            .bool("Target strafe", false)
            .num("Strafe radius", 2.5, 0.5, 6.0, 0.1)
            .pick("Strafe when", &["Always", "Jump held", "Speed on"])
            .bool("Avoid edges", true),
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
    identity: Option<super::sunset_antibot::Token>,
    target_timer: u32,
    // Switch: the last hit landed, so the next target's turn.
    switch_next: bool,
    // The look the rotation has reached.
    look: Option<(f32, f32)>,
    // Who it aimed at last tick and the look that would have met them.
    aimed: Option<(u32, (f32, f32))>,
    // Your entity id, which changes with the world (Auto disable).
    own_id: Option<u32>,
    // Silent aura: the managed look, and who it is aiming at.
    managed: Option<Managed>,
    silent_target: Option<u32>,
    // Legacy's click clock (aura_timing.rs), and the combat model it went by last tick and why (.auradebug).
    clicks: Option<timing::Clicks>,
    version: &'static str,
    // The target's track (prediction), the point kept in its box, Switch's memory, and the last aim (.auradebug).
    track: aim::Track,
    aim_share: Option<aim::Share>,
    recent: aim::Recent,
    aimed_at: Option<([f64; 3], [f64; 3], f32)>,
    // Hits sent and not yet seen to land (target id, tick), and the counts .auradebug shows.
    pending: Vec<(u32, u64)>,
    stats: Stats,
}

// What happened to Kill Aura's attacks, as counted since it was turned on. Sent is a hit packet on its way; landed and
// unseen are inferred from the target's hurt time afterwards (another player's hit looks the same: no confirmation).
#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    sent: u64,
    landed: u64,
    unseen: u64,
    // Why ticks passed with no hit, by reason (the most recent dozen kinds).
    reasons: [(&'static str, u64); 12],
}

impl As {
    // Why it did what it did this tick, counted.
    fn why(&mut self, reason: &'static str) {
        self.reason = reason;
        let r = &mut self.stats.reasons;
        if let Some(e) = r.iter_mut().find(|e| e.0 == reason) {
            e.1 += 1;
        } else if let Some(e) = r.iter_mut().find(|e| e.0.is_empty()) {
            *e = (reason, 1);
        }
    }
}

// Kill Aura's Priority choices: the shared ones, and Smart (aura_aim::smart_score).
const PRIORITY_CHOICES: &[&str] = &["Lowest distance", "Highest distance", "Lowest health", "Closest angle", "Smart"];

static AS: Mutex<Option<As>> = Mutex::new(None);
impl As {
    fn select(&mut self, id: u32, delay: u32, multi: bool, released: bool) -> bool {
        self.attacking = true;
        self.target = Some(id);
        let changed = self.current.replace(id).is_some_and(|old| old != id);
        if (changed || released) && !multi { self.target_timer = delay; }
        let waiting = self.target_timer > 0;
        self.target_timer = self.target_timer.saturating_sub(1);
        waiting
    }
    fn clear(&mut self, preserve_scene: bool) -> (bool, usize, usize) {
        let old = std::mem::take(self);
        if preserve_scene { self.own_id = old.own_id; }
        (old.swapped, old.prev, old.given)
    }
}

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

// What to hit with. Without Auto switch, what you hold if it will do. With it, the best weapon of the hotbar it may use,
// ranked by the shared evaluator (inventory_plan::weapon_score: a hit's damage in Legacy, damage over time in Modern,
// enchantments and wear), what you hold kept unless another is over 5% better.
fn weapon_slot(game: &Game<'_>, m: &Module, legacy: bool) -> Option<usize> {
    let held_fits = fit_slot(game, m, game.selected);
    if !m.b("Auto switch") {
        return held_fits.then_some(game.selected);
    }
    let value = |slot: usize| {
        game.inventory.get(slot).and_then(Option::as_ref)
            .map(|s| super::sunset_inventory_plan::weapon_score(&super::sunset_player::plan_item(game, s), legacy))
    };
    let best = (0..9)
        .filter(|&slot| fit_slot(game, m, slot))
        .filter_map(|slot| value(slot).map(|v| (slot, v)))
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
    match (held_fits, best) {
        (true, Some((_, top))) if value(game.selected).is_none_or(|v| v * 1.05 >= top) => Some(game.selected),
        (_, Some((slot, _))) => Some(slot),
        (true, None) => Some(game.selected),
        (false, None) => None,
    }
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

// Does a ray along `look` meet the target's box within `range`, with nothing solid before it beyond Walls range?
fn ray_hits(game: &Game<'_>, m: &Module, eye: [f64; 3], look: (f32, f32), t: &Target, range: f64) -> bool {
    use horizon_core::geometry::{Aabb, Vec3};
    let dir = direction(look);
    let b = Aabb::new(Vec3::new(t.lo[0], t.lo[1], t.lo[2]), Vec3::new(t.hi[0], t.hi[1], t.hi[2]));
    let Some(d) = super::ray_box(eye, dir, range, b) else {
        return false;
    };
    d <= m.n("Walls range") || clear_line(game, [eye[0] + dir[0] * d, eye[1] + dir[1] * d, eye[2] + dir[2] * d], [i32::MIN; 3])
}

// Reach as a 1.8 server's anticheat measures it (NoCheatPlus's fight.reach, and 1.8's own check): from the eyes to the
// target's middle line (its height clamped to the eyes'), at where the server last put it (before the client's
// smoothing), not to the near side of its box.
fn axis_reach(game: &Game<'_>, eye: [f64; 3], t: &Target) -> f64 {
    let at = super::remote::sunset_combat_remote::entity_position(game, t.id).unwrap_or(t.pos);
    let height = t.hi[1] - t.lo[1];
    let y = eye[1].clamp(at[1], at[1] + height);
    dist(eye, [at[0], y, at[2]])
}

// Does a look point at the target, give or take `margin` blocks round its box (a direction check's leeway)?
fn looks_at(game: &Game<'_>, m: &Module, eye: [f64; 3], look: (f32, f32), t: &Target, margin: f64, range: f64) -> bool {
    let wide = Target {
        lo: [t.lo[0] - margin, t.lo[1] - margin, t.lo[2] - margin],
        hi: [t.hi[0] + margin, t.hi[1] + margin, t.hi[2] + margin],
        ..t.clone()
    };
    ray_hits(game, m, eye, look, &wide, range)
}

// How far off the target the look the server has may be for a hit without Ray cast: NoCheatPlus's fight.direction
// cancels a hit whose look misses the box by more (and holds the next ones back a while).
const DIRECTION_MARGIN: f64 = 0.5;

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
    if !game.mobs.iter().any(|mob| mob.id == t.id && combat_eligible(game, m, mob)) {
        return false;
    }
    if reach(eye, t) > m.n("Range") || entity_listed(m.t("Entities"), t) == (m.p("List mode") == "Blacklist") || !in_fov(game, m, t, eye) {
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
    silent_return(game, m);
    let (was, swapped, prev, given) = st(|s| {
        s.current = None;
        s.identity = None;
        s.target_timer = 0;
        s.switch_next = false;
        s.look = None;
        s.aimed = None;
        if let Some(c) = s.clicks.as_mut() { c.stop(); }
        (std::mem::take(&mut s.attacking), std::mem::take(&mut s.swapped), s.prev, s.given)
    });
    if was && swapped && m.b("Swap back") && game.selected == given && control::owns(HAND) {
        hold(game, prev);
    }
    control::release(HAND);
    super::sunset::cancel_turns(HAND);
}

// Turned off: back to the weapon it swapped from, and nothing kept for next time (the game sends your own look again).
pub(super) fn disabled(game: &mut Game<'_>, swap_back: bool) {
    reset(game, swap_back, false);
}
pub(super) fn clear_targets(game: &mut Game<'_>) {
    reset(game, false, true);
}
fn reset(game: &mut Game<'_>, swap_back: bool, preserve_scene: bool) {
    ENGAGED.store(false, Ordering::Relaxed);
    let (swapped, prev, given) = st(|s| s.clear(preserve_scene));
    super::sunset_rotation::let_go();
    if swap_back && swapped && game.selected == given {
        hold(game, prev);
    }
    control::release(HAND);
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn clearing_targets_preserves_auto_disable_world_comparison() {
        let mut state = As { own_id: Some(7), current: Some(3), ..As::default() };
        state.clear(true);
        assert!(state.current.is_none());
        assert!(state.own_id.replace(8).is_some_and(|old| old != 8));
        state.clear(false);
        assert!(state.own_id.is_none());
    }
    #[test]
    fn releasing_an_ineligible_target_keeps_the_configured_switch_delay() {
        let mut state = As::default();
        assert!(state.select(2, 3, false, true));
        assert!(state.select(2, 3, false, false));
        assert!(state.select(2, 3, false, false));
        assert!(!state.select(2, 3, false, false));
        let mut multi = As::default();
        assert!(!multi.select(2, 3, true, true));
    }
}

// Who to hit this tick: Multi all it may, Switch each in turn, Single one (the same one with Target lock).
fn choose(m: &Module, mut list: Vec<Target>, now: u64) -> Vec<Target> {
    let mode = m.p("Attack mode");
    if mode == "Multi" {
        list.truncate(m.n("Max targets") as usize);
        return list;
    }
    let (current, next) = st(|s| (s.current, std::mem::take(&mut s.switch_next)));
    let at = current.and_then(|id| list.iter().position(|t| t.id == id));
    // Switch moves on after each hit sent (no landing is confirmed), to the best target not hit in the last 10 ticks.
    let fresh = |now: u64| {
        let ids: Vec<u32> = list.iter().map(|t| t.id).collect();
        st(|s| s.recent.next(&ids, now, 10)).unwrap_or(0)
    };
    let pick = match (mode, at) {
        ("Switch", Some(_)) if next => fresh(now),
        ("Switch", Some(i)) => i,
        ("Single", Some(i)) if m.b("Target lock") => i,
        _ => 0,
    };
    vec![list.swap_remove(pick)]
}

// Kill Aura had someone to hit on its last tick (Scaffold's Disable with Kill Aura pauses for that, not for the module
// merely being on).
static ENGAGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn engaged() -> bool {
    ENGAGED.load(Ordering::Relaxed)
}

// Scaffold is on and placing (not paused for Kill Aura or Blink, and its Only on click held if set): Kill Aura leaves it
// the look and the hand, as both would rewrite the one move packet's look.
fn scaffold_placing(game: &Game<'_>) -> bool {
    let clicking = game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right);
    super::sunset::with(|s| {
        let blink = s.is_on("Blink");
        s.module("Scaffold").filter(|m| m.on).is_some_and(|m| {
            !blink && !m.b("Disable with Kill Aura") && (clicking || !m.b("Only on click"))
        })
    })
}

// Silent aura: the managed look a tick nearer the target's aim point, for the game's next move packet. SilentAura
// aims at the middle of the box level with your eyes (its feet when you are below them), with a drifting jitter.
fn silent_aim(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3]) -> (f32, f32) {
    let view = (game.yaw, game.pitch);
    let motion = game.player.as_ref().filter(|_| game.riding.is_none()).map_or([0.0; 3], |p| {
        [p.position.x - game.previous_position.x, p.position.y - game.previous_position.y, p.position.z - game.previous_position.z]
    });
    let feet = game.player.as_ref().map_or(eye, |p| [p.position.x, p.position.y, p.position.z]);
    let point = [(t.lo[0] + t.hi[0]) / 2.0, t.lo[1], (t.lo[2] + t.hi[2]) / 2.0];
    let moved = (t.pos[0] - t.prev[0]).hypot(t.pos[2] - t.prev[2]);
    let distance = dist(feet, point);
    let sensitivity = f64::from(game.settings.sensitivity);
    let ticks = game.ticks as u32;
    st(|s| {
        let seed = u64::from(t.id) << 20 ^ u64::from(ticks) ^ 0x5a_5a17;
        let mm = s.managed.get_or_insert_with(|| Managed::start(view, seed));
        if mm.returning() || s.silent_target != Some(t.id) {
            mm.new_target();
            s.silent_target = Some(t.id);
        }
        mm.aim(eye, point, t.hi[1] - t.lo[1], motion, moved, distance, ticks);
        mm.tick(view, sensitivity, m.n("Aim speed"), distance);
        mm.send();
        mm.look
    })
}

// Silent aura with nothing to aim at: the look goes back to yours at the same speed, then the game sends yours.
fn silent_return(game: &Game<'_>, m: &Module) {
    let view = (game.yaw, game.pitch);
    let sensitivity = f64::from(game.settings.sensitivity);
    st(|s| {
        s.silent_target = None;
        let Some(mm) = s.managed.as_mut() else {
            return;
        };
        mm.release();
        if mm.tick(view, sensitivity, m.n("Aim speed"), f64::MAX) {
            s.managed = None;
            super::sunset_rotation::let_go();
        } else {
            mm.send();
        }
    });
}

pub(super) fn tick(game: &mut Game<'_>, m: &Module, crits: bool) {
    preset(game, m);
    observe_hits(game);
    st(|s| s.target = None);
    // Who it is engaged with is decided again below; a hit still on its way keeps it.
    let was_engaged = ENGAGED.swap(false, Ordering::Relaxed);
    let held = st(|s| (s.current, s.identity));
    let released = held.0.is_some_and(|id| held.1.is_none_or(|token| !super::sunset_antibot::current(token)) || !eligible(game, m, id));
    if released {
        stop(game, m);
    }
    if game.remote.is_none() {
        st(|s| s.managed = None);
        super::sunset_rotation::let_go();
        return;
    }
    if !game.native_playing() {
        return;
    }
    // A hit on its way: nothing else may take the hand before it goes.
    if super::sunset::CRIT_STAGE.load(Ordering::Relaxed) != 0 || super::sunset::strike_waiting() {
        let _ = control::claim(HAND);
        ENGAGED.store(was_engaged, Ordering::Relaxed);
        st(|s| s.why("a hit is on its way"));
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
        st(|s| s.why("lost the hand before the hit went"));
    }
    // The hand: kept only for a weapon to swap back from, and left alone while another module has it.
    if !st(|s| s.swapped) {
        control::release(HAND);
    }
    if !control::available(HAND) || !super::sunset::inventory_idle(game) {
        stop(game, m);
        st(|s| s.why("another module has the hand, or a menu is open"));
        return;
    }
    if game.player.as_ref().is_none_or(|p| p.health.health() <= 0.0) || game.spectator() {
        stop(game, m);
        return;
    }
    let using = game.using.is_some() && !super::sunset_hands::shielding(game);
    if m.b("Pause on use") && (using || game.mining.is_some()) {
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
    let mut list: Vec<Target> = if m.b("Only on look") {
        game.mob_hit().map(|(i, ..)| mob_target(&game.mobs[i])).into_iter().collect()
    } else {
        every_living(game)
    };
    list.retain(|t| {
        Some(t.id) != own
            && entity_check(game, m, t, eye)
    });
    if m.p("Priority") == "Smart" {
        let look = st(|s| s.look).unwrap_or((game.yaw, game.pitch));
        let held = st(|s| s.current);
        let range = m.n("Range");
        let mut scored: Vec<(f64, Target)> = list.into_iter().map(|t| {
            let hurt = game.mobs.iter().any(|x| x.id == t.id && x.hurt_time > 0);
            let turn = aim::angle(look, look_at(eye, centre(&t)));
            (aim::smart_score(reach(eye, &t), range, turn, hurt, in_sight(game, &t), held == Some(t.id)), t)
        }).collect();
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        list = scored.into_iter().map(|(_, t)| t).collect();
    } else {
        sort_targets(game, &mut list, m.p("Priority"));
    }
    if m.b("Hurt time priority") {
        // Who can take a hit soonest; the sort keeps Priority's order among equals.
        let hurt = |t: &Target| game.mobs.iter().find(|x| x.id == t.id).map_or(0, |x| x.hurt_time);
        list.sort_by_key(hurt);
    }
    ENGAGED.store(!list.is_empty(), Ordering::Relaxed);
    if list.is_empty() {
        stop(game, m);
        st(|s| s.why("no target"));
        return;
    }
    if scaffold_placing(game) {
        stop(game, m);
        st(|s| s.why("Scaffold is placing"));
        return;
    }
    // Pick the weapon: never out of a hand eating, blocking or mining.
    let legacy_now = timing::version(m.p("Combat version"), game.main_hand_attack_speed()).0 == timing::Version::Legacy;
    let Some(slot) = weapon_slot(game, m, legacy_now) else {
        stop(game, m);
        st(|s| s.why("no usable weapon"));
        return;
    };
    if slot != game.selected {
        if using || game.mining.is_some() || !control::claim(HAND) {
            st(|s| s.why("the hand is busy"));
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
        st(|s| s.why("the hand is busy"));
        return;
    }
    let list = choose(m, list, game.ticks);
    let primary = list[0].clone();
    let delay = m.n("Target switch delay") as u32;
    let waiting = st(|s| {
        s.identity = super::sunset_antibot::token(primary.id);
        s.select(primary.id, delay, m.p("Attack mode") == "Multi", released)
    });
    // Aiming waits for Rotation range (the target stays chosen meanwhile).
    let rotate_mode = if m.p("Rotate") == "Always" && reach(eye, &primary) > m.n("Rotation range") { "None" } else { m.p("Rotate") };
    let silent = rotate_mode == "Always" && m.p("Rotation mode") == "Silent aura";
    let attack_range = m.n("Attack range");
    if !silent {
        silent_return(game, m);
    }
    // The look this tick: from where the last tick's got to (or your own), a step toward the target. Silent tells only
    // the server; Visible turns your view. A hit made now goes out before that look does, so it counts the last one.
    let sent = st(|s| s.look);
    if rotate_mode == "Always" {
        let from = sent.unwrap_or((game.yaw, game.pitch));
        // The look is checked next tick: aim at where you and the target will be by then, each moving as this tick.
        let yours = game.player.as_ref().filter(|_| game.riding.is_none()).map_or([0.0; 3], |p| {
            [p.position.x - game.previous_position.x, p.position.y - game.previous_position.y, p.position.z - game.previous_position.z]
        });
        // Where the target will be (the predictor's track, at your ping), and the point of its box to aim at there.
        let latency = super::remote::sunset_combat_remote::latency(game);
        let prediction = aim::Prediction::named(m.p("Prediction"));
        let ahead = st(|s| {
            s.track.observe(primary.id, primary.pos);
            s.track.predict(prediction, aim::horizon_ticks(latency), m.n("Prediction strength"))
        });
        let shift = [ahead[0] - primary.pos[0], ahead[1] - primary.pos[1], ahead[2] - primary.pos[2]];
        let (lo, hi) = (
            [primary.lo[0] + shift[0], primary.lo[1] + shift[1], primary.lo[2] + shift[2]],
            [primary.hi[0] + shift[0], primary.hi[1] + shift[1], primary.hi[2] + shift[2]],
        );
        let eye_next = [eye[0] + yours[0], eye[1] + yours[1], eye[2] + yours[2]];
        let walls = m.n("Walls range");
        let point_mode = aim::AimPoint::named(m.p("Aim point"), m.n("Aim height (%)"));
        let kept = st(|s| s.aim_share);
        let mut visible = |p: [f64; 3]| dist(eye_next, p) <= walls || clear_line(game, p, [i32::MIN; 3]);
        let (point, share) = aim::plan_point(eye_next, lo, hi, from, point_mode, attack_range, kept, &mut visible);
        let to = look_at(eye_next, point);
        st(|s| {
            s.aim_share = Some(share);
            s.aimed_at = Some((point, ahead, aim::angle(from, to)));
        });
        let was = st(|s| s.aimed.replace((primary.id, to))).filter(|a| a.0 == primary.id).map(|a| a.1);
        let next = if silent {
            // Silent aura: the game's own move packet carries it (only when it changed), whatever Rotation view says.
            silent_aim(game, m, &primary, eye)
        } else {
            let next = if m.p("Rotation mode") == "Adaptive" {
                // A hit is due when the cooldown (Modern) is nearly charged; Legacy's clicks can come any tick.
                let due = legacy_now || game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 0.85;
                let carried = was.map_or(from, |w| (from.0 + ((to.0 - w.0 + 180.0).rem_euclid(360.0) - 180.0), from.1 + (to.1 - w.1)));
                aim::adaptive_step(carried, to, (m.n("Yaw speed") as f32, m.n("Pitch speed") as f32), due)
            } else {
                track(m.p("Rotation mode"), from, to, was, m.n("Yaw speed") as f32, m.n("Pitch speed") as f32, m.n("Smoothing") as f32)
            };
            if m.p("Rotation view") == "Visible" {
                (game.yaw, game.pitch) = next;
            } else {
                super::sunset::turn("Kill Aura", next.0, next.1);
            }
            next
        };
        st(|s| s.look = Some(next));
    }
    // Target switch delay: the new target waits.
    if waiting {
        st(|s| s.why("turning to a new target"));
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
        st(|s| s.why("weapon switch delay"));
        return;
    }
    // Legacy clicks at a rate; Modern waits for the cooldown, a fixed delay, or (Hit select) the hurt time too.
    let speed = game.main_hand_attack_speed();
    let (version, why) = timing::version(m.p("Combat version"), speed);
    st(|s| s.version = why);
    let legacy = version == timing::Version::Legacy;
    let timing = if legacy { "Clicks" } else { m.p("Attack timing") };
    let ready = if legacy {
        let seed = u64::from(primary.id) << 32 ^ game.ticks;
        let pattern = timing::Pattern::named(m.p("CPS pattern"));
        st(|s| s.clicks.get_or_insert_with(|| timing::Clicks::new(seed)).tick(m.n("Min CPS"), m.n("Max CPS"), pattern))
    } else if timing == "Custom delay" {
        st(|s| {
            if s.hit_timer < m.n("Hit delay") as u32 {
                s.hit_timer += 1;
                false
            } else {
                true
            }
        })
    } else {
        timing::modern_ready(game.hand_strength.scale(0.5, speed), m.n("Cooldown"))
    };
    let spear = game.held().is_some_and(|s| Kinetic::of(&s.item).is_some());
    if !ready || (spear && game.cannot_attack_with_item(game.hand_strength.ticker, 0)) {
        st(|s| s.why("cooling down"));
        return;
    }
    if !control::claim(HAND) {
        st(|s| s.why("the hand is busy"));
        return;
    }
    // The looks a hit can be judged by. It goes at the start of the next tick, before that tick's move packet: the
    // server has the look the last move packet carried, and the move packet right after the hit carries the look worked
    // out this tick (as a player's click goes with the look of the frame it was made in, sent just after it).
    let server = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let coming = if rotate_mode == "Always" { st(|s| s.look).unwrap_or(server) } else { (game.yaw, game.pitch) };
    let ray_cast = m.b("Ray cast");
    let mut hit = false;
    st(|s| s.reason = "the look does not meet the target");
    // A stab is one action, whoever it goes through.
    for t in list.iter().take(if spear { 1 } else { usize::MAX }) {
        // Hit select: not while the target's hurt time still guards it; its last tick counts (the hit lands as it ends).
        if timing == "Hit select" && game.mobs.iter().any(|x| x.id == t.id && x.hurt_time > 1) {
            st(|s| s.why("hit select: the target is still hurt"));
            continue;
        }
        let on_hit = (rotate_mode == "On hit").then(|| look_at(eye, centre(t)));
        // A hit goes within Attack range (Range only picks whom it aims at). With Ray cast it also needs a look that
        // meets the target: the one the server has or the one sent right after the hit (On hit's own turn goes first).
        // Without, the rotation aims but the hit does not wait for it. A stab goes along the look the server has.
        // Legacy: reach to the target's middle line, as a 1.8 server's anticheat measures it.
        let far = if legacy { axis_reach(game, eye, t) } else { reach(eye, t) };
        if !spear && far > attack_range {
            st(|s| s.why("out of attack range"));
            continue;
        }
        // Legacy: the hit reaches a 1.8 server before the look sent after it, so only the look it has counts.
        let meets = |look: (f32, f32)| ray_hits(game, m, eye, look, t, attack_range + 1.0);
        let fits = if spear {
            stab_hits(game, m, eye, on_hit.unwrap_or(server), t)
        } else if let Some(look) = on_hit {
            !ray_cast || meets(look)
        } else if ray_cast {
            meets(server) || (!legacy && meets(coming))
        } else if rotate_mode == "Always" {
            // No Ray cast: the rotation need not meet the box, but a look far off it (a new target's first tick, a
            // flick) waits for the next one.
            let near = |look: (f32, f32)| looks_at(game, m, eye, look, t, DIRECTION_MARGIN, attack_range + 1.0);
            near(server) || (!legacy && near(coming))
        } else {
            true
        };
        if !fits {
            st(|s| s.why("the look does not meet the target"));
            continue;
        }
        super::sunset::attack_for(game, t.id, crits, on_hit, super::sunset::AttackSource::KillAura);
        let tick = game.ticks;
        st(|s| {
            s.stats.sent += 1;
            s.recent.hit(t.id, tick);
            if s.pending.len() >= 8 {
                s.pending.remove(0);
            }
            s.pending.push((t.id, tick));
        });
        hit = true;
    }
    if !hit && legacy {
        // The click was due: it goes as soon as the look meets, not a whole gap later.
        st(|s| if let Some(c) = s.clicks.as_mut() { c.missed(); });
    }
    if hit {
        st(|s| {
            s.hit_timer = 0;
            s.switch_next = true;
            s.why("hit sent");
            s.hit = Some(control::finish(HAND));
        });
    }
}

// Rechecked after deferred rotation/Criticals and immediately before a hit.
pub(super) fn eligible(game: &Game<'_>, m: &Module, id: u32) -> bool {
    game.eye_position().zip(game.mobs.iter().find(|mob| mob.id == id))
        .is_some_and(|(eye, mob)| entity_check(game, m, &mob_target(mob), eye))
}

// Hits sent a few ticks ago: the target's hurt time starting again says it took one (or someone else's, so: inferred);
// four ticks with none says it did not show.
fn observe_hits(game: &Game<'_>) {
    let now = game.ticks;
    let pending = st(|s| std::mem::take(&mut s.pending));
    let mut keep = Vec::with_capacity(pending.len());
    let (mut landed, mut unseen) = (0, 0);
    for (id, tick) in pending {
        let hurt = game.mobs.iter().find(|x| x.id == id).map(|x| x.hurt_time);
        match hurt {
            Some(h) if h >= 8 && now > tick => landed += 1,
            _ if now.saturating_sub(tick) > 4 => unseen += 1,
            _ => keep.push((id, tick)),
        }
    }
    st(|s| {
        s.pending = keep;
        s.stats.landed += landed;
        s.stats.unseen += unseen;
    });
}

// Kill Aura's presets: real settings for a way of fighting, filled in when picked (not on the first look, so a change
// made afterwards stays). HvH: one locked target, Smart priority, Adaptive aim at a dynamic point with adaptive
// prediction. Blatant: fast switching and snapping at the nearest point, at full range. Ghost: only while you click,
// within a narrow field of view, slow adaptive turns.
const PRESETS: [(&str, &[(&str, &str)]); 3] = [
    ("HvH", &[("Attack mode", "Single"), ("Target lock", "true"), ("Priority", "Smart"), ("Hurt time priority", "false"),
        ("Rotate", "Always"), ("Rotation mode", "Adaptive"), ("Aim point", "Dynamic"), ("Prediction", "Adaptive"),
        ("Ray cast", "true"), ("Auto switch", "true"), ("Range", "6"), ("Rotation range", "6"), ("Only on click", "false"),
        ("Yaw speed", "180"), ("Pitch speed", "180")]),
    ("Blatant", &[("Attack mode", "Switch"), ("Priority", "Lowest distance"), ("Rotate", "Always"), ("Rotation mode", "Snap"),
        ("Aim point", "Nearest"), ("Prediction", "Basic"), ("Range", "6"), ("Rotation range", "6"), ("Target FOV", "360"),
        ("Auto switch", "true"), ("Only on click", "false")]),
    ("Ghost", &[("Attack mode", "Single"), ("Priority", "Closest angle"), ("Rotate", "Always"), ("Rotation mode", "Adaptive"),
        ("Aim point", "Dynamic"), ("Prediction", "Basic"), ("Range", "3.8"), ("Rotation range", "3.8"), ("Target FOV", "90"),
        ("Only on click", "true"), ("Yaw speed", "25"), ("Pitch speed", "20")]),
];
static LAST_PRESET: Mutex<String> = Mutex::new(String::new());

fn preset(game: &Game<'_>, m: &Module) {
    let picked = m.p("Preset");
    let before = std::mem::replace(&mut *LAST_PRESET.lock().unwrap_or_else(|e| e.into_inner()), picked.to_owned());
    if before.is_empty() || before == picked {
        return;
    }
    if let Some((_, fields)) = PRESETS.iter().find(|p| p.0 == picked) {
        for (name, value) in fields.iter() {
            super::sunset::set_setting(game, &format!("killaura.{}", super::sunset::norm(name)), value);
        }
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    // Every preset field is one of Kill Aura's settings and takes its value.
    #[test]
    fn kill_aura_presets_set_real_settings() {
        let m = defaults().into_iter().next().unwrap();
        for (preset, fields) in PRESETS {
            for (name, value) in fields.iter() {
                let setting = m.setting(name).unwrap_or_else(|| panic!("{preset}: no setting {name}"));
                assert!(setting.clone().parse(value), "{preset}: {name} = {value}");
            }
        }
    }
}

// Who Kill Aura is hitting.
pub(super) fn target_id() -> Option<u32> {
    st(|s| s.target)
}

pub(super) fn diagnostics() -> String {
    st(|s| {
        let mut out = format!("KillAura reason={} target={:?} hand={:?} combat={} autoblock={}", s.reason, s.target, control::owner(), s.version,
            super::sunset_hands::autoblock_status());
        if let Some((point, ahead, error)) = s.aimed_at {
            out.push_str(&format!(" aim=[{:.2},{:.2},{:.2}] predicted=[{:.2},{:.2},{:.2}] turn_left={error:.1} trust={:.2}",
                point[0], point[1], point[2], ahead[0], ahead[1], ahead[2], s.track.trust()));
        }
        out.push_str(&format!(" | sent={} landed={} unseen={} (landed/unseen inferred from hurt time)", s.stats.sent, s.stats.landed, s.stats.unseen));
        let mut reasons: Vec<&(&str, u64)> = s.stats.reasons.iter().filter(|r| !r.0.is_empty()).collect();
        reasons.sort_by(|a, b| b.1.cmp(&a.1));
        for (r, n) in reasons.iter().take(6) {
            out.push_str(&format!(" | {r}: {n}"));
        }
        out
    })
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
