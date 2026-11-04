// Anchor Aura's Pearl catch: an enemy's ender pearl in flight gets a respawn anchor beside where it will land, put
// down, covered (Safe cover) and charged just before it lands, and set off once its thrower is there. The spot is the
// one round the landing that hurts them most (before armour) for at most Catch max self damage to you, a cover block
// between you and the anchor counted. Every click goes at the start of a tick, before its move packet, with the look
// the server already has; each step waits for the server to show the block it changed.
use super::sunset::{Module, hold, turn, with};
use super::sunset_explosive_damage::{Blast, observed_local_damage};
use super::sunset_explosive_policy::Budget;
use super::sunset_rotation::Rng;
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use horizon_world::registry::BlockStateId;
use std::sync::Mutex;

const NAME: &str = "Anchor Aura";
// The combat lease and the looks go by this name, apart from Anchor Aura's own.
const OWNER: &str = "Anchor Pearl Catch";
// A cover goes only where it saves at least this much.
const COVER_SAVES: f64 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Wait,
    Place,
    Cover,
    Charge,
    Detonate,
}

#[derive(Clone, Copy)]
struct Job {
    pearl: u32,
    thrower: u32,
    land: [f64; 3],
    anchor: [i32; 3],
    cover: Option<[i32; 3]>,
    step: Step,
    // The tick the job began and the step began; the tick the step may act; failed tries this step.
    began: u64,
    step_at: u64,
    next: u64,
    tries: u32,
    // The delays rolled for this job (ticks): after the anchor, after the cover or anchor before charging, before the
    // set-off.
    place_delay: u64,
    charge_delay: u64,
    detonate_delay: u64,
    back: usize,
}

#[derive(Default)]
struct State {
    job: Option<Job>,
    answered: Vec<u32>,
    rng: Option<Rng>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on && m.b("Pearl catch")).cloned())
}

fn roll_ticks(game: &Game<'_>, m: &Module, shortest: &str, longest: &str) -> u64 {
    let (lo, hi) = (m.n(shortest).min(m.n(longest)), m.n(shortest).max(m.n(longest)));
    let r = st(|s| s.rng.get_or_insert_with(|| Rng::new(game.ticks ^ 0x2545_F491_4F6C_DD1D)).next());
    ((lo + r * (hi - lo)) / 50.0).ceil().max(0.0) as u64
}

fn hotbar(game: &Game<'_>, name: &str) -> Option<usize> {
    util::find_hotbar(game, |n| n == name)
}

fn charges(game: &Game<'_>, p: [i32; 3]) -> Option<u8> {
    let b = game.world.block(p[0], p[1], p[2]);
    if !game.is_anchor(b) {
        return None;
    }
    game.pack.assets.registry.state(b)?.properties.get("charges")?.parse::<u8>().ok()
}

fn safe_item(m: &Module) -> &'static str {
    if m.p("Safe block") == "Obsidian" { "obsidian" } else { "glowstone" }
}

