// Original Crystal Aura. Only observed entity ids are attacked; a placement
// remains pending until its crystal arrives or a bounded server timeout expires.
use super::sunset::{self, Category, Module, gui_open, own_id, with};
use super::sunset_explosive_damage::{Blast, clear_line, local_damage_at, safety_damage, target_damage};
use super::sunset_explosive_policy::{Budget, Limits, in_range, safe};
use super::sunset_explosive_aura::Rating;
use super::sunset_crystal_setup_policy as setup;
use super::remote::sunset_aura_base::{self as base_native, BaseClick};
use horizon_world::registry::BlockStateId;
use std::{any::Any, sync::Arc};
type SetupScene = (i32, super::Dim, Arc<dyn Any + Send + Sync>);
use super::sunset_util::{self as util, Target};
use super::{sunset_prediction as prediction,sunset_lead_policy as lead,sunset_faceplace_policy as face};
use super::Game;
use horizon_protocol::packets::{Empty, play::{self, Clientbound as C, Serverbound as S}};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const NAME: &str = "Crystal Aura";
const MAX_ENTITIES: usize = 512;
const MAX_TARGETS: usize = 4;
const SCAN_CELLS: usize = 96;
const MAX_CANDIDATES: usize = 4;
const PREDICTIONS: usize = 24;

