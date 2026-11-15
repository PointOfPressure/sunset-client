// Runtime part (see runtime.rs): cooldowns, Criticals, the attack guard, strikes, Kill Aura and Trigger Bot.

use super::Game;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use super::sunset::*;

pub(super) fn cooldown_ready(game: &Game<'_>, m: &Module, wait: &mut u32, need: f32) -> bool {
    if m.b("Wait for cooldown") {
        // The ticker the hand counts on a server (attack_ticks is the
        // local world's).
        game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= need
    } else if *wait > 0 {
        *wait -= 1;
        false
    } else {
        true
    }
}

// How Criticals makes a hit critical, when it can: a hop of so many blocks over two ticks (Packet, the mace's
// smash), or a wait for the fall (Jump, Mini jump, a hit while rising).
#[derive(Clone, Copy)]
enum Crit {
    Hop(f64),
    Jump,
    MiniJump,
    Fall,
}

// Criticals (Sunset's) for a hit now: never flying, gliding, riding, in water or lava or on a ladder. A mace (Mace
// smash) hops 1.501 blocks plus Additional height so the server counts a smash; on the ground the Mode; in the air,
// with Only on ground off, a hit waits for the fall. A hop needs the room above you, else Packet's goes back to 0.0625.
fn crit_plan(game: &Game<'_>, m: &Module) -> Option<Crit> {
    let p = game.player.as_ref()?;
    let item = game.held().map_or("", |s| &*s.item);
    let weapon = ["_sword", "_axe", "_spear", ":mace", ":trident"].iter().any(|w| item.ends_with(w));
    if (m.b("Only with a weapon") && !weapon)
        || p.flying
        || p.fall_flying
        || game.riding.is_some()
        || p.swimming
        || p.fluids.was_touching_water
        || p.is_in_lava()
        || p.last_climbable.is_some()
    {
        return None;
    }
    let extra = m.n("Additional height");
    let room = |h: f64| room_above(game, h);
    if m.b("Mace smash") && item.ends_with(":mace") && room(1.501 + extra) {
        return Some(Crit::Hop(1.501 + extra));
    }
    // In the air, a hit waits until you have fallen for two ticks (a server counts no fall on the first after a jump's
    // top, and Horizon's only after the tick it came in); after that it is critical as it is, unless the next move
    // lands: a hit is judged with the ground of its tick's move, so that one waits too.
    if !p.ground.on_ground {
        return (!m.b("Only on ground") && (p.velocity.y > -0.15 || lands_next(game))).then_some(Crit::Fall);
    }
    Some(match m.p("Mode") {
        "Jump" => Crit::Jump,
        "Mini jump" => Crit::MiniJump,
        _ => Crit::Hop(if room(0.0625 + extra) { 0.0625 + extra } else { 0.0625 }),
    })
}

// Nothing in the way of your box moved `h` blocks up (down if negative).
fn room_above(game: &Game<'_>, h: f64) -> bool {
    use horizon_core::geometry::Vec3;
    game.player.as_ref().is_some_and(|p| {
        horizon_client::collisions::SceneCollisions { world: &game.world, binding: game.collision, subject: None }
            .unobstructed(p.bounds().expand_towards(Vec3::new(0.0, h, 0.0)))
            .unwrap_or(false)
    })
}

// Falling, the next tick's move (the velocity the tick left you) puts you on the ground.
fn lands_next(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| !p.ground.on_ground && p.velocity.y < 0.0) && !room_above(game, game.player.as_ref().map_or(0.0, |p| p.velocity.y))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum AttackSource { KillAura, TriggerBot, TriggerReach, SilentAim, HitFlick }

