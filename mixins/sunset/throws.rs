// Pearl and wind charge throws: Key Throw's Short pearl key, Legit throws, Wind charge after pearl, Auto short pearl
// and Pearl follow; Pearl Catch's Catch key (Up, Fast, Offhand, Diagonal, Diagonal macro); Wind Jump's Elytra and Hit
// charges. A throw goes at the start of a tick, before its move packet, once the server has its look from the last
// one (asked a tick ahead); a hotbar item is held for it and the slot from before comes back at the start of a later
// tick, never after a use in the same one; one slot change a tick.
use super::sunset::{Module, hold, own_id, turn, with};
use super::sunset_rotation::Rng;
use super::sunset_util::{self as util, OFFHAND};
use super::{Game, Screen};
use horizon_client::projectiles::ProjectileKind;
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::sync::Mutex;

const THROW: &str = "Key Throw";
const CATCH: &str = "Pearl Catch";
const WIND: &str = "Wind Jump";

// A pearl leaves at 1.5 blocks a tick, slowed by 0.99 and pulled down 0.03 a tick; a wind charge flies at 1.5.
const THROW_SPEED: f64 = 1.5;
const PEARL_DRAG: f64 = 0.99;
const PEARL_GRAVITY: f64 = 0.03;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    Pearl,
    Wind,
}

impl Item {
    fn id(self) -> &'static str {
        match self {
            Self::Pearl => "ender_pearl",
            Self::Wind => "wind_charge",
        }
    }
}

// The look a throw goes with: a set one, or your yaw at a pitch.
#[derive(Clone, Copy)]
enum Aim {
    At(f32, f32),
    Pitch(f32),
}

impl Aim {
    fn look(self, game: &Game<'_>) -> (f32, f32) {
        match self {
            Self::At(yaw, pitch) => (yaw, pitch.clamp(-90.0, 90.0)),
            Self::Pitch(pitch) => (game.yaw, pitch.clamp(-90.0, 90.0)),
        }
    }
}

// One throw waiting: who asked, what, the look, not before tick `at` and not after `until`; a jump with it from the
// ground; and whether it is a pearl Wind charge after pearl follows.
#[derive(Clone, Copy)]
struct Single {
    owner: &'static str,
    item: Item,
    aim: Aim,
    at: u64,
    until: u64,
    jump: bool,
    then_wind: bool,
}

// Key Throw's Legit throws, a step a tick at most and a rolled delay between: the item's slot held, the camera turned
// to the pitch (if any), the throw with your own look once the server has it, the camera back, the slot back.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Select,
    Turn,
    Throw,
    TurnBack,
    Back,
}

#[derive(Clone, Copy)]
struct Legit {
    item: Item,
    pitch: Option<f32>,
    jump: bool,
    stage: Stage,
    next: u64,
    back: usize,
    from: f32,
    until: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Up,
    Fast,
    Offhand,
    Diagonal,
    Macro,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Prep,
    Pearl,
    Wind,
    Restore,
}

// Pearl Catch's Catch key: the pearl and the wind charge after it. `wind_at` is the tick the charge goes; `thrown` the
// pearl's; `plan` Diagonal's charge look; `offhand_from` the hotbar slot Offhand swapped its charge from.
#[derive(Clone, Copy)]
struct Catch {
    mode: Mode,
    phase: Phase,
    yaw: f32,
    wind_at: u64,
    thrown: u64,
    plan: Option<(f32, f32)>,
    offhand_from: Option<usize>,
    until: u64,
}

// A pearl or wind charge in flight as first seen: where, when, whether near your eyes, and its owner's entity id when
// the server named one.
#[derive(Clone, Copy)]
struct Seen {
    id: u32,
    item: Item,
    first: [f64; 3],
    tick: u64,
    near_me: bool,
}

#[derive(Default)]
struct Throws {
    rng: Option<Rng>,
    single: Option<Single>,
    legit: Option<Legit>,
    catch: Option<Catch>,
    // The slot held before the first borrowed one, the tick of the last use, and the combat lease held.
    back: Option<usize>,
    used: u64,
    lease: Option<&'static str>,
    // A jump in this tick's input.
    jump: bool,
    // Wind charge after pearl: armed for a pearl Key Throw threw, until this tick.
    after_pearl: Option<u64>,
    seen: Vec<Seen>,
    owners: Vec<(u32, u32)>,
    last_pearl: u64,
    last_wind: u64,
    // Pearl follow: the pearls looked at, and where to follow one to from which tick.
    followed: Vec<u32>,
    follow: Option<([f64; 3], u64)>,
    // Wind Jump: the last charge hit, the last Elytra charge, the last glide try.
    hit_at: u64,
    elytra_at: u64,
    glide_at: u64,
    keys_down: Vec<String>,
}

