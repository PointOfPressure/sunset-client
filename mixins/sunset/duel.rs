// Ported from Sayer's Duelkit (MIT, github.com/PointOfPressure/duelkit): Wind Jump, Hold to Eat and Pearl Catch
// (Loadouts are in loadouts.rs).
use super::sunset::{Category, Module, hold, with};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::projectiles::ProjectileKind;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use winit::event::MouseButton;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Wind Jump", Category::Movement, "Jumps with wind charges.").num("Min pitch", 60.0, 0.0, 90.0, 5.0),
        Module::new("Hold to Eat", Category::Player, "Eats from a held hotbar key.")
            .bool("Golden apples", true)
            .bool("Any food", false),
        Module::new("Pearl Catch", Category::Movement, "Catches pearls with wind charges.")
            .num("Min look up", 45.0, 0.0, 90.0, 5.0)
            .bool("Auto", true)
            .num("Max delay", 10.0, 1.0, 20.0, 1.0)
            .num("Delay", 2.0, 1.0, 20.0, 1.0)
            .bool("At feet", false),
    ]
}

// Duelkit's numbers: a pearl leaves at 1.5 blocks a tick and slows by 0.99 with 0.03 gravity; a wind charge flies
// straight at 1.5 with neither; its blast (radius 1.2) pushes with 1.22.
const THROW: f64 = 1.5;
const RADIUS: f64 = 1.2;
const KICK: f64 = 1.22;

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn pearl_step(pos: [f64; 3], vel: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let pos = add(pos, vel);
    (pos, add(scale(vel, 0.99), [0.0, -0.03, 0.0]))
}

// The earliest tick a wind charge thrown now from `from` (inheriting `inherit` a tick) meets the pearl: the direction
// to throw it and how high the kicked pearl goes.
fn intercept(from: [f64; 3], inherit: [f64; 3], pearl: [f64; 3], speed: [f64; 3]) -> Option<([f64; 3], f64)> {
    let (mut pos, mut vel) = (pearl, speed);
    for k in 1..=40 {
        (pos, vel) = pearl_step(pos, vel);
        let need = add(add(pos, scale(from, -1.0)), scale(inherit, -f64::from(k)));
        let len = need.iter().map(|v| v * v).sum::<f64>().sqrt();
        if len <= THROW * f64::from(k) && len > 0.0 {
            let dir = scale(need, 1.0 / len);
            let kicked = add(vel, scale(dir, KICK * (1.0 - 0.2 / (2.0 * RADIUS))));
            let (mut y, mut vy) = (pos[1], kicked[1]);
            while vy > 0.0 {
                y += vy;
                vy = vy * 0.99 - 0.03;
            }
            return Some((dir, y));
        }
    }
    None
}

// Your eyes, a pearl's start and speed thrown with your look and movement, and the movement a charge inherits.
fn throw_start(game: &Game<'_>) -> Option<([f64; 3], [f64; 3], [f64; 3], bool)> {
    let p = game.player.as_ref()?;
    let eye = game.eye_position()?;
    let look = super::view_vector(game.yaw, game.pitch);
    let ground = p.ground.on_ground;
    let move_ = [p.velocity.x, if ground { 0.0 } else { p.velocity.y }, p.velocity.z];
    Some((eye, add(scale(look, THROW), move_), move_, ground))
}

// Auto: the delay (ticks) whose catch sends the pearl highest, simulated for both throws; None if none catches it.
fn best_delay(game: &Game<'_>, max: u32) -> Option<u32> {
    let (eye, pearl_speed, move_, ground) = throw_start(game)?;
    let (mut me, mut mv) = (eye, move_);
    let (mut pp, mut pv) = (add(eye, [0.0, -0.1, 0.0]), pearl_speed);
    let mut best: Option<(u32, f64)> = None;
    for d in 1..=max {
        if !ground {
            me = add(me, mv);
            mv = [mv[0] * 0.91, (mv[1] - 0.08) * 0.98, mv[2] * 0.91];
        }
        (pp, pv) = pearl_step(pp, pv);
        let inherit = if ground { move_ } else { mv };
        if let Some((_, apex)) = intercept(add(me, [0.0, -0.1, 0.0]), inherit, pp, pv)
            && best.is_none_or(|b| apex > b.1)
        {
            best = Some((d, apex));
        }
    }
    best.map(|b| b.0)
}