// BEGIN PURE CRYSTAL STATE
mod state {
    // Stop just outside the clicked top face. Ignoring the whole base would
    // incorrectly allow a ray from below to pass through the obsidian itself.
    pub(super) fn place_ray_end(top: [f64; 3]) -> [f64; 3] {
        [top[0], top[1] + 1.0e-5, top[2]]
    }
    // Unit direction, slab intersection with the actual entity bounds. Return
    // the first contact, including the eye when it is already inside the box.
    pub(super) fn ray_box(eye: [f64; 3], direction: [f64; 3], lo: [f64; 3], hi: [f64; 3], range: f64) -> Option<[f64; 3]> {
        if !eye.iter().chain(direction.iter()).chain(lo.iter()).chain(hi.iter()).chain([range].iter()).all(|v| v.is_finite())
            || range < 0.0 || (0..3).any(|i| lo[i] > hi[i]) { return None; }
        let norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm < 1.0e-12 { return None; }
        let dir = direction.map(|v| v / norm);
        let (mut near, mut far) = (0.0f64, range);
        for i in 0..3 {
            if dir[i].abs() < 1.0e-12 {
                if eye[i] < lo[i] || eye[i] > hi[i] { return None; }
            } else {
                let a = (lo[i] - eye[i]) / dir[i];
                let b = (hi[i] - eye[i]) / dir[i];
                near = near.max(a.min(b)); far = far.min(a.max(b));
                if near > far { return None; }
            }
        }
        Some(std::array::from_fn(|i| eye[i] + dir[i] * near))
    }
    pub(super) fn ray_end(eye: [f64; 3], hit: [f64; 3]) -> [f64; 3] {
        let length = (0..3).map(|i| (hit[i] - eye[i]).powi(2)).sum::<f64>().sqrt();
        if length <= 1.0e-5 { return eye; }
        std::array::from_fn(|i| hit[i] + (eye[i] - hit[i]) / length * 1.0e-5)
    }
    pub(super) fn candidate_score(damage: f64, health: f64, own: f64) -> f64 {
        if ![damage, health, own].iter().all(|v| v.is_finite() && *v >= 0.0) || health <= 0.0 { return f64::NEG_INFINITY; }
        damage.min(health) + (if damage >= health { 1000.0 } else { 0.0 }) - own * 0.5
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum Action {
        Place([i32; 3]),
        Base([i32; 3]),
        Break(u32, [f64; 3]),
    }
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Waiting { pub action: Action, pub sent: u64 }
    #[derive(Default)]
    pub(super) struct Clock {
        pub cursor: usize,
        pub crystals: usize,
        pub queued: bool,
        pub waiting: Option<Waiting>,
        pub next: u64,
        pub next_base: u64,
        pub next_place: u64,
        pub next_break: u64,
        pub generation: u64,
    }
    impl Clock {
        pub fn suspend(&mut self) {
            self.queued = false;
            self.generation = self.generation.wrapping_add(1);
        }
        pub fn reset(&mut self) {
            let generation = self.generation.wrapping_add(1);
            *self = Self { generation, ..Self::default() };
        }
        pub fn sent(&mut self, action: Action, tick: u64, delay: u64) {
            self.queued = false;
            self.waiting = Some(Waiting { action, sent: tick });
            self.next = tick.saturating_add(delay.max(2));
            match action { Action::Base(_) => self.next_base = self.next, Action::Place(_) => self.next_place = self.next,
                Action::Break(_, _) => self.next_break = self.next }
        }
        pub fn observe(&mut self, tick: u64, acknowledged: bool) {
            if self.waiting.is_some_and(|w| acknowledged || tick.saturating_sub(w.sent) >= 30) {
                self.waiting = None;
            }
        }
        #[cfg(test)]
        pub fn ready(&self, tick: u64) -> bool {
            !self.queued && self.waiting.is_none() && tick >= self.next
        }
        pub fn available(&self) -> bool { !self.queued && self.waiting.is_none() }
        pub fn due(&self, action: Action, tick: u64) -> bool {
            self.available() && tick >= match action { Action::Base(_) => self.next_base, Action::Place(_) => self.next_place, Action::Break(_, _) => self.next_break }
        }
        pub fn cell(&mut self, radius: i32) -> [i32; 3] {
            let side = (radius * 2 + 1) as usize;
            let n = self.cursor % (side * side * side);
            self.cursor = (n + 1) % (side * side * side);
            [(n % side) as i32 - radius, ((n / side) % side) as i32 - radius,
                (n / (side * side)) as i32 - radius]
        }
    }
}
// END PURE CRYSTAL STATE
use state::{Action, Clock};

struct Plan { action: Action, target: u32, aim: [f64; 3], rating: Rating, click: Option<BaseClick> }
struct WeaponLease { scene:SetupScene, original:usize, owned:usize }
#[derive(Default)]
struct Diagnostics { stage: usize, reason: u8, paused: bool, counters: [u64; 18], crystals: usize, cid: u32,
    invulnerable: bool, own: f64, target: f64, health: f64 }
struct State { clock: Clock, queued_at: Option<Instant>, tick: u64, diagnostics: Diagnostics,
    plan: Option<Plan>, attempts: Vec<(u32, u8)>, setup: setup::Ledger, setup_scene: Option<SetupScene>, weapon:Option<WeaponLease> }
static STATE: Mutex<State> = Mutex::new(State { clock: Clock { cursor: 0, crystals: 0, queued: false,
    waiting: None, next: 0, next_base: 0, next_place: 0, next_break: 0, generation: 0 }, queued_at: None, tick: 0,
    diagnostics: Diagnostics { stage: 0, reason: 0, paused: false, counters: [0;18], crystals: 0, cid: 0,
        invulnerable: false, own: 0.0, target: 0.0, health: 0.0 }, plan: None, attempts: Vec::new(), setup: setup::Ledger::new(), setup_scene: None,weapon:None });
fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
fn note(stage: usize) { st(|s| { s.diagnostics.paused = stage == 1;
    if stage != 1 { s.diagnostics.stage = stage; }
    s.diagnostics.counters[stage] = s.diagnostics.counters[stage].saturating_add(1); }); }
fn execution_failed(reason: u8) { st(|s| s.diagnostics.reason = reason); note(9); }
fn policy_failed(reason: u8) { st(|s| s.diagnostics.reason = reason); note(6); }
fn rank(rating: &Rating) -> f64 { state::candidate_score(rating.target_damage, rating.target_health, rating.self_damage) }
// Formatting occurs only when the explicit diagnostic command calls this.
pub(super) fn diagnostics() -> String {
    st(|s| format!("Crystal stage={} reason={} paused={} ticks={} crystals={} cid={} invulnerable={} own={:.3} target={:.3} health={:.3} queued={} waiting={} attempts={} counters={:?}",
        s.diagnostics.stage, s.diagnostics.reason, s.diagnostics.paused, s.tick, s.diagnostics.crystals, s.diagnostics.cid,
        s.diagnostics.invulnerable, s.diagnostics.own, s.diagnostics.target, s.diagnostics.health,
        s.clock.queued, s.clock.waiting.is_some(), s.attempts.len(), s.diagnostics.counters))
}

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(NAME, Category::Combat,
        "Places and breaks observed crystals with bounded damage checks. Target damage is estimated; self checks use the selected damage model, friends use unmitigated damage. Protect loot pauses module-planned blasts near an observed player death for 30 seconds; it does not track dropped items.")
        .text("Entities", "player")
        .bool("Place", true).bool("Break", true)
        .bool("Auto obsidian", true).num("Obsidian delay", 0.0, 0.0, 30.0, 1.0)
        .num("Target range", 8.0, 1.0, 12.0, 0.5)
        .num("Place range", 4.5, 1.0, 4.5, 0.1)
        .num("Break range", 3.0, 1.0, 3.0, 0.1)
        .num("Walls range", 0.0, 0.0, 4.5, 0.1)
        .num("Min target damage", 4.0, 0.0, 20.0, 0.5)
        .bool("Face Place", true).text("Force Face Place", "None")
        .bool("Require Hold", false).text("Hold Key", "mouse4")
        .bool("Switch Back", true)
        .num("Turn Speed", 4.5, 1.0, 10.0, 0.5)
        .num("View Angle", 90.0, 30.0, 180.0, 5.0)
        .bool("Human Pace", true).bool("Render",true).text("Render Color","#FFD65CFF")
        .num("Max self damage", 8.0, 0.0, 20.0, 0.5)
        .pick("Damage model", &["Observed equipment", "Unmitigated"])
        .bool("Protect loot", true)
        .num("Max friend damage", 0.0, 0.0, 36.0, 0.5)
        .bool("Anti suicide", true)
        .num("Min remaining health", 4.0, 1.0, 20.0, 0.5)
        .num("Place delay", 0.0, 0.0, 10.0, 1.0)
        .num("Break delay", 0.0, 0.0, 10.0, 1.0)
        .bool("Anti bot", true).bool("Ignore teams", true)
        .bool("Auto switch", true).bool("Rotate", true).bool("Swing", true)]
}