#[derive(Clone, Copy, Debug)]
pub(super) struct AttackGuard {
    token: super::sunset_antibot::Token,
    pub(super) source: Option<AttackSource>,
}
impl AttackGuard {
    pub(super) fn id(self) -> u32 { self.token.id }
    pub(super) fn valid(self, game: &Game<'_>) -> bool {
        if !running() { return false; }
        if game.remote.is_none() || game.player.as_ref().is_none_or(|p| !p.health.health().is_finite() || p.health.health() <= 0.0) { return false; }
        if !super::sunset_antibot::current(self.token) { return false; }
        let Some(mob) = game.mobs.iter().find(|m| m.id == self.token.id && m.alive() && !m.removed) else { return false; };
        let Some(source) = self.source else { return true; };
        if !game.native_playing() || gui_open() || game.spectator() { return false; }
        let name = match source {
            AttackSource::KillAura => "Kill Aura", AttackSource::TriggerBot | AttackSource::TriggerReach => "Trigger Bot",
            AttackSource::SilentAim | AttackSource::HitFlick => "Aim Assist",
        };
        let Some(m) = with(|s| s.module(name).filter(|m| m.on).cloned()) else { return false; };
        if !super::sunset_util::combat_eligible(game, &m, mob) { return false; }
        match source {
            AttackSource::KillAura => super::sunset_aura::eligible(game, &m, mob.id),
            AttackSource::TriggerBot => wanted(&m, kind_of(mob), &mob.kind.id)
                && game.mob_hit().is_some_and(|(index, distance, _)| game.mobs[index].id == mob.id && game.before_block(distance)),
            AttackSource::TriggerReach => wanted(&m, kind_of(mob), &mob.kind.id),
            AttackSource::SilentAim | AttackSource::HitFlick => {
                let silent = matches!(source, AttackSource::SilentAim);
                let players_only = m.b(if silent { "Silent players only" } else { "Flick players only" });
                if !m.b(if silent { "Silent aim" } else { "Hit flick" })
                    || !super::sunset_combat::click_eligible(game, &m, mob, players_only)
                { return false; }
                let Some(eye) = game.eye_position() else { return false; };
                let b = mob.bounds();
                let (lo, hi) = (b.min_position(), b.max_position());
                let at = [eye[0].clamp(lo.x, hi.x), eye[1].clamp(lo.y, hi.y), eye[2].clamp(lo.z, hi.z)];
                super::sunset_util::dist(eye, at) <= 3.0 && super::sunset_util::clear_line(game, at, [i32::MIN; 3])
            }
        }
    }
}
pub(super) fn attack_guard(id: u32, source: AttackSource) -> Option<AttackGuard> {
    Some(AttackGuard { token: super::sunset_antibot::token(id)?, source: Some(source) })
}
pub(super) fn attack_for(game: &mut Game<'_>, id: u32, crits: bool, rotate: Option<(f32, f32)>, source: AttackSource) {
    if let Some(guard) = attack_guard(id, source) { attack_guarded(game, guard, crits, rotate); }
}
pub(super) fn attack(game: &mut Game<'_>, id: u32, crits: bool, rotate: Option<(f32, f32)>) {
    if let Some(token) = super::sunset_antibot::token(id) {
        attack_guarded(game, AttackGuard { token, source: None }, crits, rotate);
    }
}
pub(super) fn attack_guarded(game: &mut Game<'_>, guard: AttackGuard, crits: bool, rotate: Option<(f32, f32)>) {
    if !running() { return; }
    if !guard.valid(game) { return; }
    let plan = with(|s| s.module("Criticals").filter(|m| m.on).cloned()).filter(|_| crits).and_then(|m| crit_plan(game, &m));
    let Some(plan) = plan else {
        match rotate {
            Some((yaw, pitch)) => turn_then_checked("Attack", yaw, pitch, guard, move |game| queue_strike(game, guard, false)),
            None => queue_strike(game, guard, false),
        }
        return;
    };
    with(|s| {
        s.crit_target = Some(guard);
        s.crit_ticks = 0;
        s.crit_falls = 0;
        s.crit_again = false;
        s.crit_look = rotate;
    });
    if game.player.as_ref().is_some_and(|p| p.sprinting) && CRIT_SPRINT.swap(CRIT_SPRINT_STOPPED, Ordering::Relaxed) == 0 {
        sprint_command(game, 2);
    }
    start_crit(game, plan);
}

// The hop starts, or the jump, or the wait for the fall.
fn start_crit(game: &mut Game<'_>, plan: Crit) {
    match plan {
        Crit::Hop(height) => {
            CRIT_HOP.store(height.to_bits(), Ordering::Relaxed);
            // The look rides on the hop's packets, and the hit follows them.
            if let Some((yaw, pitch)) = with(|s| s.crit_look.take()) {
                turn("Attack", yaw, pitch);
            }
            CRIT_STAGE.store(1, Ordering::Relaxed);
            return;
        }
        Crit::Jump => CRIT_JUMP.store(true, Ordering::Relaxed),
        Crit::MiniJump => {
            if let Some(p) = game.player.as_mut() {
                p.velocity.y = 0.25;
            }
        }
        Crit::Fall => {}
    }
    CRIT_STAGE.store(CRIT_FALL, Ordering::Relaxed);
}