fn wrap(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn look_to(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    ((-d[0]).atan2(d[2]).to_degrees() as f32, (-d[1].atan2(d[0].hypot(d[2])).to_degrees()) as f32)
}

// A straight line between two points with nothing solid on it but `except`.
fn open_line(game: &Game<'_>, from: [f64; 3], to: [f64; 3], except: [i32; 3]) -> bool {
    let steps = (util::dist(from, to) * 8.0).ceil().max(1.0) as i32;
    (1..steps).all(|i| {
        let t = f64::from(i) / f64::from(steps);
        let at = [0, 1, 2].map(|k| (from[k] + (to[k] - from[k]) * t).floor() as i32);
        at == except || util::is_air(game, at) || !util::facts(game, at).full_collision
    })
}

// What an anchor at `p` does someone landing at `land` before armour: the blast's falloff over its 10 blocks times
// the share of three points up their height (0.2, 1.0, 1.7) it sees.
fn hurts(game: &Game<'_>, p: [i32; 3], land: [f64; 3]) -> f64 {
    let centre = util::center_of(p);
    let d = util::dist(centre, land) / 10.0;
    if d > 1.0 {
        return 0.0;
    }
    let seen = [0.2, 1.0, 1.7].iter().filter(|h| open_line(game, centre, [land[0], land[1] + **h, land[2]], p)).count();
    let k = (1.0 - d) * seen as f64 / 3.0;
    (k * k + k) / 2.0 * 7.0 * 10.0 + 1.0
}

// The face of a solid neighbour of `p` to click to put a block there, nearest your eyes and within 4.25 of them.
fn click_for(game: &Game<'_>, p: [i32; 3], eye: [f64; 3]) -> Option<([i32; 3], i32, [f64; 3])> {
    let mut best: Option<([i32; 3], i32, [f64; 3], f64)> = None;
    for (d, face) in [([0, -1, 0], 1), ([0, 1, 0], 0), ([0, 0, -1], 3), ([0, 0, 1], 2), ([-1, 0, 0], 5), ([1, 0, 0], 4)] {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if util::replaceable(game, n) {
            continue;
        }
        let c = util::center_of(p);
        let hit = [c[0] + f64::from(d[0]) * 0.5, c[1] + f64::from(d[1]) * 0.5, c[2] + f64::from(d[2]) * 0.5];
        let far = util::dist(eye, hit);
        if far <= 4.25 && best.is_none_or(|b| far < b.3) {
            best = Some((n, face, hit, far));
        }
    }
    best.map(|b| (b.0, b.1, b.2))
}

// The face of the anchor itself toward your eyes.
fn anchor_face(eye: [f64; 3], p: [i32; 3]) -> (i32, [f64; 3]) {
    let mut hit = util::center_of(p);
    let delta = [eye[0] - hit[0], eye[1] - hit[1], eye[2] - hit[2]];
    let axis = (0..3).max_by(|&a, &b| delta[a].abs().total_cmp(&delta[b].abs())).unwrap_or(1);
    let positive = delta[axis] >= 0.0;
    hit[axis] += if positive { 0.5 } else { -0.5 };
    let face = match (axis, positive) {
        (0, false) => 4,
        (0, true) => 5,
        (1, false) => 0,
        (1, true) => 1,
        (2, false) => 2,
        _ => 3,
    };
    (face, hit)
}

fn self_damage(game: &mut Game<'_>, anchor: [i32; 3], cover: Option<([i32; 3], BlockStateId)>, budget: &mut Budget) -> Option<f64> {
    let blast = match cover {
        Some((p, state)) => Blast::anchor(anchor).with_block(p, state),
        None => Blast::anchor(anchor),
    };
    observed_local_damage(game, blast, budget)
}

// The best spot round the landing: within 3 across and -1 to 2 up of it (not where they land), 4.4 of your eyes, an
// anchor there or room to put one; enough damage to them, little enough to you (a cover counted), the most damage
// less half a point a block from the landing, an anchor already there worth 3.
fn plan(game: &mut Game<'_>, m: &Module, land: [f64; 3]) -> Option<([i32; 3], Option<[i32; 3]>)> {
    let eye = game.eye_position()?;
    let health = game.player.as_ref().map(|p| f64::from(p.health.health() + p.health.absorption()))?;
    let base = land.map(|v| v.floor() as i32);
    let cover_state = hotbar(game, safe_item(m))
        .and_then(|slot| game.inventory[slot].as_ref().map(|s| s.item.clone()))
        .and_then(|item| game.items().get(&item).and_then(|i| i.block));
    let mut spots: Vec<([i32; 3], f64, bool)> = Vec::new();
    for dx in -3..=3 {
        for dy in -1..=2 {
            for dz in -3..=3 {
                let p = [base[0] + dx, base[1] + dy, base[2] + dz];
                if (dx == 0 && dz == 0 && (dy == 0 || dy == 1)) || util::dist(eye, util::center_of(p)) > 4.4 {
                    continue;
                }
                let there = charges(game, p).is_some();
                if !there && (!util::replaceable(game, p) || !util::can_place(game, p, true) || click_for(game, p, eye).is_none()) {
                    continue;
                }
                let damage = hurts(game, p, land);
                if damage >= m.n("Catch min damage") {
                    spots.push((p, damage - 0.5 * util::dist(land, util::center_of(p)) + if there { 3.0 } else { 0.0 }, there));
                }
            }
        }
    }
    spots.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut budget = Budget::new(48);
    for (p, _, _) in spots {
        let Some(open) = self_damage(game, p, None, &mut budget) else {
            continue;
        };
        if m.b("Anti suicide") && open >= health * 2.0 {
            continue;
        }
        let mut taken = (open, None);
        if m.b("Safe cover")
            && open > 6.0
            && let Some(state) = cover_state
        {
            for d in [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1], [-1, 0, -1], [-1, 0, 1], [1, 0, -1], [1, 0, 1]] {
                let c = [p[0] + d[0], p[1], p[2] + d[2]];
                if c == base || c == [base[0], base[1] + 1, base[2]] || !util::replaceable(game, c) || !util::can_place(game, c, true) || click_for(game, c, eye).is_none() {
                    continue;
                }
                if let Some(with) = self_damage(game, p, Some((c, state)), &mut budget)
                    && with < taken.0
                    && open - with >= COVER_SAVES
                {
                    taken = (with, Some(c));
                }
            }
        }
        if taken.0 <= m.n("Catch max self damage") && !(m.b("Anti suicide") && taken.0 >= health - 1.0) {
            return Some((p, taken.1));
        }
    }
    None
}

