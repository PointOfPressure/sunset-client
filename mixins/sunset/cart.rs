// Cart: TNT minecart PvP in one module. Aura hits carts other players put down before they go off, Auto cart puts a
// rail and a cart where your arrow lands (or at your crosshair on a key, then shoots it), Xbow lights a fire on the
// line to a fresh cart and fires a crossbow through it, Cover blocks a cart that would hurt you, Refill keeps a hotbar
// slot of carts, and Pearls punishes an enemy's pearl with a cart where it lands.
//
// Every action goes out at the start of a tick, before that tick's move packet, with the look the move packet before
// it carried (Grim's Post and rotation checks), one slot change a tick at most.
use super::sunset::{Category, Module, hold, look_at, turn, with};
use super::sunset_explosive_damage::{Blast, local_damage};
use super::sunset_explosive_policy::Budget;
use super::sunset_rotation::Rng;
use super::sunset_util::{self as util, short};
use super::{Game, Screen, minecarts::CartKind};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::sync::Mutex;
use winit::event::MouseButton;

pub(super) const NAME: &str = "Cart";
const RAILS: [&str; 4] = ["rail", "powered_rail", "detector_rail", "activator_rail"];
// Blocks Auto cart's Safe blocks puts between you and the cart: the blast-proof ones a cart fighter carries.
const SHIELDS: [&str; 7] = [
    "obsidian",
    "crying_obsidian",
    "netherite_block",
    "deepslate",
    "cobbled_deepslate",
    "ender_chest",
    "respawn_anchor",
];
// How long a cart counts as yours after you put it down (ticks), and how close to the spot.
const OWN_TICKS: u64 = 100;
const OWN_RANGE: f64 = 1.5;
// Aura reaches carts this far (box centre) and hits within vanilla's 3 blocks of the box.
const AURA_SCAN: f64 = 5.0;
const AURA_REACH: f64 = 3.0;
// A placement job gives up this long after it started.
const JOB_TICKS: u64 = 80;
const ARROW_GRAVITY: f64 = 0.05;
const CROSSBOW_SPEED: f64 = 3.15;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "TNT minecart PvP: hits, places, sets off and covers carts.")
            .bool("Aura", true)
            .num("Reaction min", 100.0, 0.0, 1000.0, 10.0)
            .num("Reaction max", 250.0, 0.0, 1000.0, 10.0)
            .num("Swap delay", 50.0, 0.0, 300.0, 10.0)
            .bool("Swap back", true)
            .num("Swap back delay", 50.0, 0.0, 300.0, 10.0)
            .num("Attack min", 50.0, 0.0, 1000.0, 10.0)
            .num("Attack max", 150.0, 0.0, 1000.0, 10.0)
            .num("Target FOV", 180.0, 30.0, 360.0, 10.0)
            .bool("Humanized aim", true)
            .bool("Auto cart", true)
            .pick("Place mode", &["Trajectory", "Crosshair"])
            .bool("On bow shot", true)
            .text("Trigger key", "None")
            .num("Min charge", 1.0, 1.0, 20.0, 1.0)
            .num("Arc", 20.0, 0.0, 100.0, 5.0)
            .num("Overload", 1.0, 1.0, 4.0, 1.0)
            .text("Overload key", "None")
            .num("Overload min", 50.0, 0.0, 1000.0, 10.0)
            .num("Overload max", 150.0, 0.0, 1000.0, 10.0)
            .bool("Overload hotbar only", false)
            .bool("Auto hit placed", true)
            .num("Auto hit range", 24.0, 5.0, 50.0, 1.0)
            .num("Auto hit charge", 20.0, 10.0, 20.0, 1.0)
            .bool("Safe blocks", true)
            .num("Bow back delay", 0.0, 0.0, 20.0, 1.0)
            .bool("Back to bow", true)
            .bool("Render path", true)
            .bool("Xbow", false)
            .text("Xbow key", "None")
            .bool("Pearls", false)
            .num("Pearl range", 48.0, 16.0, 96.0, 4.0)
            .num("Pearl min damage", 15.0, 0.0, 40.0, 1.0)
            .num("Pearl self damage", 6.0, 0.0, 30.0, 0.5)
            .bool("Prefer xbow", true)
            .num("Switch delay", 2.0, 0.0, 20.0, 1.0)
            .bool("Silent aim", true)
            .num("Silent aim FOV", 180.0, 0.0, 180.0, 5.0)
            .bool("Cover", false)
            .pick("Cover mode", &["Silent", "Legit"])
            .pick("Protect", &["Both", "Only self", "Only enemy"])
            .num("Cover min damage", 5.0, 0.0, 30.0, 0.5)
            .num("Cover max damage", 5.0, 0.0, 30.0, 0.5)
            .text("Cover blocks", "logs,planks,cobblestone,obsidian")
            .num("Max blocks", 2.0, 1.0, 4.0, 1.0)
            .num("Cover delay min", 80.0, 0.0, 500.0, 10.0)
            .num("Cover delay max", 160.0, 0.0, 500.0, 10.0)
            .pick("Cover swap", &["Normal", "Silent", "None"])
            .num("Cover range", 10.0, 3.0, 15.0, 0.5)
            .num("Cover FOV", 180.0, 0.0, 180.0, 5.0)
            .bool("Refill", false)
            .pick("Refill mode", &["Always", "Key"])
            .text("Refill key", "None")
            .num("Refill slot", 9.0, 1.0, 9.0, 1.0)
            .num("Refill delay", 100.0, 0.0, 1000.0, 10.0),
    ]
}

// Which section a setting belongs to; the shared ones show with any section that uses them.
pub(super) fn visible(m: &Module, name: &str) -> bool {
    let auto = m.b("Auto cart");
    match name {
        "Aura" | "Auto cart" | "Xbow" | "Pearls" | "Cover" | "Refill" => true,
        "Reaction min" | "Reaction max" | "Swap delay" | "Swap back" | "Attack min" | "Attack max" | "Target FOV"
        | "Humanized aim" => m.b("Aura"),
        "Swap back delay" => m.b("Aura") && m.b("Swap back"),
        "Overload key" | "Overload min" | "Overload max" | "Overload hotbar only" => auto && m.n("Overload") > 1.0,
        "Auto hit range" | "Auto hit charge" => auto && m.b("Auto hit placed"),
        "Render path" => auto && m.p("Place mode") == "Trajectory",
        "Xbow key" => m.b("Xbow"),
        "Pearl range" | "Pearl min damage" | "Pearl self damage" | "Prefer xbow" => m.b("Pearls"),
        "Switch delay" => auto || m.b("Xbow") || m.b("Pearls"),
        "Silent aim" => auto || m.b("Xbow") || m.b("Pearls"),
        "Silent aim FOV" => (auto || m.b("Xbow") || m.b("Pearls")) && m.b("Silent aim"),
        "Cover FOV" => m.b("Cover") && m.p("Cover mode") == "Silent",
        n if n.starts_with("Cover") || n == "Protect" || n == "Max blocks" => m.b("Cover"),
        "Refill key" => m.b("Refill") && m.p("Refill mode") == "Key",
        n if n.starts_with("Refill") => m.b("Refill"),
        _ => auto,
    }
}

// ---- state

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    // You shot: a rail and a cart where the arrow will land, then back to the bow.
    AfterShot,
    // Trigger key: a rail and a cart at the crosshair, then your bow or crossbow at the cart.
    Trigger,
    // Overload key: the shot first toward your aim, then the carts stacked where it will land.
    ShootFirst,
    // Xbow key: rail, cart, a fire on the line to it, then a charged crossbow through the fire.
    Xbow,
    // Pearls: the cart where an enemy pearl lands, set off as it comes down.
    Pearl,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    // Pearls: waiting for the pearl to come close enough to the ground.
    Armed,
    Rail,
    Cart,
    Fire,
    Shield,
    Weapon,
    Shoot,
    Draw,
    Back,
}

#[derive(Clone, Copy, Debug)]
struct Job {
    kind: Kind,
    step: Step,
    // The block the rail stands on (the rail is the one above); None until a ShootFirst shot has gone.
    base: Option<[i32; 3]>,
    started: u64,
    // Nothing happens before this tick (Switch delay, the overload pause).
    wait: u64,
    // The slot held before the job, to come back to.
    back: usize,
    carts: u32,
    stack: u32,
    fire: Option<[i32; 3]>,
    crossbow: bool,
    // ShootFirst: the block aimed at.
    aimed: Option<[i32; 3]>,
    pearl: Option<u32>,
    // The look this step wants the server to have, and the ticks it has waited for it.
    look: Option<(f32, f32)>,
    looked: u32,
}

impl Job {
    fn new(kind: Kind, step: Step, base: Option<[i32; 3]>, now: u64, back: usize) -> Self {
        Self {
            kind,
            step,
            base,
            started: now,
            wait: now,
            back,
            carts: 0,
            stack: 1,
            fire: None,
            crossbow: false,
            aimed: None,
            pearl: None,
            look: None,
            looked: 0,
        }
    }
    fn rail(&self) -> Option<[i32; 3]> {
        self.base.map(|b| [b[0], b[1] + 1, b[2]])
    }
}