static THROWS: Mutex<Option<Throws>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut Throws) -> R) -> R {
    f(THROWS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Throws::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

fn roll(game: &Game<'_>, lo: f64, hi: f64) -> f64 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let r = st(|s| s.rng.get_or_insert_with(|| Rng::new(game.ticks ^ 0x5DEE_CE66_D1CE_4E5B)).next());
    lo + r * (hi - lo)
}

// Milliseconds as ticks.
fn ticks_of(ms: f64) -> u64 {
    (ms / 50.0).ceil().max(0.0) as u64
}

fn rolled_ticks(game: &Game<'_>, m: &Module, shortest: &str, longest: &str) -> u64 {
    ticks_of(roll(game, m.n(shortest), m.n(longest)))
}

fn wrap(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn same_look(a: (f32, f32), b: (f32, f32)) -> bool {
    wrap(a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn look_to(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let flat = d[0].hypot(d[2]);
    ((-d[0]).atan2(d[2]).to_degrees() as f32, (-d[1].atan2(flat).to_degrees()) as f32)
}

fn live(game: &Game<'_>) -> bool {
    game.remote.is_some()
        && util::in_world(game)
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
}

fn playing(game: &Game<'_>) -> bool {
    game.screen == Screen::Playing && !super::sunset::gui_open()
}

// Your movement along your yaw (blocks a tick).
fn forward_speed(game: &Game<'_>, yaw: f32) -> f32 {
    let Some(p) = game.player.as_ref() else {
        return 0.0;
    };
    let r = f64::from(yaw).to_radians();
    (p.velocity.x * -r.sin() + p.velocity.z * r.cos()) as f32
}

// ---- items

fn usable(game: &Game<'_>, slot: usize, item: Item) -> bool {
    game.stack_in(slot).is_some_and(|s| {
        util::short(&s.item) == item.id()
            && s.count > 0
            && !game.res.items().get(&s.item).is_some_and(|i| {
                game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(s, i.use_cooldown.as_ref()))
            })
    })
}

fn has(game: &Game<'_>, slot: usize, item: Item) -> bool {
    game.stack_in(slot).is_some_and(|s| util::short(&s.item) == item.id() && s.count > 0)
}

// Where a throw of `item` comes from: the offhand if it holds one, else the held slot, else the hotbar.
fn slot_of(game: &Game<'_>, item: Item) -> Option<usize> {
    std::iter::once(OFFHAND).chain(std::iter::once(game.selected)).chain(0..9).find(|&s| has(game, s, item))
}

// ---- the throw

// Whether the server has `look` from the last move packet; it is asked for either way (this tick's move packet).
fn have_look(game: &Game<'_>, owner: &'static str, look: (f32, f32)) -> bool {
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    turn(owner, look.0, look.1);
    same_look(have, look)
}

// A throw of `item` from `slot` now: Some(true) thrown, Some(false) waiting (for the look or the hotbar), None it
// can't (gone, or on cooldown).
fn throw_from(game: &mut Game<'_>, owner: &'static str, slot: usize, item: Item, look: (f32, f32)) -> Option<bool> {
    if !usable(game, slot, item) {
        return None;
    }
    let held = st(|s| s.lease);
    if held != Some(owner) {
        if !super::sunset_combat_control::claim(owner) {
            return Some(false);
        }
        if let Some(old) = held {
            super::sunset_combat_control::release(old);
        }
        st(|s| s.lease = Some(owner));
    }
    if !have_look(game, owner, look) {
        return Some(false);
    }
    if slot != OFFHAND && game.selected != slot {
        let before = game.selected;
        st(|s| {
            s.back.get_or_insert(before);
        });
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    let yaw = super::sunset::wire_yaw(game, look.0);
    game.send(S::UseItem(play::UseItem { hand: i32::from(slot == OFFHAND), sequence, yaw, pitch: look.1 }));
    game.swing_arm();
    game.use_delay = game.use_delay.max(2);
    game.clicks.right_click_delay = game.clicks.right_click_delay.max(2);
    let now = game.ticks;
    st(|s| {
        s.used = now;
        match item {
            Item::Pearl => s.last_pearl = now,
            Item::Wind => s.last_wind = now,
        }
    });
    Some(true)
}

fn throw(game: &mut Game<'_>, owner: &'static str, item: Item, look: (f32, f32)) -> Option<bool> {
    let slot = slot_of(game, item)?;
    throw_from(game, owner, slot, item, look)
}

// The slot from before comes back (a tick after the last use), and the lease goes, once nothing waits. Whether the
// slot changed.
fn settle(game: &mut Game<'_>) -> bool {
    let (idle, back, used, lease) = st(|s| (s.single.is_none() && s.legit.is_none() && s.catch.is_none(), s.back, s.used, s.lease));
    if !idle || used >= game.ticks {
        return false;
    }
    st(|s| s.back = None);
    if let Some(owner) = lease {
        super::sunset_combat_control::release(owner);
        st(|s| s.lease = None);
    }
    match back {
        Some(b) if b < 9 && game.selected != b => {
            hold(game, b);
            true
        }
        _ => false,
    }
}

pub(super) fn reset(game: &mut Game<'_>) {
    let (back, lease, legit) = st(|s| (s.back.take(), s.lease.take(), s.legit.take()));
    if let Some(b) = back.filter(|&b| b < 9 && b != game.selected && game.player.is_some()) {
        hold(game, b);
    }
    if let Some(l) = legit.filter(|l| l.back != game.selected && l.back < 9 && l.stage != Stage::Select) {
        hold(game, l.back);
    }
    if let Some(owner) = lease {
        super::sunset_combat_control::release(owner);
    }
    for owner in [THROW, CATCH, WIND] {
        super::sunset::cancel_turns(owner);
    }
    st(|s| {
        s.single = None;
        s.catch = None;
        s.after_pearl = None;
        s.follow = None;
        s.jump = false;
    });
}

// ---- pearls and charges in flight

fn tracked(kind: &ProjectileKind) -> Option<Item> {
    match kind {
        ProjectileKind::EnderPearl => Some(Item::Pearl),
        ProjectileKind::WindCharge => Some(Item::Wind),
        _ => None,
    }
}

fn track(game: &Game<'_>) {
    let eye = game.eye_position();
    let now = game.ticks;
    let flying: Vec<(u32, Item, [f64; 3])> = game
        .creatures
        .projectiles
        .iter()
        .filter(|p| p.id != 0 && !p.stuck)
        .filter_map(|p| tracked(&p.kind).map(|item| (p.id, item, [p.position.x, p.position.y, p.position.z])))
        .collect();
    st(|s| {
        s.seen.retain(|x| flying.iter().any(|(id, ..)| *id == x.id));
        s.followed.retain(|id| flying.iter().any(|(i, ..)| i == id));
        for &(id, item, at) in &flying {
            if !s.seen.iter().any(|x| x.id == id) {
                let near_me = eye.is_some_and(|e| util::dist(e, at) <= 3.0);
                s.seen.push(Seen { id, item, first: at, tick: now, near_me });
            }
        }
    });
}

fn owner_of(id: u32) -> Option<u32> {
    st(|s| s.owners.iter().rev().find(|(i, _)| *i == id).map(|(_, o)| *o)).filter(|&o| o != 0)
}

// Yours: the server named you its owner, or (no owner named) it showed up by your eyes within a few ticks of your
// own throw of one.
fn own(game: &Game<'_>, s: &Seen) -> bool {
    match owner_of(s.id) {
        Some(o) => Some(o) == own_id(game),
        None => {
            let last = st(|t| if s.item == Item::Pearl { t.last_pearl } else { t.last_wind });
            s.near_me && last > 0 && s.tick + 1 >= last && s.tick <= last + 8
        }
    }
}

// Who threw it: the owner the server named, or the nearest player within 3 blocks of where it first showed up (not by
// your eyes).
fn thrower<'a>(game: &'a Game<'_>, s: &Seen) -> Option<&'a horizon_client::mobs::Mob> {
    let me = own_id(game);
    match owner_of(s.id) {
        Some(o) => game.mobs.iter().find(|m| m.id == o && Some(m.id) != me),
        None if !s.near_me => game
            .mobs
            .iter()
            .filter(|m| m.kind.id == "minecraft:player" && Some(m.id) != me && m.alive() && !m.removed)
            .map(|m| (util::dist([m.position.x, m.position.y + 1.5, m.position.z], s.first), m))
            .filter(|(d, _)| *d <= 3.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, m)| m),
        None => None,
    }
}

// A pearl or charge seen for at least two ticks: its speed is the server's by then (the first can be off).
pub(super) fn settled(game: &Game<'_>, id: u32) -> bool {
    st(|s| s.seen.iter().any(|x| x.id == id && x.tick + 2 <= game.ticks))
}

// The entity id of an enemy player who threw the pearl or charge `id` (Pearl follow, Anchor Aura's Pearl catch).
pub(super) fn enemy_thrower(game: &Game<'_>, id: u32) -> Option<u32> {
    let s = st(|t| t.seen.iter().find(|x| x.id == id).copied())?;
    if own(game, &s) {
        return None;
    }
    thrower(game, &s).filter(|by| util::player_enemy(game, by)).map(|by| by.id)
}

fn own_pearl_since(game: &Game<'_>, tick: u64) -> bool {
    let seen = st(|s| s.seen.clone());
    seen.iter().any(|s| s.item == Item::Pearl && s.tick + 1 >= tick && own(game, s))
}

// A pearl's flight from `pos` at `vel` until it meets a block: where, after how many ticks, its highest point, and
// (with `keep`) the points it passes.
pub(super) struct Flight {
    pub(super) land: [f64; 3],
    pub(super) ticks: u32,
    pub(super) apex: f64,
    pub(super) path: Vec<[f64; 3]>,
}

pub(super) fn pearl_flight(game: &Game<'_>, mut pos: [f64; 3], mut vel: [f64; 3], max: u32, keep: bool) -> Option<Flight> {
    let mut apex = pos[1];
    let mut path = if keep { vec![pos] } else { Vec::new() };
    for tick in 1..=max {
        let next = add(pos, vel);
        let d = util::dist(pos, next);
        if d > 1e-9
            && let Some(hit) = game.clip_ray(pos, scale([next[0] - pos[0], next[1] - pos[1], next[2] - pos[2]], 1.0 / d), d)
        {
            if keep {
                path.push(hit.point);
            }
            return Some(Flight { land: hit.point, ticks: tick, apex, path });
        }
        pos = next;
        apex = apex.max(pos[1]);
        if keep {
            path.push(pos);
        }
        vel = [vel[0] * PEARL_DRAG, vel[1] * PEARL_DRAG - PEARL_GRAVITY, vel[2] * PEARL_DRAG];
        if pos[1] < -128.0 {
            break;
        }
    }
    None
}

// A flying pearl's landing (Cart's Pearls, Pearl follow, Trajectories).
pub(super) fn pearl_landing(game: &Game<'_>, id: u32, max: u32, keep: bool) -> Option<Flight> {
    let p = game.creatures.projectiles.iter().find(|p| p.id == id && !p.stuck)?;
    pearl_flight(game, [p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.y, p.velocity.z], max, keep)
}

// ---- hooks

// The packets of the last tick: owners of pearls and charges, a player's hit on you (Auto short pearl), your teleport
// (Wind charge after pearl).
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    match packet {
        C::AddEntity(p) => {
            let kind = game.remote.as_ref().and_then(|r| r.builtin.get("minecraft:entity_type")).and_then(|t| t.name(p.kind));
            if matches!(kind, Some("minecraft:ender_pearl" | "minecraft:wind_charge"))
                && let (Ok(id), Ok(owner)) = (u32::try_from(p.id), u32::try_from(p.data))
            {
                st(|s| {
                    s.owners.push((id, owner));
                    if s.owners.len() > 256 {
                        s.owners.remove(0);
                    }
                });
            }
        }
        C::DamageEvent(p) if game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) => {
            let player_hit = game
                .remote
                .as_ref()
                .and_then(|r| r.registries.name("minecraft:damage_type", p.source_type))
                == Some("minecraft:player_attack");
            if let Some(m) = module(THROW).filter(|m| player_hit && m.b("Auto short pearl"))
                && st(|s| s.single.is_none())
            {
                let at = game.ticks + rolled_ticks(game, &m, "Shortest hit delay (ms)", "Longest hit delay (ms)");
                let pitch = 89.0 + roll(game, 0.0, 1.0) as f32;
                st(|s| s.single = Some(Single { owner: THROW, item: Item::Pearl, aim: Aim::Pitch(pitch), at, until: at + 10, jump: false, then_wind: false }));
            }
        }
        C::PlayerPosition(_) => {
            if st(|s| s.after_pearl.take()).is_some_and(|until| game.ticks <= until) && st(|s| s.single.is_none()) {
                let at = game.ticks;
                st(|s| s.single = Some(Single { owner: THROW, item: Item::Wind, aim: Aim::Pitch(90.0), at, until: at + 10, jump: true, then_wind: false }));
            }
        }
        _ => {}
    }
}

// Key Throw threw a pearl: Wind charge after pearl waits for the teleport.
pub(super) fn pearl_thrown(game: &Game<'_>) {
    let now = game.ticks;
    st(|s| s.last_pearl = now);
    if module(THROW).is_some_and(|m| m.b("Wind charge after pearl")) {
        st(|s| s.after_pearl = Some(now + 200));
    }
}

// Key Throw's Short pearl key and Pearl Catch's Catch key: a press starts its throw.
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    let fresh = st(|s| {
        let was = s.keys_down.iter().any(|k| k == key);
        if pressed && !was {
            s.keys_down.push(key.to_owned());
        } else if !pressed {
            s.keys_down.retain(|k| k != key);
        }
        pressed && !was
    });
    if !fresh || !live(game) || !playing(game) {
        return;
    }
    let named = |m: &Module, setting: &str| super::sunset::key_from_text(m.t(setting)).as_deref() == Some(key);
    if let Some(m) = module(THROW).filter(|m| named(m, "Short pearl key")) {
        let pitch = m.n("Short pearl pitch") as f32;
        if m.p("Throw mode") == "Legit" {
            legit_start(game, &m, Item::Pearl, Some(pitch), false);
        } else if st(|s| s.single.is_none()) {
            let pitch = pitch - 1.0 + roll(game, 0.0, 2.0) as f32;
            let then_wind = m.b("Wind charge after pearl");
            let at = game.ticks;
            st(|s| s.single = Some(Single { owner: THROW, item: Item::Pearl, aim: Aim::Pitch(pitch), at, until: at + 10, jump: false, then_wind }));
        }
    }
    if let Some(m) = module(CATCH).filter(|m| named(m, "Catch key")) {
        catch_start(game, &m);
    }
}

// Key Throw's Pearl and Wind charge keys in Legit mode.
pub(super) fn legit_press(game: &mut Game<'_>, m: &Module, item: Item) {
    if !live(game) || !playing(game) {
        return;
    }
    match item {
        Item::Pearl => legit_start(game, m, Item::Pearl, None, false),
        Item::Wind if m.b("Wind charge along look") => legit_start(game, m, Item::Wind, None, false),
        Item::Wind => {
            let pitch = m.n("Wind charge pitch") as f32 - 1.0 + roll(game, 0.0, 2.0) as f32;
            legit_start(game, m, Item::Wind, Some(pitch.min(90.0)), true);
        }
    }
}

fn legit_start(game: &mut Game<'_>, m: &Module, item: Item, pitch: Option<f32>, jump: bool) {
    if st(|s| s.legit.is_some()) || !(0..9).chain(std::iter::once(OFFHAND)).any(|s| has(game, s, item)) {
        return;
    }
    let next = game.ticks + rolled_ticks(game, m, "Shortest step delay (ms)", "Longest step delay (ms)");
    let legit = Legit { item, pitch, jump, stage: Stage::Select, next, back: game.selected, from: game.pitch, until: game.ticks + 100 };
    st(|s| s.legit = Some(legit));
}

// ---- the tick

// The start of a tick, before its move packet. `busy`: a hit or a slot change went already this tick.
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    let (throw_m, catch_m, wind_m) = (module(THROW), module(CATCH), module(WIND));
    if live(game) {
        track(game);
    }
    if !live(game) || (throw_m.is_none() && catch_m.is_none() && wind_m.is_none()) {
        if st(|s| s.back.is_some() || s.lease.is_some() || s.legit.is_some() || s.catch.is_some() || s.single.is_some()) {
            reset(game);
        }
        return;
    }
    if busy || !playing(game) {
        return;
    }
    if settle(game) {
        return;
    }
    if let Some(m) = &throw_m
        && st(|s| s.legit.is_some())
    {
        legit_step(game, m);
        return;
    }
    if let Some(m) = &catch_m
        && st(|s| s.catch.is_some())
    {
        catch_step(game, m);
        return;
    }
    if st(|s| s.single.is_some()) {
        single_step(game);
        return;
    }
    if let Some(m) = throw_m.as_ref().filter(|m| m.b("Pearl follow")) {
        follow(game, m);
    }
    if let Some(m) = &wind_m {
        if m.b("Elytra") {
            elytra(game);
        }
        if m.b("Hit charges") && st(|s| s.single.is_none()) {
            hit_charges(game, m);
        }
    }
}