// A temporary menu, manual use or competing action invalidates queued work,
// but must not replenish an unchanged setup cell's retry budget or lose its
// authoritative acknowledgement. Only a module/scene reset clears history.
pub(super) fn suspend() {
    super::sunset_combat_control::release(NAME);
    sunset::cancel_turns(NAME);
    st(|s| { s.clock.suspend(); s.queued_at = None; s.plan = None; });
    note(1);
}
pub(super) fn reset() {
    super::sunset_explosive_aura::clear_view(false);
    prediction::reset_receipts();
    suspend();
    st(|s| { s.clock.reset(); s.attempts.clear(); s.setup.clear(); s.setup_scene = None;s.weapon=None; });
}
fn restore_weapon(game:&mut Game<'_>,requested:bool) {
    let Some(lease)=st(|s|s.weapon.take()) else{return;};
    if requested && same_setup_scene(game,&lease.scene) && game.using.is_none()
        && !super::sunset_player::gap_eating()
        && game.player.as_ref().is_some_and(|p|p.health.health().is_finite() && p.health.health()>0.0)
        && super::sunset_combat_control::owner().is_none_or(|owner|owner==NAME) {
        if let Some(slot)=super::sunset_macro_policy::restore(lease.original,lease.owned,game.selected,true){weapon_hold(game,slot);}
    }
}
fn weapon_hold(game:&mut Game<'_>,slot:usize) {
    sunset::hold(game,slot);super::sunset_combat_macros::note_main(game);
}
pub(super) fn paused(game:&mut Game<'_>) {
    let requested=with(|s|s.module(NAME).is_none_or(|m|m.b("Switch Back")));
    restore_weapon(game,requested);suspend();
}
pub(super) fn disabled(game:&mut Game<'_>) {
    let requested=with(|s|s.module(NAME).is_none_or(|m|m.b("Switch Back")));
    restore_weapon(game,requested);reset();
}

fn allowed(game: &Game<'_>) -> bool {
    super::sunset_explosive_aura::can_act(game)
        && util::in_world(game) && !gui_open() && !game.spectator() && !game.creative
        && game.using.is_none() && game.mining.is_none()
        && util::inventory_idle(game)
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && game.mobs.len() <= MAX_ENTITIES && game.crystals.len() <= MAX_ENTITIES
}

// Refuse nameless players: identity must be known before deciding friendship.
// Scan the complete bounded list, so omitted friends cannot silently disappear.
fn players(game: &Game<'_>, m: &Module) -> Option<(Vec<Target>, Vec<Target>)> {
    let eye = game.eye_position()?;
    let own = own_id(game);
    let friends = with(|s| s.friends.clone());
    let mut targets = Vec::new();
    let mut protected = Vec::new();
    for mob in &game.mobs {
        if mob.removed || !mob.alive() || Some(mob.id) == own { continue; }
        let target = util::mob_target(mob);
        if !target.pos.iter().chain(target.lo.iter()).chain(target.hi.iter()).all(|v| v.is_finite()) { return None; }
        let friend = if target.player {
            let name = game.remote_player_name(mob.id)?;
            friends.iter().any(|f| f.eq_ignore_ascii_case(name))
        } else { false };
        if friend || util::spared(game, m, target.id, target.player) {
            protected.push(target);
        } else if util::player_enemy(game,mob)
            && in_range(eye, target.pos, m.n("Target range").clamp(1.0, 12.0)) {
            targets.push(target);
        }
    }
    targets.sort_unstable_by(|a, b| util::dist(eye, a.pos).total_cmp(&util::dist(eye, b.pos)));
    if let Some(steer)=super::sunset_crystal_bot::steer(game) {targets.sort_by_key(|t|t.id!=steer.target);}
    targets.truncate(MAX_TARGETS);
    Some((targets, protected))
}

fn limits(game:&Game<'_>,m: &Module) -> Limits {
    let steering=super::sunset_crystal_bot::steer(game);
    Limits { min_target: steering.map_or_else(||m.n("Min target damage"),|s|s.min), max_self: steering.map_or_else(||m.n("Max self damage"),|s|s.self_cap),
        max_friend: m.n("Max friend damage"), min_remaining: m.n("Min remaining health"),
        anti_suicide: m.b("Anti suicide") }
}

fn valid_damage(game: &mut Game<'_>, m: &Module, at: [f64; 3], targets: &[Target],
    friends: &[Target], action: Action, budget: &mut Budget) -> Option<(Rating, u32)>
{
    // Horizon destroys explosion-damaged neighboring crystals without a
    // second blast (destroy_crystal(..., CrystalDamage::Explosion)).
    let blast = match action {
        Action::Base(base)=>Blast::explosion(at,6.0).with_block(base,game.items().get("minecraft:obsidian")?.block?),
        _=>Blast::explosion(at,6.0),
    };
    let breaking=matches!(action,Action::Break(_, _));
    let horizon=lead::horizons(if breaking {lead::Use::CrystalBreak}else{lead::Use::CrystalPlace},prediction::rtt(game));
    if !super::sunset_loot_guard::allows(game, blast, m.b("Protect loot")) { policy_failed(8); return None; }
    let local_offset=prediction::local_offset(game,horizon.local)?;
    let Some(prediction) = local_damage_at(game, blast, local_offset, budget) else { policy_failed(1); return None; };
    let own = safety_damage(prediction, m.p("Damage model"));
    let p = game.player.as_ref()?;
    let health = f64::from(p.health.health() + p.health.absorption());
    st(|s| { s.diagnostics.own = own; s.diagnostics.health = health; });
    let l = limits(game,m);
    if own > l.max_self { policy_failed(2); return None; }
    if l.anti_suicide && (health <= own || health - own < l.min_remaining) { policy_failed(3); return None; }
    let mut friend: f64 = 0.0;
    for target in friends {
        // The engine rejects entities by feet distance before exposure. Keep
        // distant friends from exhausting the prediction budget with zeros.
        let projected=prediction::target(game,target,horizon.others)?;
        if !in_range(at, target.pos, 12.0) && !in_range(at,projected.pos,12.0) { continue; }
        let Some(prediction) = target_damage(game, &projected, blast, budget) else { policy_failed(4); return None; };
        friend = friend.max(prediction.upper);
        if friend > l.max_friend { policy_failed(5); return None; }
    }
    let mut best: Option<(Rating, u32)> = None;
    for target in targets {
        let projected=prediction::target(game,target,horizon.others)?;
        let Some((damage,_))=prediction::estimate(game,&projected,blast,
            if breaking {0}else{prediction::rtt(game)},prediction::rtt(game),budget) else {policy_failed(6);return None;};
        let facts=prediction::face_facts(game,target);
        let forced=util::setting_key_down(game,m,"Force Face Place");
        let threshold=face::crystal(l.min_target,m.b("Face Place"),forced,breaking,&facts)?.minimum;
        if !face::sufficient(damage,threshold,facts.effective_health){continue;}
        let mut l=l;l.min_target=0.0;
        st(|s| s.diagnostics.target = damage);
        let rating = Rating { target_damage: damage, target_health: prediction::effective_health(game,target).unwrap_or(f64::from(target.health)),
            self_damage: own, attack: matches!(action, Action::Break(_, _)) };
        if safe(l, damage, own, health, friend) && best.as_ref().is_none_or(|(old, _)| rank(&rating) > rank(old)) {
            best = Some((rating, target.id));
        }
    }
    if best.is_none() { policy_failed(7); }
    best
}