#[derive(Default)]
struct Aura {
    target: Option<u32>,
    react_at: u64,
    // The slot held before the sword, and when the hit may go.
    swapped: Option<usize>,
    hit_at: u64,
    next_hit: u64,
    back_at: Option<u64>,
    back_delay: u64,
}

#[derive(Default)]
struct Cart {
    rng: Option<Rng>,
    job: Option<Job>,
    aura: Aura,
    // Carts you put down: where, and when.
    own: Vec<([f64; 3], u64)>,
    // Your bow's draw as last seen (it is gone by the time the release packet goes).
    drawn: Option<u32>,
    // The game is about to send a shot Sunset made: not one to place after.
    own_shot: bool,
    // You drew the bow; Auto hit placed let it go at a cart.
    auto_released: bool,
    pending_shot: Option<(f32, f32, f64)>,
    // Cover: blocks put against each cart, the last spot, when the next may go and whether a cart threatened.
    covered: Vec<(u32, u32)>,
    cover_spot: Option<([i32; 3], u64)>,
    cover_at: u64,
    threatened: bool,
    cover_back: Option<usize>,
    refill_at: u64,
    // Pearls seen and where first (enemy pearls are the ones first seen away from you), and those already answered.
    pearls: Vec<(u32, [f64; 3])>,
    answered: Vec<u32>,
    // The slot a cart was pulled into from the main inventory, waiting to show.
    pulled: Option<(usize, u64)>,
    // Fresh key presses: Trigger, Overload, Xbow.
    pressed: [bool; 3],
    shown: Vec<([i32; 3], u32)>,
}

static CART: Mutex<Option<Cart>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut Cart) -> R) -> R {
    f(CART.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Cart::default))
}

fn roll(lo: f64, hi: f64, seed: u64) -> f64 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    st(|s| {
        let r = s.rng.get_or_insert_with(|| Rng::new(seed ^ 0x9e37_79b9_7f4a_7c15)).next();
        lo + r * (hi - lo)
    })
}

// Milliseconds as ticks, at least `min`.
fn ticks_of(ms: f64, min: u64) -> u64 {
    ((ms / 50.0).ceil().max(0.0) as u64).max(min)
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

// ---- items

fn item_is(game: &Game<'_>, slot: usize, want: impl Fn(&str) -> bool) -> bool {
    game.inventory.get(slot).and_then(|s| s.as_ref()).is_some_and(|s| want(short(&s.item)))
}

fn rail_slot(game: &Game<'_>) -> Option<usize> {
    util::find_hotbar(game, |n| RAILS.contains(&n))
}

fn flint_slot(game: &Game<'_>) -> Option<usize> {
    util::find_hotbar(game, |n| n == "flint_and_steel")
}

fn bow_slot(game: &Game<'_>) -> Option<usize> {
    util::find_hotbar(game, |n| n == "bow")
}

fn sword_slot(game: &Game<'_>) -> Option<usize> {
    util::find_hotbar(game, |n| n.ends_with("_sword"))
}

// A crossbow loaded with an arrow (Xbow and Pearls need the arrow's fire), or loaded at all (Auto cart's shots).
fn crossbow_slot(game: &Game<'_>, arrow: bool) -> Option<usize> {
    (0..9).find(|&i| {
        game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| {
            short(&s.item) == "crossbow"
                && s.data().is_some_and(|d| {
                    !d.charged.is_empty()
                        && (!arrow || d.charged.iter().any(|a| matches!(short(&a.item), "arrow" | "spectral_arrow")))
                })
        })
    })
}

fn charged_in_hand(game: &Game<'_>) -> bool {
    crossbow_slot(game, false) == Some(game.selected)
}

// A hotbar slot of TNT carts; with `pull`, one is brought up from the main inventory into an empty hotbar slot first
// (None this tick: it shows once the server has it).
fn cart_slot(game: &mut Game<'_>, pull: bool) -> Option<usize> {
    if let Some(slot) = util::find_hotbar(game, |n| n == "tnt_minecart") {
        st(|s| s.pulled = None);
        return Some(slot);
    }
    if !pull || !util::inventory_idle(game) {
        return None;
    }
    if st(|s| s.pulled.is_some_and(|(_, at)| game.ticks < at + 10)) {
        return None;
    }
    let from = (9..36).find(|&i| item_is(game, i, |n| n == "tnt_minecart"))?;
    let to =
        (0..9).find(|&i| i != game.selected && game.inventory[i].is_none() && !super::sunset_loadouts::locked(i))?;
    swap_click(game, from, to);
    st(|s| s.pulled = Some((to, game.ticks)));
    None
}

// The inventory's own hotbar-key swap: the stack in `from` and hotbar slot `to` change places.
fn swap_click(game: &mut Game<'_>, from: usize, to: usize) {
    if super::sunset_loadouts::locked(from) || super::sunset_loadouts::locked(to) {
        return;
    }
    game.remote_menu_ops(&[(Some(from), to as i32, horizon_client::menu::Input::Swap)]);
}

// ---- geometry

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn centre(p: [i32; 3]) -> [f64; 3] {
    util::center_of(p)
}