// The jump a throw asked for, in this tick's input.
pub(super) fn pre_input(game: &mut Game<'_>) {
    if st(|s| std::mem::take(&mut s.jump)) {
        super::sunset_melee::press_jump(game);
    }
}

fn single_step(game: &mut Game<'_>) {
    let Some(one) = st(|s| s.single) else {
        return;
    };
    let now = game.ticks;
    if now > one.until || module(one.owner).is_none() {
        st(|s| s.single = None);
        return;
    }
    let look = one.aim.look(game);
    if now + 1 < one.at {
        return;
    }
    if now < one.at {
        turn(one.owner, look.0, look.1);
        return;
    }
    match throw(game, one.owner, one.item, look) {
        Some(true) => {
            let on_ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            st(|s| {
                s.single = None;
                s.jump = one.jump && on_ground;
            });
            if one.then_wind {
                pearl_thrown(game);
            }
        }
        Some(false) => {}
        None => st(|s| s.single = None),
    }
}

fn legit_step(game: &mut Game<'_>, m: &Module) {
    let Some(mut l) = st(|s| s.legit) else {
        return;
    };
    let now = game.ticks;
    if now > l.until {
        st(|s| s.legit = None);
        return;
    }
    if now < l.next {
        return;
    }
    let share = ((m.n("Smoothing") / 2.0) as f32).clamp(0.05, 1.0);
    let toward = |game: &mut Game<'_>, to: f32| -> bool {
        let left = to - game.pitch;
        if left.abs() <= 0.5 {
            game.pitch = to;
            return true;
        }
        game.pitch += left * share;
        false
    };
    let done = match l.stage {
        Stage::Select => {
            let Some(slot) = (0..9).find(|&s| has(game, s, l.item)).or_else(|| has(game, OFFHAND, l.item).then_some(OFFHAND)) else {
                st(|s| s.legit = None);
                return;
            };
            if slot != OFFHAND && game.selected != slot && !has(game, game.selected, l.item) {
                hold(game, slot);
            }
            l.stage = if l.pitch.is_some() { Stage::Turn } else { Stage::Throw };
            true
        }
        Stage::Turn => {
            if toward(game, l.pitch.unwrap_or(game.pitch)) {
                l.stage = Stage::Throw;
                true
            } else {
                false
            }
        }
        Stage::Throw => {
            let own = (game.yaw, game.pitch);
            let have = super::sunset_combat_wire::sent_look(game).unwrap_or(own);
            let slot = if has(game, game.selected, l.item) { Some(game.selected) } else { has(game, OFFHAND, l.item).then_some(OFFHAND) };
            match slot {
                None => {
                    l.stage = Stage::TurnBack;
                    true
                }
                Some(_) if !same_look(have, own) => false,
                Some(slot) => {
                    if !usable(game, slot, l.item) {
                        false
                    } else {
                        let sequence = util::sequence(game);
                        game.send(S::UseItem(play::UseItem { hand: i32::from(slot == OFFHAND), sequence, yaw: own.0, pitch: own.1 }));
                        game.swing_arm();
                        game.use_delay = game.use_delay.max(4);
                        st(|s| {
                            s.used = now;
                            s.jump = l.jump && game.player.as_ref().is_some_and(|p| p.ground.on_ground);
                            match l.item {
                                Item::Pearl => s.last_pearl = now,
                                Item::Wind => s.last_wind = now,
                            }
                        });
                        if l.item == Item::Pearl {
                            pearl_thrown(game);
                        }
                        l.stage = if l.pitch.is_some() { Stage::TurnBack } else { Stage::Back };
                        true
                    }
                }
            }
        }
        Stage::TurnBack => {
            if toward(game, l.from) {
                l.stage = Stage::Back;
                true
            } else {
                false
            }
        }
        Stage::Back => {
            if l.back < 9 && game.selected != l.back {
                hold(game, l.back);
            }
            st(|s| s.legit = None);
            return;
        }
    };
    if done {
        l.next = now + rolled_ticks(game, m, "Shortest step delay (ms)", "Longest step delay (ms)").max(1);
    }
    st(|s| s.legit = Some(l));
}

