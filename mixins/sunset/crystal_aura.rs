// Crystal Aura (Rust's, PR #16): places end crystals (and the obsidian under them, Auto Obsidian) and breaks them
// on enemy players, for the damage it can see they take and never more than Max Self Damage to you or a friend. Only
// crystals the server has shown are hit; a placement waits for its crystal or a timeout.
use super::remote::sunset_aura_base::{self as base_native, BaseClick};
use super::sunset::{self, Category, Module, gui_open, own_id, with};
use super::sunset_crystal_setup_policy as setup;
use super::sunset_explosive_aura::Rating;
use super::sunset_explosive_damage::{Blast, clear_line, local_damage_at};
use super::sunset_explosive_policy::{Budget, in_range};
use super::{sunset_explosive_choice::Kind, sunset_explosive_tactics as tactics};
use horizon_world::registry::BlockStateId;
use std::{any::Any, sync::Arc};
type SetupScene = (i32, super::Dim, Arc<dyn Any + Send + Sync>);
use super::Game;
use super::sunset_util::{self as util, Target};
use super::{sunset_faceplace_policy as face, sunset_lead_policy as lead, sunset_prediction as prediction};
use horizon_protocol::packets::{
    Empty,
    play::{self, Clientbound as C, Serverbound as S},
};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const NAME: &str = "Crystal Aura";
const MAX_ENTITIES: usize = 512;
const SCAN_CELLS: usize = 96;
const MAX_CANDIDATES: usize = 4;
const PREDICTIONS: usize = 24;

mod state {
    // Stop just outside the clicked top face. Ignoring the whole base would
    // incorrectly allow a ray from below to pass through the obsidian itself.
    pub(super) fn place_ray_end(top: [f64; 3]) -> [f64; 3] {
        [top[0], top[1] + 1.0e-5, top[2]]
    }
    // Unit direction, slab intersection with the actual entity bounds. Return
    // the first contact, including the eye when it is already inside the box.
    pub(super) fn ray_box(
        eye: [f64; 3],
        direction: [f64; 3],
        lo: [f64; 3],
        hi: [f64; 3],
        range: f64,
    ) -> Option<[f64; 3]> {
        if !eye
            .iter()
            .chain(direction.iter())
            .chain(lo.iter())
            .chain(hi.iter())
            .chain([range].iter())
            .all(|v| v.is_finite())
            || range < 0.0
            || (0..3).any(|i| lo[i] > hi[i])
        {
            return None;
        }
        let norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm < 1.0e-12 {
            return None;
        }
        let dir = direction.map(|v| v / norm);
        let (mut near, mut far) = (0.0f64, range);
        for i in 0..3 {
            if dir[i].abs() < 1.0e-12 {
                if eye[i] < lo[i] || eye[i] > hi[i] {
                    return None;
                }
            } else {
                let a = (lo[i] - eye[i]) / dir[i];
                let b = (hi[i] - eye[i]) / dir[i];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(std::array::from_fn(|i| eye[i] + dir[i] * near))
    }
    pub(super) fn ray_end(eye: [f64; 3], hit: [f64; 3]) -> [f64; 3] {
        let length = (0..3).map(|i| (hit[i] - eye[i]).powi(2)).sum::<f64>().sqrt();
        if length <= 1.0e-5 {
            return eye;
        }
        std::array::from_fn(|i| hit[i] + (eye[i] - hit[i]) / length * 1.0e-5)
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum Action {
        Place([i32; 3]),
        Base([i32; 3]),
        Break(u32, [f64; 3]),
    }
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Waiting {
        pub action: Action,
        pub sent: u64,
    }
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
            self.next = tick.saturating_add(delay.max(1));
            match action {
                Action::Base(_) | Action::Place(_) => {
                    self.next_base = self.next;
                    self.next_place = self.next;
                }
                Action::Break(_, _) => self.next_break = self.next,
            }
        }
        pub fn observe(&mut self, tick: u64, acknowledged: bool) {
            if self.waiting.is_some_and(|w| acknowledged || tick.saturating_sub(w.sent) >= 30) {
                self.waiting = None;
            }
        }

        pub fn available(&self) -> bool {
            !self.queued && self.waiting.is_none()
        }
        pub fn due(&self, action: Action, tick: u64) -> bool {
            self.available()
                && tick
                    >= match action {
                        Action::Base(_) => self.next_base,
                        Action::Place(_) => self.next_place,
                        Action::Break(_, _) => self.next_break,
                    }
        }
        pub fn cell(&mut self, radius: i32) -> [i32; 3] {
            let side = (radius * 2 + 1) as usize;
            let n = self.cursor % (side * side * side);
            self.cursor = (n + 1) % (side * side * side);
            [(n % side) as i32 - radius, ((n / side) % side) as i32 - radius, (n / (side * side)) as i32 - radius]
        }
    }
}
use state::{Action, Clock};

struct Plan {
    action: Action,
    target: u32,
    aim: [f64; 3],
    rating: Rating,
    click: Option<BaseClick>,
}
struct WeaponLease {
    scene: SetupScene,
    original: usize,
    owned: usize,
    owner: &'static str,
    until: u64,
}
struct State {
    clock: Clock,
    queued_at: Option<Instant>,
    tick: u64,
    plan: Option<Plan>,
    retained: Option<Action>,
    lookahead: Option<Plan>,
    opening: f64,
    probe: usize,
    attempts: Vec<(u32, u8)>,
    setup: setup::Ledger,
    setup_scene: Option<SetupScene>,
    weapon: Option<WeaponLease>,
}
static STATE: Mutex<State> = Mutex::new(State {
    clock: Clock {
        cursor: 0,
        crystals: 0,
        queued: false,
        waiting: None,
        next: 0,
        next_base: 0,
        next_place: 0,
        next_break: 0,
        generation: 0,
    },
    queued_at: None,
    tick: 0,
    plan: None,
    retained: None,
    lookahead: None,
    opening: 0.0,
    probe: 0,
    attempts: Vec::new(),
    setup: setup::Ledger::new(),
    setup_scene: None,
    weapon: None,
});
fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
fn rank(rating: &Rating) -> f64 {
    rating.planning_value()
}

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Places and breaks end crystals on enemy players, for the damage it can see they take, never more than Max self damage to you or a friend.")
            .bool("Require hold", false)
            .text("Hold key", "mouse4")
            .num("Place delay", 0.0, 0.0, 10.0, 1.0)
            .num("Break delay", 0.0, 0.0, 10.0, 1.0)
            .num("Min damage", 4.0, 0.0, 20.0, 0.5)
            .num("Max self damage", 8.0, 0.0, 20.0, 0.5)
            .bool("Face place", true)
            .text("Force face place", "None")
            .bool("Auto obsidian", true)
            .bool("Switch back", true)
            .num("Turn speed", 4.5, 1.0, 10.0, 0.5)
            .num("View angle", 90.0, 30.0, 180.0, 5.0)
            .bool("Human pace", true)
            .bool("Protect loot", true)
            .bool("Render", true)
            .text("Render color", "#FFD65CFF"),
    ]
}

