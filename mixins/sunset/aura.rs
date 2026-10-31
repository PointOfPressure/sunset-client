// Original native melee planner. One observed target, one accepted deed, and
// the game's actual cooldown/ray/item mechanics determine every attack.

use super::Game;
use super::sunset::{Canvas, Category, Module, health_colour, hold, look_at};
use super::sunset_combat_control as control;
use super::sunset_melee_assist as melee;
use super::sunset_melee_policy as policy;
use super::sunset_util::*;
use horizon_client::spear::Kinetic;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Kill Aura", Category::Combat, "Attacks one visible, reachable enemy per native cooldown. Wait and Jump use real falling critical hits; hotbar changes occur only inside an accepted action.")
            .pick("Attack when holding", &["Weapons", "All"])
            .text("Selected weapon types", "diamond_sword,diamond_axe,trident,mace,diamond_spear")
            .pick("Rotate", &["Always", "On hit", "None"])
            .bool("Auto switch", false)
            .bool("Swap back", false)
            .bool("Only on click", false)
            .bool("Only on look", false)
            .text("Entities", "player")
            .pick("Priority", PRIORITIES)
            .num("Range", 3.0, 1.0, 6.5, 0.1)
            .pick("Crits", &["Wait", "Off", "Jump"])
            .pick("Passive mob age filter", &["Adult", "Baby", "Both"])
            .pick("Hostile mob age filter", &["Both", "Adult", "Baby"])
            .bool("Ignore named", false)
            .bool("Ignore passive", true)
            .bool("Anti bot", false)
            .bool("Ignore teams", false)
            .bool("Custom delay", false)
            .num("Hit delay", 11.0, 0.0, 60.0, 1.0)
            .num("Switch delay", 0.0, 0.0, 10.0, 1.0),
    ]
}

#[derive(Default)]
struct As {
    last_hit: u64,
    switch_until: u64,
    last_selected: Option<usize>,
    acquired: u64,
    pending: bool,
    pending_since: Option<Instant>,
    sent: u64,
    reason: &'static str,
    // Who it is hitting this tick (the Target HUD's).
    target: Option<u32>,
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
    m.t("Selected weapon types")
        .split(',')
        .any(|w| weapon_type(w.trim()) == Some(t))
}