// ---- Pearl Catch's Catch key

fn mode_of(m: &Module) -> Mode {
    match m.p("Key mode") {
        "Fast" => Mode::Fast,
        "Offhand" => Mode::Offhand,
        "Diagonal" => Mode::Diagonal,
        "Diagonal macro" => Mode::Macro,
        _ => Mode::Up,
    }
}

fn catch_start(game: &mut Game<'_>, m: &Module) {
    if st(|s| s.catch.is_some()) || game.using.is_some() {
        return;
    }
    let mode = mode_of(m);
    let pearl = (0..9).any(|s| has(game, s, Item::Pearl));
    let wind = (0..9).chain(std::iter::once(OFFHAND)).any(|s| has(game, s, Item::Wind));
    if !pearl || !wind {
        return;
    }
    let phase = if mode == Mode::Offhand && !has(game, OFFHAND, Item::Wind) { Phase::Prep } else { Phase::Pearl };
    let back = game.selected;
    st(|s| {
        s.back.get_or_insert(back);
        s.catch = Some(Catch { mode, phase, yaw: game.yaw, wind_at: 0, thrown: 0, plan: None, offhand_from: None, until: game.ticks + 60 });
    });
}

fn pearl_look(game: &Game<'_>, m: &Module, c: &Catch) -> (f32, f32) {
    match c.mode {
        Mode::Up => (c.yaw, -90.0),
        Mode::Fast => (c.yaw, -(m.n("Pearl angle") as f32)),
        Mode::Offhand => (c.yaw, -(m.n("Offhand pearl angle") as f32)),
        Mode::Diagonal | Mode::Macro => (game.yaw, game.pitch),
    }
}