#[derive(Default)]
struct Duel {
    // Wind Jump: the tick a wind charge went down from the ground and whether it has jumped for it, till its blast.
    thrown: Option<(u64, bool)>,
    // Wind Jump: the ticks from a throw to its blast, the last three on this connection.
    blast_after: Option<(usize, Vec<u64>)>,
    // Hold to Eat: the slot eaten from, its key, and the slot to go back to.
    eating: Option<(usize, String, usize)>,
    // Pearl Catch: the tick the wind charge goes.
    catch_at: Option<u64>,
}

static DUEL: Mutex<Option<Duel>> = Mutex::new(None);

fn duel<R>(f: impl FnOnce(&mut Duel) -> R) -> R {
    f(DUEL.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Duel::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

// The use hook: a wind charge thrown from the ground looking down far enough (Wind Jump waits for its blast).
pub(super) fn used(game: &Game<'_>, slot: usize) {
    if !game.native_window() || !game.native_on(super::sunset::ID) {
        return;
    }
    let Some(stack) = game.stack_in(slot) else {
        return;
    };
    let cooling = game.res.items().get(&stack.item).is_some_and(|i| {
        game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(stack, i.use_cooldown.as_ref()))
    });
    if let Some(m) = module("Pearl Catch")
        && stack.item.ends_with(":ender_pearl")
        && !cooling
        && -f64::from(game.pitch) >= m.n("Min look up")
    {
        let delay = if m.b("Auto") { best_delay(game, m.n("Max delay") as u32) } else { Some(m.n("Delay") as u32) };
        duel(|d| d.catch_at = delay.map(|t| game.ticks + u64::from(t)));
    }
    if stack.item.ends_with(":wind_charge")
        && !cooling
        && game.player.as_ref().is_some_and(|p| p.ground.on_ground && !p.flying)
        && game.riding.is_none()
        && module("Wind Jump").is_some_and(|m| f64::from(game.pitch) >= m.n("Min pitch"))
    {
        duel(|d| d.thrown = Some((game.ticks, false)));
    }
}

// The connection a learned blast delay holds for.
fn connection(game: &Game<'_>) -> Option<usize> {
    game.remote.as_ref().map(|r| std::sync::Arc::as_ptr(&r.session.counters) as usize)
}

// Before the tick's move: Wind Jump jumps, a real jump the server sees in the tick's PlayerInput, on the tick before
// its charge's blast, so the blast pushes you in the air and adds to the rise (a jump keeps the faster of its own speed
// and the push, so a late one is never less than the blast alone; an early one is, as you are higher when it comes).
// The tick is the shortest of the last three blasts' delays on this connection (one that came late does not make the
// next jump late), or before one is known, the tick the charge is a step from the ground. Not standing, no jump; 20
// ticks on, it waits no more.
pub(super) fn pre_input(game: &mut Game<'_>) {
    let Some((thrown, jumped)) = duel(|d| d.thrown) else {
        return;
    };
    if module("Wind Jump").is_none() || game.ticks > thrown + 20 {
        duel(|d| d.thrown = None);
        return;
    }
    let Some(p) = game.player.as_ref().filter(|p| !jumped && p.ground.on_ground && !p.flying && game.riding.is_none()) else {
        return;
    };
    let feet = p.position;
    let here = connection(game);
    let learned = duel(|d| d.blast_after.as_ref().filter(|(at, _)| Some(*at) == here).and_then(|(_, after)| after.iter().min().copied()));
    let due = match learned {
        Some(after) => game.ticks + 1 >= thrown + after,
        None => game.creatures.projectiles.iter().any(|c| {
            c.kind == ProjectileKind::WindCharge
                && (c.position.x - feet.x).hypot(c.position.z - feet.z) < 1.5
                && c.position.y + c.velocity.y <= feet.y + 0.1
        }),
    };
    if due {
        duel(|d| d.thrown = Some((thrown, true)));
        super::sunset_melee::press_jump(game);
    }
}

// The explosion hook: the blast of a charge Wind Jump waits for pushed you up; the ticks it took are kept.
pub(super) fn blast(game: &Game<'_>, push: Option<[f64; 3]>) {
    if push.is_none_or(|p| p[1] <= 0.0) {
        return;
    }
    let at = connection(game);
    duel(|d| {
        if let (Some((thrown, _)), Some(at)) = (d.thrown.take(), at) {
            let mut after = d.blast_after.take().filter(|(was, _)| *was == at).map_or_else(Vec::new, |(_, after)| after);
            if after.len() == 3 {
                after.remove(0);
            }
            after.push(game.ticks.saturating_sub(thrown));
            d.blast_after = Some((at, after));
        }
    });
}

fn edible(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    game.inventory[slot].as_ref().is_some_and(|s| {
        if s.item.ends_with(":golden_apple") || s.item.ends_with(":enchanted_golden_apple") {
            m.b("Golden apples")
        } else {
            m.b("Any food") && game.res.items().get(&s.item).is_some_and(|i| i.food.is_some())
        }
    })
}

// The key hook: Hold to Eat: the hotbar key of food pressed starts eating it (unless the Loadouts panel has the keys).
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    super::sunset_loadouts::key(game, key, pressed);
    if super::sunset_loadouts::listing() {
        return;
    }
    let Some(m) = module("Hold to Eat") else {
        return;
    };
    if !pressed || game.screen != Screen::Playing || duel(|d| d.eating.is_some()) {
        return;
    }
    let Some(slot) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if edible(game, &m, slot) {
        let back = game.selected;
        duel(|d| d.eating = Some((slot, key.to_owned(), back)));
    }
}

