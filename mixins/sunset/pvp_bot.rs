// PvP Bot. Sword mode is from Rust's PR #16 (its settings and what it does; written small here): fights the nearest
// enemy player with your sword by itself. It turns you towards them at a human pace, walks and sprints in, keeps its
// distance (Patient) or presses on (Aggressive), strafes, jumps for critical hits and on being hit (Jump reset), and
// eats a golden apple when low. Hits go through the strikes queue like Kill Aura's. Your own movement keys take over.
// Crystal mode follows the behaviour of #16's crystal controller, with Sunset's own modules as its hands: it walks
// into crystal range, holes up (into a hole nearby, else Surround where it stands), and while it fights keeps Crystal
// Aura, Anchor Aura, Offhand and Auto Gap on, Self Trap when low, Auto City on a target in a hole, and pearls away
// when low with no totem left. What it learns of each rival (Rust's PR #20 store) changes how it fights them again.
use super::sunset::{self, Category, Module, hold, own_id, with};
use super::sunset_util as util;
use super::{Game, Screen};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub(super) const NAME: &str = "PvP Bot";
const KEYS: [KeyCode; 6] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD, KeyCode::ControlLeft, KeyCode::Space];
// The vanilla reach to a player's box, with a little kept back.
const REACH: f64 = 2.9;
// The modules Crystal mode drives.
const HANDS: [&str; 8] = ["Crystal Aura", "Anchor Aura", "Offhand", "Auto Gap", "Surround", "Self Trap", "Auto City", "Key Throw"];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Fights the nearest player: with a sword, or with crystals and anchors.")
            .pick("Mode", &["Sword", "Crystal"])
            .pick("Style", &["Patient", "Aggressive"])
            .num("Fight range", 16.0, 6.0, 32.0, 1.0)
            .num("Turn speed", 1.0, 0.5, 1.5, 0.05)
            .bool("Strafe", true)
            .bool("Jump crits", true)
            .bool("Jump reset", true)
            .num("Gapple below HP", 10.0, 0.0, 20.0, 1.0)
            .num("Crystal range", 4.5, 3.0, 6.0, 0.5)
            .bool("Use anchors", true)
            .bool("Hole up", true)
            .num("Self trap below HP", 12.0, 0.0, 36.0, 1.0)
            .bool("City", true)
            .num("Pearl below HP", 8.0, 0.0, 20.0, 1.0)
            .bool("Learn rivals", true),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    let crystal = m.p("Mode") == "Crystal";
    match name {
        "Style" | "Strafe" | "Jump crits" | "Jump reset" | "Gapple below HP" => !crystal,
        "Crystal range" | "Use anchors" | "Hole up" | "Self trap below HP" | "City" | "Pearl below HP" | "Learn rivals" => crystal,
        _ => true,
    }
}

#[derive(Default)]
struct Bot {
    // The keys the bot holds down (never one you hold yourself).
    pressed: HashSet<KeyCode>,
    // Strafing right (else left), and the tick it turns the other way.
    right: bool,
    flip_at: u64,
    // Eating: the slot to go back to.
    eating: Option<usize>,
    health: f32,
    // Ticks the hit has been ready for, on the ground, waiting for a jump to fall.
    ready_for: u32,
    // Crystal mode: the modules it turned on (each off before), and the tick each last went on.
    leased: Vec<&'static str>,
    leased_at: HashMap<&'static str, u64>,
    fight: Option<Fight>,
    pearl_at: u64,
}

// The fight with one rival: what it started at, for their history.
struct Fight {
    id: u32,
    uuid: Option<u128>,
    // Totems popped (theirs, yours) as it began, and the most seen since.
    start: (u32, u32),
    seen: (u32, u32),
    ticks: u32,
    hole_ticks: u32,
}