fn wind_look(game: &Game<'_>, m: &Module, c: &Catch) -> (f32, f32) {
    let lean = forward_speed(game, c.yaw) * 1.6;
    match c.mode {
        Mode::Up => (c.yaw, -90.0 + lean),
        Mode::Fast => (c.yaw, -(m.n("Wind charge angle") as f32) + lean),
        Mode::Offhand => (c.yaw, -(m.n("Offhand wind charge angle") as f32) + lean),
        Mode::Macro => (game.yaw, game.pitch),
        Mode::Diagonal => match c.plan {
            Some(plan) if f64::from(wrap(plan.0 - game.yaw).abs()) <= m.n("Max yaw delta") => plan,
            _ => (game.yaw, game.pitch),
        },
    }
}

fn catch_end(m: &Module) {
    let swap_back = m.b("Swap back");
    st(|s| {
        s.catch = None;
        if !swap_back {
            s.back = None;
        }
    });
}

fn catch_step(game: &mut Game<'_>, m: &Module) {
    let Some(mut c) = st(|s| s.catch) else {
        return;
    };
    let now = game.ticks;
    if now > c.until || game.using.is_some() {
        if c.offhand_from.is_some() {
            c.phase = Phase::Restore;
        } else {
            catch_end(m);
            return;
        }
    }
    match c.phase {
        Phase::Prep => {
            let Some(slot) = (0..9).find(|&s| has(game, s, Item::Wind)) else {
                catch_end(m);
                return;
            };
            if game.selected != slot {
                hold(game, slot);
            }
            game.remote_swap_hands();
            c.offhand_from = Some(slot);
            c.phase = Phase::Pearl;
        }
        Phase::Pearl => {
            let look = pearl_look(game, m, &c);
            let Some(slot) = (0..9).find(|&s| has(game, s, Item::Pearl)) else {
                if c.offhand_from.is_some() {
                    c.phase = Phase::Restore;
                    st(|s| s.catch = Some(c));
                } else {
                    catch_end(m);
                }
                return;
            };
            if c.mode == Mode::Diagonal && c.plan.is_none() {
                match diagonal_plan(game, look) {
                    Some((wait, plan)) => {
                        c.plan = Some(plan);
                        c.wind_at = u64::from(wait);
                    }
                    None => {
                        catch_end(m);
                        return;
                    }
                }
            }
            match throw_from(game, CATCH, slot, Item::Pearl, look) {
                Some(true) => {
                    c.thrown = now;
                    c.wind_at = match c.mode {
                        Mode::Up | Mode::Macro => {
                            let moving = game.player.as_ref().is_some_and(|p| p.velocity.x != 0.0 || p.velocity.z != 0.0);
                            now + ticks_of(m.n(if moving && c.mode == Mode::Up { "Moving delay (ms)" } else { "Wind charge delay (ms)" })).max(1)
                        }
                        Mode::Diagonal => now + c.wind_at.max(1),
                        Mode::Fast | Mode::Offhand => now + 1,
                    };
                    c.until = c.wind_at + 30;
                    c.phase = Phase::Wind;
                }
                Some(false) => {}
                None => {
                    catch_end(m);
                    return;
                }
            }
        }
        Phase::Wind => {
            let look = wind_look(game, m, &c);
            let ready = now >= c.wind_at && (c.mode == Mode::Diagonal || own_pearl_since(game, c.thrown));
            if !ready {
                if now + 1 >= c.wind_at {
                    turn(CATCH, look.0, look.1);
                }
            } else {
                let slot = if c.mode == Mode::Offhand { Some(OFFHAND) } else { slot_of(game, Item::Wind) };
                match slot.and_then(|slot| throw_from(game, CATCH, slot, Item::Wind, look)) {
                    Some(false) => {}
                    _ if c.offhand_from.is_some() => c.phase = Phase::Restore,
                    _ => {
                        catch_end(m);
                        return;
                    }
                }
            }
        }
        Phase::Restore => {
            // A tick after the throw: the charge goes back to its hotbar slot, and the offhand gets back its own item.
            if st(|s| s.used) >= now {
                st(|s| s.catch = Some(c));
                return;
            }
            if let Some(slot) = c.offhand_from {
                if game.selected != slot {
                    hold(game, slot);
                }
                game.remote_swap_hands();
            }
            catch_end(m);
            return;
        }
    }
    st(|s| s.catch = Some(c));
}