pub(super) fn is_hostile(kind: &str) -> bool {
    const H: [&str; 24] = [
        "zombie",
        "husk",
        "drowned",
        "zombie_villager",
        "skeleton",
        "stray",
        "bogged",
        "wither_skeleton",
        "creeper",
        "spider",
        "cave_spider",
        "witch",
        "blaze",
        "ghast",
        "slime",
        "magma_cube",
        "phantom",
        "pillager",
        "vindicator",
        "evoker",
        "ravager",
        "vex",
        "guardian",
        "silverfish",
    ];
    let k = short(kind);
    H.contains(&k)
        || k == "piglin_brute"
        || k == "hoglin"
        || k == "zoglin"
        || k == "endermite"
        || k == "warden"
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
        "pig",
        "cow",
        "sheep",
        "chicken",
        "horse",
        "donkey",
        "mule",
        "llama",
        "rabbit",
        "wolf",
        "cat",
        "fox",
        "panda",
        "polar_bear",
        "goat",
        "camel",
        "mooshroom",
        "villager",
        "turtle",
        "axolotl",
        "bee",
        "sniffer",
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

// How far the eyes are from the nearest point of the target's box: what Range measures.
fn reach(eye: [f64; 3], t: &Target) -> f64 {
    dist(
        eye,
        [
            eye[0].clamp(t.lo[0], t.hi[0]),
            eye[1].clamp(t.lo[1], t.hi[1]),
            eye[2].clamp(t.lo[2], t.hi[2]),
        ],
    )
}

fn entity_check(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3]) -> bool {
    if !t.health.is_finite()
        || t.health <= 0.0
        || spared(game, m, t.id, t.player)
        || t.player
            && game.remote_player_name(t.id).is_some_and(|name| {
                super::sunset::with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(name)))
            })
    {
        return false;
    }
    if policy::box_distance(eye, t.lo, t.hi).is_none_or(|d| d > m.n("Range"))
        || !entity_listed(m.t("Entities"), t)
    {
        return false;
    }
    let mob = game.mobs.iter().find(|x| x.id == t.id);
    if m.b("Ignore named") && mob.is_some_and(|x| x.custom_name.is_some()) {
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

pub(super) fn diagnostics() -> String {
    st(|s| {
        format!(
            "KillAura reason={} target={:?} pending={} sent={}",
            s.reason, s.target, s.pending, s.sent
        )
    })
}

fn current(m: &Module) -> bool {
    super::sunset::with(|s| s.module("Kill Aura").is_some_and(|now| now.on && now == m))
}
fn allowed(game: &Game<'_>, m: &Module) -> bool {
    melee::attack_allowed(game, "Kill Aura")
        && (!m.b("Only on click")
            || game.left
            || game
                .physical_buttons
                .contains(&winit::event::MouseButton::Left))
}
fn weapon_slot(game: &Game<'_>, m: &Module, id: u32) -> Option<(usize, bool)> {
    let own = if acceptable(m, game.held().map(|s| &*s.item))
        && melee::slot_strength(game, game.selected, 0.5).is_some()
    {
        Some(game.selected)
    } else {
        None
    };
    let slot =
        if m.b("Auto switch") {
            own.or_else(|| {
                (0..9).find(|&slot| {
                    game.inventory
                        .get(slot)
                        .and_then(Option::as_ref)
                        .is_some_and(|s| {
                            game.res.items().get(&s.item).is_some_and(|info| {
                                policy::usable(s.count, s.damage, info.max_damage)
                            }) && acceptable(m, Some(&s.item))
                        })
                })
            })
        } else {
            own
        }?;
    let assist = melee::preferred_weapon(game, id);
    Some(assist.map_or((slot, m.b("Swap back")), |p| (p.slot, p.restore)))
}
fn aim(game: &Game<'_>, m: &Module, t: &Target, slot: usize) -> Option<(f32, f32)> {
    let eye = game.eye_position()?;
    let spear = game
        .inventory
        .get(slot)
        .and_then(Option::as_ref)
        .is_some_and(|s| Kinetic::of(&s.item).is_some());
    if m.p("Rotate") == "None" || m.b("Only on look") {
        return melee::hits_target(game, t.id, game.yaw, game.pitch, spear, m.n("Range"))
            .then_some((game.yaw, game.pitch));
    }
    // Inset points let an exposed head/side remain hittable when a central
    // body ray is covered. Every candidate still has to hit this mob first.
    let mid = [
        (t.lo[0] + t.hi[0]) * 0.5,
        (t.lo[1] + t.hi[1]) * 0.5,
        (t.lo[2] + t.hi[2]) * 0.5,
    ];
    let inset = std::array::from_fn(|i| {
        eye[i].clamp(
            t.lo[i] + (t.hi[i] - t.lo[i]) * 0.1,
            t.hi[i] - (t.hi[i] - t.lo[i]) * 0.1,
        )
    });
    let high = [mid[0], t.lo[1] + (t.hi[1] - t.lo[1]) * 0.9, mid[2]];
    let low = [mid[0], t.lo[1] + (t.hi[1] - t.lo[1]) * 0.2, mid[2]];
    [inset, high, mid, low].into_iter().find_map(|point| {
        let (yaw, pitch) = look_at(eye, point);
        melee::hits_target(game, t.id, yaw, pitch, spear, m.n("Range")).then_some((yaw, pitch))
    })
}
fn critical_ready(game: &Game<'_>, m: &Module, acquired: u64, slot: usize) -> bool {
    let Some(scale) = melee::slot_strength(game, slot, 0.5) else {
        return false;
    };
    // Smash is native and need not wait for a normal critical condition.
    let smash = game
        .inventory
        .get(slot)
        .and_then(Option::as_ref)
        .is_some_and(|s| &*s.item == "minecraft:mace")
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying);
    smash
        || policy::critical_ready(
            m.p("Crits"),
            game.critical_attack(scale),
            game.ticks.saturating_sub(acquired),
            8,
        )
}

