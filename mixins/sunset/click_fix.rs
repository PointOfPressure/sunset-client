// Your own clicks, before the game acts on them. Aim Assist's Silent aim (a miss goes to the nearest target you could
// hit, under a look the server sees) and Hit flick (a hit on a player goes after a look away and back); No Interact's
// Hit fix (a hit on a player still in hurt time or off the ground waits until it counts), No miss delay (a sword or axe
// swing at nothing never goes, so neither the miss delay nor the cooldown reset follow) and Anti action (a sword hit
// on a raised shield, a second glowstone in an anchor, an ender chest opened with combat gear in hand, glowstone put
// down off an anchor, a firework fired on foot) and Ghost place (a block put behind the mob you click instead of a use on
// it).
use super::sunset::{self, Module, attack, gui_open, look_at, turn_then, with};
use super::sunset_util as util;
use super::Game;
use horizon_client::mobs::Mob;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Silent aim's spots on a body, from the feet (0) to the top of the head (1).
const AIM_POINTS: [(&str, f64); 7] =
    [("head", 0.95), ("neck", 0.85), ("body", 0.7), ("arms", 0.65), ("thighs", 0.35), ("legs", 0.2), ("feet", 0.05)];
// A hit held by Hit fix goes within this, or not at all.
const HIT_FIX_WAIT: Duration = Duration::from_secs(1);
// Silent aim, Hit flick and Hit fix hit within the game's own reach.
const REACH: f64 = 3.0;