fn wrap(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn same_look(a: (f32, f32), b: (f32, f32)) -> bool {
    wrap(a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
}

fn in_reach(game: &Game<'_>, at: [f64; 3]) -> bool {
    game.eye_position().is_some_and(|eye| util::dist(eye, at) <= game.reach())
}

fn is_rail(game: &Game<'_>, p: [i32; 3]) -> bool {
    RAILS.contains(&short(&util::facts(game, p).name))
}

fn solid_top(game: &Game<'_>, p: [i32; 3]) -> bool {
    !util::is_air(game, p) && util::facts(game, p).full_collision
}

fn cart_bounds(c: &super::minecarts::Minecart) -> ([f64; 3], [f64; 3]) {
    let b = c.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

fn cart_centre(c: &super::minecarts::Minecart) -> [f64; 3] {
    let (lo, hi) = cart_bounds(c);
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]
}

fn tnt_carts<'a>(game: &'a Game<'_>) -> impl Iterator<Item = &'a super::minecarts::Minecart> {
    game.carts.iter().filter(|c| c.kind == CartKind::Tnt && Some(c.id) != game.riding)
}

fn cart_in(game: &Game<'_>, p: [i32; 3]) -> bool {
    let (lo, hi) = (p.map(f64::from), p.map(|v| f64::from(v) + 1.0));
    tnt_carts(game).any(|c| {
        let (a, b) = cart_bounds(c);
        (0..3).all(|i| a[i] < hi[i] && b[i] > lo[i])
    })
}

fn own_cart(c: &super::minecarts::Minecart) -> bool {
    let at = [c.position.x, c.position.y, c.position.z];
    st(|s| s.own.iter().any(|(p, _)| util::dist(*p, at) <= OWN_RANGE))
}

// Nothing solid on the straight line between two points (sampled every eighth of a block), `except` aside.
fn open_line(game: &Game<'_>, from: [f64; 3], to: [f64; 3], except: [i32; 3]) -> bool {
    let d = util::dist(from, to);
    let steps = (d * 8.0).ceil().max(1.0) as i32;
    (1..steps).all(|i| {
        let t = f64::from(i) / f64::from(steps);
        let at = [0, 1, 2].map(|k| (from[k] + (to[k] - from[k]) * t).floor() as i32);
        at == except || util::is_air(game, at) || !util::facts(game, at).full_collision
    })
}

// The first block a straight segment meets, if any.
fn segment_hit(game: &Game<'_>, from: [f64; 3], to: [f64; 3]) -> Option<[i32; 3]> {
    let d = util::dist(from, to);
    if d < 1e-9 {
        return None;
    }
    let dir = [0, 1, 2].map(|k| (to[k] - from[k]) / d);
    game.clip_ray(from, dir, d).map(|h| h.pos)
}

// ---- shots

// An arrow's start and speed shot with (yaw, pitch) at `power` (a bow's draw 0 to 1, 1.05 a crossbow's): from just
// under the eyes, with the shooter's movement (up and down only off the ground).
fn launch(game: &Game<'_>, look: (f32, f32), power: f64) -> Option<([f64; 3], [f64; 3])> {
    let eye = game.eye_position()?;
    let p = game.player.as_ref()?;
    let dir = super::view_vector(look.0, look.1);
    let speed = power.clamp(0.0, 1.05) * 3.0;
    let mut v = dir.map(|d| d * speed);
    v[0] += p.velocity.x;
    v[2] += p.velocity.z;
    if !p.ground.on_ground {
        v[1] += p.velocity.y;
    }
    Some(([eye[0], eye[1] - 0.1, eye[2]], v))
}

// The block an arrow shot so first lands on.
fn landing(game: &Game<'_>, look: (f32, f32), power: f64) -> Option<[i32; 3]> {
    let (mut pos, mut v) = launch(game, look, power)?;
    for _ in 0..300 {
        let next = add(pos, v);
        if let Some(hit) = segment_hit(game, pos, next) {
            return Some(hit);
        }
        pos = next;
        v = [v[0] * 0.99, v[1] * 0.99 - ARROW_GRAVITY, v[2] * 0.99];
        if pos[1] < -65.0 {
            break;
        }
    }
    None
}

// A bow's draw after `ticks` (BowItem.getPowerForTime).
fn draw_power(ticks: u32) -> f64 {
    let t = f64::from(ticks) / 20.0;
    ((t * t + t * 2.0) / 3.0).min(1.0)
}

// The look whose arrow lands nearest `target` at `power`, among the arcs that land within two blocks of it: Arc 0
// takes the flattest of them, 100 the highest lob.
fn arc_look(game: &Game<'_>, target: [f64; 3], power: f64, arc: f64) -> Option<(f32, f32)> {
    let eye = game.eye_position()?;
    let yaw = look_at(eye, target).0;
    let d2 = |p: [i32; 3]| {
        let c = centre(p);
        (0..3).map(|i| (c[i] - target[i]).powi(2)).sum::<f64>()
    };
    let mut found: Vec<(f32, f64)> = Vec::new();
    let mut pitch = 60.0f32;
    while pitch >= -89.0 {
        if let Some(p) = landing(game, (yaw, pitch), power) {
            found.push((pitch, d2(p)));
        }
        pitch -= 0.5;
    }
    let best = found.iter().map(|f| f.1).fold(f64::INFINITY, f64::min);
    if best > 4.0 {
        return None;
    }
    found.retain(|f| f.1 <= best + 0.25);
    let i = ((arc / 100.0) * (found.len() as f64 - 1.0)).round().max(0.0) as usize;
    let (mut pick, mut near) = found[i.min(found.len() - 1)];
    let mut p = pick - 0.5;
    while p <= pick + 0.5 {
        if let Some(l) = landing(game, (yaw, p), power) {
            let d = d2(l);
            if d < near {
                (near, pick) = (d, p);
            }
        }
        p += 0.05;
    }
    Some((yaw, pick))
}

// The arc at power `power` whose arrow meets the box of a cart on `rail` (Arc 0 the flattest that does, 100 the
// highest), taken from the middle of the run of looks that meet it, so a little spread still hits.
fn cart_arc(game: &Game<'_>, rail: [i32; 3], power: f64, arc: f64) -> Option<(f32, f32)> {
    let eye = game.eye_position()?;
    let yaw = look_at(eye, centre(rail)).0;
    let speed = power.min(1.05) * 3.0;
    let mut runs: Vec<(f32, f32)> = Vec::new();
    let mut pitch = 60.0f32;
    let mut open: Option<f32> = None;
    while pitch >= -89.5 {
        let hits = pitch >= -89.0 && reaches_cart(game, (yaw, pitch), speed, rail, None).is_some();
        match (hits, open) {
            (true, None) => open = Some(pitch),
            (false, Some(start)) => {
                runs.push((start, pitch + 0.5));
                open = None;
            }
            _ => {}
        }
        pitch -= 0.5;
    }
    let i = ((arc / 100.0) * (runs.len() as f64 - 1.0)).round().max(0.0) as usize;
    let (a, b) = *runs.get(i.min(runs.len().checked_sub(1)?))?;
    Some((yaw, (a + b) / 2.0))
}

// A crossbow's (or full bow's) near-straight shot at the cart on `rail`: aimed a little above it for the drop.
fn straight_look(game: &Game<'_>, rail: [i32; 3], speed: f64, lift: f64) -> Option<(f32, f32)> {
    let eye = game.eye_position()?;
    let at = [f64::from(rail[0]) + 0.5, f64::from(rail[1]) + 0.45 + lift, f64::from(rail[2]) + 0.5];
    let n = (util::dist(eye, at) / speed).ceil();
    let raised = [at[0], at[1] + ARROW_GRAVITY * n * (n + 1.0) / 2.0, at[2]];
    Some(look_at([eye[0], eye[1] - 0.1, eye[2]], raised))
}

// Follows a shot in quarter-block steps: Some(ticks) when it reaches the cart's box on `rail` (through `fire`'s block
// first, if given) before any block but the rail's, None if it never does.
fn reaches_cart(game: &Game<'_>, look: (f32, f32), speed: f64, rail: [i32; 3], fire: Option<[i32; 3]>) -> Option<u32> {
    let (mut pos, v) = launch(game, look, speed / 3.0)?;
    let mut v = v;
    let r = rail.map(f64::from);
    let in_cart = |p: [f64; 3]| {
        p[0] > r[0] + 0.01
            && p[0] < r[0] + 0.99
            && p[1] > r[1] + 0.0625
            && p[1] < r[1] + 0.7625
            && p[2] > r[2] + 0.01
            && p[2] < r[2] + 0.99
    };
    let mut through = fire.is_none();
    for tick in 1..=60 {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let parts = (len / 0.25).ceil().max(1.0) as i32;
        let start = pos;
        for _ in 0..parts {
            pos = [0, 1, 2].map(|k| pos[k] + v[k] / f64::from(parts));
            if fire.is_some_and(|f| pos.map(|c| c.floor() as i32) == f) {
                through = true;
            }
            if in_cart(pos) {
                return through.then_some(tick);
            }
        }
        if segment_hit(game, start, pos).is_some_and(|hit| hit != rail) || pos[1] < r[1] - 3.0 {
            return None;
        }
        v = [v[0] * 0.99, v[1] * 0.99 - ARROW_GRAVITY, v[2] * 0.99];
    }
    None
}

// Xbow: where on the crossbow's line to the cart a fire can go: an open block with a solid top under it, in reach,
// clear of you and of the rail and the cart.
fn fire_spot(game: &Game<'_>, rail: [i32; 3]) -> Option<[i32; 3]> {
    let look = straight_look(game, rail, CROSSBOW_SPEED, 0.0)?;
    let (mut pos, mut v) = launch(game, look, 1.05)?;
    let me = game.player.as_ref()?.bounds();
    let (lo, hi) = (me.min_position(), me.max_position());
    let near_me = |b: [i32; 3]| {
        let (a, z) = (b.map(f64::from), b.map(|c| f64::from(c) + 1.0));
        a[0] < hi.x + 0.3
            && z[0] > lo.x - 0.3
            && a[1] < hi.y + 0.3
            && z[1] > lo.y - 0.3
            && a[2] < hi.z + 0.3
            && z[2] > lo.z - 0.3
    };
    let above = [rail[0], rail[1] + 1, rail[2]];
    for _ in 0..60 {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let parts = (len / 0.25).ceil().max(1.0) as i32;
        for _ in 0..parts {
            pos = [0, 1, 2].map(|k| pos[k] + v[k] / f64::from(parts));
            let b = pos.map(|c| c.floor() as i32);
            if b == rail || b == above {
                return None;
            }
            let below = [b[0], b[1] - 1, b[2]];
            if !near_me(b)
                && in_reach(game, [f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5])
                && util::replaceable(game, b)
                && solid_top(game, below)
            {
                return Some(b);
            }
        }
        v = [v[0] * 0.99, v[1] * 0.99 - ARROW_GRAVITY, v[2] * 0.99];
    }
    None
}

// ---- the server's look

// The look a step wants: none with Silent aim off or past Silent aim FOV (it acts with your own look then).
fn wanted(game: &Game<'_>, m: &Module, look: (f32, f32), fov: f64) -> Option<(f32, f32)> {
    Some(look).filter(|l| m.b("Silent aim") && f64::from(wrap(l.0 - game.yaw).abs()) <= fov)
}

// Whether the server has `look` from the last move packet; if not, it is asked for (it goes with this tick's move
// packet) and the step waits a tick. Either way the look stays for this tick's move packet.
fn have_look(game: &Game<'_>, look: Option<(f32, f32)>) -> bool {
    let Some(look) = look else {
        return true;
    };
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    turn(NAME, look.0, look.1);
    same_look(have, look)
}

// ---- packets

fn place_on(game: &mut Game<'_>, clicked: [i32; 3], face: i32, cursor: [f32; 3]) {
    let sequence = util::sequence(game);
    game.send(S::UseItemOn(play::UseItemOn {
        hand: 0,
        pos: clicked,
        face,
        cursor,
        inside: false,
        world_border: false,
        sequence,
    }));
    game.swing_arm();
    game.send(S::Punch(Empty));
}

fn fire_crossbow(game: &mut Game<'_>, look: (f32, f32)) {
    st(|s| s.own_shot = true);
    let sequence = util::sequence(game);
    let yaw = super::sunset::wire_yaw(game, look.0);
    game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: look.1.clamp(-90.0, 90.0) }));
    game.swing_arm();
    game.use_delay = game.use_delay.max(2);
    game.clicks.right_click_delay = game.clicks.right_click_delay.max(2);
}

// ---- hooks