// What the bot learnt of a rival over its fights: their totems it popped, yours they popped, and how much of the time
// they sat in a hole.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct Rival {
    pub fights: u32,
    pub pops: u32,
    pub taken: u32,
    pub ticks: u32,
    pub hole_ticks: u32,
}

impl Rival {
    fn text(&self) -> String {
        serde_json::json!({"version": 1, "fights": self.fights, "pops": self.pops, "taken": self.taken,
            "ticks": self.ticks, "hole_ticks": self.hole_ticks})
        .to_string()
    }
    fn parse(text: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        if v["version"].as_u64() != Some(1) {
            return None;
        }
        let n = |k: &str| v[k].as_u64().and_then(|n| u32::try_from(n).ok());
        Some(Self { fights: n("fights")?, pops: n("pops")?, taken: n("taken")?, ticks: n("ticks")?, hole_ticks: n("hole_ticks")? })
    }
    // Sits in a hole half the time or more: Auto City is ready from the start.
    fn holes_up(&self) -> bool {
        self.fights > 0 && self.hole_ticks * 2 >= self.ticks
    }
    // Has popped more of your totems than you of theirs: the pearl goes earlier.
    fn beats_you(&self) -> bool {
        self.taken > self.pops
    }
}

// Rivals by UUID: None while the store is read (off the game thread).
static RIVALS: Mutex<Option<HashMap<u128, Option<Rival>>>> = Mutex::new(None);