// Criticals on your own clicks (the attack hook): the hit the game would make on a mob, nearer than a block, a
// projectile it would turn or an object, waits as Kill Aura's does, and the click does nothing more now; while a hit
// waits, clicks do nothing. None: the game's own attack.
pub(super) fn crit_click(game: &mut Game<'_>) -> Option<bool> {
    let m = with(|s| s.module("Criticals").filter(|m| m.on && !m.b("Only Kill Aura")).cloned())?;
    if game.remote.is_none() || game.spectator() || game.clicks.miss_time > 0 {
        return None;
    }
    if CRIT_STAGE.load(Ordering::Relaxed) != 0 {
        return Some(false);
    }
    if game.held().is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some()) {
        return None;
    }
    let (i, distance, _) = game.mob_hit()?;
    let projectile = game.redirectable_hit().is_some_and(|(_, d)| d <= distance);
    let object = game.remote_object_hit(true).is_some_and(|hit| hit.distance < distance);
    if projectile || object || !game.before_block(distance) {
        return None;
    }
    crit_plan(game, &m)?;
    let id = game.mobs[i].id;
    attack(game, id, true, None);
    Some(false)
}

// Hits wait for the start of the next tick: the game's own attack goes out with the input, before the tick's move
// packet, and modules run after it. By then the server also has the look the last move packet carried. A critical
// hit is marked: the sprint it stopped comes back the tick after it goes. The weapon Auto Weapon wants for a hit goes
// in hand at the start of the next tick, before its move packet as a hotbar key does, and the hit a tick after that
// (Horizon's server hits with what was held as its tick began); it goes back at the start of the tick after the hit,
// or that many ticks later with Back after A delay (`back`: the slot, the weapon's slot it replaces if still held,
// and the tick it may go back). Nothing changes slot after a tick's move packet (Grim's Post) or after a hit in the
// same tick (Grim's PacketOrderE).
struct Strikes {
    pub(super) queue: Vec<(AttackGuard, bool)>,
    pub(super) weapon: Option<StrikeWeapon>,
    pub(super) back: Option<(usize, usize, u64)>,
}

// The weapon for the hits waiting: its slot, the delay before it goes back (None: it stays), and the slot it replaced
// once it is in hand.
#[derive(Clone, Copy)]
pub(super) struct StrikeWeapon {
    pub(super) slot: usize,
    back_after: Option<u64>,
    replaced: Option<usize>,
}

static STRIKES: Mutex<Strikes> = Mutex::new(Strikes { queue: Vec::new(), weapon: None, back: None });

fn strikes<R>(f: impl FnOnce(&mut Strikes) -> R) -> R {
    f(&mut STRIKES.lock().unwrap_or_else(|e| e.into_inner()))
}

// A new world or life, Sunset off: nothing waiting goes out, no slot goes back.
pub(super) fn drop_strikes() {
    strikes(|s| {
        s.queue.clear();
        s.weapon = None;
        s.back = None;
    });
}

pub(super) fn strike(game: &mut Game<'_>, id: u32) {
    if let Some(token) = super::sunset_antibot::token(id) {
        queue_strike(game, AttackGuard { token, source: None }, false);
    }
}

// Decided after the tick's move packet: the hit and its weapon both go at the start of the next tick, the slot first,
// as a hotbar key before a click (a slot change after the move packet is what Grim's Post check flags). The weapon is
// Auto Mace's mace while you fall, else Auto Weapon's.
fn queue_strike(game: &mut Game<'_>, guard: AttackGuard, critical: bool) {
    if !running() { return; }
    if !guard.valid(game) { if critical { crit_done(); } return; }
    let id = guard.token.id;
    let first = strikes(|s| s.queue.is_empty() && s.weapon.is_none());
    let weapon = first
        .then(|| super::sunset_auto_mace::strike_weapon(game, id).or_else(|| super::sunset_melee::strike_weapon(game, id)))
        .flatten()
        .map(|(slot, back_after)| StrikeWeapon { slot, back_after, replaced: None });
    strikes(|s| {
        s.queue.push((guard, critical));
        s.weapon = s.weapon.or(weapon);
    });
}

// A hit is on its way: the attack cooldown has not restarted yet, so nothing else may hit this tick.
pub(super) fn strike_waiting() -> bool {
    strikes(|s| !s.queue.is_empty())
}

// Hits go out this tick (not only their weapon).
pub(super) fn strike_due() -> bool {
    strikes(|s| !s.queue.is_empty() && s.weapon.is_none_or(|w| w.replaced.is_some()))
}

