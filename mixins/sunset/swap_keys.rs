// What goes in your main hand for a fight: Shield (Auto block: the shield up while a player who can hit you is near,
// down when an axe or a crit would get through it; Swap key: a shield in hand on a key, from the hotbar or the main
// inventory, and back on the next press), Key Throw's Crossbow key (a crossbow in hand, from the hotbar or the
// inventory, kept loaded while you hold it, put back when you change slot) and Offhand's Double hand (a hotbar totem
// in hand when a blast or a smash near an enemy could take more than the health you have). All of it at the start of
// a tick, before its move packet, and only in a tick no other slot change went in.
use super::sunset::{Category, Module, gui_open, hold, with};
use super::sunset_util as util;
use super::Game;
use horizon_client::items::ItemStack;
use horizon_client::menu::Input;
use std::sync::Mutex;
use winit::event::MouseButton;

const NAME: &str = "Shield";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Blocks with your shield in fights, or swaps one in on a key.")
            .bool("Auto block", true)
            .num("Target distance", 5.0, 1.0, 10.0, 0.5)
            .num("FOV", 90.0, 0.0, 180.0, 5.0)
            .num("Movement threshold", 0.1, 0.0, 10.0, 0.1)
            .num("Sword cooldown correction (%)", 10.0, 0.0, 30.0, 1.0)
            .bool("Disable tracker", true)
            .bool("Swap to sword", false)
            .text("Swap key", "None")
            .bool("From inventory", true),
    ]
}

// An item borrowed into the hand: the slot held before, the hotbar slot it is in, and the inventory slot it came from
// (swapped into the held slot) if it was not in the hotbar.
#[derive(Clone, Copy)]
struct Borrow {
    previous: usize,
    slot: usize,
    from: Option<usize>,
}

#[derive(Default)]
struct State {
    // Auto block holds the use button (Some(true)) or keeps it up (Some(false)); None: it is yours.
    pressed: Option<bool>,
    // Your last hit: Auto block looks again two ticks after it.
    hit_tick: u64,
    // The tick each player near you last started a swing, and their swing clock last tick.
    swings: Vec<(u32, u64, f32)>,
    // The shield's cooldown (disabled by an axe) seen last tick.
    disabled: bool,
    shield: Option<Borrow>,
    crossbow: Option<Borrow>,
    // The crossbow key holds the use button.
    loading: bool,
    // Double hand: not again before this tick; the pearl cooldown seen last tick.
    double_next: u64,
    pearl_cooling: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

fn live(game: &Game<'_>) -> bool {
    game.remote.is_some()
        && game.native_playing()
        && !gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
        && util::inventory_idle(game)
}

fn is(stack: Option<&ItemStack>, name: &str) -> bool {
    stack.is_some_and(|s| util::short(&s.item) == name)
}

fn tagged(game: &Game<'_>, item: &str, tag: &str) -> bool {
    game.res.items().item_tags.get(tag).is_some_and(|t| t.contains(item))
}

// Puts `name` in hand: a hotbar one is selected; else, with `inventory`, one from the main inventory changes places
// with what you hold.
fn bring(game: &mut Game<'_>, name: &str, inventory: bool) -> Option<Borrow> {
    if is(game.held(), name) {
        return None;
    }
    let previous = game.selected;
    if let Some(slot) = (0..9).find(|&i| is(game.stack_in(i), name)) {
        hold(game, slot);
        return Some(Borrow { previous, slot, from: None });
    }
    let from = (9..36).find(|&i| is(game.stack_in(i), name)).filter(|_| inventory && !super::sunset_loadouts::locked(previous))?;
    game.remote_menu_ops(&[(Some(from), previous as i32, Input::Swap)]);
    Some(Borrow { previous, slot: previous, from: Some(from) })
}

// What `bring` did, undone: the item goes back where it came from, or the slot you held before is held again.
fn put_back(game: &mut Game<'_>, b: Borrow) {
    match b.from {
        Some(from) => {
            game.remote_menu_ops(&[(Some(from), b.slot as i32, Input::Swap)]);
        }
        None if game.selected == b.slot => hold(game, b.previous),
        None => {}
    }
}

// The use button as you hold it.
fn yours(game: &mut Game<'_>) {
    let down = game.physical_buttons.contains(&MouseButton::Right);
    let _ = game.play_mouse(MouseButton::Right, down);
}

fn press(game: &mut Game<'_>, down: bool) {
    if st(|s| s.pressed) != Some(down) {
        let _ = game.play_mouse(MouseButton::Right, down);
    }
    st(|s| s.pressed = Some(down));
}

fn let_go(game: &mut Game<'_>) {
    if st(|s| s.pressed.take()).is_some() {
        yours(game);
    }
}

// The key handler: Shield's Swap key and Key Throw's Crossbow key.
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !live(game) {
        return;
    }
    let pressed_key = super::sunset::key_from_text(key);
    let named = |m: &Module, setting: &str| super::sunset::key_from_text(m.t(setting)).is_some_and(|k| Some(k) == pressed_key);
    if let Some(m) = module(NAME).filter(|m| named(m, "Swap key")) {
        match st(|s| s.shield.take()) {
            Some(b) => put_back(game, b),
            None => {
                let b = bring(game, "shield", m.b("From inventory"));
                st(|s| s.shield = b);
            }
        }
    }
    if let Some(m) = module("Key Throw").filter(|m| named(m, "Crossbow key"))
        && st(|s| s.crossbow.is_none())
    {
        let b = bring(game, "crossbow", m.b("Crossbow from inventory"));
        st(|s| s.crossbow = b);
    }
}