fn center(base: [i32; 3]) -> [f64; 3] {
    [f64::from(base[0]) + 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) + 0.5]
}
fn obsidian(game: &Game<'_>, base: [i32; 3]) -> bool {
    &*util::facts(game, base).name == "minecraft:obsidian"
}
fn place_valid(game: &Game<'_>, base: [i32; 3]) -> bool {
    matches!(&*util::facts(game, base).name, "minecraft:obsidian" | "minecraft:bedrock")
        && st(|s| s.setup.crystal_allowed(base, s.tick, obsidian(game, base))) && crystal_column(game, base)
}
fn setup_valid(game: &Game<'_>, base: [i32; 3]) -> bool {
    let f = util::facts(game, base);
    f.replaceable && f.fluid == super::Fluid::None && f.fluid_level == 0 && !f.waterlogged && util::can_place(game, base, true)
        && crystal_column(game, base)
}
fn crystal_column(game: &Game<'_>, base: [i32; 3]) -> bool {
    if !game.world.loaded(base[0], base[2]) || base[1] < -64 || base[1] >= 318 { return false; }
    if !util::is_air(game, [base[0], base[1] + 1, base[2]]) { return false; }
    let at = center(base);
    let lo = [at[0] - 0.5, at[1], at[2] - 0.5];
    let hi = [lo[0] + 1.0, lo[1] + 2.0, lo[2] + 1.0];
    let hits = |a: horizon_core::geometry::Aabb| {
        let (a, b) = (a.min_position(), a.max_position());
        a.x < hi[0] && b.x > lo[0] && a.y < hi[1] && b.y > lo[1] && a.z < hi[2] && b.z > lo[2]
    };
    if game.player.as_ref().is_some_and(|p| hits(p.bounds()))
        || game.mobs.iter().any(|mob| !mob.removed && hits(mob.bounds())) { return false; }
    !game.crystals.iter().any(|c| (c.position.x - at[0]).abs() < 1.5
        && c.position.y < hi[1] && c.position.y + 2.0 > lo[1] && (c.position.z - at[2]).abs() < 1.5)
}

fn hit_visible(game: &Game<'_>, m: &Module, eye: [f64; 3], hit: [f64; 3], ray_end: [f64; 3], range: f64, budget: &mut Budget) -> bool {
    let range = range.clamp(1.0, 4.5);
    in_range(eye, hit, range) && (in_range(eye, hit, m.n("Walls range").clamp(0.0, range))
        || clear_line(game, eye, ray_end, None, budget))
}
fn break_contact(game: &Game<'_>, m: &Module, at: [f64; 3], aim: [f64; 3], budget: &mut Budget) -> Option<[f64; 3]> {
    let eye = game.eye_position()?;
    let lo = [at[0] - 1.0, at[1], at[2] - 1.0];
    let hi = [at[0] + 1.0, at[1] + 2.0, at[2] + 1.0];
    let direction = std::array::from_fn(|i| aim[i] - eye[i]);
    let hit = state::ray_box(eye, direction, lo, hi, m.n("Break range").clamp(1.0, 4.5))?;
    hit_visible(game, m, eye, hit, state::ray_end(eye, hit), m.n("Break range"), budget).then_some(hit)
}
fn reachable(game: &Game<'_>, m: &Module, action: Action, budget: &mut Budget) -> Option<[f64; 3]> {
    let eye = game.eye_position()?;
    match action {
        Action::Base(_) => None,
        Action::Place(base) => {
            let top = center(base);
            hit_visible(game, m, eye, top, state::place_ray_end(top), m.n("Place range"), budget).then_some(top)
        }
        Action::Break(_, at) => {
            let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
            let dir = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
            let current = std::array::from_fn(|i| eye[i] + dir[i] * 5.0);
            if let Some(hit) = break_contact(game, m, at, current, budget) {
                return Some(std::array::from_fn(|i| hit[i] + dir[i] * 0.01));
            }
            if !m.b("Rotate") { return None; }
            let nearest = [eye[0].clamp(at[0] - 0.9, at[0] + 0.9), eye[1].clamp(at[1] + 0.1, at[1] + 1.9),
                eye[2].clamp(at[2] - 0.9, at[2] + 0.9)];
            for aim in [nearest, [at[0], at[1] + 1.0, at[2]]] {
                if break_contact(game, m, at, aim, budget).is_some() { return Some(aim); }
            }
            None
        }
    }
}