// A temporary menu, manual use or competing action invalidates queued work,
// but must not replenish an unchanged setup cell's retry budget or lose its
// authoritative acknowledgement. Only a module/scene reset clears history.
pub(super) fn suspend() {
    super::sunset_combat_control::release(NAME);
    sunset::cancel_turns(NAME);
    st(|s| {
        s.clock.suspend();
        s.queued_at = None;
        s.plan = None;
        s.retained = None;
        s.lookahead = None;
        s.opening = 0.0;
    });
}
pub(super) fn reset() {
    super::sunset_explosive_aura::clear_view(false);
    prediction::reset_receipts();
    suspend();
    st(|s| {
        s.clock.reset();
        s.attempts.clear();
        s.setup.clear();
        s.setup_scene = None;
        s.lookahead = None;
        s.opening = 0.0;
    });
}
fn restore_weapon(game: &mut Game<'_>, requested: bool) {
    restore_shared(game, NAME, requested);
}
pub(super) fn restore_shared(game: &mut Game<'_>, owner: &str, requested: bool) {
    if !st(|s| s.weapon.as_ref().is_some_and(|h| h.owner == owner)) {
        return;
    }
    // A competing queued/native hand action gets to finish before a return.
    // Keep the baseline until ownership becomes available, unless the player
    // already changed the selected slot or this scene is gone.
    let stale =
        st(|s| s.weapon.as_ref().is_some_and(|h| !same_setup_scene(game, &h.scene) || h.owned != game.selected));
    if stale {
        st(|s| s.weapon = None);
        return;
    }
    if requested
        && (game.using.is_some()
            || super::sunset_player::gap_eating()
            || super::sunset_combat_control::owner().is_some_and(|active| active != owner))
    {
        return;
    }
    let Some(lease) = st(|s| s.weapon.take()) else {
        return;
    };
    if requested && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0) {
        if let Some(slot) = super::sunset_macro_policy::restore(lease.original, lease.owned, game.selected, true) {
            weapon_hold(game, slot);
        }
    }
}
pub(super) fn take_shared(game: &mut Game<'_>, slot: usize, owner: &'static str) {
    if slot == util::OFFHAND {
        return;
    }
    let Some(remote) = game.remote.as_ref() else {
        return;
    };
    let scene = (remote.entity_id, game.dim, remote.session.counters.clone() as Arc<dyn Any + Send + Sync>);
    let before = game.selected;
    st(|s| {
        if s.weapon.as_ref().is_some_and(|h| !same_setup_scene(game, &h.scene) || h.owned != before) {
            s.weapon = None;
        }
        if let Some(lease) = s.weapon.as_mut() {
            lease.owned = slot;
            lease.owner = owner;
            lease.until = game.ticks.saturating_add(10);
        } else if before != slot {
            s.weapon =
                Some(WeaponLease { scene, original: before, owned: slot, owner, until: game.ticks.saturating_add(10) });
        }
    });
    weapon_hold(game, slot);
}
fn take_weapon(game: &mut Game<'_>, slot: usize) {
    take_shared(game, slot, NAME);
}
pub(super) fn idle_shared(game: &mut Game<'_>, owner: &str, requested: bool) {
    if st(|s| s.weapon.as_ref().is_some_and(|h| h.owner == owner && game.ticks > h.until)) {
        restore_shared(game, owner, requested);
    }
}
fn idle_weapon(game: &mut Game<'_>, requested: bool) {
    idle_shared(game, NAME, requested);
}
pub(super) fn cleanup_shared(game: &mut Game<'_>) {
    let owner = st(|s| s.weapon.as_ref().map(|h| h.owner));
    if let Some(owner) = owner {
        let requested = owner != "Crystal Aura" || with(|s| s.module(NAME).is_none_or(|m| m.b("Switch back")));
        restore_shared(game, owner, requested);
    }
}
fn weapon_hold(game: &mut Game<'_>, slot: usize) {
    sunset::hold(game, slot);
    super::sunset_combat_macros::note_main(game);
}

pub(super) fn paused(game: &mut Game<'_>) {
    let requested = with(|s| s.module(NAME).is_none_or(|m| m.b("Switch back")));
    restore_weapon(game, requested);
    suspend();
}
pub(super) fn disabled(game: &mut Game<'_>) {
    cleanup_shared(game);
    reset();
}

fn allowed(game: &Game<'_>) -> bool {
    super::sunset_explosive_aura::can_act(game)
        && util::in_world(game)
        && !gui_open()
        && !game.spectator()
        && game.using.is_none()
        && game.mining.is_none()
        && util::inventory_idle(game)
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && game.mobs.len() <= MAX_ENTITIES
        && game.crystals.len() <= MAX_ENTITIES
}