// The start of a tick; `busy`: a slot changed already in it (a hit's weapon, a slot coming back).
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    if !live(game) {
        st(|s| {
            s.shield = None;
            s.crossbow = None;
            s.loading = false;
            s.swings.clear();
        });
        let_go(game);
        return;
    }
    let (loaded, kept) = (crossbow(game), swap_key(game));
    let busy = busy || !loaded || !kept;
    note_swings(game);
    let busy = busy || double_hand(game, busy);
    match module(NAME).filter(|m| m.b("Auto block")).and_then(|m| block(game, &m, busy)) {
        Some(down) => press(game, down),
        None => let_go(game),
    }
}

// Swap key's shield: it goes back if you change slot or it leaves your hand. False: a slot changed.
fn swap_key(game: &mut Game<'_>) -> bool {
    let Some(b) = st(|s| s.shield) else {
        return true;
    };
    if module(NAME).is_none() || game.selected != b.slot || !is(game.stack_in(b.slot), "shield") {
        st(|s| s.shield = None);
        if b.from.is_some() && game.selected != b.slot {
            put_back(game, b);
            return false;
        }
    }
    true
}

// Crossbow key: the use button held until the crossbow is loaded; again after each shot while it is in hand; once you
// hold another slot it goes back. False: a slot changed.
fn crossbow(game: &mut Game<'_>) -> bool {
    let Some(b) = st(|s| s.crossbow) else {
        return true;
    };
    if module("Key Throw").is_none() || game.selected != b.slot || !is(game.held(), "crossbow") {
        if st(|s| std::mem::take(&mut s.loading)) {
            yours(game);
        }
        st(|s| s.crossbow = None);
        if game.selected != b.slot && b.from.is_some() {
            put_back(game, b);
            return false;
        }
        return true;
    }
    let Some(stack) = game.held() else {
        return true;
    };
    let loaded = stack.data().is_some_and(|d| !d.charged.is_empty());
    let pull = (25 - 5 * stack.enchantment_level("minecraft:quick_charge") as i64).max(1) as u32;
    let drawn = game.using.is_some_and(|(slot, ticks)| slot == game.selected && ticks >= pull);
    if loaded || drawn {
        if st(|s| std::mem::take(&mut s.loading)) {
            yours(game);
        }
    } else if !st(|s| s.loading) {
        let _ = game.play_mouse(MouseButton::Right, true);
        st(|s| s.loading = true);
    }
    true
}

// Each player's swing start, from their swing clock going up from rest.
fn note_swings(game: &Game<'_>) {
    let now = game.ticks;
    st(|s| {
        s.swings.retain(|(id, _, _)| game.mobs.iter().any(|m| m.id == *id));
        for mob in game.mobs.iter().filter(|m| m.kind.id == "minecraft:player") {
            let anim = mob.anim.attack_anim;
            match s.swings.iter_mut().find(|(id, _, _)| *id == mob.id) {
                Some((_, at, was)) => {
                    if anim > 0.0 && (*was == 0.0 || anim < *was) {
                        *at = now;
                    }
                    *was = anim;
                }
                None => s.swings.push((mob.id, 0, anim)),
            }
        }
    });
}

// Your hit: Auto block waits two ticks before looking again.
pub(super) fn hit(game: &Game<'_>) {
    st(|s| s.hit_tick = game.ticks);
}