fn crystal_slot(game: &Game<'_>, m: &Module) -> Option<usize> { item_slot(game, m, "end_crystal") }
fn obsidian_slot(game: &Game<'_>, m: &Module) -> Option<usize> { item_slot(game, m, "obsidian") }
fn item_slot(game: &Game<'_>, m: &Module, wanted: &str) -> Option<usize> {
    let slot = util::find_held(game, |name| name == wanted)?;
    let stack = if slot == util::OFFHAND { game.offhand.as_ref() }
        else { game.inventory.get(slot).and_then(|s| s.as_ref()) }?;
    if stack.count == 0 { return None; }
    (slot == util::OFFHAND || slot == game.selected || m.b("Auto switch")).then_some(slot)
}

fn observed(game: &Game<'_>, action: Action) -> bool {
    match action {
        Action::Base(pos) => st(|s| s.setup.acknowledged(pos, obsidian(game, pos))),
        Action::Break(id, _) => !game.crystals.iter().any(|c| c.id == id),
        Action::Place(base) => {
            let at = center(base);
            game.crystals.iter().any(|c| in_range(at, [c.position.x, c.position.y, c.position.z], 0.25))
        }
    }
}

fn same_setup_scene(game: &Game<'_>, old: &SetupScene) -> bool {
    game.remote.as_ref().is_some_and(|r| r.entity_id == old.0 && game.dim == old.1
        && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ())
}
fn setup_payload(game: &Game<'_>, pos: [i32; 3], id: u32) {
    let id = BlockStateId(id);
    if game.pack.assets.registry.state(id).is_none() { return; }
    let is_obsidian = &*game.res.facts().get(id).name == "minecraft:obsidian";
    st(|s| s.setup.payload(pos, id.0, is_obsidian));
}
// Called from the accepted native packet observer; never from prediction/ACK.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::Disconnect(_)) { reset(); return; }
    if !matches!(packet, C::BlockUpdate(_) | C::SectionBlocksUpdate(_)) { return; }
    if !st(|s| s.setup.len() != 0 && s.setup_scene.as_ref().is_some_and(|old| same_setup_scene(game, old))) { return; }
    match packet {
        C::BlockUpdate(p) => if let Ok(id) = u32::try_from(p.state) { setup_payload(game, p.pos, id); },
        C::SectionBlocksUpdate(p) => for change in p.changes.iter().take(4096) {
            if let Some((pos, id)) = setup::unpack_section(p.section, *change) { setup_payload(game, pos, id); }
        },
        _ => {}
    }
}

pub(super) fn prepare(game: &mut Game<'_>, m: Option<&Module>, ticks: u64) -> Option<Rating> {
    let Some(m) = m.filter(|m| m.on) else { disabled(game); return None; };
    if st(|s| s.setup_scene.as_ref().is_some_and(|old| !same_setup_scene(game, old))) { reset(); }
    if !allowed(game) || m.b("Require Hold") && !util::setting_key_down(game,m,"Hold Key") { paused(game); return None; }
    if st(|s|s.weapon.as_ref().is_some_and(|lease|!same_setup_scene(game,&lease.scene) || lease.owned!=game.selected)) {
        st(|s|s.weapon=None);suspend();return None;
    }
    super::sunset_combat_macros::note_main(game);
    st(|s| { s.setup_scene = game.remote.as_ref().map(|r|
        (r.entity_id, game.dim, r.session.counters.clone() as Arc<dyn Any + Send + Sync>)); });
    st(|s| { s.tick = ticks; s.plan = None; s.diagnostics.crystals = game.crystals.len();
        s.attempts.retain(|(id, _)| game.crystals.iter().any(|c| c.id == *id)); });
    // Expired queued closures are canceled instead of building a backlog.
    if st(|s| s.queued_at.is_some_and(|t| t.elapsed() > Duration::from_millis(200))) { suspend(); }
    let waiting = st(|s| s.clock.waiting);
    if let Some(waiting) = waiting {
        let ack = observed(game, waiting.action) || ticks.saturating_sub(waiting.sent)>=u64::from(prediction::rtt(game).saturating_add(2).clamp(4,20));
        st(|s| s.clock.observe(ticks, ack));
    }
    if !st(|s| s.clock.available()) { note(2); return None; }
    let Some((targets, friends)) = players(game, m).filter(|(t, _)| !t.is_empty()) else {
        restore_weapon(game,m.b("Switch Back"));note(11);return None;
    };
    let mut budget = Budget::new(PREDICTIONS);
    let mut best: Option<Plan> = None;
    let mut evaluated = 0;
    let mut setup_cells = Vec::new();
    note(3);
    if m.b("Break") {
        let start = st(|s| s.clock.crystals);
        let count = game.crystals.len();
        let mut shortlist = Vec::with_capacity(count.min(64));
        for n in 0..count.min(64) {
            let c = &game.crystals[(start + n) % count];
            st(|s| { s.diagnostics.cid = c.id; s.diagnostics.invulnerable = c.invulnerable; });
            if c.id == 0 || c.invulnerable { note(4); continue; }
            if st(|s| s.attempts.iter().any(|&(id, attempts)| id == c.id && attempts >= 3)) { note(14); continue; }
            let at = [c.position.x, c.position.y, c.position.z];
            if super::sunset_crystal_bot::steer(game).is_some_and(|s|at[1]<f64::from(s.floor)-0.01) {continue;}
            if super::sunset_crystal_optimizer::hidden_position(game,at) {continue;}
            let action = Action::Break(c.id, at);
            if !st(|s| s.clock.due(action, ticks)) { note(12); continue; }
            let Some(aim) = reachable(game, m, action, &mut budget) else { note(5); continue; };
            let distance = targets.iter().map(|target| util::dist(at, target.pos)).fold(f64::INFINITY, f64::min);
            shortlist.push((distance, action, aim));
        }
        // Cheap distance bounds precede expensive exposure rays. All observed
        // candidates in the bounded slice compete for the four evaluations.
        shortlist.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for (_, action, aim) in shortlist.into_iter().take(MAX_CANDIDATES) {
            let Action::Break(id, at) = action else { continue; };
            st(|s| s.diagnostics.cid = id);
            evaluated += 1;
            if let Some((rating, target)) = valid_damage(game, m, at, &targets, &friends, action, &mut budget) {
                if best.as_ref().is_none_or(|old| rank(&rating) > rank(&old.rating)) {
                    best = Some(Plan { rating, target, action, aim, click: None });
                }
            } else { note(6); }
        }
        st(|s| s.clock.crystals = if count == 0 { 0 } else { (start + 64) % count });
    }
    // Breaking takes priority, and both phases share one prediction budget.
    if best.is_none() && evaluated < MAX_CANDIDATES && m.b("Place") && crystal_slot(game, m).is_some() {
        let Some(origin) = util::block_pos(game) else { return None; };
        for _ in 0..SCAN_CELLS {
            let d = st(|s| s.clock.cell(5));
            let base = [origin[0].saturating_add(d[0]), origin[1].saturating_add(d[1]), origin[2].saturating_add(d[2])];
            if super::sunset_crystal_bot::steer(game).is_some_and(|s|base[1]+1<s.floor) {continue;}
            let action = Action::Place(base);
            if !st(|s| s.clock.due(action, ticks)) { note(12); break; }
            if !place_valid(game, base) {
                if m.b("Auto obsidian") && setup_valid(game, base) && st(|s| s.setup.can_send(base, ticks)) {
                    setup_cells.push(base);
                }
                continue;
            }
            let Some(aim) = reachable(game, m, action, &mut budget) else { note(5); continue; };
            evaluated += 1;
            if let Some((rating, target)) = valid_damage(game, m, center(base), &targets, &friends, action, &mut budget) {
                if best.as_ref().is_none_or(|old| rank(&rating) > rank(&old.rating)) {
                    best = Some(Plan { rating, target, action, aim, click: None });
                }
            } else { note(6); }
            if evaluated >= MAX_CANDIDATES { break; }
        }
    }
    // Optional fallback only: never compete with a ready break/existing base.
    // Reuse the same four prediction evaluations and bounded scan allowance.
    let eager=super::sunset_crystal_bot::steer(game).is_some_and(|s|s.eager);
    let may_setup=best.as_ref().is_none_or(|plan|matches!(plan.action,Action::Place(_))
        && plan.rating.target_damage<if eager {11.0}else{8.0});
    if may_setup && evaluated < MAX_CANDIDATES && m.b("Place") && m.b("Auto obsidian")
        && crystal_slot(game, m).is_some() {
        let slot = obsidian_slot(game, m)?;
        let mut rays = 48usize;
        for base in setup_cells {
            let action = Action::Base(base);
            if !st(|s| s.clock.due(action, ticks)) { break; }
            if !setup_valid(game, base) || !st(|s| s.setup.can_send(base, ticks)) { continue; }
            let Some(click) = base_native::plan(game, base, slot, m.n("Place range").clamp(1.0,4.5), &mut rays) else {
                if rays == 0 { break; } else { continue; }
            };
            evaluated += 1;
            if let Some((rating, target)) = valid_damage(game, m, center(base), &targets, &friends, action, &mut budget) {
                if best.as_ref().is_none_or(|old| setup::upgrade(old.rating.target_damage,rating.target_damage,eager)) {
                    best = Some(Plan { rating, target, action, aim: click.point, click: Some(click) });
                }
            }
            if evaluated >= MAX_CANDIDATES || rays == 0 { break; }
        }
    }
    let Some(best)=best else {restore_weapon(game,m.b("Switch Back"));return None;};
    let rating = best.rating;
    let cell=match best.action {Action::Place(p)|Action::Base(p)=>p,Action::Break(_,p)=>[p[0].floor() as i32,p[1].floor() as i32-1,p[2].floor() as i32]};
    super::sunset_explosive_aura::show(game,false,cell);
    st(|s| s.plan = Some(best));
    note(16);
    Some(rating)
}

pub(super) fn execute(game: &mut Game<'_>, m: &Module, ticks: u64) -> bool {
    let Some(plan) = st(|s| s.plan.take()) else { return false; };
    if !m.on || !allowed(game) || !st(|s| s.clock.due(plan.action, ticks)) { execution_failed(2); return false; }
    queue(game, m, plan.action, plan.target, plan.aim, plan.click, ticks)
}

fn queue(game: &mut Game<'_>, m: &Module, action: Action, target: u32, aim: [f64; 3], click: Option<BaseClick>, ticks: u64) -> bool {
    let Some(eye) = game.eye_position() else { return false; };
    let desired=sunset::look_at(eye,aim);
    let turn=if m.b("Rotate") {super::sunset_aura::next_look(game,desired,if super::sunset_crystal_bot::steer(game).is_some_and(|s|s.full_speed){10.0}else{m.n("Turn Speed")})} else {(game.yaw,game.pitch)};
    let angle=f64::from((desired.0-game.yaw+180.0).rem_euclid(360.0)-180.0).abs();
    let held=super::sunset_combat_wire::sent_look(game).is_some_and(|look| match action {
        Action::Break(_,at)=>{
            let y=f64::from(look.0).to_radians();let p=f64::from(look.1).to_radians();
            break_contact(game,m,at,[eye[0]-y.sin()*p.cos()*5.0,eye[1]-p.sin()*5.0,eye[2]+y.cos()*p.cos()*5.0],&mut Budget::new(0)).is_some()
        },
        Action::Base(_)=>click.is_some_and(|c|util::block_contact(game,look,c.clicked,base_native::face(c),m.n("Place range")).is_some()),
        Action::Place(base)=>util::block_contact(game,look,base,1,m.n("Place range")).is_some(),
    });
    if m.n("View Angle")<180.0 && angle>m.n("View Angle")+if held {20.0}else{0.0} {return false;}
    let mut click=click;
    let contact=match action {
        Action::Base(_)=>click.and_then(|mut c| {c.point=util::block_contact(game,turn,c.clicked,base_native::face(c),m.n("Place range"))?;click=Some(c);Some(c.point)}),
        Action::Place(base)=>util::block_contact(game,turn,base,1,m.n("Place range")),
        Action::Break(_,at)=>{
            let y=f64::from(turn.0).to_radians();let p=f64::from(turn.1).to_radians();
            let along=[eye[0]-y.sin()*p.cos()*5.0,eye[1]-p.sin()*5.0,eye[2]+y.cos()*p.cos()*5.0];
            break_contact(game,m,at,along,&mut Budget::new(0))
        },
    };
    let Some(aim)=contact else {if m.b("Rotate"){sunset::turn(NAME,turn.0,turn.1);}return false;};
    let own = own_id(game);
    let selected_slot=game.selected;
    let dimension = game.dim;
    // Retain a unique session allocation: an in-place Remote address can be
    // reused by a reconnect, while this live Arc cannot be recycled.
    let Some(session) = game.remote.as_ref().map(|r| r.session.counters.clone()) else { return false; };
    let snapshot = m.clone();
    let at = match action { Action::Base(p) | Action::Place(p) => center(p), Action::Break(_, at) => at };
    let generation = st(|s| { s.clock.queued = true; s.queued_at = Some(Instant::now()); s.clock.generation });
    let deadline = Instant::now();
    if !super::sunset_combat_control::claim(NAME) {
        st(|s| { s.clock.queued = false; s.queued_at = None; }); execution_failed(2); return false;
    }
    let finish = super::sunset_combat_control::finish(NAME);
    let deed = move |game: &mut Game<'_>| {
        let _finish = finish;
        if !_finish.current() { execution_failed(2); return; }
        if st(|s| s.clock.generation != generation || !s.clock.queued) { execution_failed(15); return; }
        st(|s| { s.clock.queued = false; s.queued_at = None; });
        if deadline.elapsed() > Duration::from_millis(200) { execution_failed(1); return; }
        if !allowed(game) { execution_failed(2); return; }
        if own_id(game) != own { execution_failed(3); return; }
        if game.selected!=selected_slot {st(|s|s.weapon=None);execution_failed(17);return;}
        if game.dim != dimension { execution_failed(4); return; }
        if !game.remote.as_ref().is_some_and(|r| std::sync::Arc::ptr_eq(&session, &r.session.counters)) { execution_failed(5); return; }
        if !game.eye_position().is_some_and(|now| in_range(now, eye, 0.5)) { execution_failed(6); return; }
        if !with(|s| s.module(NAME).is_some_and(|current| current == &snapshot && current.on)) { execution_failed(7); return; }
        let Some((targets, friends)) = players(game, &snapshot) else { execution_failed(8); return; };
        let Some(selected) = targets.iter().find(|t| t.id == target).cloned() else { execution_failed(9); return; };
        let mut budget = Budget::new(PREDICTIONS);
        let Some(sent_look)=super::sunset_combat_wire::sent_look(game) else {execution_failed(10);return;};
        let Some(now_eye)=game.eye_position() else {execution_failed(10);return;};
        let mut click=click;
        let current_contact=match action {
            Action::Base(base)=>click.and_then(|mut current| {
                current.point=util::block_contact(game,sent_look,current.clicked,base_native::face(current),snapshot.n("Place range"))?;
                let slot=obsidian_slot(game,&snapshot)?;
                if !base_native::valid(game,base,slot,current,snapshot.n("Place range"),&mut 48) {return None;}
                click=Some(current);Some(current.point)
            }),
            Action::Place(base)=>util::block_contact(game,sent_look,base,1,snapshot.n("Place range")),
            Action::Break(_,position)=>{
                let y=f64::from(sent_look.0).to_radians();let p=f64::from(sent_look.1).to_radians();
                let along=[now_eye[0]-y.sin()*p.cos()*5.0,now_eye[1]-p.sin()*5.0,now_eye[2]+y.cos()*p.cos()*5.0];
                break_contact(game,&snapshot,position,along,&mut budget)
            },
        };
        let reached=current_contact.is_some();
        let aim=current_contact.unwrap_or(aim);
        if !reached { execution_failed(10); return; }
        let slot = match action {
            Action::Base(base) => {
                let tick = st(|s| s.tick.max(ticks));
                if !snapshot.b("Place") || !snapshot.b("Auto obsidian") || !setup_valid(game, base)
                    || crystal_slot(game, &snapshot).is_none() || !st(|s| s.setup.can_send(base, tick)) {
                    execution_failed(11); return;
                }
                let Some(slot) = obsidian_slot(game, &snapshot) else { execution_failed(12); return; };
                Some(slot)
            }
            Action::Place(base) => {
                if !snapshot.b("Place") || !place_valid(game, base) { execution_failed(11); return; }
                let Some(slot) = crystal_slot(game, &snapshot) else { execution_failed(12); return; };
                Some(slot)
            }
            Action::Break(id, position) => {
                if !snapshot.b("Break") || !game.crystals.iter().any(|c| c.id == id && !c.invulnerable
                    && in_range(position, [c.position.x, c.position.y, c.position.z], 0.01)) { execution_failed(13); return; }
                let Some(weapon)=super::sunset_combat_macros::weapon_route(game) else {execution_failed(18);return;};
                if weapon!=game.selected && !snapshot.b("Auto switch"){execution_failed(18);return;}
                Some(weapon)
            }
        };
        if valid_damage(game, &snapshot, at, &[selected.clone()], &friends, action, &mut budget).is_none() { execution_failed(14); return; }
        // Slot changes are inside the accepted action. No inventory rewrite,
        // offhand swap or client entity removal is performed.
        let before = game.selected;
        match action {
            Action::Base(base) => {
                let (Some(slot), Some(click)) = (slot, click) else { execution_failed(12); return; };
                let tick = st(|s| s.tick.max(ticks));
                let before_state = game.world.block(base[0], base[1], base[2]).0;
                if !st(|s| s.setup.sent(base, before_state, tick, snapshot.n("Obsidian delay").clamp(0.0,30.0) as u64)) { execution_failed(11); return; }
                if slot != util::OFFHAND { weapon_hold(game, slot); }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn { hand: i32::from(slot == util::OFFHAND),
                    pos: click.clicked, face: base_native::face(click),
                    cursor: std::array::from_fn(|i| (click.point[i] - f64::from(click.clicked[i])) as f32),
                    inside: false, world_border: false, sequence }));
                if slot != util::OFFHAND && game.selected == slot { weapon_hold(game, before); }
                note(15);
            }
            Action::Place(base) => {
                let slot = slot.unwrap_or(util::OFFHAND);
                if slot != util::OFFHAND { weapon_hold(game, slot); }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn { hand: i32::from(slot == util::OFFHAND), pos: base,
                    face: 1, cursor: std::array::from_fn(|i|(aim[i]-f64::from(base[i])) as f32), inside: false, world_border: false, sequence }));
                if snapshot.b("Switch Back") && slot != util::OFFHAND { weapon_hold(game, before); }
                else if slot!=util::OFFHAND {st(|s|s.weapon=None);}
                note(15);
            }
            Action::Break(id, _) => {
                let Some(weapon)=slot else {execution_failed(18);return;};
                if weapon!=game.selected {
                    let Some(remote)=game.remote.as_ref() else{return;};
                    let scene=(remote.entity_id,game.dim,remote.session.counters.clone() as Arc<dyn Any+Send+Sync>);
                    let before=game.selected;
                    st(|s| {
                        if let Some(lease)=s.weapon.as_mut(){lease.owned=weapon;}
                        else{s.weapon=Some(WeaponLease{scene,original:before,owned:weapon});}
                    });
                    weapon_hold(game,weapon);
                }
                if super::sunset_combat_macros::weak_settling(game){note(2);return;}
                if !prediction::arm_blast(game,super::sunset_hurt_policy::ShotSource::Crystal(id),
                    Blast::explosion(at,6.0),&targets,selected.id,prediction::rtt(game).min(5),&mut budget) {
                    restore_weapon(game,snapshot.b("Switch Back"));execution_failed(14);return;
                }
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.reset_attack_strength();
                st(|s| {
                    if let Some((_, count)) = s.attempts.iter_mut().find(|(cid, _)| *cid == id) { *count = count.saturating_add(1); }
                    else if s.attempts.len() < MAX_ENTITIES { s.attempts.push((id, 1)); }
                });
                note(10);
                restore_weapon(game,snapshot.b("Switch Back"));
            }
        }
        if snapshot.b("Swing") { game.swing_arm(); game.send(S::Punch(Empty)); }
        let tick = st(|s| s.tick.max(ticks));
        let delay = match action { Action::Base(_) => snapshot.n("Obsidian delay"), Action::Place(_) => snapshot.n("Place delay"), Action::Break(_, _) => snapshot.n("Break delay") };
        let gap=if snapshot.b("Human Pace") && !super::sunset_crystal_bot::steer(game).is_some_and(|s|s.full_speed) {match ((tick^u64::from(target)).wrapping_mul(6364136223846793005)>>32)%100 {0..=69=>2,70..=94=>3,_=>4}} else {0};
        st(|s| s.clock.sent(action, tick, (delay.clamp(0.0,30.0) as u64).saturating_add(1).max(gap)));
    };
    let (yaw, pitch) = turn;
    if !sunset::try_turn_then(NAME, yaw, pitch, deed) {
        st(|s| { s.clock.queued = false; s.queued_at = None; });
        note(7);
        return false;
    }
    note(8);
    true
}