// The start of a tick after a hit, before any hit or use in it: the hotbar slot the hit's weapon replaced comes back.
pub(super) fn strike_weapon_back(game: &mut Game<'_>) {
    let ticks = game.ticks;
    if let Some((slot, weapon, _)) =
        strikes(|s| (s.queue.is_empty() && s.back.is_some_and(|b| ticks >= b.2)).then(|| s.back.take()).flatten())
        && game.selected == weapon
    {
        hold(game, slot);
    }
}

pub(super) fn send_strikes(game: &mut Game<'_>) {
    let mut queue = strikes(|s| std::mem::take(&mut s.queue));
    queue.retain(|(guard, critical)| {
        let keep = guard.valid(game);
        if !keep && *critical { crit_done(); }
        keep
    });
    strikes(|s| s.queue = queue);
    if strikes(|s| s.queue.is_empty()) {
        if let Some(StrikeWeapon { slot, back_after: Some(_), replaced: Some(replaced) }) = strikes(|s| s.weapon.take())
            && game.selected == slot
        { strikes(|s| s.back = s.back.or(Some((replaced, slot, game.ticks + 1)))); }
        return;
    }
    if let Some(slot) = strikes(|s| s.weapon.filter(|w| w.replaced.is_none()).map(|w| w.slot)) {
        let replaced = game.selected;
        hold(game, slot);
        strikes(|s| s.weapon = s.weapon.map(|w| StrikeWeapon { replaced: Some(replaced), ..w }));
        return;
    }
    if let Some(StrikeWeapon { slot, back_after: Some(delay), replaced: Some(replaced) }) = strikes(|s| s.weapon.take())
        && game.selected == slot
    {
        strikes(|s| s.back = s.back.or(Some((replaced, slot, game.ticks + 1 + delay))));
    }
    for (guard, critical) in strikes(|s| std::mem::take(&mut s.queue)) {
        if game.remote.is_some() {
            strike_now(game, guard);
        }
        if critical {
            crit_done();
        }
    }
}

fn strike_now(game: &mut Game<'_>, guard: AttackGuard) {
    if !guard.valid(game) { return; }
    let id = guard.token.id;
    // Gone since the hit was decided: the id may be someone else's by now. Fake Player's is no one's on the server.
    if !game.mobs.iter().any(|m| m.id == id && m.alive() && !m.removed) || super::sunset_parity::fake(id) {
        return;
    }
    // A spear stabs along the look instead, as the game's own left click does.
    if let Some(kinetic) = game.held().and_then(|s| horizon_client::spear::Kinetic::of(&s.item)) {
        if !game.cannot_attack_with_item(game.hand_strength.ticker, 0) {
            game.send(S::PlayerAction(play::PlayerAction { action: 8, pos: [0; 3], direction: 0, sequence: 0 }));
            game.hand_strength.reset_attack();
            game.swing_hand(horizon_client::first_person::Hand::Main, kinetic.attack_animation());
            game.play_at_feet(kinetic.attack_sound);
        }
    } else {
        super::sunset_melee::spear_pass(game, id);
        // An axe hit at a raised shield readies Auto Mace's Stun slam, as your own click does.
        super::sunset_auto_mace::before_strike(game);
        // A 1.8 client swings, then hits (and a 1.8 server's anticheat may want the swing first); 1.9+ hits first.
        let legacy = legacy_combat(game, guard.source);
        if legacy {
            super::sunset_hands::strike_swing(game);
        }
        game.send(S::Attack(play::Attack { id: id as i32 }));
        game.swing_arm();
        if !legacy {
            game.send(S::Punch(Empty));
        }
        game.reset_attack_strength();
    }
}

// Hits go the 1.8 way (swing first) by the Combat version of the module that sent them: Trigger Bot's own for its hits,
// Kill Aura's for the rest. Auto decides by your attack speed.
fn legacy_combat(game: &Game<'_>, source: Option<AttackSource>) -> bool {
    let speed = game.main_hand_attack_speed();
    let owner = if matches!(source, Some(AttackSource::TriggerBot | AttackSource::TriggerReach)) { "Trigger Bot" } else { "Kill Aura" };
    with(|s| {
        let setting = s.module(owner).map_or("Auto", |m| m.p("Combat version"));
        super::sunset_aura_timing::version(setting, speed).0 == super::sunset_aura_timing::Version::Legacy
    })
}