// Auto block, each tick: Some(true) to hold the shield up, Some(false) to keep it down, None to leave the button to you.
fn block(game: &mut Game<'_>, m: &Module, busy: bool) -> Option<bool> {
    // Disable tracker: an axe took the shield out (its cooldown runs): the button is yours, and with Swap to sword the
    // sword comes out.
    let shield_down = game.cooldowns.is_on_cooldown("minecraft:shield");
    let newly = shield_down && !st(|s| std::mem::replace(&mut s.disabled, shield_down));
    if !shield_down {
        st(|s| s.disabled = false);
    }
    if m.b("Disable tracker") && shield_down {
        if newly
            && !busy
            && m.b("Swap to sword")
            && let Some(slot) = (0..9).find(|&i| game.stack_in(i).is_some_and(|s| tagged(game, &s.item, "minecraft:swords")))
        {
            hold(game, slot);
        }
        return None;
    }
    if game.ticks < st(|s| s.hit_tick) + 2 {
        return st(|s| s.pressed);
    }
    let p = game.player.as_ref()?;
    let held = game.held().map(|s| s.item.clone());
    let free_hand = held.as_ref().is_none_or(|h| tagged(game, h, "minecraft:swords") || tagged(game, h, "minecraft:axes") || &**h == "minecraft:mace");
    let shield = is(game.held(), "shield") || (is(game.offhand.as_ref(), "shield") && free_hand);
    if !shield || p.fall_flying {
        return None;
    }
    let eye = game.eye_position()?;
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let me = [p.position.x, p.position.y, p.position.z];
    let target = game
        .mobs
        .iter()
        .filter(|t| util::player_enemy(game, t))
        .filter(|t| {
            let to = [t.position.x - eye[0], t.position.y + 0.9 - eye[1], t.position.z - eye[2]];
            let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt().max(1e-9);
            let angle = ((0..3).map(|i| look[i] * to[i]).sum::<f64>() / len).clamp(-1.0, 1.0).acos().to_degrees();
            angle <= m.n("FOV")
        })
        .min_by(|a, b| util::dist(me, [a.position.x, a.position.y, a.position.z]).total_cmp(&util::dist(me, [b.position.x, b.position.y, b.position.z])))?;
    let away = util::dist(me, [target.position.x, target.position.y, target.position.z]);
    if away > m.n("Target distance") {
        return None;
    }
    let weapon = target.extra.main_hand.as_deref().unwrap_or("");
    // An axe would take the shield out; anything but a sword or a mace does little.
    if tagged(game, weapon, "minecraft:axes") || !(tagged(game, weapon, "minecraft:swords") || weapon == "minecraft:mace") {
        return Some(false);
    }
    // Your own crit on the way down: no shield in the way of it.
    let strength = super::sunset_melee::slot_strength(game, game.selected, 0.0).unwrap_or(0.0);
    if !p.ground.on_ground && p.fall_distance > 0.0 && strength > 0.94 {
        return Some(false);
    }
    // Their sword charged again (a little early, by the correction), and coming at you or within 3 blocks.
    let swung = st(|s| s.swings.iter().find(|(id, _, _)| *id == target.id).map_or(0, |(_, at, _)| *at));
    let ready = (game.ticks.saturating_sub(swung)) as f64 * 50.0 >= 625.0 * (0.94 - m.n("Sword cooldown correction (%)") / 100.0);
    let motion = [target.position.x - target.previous.x, target.position.y - target.previous.y, target.position.z - target.previous.z];
    let speed = (motion[0] * motion[0] + motion[1] * motion[1] + motion[2] * motion[2]).sqrt();
    let to_me = [me[0] - target.position.x, me[1] - target.position.y, me[2] - target.position.z];
    let coming = speed >= m.n("Movement threshold") / 10.0 && (0..3).map(|i| motion[i] * to_me[i]).sum::<f64>() > 0.0;
    if ready && (coming || away <= 3.0) {
        return Some(true);
    }
    // Going up from a jump, the shield stays up; otherwise it comes down.
    Some(!p.ground.on_ground && p.fall_distance <= 0.0)
}