fn rivals<R>(f: impl FnOnce(&mut HashMap<u128, Option<Rival>>) -> R) -> R {
    f(RIVALS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

static BOT: Mutex<Option<Bot>> = Mutex::new(None);

fn bot<R>(f: impl FnOnce(&mut Bot) -> R) -> R {
    f(BOT.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Bot::default))
}

// Lets go of the keys the bot holds, stops eating, turns off the modules Crystal mode turned on and ends the fight
// (its rival history saved).
pub(super) fn release(game: &mut Game<'_>) {
    let (pressed, eating) = bot(|b| (std::mem::take(&mut b.pressed), b.eating.take()));
    for key in pressed {
        if !game.physical_keys.contains(&key) {
            game.keys.remove(&key);
        }
    }
    if let Some(back) = eating {
        let _ = game.play_mouse(MouseButton::Right, false);
        hold(game, back);
    }
    for name in HANDS {
        lease(game, name, false);
    }
    end_fight(game);
}

fn busy() -> bool {
    bot(|b| !b.pressed.is_empty() || b.eating.is_some() || !b.leased.is_empty() || b.fight.is_some())
}

// Set while the bot itself turns a module on, so its toggle isn't taken for yours.
static LEASING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// A module Crystal mode drives, on or off: turned on only when it is off (then saved as off), turned off only when
// the bot turned it on. One that turned itself off again is turned back on after 20 ticks at the soonest.
fn lease(game: &mut Game<'_>, name: &'static str, on: bool) {
    use std::sync::atomic::Ordering;
    let is_on = with(|s| s.module(name).is_some_and(|m| m.on));
    let now = game.ticks;
    if on && !is_on {
        let Some(i) = with(|s| s.find(name)) else {
            return;
        };
        let soon = bot(|b| b.leased_at.get(name).is_some_and(|&at| now >= at && now < at + 20));
        if soon {
            return;
        }
        bot(|b| {
            if !b.leased.contains(&name) {
                b.leased.push(name);
            }
            b.leased_at.insert(name, now);
        });
        LEASING.store(true, Ordering::Relaxed);
        sunset::toggle(game, i, false);
        LEASING.store(false, Ordering::Relaxed);
    } else if !on && bot(|b| b.leased.contains(&name)) {
        bot(|b| b.leased.retain(|n| *n != name));
        util::toggle_off(game, name);
    }
}

// A module turned on or off by hand (or by itself): no longer the bot's to turn off.
pub(super) fn toggled(name: &str) {
    if !LEASING.load(std::sync::atomic::Ordering::Relaxed) {
        bot(|b| b.leased.retain(|n| *n != name));
    }
}

// The on state to save: a module on only because the bot turned it on is saved as off.
pub(super) fn saved_on(name: &str, on: bool) -> bool {
    on && !bot(|b| b.leased.contains(&name))
}

fn press(game: &mut Game<'_>, want: &[KeyCode]) {
    let before = bot(|b| std::mem::take(&mut b.pressed));
    let mut pressed = HashSet::new();
    for key in KEYS {
        if want.contains(&key) {
            game.keys.insert(key);
            pressed.insert(key);
        } else if before.contains(&key) && !game.physical_keys.contains(&key) {
            game.keys.remove(&key);
        }
    }
    bot(|b| b.pressed = pressed);
}

// Turns the view towards (yaw, pitch) by at most `step` degrees a tick.
fn turn(game: &mut Game<'_>, yaw: f32, pitch: f32, step: f32) {
    let dy = (yaw - game.yaw + 540.0).rem_euclid(360.0) - 180.0;
    game.yaw += dy.clamp(-step, step);
    game.pitch += (pitch - game.pitch).clamp(-step, step);
}

// Ground a step ahead along `dir`: a drop of more than 2 blocks, lava or fire there is no place to walk.
fn safe_ahead(game: &Game<'_>, dir: [f64; 2]) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let (x, z) = ((p.position.x + dir[0]).floor() as i32, (p.position.z + dir[1]).floor() as i32);
    let y = p.position.y.floor() as i32;
    let burns = |y: i32| {
        let name = &game.res.facts().get(game.world.block(x, y, z)).name;
        name.ends_with(":lava") || name.ends_with(":fire")
    };
    if burns(y) || burns(y - 1) {
        return false;
    }
    (1..=3).any(|d| !game.res.facts().is_air(game.world.block(x, y - d, z)))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else {
        if busy() {
            release(game);
        }
        return;
    };
    let alive = game.remote.is_some() && !game.spectator() && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    let playing = game.screen == Screen::Playing && !sunset::gui_open();
    let live = alive && playing;
    // Your own movement keys: the bot lets go and waits.
    let yours = KEYS.iter().any(|k| game.physical_keys.contains(k));
    let me = own_id(game);
    let range = m.n("Fight range");
    let target = game
        .mobs
        .iter()
        .filter(|mob| Some(mob.id) != me && util::player_enemy(game, mob))
        .map(util::mob_target)
        .filter(|t| game.player.as_ref().is_some_and(|p| util::dist([p.position.x, p.position.y, p.position.z], t.pos) <= range))
        .min_by(|a, b| {
            let at = game.player.as_ref().map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
            util::dist(at, a.pos).total_cmp(&util::dist(at, b.pos))
        });
    let crystal = m.p("Mode") == "Crystal";
    // Crystal mode keeps its modules on through a screen (Offhand opens the inventory for a totem), only standing.
    if crystal && let (Some(t), Some(eye), true) = (target.as_ref(), game.eye_position(), alive) {
        crystal_tick(game, m, mods, t, eye, yours, !playing);
        return;
    }
    if crystal || bot(|b| !b.leased.is_empty() || b.fight.is_some()) {
        release(game);
        if crystal {
            return;
        }
    }
    let (Some(t), Some(eye), true, false) = (target, game.eye_position(), live, yours) else {
        release(game);
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (health, ground, falling, in_water) = (
        p.health.health() + p.health.absorption(),
        p.ground.on_ground,
        !p.ground.on_ground && p.velocity.y < 0.0,
        p.fluids.was_touching_water,
    );
    let hurt = bot(|b| std::mem::replace(&mut b.health, health)) > health + 0.01;
    // A golden apple when low; the fight waits while it eats.
    let low = f64::from(health) < m.n("Gapple below HP");
    let eating = bot(|b| b.eating);
    match (eating, low) {
        (Some(_), true) => {
            press(game, &[KeyCode::KeyS]);
            if game.using.is_none() {
                let _ = game.play_mouse(MouseButton::Right, true);
            }
            return;
        }
        (Some(_), false) => release(game),
        (None, true) => {
            if let Some(slot) = util::find_held(game, |n| n.ends_with("golden_apple")) {
                let back = game.selected;
                press(game, &[KeyCode::KeyS]);
                hold(game, slot);
                let _ = game.play_mouse(MouseButton::Right, true);
                bot(|b| b.eating = Some(back));
                return;
            }
        }
        (None, false) => {}
    }
    if let Some(sword) = util::find_held(game, |n| n.ends_with("_sword"))
        && !game.held().is_some_and(|s| s.item.ends_with("_sword") || s.item.ends_with("_axe"))
    {
        hold(game, sword);
    }
    // Aim at the nearest point of their box to your eyes' height, turned to at Turn speed (40 degrees a tick at 1).
    let aim = [(t.lo[0] + t.hi[0]) / 2.0, eye[1].clamp(t.lo[1] + 0.2, t.hi[1] - 0.2), (t.lo[2] + t.hi[2]) / 2.0];
    let (yaw, pitch) = sunset::look_at(eye, aim);
    turn(game, yaw, pitch, 40.0 * m.n("Turn speed") as f32);
    let near = [(t.lo[0] - eye[0]).max(eye[0] - t.hi[0]).max(0.0), (t.lo[1] - eye[1]).max(eye[1] - t.hi[1]).max(0.0), (t.lo[2] - eye[2]).max(eye[2] - t.hi[2]).max(0.0)];
    let reach = (near[0] * near[0] + near[1] * near[1] + near[2] * near[2]).sqrt();
    let off = ((yaw - game.yaw + 540.0).rem_euclid(360.0) - 180.0).abs();
    let ready = game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 1.0;
    // The hit: cooldown full, in reach and in view; with Jump crits, on the way down from a jump (or after 10 ticks
    // ready on the ground, so it never stalls).
    let ready_for = bot(|b| {
        b.ready_for = if ready && reach <= REACH && ground { b.ready_for + 1 } else { 0 };
        b.ready_for
    });
    let crit_ok = !m.b("Jump crits") || falling || in_water || ready_for > 10;
    if ready && reach <= REACH && off < 30.0 && crit_ok && !sunset::strike_waiting() {
        sunset::strike(game, t.id);
    }
    // Moving: in (sprinting) while out of reach; Patient backs off to 2.2 blocks while the cooldown fills, Aggressive
    // stays on them. Strafing turns the other way every 20 to 40 ticks.
    let mut want = Vec::new();
    let dir = [-(game.yaw.to_radians() as f64).sin(), (game.yaw.to_radians() as f64).cos()];
    let gap = util::dist([eye[0], 0.0, eye[2]], [t.pos[0], 0.0, t.pos[2]]);
    if gap > 3.0 || (m.p("Style") == "Aggressive" && gap > 1.5) {
        if safe_ahead(game, dir) {
            want.extend([KeyCode::KeyW, KeyCode::ControlLeft]);
        }
    } else if m.p("Style") == "Patient" && !ready && gap < 2.2 && safe_ahead(game, [-dir[0], -dir[1]]) {
        want.push(KeyCode::KeyS);
    }
    if m.b("Strafe") && gap < 6.0 {
        let now = game.ticks;
        let right = bot(|b| {
            if now >= b.flip_at {
                b.right = !b.right;
                b.flip_at = now + 20 + (now * 7919 % 21);
            }
            b.right
        });
        let side = if right { [-dir[1], dir[0]] } else { [dir[1], -dir[0]] };
        if safe_ahead(game, side) {
            want.push(if right { KeyCode::KeyD } else { KeyCode::KeyA });
        }
    }
    // A jump: for the critical hit as the cooldown comes round in reach, or as a hit lands (Jump reset).
    let soon = game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 0.85;
    if ground && !in_water && ((m.b("Jump crits") && soon && reach <= REACH + 0.5) || (m.b("Jump reset") && hurt)) {
        want.push(KeyCode::Space);
    }
    press(game, &want);
}

fn totems(game: &Game<'_>) -> usize {
    game.inventory.iter().chain(std::iter::once(&game.offhand)).flatten().filter(|s| s.item.ends_with(":totem_of_undying")).count()
}

fn air(game: &Game<'_>, p: [i32; 3]) -> bool {
    game.res.facts().is_air(game.world.block(p[0], p[1], p[2]))
}

// The nearest hole within 3 blocks (at your feet or one down) with room to stand in it.
fn hole_near(game: &Game<'_>, feet: [i32; 3]) -> Option<[i32; 3]> {
    let mut cells = Vec::new();
    for dx in -3..=3 {
        for dz in -3..=3 {
            for dy in -1..=0 {
                cells.push([feet[0] + dx, feet[1] + dy, feet[2] + dz]);
            }
        }
    }
    cells
        .into_iter()
        .filter(|&c| air(game, c) && air(game, [c[0], c[1] + 1, c[2]]) && super::sunset_parity::in_hole(game, c))
        .min_by_key(|c| (c[0] - feet[0]).pow(2) + (c[1] - feet[1]).pow(2) + (c[2] - feet[2]).pow(2))
}

// A look to pearl away along, turned from straight back (`toward` + 180) a little at a time: one whose pearl lands
// where you stand clear (no block in your body, ground within 3 blocks, no lava) at least 6 blocks farther from them.
fn pearl_look(game: &Game<'_>, eye: [f64; 3], toward: f32, t: &util::Target) -> Option<(f32, f32)> {
    let now = util::dist([eye[0], 0.0, eye[2]], [t.pos[0], 0.0, t.pos[2]]);
    let clear = |x: f64, y: i32, z: f64| {
        [(-0.3, -0.3), (-0.3, 0.3), (0.3, -0.3), (0.3, 0.3)]
            .iter()
            .all(|(dx, dz)| air(game, [(x + dx).floor() as i32, y, (z + dz).floor() as i32]))
    };
    for turn in [0.0f32, 25.0, -25.0, 50.0, -50.0, 80.0, -80.0] {
        for pitch in [-30.0f32, -15.0, -45.0, 0.0] {
            let (yr, pr) = ((toward + 180.0 + turn).to_radians() as f64, pitch.to_radians() as f64);
            let vel = [-yr.sin() * pr.cos() * 1.5, -pr.sin() * 1.5, yr.cos() * pr.cos() * 1.5];
            let Some(flight) = super::sunset_throws::pearl_flight(game, [eye[0], eye[1] - 0.1, eye[2]], vel, 100, false) else {
                continue;
            };
            let land = flight.land;
            let feet = (land[1] + 0.01).floor() as i32;
            let cell = [land[0].floor() as i32, feet, land[2].floor() as i32];
            let ground = (1..=3).any(|d| !air(game, [cell[0], feet - d, cell[2]]));
            let lava = (0..=3).any(|d| game.res.facts().get(game.world.block(cell[0], feet - d, cell[2])).name.ends_with(":lava"));
            let far = util::dist([land[0], 0.0, land[2]], [t.pos[0], 0.0, t.pos[2]]) >= now + 6.0;
            if far && ground && !lava && clear(land[0], feet, land[2]) && clear(land[0], feet + 1, land[2]) {
                return Some((toward + 180.0 + turn, pitch));
            }
        }
    }
    None
}

fn target_feet(t: &util::Target) -> [i32; 3] {
    [t.pos[0].floor() as i32, (t.lo[1] + 0.1).floor() as i32, t.pos[2].floor() as i32]
}

// A fight with `id` from now: the one before ends; their history is read in the background the first time.
fn begin_fight(game: &mut Game<'_>, id: u32, learn: bool) {
    if bot(|b| b.fight.as_ref().is_some_and(|f| f.id == id)) {
        return;
    }
    end_fight(game);
    let uuid = super::remote::sunset_combat_remote::player_uuid(game, id);
    let own = own_id(game).map_or(0, super::sunset_effects::pops);
    let start = (super::sunset_effects::pops(id), own);
    let fight = Fight { id, uuid, start, seen: start, ticks: 0, hole_ticks: 0 };
    bot(|b| b.fight = Some(fight));
    let Some(uuid) = uuid.filter(|_| learn) else {
        return;
    };
    if rivals(|r| r.contains_key(&uuid)) {
        return;
    }
    rivals(|r| {
        r.insert(uuid, None);
    });
    let data = game.native_data_dir(sunset::ID);
    std::thread::spawn(move || {
        let read = super::sunset_rival_store::read(&data, uuid).ok().and_then(|text| Rival::parse(&text)).unwrap_or_default();
        rivals(|r| {
            r.insert(uuid, Some(read));
        });
    });
}

// The fight ends: with Learn rivals, their history gets what it saw and is saved in the background.
fn end_fight(game: &mut Game<'_>) {
    let Some(f) = bot(|b| b.fight.take()) else {
        return;
    };
    let learn = with(|s| s.module(NAME).is_some_and(|m| m.b("Learn rivals")));
    let Some(uuid) = f.uuid.filter(|_| learn && f.ticks > 0) else {
        return;
    };
    let pops = super::sunset_effects::pops(f.id).max(f.seen.0);
    let own = own_id(game).map_or(0, super::sunset_effects::pops).max(f.seen.1);
    let Some(next) = rivals(|r| {
        let rival = r.get_mut(&uuid)?.as_mut()?;
        rival.fights += 1;
        rival.ticks += f.ticks;
        rival.hole_ticks += f.hole_ticks;
        rival.pops += pops.saturating_sub(f.start.0);
        rival.taken += own.saturating_sub(f.start.1);
        Some(*rival)
    }) else {
        return;
    };
    let data = game.native_data_dir(sunset::ID);
    std::thread::spawn(move || {
        let _ = super::sunset_rival_store::save(&data, &[(uuid, next.text())]);
    });
}

// What the bot knows of a rival (None: never fought, or still being read).
pub(super) fn rival(uuid: u128) -> Option<Rival> {
    rivals(|r| r.get(&uuid).copied().flatten())
}

fn crystal_tick(game: &mut Game<'_>, m: &Module, mods: &[Module], t: &util::Target, eye: [f64; 3], yours: bool, paused: bool) {
    let learn = m.b("Learn rivals");
    begin_fight(game, t.id, learn);
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let health = f64::from(p.health.health() + p.health.absorption());
    let ground = p.ground.on_ground;
    let Some(feet) = util::block_pos(game) else {
        return;
    };
    let their_feet = target_feet(t);
    let their_hole = super::sunset_parity::in_hole(game, their_feet);
    let history = bot(|b| b.fight.as_ref().and_then(|f| f.uuid)).filter(|_| learn).and_then(rival);
    // Each tick: the totems popped so far (they go when the rival leaves), time in a hole.
    let (pops, own) = (super::sunset_effects::pops(t.id), own_id(game).map_or(0, super::sunset_effects::pops));
    bot(|b| {
        if let Some(f) = b.fight.as_mut() {
            f.ticks += 1;
            f.hole_ticks += u32::from(their_hole);
            f.seen = (f.seen.0.max(pops), f.seen.1.max(own));
        }
    });
    if paused {
        press(game, &[]);
        return;
    }
    let gap = util::dist([eye[0], 0.0, eye[2]], [t.pos[0], 0.0, t.pos[2]]);
    let (yaw, pitch) = sunset::look_at(eye, [t.pos[0], (t.lo[1] + t.hi[1]) / 2.0, t.pos[2]]);
    // Low with no totem left: one pearl away from them (Key Throw throws it along the look; another 10 s on at the
    // soonest), then keep back and eat.
    let below = m.n("Pearl below HP") + if history.is_some_and(|r| r.beats_you()) { 4.0 } else { 0.0 };
    let escaping = m.n("Pearl below HP") > 0.0 && health < below && totems(game) == 0;
    let now = game.ticks;
    if escaping {
        let pearl = util::find_held(game, |n| n == "ender_pearl").is_some() && !game.cooldowns.is_on_cooldown("minecraft:ender_pearl");
        let waited = bot(|b| b.pearl_at == 0 || now >= b.pearl_at + 200 || now < b.pearl_at);
        if pearl
            && waited
            && let Some(look) = pearl_look(game, eye, yaw, t)
        {
            lease(game, "Crystal Aura", false);
            lease(game, "Anchor Aura", false);
            lease(game, "Key Throw", true);
            (game.yaw, game.pitch) = look;
            let ready = mods.iter().any(|m| m.name == "Key Throw" && m.on) && super::sunset_combat_control::owner().is_none();
            if ready {
                super::sunset_combat_inventory::throw_pearl(game);
                bot(|b| b.pearl_at = now);
            }
            press(game, &[]);
            return;
        }
    }
    // The pearl's look holds for 5 ticks while Key Throw turns to it.
    let throwing = bot(|b| b.pearl_at > 0 && now >= b.pearl_at && now < b.pearl_at + 5);
    lease(game, "Key Throw", throwing);
    lease(game, "Crystal Aura", !escaping);
    lease(game, "Anchor Aura", !escaping && m.b("Use anchors"));
    lease(game, "Offhand", true);
    lease(game, "Auto Gap", true);
    lease(game, "Auto City", m.b("City") && gap <= 5.5 && (their_hole || history.is_some_and(|r| r.holes_up())));
    let in_hole = super::sunset_parity::in_hole(game, feet);
    let range = m.n("Crystal range");
    // Where to go: back from them while escaping, in while out of crystal range, into a hole nearby once in it.
    let hole = if m.b("Hole up") && !in_hole && gap <= range + 1.5 && !escaping { hole_near(game, feet) } else { None };
    let mut want = Vec::new();
    let mut facing = (yaw, pitch);
    let mut standing = false;
    if let Some(h) = hole.filter(|_| ground) {
        facing = (sunset::look_at(eye, [f64::from(h[0]) + 0.5, eye[1], f64::from(h[2]) + 0.5]).0, game.pitch);
        let dir = [-(facing.0.to_radians() as f64).sin(), (facing.0.to_radians() as f64).cos()];
        let close = (game.yaw - facing.0 + 540.0).rem_euclid(360.0) - 180.0;
        if close.abs() < 20.0 && safe_ahead(game, dir) {
            want.push(KeyCode::KeyW);
        }
    } else if escaping {
        if safe_ahead(game, [(yaw.to_radians() as f64).sin(), -(yaw.to_radians() as f64).cos()]) {
            want.push(KeyCode::KeyS);
        }
    } else if gap > range {
        let dir = [-(game.yaw.to_radians() as f64).sin(), (game.yaw.to_radians() as f64).cos()];
        if safe_ahead(game, dir) {
            want.extend([KeyCode::KeyW, KeyCode::ControlLeft]);
        }
    } else {
        standing = ground;
    }
    // Standing to fight: Surround where it stands (or the hole's open sides), Self Trap when low.
    lease(game, "Surround", m.b("Hole up") && standing && !yours);
    lease(game, "Self Trap", standing && !yours && health < m.n("Self trap below HP"));
    if yours {
        press(game, &[]);
        return;
    }
    if !throwing {
        turn(game, facing.0, facing.1, 40.0 * m.n("Turn speed") as f32);
    }
    press(game, &want);
}