// The ticks the steps after Wait take, and so how long before the landing they start.
fn lead(job: &Job) -> u64 {
    4 + job.place_delay.max(1) + if job.cover.is_some() { 3 } else { 0 } + job.charge_delay.max(1) + 3 + 2
}

fn give_up(game: &mut Game<'_>, m: &Module) {
    if let Some(job) = st(|s| s.job.take()) {
        if m.b("Catch swap back") && job.back < 9 && game.selected != job.back && job.step != Step::Wait {
            hold(game, job.back);
        }
    }
    super::sunset::cancel_turns(OWNER);
    super::sunset_combat_control::release(OWNER);
}

pub(super) fn reset(game: &mut Game<'_>) {
    if let Some(m) = with(|s| s.module(NAME).cloned()) {
        give_up(game, &m);
    }
    st(|s| s.answered.clear());
}

// Draws the landing, the anchor and the cover while a catch runs.
pub(super) fn post_tick() {
    if let Some(job) = st(|s| s.job) {
        util::show(job.land.map(|v| v.floor() as i32), 0xFF50_AAFF);
        util::show(job.anchor, 0xFFFF_5A3C);
        if let Some(c) = job.cover {
            util::show(c, 0xFFFF_DC5A);
        }
    }
}

// A new enemy pearl within Scan range that lands within 100 ticks where an anchor can catch it starts a job.
fn find(game: &mut Game<'_>, m: &Module) -> Option<Job> {
    let eye = game.eye_position()?;
    let pearls: Vec<u32> = game
        .creatures
        .projectiles
        .iter()
        .filter(|p| p.kind == horizon_client::projectiles::ProjectileKind::EnderPearl && p.id != 0 && !p.stuck)
        .filter(|p| util::dist(eye, [p.position.x, p.position.y, p.position.z]) <= m.n("Scan range"))
        .map(|p| p.id)
        .collect();
    st(|s| s.answered.retain(|id| pearls.contains(id)));
    for id in pearls {
        if st(|s| s.answered.contains(&id)) || !super::sunset_throws::settled(game, id) {
            continue;
        }
        st(|s| s.answered.push(id));
        let Some(thrower) = super::sunset_throws::enemy_thrower(game, id) else {
            continue;
        };
        let Some(flight) = super::sunset_throws::pearl_landing(game, id, 200).filter(|f| f.ticks <= 100) else {
            continue;
        };
        let Some((anchor, cover)) = plan(game, m, flight.land) else {
            continue;
        };
        return Some(Job {
            pearl: id,
            thrower,
            land: flight.land,
            anchor,
            cover,
            step: Step::Wait,
            began: game.ticks,
            step_at: game.ticks,
            next: game.ticks,
            tries: 0,
            place_delay: roll_ticks(game, m, "Shortest place delay (ms)", "Longest place delay (ms)"),
            charge_delay: roll_ticks(game, m, "Shortest charge delay (ms)", "Longest charge delay (ms)"),
            detonate_delay: roll_ticks(game, m, "Shortest detonate delay (ms)", "Longest detonate delay (ms)"),
            back: game.selected,
        });
    }
    None
}

// The pearl's ticks left and where it lands now: (0, None) once it is gone (landed).
fn eta(game: &Game<'_>, job: &Job) -> Option<(u32, Option<[f64; 3]>)> {
    if !game.creatures.projectiles.iter().any(|p| p.id == job.pearl && !p.stuck) {
        return Some((0, None));
    }
    super::sunset_throws::pearl_landing(game, job.pearl, 200).map(|f| (f.ticks, Some(f.land)))
}