// Diagonal: you and the pearl thrown with `look` simulated (you falling or standing, the pearl for 30 ticks); the tick
// (2 to 15 after) and the look of a wind charge (straight at 1.5, with your movement) whose path passes closest to the
// pearl, if within 0.4 of it.
fn diagonal_plan(game: &Game<'_>, look: (f32, f32)) -> Option<(u32, (f32, f32))> {
    let eye = game.eye_position()?;
    let p = game.player.as_ref()?;
    let ground = p.ground.on_ground;
    let inherit = |v: [f64; 3], ground: bool| [v[0], if ground { 0.0 } else { v[1] }, v[2]];
    // Standing, you stay where you are (your step slowing as on the ground); else you fall.
    let mut me = vec![(eye, [p.velocity.x, if ground { 0.0 } else { p.velocity.y }, p.velocity.z], ground)];
    for _ in 0..15 {
        let (pos, v, on) = me[me.len() - 1];
        let next = if on { [v[0] * 0.546, 0.0, v[2] * 0.546] } else { [v[0] * 0.91, (v[1] - 0.08) * 0.98, v[2] * 0.91] };
        me.push((add(pos, v), next, on));
    }
    let mut pearl = vec![add(eye, [0.0, -0.1, 0.0])];
    let mut v = add(scale(super::view_vector(look.0, look.1), THROW_SPEED), inherit(me[0].1, ground));
    for _ in 0..30 {
        let next = add(pearl[pearl.len() - 1], v);
        pearl.push(next);
        v = [v[0] * PEARL_DRAG, v[1] * PEARL_DRAG - PEARL_GRAVITY, v[2] * PEARL_DRAG];
    }
    // The closest a charge thrown at tick `at` with (yaw, pitch) comes to the pearl over `steps` ticks.
    let closest = |at: usize, yaw: f32, pitch: f32, steps: usize| -> f64 {
        let (from, mv, on) = me[at];
        let mut pos = from;
        let v = add(scale(super::view_vector(yaw, pitch), THROW_SPEED), inherit(mv, on));
        let mut best = f64::MAX;
        for j in 1..steps {
            if at + j >= pearl.len() {
                break;
            }
            pos = add(pos, v);
            best = best.min(util::dist(pos, pearl[at + j]));
        }
        best
    };
    let mut best: Option<(f64, usize, f32, f32)> = None;
    for at in 2..=15usize {
        if at + 1 >= pearl.len() {
            break;
        }
        for k in at + 1..pearl.len() {
            let (from, mv, on) = me[at];
            let need = add(add(pearl[k], scale(from, -1.0)), scale(inherit(mv, on), -((k - at) as f64)));
            if need.iter().map(|x| x * x).sum::<f64>() < 1e-6 {
                continue;
            }
            let (yaw0, pitch0) = look_to([0.0; 3], need);
            for pi in -10..=10 {
                for yi in -10..=10 {
                    let (yaw, pitch) = (yaw0 + yi as f32 * 0.5, pitch0 + pi as f32 * 0.5);
                    let d = closest(at, yaw, pitch, k - at + 4);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, at, yaw, pitch));
                    }
                }
            }
        }
    }
    let (mut d, at, mut yaw, mut pitch) = best?;
    let (y0, p0) = (yaw, pitch);
    for pi in -10..=10 {
        for yi in -10..=10 {
            let (y, p) = (y0 + yi as f32 * 0.1, p0 + pi as f32 * 0.05);
            let e = closest(at, y, p, pearl.len() - at);
            if e < d {
                (d, yaw, pitch) = (e, y, p);
            }
        }
    }
    (d <= 0.4).then_some((at as u32, (yaw, pitch.clamp(-90.0, 90.0))))
}