pub(super) fn tick(game: &mut Game<'_>, m: &Module, _crits: bool) {
    if !current(m) || !allowed(game, m) {
        super::sunset::cancel_turns("Kill Aura");
        st(|s| {
            s.pending = false;
            s.target = None;
        });
        return;
    }
    if st(|s| s.pending) {
        if st(|s| {
            s.pending_since
                .is_none_or(|t| t.elapsed() > Duration::from_millis(200))
        }) {
            super::sunset::cancel_turns("Kill Aura");
            st(|s| {
                s.pending = false;
                s.pending_since = None;
                s.reason = "expired deferred action";
            });
            return;
        }
        let _ = control::claim("Kill Aura");
        return;
    }
    if st(|s| s.last_selected.replace(game.selected)) != Some(game.selected) {
        st(|s| {
            s.switch_until = game
                .ticks
                .saturating_add(m.n("Switch delay").clamp(0.0, 10.0) as u64)
        });
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let old = st(|s| s.target);
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let friends = super::sunset::with(|s| s.friends.clone());
    let names = game.remote_player_names();
    let look = m
        .b("Only on look")
        .then(|| game.mob_hit().map(|(i, _)| game.mobs[i].id))
        .flatten();
    let mut list: Vec<Target> = game
        .mobs
        .iter()
        .filter(|x| x.alive() && !x.removed && Some(x.id) != own && Some(x.id) != game.riding)
        .filter(|x| !m.b("Only on look") || Some(x.id) == look)
        .map(mob_target)
        .filter(|t| {
            !(t.player
                && names.iter().any(|(id, n)| {
                    *id == t.id && friends.iter().any(|f| f.eq_ignore_ascii_case(n))
                }))
                && entity_check(game, m, t, eye)
        })
        .collect();
    sort_targets(game, &mut list, m.p("Priority"));
    // Focus a still valid victim until it leaves; at most eight ray candidates
    // are considered from this bounded population in one planning tick.
    if let Some(index) = list.iter().position(|t| Some(t.id) == old) {
        let kept = list.remove(index);
        list.insert(0, kept);
    }
    let selected = list.iter().take(8).find_map(|t| {
        let (slot, restore) = weapon_slot(game, m, t.id)?;
        let look = aim(game, m, t, slot)?;
        Some((t.clone(), slot, restore, look))
    });
    let Some((target, slot, restore, (yaw, pitch))) = selected else {
        st(|s| {
            s.target = None;
            s.reason = "no visible reachable enemy/weapon";
        });
        return;
    };
    st(|s| {
        if s.target != Some(target.id) {
            s.target = Some(target.id);
            s.acquired = game.ticks;
        }
    });
    let acquired = st(|s| s.acquired);
    if game.ticks < st(|s| s.switch_until) {
        return;
    }
    let ready = if m.b("Custom delay") {
        game.ticks.saturating_sub(st(|s| s.last_hit)) >= m.n("Hit delay").clamp(0.0, 60.0) as u64
    } else {
        melee::slot_strength(game, slot, 0.5).is_some_and(|scale| scale >= 1.0)
    };
    if !ready {
        return;
    }
    if !critical_ready(game, m, acquired, slot) {
        if m.p("Crits") == "Jump"
            && game.player.as_ref().is_some_and(|p| {
                p.ground.on_ground
                    && !p.flying
                    && !p.fall_flying
                    && !p.swimming
                    && !p.fluids.was_touching_water
                    && !p.is_in_lava()
                    && p.last_climbable.is_none()
            })
            && game.riding.is_none()
        {
            melee::request_jump("Kill Aura");
        }
        st(|s| s.reason = "waiting for real critical fall");
        return;
    }
    if !control::claim("Kill Aura") {
        return;
    }
    let done = control::finish("Kill Aura");
    let Some(session) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return;
    };
    let dimension = game.dim;
    let queued = Instant::now();
    let snapshot = m.clone();
    let original = game.selected;
    let id = target.id;
    st(|s| {
        s.pending = true;
        s.pending_since = Some(queued);
    });
    let accepted = super::sunset::try_turn_then("Kill Aura", yaw, pitch, move |game| {
        let done = done;
        st(|s| s.pending = false);
        if !done.current()
            || !current(&snapshot)
            || !allowed(game, &snapshot)
            || game.selected != original
            || game.dim != dimension
            || queued.elapsed() > Duration::from_millis(200)
            || !game
                .remote
                .as_ref()
                .is_some_and(|r| std::sync::Arc::ptr_eq(&session, &r.session.counters))
        {
            return;
        }
        let Some(mob) = game
            .mobs
            .iter()
            .find(|t| t.id == id && t.alive() && !t.removed)
        else {
            return;
        };
        let t = mob_target(mob);
        let Some(eye) = game.eye_position() else {
            return;
        };
        if !entity_check(game, &snapshot, &t, eye)
            || !weapon_slot(game, &snapshot, id).is_some_and(|(now, _)| now == slot)
            || !game.inventory.get(slot).and_then(Option::as_ref).map_or(
                snapshot.p("Attack when holding") == "All",
                |s| {
                    game.res
                        .items()
                        .get(&s.item)
                        .is_some_and(|i| policy::usable(s.count, s.damage, i.max_damage))
                },
            )
            || !critical_ready(game, &snapshot, acquired, slot)
        {
            return;
        }
        let spear = game
            .inventory
            .get(slot)
            .and_then(Option::as_ref)
            .is_some_and(|s| Kinetic::of(&s.item).is_some());
        if !melee::hits_target(game, id, yaw, pitch, spear, snapshot.n("Range")) {
            return;
        }
        if !snapshot.b("Custom delay")
            && !melee::slot_strength(game, slot, 0.5).is_some_and(|scale| scale >= 1.0)
        {
            return;
        }
        hold(game, slot);
        let sent = melee::send_hit_at(game, id, yaw, pitch);
        if restore && game.selected == slot {
            hold(game, original);
        }
        if sent {
            st(|s| {
                s.sent = s.sent.saturating_add(1);
                s.last_hit = game.ticks;
                s.acquired = game.ticks;
                s.reason = "native hit sent";
            });
        }
    });
    if !accepted {
        st(|s| s.pending = false);
    }
}