fn thrower_at(game: &Game<'_>, job: &Job) -> Option<[f64; 3]> {
    game.mobs.iter().find(|x| x.id == job.thrower && x.alive() && !x.removed).map(|x| [x.position.x, x.position.y, x.position.z])
}

impl Job {
    // A step starts, after its delay: the cover after the anchor, the charge after the anchor or cover, the set-off.
    fn begin(&mut self, step: Step, now: u64) {
        let delay = match step {
            Step::Cover => self.place_delay,
            Step::Charge => self.charge_delay,
            Step::Detonate => self.detonate_delay,
            Step::Wait | Step::Place => 0,
        };
        self.step = step;
        self.step_at = now;
        self.next = now + delay;
        self.tries = 0;
    }
}

// A click on `clicked`'s `face` at `hit` with `slot` in hand, once the server has the look (Silent aim FOV past which
// the job gives up). Some(true) clicked, Some(false) waiting, None given up.
fn click(game: &mut Game<'_>, m: &Module, slot: usize, clicked: [i32; 3], face: i32, hit: [f64; 3]) -> Option<bool> {
    let eye = game.eye_position()?;
    let look = look_to(eye, hit);
    if f64::from(wrap(look.0 - game.yaw).abs()) > m.n("Silent aim FOV") {
        return None;
    }
    let have = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    turn(OWNER, look.0, look.1);
    if wrap(have.0 - look.0).abs() >= 0.05 || (have.1 - look.1.clamp(-90.0, 90.0)).abs() >= 0.05 {
        return Some(false);
    }
    if game.selected != slot {
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    let cursor = [0, 1, 2].map(|k| (hit[k] - f64::from(clicked[k])).clamp(0.0, 1.0) as f32);
    game.send(S::UseItemOn(play::UseItemOn { hand: 0, pos: clicked, face, cursor, inside: false, world_border: false, sequence }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    Some(true)
}

// The start of a tick, before its move packet. `busy`: a hit or a slot change went already this tick.
pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    let Some(m) = module() else {
        if st(|s| s.job.is_some()) {
            reset(game);
        }
        return;
    };
    let alive = game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    if !alive || game.remote.is_none() || game.anchor_works() || game.spectator() || !util::in_world(game) {
        give_up(game, &m);
        return;
    }
    if busy || game.screen != Screen::Playing || super::sunset::gui_open() || game.using.is_some() {
        return;
    }
    let now = game.ticks;
    let Some(mut job) = st(|s| s.job).or_else(|| find(game, &m)) else {
        return;
    };
    if now > job.began + 160 || (job.step != Step::Wait && now > job.step_at + 40) || job.tries >= 3 {
        give_up(game, &m);
        return;
    }
    if job.step != Step::Wait && !super::sunset_combat_control::claim(OWNER) {
        st(|s| s.job = Some(job));
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    match job.step {
        Step::Wait => {
            // A landing that moved more than 2.5 blocks (a knock, a server's late speed) is planned again.
            if let Some((_, Some(land))) = eta(game, &job)
                && util::dist(land, job.land) > 2.5
            {
                match plan(game, &m, land) {
                    Some((anchor, cover)) => (job.land, job.anchor, job.cover) = (land, anchor, cover),
                    None => {
                        give_up(game, &m);
                        return;
                    }
                }
            }
            match eta(game, &job).map(|e| e.0) {
                None => {
                    give_up(game, &m);
                    return;
                }
                Some(0) if thrower_at(game, &job).is_none_or(|at| util::dist(at, job.land) > 3.0) => {
                    give_up(game, &m);
                    return;
                }
                Some(t) if u64::from(t) <= lead(&job) => {
                    if !super::sunset_combat_control::claim(OWNER) {
                        st(|s| s.job = Some(job));
                        return;
                    }
                    let step = match charges(game, job.anchor) {
                        None => Step::Place,
                        Some(_) if job.cover.is_some() => Step::Cover,
                        Some(0) => Step::Charge,
                        Some(_) => Step::Detonate,
                    };
                    job.begin(step, now);
                }
                Some(_) => {}
            }
        }
        _ if now < job.next => {}
        Step::Place => {
            if charges(game, job.anchor).is_some() {
                let step = if job.cover.is_some() { Step::Cover } else { Step::Charge };
                job.begin(step, now);
            } else if job.tries == 0 || now >= job.next + 4 {
                let (Some(slot), Some((clicked, face, hit))) = (hotbar(game, "respawn_anchor"), click_for(game, job.anchor, eye)) else {
                    give_up(game, &m);
                    return;
                };
                match click(game, &m, slot, clicked, face, hit) {
                    None => {
                        give_up(game, &m);
                        return;
                    }
                    Some(true) => {
                        job.tries += 1;
                        job.next = now;
                    }
                    Some(false) => {}
                }
            }
        }
        Step::Cover => {
            let cover = job.cover.unwrap_or(job.anchor);
            let slot = hotbar(game, safe_item(&m));
            if !util::replaceable(game, cover) || slot.is_none() {
                job.begin(Step::Charge, now);
            } else if job.tries == 0 || now >= job.next + 4 {
                let (Some(slot), Some((clicked, face, hit))) = (slot, click_for(game, cover, eye)) else {
                    job.begin(Step::Charge, now);
                    st(|s| s.job = Some(job));
                    return;
                };
                match click(game, &m, slot, clicked, face, hit) {
                    None => {
                        give_up(game, &m);
                        return;
                    }
                    Some(true) => {
                        job.tries += 1;
                        job.next = now;
                    }
                    Some(false) => {}
                }
            }
        }
        Step::Charge => match charges(game, job.anchor) {
            None => {
                give_up(game, &m);
                return;
            }
            Some(c) if c > 0 => job.begin(Step::Detonate, now),
            Some(_) if job.tries == 0 || now >= job.next + 4 => {
                let Some(slot) = hotbar(game, "glowstone") else {
                    give_up(game, &m);
                    return;
                };
                let (face, hit) = anchor_face(eye, job.anchor);
                match click(game, &m, slot, job.anchor, face, hit) {
                    None => {
                        give_up(game, &m);
                        return;
                    }
                    Some(true) => {
                        job.tries += 1;
                        job.next = now;
                    }
                    Some(false) => {}
                }
            }
            Some(_) => {}
        },
        Step::Detonate => {
            match charges(game, job.anchor) {
                None => {
                    give_up(game, &m);
                    return;
                }
                Some(0) => job.begin(Step::Charge, now),
                Some(_) => {
                    let landed = matches!(eta(game, &job), Some((0 | 1, _)));
                    let there = thrower_at(game, &job).is_some_and(|at| util::dist(at, util::center_of(job.anchor)) <= 4.0);
                    let hurt = game.mobs.iter().find(|x| x.id == job.thrower).is_some_and(|x| x.hurt_time > 0);
                    let health = game.player.as_ref().map_or(0.0, |p| f64::from(p.health.health() + p.health.absorption()));
                    let mut budget = Budget::new(2);
                    let own = self_damage(game, job.anchor, None, &mut budget).unwrap_or(f64::INFINITY);
                    let fuel_in_offhand = game.offhand.as_ref().is_some_and(|o| util::short(&o.item) == "glowstone");
                    if fuel_in_offhand {
                        give_up(game, &m);
                        return;
                    }
                    if landed && there && !(m.b("Damage tick") && hurt) && !(m.b("Anti suicide") && own >= health) {
                        // Anything but glowstone sets it off; a totem or a weapon rather than a block.
                        let slot = (0..9)
                            .filter(|&s| game.inventory[s].as_ref().is_none_or(|i| util::short(&i.item) != "glowstone"))
                            .min_by_key(|&s| match game.inventory[s].as_ref().map(|i| util::short(&i.item)) {
                                Some("totem_of_undying") => 0,
                                _ if s == game.selected => 1,
                                None => 2,
                                _ => 3,
                            });
                        if let Some(slot) = slot {
                            let (face, hit) = anchor_face(eye, job.anchor);
                            match click(game, &m, slot, job.anchor, face, hit) {
                                Some(false) => {}
                                _ => {
                                    give_up(game, &m);
                                    return;
                                }
                            }
                        }
                    } else {
                        // The look waits on the anchor, so the set-off needs no turn first.
                        let look = look_to(eye, anchor_face(eye, job.anchor).1);
                        turn(OWNER, look.0, look.1);
                    }
                }
            }
        }
    }
    st(|s| s.job = Some(job));
}