// Pearl Catch's wind charge: from the offhand or the hotbar (off cooldown), aimed at your pearl (Auto), straight
// down (At feet) or along your look. The use packet carries the aim; your view does not move.
fn catch(game: &mut Game<'_>, m: &Module) {
    let ready = |game: &Game<'_>, slot: usize| {
        game.stack_in(slot).is_some_and(|s| {
            s.item.ends_with(":wind_charge")
                && !game.res.items().get(&s.item).is_some_and(|i| {
                    game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(s, i.use_cooldown.as_ref()))
                })
        })
    };
    let Some(slot) = std::iter::once(util::OFFHAND).chain(0..9).find(|&s| ready(game, s)) else {
        return;
    };
    let (mut yaw, mut pitch) = (game.yaw, game.pitch);
    if m.b("At feet") {
        pitch = 90.0;
    } else if m.b("Auto")
        && let Some((eye, _, move_, _)) = throw_start(game)
        && let Some(pearl) = game
            .creatures
            .projectiles
            .iter()
            .filter(|p| p.kind == ProjectileKind::EnderPearl && util::dist(eye, [p.position.x, p.position.y, p.position.z]) < 48.0)
            .max_by_key(|p| p.id)
        && let Some((dir, _)) = intercept(
            add(eye, [0.0, -0.1, 0.0]),
            move_,
            [pearl.position.x, pearl.position.y, pearl.position.z],
            [pearl.velocity.x, pearl.velocity.y, pearl.velocity.z],
        )
    {
        yaw = (-dir[0]).atan2(dir[2]).to_degrees() as f32;
        pitch = -dir[1].asin().to_degrees() as f32;
    }
    let back = game.selected;
    if slot != util::OFFHAND {
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    game.send(S::UseItem(play::UseItem { hand: i32::from(slot == util::OFFHAND), sequence, yaw, pitch }));
    game.swing_arm();
    hold(game, back);
}

pub(super) fn tick(game: &mut Game<'_>) {
    if let Some(at) = duel(|d| d.catch_at)
        && game.ticks >= at
    {
        duel(|d| d.catch_at = None);
        if let Some(m) = module("Pearl Catch")
            && game.screen == Screen::Playing
        {
            catch(game, &m);
        }
    }
    let Some((slot, key, back)) = duel(|d| d.eating.clone()) else {
        return;
    };
    let held = game.physical_keys.iter().any(|k| horizon_client::keybinds::key_name(*k) == Some(key.as_str()));
    let go_on = module("Hold to Eat").is_some_and(|m| edible(game, &m, slot));
    if held && go_on && game.screen == Screen::Playing {
        hold(game, slot);
        if game.using.is_none() {
            let _ = game.play_mouse(MouseButton::Right, true);
        }
        return;
    }
    // Let go (or ran out): stop eating, and the slot from before comes back.
    let _ = game.play_mouse(MouseButton::Right, false);
    hold(game, back);
    duel(|d| d.eating = None);
}