// The Criticals hop, a tick at a time. The regular position packet of a tick
// becomes the hop (see `outgoing`); when a tick has none, this sends it. Waiting
// for the fall, the hit goes on the second tick in a row that took you down off
// the ground (a server counts no fall on the first one after a jump's top), or
// anyway once you land again or a second passes.
pub(super) fn crit_tick(game: &mut Game<'_>) {
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    if stage == 0 {
        return;
    }
    // The tick of the hit had no move packet to keep off the ground: nothing to do.
    if stage == CRIT_LAND {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        return;
    }
    let ticks = with(|s| {
        s.crit_ticks += 1;
        s.crit_ticks
    });
    if stage == CRIT_FALL {
        let Some(p) = game.player.as_ref() else {
            return;
        };
        let (ground, falling) = (p.ground.on_ground, p.position.y < game.previous_position.y);
        let falls = with(|s| {
            s.crit_falls = if falling && !ground { s.crit_falls + 1 } else { 0 };
            s.crit_falls
        });
        // Landed first (the fall's last move would have had the hit judged on the ground): it starts again from the
        // ground, the way Mode makes it critical there.
        if ground && ticks > 3 && !with(|s| s.crit_again) {
            let plan = with(|s| s.module("Criticals").filter(|m| m.on).cloned()).and_then(|m| crit_plan(game, &m));
            if let Some(plan @ (Crit::Hop(_) | Crit::Jump | Crit::MiniJump)) = plan {
                with(|s| {
                    s.crit_again = true;
                    s.crit_ticks = 0;
                });
                start_crit(game, plan);
                return;
            }
        }
        if (falls >= 2 && !lands_next(game)) || (ground && ticks > 3) || ticks > 20 {
            CRIT_STAGE.store(0, Ordering::Relaxed);
            match with(|s| s.crit_target.take().map(|id| (id, s.crit_look.take()))) {
                Some((guard, Some((yaw, pitch)))) => queue_turn("Attack", yaw, pitch, Some(guard), true, move |game| crit_strike(game, guard)),
                Some((id, None)) => crit_strike(game, id),
                None => crit_done(),
            }
        }
        return;
    }
    if ticks > 6 {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        with(|s| s.crit_target = None);
        crit_done();
        return;
    }
    if stage < CRIT_HIT && !MOVED_THIS_TICK.load(Ordering::Relaxed) {
        let Some(p) = game.player.as_ref() else {
            return;
        };
        let (x, y, z) = (p.position.x, p.position.y, p.position.z);
        // outgoing makes it the hop and moves the stage on.
        game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags: 0 }));
    }
    if CRIT_STAGE.load(Ordering::Relaxed) == CRIT_HIT {
        CRIT_STAGE.store(CRIT_LAND, Ordering::Relaxed);
        match with(|s| s.crit_target.take()) {
            Some(id) => crit_strike(game, id),
            None => crit_done(),
        }
    }
}

// A critical hit, with the sprint stopped since it was planned (CRIT_SPRINT): it gives up sprinting's extra
// knockback, as a critical one does in vanilla.
fn crit_strike(game: &mut Game<'_>, guard: AttackGuard) {
    queue_strike(game, guard, true);
}

// The critical hit went, or none will: the sprint Criticals stopped goes back on next tick.
pub(super) fn crit_done() {
    let _ = CRIT_SPRINT.compare_exchange(CRIT_SPRINT_STOPPED, CRIT_SPRINT_DUE, Ordering::Relaxed, Ordering::Relaxed);
}

// Before the tick after the hit (its own tick's packets have gone): the sprint Criticals stopped goes back on, if you
// still sprint (else the server already has it right).
pub(super) fn crit_sprint_back(game: &mut Game<'_>) {
    if CRIT_SPRINT.load(Ordering::Relaxed) == CRIT_SPRINT_DUE {
        CRIT_SPRINT.store(0, Ordering::Relaxed);
        if game.player.as_ref().is_some_and(|p| p.sprinting) {
            sprint_command(game, 1);
        }
    }
}

// START_SPRINTING (1) or STOP_SPRINTING (2) for the player.
fn sprint_command(game: &mut Game<'_>, action: i32) {
    let me = game.remote.as_ref().map_or(0, |r| r.entity_id);
    game.send(S::PlayerCommand(play::PlayerCommand { id: me, action, data: 0 }));
}

pub(super) fn kill_aura(game: &mut Game<'_>, m: &Module, crits: bool) {
    super::sunset_aura::tick(game, m, crits);
}

