// Kill Aura, ported from Meteor's source with its settings: which weapons it
// attacks with, how it rotates and switches, who it targets and how many at
// once, and its timing.

use super::sunset::{Canvas, Category, Module, health_colour, hold, look_at};
use super::sunset_util::*;
use super::Game;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Kill Aura", Category::Combat, "Attacks the entities you choose around you.")
            .pick("Attack when holding", &["Weapons", "All"])
            .text("Selected weapon types", "diamond_sword,diamond_axe,trident")
            .pick("Rotate", &["Always", "On hit", "None"])
            .bool("Auto switch", false)
            .bool("Swap back", false)
            .bool("Only on click", false)
            .bool("Only on look", false)
            .text("Entities", "player")
            .pick("Priority", PRIORITIES)
            .num("Max targets", 1.0, 1.0, 5.0, 1.0)
            .num("Range", 4.5, 1.0, 6.0, 0.1)
            .num("Walls range", 3.5, 1.0, 6.0, 0.1)
            .pick("Passive mob age filter", &["Adult", "Baby", "Both"])
            .pick("Hostile mob age filter", &["Both", "Adult", "Baby"])
            .bool("Ignore named", false)
            .bool("Ignore passive", true)
            .bool("Anti bot", false)
            .bool("Ignore teams", false)
            .bool("Pause on use", false)
            .bool("Custom delay", false)
            .num("Hit delay", 11.0, 0.0, 60.0, 1.0)
            .num("Switch delay", 0.0, 0.0, 10.0, 1.0),
    ]
}

#[derive(Default)]
struct As {
    hit_timer: u32,
    switch_timer: u32,
    last_selected: Option<usize>,
    swapped: bool,
    prev: usize,
    attacking: bool,
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

// How far the eyes are from the nearest point of the target's box: what Range measures.
fn reach(eye: [f64; 3], t: &Target) -> f64 {
    dist(eye, [eye[0].clamp(t.lo[0], t.hi[0]), eye[1].clamp(t.lo[1], t.hi[1]), eye[2].clamp(t.lo[2], t.hi[2])])
}

fn entity_check(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3]) -> bool {
    if t.health <= 0.0 || spared(game, m, t.id, t.player) {
        return false;
    }
    if reach(eye, t) > m.n("Range") || !entity_listed(m.t("Entities"), t) {
        return false;
    }
    let mob = game.mobs.iter().find(|x| x.id == t.id);
    if m.b("Ignore named") && mob.is_some_and(|x| x.custom_name.is_some()) {
        return false;
    }
    let centre = [(t.lo[0] + t.hi[0]) / 2.0, (t.lo[1] + t.hi[1]) / 2.0, (t.lo[2] + t.hi[2]) / 2.0];
    if !clear_line(game, centre, [i32::MIN; 3]) && dist(eye, t.pos) > m.n("Walls range") {
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
    let (was, swapped, prev) = st(|s| (std::mem::take(&mut s.attacking), s.swapped, s.prev));
    if was && m.b("Swap back") && swapped {
        hold(game, prev);
        st(|s| s.swapped = false);
    }
}

pub(super) fn tick(game: &mut Game<'_>, m: &Module, crits: bool) {
    st(|s| s.target = None);
    if !game.native_playing() || game.remote.is_none() || super::sunset::CRIT_STAGE.load(Ordering::Relaxed) != 0 {
        return;
    }
    // A change of hotbar slot starts the switch delay.
    let sel = game.selected;
    if st(|s| s.last_selected.replace(sel)) != Some(sel) {
        st(|s| s.switch_timer = m.n("Switch delay") as u32);
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
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
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
    list.truncate(if m.b("Only on look") { 1 } else { m.n("Max targets") as usize });
    if list.is_empty() {
        stop(game, m);
        return;
    }
    // Pick the weapon.
    if m.b("Auto switch") {
        let want = find_hotbar(game, |n| acceptable(m, Some(n)));
        if let Some(slot) = want {
            if !st(|s| s.swapped) {
                st(|s| {
                    s.prev = game.selected;
                    s.swapped = true;
                });
            }
            if slot != game.selected {
                hold(game, slot);
            }
        }
    }
    let held = game.held().map(|s| short(&s.item).to_owned());
    if !acceptable(m, held.as_deref()) {
        stop(game, m);
        return;
    }
    let primary = &list[0];
    st(|s| {
        s.attacking = true;
        s.target = Some(primary.id);
    });
    let centre = [(primary.lo[0] + primary.hi[0]) / 2.0, (primary.lo[1] + primary.hi[1]) / 2.0, (primary.lo[2] + primary.hi[2]) / 2.0];
    let rotate_mode = m.p("Rotate");
    if rotate_mode == "Always" {
        let (yaw, pitch) = look_at(eye, centre);
        super::sunset::turn("Kill Aura", yaw, pitch);
    }
    // The delay: the game's cooldown, or a fixed number of ticks.
    if st(|s| {
        if s.switch_timer > 0 {
            s.switch_timer -= 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let ready = if m.b("Custom delay") {
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
    if !ready {
        return;
    }
    for t in &list {
        let on_hit = (rotate_mode == "On hit").then(|| look_at(eye, [(t.lo[0] + t.hi[0]) / 2.0, (t.lo[1] + t.hi[1]) / 2.0, (t.lo[2] + t.hi[2]) / 2.0]));
        super::sunset::attack(game, t.id, crits, on_hit);
    }
    st(|s| s.hit_timer = 0);
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