// Refuse nameless players: identity must be known before deciding friendship.
// Scan the complete bounded list, so omitted friends cannot silently disappear.
fn players(game: &Game<'_>, _m: &Module) -> Option<(Vec<Target>, Vec<Target>)> {
    tactics::players(game)
}
fn cap(m: &Module) -> f64 {
    m.n("Max self damage")
}
fn valid_damage(
    game: &mut Game<'_>,
    m: &Module,
    at: [f64; 3],
    targets: &[Target],
    friends: &[Target],
    action: Action,
    budget: &mut Budget,
) -> Option<(Rating, u32)> {
    damage_pair(game, m, at, targets, friends, action, budget)?.0
}
// One geometric sample per victim; temporal readiness reuses that raw sample.
fn damage_pair(
    game: &mut Game<'_>,
    m: &Module,
    at: [f64; 3],
    targets: &[Target],
    friends: &[Target],
    action: Action,
    budget: &mut Budget,
) -> Option<(Option<(Rating, u32)>, Option<(Rating, u32)>)> {
    let blast = match action {
        Action::Base(base) => {
            Blast::explosion(at, 6.0).with_block(base, game.items().get("minecraft:obsidian")?.block?)
        }
        _ => Blast::explosion(at, 6.0),
    };
    if !super::sunset_loot_guard::allows(game, blast, m.b("Protect loot")) {
        return None;
    }
    let breaking = matches!(action, Action::Break(_, _));
    let horizon =
        lead::horizons(if breaking { lead::Use::CrystalBreak } else { lead::Use::CrystalPlace }, prediction::rtt(game));
    let offset = prediction::local_offset(game, horizon.local)?;
    let own = local_damage_at(game, blast, offset, budget)?.estimate;
    let p = game.player.as_ref()?;
    let health = f64::from(p.health.health() + p.health.absorption());
    let max = cap(m);
    if own > max || tactics::endangered(own, health, game.creative) {
        return None;
    }
    for friend in friends {
        let projected = prediction::target(game, friend, horizon.others)?;
        if !in_range(at, friend.pos, 12.0) && !in_range(at, projected.pos, 12.0) {
            continue;
        }
        let damage = tactics::friend_damage(game, &projected, blast, budget)?;
        let creative = super::remote::sunset_combat_remote::player_mode(game, friend.id) == Some(1);
        if damage > max || tactics::endangered(damage, tactics::friend_health(game, friend), creative) {
            return None;
        }
    }
    let mut best: [Option<(Rating, u32)>; 2] = [None, None];
    for target in targets {
        let projected = prediction::target(game, target, horizon.others)?;
        let ahead = if breaking { 0 } else { prediction::rtt(game) };
        let opening = if breaking { prediction::opens_in(target.id, prediction::rtt(game)) } else { ahead };
        let (damage, _) =
            prediction::estimate_pair(game, &projected, blast, ahead, opening, prediction::rtt(game), budget)?;
        let facts = prediction::face_facts(game, target);
        let minimum = m.n("Min damage");
        let threshold = face::crystal(
            minimum,
            m.b("Face place"),
            util::setting_key_down(game, m, "Force face place"),
            breaking,
            &facts,
        )?
        .minimum;
        for i in 0..2 {
            if !face::sufficient(damage[i], threshold, facts.effective_health) {
                continue;
            }
            let kind = match action {
                Action::Break(_, _) => Kind::CrystalHit,
                Action::Place(_) => Kind::CrystalPlace,
                Action::Base(_) => Kind::ObsidianPlace,
            };
            let ready = match action {
                Action::Break(_, _) => {
                    if i == 0 {
                        0
                    } else {
                        opening
                    }
                }
                Action::Place(_) => prediction::rtt(game),
                Action::Base(_) => prediction::rtt(game) + 1,
            };
            let mut rating = Rating::new(
                kind,
                damage[i],
                prediction::effective_health(game, target).unwrap_or(f64::from(target.health)),
                own,
                prediction::held_totem(target.id),
                ready.min(u32::from(u16::MAX)) as u16,
            );
            rating.full_damage = damage[2];
            if best[i].as_ref().is_none_or(|(old, _)| rank(&rating) > rank(old)) {
                best[i] = Some((rating, target.id));
            }
        }
    }
    Some((best[0], best[1]))
}

fn center(base: [i32; 3]) -> [f64; 3] {
    [f64::from(base[0]) + 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) + 0.5]
}
fn obsidian(game: &Game<'_>, base: [i32; 3]) -> bool {
    &*util::facts(game, base).name == "minecraft:obsidian"
}
fn place_valid(game: &Game<'_>, base: [i32; 3]) -> bool {
    if !tactics::breakable(game, base) {
        return false;
    }
    matches!(&*util::facts(game, base).name, "minecraft:obsidian" | "minecraft:bedrock")
        && st(|s| s.setup.crystal_allowed(base, s.tick, obsidian(game, base)))
        && crystal_column(game, base)
}
fn setup_valid(game: &Game<'_>, base: [i32; 3]) -> bool {
    if !tactics::breakable(game, base) {
        return false;
    }
    let f = util::facts(game, base);
    f.replaceable
        && f.fluid == super::Fluid::None
        && f.fluid_level == 0
        && !f.waterlogged
        && util::can_place(game, base, true)
        && crystal_column(game, base)
}
// Keep only a cell identity between scan slices. The current world, ledger,
// bot floor and setting choose its action again; no hit, damage or slot survives.
fn setup_action(game: &Game<'_>, m: &Module, base: [i32; 3], ticks: u64) -> Option<Action> {
    if place_valid(game, base) {
        Some(Action::Place(base))
    } else if m.b("Auto obsidian") && setup_valid(game, base) && st(|s| s.setup.can_send(base, ticks)) {
        Some(Action::Base(base))
    } else {
        None
    }
}
fn setup_cell(action: Action) -> Option<[i32; 3]> {
    match action {
        Action::Place(base) | Action::Base(base) => Some(base),
        Action::Break(_, _) => None,
    }
}
fn crystal_column(game: &Game<'_>, base: [i32; 3]) -> bool {
    if !game.world.loaded(base[0], base[2]) || base[1] < -64 || base[1] >= 318 {
        return false;
    }
    if !util::is_air(game, [base[0], base[1] + 1, base[2]]) {
        return false;
    }
    let at = center(base);
    let lo = [at[0] - 0.5, at[1], at[2] - 0.5];
    let hi = [lo[0] + 1.0, lo[1] + 2.0, lo[2] + 1.0];
    let hits = |a: horizon_core::geometry::Aabb| {
        let (a, b) = (a.min_position(), a.max_position());
        a.x < hi[0] && b.x > lo[0] && a.y < hi[1] && b.y > lo[1] && a.z < hi[2] && b.z > lo[2]
    };
    if game.player.as_ref().is_some_and(|p| hits(p.bounds()))
        || game.mobs.iter().any(|mob| !mob.removed && hits(mob.bounds()))
    {
        return false;
    }
    for mob in game.mobs.iter().filter(|m| m.kind.id == "minecraft:player" && !m.removed && m.alive()) {
        let Some(future) =
            prediction::target(game, &util::mob_target(mob), prediction::rtt(game).saturating_mul(2).min(5))
        else {
            return false;
        };
        if future.lo[0] < hi[0]
            && future.hi[0] > lo[0]
            && future.lo[1] < hi[1]
            && future.hi[1] > lo[1]
            && future.lo[2] < hi[2]
            && future.hi[2] > lo[2]
        {
            return false;
        }
    }
    !game.crystals.iter().any(|c| {
        !st(|s| s.clock.waiting.is_some_and(|w| matches!(w.action,Action::Break(id,_) if id==c.id)))
            && (c.position.x - at[0]).abs() < 1.5
            && c.position.y < hi[1]
            && c.position.y + 2.0 > lo[1]
            && (c.position.z - at[2]).abs() < 1.5
    })
}