pub(super) fn trigger_bot(game: &mut Game<'_>, m: &Module, crits: bool) {
    use winit::event::MouseButton;
    if !game.native_playing()
        || game.remote.is_none()
        || (game.using.is_some() && !super::sunset_hands::shielding(game))
        || CRIT_STAGE.load(Ordering::Relaxed) != 0
        || strike_waiting()
        || (m.b("Only on left click") && !(game.physical_buttons.contains(&MouseButton::Left) || game.left))
    {
        return;
    }
    // Nobody under the crosshair: Auto Spear's Extended reach may give a target along your look (Works with Trigger
    // Bot), hit with its spear's stab.
    let reach = game.mob_hit().is_none().then(|| super::sunset_melee::reach_trigger(game)).flatten();
    let Some(i) = game.mob_hit().map(|(i, ..)| i).or_else(|| reach.and_then(|id| game.mobs.iter().position(|t| t.id == id))) else {
        with(|s| s.trigger_missed = None);
        return;
    };
    let mob = &game.mobs[i];
    let (id, kind) = (mob.id, kind_of(mob));
    let want = wanted(m, kind, &mob.kind.id);
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    let player = kind == Kind::Player;
    let identity = super::sunset_antibot::token(id);
    if with(|s| s.trigger_missed.is_some_and(|t| t.id != id || !super::sunset_antibot::current(t))) {
        with(|s| s.trigger_missed = None);
    }
    if !want || (player && is_friend(&friends, &names, id)) || !super::sunset_util::combat_eligible(game, m, mob) {
        return;
    }
    if m.b("Sticky target") && trigger_sticky_elsewhere(game, m, id) {
        return;
    }
    // Crit timing waits out the rise of a jump to hit as you fall; Crits only hits only then (with a sword, axe,
    // spear, mace or trident in hand).
    if super::sunset_combat::holds_weapon(game) && let Some(p) = game.player.as_ref() {
        let falling = !p.ground.on_ground && p.fall_distance > 0.0 && p.velocity.y < 0.0;
        if (m.b("Crit timing") && !p.ground.on_ground && !falling) || (m.b("Crits only") && !falling) {
            return;
        }
    }
    let mut wait = with(|s| s.trigger_wait);
    let ready = cooldown_ready(game, m, &mut wait, with(|s| s.trigger_need));
    with(|s| s.trigger_wait = wait);
    if !ready || with(|s| s.trigger_missed.is_some_and(|t| Some(t) == identity)) {
        return;
    }
    let mut rng = super::sunset_rotation::Rng::new(game.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(id));
    // Miss chance: a target rolled as missed is let go until the crosshair leaves it or another is under it.
    if rng.next() * 100.0 < m.n("Miss chance (%)") {
        with(|s| s.trigger_missed = identity);
        return;
    }
    if reach == Some(id) {
        if let Some(guard) = attack_guard(id, AttackSource::TriggerReach) {
            super::sunset_melee::reach_strike(guard);
        }
    } else {
        attack_for(game, id, crits, None, AttackSource::TriggerBot);
    }
    let (lo, hi) = (m.n("Shortest cooldown (%)"), m.n("Longest cooldown (%)").max(m.n("Shortest cooldown (%)")));
    let need = ((lo + rng.next() * (hi - lo)) / 100.0) as f32;
    with(|s| {
        s.trigger_wait = m.n("Delay (ticks)") as u32;
        s.trigger_need = need;
        if m.b("Sticky target") {
            s.trigger_sticky = identity;
        }
    });
}

// Sticky target: another target is locked (the first one hit), still alive and within Sticky range of you.
fn trigger_sticky_elsewhere(game: &Game<'_>, m: &Module, id: u32) -> bool {
    let Some(locked) = with(|s| s.trigger_sticky) else {
        return false;
    };
    let here = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]);
    let held = super::sunset_antibot::current(locked) && game.mobs.iter().find(|mob| mob.id == locked.id).is_some_and(|mob| {
        mob.health > 0.0
            && super::sunset_util::combat_eligible(game, m, mob)
            && wanted(m, kind_of(mob), &mob.kind.id)
            && here.is_some_and(|h| super::sunset_util::dist(h, [mob.position.x, mob.position.y, mob.position.z]) <= m.n("Sticky range"))
    });
    if !held {
        with(|s| s.trigger_sticky = None);
    }
    held && locked.id != id
}

// A totem in the hotbar goes to the offhand by selecting it and pressing the
// swap key, as a player would; then the old slot comes back.

// Moves a totem from the hotbar to the offhand: select it, swap hands, select
// the old slot again, then give the server a few ticks to answer.