// A packet going out (after Sunset changed it): carts you put down, and the shots Auto cart places after.
pub(super) fn sent(game: &Game<'_>, packet: &S) {
    match packet {
        S::UseItemOn(p) if p.hand == 0 && item_is(game, game.selected, |n| n == "tnt_minecart") => {
            let face = match p.face {
                0 => [0, -1, 0],
                1 => [0, 1, 0],
                2 => [0, 0, -1],
                3 => [0, 0, 1],
                4 => [-1, 0, 0],
                _ => [1, 0, 0],
            };
            let ticks = game.ticks;
            st(|s| {
                s.own.retain(|(_, at)| ticks < at + OWN_TICKS);
                s.own.push((centre(p.pos), ticks));
                s.own.push((centre([p.pos[0] + face[0], p.pos[1] + face[1], p.pos[2] + face[2]]), ticks));
            });
        }
        S::PlayerAction(p) if p.action == 6 && item_is(game, game.selected, |n| n == "bow") => {
            let drawn = st(|s| s.drawn).unwrap_or(0);
            shot_sent(game, (drawn >= 4).then(|| draw_power(drawn)));
        }
        S::UseItem(p) if p.hand == 0 && charged_in_hand(game) => shot_sent(game, Some(1.05)),
        _ => {}
    }
}

// A shot of yours went (None: a bow let go too early to fly far): Auto cart places at its landing, unless Sunset shot
// it.
fn shot_sent(game: &Game<'_>, power: Option<f64>) {
    let look = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    st(|s| {
        let sunsets = std::mem::take(&mut s.own_shot) | std::mem::take(&mut s.auto_released);
        if let Some(power) = power.filter(|_| !sunsets) {
            s.pending_shot = Some((look.0, look.1, power));
        }
    });
}

// A key went down: Trigger key, Overload key or Xbow key starts its job at the next tick.
pub(super) fn key(game: &Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_playing() || super::sunset::gui_open() {
        return;
    }
    let Some(m) = module() else {
        return;
    };
    let named = |setting: &str| {
        super::sunset::key_from_text(m.t(setting)).is_some_and(|w| super::sunset::key_from_text(key) == Some(w))
    };
    let wants = [
        m.b("Auto cart") && named("Trigger key"),
        m.b("Auto cart") && named("Overload key"),
        m.b("Xbow") && named("Xbow key"),
    ];
    st(|s| {
        for (i, w) in wants.into_iter().enumerate() {
            s.pressed[i] |= w;
        }
    });
}

// Turned off, a new world or life: let go of everything.
pub(super) fn reset(game: &mut Game<'_>) {
    let (job, back) = st(|s| (s.job.take(), s.aura.swapped.take().or(s.cover_back.take())));
    if let Some(j) = job {
        if matches!(j.step, Step::Draw) {
            let _ = game.play_mouse(MouseButton::Right, false);
        }
        if j.back < 9 && game.selected != j.back {
            hold(game, j.back);
        }
    } else if let Some(b) = back.filter(|&b| b < 9 && b != game.selected) {
        hold(game, b);
    }
    super::sunset::cancel_turns(NAME);
    super::sunset_combat_control::release(NAME);
    st(|s| *s = Cart { rng: s.rng, own: std::mem::take(&mut s.own), ..Cart::default() });
}

// The start of a tick, before its move packet. `busy`: a hit or a slot change went already this tick.
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    let Some(m) = module() else {
        if st(|s| s.job.is_some() || s.aura.swapped.is_some() || s.cover_back.is_some()) {
            reset(game);
        }
        return;
    };
    // Refill works with the inventory open; the rest only while playing.
    if !busy && m.b("Refill") && game.player.is_some() && refill(game, &m) {
        return;
    }
    if !util::in_world(game) || game.player.as_ref().is_none_or(|p| p.health.health() <= 0.0) {
        reset(game);
        return;
    }
    st(|s| s.drawn = game.using.filter(|_| item_is(game, game.selected, |n| n == "bow")).map(|(_, t)| t));
    if busy {
        return;
    }
    let mut acted = false;
    if st(|s| s.job.is_some()) {
        acted |= run_job(game, &m);
    }
    if !acted && st(|s| s.job.is_none()) {
        acted |= start_job(game, &m);
    }
    if !acted && m.b("Auto cart") && m.b("Auto hit placed") {
        acted |= auto_hit(game, &m);
    }
    if !acted && m.b("Cover") {
        acted |= cover(game, &m);
    }
    if !acted && m.b("Aura") {
        aura(game, &m);
    } else if !m.b("Aura")
        && let Some(back) = st(|s| s.aura.swapped.take())
    {
        hold(game, back);
    }
}

// After the tick: what to draw (the boxes are cleared each tick before this).
pub(super) fn post_tick(game: &Game<'_>) {
    let Some(m) = module() else {
        return;
    };
    let mut shown = st(|s| std::mem::take(&mut s.shown));
    if m.b("Auto cart") && m.b("Render path") && m.p("Place mode") == "Trajectory" {
        let power = match game.held().map(|s| short(&s.item).to_owned()).as_deref() {
            Some("bow") => game.using.map(|(_, t)| draw_power(t)).filter(|p| *p >= 0.05),
            Some("crossbow") if charged_in_hand(game) => Some(1.05),
            _ => None,
        };
        let look = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
        if let Some(at) = power.and_then(|p| landing(game, look, p)) {
            shown.push(([at[0], at[1] + 1, at[2]], 0xFF83_76FC));
        }
    }
    for (p, c) in shown {
        util::show(p, c);
    }
}

// ---- Refill

fn refill(game: &mut Game<'_>, m: &Module) -> bool {
    // The player's own inventory only (no chest or other menu open).
    let own_menu = game.remote.is_some() && game.remote_menu().1 == "inventory";
    let open = match m.p("Refill mode") {
        "Key" => game.screen == Screen::Playing && util::setting_key_down(game, m, "Refill key"),
        _ => game.screen == Screen::Inventory,
    };
    let slot = (m.n("Refill slot") as usize).clamp(1, 9) - 1;
    if !open || !own_menu || game.ticks < st(|s| s.refill_at) || game.inventory[slot].is_some() {
        return false;
    }
    let Some(from) = (9..36).find(|&i| item_is(game, i, |n| n == "tnt_minecart")) else {
        return false;
    };
    swap_click(game, from, slot);
    let wait = ticks_of(m.n("Refill delay"), 1);
    st(|s| s.refill_at = game.ticks + wait);
    true
}

// ---- starting a job

fn start_job(game: &mut Game<'_>, m: &Module) -> bool {
    let pressed = st(|s| std::mem::take(&mut s.pressed));
    let shot = st(|s| s.pending_shot.take());
    let now = game.ticks;
    let back = game.selected;
    if (game.using.is_some() && shot.is_none()) || st(|s| s.aura.swapped.is_some()) {
        return false;
    }
    let mut job = None;
    if m.b("Auto cart") && (pressed[0] || pressed[1]) && rail_slot(game).is_some() && cart_slot(game, true).is_some() {
        let first = pressed[1] && m.n("Overload") > 1.0;
        if first && (crossbow_slot(game, false).is_some() || bow_slot(game).is_some()) {
            let mut j = Job::new(Kind::ShootFirst, Step::Weapon, None, now, back);
            j.aimed = crosshair_block(game);
            j.stack = m.n("Overload") as u32;
            job = j.aimed.map(|_| j);
        } else if !first {
            let mut j = Job::new(Kind::Trigger, Step::Rail, crosshair_base(game), now, back);
            j.stack = overload_held(game, m);
            job = j.base.map(|_| j);
        }
    } else if m.b("Xbow") && pressed[2] {
        if rail_slot(game).is_some()
            && flint_slot(game).is_some()
            && crossbow_slot(game, true).is_some()
            && cart_slot(game, true).is_some()
        {
            let j = Job::new(Kind::Xbow, Step::Rail, crosshair_base(game), now, back);
            job = j.base.map(|_| j);
        }
    } else if m.b("Auto cart")
        && m.b("On bow shot")
        && let Some((yaw, pitch, power)) = shot
    {
        job = after_shot(game, m, (yaw, pitch), power).map(|base| {
            let mut j = Job::new(Kind::AfterShot, Step::Rail, base, now, back);
            j.stack = overload_held(game, m);
            j
        });
    } else if m.b("Pearls") {
        job = pearl_job(game, m);
    }
    let Some(job) = job else {
        return false;
    };
    if !super::sunset_combat_control::claim(NAME) {
        return false;
    }
    st(|s| s.job = Some(job));
    run_job(game, m)
}

fn overload_held(game: &Game<'_>, m: &Module) -> u32 {
    if m.n("Overload") > 1.0 && util::setting_key_down(game, m, "Overload key") { m.n("Overload") as u32 } else { 1 }
}

// The block under your crosshair, if it is the top of one.
fn crosshair_base(game: &Game<'_>) -> Option<[i32; 3]> {
    let eye = game.eye_position()?;
    let hit = game.clip_ray(eye, super::view_vector(game.yaw, game.pitch), game.reach())?;
    (hit.face == [0, 1, 0]).then_some(hit.pos)
}