// ---- Key Throw's Pearl follow

fn follow(game: &mut Game<'_>, m: &Module) {
    let Some(eye) = game.eye_position() else {
        return;
    };
    let now = game.ticks;
    if st(|s| s.follow.is_none()) {
        let seen: Vec<Seen> = st(|s| {
            s.seen.iter().filter(|x| x.item == Item::Pearl && x.tick + 2 <= now && !s.followed.contains(&x.id)).copied().collect()
        });
        for s in seen {
            st(|t| t.followed.push(s.id));
            let Some(by) = enemy_thrower(game, s.id).and_then(|id| game.mobs.iter().find(|x| x.id == id)) else {
                continue;
            };
            if util::dist(eye, [by.position.x, by.position.y, by.position.z]) > m.n("Thrower range") {
                continue;
            }
            let Some(flight) = pearl_landing(game, s.id, 400, false) else {
                continue;
            };
            let d = util::dist(eye, flight.land);
            if d < m.n("Min landing distance") || d > m.n("Landing range") {
                continue;
            }
            let at = now + rolled_ticks(game, m, "Shortest follow delay (ms)", "Longest follow delay (ms)");
            st(|t| t.follow = Some((flight.land, at)));
            break;
        }
    }
    let Some((to, at)) = st(|s| s.follow) else {
        return;
    };
    if now < at || (m.b("Follow only on ground") && !game.player.as_ref().is_some_and(|p| p.ground.on_ground)) {
        if now > at + 40 {
            st(|s| s.follow = None);
        }
        return;
    }
    st(|s| s.follow = None);
    if !(0..9).chain(std::iter::once(OFFHAND)).any(|s| has(game, s, Item::Pearl)) {
        return;
    }
    if let Some(look) = follow_look(game, eye, to, m.n("Max error")) {
        st(|s| s.single = Some(Single { owner: THROW, item: Item::Pearl, aim: Aim::At(look.0, look.1), at: now, until: now + 10, jump: false, then_wind: false }));
    }
}

#[derive(Clone, Copy)]
struct Shot {
    pitch: f32,
    error: f64,
    ticks: u32,
    apex: f64,
}

// The better of two shots: sooner there, then a lower arc, then nearer, then the lower pitch (more downward).
fn better(a: Shot, b: Shot) -> Shot {
    let key = |s: &Shot| (s.ticks, s.apex, s.error);
    match key(&a).partial_cmp(&key(&b)) {
        Some(std::cmp::Ordering::Less) => a,
        Some(std::cmp::Ordering::Greater) => b,
        _ if a.pitch > b.pitch => a,
        _ => b,
    }
}

// The look whose pearl lands nearest `to` (within Max error): every 2 degrees of pitch toward it, the three best dips
// looked at again every 0.2 degrees, and of those near the best, the one there soonest.
fn follow_look(game: &Game<'_>, eye: [f64; 3], to: [f64; 3], max_error: f64) -> Option<(f32, f32)> {
    let yaw = look_to(eye, to).0;
    let start = [eye[0], eye[1] - 0.1, eye[2]];
    let shot = |pitch: f32| -> Option<Shot> {
        let f = pearl_flight(game, start, scale(super::view_vector(yaw, pitch), THROW_SPEED), 400, false)?;
        Some(Shot { pitch, error: util::dist(f.land, to), ticks: f.ticks, apex: f.apex })
    };
    let coarse: Vec<Option<Shot>> = (0..=85).map(|i| shot(-85.0 + i as f32 * 2.0)).collect();
    let err = |i: usize| coarse.get(i).and_then(|s| s.map(|s| s.error)).unwrap_or(f64::INFINITY);
    let dips: Vec<Shot> = (0..coarse.len())
        .filter_map(|i| coarse[i].filter(|s| s.error <= err(i.wrapping_sub(1)) && s.error <= err(i + 1)))
        .collect();
    let mut picked: Vec<Shot> = Vec::new();
    while picked.len() < 3 {
        let Some(next) = dips
            .iter()
            .filter(|d| !picked.iter().any(|p| (p.pitch - d.pitch).abs() < 6.0))
            .min_by(|a, b| a.error.total_cmp(&b.error))
        else {
            break;
        };
        picked.push(*next);
    }
    let first_ok = coarse.iter().flatten().filter(|s| s.error <= max_error).copied().reduce(better);
    let mut centres: Vec<f32> = picked.iter().map(|s| s.pitch).collect();
    if let Some(ok) = first_ok.filter(|ok| !picked.iter().any(|p| (p.pitch - ok.pitch).abs() < 0.1)) {
        centres.push(ok.pitch);
    }
    let mut pool = picked;
    for c in centres {
        for i in -11..=11 {
            let p = c + i as f32 * 0.2;
            if (-85.0..=85.0).contains(&p)
                && let Some(s) = shot(p)
            {
                pool.push(s);
            }
        }
    }
    let least = pool.iter().filter(|s| s.error <= max_error).map(|s| s.error).fold(f64::INFINITY, f64::min);
    if !least.is_finite() {
        return None;
    }
    let cut = max_error.min(least + 0.25);
    pool.into_iter().filter(|s| s.error <= cut).reduce(better).map(|s| (yaw, s.pitch))
}