// Offhand's Double hand, each tick (Reswap delay ticks apart): with a totem in the hotbar but not in hand, an enemy
// player within 10 blocks, and no shield up or combat module's hand in use, the totem goes in hand when a threat could
// take what health you have: a crystal there, a charged anchor within 5 of the enemy, (in the air) obsidian below you
// within 5 of them with room for a crystal (counted at half), their mace on the way down at you (6 and 5 a block
// fallen); scaled by Sensitivity. Also when they hold a sword or axe and you are under one health, and (Pearl) as
// you throw a pearl. True: the slot changed.
fn double_hand(game: &mut Game<'_>, busy: bool) -> bool {
    let cooling = game.cooldowns.is_on_cooldown("minecraft:ender_pearl");
    let thrown = cooling && !st(|s| std::mem::replace(&mut s.pearl_cooling, cooling));
    if !cooling {
        st(|s| s.pearl_cooling = false);
    }
    let Some(m) = module("Offhand").filter(|m| m.b("Double hand")) else {
        return false;
    };
    let on = |name: &str| util::listed(m.t("Double hand on"), name);
    if busy || game.ticks < st(|s| s.double_next) || is(game.held(), "totem_of_undying") || game.using.is_some() {
        return false;
    }
    if super::sunset_combat_control::owner().is_some() {
        return false;
    }
    let Some(slot) = (0..9).find(|&i| is(game.stack_in(i), "totem_of_undying")) else {
        return false;
    };
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health() + p.health.absorption());
    let (me, airborne) = ([p.position.x, p.position.y, p.position.z], !p.ground.on_ground);
    let enemy = game
        .mobs
        .iter()
        .filter(|t| util::player_enemy(game, t))
        .map(|t| (t, util::dist(me, [t.position.x, t.position.y, t.position.z])))
        .filter(|(_, d)| *d <= 10.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| (t.id, [t.position.x, t.position.y, t.position.z], t.extra.main_hand.clone(), t.fall_distance));
    let pearl = thrown && on("pearl");
    let threat = enemy.as_ref().is_some_and(|(_, at, weapon, fall)| {
        let weapon = weapon.as_deref().unwrap_or("");
        let sword = tagged(game, weapon, "minecraft:swords") || tagged(game, weapon, "minecraft:axes");
        let worst = worst_threat(game, &on, *at, me, airborne, weapon == "minecraft:mace", *fall);
        worst * m.n("Double hand sensitivity") >= health || (on("sword") && sword && health < 1.0)
    });
    if !(threat || pearl) {
        return false;
    }
    hold(game, slot);
    st(|s| s.double_next = game.ticks + m.n("Reswap delay (ticks)").max(1.0) as u64);
    true
}

// The most a threat near the enemy at `at` could take from you, each counted by how sure it is.
fn worst_threat(game: &mut Game<'_>, on: &dyn Fn(&str) -> bool, at: [f64; 3], me: [f64; 3], airborne: bool, mace: bool, fall: f64) -> f64 {
    use super::sunset_explosive_damage::{Blast, observed_local_damage};
    let mut budget = super::sunset_explosive_policy::Budget::new(24);
    let mut worst: f64 = 0.0;
    let near = |p: [i32; 3]| util::dist(at, util::center_of(p)) < 5.0;
    if on("crystals") {
        let crystals: Vec<[f64; 3]> = game.crystals.iter().map(|c| [c.position.x, c.position.y, c.position.z]).collect();
        for c in crystals {
            worst = worst.max(observed_local_damage(game, Blast::explosion(c, 6.0), &mut budget).unwrap_or(0.0));
        }
    }
    let base = at.map(|v| v.floor() as i32);
    let mut anchors = Vec::new();
    let mut bases = Vec::new();
    for x in -5..=5 {
        for y in -5..=5 {
            for z in -5..=5 {
                let p = [base[0] + x, base[1] + y, base[2] + z];
                if !near(p) || !game.world.loaded(p[0], p[2]) {
                    continue;
                }
                let name = util::short(&util::facts(game, p).name);
                if name == "respawn_anchor" && on("anchors") && charged(game, p) {
                    anchors.push(p);
                } else if name == "obsidian" && on("obsidian") && airborne && f64::from(p[1]) < me[1] && util::is_air(game, [p[0], p[1] + 1, p[2]]) {
                    bases.push(p);
                }
            }
        }
    }
    for p in anchors {
        worst = worst.max(observed_local_damage(game, Blast::anchor(p), &mut budget).unwrap_or(0.0));
    }
    for p in bases {
        let crystal = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 1.0, f64::from(p[2]) + 0.5];
        worst = worst.max(0.5 * observed_local_damage(game, Blast::explosion(crystal, 6.0), &mut budget).unwrap_or(0.0));
    }
    if on("mace") && mace && fall > 1.5 && at[1] > me[1] {
        worst = worst.max(6.0 + fall * 5.0);
    }
    worst
}

fn charged(game: &Game<'_>, p: [i32; 3]) -> bool {
    let b = game.world.block(p[0], p[1], p[2]);
    game.pack.assets.registry.state(b).and_then(|s| s.properties.get("charges")).is_some_and(|c| c != "0")
}