fn crosshair_block(game: &Game<'_>) -> Option<[i32; 3]> {
    let eye = game.eye_position()?;
    game.clip_ray(eye, super::view_vector(game.yaw, game.pitch), 128.0).map(|h| h.pos)
}

// Where a shot just sent lands, as a base for the rail (Trajectory) or the crosshair's top face (Crosshair).
fn after_shot(game: &mut Game<'_>, m: &Module, look: (f32, f32), power: f64) -> Option<Option<[i32; 3]>> {
    if rail_slot(game).is_none() || cart_slot(game, true).is_none() {
        return None;
    }
    if m.p("Place mode") == "Crosshair" {
        return Some(None);
    }
    let base = usable_base(game, landing(game, look, power)?)?;
    Some(Some(base))
}

fn usable_base(game: &Game<'_>, base: [i32; 3]) -> Option<[i32; 3]> {
    let rail = [base[0], base[1] + 1, base[2]];
    let free = !cart_in(game, rail) && !cart_in(game, base) && util::replaceable(game, rail);
    (free && in_reach(game, [f64::from(base[0]) + 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) + 0.5]))
        .then_some(base)
}

// ---- running a job

fn finish(game: &mut Game<'_>, job: &Job) {
    if job.back < 9 && game.selected != job.back {
        hold(game, job.back);
    }
    st(|s| s.job = None);
    super::sunset_combat_control::release(NAME);
}

fn give_up(game: &mut Game<'_>, job: &Job, why: &str) {
    if job.kind == Kind::Pearl {
        util::report(game, &format!("Cart: pearl skipped, {why}"));
    }
    if matches!(job.step, Step::Draw) {
        let _ = game.play_mouse(MouseButton::Right, false);
    }
    finish(game, job);
}

// One step of the job, if its wait is over. Whether it sent anything.
fn run_job(game: &mut Game<'_>, m: &Module) -> bool {
    let Some(mut job) = st(|s| s.job) else {
        return false;
    };
    let now = game.ticks;
    if now > job.started + JOB_TICKS && job.step != Step::Armed {
        give_up(game, &job, "timed out");
        return false;
    }
    if now < job.wait {
        return false;
    }
    let delay = m.n("Switch delay") as u64;
    let fov = m.n("Silent aim FOV");
    let save = |job: Job| st(|s| s.job = Some(job));
    match job.step {
        Step::Armed => {
            let Some(id) = job.pearl else {
                give_up(game, &job, "lost");
                return false;
            };
            let Some(eta) = pearl_eta(game, id) else {
                give_up(game, &job, "it landed first");
                return false;
            };
            // The steps' ticks from here to the shot: rail, the server showing it, cart, then fire and crossbow or bow
            // and draw.
            let lead = if job.crossbow { 3 * delay.max(1) + 7 } else { 2 * delay.max(1) + 24 };
            if u64::from(eta) <= lead + flight(game, &job) {
                job.step = Step::Rail;
                save(job);
            } else if now > job.started + 200 {
                give_up(game, &job, "timed out");
            }
            false
        }
        Step::Rail => {
            if job.base.is_none() {
                job.base = crosshair_base(game).and_then(|b| usable_base(game, b));
            }
            let (Some(base), Some(rail), Some(slot)) = (job.base, job.rail(), rail_slot(game)) else {
                if job.base.is_none() && now <= job.started + 10 {
                    return false;
                }
                give_up(game, &job, "no rail or no spot");
                return false;
            };
            let point = [f64::from(base[0]) + 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) + 0.5];
            if !in_reach(game, point) || !util::replaceable(game, rail) {
                give_up(game, &job, "out of reach");
                return false;
            }
            let look = game.eye_position().map(|e| look_at(e, centre(rail)));
            if !step_look(game, m, &mut job, look, fov) {
                return false;
            }
            hold(game, slot);
            place_on(game, base, 1, [0.5, 1.0, 0.5]);
            job.step = Step::Cart;
            job.wait = now + delay.max(1);
            job.look = None;
            save(job);
            true
        }
        Step::Cart => cart_step(game, m, job, now, delay, fov),
        Step::Fire => {
            let Some(rail) = job.rail() else {
                give_up(game, &job, "no rail");
                return false;
            };
            if job.fire.is_none() {
                job.fire = fire_spot(game, rail);
            }
            let (Some(fire), Some(slot)) = (job.fire, flint_slot(game)) else {
                give_up(game, &job, "no fire spot on the line");
                return false;
            };
            let below = [fire[0], fire[1] - 1, fire[2]];
            let look = game
                .eye_position()
                .map(|e| look_at(e, [f64::from(fire[0]) + 0.5, f64::from(fire[1]), f64::from(fire[2]) + 0.5]));
            if !step_look(game, m, &mut job, look, fov) {
                return false;
            }
            hold(game, slot);
            place_on(game, below, 1, [0.5, 1.0, 0.5]);
            job.step = Step::Weapon;
            job.crossbow = true;
            job.wait = now + delay.max(1);
            job.look = None;
            save(job);
            true
        }
        Step::Shield => {
            let done = shield_block(game, &job);
            job.step = Step::Back;
            job.wait = now + m.n("Bow back delay") as u64;
            save(job);
            done
        }
        Step::Weapon => {
            let slot = match job.kind {
                Kind::Xbow => crossbow_slot(game, true),
                Kind::Pearl if job.crossbow => crossbow_slot(game, true),
                Kind::Pearl => bow_slot(game),
                _ => crossbow_slot(game, false).or_else(|| bow_slot(game)),
            };
            let Some(slot) = slot else {
                give_up(game, &job, "no bow or crossbow");
                return false;
            };
            job.crossbow = item_is(game, slot, |n| n == "crossbow");
            job.step = Step::Shoot;
            job.look = None;
            save(job);
            if game.selected != slot {
                hold(game, slot);
                return true;
            }
            false
        }
        Step::Shoot => shoot_step(game, m, job, now, fov),
        Step::Draw => draw_step(game, m, job, now, fov),
        Step::Back => {
            if !m.b("Back to bow") && job.kind == Kind::AfterShot {
                st(|s| s.job = None);
                super::sunset_combat_control::release(NAME);
                return false;
            }
            finish(game, &job);
            true
        }
    }
}

// The step's look is the server's (or none is needed): go on. Else it is asked for and the step waits (up to 10 ticks,
// then goes with what the server has).
fn step_look(game: &Game<'_>, m: &Module, job: &mut Job, look: Option<(f32, f32)>, fov: f64) -> bool {
    let want = look.and_then(|l| wanted(game, m, l, fov));
    job.look = want;
    if have_look(game, want) || job.looked >= 10 {
        job.looked = 0;
        return true;
    }
    job.looked += 1;
    st(|s| s.job = Some(*job));
    false
}

fn cart_step(game: &mut Game<'_>, m: &Module, mut job: Job, now: u64, delay: u64, fov: f64) -> bool {
    let Some(rail) = job.rail() else {
        give_up(game, &job, "no rail");
        return false;
    };
    // The rail shows once the server has it.
    if !is_rail(game, rail) {
        if now > job.wait + 10 {
            give_up(game, &job, "the rail never went down");
        }
        return false;
    }
    let pull = !(m.b("Overload hotbar only") && job.carts > 0);
    let Some(slot) = cart_slot(game, pull) else {
        if !pull || (util::find_inventory(game, |n| n == "tnt_minecart").is_none() && st(|s| s.pulled.is_none())) {
            after_carts(game, m, job, now, delay);
        }
        return false;
    };
    if !in_reach(game, centre(rail)) {
        give_up(game, &job, "out of reach");
        return false;
    }
    let look = game.eye_position().map(|e| look_at(e, centre(rail)));
    if !step_look(game, m, &mut job, look, fov) {
        return false;
    }
    hold(game, slot);
    place_on(game, rail, 1, [0.5, 0.125, 0.5]);
    job.carts += 1;
    job.look = None;
    if job.carts < job.stack {
        let pause = ticks_of(roll(m.n("Overload min"), m.n("Overload max"), now), 1);
        job.wait = now + pause;
        st(|s| s.job = Some(job));
        return true;
    }
    after_carts(game, m, job, now, delay);
    true
}

fn after_carts(game: &mut Game<'_>, m: &Module, mut job: Job, now: u64, delay: u64) {
    if job.carts == 0 {
        give_up(game, &job, "no TNT carts");
        return;
    }
    job.wait = now + delay.max(1);
    job.step = match job.kind {
        Kind::Xbow => Step::Fire,
        Kind::Pearl if job.crossbow => Step::Fire,
        Kind::Pearl | Kind::Trigger => Step::Weapon,
        Kind::AfterShot | Kind::ShootFirst if m.b("Safe blocks") => Step::Shield,
        Kind::AfterShot | Kind::ShootFirst => {
            job.wait = now + m.n("Bow back delay") as u64;
            Step::Back
        }
    };
    st(|s| s.job = Some(job));
}