fn hit_visible(
    game: &Game<'_>,
    _m: &Module,
    eye: [f64; 3],
    hit: [f64; 3],
    ray_end: [f64; 3],
    range: f64,
    budget: &mut Budget,
) -> bool {
    let range = range.clamp(0.0, 64.0);
    in_range(eye, hit, range) && (in_range(eye, hit, 0.0) || clear_line(game, eye, ray_end, None, budget))
}
fn break_contact(game: &Game<'_>, m: &Module, at: [f64; 3], aim: [f64; 3], budget: &mut Budget) -> Option<[f64; 3]> {
    let eye = game.eye_position()?;
    let lo = [at[0] - 1.0, at[1], at[2] - 1.0];
    let hi = [at[0] + 1.0, at[1] + 2.0, at[2] + 1.0];
    let direction = std::array::from_fn(|i| aim[i] - eye[i]);
    let hit = state::ray_box(eye, direction, lo, hi, tactics::entity_range(game).clamp(0.0, 64.0))?;
    hit_visible(game, m, eye, hit, state::ray_end(eye, hit), tactics::entity_range(game), budget).then_some(hit)
}
fn reachable(game: &Game<'_>, m: &Module, action: Action, budget: &mut Budget) -> Option<[f64; 3]> {
    let eye = game.eye_position()?;
    match action {
        Action::Base(_) => None,
        Action::Place(base) => {
            let top = center(base);
            hit_visible(game, m, eye, top, state::place_ray_end(top), tactics::block_range(game), budget).then_some(top)
        }
        Action::Break(_, at) => {
            let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
            let dir = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
            let current = std::array::from_fn(|i| eye[i] + dir[i] * 64.0);
            if let Some(hit) = break_contact(game, m, at, current, budget) {
                return Some(std::array::from_fn(|i| hit[i] + dir[i] * 0.01));
            }
            if tactics::fixed_look(game) {
                return None;
            }
            let nearest = [
                eye[0].clamp(at[0] - 0.9, at[0] + 0.9),
                eye[1].clamp(at[1] + 0.1, at[1] + 1.9),
                eye[2].clamp(at[2] - 0.9, at[2] + 0.9),
            ];
            for aim in [nearest, [at[0], at[1] + 1.0, at[2]]] {
                if break_contact(game, m, at, aim, budget).is_some() {
                    return Some(aim);
                }
            }
            None
        }
    }
}

fn crystal_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    item_slot(game, m, "end_crystal")
}
fn obsidian_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    item_slot(game, m, "obsidian")
}
fn item_slot(game: &Game<'_>, _m: &Module, wanted: &str) -> Option<usize> {
    let slot = util::find_held(game, |name| name == wanted)?;
    let stack =
        if slot == util::OFFHAND { game.offhand.as_ref() } else { game.inventory.get(slot).and_then(|s| s.as_ref()) }?;
    if stack.count == 0 {
        return None;
    }
    Some(slot)
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
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0
            && game.dim == old.1
            && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}
fn setup_payload(game: &Game<'_>, pos: [i32; 3], id: u32) {
    let id = BlockStateId(id);
    if game.pack.assets.registry.state(id).is_none() {
        return;
    }
    let is_obsidian = &*game.res.facts().get(id).name == "minecraft:obsidian";
    st(|s| s.setup.payload(pos, id.0, is_obsidian));
}
// Called from the accepted native packet observer; never from prediction/ACK.
// A protocol world/life boundary can keep the same connection/dimension/id.
// Never restore a pre-boundary hand or allow its captured deed in the new life.
pub(super) fn discard_scene() {
    super::sunset_combat_control::reset();
    st(|s| s.weapon = None);
    reset();
    super::sunset_anchor_aura::reset();
}

pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::Disconnect(_)) {
        discard_scene();
        return;
    }
    if !matches!(packet, C::BlockUpdate(_) | C::SectionBlocksUpdate(_)) {
        return;
    }
    if !st(|s| s.setup.len() != 0 && s.setup_scene.as_ref().is_some_and(|old| same_setup_scene(game, old))) {
        return;
    }
    match packet {
        C::BlockUpdate(p) => {
            if let Ok(id) = u32::try_from(p.state) {
                setup_payload(game, p.pos, id);
            }
        }
        C::SectionBlocksUpdate(p) => {
            for change in p.changes.iter().take(4096) {
                if let Some((pos, id)) = setup::unpack_section(p.section, *change) {
                    setup_payload(game, pos, id);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn opening_value() -> f64 {
    st(|s| s.opening)
}
pub(super) fn reserved_cell(cell: [i32; 3]) -> bool {
    st(|s| {
        s.clock.waiting.is_some_and(|w| {
            if let Action::Place(base) = w.action {
                let at = center(base);
                (0..3).all(|i| {
                    let (lo, hi) = if i == 1 { (at[i], at[i] + 2.0) } else { (at[i] - 1.0, at[i] + 1.0) };
                    f64::from(cell[i]) < hi && f64::from(cell[i]) + 1.0 > lo
                })
            } else {
                false
            }
        })
    })
}
pub(super) fn owns_selection(game: &Game<'_>) -> bool {
    st(|s| s.weapon.as_ref().is_some_and(|lease| same_setup_scene(game, &lease.scene) && lease.owned == game.selected))
}
pub(super) fn idle(game: &mut Game<'_>) {
    let restore = with(|s| s.module(NAME).is_none_or(|m| m.b("Switch back")));
    idle_weapon(game, restore);
    suspend();
}
pub(super) fn lookahead() -> Option<Rating> {
    st(|s| s.lookahead.as_ref().map(|p| p.rating))
}
pub(super) fn advance_look(game: &mut Game<'_>, m: &Module) {
    if !allowed(game) || tactics::fixed_look(game) || super::sunset_combat_control::owner().is_some() {
        return;
    }
    let Some((aim, action)) = st(|s| s.lookahead.as_ref().map(|p| (p.aim, p.action))) else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    if !view_allowed(game, m, action, aim) {
        return;
    }
    let desired = sunset::look_at(tactics::aim_eye(game).unwrap_or(eye), aim);
    let turn = tactics::next_look(game, desired, m.n("Turn speed"));
    sunset::turn(NAME, turn.0, turn.1);
}
fn held(game: &Game<'_>, m: &Module, action: Action) -> bool {
    let Some(look) = super::sunset_combat_wire::sent_look(game) else {
        return false;
    };
    let Some(eye) = game.eye_position() else {
        return false;
    };
    match action {
        Action::Break(_, at) => {
            let y = f64::from(look.0).to_radians();
            let p = f64::from(look.1).to_radians();
            break_contact(
                game,
                m,
                at,
                [eye[0] - y.sin() * p.cos() * 64.0, eye[1] - p.sin() * 64.0, eye[2] + y.cos() * p.cos() * 64.0],
                &mut Budget::new(0),
            )
            .is_some()
        }
        Action::Place(base) => util::block_contact(game, look, base, 1, tactics::block_range(game)).is_some(),
        Action::Base(_) => false,
    }
}
fn view_allowed(game: &Game<'_>, m: &Module, action: Action, aim: [f64; 3]) -> bool {
    if tactics::fixed_look(game) {
        return tactics::same_camera(game);
    }
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let desired = sunset::look_at(tactics::aim_eye(game).unwrap_or(eye), aim);
    let angle = f64::from((desired.0 - game.yaw + 180.0).rem_euclid(360.0) - 180.0).abs();
    angle <= m.n("View angle") + if held(game, m, action) { 20.0 } else { 0.0 }
}
fn credit(game: &Game<'_>, m: &Module, action: Action, aim: [f64; 3]) -> f64 {
    if tactics::fixed_look(game) {
        return 0.0;
    }
    let mut score = if held(game, m, action) { 2.0 } else { 0.0 };
    if let Action::Place(base) = action {
        if let Some(eye) = game.eye_position() {
            let at = center(base);
            let dir = std::array::from_fn(|i| aim[i] - eye[i]);
            if state::ray_box(
                eye,
                dir,
                [at[0] - 1.0, at[1], at[2] - 1.0],
                [at[0] + 1.0, at[1] + 2.0, at[2] + 1.0],
                tactics::entity_range(game),
            )
            .is_some()
            {
                score += 2.0;
            }
        }
    }
    score
}
fn optimistic(at: [f64; 3], targets: &[Target]) -> f64 {
    targets
        .iter()
        .map(|t| {
            let raw = super::sunset_explosive_policy::raw_damage(util::dist(at, t.pos), 6.0, 1.0).unwrap_or(0.0);
            let hp = f64::from(t.health).max(0.01);
            let damage = f64::from(raw) * 1.5;
            damage.min(hp) + if damage >= hp { 1000.0 } else { 0.0 }
        })
        .fold(0.0, f64::max)
}
fn keep(best: &mut Option<Plan>, plan: Plan) {
    if best.as_ref().is_none_or(|old| rank(&plan.rating) > rank(&old.rating)) {
        *best = Some(plan);
    }
}
pub(super) fn prepare(game: &mut Game<'_>, m: Option<&Module>, ticks: u64) -> Option<Rating> {
    let Some(m) = m.filter(|m| m.on) else {
        disabled(game);
        return None;
    };
    if st(|s| s.setup_scene.as_ref().is_some_and(|old| !same_setup_scene(game, old))) {
        reset();
    }
    st(|s| {
        s.lookahead = None;
        s.opening = 0.0;
    });
    if !allowed(game) || m.b("Require hold") && !util::setting_key_down(game, m, "Hold key") {
        paused(game);
        return None;
    }
    if tactics::fixed_look(game) && !tactics::same_camera(game) {
        suspend();
        return None;
    }
    if st(|s| {
        s.weapon.as_ref().is_some_and(|lease| !same_setup_scene(game, &lease.scene) || lease.owned != game.selected)
    }) {
        st(|s| s.weapon = None);
        suspend();
        return None;
    }
    idle_weapon(game, m.b("Switch back"));
    super::sunset_combat_macros::note_main(game);
    st(|s| {
        s.setup_scene = game
            .remote
            .as_ref()
            .map(|r| (r.entity_id, game.dim, r.session.counters.clone() as Arc<dyn Any + Send + Sync>));
        s.tick = ticks;
        s.plan = None;
        s.attempts.retain(|(id, _)| game.crystals.iter().any(|c| c.id == *id));
    });
    if st(|s| s.queued_at.is_some_and(|t| t.elapsed() > Duration::from_millis(200))) {
        suspend();
    }
    if let Some(waiting) = st(|s| s.clock.waiting) {
        let ack = observed(game, waiting.action)
            || ticks.saturating_sub(waiting.sent) >= u64::from(prediction::rtt(game).saturating_add(2).clamp(4, 20));
        st(|s| s.clock.observe(ticks, ack));
    }
    if st(|s| s.clock.queued) {
        return None;
    }
    let Some((targets, friends)) = players(game, m).filter(|(t, _)| !t.is_empty()) else {
        st(|s| s.retained = None);
        restore_weapon(game, m.b("Switch back"));
        return None;
    };
    let mut budget = Budget::new(PREDICTIONS);
    let mut best = None;
    let mut opening = None;
    let mut evaluated = 0;
    let mut stage = super::sunset_explosive_choice::Stage {
        crystal_receipt_pending: st(|s| s.clock.waiting.is_some_and(|w| matches!(w.action, Action::Place(_)))),
        ..Default::default()
    };
    let count = game.crystals.len();
    let start = st(|s| s.clock.crystals);
    let mut shortlist = Vec::with_capacity(count.min(64));
    for n in 0..count.min(64) {
        let c = &game.crystals[(start + n) % count];
        if st(|s| s.clock.waiting.is_some_and(|w| matches!(w.action,Action::Break(id,_) if id==c.id))) {
            continue;
        }
        if c.id == 0 || c.invulnerable || st(|s| s.attempts.iter().any(|&(id, attempts)| id == c.id && attempts >= 3)) {
            continue;
        }
        let at = [c.position.x, c.position.y, c.position.z];
        let action = Action::Break(c.id, at);
        let Some(aim) = reachable(game, m, action, &mut budget) else {
            continue;
        };
        if !view_allowed(game, m, action, aim) {
            continue;
        }
        shortlist.push((optimistic(at, &targets) + credit(game, m, action, aim), action, aim));
    }
    shortlist.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Reserve one exposure evaluation for a placement when all examined breaks
    // are ineffective. Rotate a tie/failure probe so four stale entities cannot
    // permanently starve an unseen candidate in a short list.
    let probe = st(|s| {
        let n = s.probe;
        s.probe = s.probe.wrapping_add(1);
        n
    });
    if shortlist.len() > 2 {
        let tail = 1 + probe % (shortlist.len() - 1);
        shortlist.swap(1, tail);
    }
    for (_, action, aim) in shortlist.into_iter().take(MAX_CANDIDATES - 1) {
        let Action::Break(_, at) = action else {
            continue;
        };
        evaluated += 1;
        if let Some((now, later)) = damage_pair(game, m, at, &targets, &friends, action, &mut budget) {
            stage.immediate_crystal |= now.is_some();
            stage.opening_crystal |= later.is_some();
            if let Some((mut rating, target)) = later {
                st(|s| s.opening = s.opening.max(rating.value()));
                rating.look_credit = credit(game, m, action, aim);
                keep(&mut opening, Plan { rating, target, action, aim, click: None });
            }
            if let Some((mut rating, target)) = now {
                rating.look_credit = credit(game, m, action, aim);
                if st(|s| s.clock.due(action, ticks)) {
                    keep(&mut best, Plan { rating, target, action, aim, click: None });
                }
            }
        }
    }
    st(|s| s.clock.crystals = if count == 0 { 0 } else { (start + 64) % count });
    if best.is_none() && stage.new_crystal() && evaluated < MAX_CANDIDATES && crystal_slot(game, m).is_some() {
        let Some(origin) = util::block_pos(game) else {
            return None;
        };
        let retained = st(|s| s.retained).and_then(setup_cell).and_then(|base| setup_action(game, m, base, ticks));
        let mut cells = Vec::new();
        for _ in 0..SCAN_CELLS {
            let radius = tactics::block_range(game).ceil().clamp(1.0, 64.0) as i32;
            let d = st(|s| s.clock.cell(radius));
            let base = std::array::from_fn(|i| origin[i].saturating_add(d[i]));
            if retained.and_then(setup_cell) == Some(base) {
                continue;
            }
            if let Some(action) = setup_action(game, m, base, ticks) {
                cells.push((optimistic(center(base), &targets), action));
            }
        }
        cells.sort_by(|a, b| b.0.total_cmp(&a.0));
        // The retained cell gets the reserved fourth evaluation before newly
        // scanned cells. It still uses every fresh geometry/material/damage
        // check below and shares the same MAX_CANDIDATES and ray budgets.
        if let Some(action) = retained {
            cells.insert(0, (0.0, action));
        }
        let mut base_best = None;
        let mut crystal_best = None;
        let mut rays = 48;
        for (_, action) in cells {
            if evaluated >= MAX_CANDIDATES {
                break;
            }
            let (aim, click) = match action {
                Action::Place(_) => {
                    let Some(aim) = reachable(game, m, action, &mut budget) else {
                        continue;
                    };
                    (aim, None)
                }
                Action::Base(base) => {
                    let Some(slot) = obsidian_slot(game, m) else {
                        continue;
                    };
                    let Some(click) = base_native::plan(game, base, slot, tactics::block_range(game), &mut rays) else {
                        continue;
                    };
                    (click.point, Some(click))
                }
                _ => continue,
            };
            if !view_allowed(game, m, action, aim) {
                continue;
            }
            evaluated += 1;
            let at = match action {
                Action::Place(base) | Action::Base(base) => center(base),
                _ => continue,
            };
            let Some((mut rating, target)) = valid_damage(game, m, at, &targets, &friends, action, &mut budget) else {
                continue;
            };
            rating.look_credit = credit(game, m, action, aim);
            let plan = Plan { rating, target, action, aim, click };
            if matches!(action, Action::Base(_)) {
                keep(&mut base_best, plan);
            } else {
                keep(&mut crystal_best, plan);
            }
        }
        best = match (crystal_best, base_best) {
            (Some(c), Some(b)) => Some(if setup::upgrade(c.rating.full_damage, b.rating.full_damage) { b } else { c }),
            (c, b) => c.or(b),
        };
    }
    st(|s| s.retained = best.as_ref().and_then(|p| setup_cell(p.action).map(|_| p.action)));
    if let Some(plan) = best {
        let due = st(|s| s.clock.due(plan.action, ticks));
        let rating = plan.rating;
        let cell = match plan.action {
            Action::Place(p) | Action::Base(p) => p,
            Action::Break(_, p) => [p[0].floor() as i32, p[1].floor() as i32 - 1, p[2].floor() as i32],
        };
        super::sunset_explosive_aura::show(game, false, cell, rating);
        if due {
            st(|s| s.plan = Some(plan));
            return Some(rating);
        }
        keep(&mut opening, plan);
    }
    st(|s| s.lookahead = opening);
    None
}

pub(super) fn execute(game: &mut Game<'_>, m: &Module, ticks: u64) -> bool {
    let Some(plan) = st(|s| s.plan.take()) else {
        return false;
    };
    if !m.on || !allowed(game) || !st(|s| s.clock.due(plan.action, ticks)) {
        return false;
    }
    queue(game, m, plan.action, plan.target, plan.aim, plan.click, ticks)
}

fn queue(
    game: &mut Game<'_>,
    m: &Module,
    action: Action,
    target: u32,
    aim: [f64; 3],
    click: Option<BaseClick>,
    ticks: u64,
) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let desired = sunset::look_at(tactics::aim_eye(game).unwrap_or(eye), aim);
    let turn = if !tactics::fixed_look(game) {
        tactics::next_look(game, desired, m.n("Turn speed"))
    } else {
        (game.yaw, game.pitch)
    };
    let angle = f64::from((desired.0 - game.yaw + 180.0).rem_euclid(360.0) - 180.0).abs();
    let held = super::sunset_combat_wire::sent_look(game).is_some_and(|look| match action {
        Action::Break(_, at) => {
            let y = f64::from(look.0).to_radians();
            let p = f64::from(look.1).to_radians();
            break_contact(
                game,
                m,
                at,
                [eye[0] - y.sin() * p.cos() * 64.0, eye[1] - p.sin() * 64.0, eye[2] + y.cos() * p.cos() * 64.0],
                &mut Budget::new(0),
            )
            .is_some()
        }
        Action::Base(_) => click.is_some_and(|c| {
            util::block_contact(game, look, c.clicked, base_native::face(c), tactics::block_range(game)).is_some()
        }),
        Action::Place(base) => util::block_contact(game, look, base, 1, tactics::block_range(game)).is_some(),
    });
    if !tactics::fixed_look(game)
        && m.n("View angle") < 180.0
        && angle > m.n("View angle") + if held { 20.0 } else { 0.0 }
    {
        return false;
    }
    let mut click = click;
    let contact = match action {
        Action::Base(_) => click.and_then(|mut c| {
            c.point = util::block_contact(game, turn, c.clicked, base_native::face(c), tactics::block_range(game))?;
            click = Some(c);
            Some(c.point)
        }),
        Action::Place(base) => util::block_contact(game, turn, base, 1, tactics::block_range(game)),
        Action::Break(_, at) => {
            let y = f64::from(turn.0).to_radians();
            let p = f64::from(turn.1).to_radians();
            let along = [eye[0] - y.sin() * p.cos() * 64.0, eye[1] - p.sin() * 64.0, eye[2] + y.cos() * p.cos() * 64.0];
            break_contact(game, m, at, along, &mut Budget::new(0))
        }
    };
    let Some(aim) = contact else {
        if !tactics::fixed_look(game) {
            sunset::turn(NAME, turn.0, turn.1);
        }
        return false;
    };
    let own = own_id(game);
    let selected_slot = game.selected;
    let dimension = game.dim;
    // Retain a unique session allocation: an in-place Remote address can be
    // reused by a reconnect, while this live Arc cannot be recycled.
    let Some(session) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return false;
    };
    let snapshot = m.clone();
    let at = match action {
        Action::Base(p) | Action::Place(p) => center(p),
        Action::Break(_, at) => at,
    };
    let generation = st(|s| {
        s.clock.queued = true;
        s.queued_at = Some(Instant::now());
        s.clock.generation
    });
    let deadline = Instant::now();
    if !super::sunset_combat_control::claim(NAME) {
        st(|s| {
            s.clock.queued = false;
            s.queued_at = None;
        });
        return false;
    }
    let finish = super::sunset_combat_control::finish(NAME);
    let deed = move |game: &mut Game<'_>| {
        let _finish = finish;
        if !_finish.current() {
            return;
        }
        if st(|s| s.clock.generation != generation || !s.clock.queued) {
            return;
        }
        st(|s| {
            s.clock.queued = false;
            s.queued_at = None;
        });
        if deadline.elapsed() > Duration::from_millis(200) {
            return;
        }
        if !tactics::refresh(game) || !allowed(game) {
            return;
        }
        if own_id(game) != own {
            return;
        }
        if game.selected != selected_slot {
            st(|s| s.weapon = None);
            return;
        }
        if game.dim != dimension {
            return;
        }
        if !game.remote.as_ref().is_some_and(|r| std::sync::Arc::ptr_eq(&session, &r.session.counters)) {
            return;
        }
        if !game.eye_position().is_some_and(|now| in_range(now, eye, 0.5)) {
            return;
        }
        if !with(|s| s.module(NAME).is_some_and(|current| current == &snapshot && current.on)) {
            return;
        }
        let Some((targets, friends)) = players(game, &snapshot) else {
            return;
        };
        let Some(selected) = targets.iter().find(|t| t.id == target).cloned() else {
            return;
        };
        let mut budget = Budget::new(PREDICTIONS);
        let Some(sent_look) = super::sunset_combat_wire::sent_look(game) else {
            return;
        };
        let Some(now_eye) = game.eye_position() else {
            return;
        };
        let mut click = click;
        let current_contact = match action {
            Action::Base(base) => click.and_then(|mut current| {
                current.point = util::block_contact(
                    game,
                    sent_look,
                    current.clicked,
                    base_native::face(current),
                    tactics::block_range(game),
                )?;
                let slot = obsidian_slot(game, &snapshot)?;
                if !base_native::valid(game, base, slot, current, tactics::block_range(game), &mut 48) {
                    return None;
                }
                click = Some(current);
                Some(current.point)
            }),
            Action::Place(base) => util::block_contact(game, sent_look, base, 1, tactics::block_range(game)),
            Action::Break(_, position) => {
                let y = f64::from(sent_look.0).to_radians();
                let p = f64::from(sent_look.1).to_radians();
                let along = [
                    now_eye[0] - y.sin() * p.cos() * 64.0,
                    now_eye[1] - p.sin() * 64.0,
                    now_eye[2] + y.cos() * p.cos() * 64.0,
                ];
                break_contact(game, &snapshot, position, along, &mut budget)
            }
        };
        let reached = current_contact.is_some();
        let aim = current_contact.unwrap_or(aim);
        if !reached {
            return;
        }
        let slot = match action {
            Action::Base(base) => {
                let tick = st(|s| s.tick.max(ticks));
                if !snapshot.b("Auto obsidian")
                    || !setup_valid(game, base)
                    || crystal_slot(game, &snapshot).is_none()
                    || !st(|s| s.setup.can_send(base, tick))
                {
                    return;
                }
                let Some(slot) = obsidian_slot(game, &snapshot) else {
                    return;
                };
                Some(slot)
            }
            Action::Place(base) => {
                if !place_valid(game, base) {
                    return;
                }
                let Some(slot) = crystal_slot(game, &snapshot) else {
                    return;
                };
                Some(slot)
            }
            Action::Break(id, position) => {
                if !game.crystals.iter().any(|c| {
                    c.id == id
                        && !c.invulnerable
                        && in_range(position, [c.position.x, c.position.y, c.position.z], 0.01)
                }) {
                    return;
                }
                let Some(weapon) = super::sunset_combat_macros::weapon_route(game) else {
                    return;
                };

                Some(weapon)
            }
        };
        if valid_damage(game, &snapshot, at, &[selected.clone()], &friends, action, &mut budget).is_none() {
            return;
        }
        // Slot changes are inside the accepted action. No inventory rewrite,
        // offhand swap or client entity removal is performed.
        match action {
            Action::Base(base) => {
                let (Some(slot), Some(click)) = (slot, click) else {
                    return;
                };
                let tick = st(|s| s.tick.max(ticks));
                let before_state = game.world.block(base[0], base[1], base[2]).0;
                if !st(|s| s.setup.sent(base, before_state, tick, 0)) {
                    return;
                }
                if slot != util::OFFHAND {
                    take_weapon(game, slot);
                }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn {
                    hand: i32::from(slot == util::OFFHAND),
                    pos: click.clicked,
                    face: base_native::face(click),
                    cursor: std::array::from_fn(|i| (click.point[i] - f64::from(click.clicked[i])) as f32),
                    inside: false,
                    world_border: false,
                    sequence,
                }));
            }
            Action::Place(base) => {
                let slot = slot.unwrap_or(util::OFFHAND);
                if slot != util::OFFHAND {
                    take_weapon(game, slot);
                }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn {
                    hand: i32::from(slot == util::OFFHAND),
                    pos: base,
                    face: 1,
                    cursor: std::array::from_fn(|i| (aim[i] - f64::from(base[i])) as f32),
                    inside: false,
                    world_border: false,
                    sequence,
                }));
                if !snapshot.b("Switch back") && slot != util::OFFHAND {
                    st(|s| s.weapon = None);
                }
            }
            Action::Break(id, _) => {
                let Some(weapon) = slot else {
                    return;
                };
                take_weapon(game, weapon);
                if super::sunset_combat_macros::weak_settling(game) {
                    return;
                }
                if !prediction::arm_blast(
                    game,
                    super::sunset_hurt_policy::ShotSource::Crystal(id),
                    Blast::explosion(at, 6.0),
                    &targets,
                    selected.id,
                    prediction::rtt(game).min(5),
                    &mut budget,
                ) {
                    restore_weapon(game, snapshot.b("Switch back"));
                    return;
                }
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.reset_attack_strength();
                st(|s| {
                    if let Some((_, count)) = s.attempts.iter_mut().find(|(cid, _)| *cid == id) {
                        *count = count.saturating_add(1);
                    } else if s.attempts.len() < MAX_ENTITIES {
                        s.attempts.push((id, 1));
                    }
                });
            }
        }
        game.swing_arm();
        game.send(S::Punch(Empty));
        let tick = st(|s| s.tick.max(ticks));
        st(|s| {
            if let Some(lease) = s.weapon.as_mut() {
                lease.until = game.ticks + 10;
            }
        });
        let delay = match action {
            Action::Base(_) | Action::Place(_) => snapshot.n("Place delay"),
            Action::Break(_, _) => snapshot.n("Break delay"),
        };
        let gap = if snapshot.b("Human pace") {
            match ((tick ^ u64::from(target)).wrapping_mul(6364136223846793005) >> 32) % 100 {
                0..=69 => 2,
                70..=94 => 3,
                _ => 4,
            }
        } else {
            0
        };
        st(|s| s.clock.sent(action, tick, (delay.clamp(0.0, 30.0) as u64).saturating_add(1).max(gap)));
    };
    let (yaw, pitch) = turn;
    if !sunset::try_turn_then(NAME, yaw, pitch, deed) {
        st(|s| {
            s.clock.queued = false;
            s.queued_at = None;
        });

        return false;
    }

    true
}