// The Target HUD: a box by the crosshair with the name, distance and health of who Kill Aura is hitting.
// Potato: the health bar alone, no text.
pub(super) fn draw_target(game: &Game<'_>, w: f32, h: f32, potato: bool, out: &mut impl Canvas) {
    let Some(id) = st(|s| s.target) else {
        return;
    };
    let (Some(mob), Some(eye)) = (game.mobs.iter().find(|x| x.id == id), game.eye_position())
    else {
        return;
    };
    let fraction = (mob.health / mob.kind.max_health.max(1.0)).clamp(0.0, 1.0);
    let (x, y) = (w / 2.0 + 12.0, h / 2.0 + 12.0);
    if potato {
        out.fill([x, y, 60.0, 4.0], 0xFF30_3030);
        out.fill(
            [x, y, 60.0 * fraction, 4.0],
            0xFF00_0000 | health_colour(fraction),
        );
        return;
    }
    let name = display_name(game, mob);
    let away = format!("{:.1}m", reach(eye, &mob_target(mob)));
    let health = format!("{:.1}", (mob.health * 10.0).round() / 10.0);
    let (nw, aw, hw) = (
        game.native_text_width(&name),
        game.native_text_width(&away),
        game.native_text_width(&health),
    );
    let width = (nw + aw + 16.0).max(90.0);
    out.fill([x, y, width, 26.0], 0x9000_0000);
    out.text(&name, [x + 4.0, y + 3.0], 0xFFFFFF);
    out.text(&away, [x + width - 4.0 - aw, y + 3.0], 0xFFFFFF);
    let bar = width - 12.0 - hw;
    out.fill([x + 4.0, y + 17.0, bar, 4.0], 0xFF30_3030);
    out.fill(
        [x + 4.0, y + 17.0, bar * fraction, 4.0],
        0xFF00_0000 | health_colour(fraction),
    );
    out.text(
        &health,
        [x + width - 4.0 - hw, y + 14.0],
        health_colour(fraction),
    );
}