// Auto cart's Safe blocks: a blast-proof block on the top of the block at your crosshair when it stands between you and
// the cart, no higher than your feet, not a rail.
fn shield_block(game: &mut Game<'_>, job: &Job) -> bool {
    let (Some(slot), Some(rail), Some(feet), Some(eye)) =
        (util::find_hotbar(game, |n| SHIELDS.contains(&n)), job.rail(), util::player_pos(game), game.eye_position())
    else {
        return false;
    };
    let Some(top) = crosshair_base(game) else {
        return false;
    };
    let nearer = util::dist(feet, centre(top)) < util::dist(feet, centre(rail));
    let point = [f64::from(top[0]) + 0.5, f64::from(top[1]) + 1.0, f64::from(top[2]) + 0.5];
    if top[1] > feet[1].floor() as i32
        || !nearer
        || is_rail(game, top)
        || util::replaceable(game, top)
        || util::dist(eye, point) > game.reach()
    {
        return false;
    }
    hold(game, slot);
    place_on(game, top, 1, [0.5, 1.0, 0.5]);
    true
}

// The shot at the cart: a crossbow fires with the aimed look; a bow starts drawing with your own look (the server has
// it back by now) and aims while it draws.
fn shoot_step(game: &mut Game<'_>, m: &Module, mut job: Job, now: u64, fov: f64) -> bool {
    if job.crossbow {
        if !charged_in_hand(game) {
            give_up(game, &job, "the crossbow is not loaded");
            return false;
        }
        let Some(aim) = shot_aim_at(game, m, &job, 1.05) else {
            give_up(game, &job, "no line to the cart");
            return false;
        };
        let want = wanted(game, m, aim, fov);
        if !have_look(game, want) && job.looked < 5 {
            job.looked += 1;
            st(|s| s.job = Some(job));
            return false;
        }
        fire_crossbow(game, want.unwrap_or((game.yaw, game.pitch)));
        shot_gone(game, job, now, aim, 1.05);
        return true;
    }
    if game.using.is_none() {
        st(|s| s.own_shot = true);
        let _ = game.play_mouse(MouseButton::Right, true);
        job.step = Step::Draw;
        job.looked = 0;
        st(|s| s.job = Some(job));
        return true;
    }
    false
}

fn draw_step(game: &mut Game<'_>, m: &Module, mut job: Job, now: u64, fov: f64) -> bool {
    let Some((_, ticks)) = game.using else {
        give_up(game, &job, "the bow stopped drawing");
        return false;
    };
    let need = match job.kind {
        Kind::Pearl => 20,
        _ => (m.n("Min charge") as u32).max(3),
    };
    let power = draw_power(ticks.max(need));
    let aim = shot_aim_at(game, m, &job, power);
    let want = aim.and_then(|a| wanted(game, m, a, fov));
    let have = have_look(game, want);
    let pearl_ready = job.kind != Kind::Pearl
        || job.pearl.and_then(|id| pearl_eta(game, id)).is_none_or(|eta| u64::from(eta) <= flight(game, &job) + 1);
    let ready = ticks >= need && (pearl_ready || ticks >= 30);
    if !ready || (aim.is_some() && !have && job.looked < 10) {
        if ready {
            job.looked += 1;
        }
        st(|s| s.job = Some(job));
        return false;
    }
    if aim.is_none() && job.kind == Kind::Pearl {
        give_up(game, &job, "no line to the cart");
        return false;
    }
    st(|s| s.own_shot = true);
    let _ = game.play_mouse(MouseButton::Right, false);
    shot_gone(game, job, now, want.unwrap_or((game.yaw, game.pitch)), draw_power(ticks));
    true
}

// The look for the shot at the cart: straight with a crossbow (through the fire for Xbow and Pearls), an arc with a
// bow; ShootFirst's arc at the block it aimed at.
fn shot_aim_at(game: &Game<'_>, m: &Module, job: &Job, power: f64) -> Option<(f32, f32)> {
    if job.kind == Kind::ShootFirst {
        let aimed = job.aimed?;
        return arc_look(game, centre(aimed), power, m.n("Arc"));
    }
    let rail = job.rail()?;
    let speed = power.min(1.05) * 3.0;
    if job.crossbow || job.kind == Kind::Pearl {
        let fire = job.fire.filter(|_| job.crossbow);
        for lift in [0.0, 0.15, 0.3, 0.5, 0.8, 1.2, 1.8, 2.6] {
            let look = straight_look(game, rail, speed, lift)?;
            if reaches_cart(game, look, speed, rail, fire).is_some() {
                return Some(look);
            }
        }
        return (job.kind == Kind::Trigger).then(|| straight_look(game, rail, speed, 0.0)).flatten();
    }
    cart_arc(game, rail, power, m.n("Arc"))
}

// The shot went: ShootFirst places at its landing next; the rest are done.
fn shot_gone(game: &mut Game<'_>, mut job: Job, now: u64, look: (f32, f32), power: f64) {
    if job.kind == Kind::ShootFirst && job.carts == 0 {
        match landing(game, look, power).and_then(|b| usable_base(game, b)) {
            Some(base) => {
                job.base = Some(base);
                job.step = Step::Rail;
                job.wait = now + 1;
                job.looked = 0;
                st(|s| s.job = Some(job));
            }
            None => give_up(game, &job, "the shot lands out of reach"),
        }
        return;
    }
    if job.kind == Kind::Pearl
        && let Some(id) = job.pearl
    {
        st(|s| s.answered.push(id));
    }
    job.step = Step::Back;
    job.wait = now + 1;
    st(|s| s.job = Some(job));
}

// ---- Auto hit placed