// ---- Wind Jump's Elytra and Hit charges

// Elytra: worn (used from the hotbar if not), a jump from the ground and another in the air to glide, and a wind
// charge straight down when the glide comes within reach of the ground (closer the faster you go forward), at most
// one every 8 ticks.
fn elytra(game: &mut Game<'_>) {
    let now = game.ticks;
    let worn = game.armor[1].as_ref().is_some_and(|s| util::short(&s.item) == "elytra");
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (ground, gliding, feet) = (p.ground.on_ground, p.fall_flying, [p.position.x, p.position.y, p.position.z]);
    if !worn {
        if let Some(slot) = (0..9).find(|&s| game.stack_in(s).is_some_and(|i| util::short(&i.item) == "elytra")) {
            if game.selected != slot {
                let before = game.selected;
                st(|s| {
                    s.back.get_or_insert(before);
                });
                hold(game, slot);
            }
            let sequence = util::sequence(game);
            game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw: game.yaw, pitch: game.pitch }));
            st(|s| s.used = now);
        }
        return;
    }
    if ground {
        st(|s| s.jump = true);
        return;
    }
    if !gliding {
        if now >= st(|s| s.glide_at) + 8 {
            st(|s| {
                s.glide_at = now;
                s.jump = true;
            });
        }
        return;
    }
    let above = game.clip_ray(feet, [0.0, -1.0, 0.0], 256.0).map_or(256.0, |h| feet[1] - h.point[1]);
    let lean = f64::from(forward_speed(game, game.yaw).clamp(0.0, 1.0));
    if above > 1.2 - 0.8 * lean || now < st(|s| s.elytra_at) + 8 || !(0..9).chain(std::iter::once(OFFHAND)).any(|s| has(game, s, Item::Wind)) {
        return;
    }
    let pitch = 87.0 + roll(game, 0.0, 3.0) as f32;
    st(|s| {
        s.elytra_at = now;
        s.single = Some(Single { owner: WIND, item: Item::Wind, aim: Aim::Pitch(pitch), at: now, until: now + 4, jump: false, then_wind: false });
    });
    single_step(game);
}

// Hit charges: another's wind charge within 5 blocks, in sight and within Hit reach of your eyes, hit away; the look
// goes ahead of it (where it will be next tick) and the hit waits for a server look within 6 degrees of it (two
// ticks apart at least); not while you use an item, nor on the ground with In air only.
fn hit_charges(game: &mut Game<'_>, m: &Module) {
    let now = game.ticks;
    let Some(eye) = game.eye_position() else {
        return;
    };
    let on_ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    if game.using.is_some() || (m.b("In air only") && on_ground) || now < st(|s| s.hit_at) + 2 {
        return;
    }
    if super::sunset_combat_control::owner().is_some_and(|o| o != WIND) {
        return;
    }
    let seen = st(|s| s.seen.clone());
    let feet = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]).unwrap_or(eye);
    let target = game
        .creatures
        .projectiles
        .iter()
        .filter(|c| c.kind == ProjectileKind::WindCharge && c.id != 0 && !c.stuck)
        .filter(|c| seen.iter().find(|s| s.id == c.id).is_some_and(|s| !own(game, s)))
        .map(|c| (c.id, [c.position.x, c.position.y + 0.156_25, c.position.z], [c.velocity.x, c.velocity.y, c.velocity.z]))
        .filter(|(_, at, _)| util::dist(feet, *at) <= 5.0)
        .min_by(|a, b| util::dist(feet, a.1).total_cmp(&util::dist(feet, b.1)));
    let Some((id, centre, vel)) = target else {
        return;
    };
    if f64::from(wrap(look_to(eye, centre).0 - game.yaw).abs()) > m.n("Max yaw") || !util::clear_line(game, centre, [i32::MIN; 3]) {
        return;
    }
    let (mut yaw, mut pitch) = look_to(eye, add(centre, vel));
    if m.b("Humanized aim") {
        let spread = (1.0 / util::dist(eye, centre).max(1.0)) as f32;
        yaw += (roll(game, -1.0, 1.0) as f32) * spread;
        pitch += (roll(game, -1.0, 1.0) as f32) * spread;
    }
    turn(WIND, yaw, pitch.clamp(-89.9, 89.9));
    let half = 0.156_25;
    let near = [0, 1, 2].map(|k| eye[k].clamp(centre[k] - half, centre[k] + half));
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let now_look = look_to(eye, centre);
    if util::dist(eye, near) > m.n("Hit reach") || wrap(have.0 - now_look.0).abs() > 6.0 || (have.1 - now_look.1).abs() > 6.0 {
        return;
    }
    game.send(S::Attack(play::Attack { id: id as i32 }));
    game.swing_arm();
    game.send(S::Punch(horizon_protocol::packets::Empty));
    game.reset_attack_strength();
    st(|s| s.hit_at = now);
}