#[derive(Default)]
struct State {
    // Hit fix: the player a hit waits for, and since when.
    held: Option<(u32, Instant)>,
    // A hit Silent aim, Hit flick or Hit fix sent on: the clicks wait until it has gone.
    busy_until: u64,
    roll: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn random(game: &Game<'_>) -> f64 {
    st(|s| {
        s.roll = (s.roll ^ game.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
        s.roll ^= s.roll << 13;
        s.roll ^= s.roll >> 7;
        s.roll ^= s.roll << 17;
        (s.roll >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

fn live(game: &Game<'_>) -> bool {
    game.remote.is_some() && game.native_playing() && !gui_open() && !game.spectator() && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
}

fn held_is(game: &Game<'_>, tag: &str) -> bool {
    game.held().is_some_and(|s| game.res.items().item_tags.get(tag).is_some_and(|t| t.contains(&*s.item)))
}

fn sword_or_axe(game: &Game<'_>) -> bool {
    held_is(game, "minecraft:swords") || held_is(game, "minecraft:axes")
}

fn mob<'a>(game: &'a Game<'_>, id: u32) -> Option<&'a Mob> {
    game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed)
}

// The point of `mob`'s box nearest your eyes across, at the listed height nearest your eyes.
fn aim_point(eye: [f64; 3], mob: &Mob, points: &str) -> [f64; 3] {
    let b = mob.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let (x, z) = (eye[0].clamp(lo.x, hi.x), eye[2].clamp(lo.z, hi.z));
    let across = (eye[0] - x).powi(2) + (eye[2] - z).powi(2);
    AIM_POINTS
        .iter()
        .filter(|(name, _)| util::listed(points, name))
        .map(|(_, share)| lo.y + share * (hi.y - lo.y))
        .min_by(|a, b| (across + (eye[1] - a).powi(2)).total_cmp(&(across + (eye[1] - b).powi(2))))
        .map_or([x, lo.y + 0.9 * (hi.y - lo.y), z], |y| [x, y, z])
}

fn wrap(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

// ---- Aim Assist

// Your attack click, after No Interact: Some(instant) takes it (as the attack hook returns), None leaves it to the game.
pub(super) fn aim_click(game: &mut Game<'_>) -> Option<bool> {
    let m = module("Aim Assist").filter(|m| m.b("Silent aim") || m.b("Hit flick"))?;
    if !live(game) || game.clicks.miss_time > 0 || sunset::strike_waiting() || game.held().is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some()) {
        return None;
    }
    if game.ticks < st(|s| s.busy_until) {
        return Some(false);
    }
    let eye = game.eye_position()?;
    match game.mob_hit() {
        Some((i, distance)) if game.before_block(distance) => {
            let target = &game.mobs[i];
            if !m.b("Hit flick")
                || (m.b("Flick players only") && target.kind.id != "minecraft:player")
                || (target.kind.id == "minecraft:player" && !util::player_enemy(game, target))
                || (m.b("Flick needs sprint") && !game.player.as_ref().is_some_and(|p| p.sprinting))
                || game.using.is_some()
            {
                return None;
            }
            let id = target.id;
            let side = match m.p("Flick direction") {
                "Left" => -1.0,
                "Right" => 1.0,
                _ if random(game) < 0.5 => -1.0,
                _ => 1.0,
            };
            let (a, b) = (m.n("Flick min angle").min(m.n("Flick max angle")), m.n("Flick min angle").max(m.n("Flick max angle")));
            let angle = (a + random(game) * (b - a)) as f32;
            let yaw = game.yaw + side * angle + (random(game) * 3.0 - 1.5) as f32;
            let pitch = (game.pitch + (random(game) * 2.0 - 1.0) as f32).clamp(-89.9, 89.9);
            let points = m.t("Aim points").to_owned();
            st(|s| s.busy_until = game.ticks + 4);
            // The look away goes with the next move packet; the look back and the hit after it.
            turn_then("Hit flick", yaw, pitch, move |game| {
                let (Some(eye), Some(target)) = (game.eye_position(), mob(game, id)) else {
                    return;
                };
                let at = aim_point(eye, target, &points);
                if util::dist(eye, at) > REACH {
                    return;
                }
                let (yaw, pitch) = look_at(eye, at);
                attack(game, id, true, Some((yaw, pitch.clamp(-89.9, 89.9))));
            });
            Some(false)
        }
        hit => {
            // A miss, or a block with a sword in hand.
            let block = hit.is_some() || game.pick().is_some();
            if !m.b("Silent aim") || (block && !held_is(game, "minecraft:swords")) || (m.b("Silent weapon only") && !sword_or_axe(game) && game.held().is_none_or(|s| &*s.item != "minecraft:mace")) {
                return None;
            }
            let points = m.t("Aim points");
            let (yaw, pitch) = (game.yaw, game.pitch);
            let (id, at) = game
                .mobs
                .iter()
                .filter(|t| t.alive() && !t.removed && Some(t.id) != sunset::own_id(game))
                .filter(|t| if t.kind.id == "minecraft:player" { util::player_enemy(game, t) } else { !m.b("Silent players only") })
                .map(|t| (t.id, aim_point(eye, t, points)))
                .filter(|(_, at)| util::dist(eye, *at) <= REACH && util::clear_line(game, *at, [i32::MIN; 3]))
                .filter(|(_, at)| {
                    let (y, p) = look_at(eye, *at);
                    f64::from(wrap(y - yaw).abs()) <= m.n("Silent max yaw") && f64::from((p - pitch).abs()) <= m.n("Silent max pitch")
                })
                .min_by(|a, b| util::dist(eye, a.1).total_cmp(&util::dist(eye, b.1)))?;
            let (yaw, pitch) = look_at(eye, at);
            st(|s| s.busy_until = game.ticks + 3);
            attack(game, id, true, Some((yaw, pitch.clamp(-89.9, 89.9))));
            Some(false)
        }
    }
}

// ---- No Interact

// Your attack click, before anything else: true stops it. Hit fix holds a hit on a player in hurt time or off the
// ground (and every click on them while one waits); Anti action's Sword on shield stops a sword hit on a raised shield.
pub(super) fn hit_blocked(game: &Game<'_>) -> bool {
    let Some(m) = module("No Interact") else {
        return false;
    };
    let Some((i, distance)) = game.mob_hit().filter(|(_, d)| game.before_block(*d)) else {
        return false;
    };
    let target = &game.mobs[i];
    if target.kind.id != "minecraft:player" || !live(game) {
        return false;
    }
    if m.b("Sword on shield") && held_is(game, "minecraft:swords") && super::sunset_melee::observed_shield(game, target.id) {
        return true;
    }
    if !m.b("Hit fix") || (m.b("Hit fix weapons only") && !sword_or_axe(game)) || distance > REACH {
        return false;
    }
    let waiting = st(|s| s.held.is_some_and(|(id, at)| id == target.id && at.elapsed() < HIT_FIX_WAIT));
    if waiting || !takes_hit(&m, target) {
        let id = target.id;
        st(|s| {
            if !waiting {
                s.held = Some((id, Instant::now()));
            }
        });
        return true;
    }
    false
}

// Hit fix: a hit on `target` would count now (out of hurt time with Hurt time, on the ground with On ground).
fn takes_hit(m: &Module, target: &Mob) -> bool {
    !(m.b("Hit fix hurt time") && target.hurt_time > 0) && !(m.b("Hit fix on ground") && !target.on_ground)
}

// No miss delay: a sword or axe swing at nothing (no mob, no block) does not go, but by Humanize's chance, and with
// In combat only only with an enemy player within Combat range.
pub(super) fn miss_blocked(game: &Game<'_>) -> bool {
    let Some(m) = module("No Interact").filter(|m| m.b("No miss delay")) else {
        return false;
    };
    if !live(game) || !sword_or_axe(game) || game.mob_hit().is_some() || game.pick().is_some() || game.redirectable_hit().is_some() || game.remote_object_hit(true).is_some() {
        return false;
    }
    if m.b("In combat only") {
        let (Some(me), range) = (util::player_pos(game), m.n("Combat range")) else {
            return false;
        };
        if !game.mobs.iter().any(|t| util::player_enemy(game, t) && util::dist(me, [t.position.x, t.position.y, t.position.z]) <= range) {
            return false;
        }
    }
    random(game) * 100.0 >= m.n("Humanize (%)")
}

// Each tick: Hit fix's waiting hit goes once the player can take it, within reach and in sight, under a look at
// them; past a second it is dropped.
pub(super) fn tick(game: &mut Game<'_>) {
    let Some((id, at)) = st(|s| s.held) else {
        return;
    };
    let m = module("No Interact").filter(|m| m.b("Hit fix"));
    let (Some(m), Some(eye)) = (m, game.eye_position()) else {
        st(|s| s.held = None);
        return;
    };
    if at.elapsed() >= HIT_FIX_WAIT || !live(game) || game.using.is_some() {
        st(|s| s.held = None);
        return;
    }
    let Some(target) = mob(game, id) else {
        st(|s| s.held = None);
        return;
    };
    if !takes_hit(&m, target) || sunset::strike_waiting() || sunset::turn_pending() {
        return;
    }
    let point = aim_point(eye, target, "head,body");
    st(|s| s.held = None);
    if util::dist(eye, point) > REACH || !util::clear_line(game, point, [i32::MIN; 3]) {
        return;
    }
    let (yaw, pitch) = look_at(eye, point);
    st(|s| s.busy_until = game.ticks + 3);
    attack(game, id, true, Some((yaw, pitch.clamp(-89.9, 89.9))));
}

// Your right click: true stops it (Anti action).
pub(super) fn use_blocked(game: &Game<'_>) -> bool {
    let Some(m) = module("No Interact").filter(|m| m.b("Anti action")) else {
        return false;
    };
    if !live(game) {
        return false;
    }
    let main = game.held().map(|s| &*s.item);
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    if m.b("Ground firework")
        && main == Some("minecraft:firework_rocket")
        && !game.player.as_ref().is_some_and(|p| p.fall_flying)
        && game.offhand.as_ref().is_none_or(|s| &*s.item != "minecraft:crossbow")
    {
        return true;
    }
    if game.mob_hit().is_some_and(|(_, d)| game.before_block(d)) {
        return false;
    }
    let Some(hit) = game.pick() else {
        return false;
    };
    let name = util::short(&util::facts(game, hit.pos).name).to_owned();
    let glowstone = main == Some("minecraft:glowstone");
    if name == "respawn_anchor" && !sneaking {
        return m.b("Double glowstone") && glowstone && anchor_charged(game, hit.pos);
    }
    if m.b("Place glowstone") && glowstone {
        return true;
    }
    m.b("Open ender chest") && name == "ender_chest" && !sneaking && (combat_gear(game, game.held()) || combat_gear(game, game.offhand.as_ref()))
}

// Ghost place: your right click on a player or mob (not a friend) with cobweb in hand (any block, with Cobweb only
// off; the offhand's, with Use offhand and nothing placeable in the main hand) goes to the block behind them along
// your look instead, as a placement, rather than to them. The hand it places with.
pub(super) fn ghost_place(game: &Game<'_>) -> Option<horizon_client::first_person::Hand> {
    use horizon_client::first_person::Hand;
    let m = module("No Interact").filter(|m| m.b("Ghost place"))?;
    if !live(game) || game.using.is_some() {
        return None;
    }
    let placeable = |s: Option<&horizon_client::items::ItemStack>| {
        s.is_some_and(|s| if m.b("Cobweb only") { &*s.item == "minecraft:cobweb" } else { game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()) })
    };
    let hand = if placeable(game.held()) {
        Hand::Main
    } else if m.b("Use offhand") && placeable(game.offhand.as_ref()) {
        Hand::Off
    } else {
        return None;
    };
    let (i, _) = game.mob_hit().filter(|(_, d)| game.before_block(*d))?;
    let mob = &game.mobs[i];
    let friend = mob.kind.id == "minecraft:player" && sunset::is_friend(&with(|s| s.friends.clone()), &game.remote_player_names(), mob.id);
    (mob.kind.id != "minecraft:armor_stand" && !mob.removed && !friend && game.pick().is_some()).then_some(hand)
}

fn anchor_charged(game: &Game<'_>, p: [i32; 3]) -> bool {
    let b = game.world.block(p[0], p[1], p[2]);
    game.pack.assets.registry.state(b).and_then(|s| s.properties.get("charges")).is_some_and(|c| c != "0")
}

// Gear you fight with, which an opened ender chest would get in the way of.
fn combat_gear(game: &Game<'_>, stack: Option<&horizon_client::items::ItemStack>) -> bool {
    stack.is_some_and(|s| {
        let tagged = |tag: &str| game.res.items().item_tags.get(tag).is_some_and(|t| t.contains(&*s.item));
        tagged("minecraft:swords")
            || tagged("minecraft:axes")
            || matches!(
                &*s.item,
                "minecraft:mace" | "minecraft:end_crystal" | "minecraft:obsidian" | "minecraft:crying_obsidian" | "minecraft:totem_of_undying"
                    | "minecraft:experience_bottle" | "minecraft:glowstone"
            )
    })
}