// While you draw a bow: an arc at the cart nearest your aim, let go once it is drawn far enough and the server has the
// look.
fn auto_hit(game: &mut Game<'_>, m: &Module) -> bool {
    let Some((_, ticks)) = game.using.filter(|_| item_is(game, game.selected, |n| n == "bow")) else {
        return false;
    };
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let view = super::view_vector(game.yaw, game.pitch);
    let range = m.n("Auto hit range");
    let target = tnt_carts(game)
        .filter_map(|c| {
            let at = [c.position.x, c.position.y, c.position.z];
            let d = util::dist(eye, at);
            let to = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
            if d > range || to[0].hypot(to[2]) < 1e-6 {
                return None;
            }
            let cos = (0..3).map(|i| view[i] * to[i] / d).sum::<f64>().clamp(-1.0, 1.0);
            Some((cos.acos() * 8.0 + d * 0.05, at))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|t| t.1);
    let Some(at) = target else {
        return false;
    };
    let block = [at[0].floor() as i32, (at[1] + 0.1).floor() as i32, at[2].floor() as i32];
    let Some(aim) = cart_arc(game, block, 1.0, m.n("Arc")) else {
        return false;
    };
    let want = wanted(game, m, aim, m.n("Silent aim FOV"));
    let have = have_look(game, want);
    if f64::from(ticks) >= m.n("Auto hit charge") && have {
        st(|s| s.auto_released = true);
        let _ = game.play_mouse(MouseButton::Right, false);
        return true;
    }
    false
}

// ---- Pearls

// An enemy pearl's flight: where it first meets a block and in how many ticks.
fn pearl_path(game: &Game<'_>, id: u32) -> Option<([f64; 3], u32)> {
    super::sunset_throws::pearl_landing(game, id, 200, false).map(|f| (f.land, f.ticks))
}

fn pearl_eta(game: &Game<'_>, id: u32) -> Option<u32> {
    pearl_path(game, id).map(|p| p.1)
}

// The shot's flight to the cart (ticks), as the job will fire it.
fn flight(game: &Game<'_>, job: &Job) -> u64 {
    let Some(rail) = job.rail() else {
        return 0;
    };
    let speed = if job.crossbow { CROSSBOW_SPEED } else { 3.0 };
    [0.0, 0.15, 0.3, 0.5, 0.8, 1.2, 1.8, 2.6]
        .into_iter()
        .find_map(|lift| straight_look(game, rail, speed, lift).and_then(|l| reaches_cart(game, l, speed, rail, None)))
        .map_or(0, u64::from)
}

// A blast of `power` at `from` on something standing at `at` (before armour): the vanilla falloff, with the share of
// three points up its height the blast can see.
fn raw_damage(game: &Game<'_>, from: [f64; 3], at: [f64; 3], power: f64) -> f64 {
    let size = power * 2.0;
    let d = util::dist(from, at) / size;
    if d > 1.0 {
        return 0.0;
    }
    let seen =
        [0.2, 1.0, 1.7].into_iter().filter(|h| open_line(game, from, [at[0], at[1] + h, at[2]], [i32::MIN; 3])).count();
    let impact = (1.0 - d) * seen as f64 / 3.0;
    (impact * impact + impact) / 2.0 * 7.0 * size + 1.0
}

// What a blast of `power` at `at` does you, armour and all (None from the estimate, unknown, counts as too much). The
// shared estimate takes powers up to 8 (a crystal's 6, an anchor's 5); a TNT cart's grows with its speed (4 + 1.5 a
// block a tick, up to 11.5), so past 8 it is scaled up by how much more the bigger blast does you unshielded.
fn self_damage(
    game: &mut Game<'_>,
    at: [f64; 3],
    power: f32,
    block: Option<([i32; 3], horizon_world::registry::BlockStateId)>,
) -> f64 {
    const MOST: f32 = 8.0;
    let mut blast = Blast::explosion(at, power.min(MOST));
    if let Some((p, s)) = block {
        blast = blast.with_block(p, s);
    }
    let Some(estimate) = local_damage(game, blast, &mut Budget::new(4)).map(|p| p.estimate) else {
        return f64::INFINITY;
    };
    if power <= MOST {
        return estimate;
    }
    let feet = util::player_pos(game).unwrap_or(at);
    let distance = util::dist(feet, at);
    let size = f64::from(power) * 2.0;
    if distance >= size {
        return 0.0;
    }
    let impact = 1.0 - distance / size;
    let big = (impact * impact + impact) / 2.0 * 7.0 * size + 1.0;
    // Out of the smaller blast's reach there is nothing to scale: the unshielded damage, the most it can be.
    match super::sunset_explosive_policy::raw_damage(distance, MOST, 1.0).map(f64::from) {
        Some(small) if small > 0.0 => estimate * big / small,
        _ => big,
    }
}

// A new enemy pearl in range: the spot near its landing that hurts its thrower most (at least Pearl min damage) and you
// no more than Pearl self damage, with a line for the shot.
fn pearl_job(game: &mut Game<'_>, m: &Module) -> Option<Job> {
    let eye = game.eye_position()?;
    let range = m.n("Pearl range");
    let seen: Vec<(u32, [f64; 3])> = game
        .creatures
        .projectiles
        .iter()
        .filter(|p| p.kind == horizon_client::projectiles::ProjectileKind::EnderPearl && p.id != 0 && !p.stuck)
        .map(|p| (p.id, [p.position.x, p.position.y, p.position.z]))
        .collect();
    let candidates: Vec<u32> = st(|s| {
        s.pearls.retain(|(id, _)| seen.iter().any(|(i, _)| i == id));
        s.answered.retain(|id| seen.iter().any(|(i, _)| i == id));
        for (id, at) in &seen {
            if !s.pearls.iter().any(|(i, _)| i == id) {
                s.pearls.push((*id, *at));
            }
        }
        s.pearls
            .iter()
            .filter(|(id, first)| util::dist(*first, eye) > 4.0 && !s.answered.contains(id))
            .filter(|(id, _)| seen.iter().any(|(i, at)| i == id && util::dist(*at, eye) <= range))
            .map(|(id, _)| *id)
            .collect()
    });
    if candidates.is_empty() || rail_slot(game).is_none() || cart_slot(game, true).is_none() {
        return None;
    }
    let xbow = m.b("Prefer xbow") && crossbow_slot(game, true).is_some() && flint_slot(game).is_some();
    if !xbow && bow_slot(game).is_none() {
        return None;
    }
    for id in candidates {
        st(|s| s.answered.push(id));
        let Some((land, eta)) = pearl_path(game, id).filter(|p| p.1 <= 100) else {
            continue;
        };
        let Some(base) = pearl_spot(game, m, land, xbow) else {
            continue;
        };
        let mut job = Job::new(Kind::Pearl, Step::Armed, Some(base), game.ticks, game.selected);
        job.crossbow = xbow;
        job.pearl = Some(id);
        if flight(game, &job) == 0 && eta > 0 {
            continue;
        }
        if m.b("Render path") {
            st(|s| s.shown.push(([base[0], base[1] + 1, base[2]], 0xFFFF_5050)));
        }
        return Some(job);
    }
    None
}

fn pearl_spot(game: &mut Game<'_>, m: &Module, land: [f64; 3], xbow: bool) -> Option<[i32; 3]> {
    // The blast's expected size for the arrow that sets it off (its speed counts), and the worst one for you.
    let (expected, speed) = if xbow { (6.4, CROSSBOW_SPEED) } else { (6.25, 3.0) };
    let worst = (4.0 + 1.5 * speed) as f32;
    let at = land.map(|c| c.floor() as i32);
    let mut best: Option<([i32; 3], f64)> = None;
    let floor = m.n("Pearl min damage");
    for dx in -4..=4 {
        for dy in -1..=2 {
            for dz in -4..=4 {
                let base = [at[0] + dx, at[1] + dy, at[2] + dz];
                let rail = [base[0], base[1] + 1, base[2]];
                if util::replaceable(game, base) || !util::replaceable(game, rail) || !in_reach(game, centre(rail)) {
                    continue;
                }
                let blast = [f64::from(rail[0]) + 0.5, f64::from(rail[1]) + 0.0625, f64::from(rail[2]) + 0.5];
                let dealt = raw_damage(game, blast, land, expected);
                if dealt < best.map_or(floor, |b| b.1) {
                    continue;
                }
                if self_damage(game, blast, worst, None) > m.n("Pearl self damage") {
                    continue;
                }
                best = Some((base, dealt));
            }
        }
    }
    best.map(|b| b.0)
}

// ---- Cover

fn cover_block(game: &Game<'_>, m: &Module, swap: bool) -> Option<(usize, horizon_world::registry::BlockStateId)> {
    let wants = |n: &str, entry: &str| match entry {
        "logs" => n.ends_with("_log") || n.ends_with("_wood") || n.ends_with("_stem") || n.ends_with("_hyphae"),
        "planks" => n.ends_with("_planks"),
        e => n == e,
    };
    for entry in m.t("Cover blocks").split(',').map(|e| e.trim().trim_start_matches("minecraft:").to_ascii_lowercase())
    {
        if entry.is_empty() {
            continue;
        }
        let slot = if swap {
            util::find_hotbar(game, |n| wants(n, &entry))
        } else {
            Some(game.selected).filter(|&s| item_is(game, s, |n| wants(n, &entry)))
        };
        if let Some(slot) = slot {
            let item = game.inventory[slot].as_ref()?.item.clone();
            let state = game.items().get(&item)?.block?;
            return Some((slot, state));
        }
    }
    None
}

// Where a block for `p` can be clicked on: a neighbour's face in reach that your eyes see.
fn cover_click(game: &Game<'_>, p: [i32; 3]) -> Option<([i32; 3], i32, [f64; 3])> {
    let eye = game.eye_position()?;
    for (d, face) in [([0, -1, 0], 1), ([0, 0, -1], 3), ([1, 0, 0], 4), ([0, 0, 1], 2), ([-1, 0, 0], 5), ([0, 1, 0], 0)]
    {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if util::replaceable(game, n) {
            continue;
        }
        let c = centre(p);
        let hit = [c[0] + f64::from(d[0]) * 0.5, c[1] + f64::from(d[1]) * 0.5, c[2] + f64::from(d[2]) * 0.5];
        if util::dist(eye, hit) <= game.reach() && open_line(game, eye, hit, n) {
            return Some((n, face, hit));
        }
    }
    None
}

// Cover: the TNT carts within range that would hurt you at least Cover min damage and more than Cover max damage get
// a block between, the one that cuts the blast most (up to Max blocks a cart), Cover delay apart.
fn cover(game: &mut Game<'_>, m: &Module) -> bool {
    if let Some(back) = st(|s| s.cover_back.take()) {
        if back < 9 && game.selected != back {
            hold(game, back);
            return true;
        }
    }
    if game.using.is_some() || st(|s| s.job.is_some()) || !super::sunset_combat_control::available(NAME) {
        return false;
    }
    let swap = m.p("Cover swap");
    let Some((slot, state)) = cover_block(game, m, swap != "None") else {
        return false;
    };
    let Some(feet) = util::player_pos(game) else {
        return false;
    };
    let range = m.n("Cover range");
    let protect = m.p("Protect");
    let max_blocks = m.n("Max blocks") as u32;
    let carts: Vec<(u32, [f64; 3], [f64; 3], f64)> = tnt_carts(game)
        .filter(|c| util::dist(feet, [c.position.x, c.position.y, c.position.z]) <= range)
        .filter(|c| match protect {
            "Only self" => own_cart(c),
            "Only enemy" => !own_cart(c),
            _ => true,
        })
        .filter(|c| st(|s| s.covered.iter().find(|(id, _)| *id == c.id).map_or(0, |e| e.1)) < max_blocks)
        .map(|c| {
            let speed = (c.position.x - c.previous.x).hypot(c.position.z - c.previous.z);
            (
                c.id,
                [c.position.x, c.position.y, c.position.z],
                [c.position.x.floor(), c.position.y.floor(), c.position.z.floor()],
                speed,
            )
        })
        .collect();
    let mut threats = Vec::new();
    for (id, at, block, speed) in carts {
        let power = (4.0 + 1.5 * speed.min(5.0)) as f32;
        let dmg = self_damage(game, at, power, None);
        if dmg >= m.n("Cover min damage") && dmg > m.n("Cover max damage") {
            threats.push((id, at, block.map(|v| v as i32), power, dmg));
        }
    }
    threats.sort_by(|a, b| b.4.total_cmp(&a.4));
    let fresh = !threats.is_empty() && !st(|s| s.threatened);
    st(|s| {
        s.threatened = !threats.is_empty();
        let live: Vec<u32> = game.carts.iter().map(|c| c.id).collect();
        s.covered.retain(|(id, _)| live.contains(id));
    });
    let now = game.ticks;
    if fresh {
        let wait = ticks_of(roll(m.n("Cover delay min"), m.n("Cover delay max"), now), 0);
        st(|s| s.cover_at = now + wait);
    }
    if threats.is_empty() || now < st(|s| s.cover_at) {
        return false;
    }
    let me = util::block_pos(game).unwrap_or([0; 3]);
    // The last block placed, while the server has not shown it yet: not placed again.
    let last = st(|s| s.cover_spot).filter(|(p, at)| now < at + 20 && util::replaceable(game, *p)).map(|c| c.0);
    for (id, at, block, power, dmg) in threats {
        let spot = if m.p("Cover mode") == "Legit" {
            legit_spot(game).filter(|p| Some(*p) != last).map(|p| (p, self_damage(game, at, power, Some((p, state)))))
        } else {
            let mut best: Option<([i32; 3], f64)> = None;
            let mut around = Vec::new();
            for c in [me, [me[0], me[1] + 1, me[2]], block, [block[0], block[1] + 1, block[2]]] {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        let p = [c[0] + dx, c[1], c[2] + dz];
                        if (dx, dz) != (0, 0) && !around.contains(&p) {
                            around.push(p);
                        }
                    }
                }
            }
            for p in around {
                if Some(p) == last || !util::can_place(game, p, true) || cover_click(game, p).is_none() {
                    continue;
                }
                let d = self_damage(game, at, power, Some((p, state)));
                if best.is_none_or(|b| d < b.1) {
                    best = Some((p, d));
                }
            }
            best
        };
        let Some(p) = spot.filter(|s| s.1 < dmg - 0.5).map(|s| s.0) else {
            continue;
        };
        let Some((clicked, face, hit)) = cover_click(game, p) else {
            continue;
        };
        let look = game.eye_position().map(|e| look_at(e, hit));
        let want = if m.p("Cover mode") == "Silent" {
            look.filter(|l| f64::from(wrap(l.0 - game.yaw).abs()) <= m.n("Cover FOV"))
        } else {
            None
        };
        if !have_look(game, want) {
            return false;
        }
        let back = game.selected;
        if slot != game.selected {
            hold(game, slot);
            if swap == "Silent" {
                st(|s| s.cover_back = Some(back));
            }
        }
        let cursor = [0, 1, 2].map(|k| (hit[k] - f64::from(clicked[k])) as f32);
        place_on(game, clicked, face, cursor);
        let wait = ticks_of(roll(m.n("Cover delay min"), m.n("Cover delay max"), now), 1);
        st(|s| {
            match s.covered.iter_mut().find(|(c, _)| *c == id) {
                Some(e) => e.1 += 1,
                None => s.covered.push((id, 1)),
            }
            s.cover_spot = Some((p, now));
            s.cover_at = now + wait;
        });
        return true;
    }
    false
}

// Legit: the open block your crosshair points into (or a fire there).
fn legit_spot(game: &Game<'_>) -> Option<[i32; 3]> {
    let eye = game.eye_position()?;
    let hit = game.clip_ray(eye, super::view_vector(game.yaw, game.pitch), game.reach())?;
    let name = short(&util::facts(game, hit.pos).name);
    let p = if name == "fire" || name == "soul_fire" {
        hit.pos
    } else {
        [hit.pos[0] + hit.face[0], hit.pos[1] + hit.face[1], hit.pos[2] + hit.face[2]]
    };
    util::can_place(game, p, true).then_some(p)
}

// ---- Aura

// Aura's cart `id`, while it is someone else's and in range.
fn aura_cart<'a>(game: &'a Game<'_>, id: u32, eye: [f64; 3]) -> Option<&'a super::minecarts::Minecart> {
    tnt_carts(game).find(|c| c.id == id).filter(|c| !own_cart(c) && util::dist(eye, cart_centre(c)) <= AURA_SCAN)
}

// Aura: a TNT cart someone else put down, within reach and sight, is hit with a sword (swapped in a tick ahead), after
// a reaction time for each new one and a random gap between hits.
fn aura(game: &mut Game<'_>, m: &Module) {
    let now = game.ticks;
    if let Some(at) = st(|s| s.aura.back_at)
        && now >= at
    {
        let back = st(|s| {
            s.aura.back_at = None;
            s.aura.swapped.take()
        });
        if let Some(b) = back.filter(|&b| b < 9 && b != game.selected) {
            hold(game, b);
            super::sunset_combat_control::release(NAME);
            return;
        }
        super::sunset_combat_control::release(NAME);
    }
    if game.using.is_some() || st(|s| s.job.is_some()) {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let mut target = st(|s| s.aura.target).filter(|&id| aura_cart(game, id, eye).is_some());
    if target.is_none() {
        let view = super::view_vector(game.yaw, game.pitch);
        let half = m.n("Target FOV") / 2.0;
        target = tnt_carts(game)
            .filter(|c| !own_cart(c))
            .filter_map(|c| {
                let at = cart_centre(c);
                let d = util::dist(eye, at);
                let to = [0, 1, 2].map(|i| (at[i] - eye[i]) / d.max(1e-9));
                let angle = (0..3).map(|i| view[i] * to[i]).sum::<f64>().clamp(-1.0, 1.0).acos().to_degrees();
                (d <= AURA_SCAN && (half >= 180.0 || angle <= half)).then_some((d, c.id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|t| t.1);
        let react = target.map(|_| ticks_of(roll(m.n("Reaction min"), m.n("Reaction max"), now), 0));
        st(|s| {
            s.aura.target = target;
            if let Some(r) = react {
                s.aura.react_at = now + r;
            }
        });
    }
    let Some(id) = target else {
        st(|s| {
            if s.aura.swapped.is_some() && s.aura.back_at.is_none() {
                s.aura.back_at = Some(now + s.aura.back_delay);
            }
        });
        return;
    };
    let Some(cart) = aura_cart(game, id, eye) else {
        return;
    };
    let (lo, hi) = cart_bounds(cart);
    let mut point = cart_centre(cart);
    if m.b("Humanized aim") {
        let d = util::dist(eye, point);
        let k = (d / AURA_REACH).clamp(0.3, 1.0) * 0.12;
        for (i, v) in point.iter_mut().enumerate() {
            *v = (*v + (roll(-1.0, 1.0, now ^ id as u64 ^ i as u64)) * k).clamp(lo[i] + 0.05, hi[i] - 0.05);
        }
    }
    let look = look_at(eye, point);
    let box_ = horizon_core::geometry::Aabb::new(
        horizon_core::geometry::Vec3::new(lo[0], lo[1], lo[2]),
        horizon_core::geometry::Vec3::new(hi[0], hi[1], hi[2]),
    );
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let on_it = super::ray_box(eye, super::view_vector(have.0, have.1), AURA_REACH, box_).is_some();
    let reachable = super::ray_box(eye, super::view_vector(look.0, look.1), AURA_REACH, box_).is_some();
    if !open_line(game, eye, cart_centre(cart), [i32::MIN; 3]) || !reachable {
        return;
    }
    turn(NAME, look.0, look.1);
    if now < st(|s| s.aura.react_at) {
        return;
    }
    let held_sword = item_is(game, game.selected, |n| n.ends_with("_sword"));
    if !held_sword {
        let Some(sword) = sword_slot(game) else {
            return;
        };
        if !super::sunset_combat_control::claim(NAME) {
            return;
        }
        let back = game.selected;
        hold(game, sword);
        let wait = ticks_of(m.n("Swap delay"), 1);
        st(|s| {
            s.aura.swapped = Some(s.aura.swapped.unwrap_or(back));
            s.aura.back_at = None;
            s.aura.hit_at = now + wait;
        });
        return;
    }
    if now < st(|s| s.aura.hit_at.max(s.aura.next_hit)) || !on_it {
        return;
    }
    game.send(S::Attack(play::Attack { id: id as i32 }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    game.reset_attack_strength();
    let gap = ticks_of(roll(m.n("Attack min"), m.n("Attack max"), now), 1);
    let back_after = ticks_of(m.n("Swap back delay"), 1);
    let swap_back = m.b("Swap back");
    // The sword stays in hand while the cart is there; Swap back delay counts from when it is gone.
    st(|s| {
        s.aura.next_hit = now + gap;
        s.aura.back_delay = back_after;
        if !swap_back {
            s.aura.swapped = None;
        }
    });
    if !swap_back {
        super::sunset_combat_control::release(NAME);
    }
}
