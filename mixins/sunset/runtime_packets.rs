// Runtime part (see runtime.rs): outgoing packets: moves, the look and fall the server sees, turns and Blink.

use super::Game;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use super::sunset::*;

// ---------------------------------------------------------------- packets

// What the send hook does with an outgoing packet.
pub(super) enum Out {
    Pass,
    Drop,
    Replace(S),
}

// A move packet taken apart: where (if it says), the look (if it says), and its flags (bit 0 on the ground, bit 1
// against a wall).
#[derive(Clone, Copy, PartialEq)]
struct Move {
    pub(super) at: Option<[f64; 3]>,
    pub(super) look: Option<(f32, f32)>,
    pub(super) flags: u8,
}

impl Move {
    pub(super) fn of(packet: &S) -> Option<Self> {
        Some(match packet {
            S::MovePlayerPos(m) => Self { at: Some([m.x, m.y, m.z]), look: None, flags: m.flags },
            S::MovePlayerPosRot(m) => Self { at: Some([m.x, m.y, m.z]), look: Some((m.yaw, m.pitch)), flags: m.flags },
            S::MovePlayerRot(m) => Self { at: None, look: Some((m.yaw, m.pitch)), flags: m.flags },
            S::MovePlayerStatus(m) => Self { at: None, look: None, flags: m.flags },
            _ => return None,
        })
    }
    fn packet(self) -> S {
        let flags = self.flags;
        match (self.at, self.look) {
            (Some([x, y, z]), Some((yaw, pitch))) => S::MovePlayerPosRot(play::MovePlayerPosRot { x, y, z, yaw, pitch, flags }),
            (Some([x, y, z]), None) => S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }),
            (None, Some((yaw, pitch))) => S::MovePlayerRot(play::MovePlayerRot { yaw, pitch, flags }),
            (None, None) => S::MovePlayerStatus(play::MovePlayerStatus { flags }),
        }
    }
}

// No Slow's No item release (Sunset's NoItemRelease): letting go of the use button never tells the server, with a
// menu open or what Release items lists in hand (`swords`, `food`, `potions` or item names), so it goes on using it.
fn kept_using(game: &Game<'_>) -> bool {
    let Some(m) = with(|s| s.module("No Slow").filter(|m| m.on && m.b("No item release")).cloned()) else {
        return false;
    };
    if gui_open() || !matches!(game.screen, super::Screen::Playing) {
        return true;
    }
    let list = m.t("Release items");
    game.held().is_some_and(|h| {
        let tagged = |tag: &str| game.res.items().item_tags.get(tag).is_some_and(|t| t.contains(&*h.item));
        super::sunset_util::listed(list, &h.item)
            || (super::sunset_util::listed(list, "swords") && tagged("minecraft:swords"))
            || (super::sunset_util::listed(list, "food") && game.res.items().get(&h.item).is_some_and(|i| i.food.is_some()))
            || (super::sunset_util::listed(list, "potions") && h.item.ends_with("potion"))
    })
}

// Outgoing packets: Blink holds moves, Anti Hunger hides sprinting and the ground, No Fall claims the ground as you
// fall (or, in No ground, never), Criticals hops, Flight's anti kick sinks, no move claims the ground with a fall that
// is not yours to pay (settle_fall), and the look modules asked for (turn_then) rides along. Each is a change to the
// tick's own move packet, never another one.
pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> Out {
    if !running() { return Out::Pass; }
    if let Some(changed) = super::sunset_world_parity::outgoing(game, packet) {
        return Out::Replace(changed);
    }
    if let S::PlayerAction(a) = packet
        && a.action == 6
        && !super::sunset_hands::owned_block_packet()
        && kept_using(game)
    {
        return Out::Drop;
    }
    if ANTI_SPRINT.load(Ordering::Relaxed) && !super::sunset_gui_move::stopping()
        && let S::PlayerCommand(c) = packet
        && (c.action == 1 || c.action == 2)
    {
        return Out::Drop;
    }
    // A teleport: the server goes on counting from where it put you, though the moves after it are held (Blink) or
    // the next one is far away.
    if let S::AcceptTeleportation(t) = packet {
        fall(|f| f.last = Some(t.y));
        // Put back mid-hop (the hop refused, or anything else): no more of it goes from the new place; the hit
        // follows now, uncritical as it may be (crit_tick).
        if (1..CRIT_HIT).contains(&CRIT_STAGE.load(Ordering::Relaxed)) {
            CRIT_STAGE.store(CRIT_HIT, Ordering::Relaxed);
        }
        // The server set your look: what it has now is not the last one sent.
        set_last_look(None);
    }
    // A mace hit with a smash's fall counted: the server ends that fall if the smash lands (smash_confirmed).
    if let S::Attack(a) = packet
        && a.id >= 0
        && game.held().is_some_and(|s| &*s.item == "minecraft:mace")
        && !game.player.as_ref().is_some_and(|p| p.fall_flying)
        && fall(|f| f.counted) > 1.5
    {
        SMASH_SENT.store(game.ticks.max(1), Ordering::Relaxed);
    }
    let Some(sent) = Move::of(packet) else {
        return Out::Pass;
    };
    if BLINK.load(Ordering::Relaxed) && !super::sunset_gui_move::stopping() {
        BLINK_HELD.store(true, Ordering::Relaxed);
        return Out::Drop;
    }
    if super::sunset_gui_move::stopping() {
        // This native movement supersedes Blink's held endpoint; never add a second move to drain it.
        BLINK_HELD.store(false, Ordering::Relaxed);
    }
    MOVED_THIS_TICK.store(true, Ordering::Relaxed);
    let Some(p) = game.player.as_ref() else {
        return Out::Pass;
    };
    let mut m = sent;
    let here = [p.position.x, p.position.y, p.position.z];
    // What a module tells the server of the ground, if it changes it; and whether No Fall is on to spare you the fall.
    let ground;
    let mut spared = false;
    // Criticals: this tick's move is the hop (stage 1: up, 2: half way down, 3: back down, then the hit's tick where
    // it ended), all off the ground. A jump this tick (your key, Auto Jump) would carry it up with you: the hit waits
    // for the jump's fall instead.
    if (1..CRIT_HIT).contains(&CRIT_STAGE.load(Ordering::Relaxed)) && !p.ground.on_ground && p.velocity.y > 0.0 {
        CRIT_STAGE.store(CRIT_FALL, Ordering::Relaxed);
    }
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    let hopping = (stage > 0 && stage < CRIT_HIT) || stage == CRIT_LAND;
    if hopping {
        let [x, y, z] = m.at.unwrap_or(here);
        let lift = crit_lift(stage, f64::from_bits(CRIT_HOP.load(Ordering::Relaxed)), CRIT_DROP.load(Ordering::Relaxed));
        m.at = Some([x, y + lift, z]);
        m.flags = 0;
        ground = Some(false);
        // The landing stays off the ground until the hit has gone (a weapon swapped in for it delays it a tick).
        let next = match stage {
            CRIT_LAND if critical_waiting() => CRIT_LAND,
            CRIT_LAND => 0,
            _ => stage + 1,
        };
        CRIT_STAGE.store(next, Ordering::Relaxed);
    } else {
        // Flight's Packet anti kick: the move carries the position, sunk 0.0313 below the last height sent while it
        // is due and you are in the air (Sunset's antiKickPacket).
        if FLY_PACKET.load(Ordering::Relaxed) && super::sunset_flight::damage_open() {
            fly_down(game, &mut m, here);
        }
        if let Some(dy) = super::sunset_flight::offset() {
            let mut at = m.at.unwrap_or(here);
            at[1] += dy;
            m.at = Some(at);
        }
        // The server adds each move's drop to the fall, and a move that says you are on the ground costs what has
        // added up beyond three blocks. No Fall's Packet mode claims the ground on every move down, so the count
        // never passes one move's drop (Sunset's waits for 0.5 blocks a tick, which leaves a fall the server counted
        // before to hurt), and one move drops 3.92 at most, which costs nothing. No ground never claims it. Flying that
        // way claims it all the time. settle_fall has the last word.
        // Anti Hunger hides the ground, but not while you break a block: off the ground the server mines five times
        // slower and refuses the break, so the block comes back (Sunset's skips it then too). Nor once the server
        // has counted a fall, or this move adds one: hidden, every jump and step down would add up to one fall that
        // the first move you are on the ground for makes you pay.
        let falling = !p.ground.on_ground && !p.flying && !p.fall_flying;
        let no_fall = NO_FALL.load(Ordering::Relaxed);
        let paused = game.creative || (NO_FALL_MACE.load(Ordering::Relaxed) && game.held().is_some_and(|s| s.item.ends_with(":mace")));
        spared = no_fall != 0 && !paused;
        let flight_ground = FLIGHT_GROUND.load(Ordering::Relaxed) && super::sunset_flight::damage_open();
        ground = if flight_ground || (GLIDE_GROUND.load(Ordering::Relaxed) && !p.flying) {
            Some(true)
        } else if no_fall == 1 && !paused && falling && p.velocity.y < 0.0 && !crit_falling() {
            Some(true)
        } else if no_fall == 2 && !paused {
            Some(false)
        } else if ANTI_GROUND.load(Ordering::Relaxed)
            && (p.fall_distance <= 0.0 || p.flying)
            && super::remote::sunset_remote::breaking(game).is_none()
            && fall_counted(&m) == 0.0
        {
            Some(false)
        } else {
            None
        };
        if let Some(g) = ground {
            m.flags = (m.flags & !1) | u8::from(g);
        }
    }
    // Flight's Damage fly dealing itself fall damage: this tick's height of its fall, off the ground, or its landing.
    // That fall is yours to pay (settle_fall lets it land).
    let own_fall = super::sunset_flight::fall_move();
    if let Some((y, lands)) = own_fall {
        let mut at = m.at.unwrap_or(here);
        at[1] = y;
        m.at = Some(at);
        m.flags = (m.flags & !1) | u8::from(lands);
    }
    let told = settle_fall(game, &m, sent.flags & 1 != 0, ground == Some(true) || spared, own_fall.is_some());
    m.flags = (m.flags & !1) | u8::from(told);
    if ground.is_some() || own_fall.is_some() {
        horizon_client::player_sync::sunset_sync::told_ground(told);
    }
    // The look a module asked for, else the player's own back after one (not during the Criticals hop, whose hit
    // follows it).
    let own = (game.yaw, game.pitch);
    let last = last_look();
    let look = turns(|t| {
        if let Some(head) = t.queue.first().filter(|_| !t.carried) {
            t.carried = true;
            let look = wire_look(last, own, head.look);
            t.restore = look != own;
            Some(look)
        } else if t.restore && !hopping {
            t.restore = false;
            Some(own)
        } else {
            None
        }
    });
    if let Some(look) = look {
        m.look = Some(look);
    }
    // A look the server already has is left out (as Sunset's look through the game's own sender never repeats one):
    // the same look twice in a row is what Grim's AimDuplicateLook catches.
    if m.look.is_some() && m.look == last_look() {
        m.look = None;
    }
    if m.look.is_some() {
        set_last_look(m.look);
    }
    if m == sent { Out::Pass } else { Out::Replace(m.packet()) }
}

// How far above where you are a Criticals hop's move puts you, by its stage: the top on the first move, then a crit
// hop half way and back down (each move after the top drops, so even Slow Falling, which keeps only the last move's
// fall, leaves one); a smash hop (`drop`) all the way down on its second move, so the whole drop is counted a tick
// before its hit. The landing (CRIT_LAND) and any other stage: none.
pub(super) fn crit_lift(stage: u8, hop: f64, drop: bool) -> f64 {
    match stage {
        1 => hop,
        2 if !drop => hop / 2.0,
        _ => 0.0,
    }
}

// The last look a move packet carried to the server (None after a teleport, which sets it).
static LAST_LOOK: Mutex<Option<(f32, f32)>> = Mutex::new(None);

pub(super) fn last_look() -> Option<(f32, f32)> {
    *LAST_LOOK.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn set_last_look(look: Option<(f32, f32)>) {
    *LAST_LOOK.lock().unwrap_or_else(|e| e.into_inner()) = look;
}

// The yaw a look asked for goes out as the game's own does: unbounded and continuous, never folded into ±180 (a fold
// is a 360-degree turn to the server, Grim's AimModulo360). It is taken nearest your own yaw (your own exactly when
// that is what was asked), then nearest the last one sent.
fn wire_look(last: Option<(f32, f32)>, own: (f32, f32), asked: (f32, f32)) -> (f32, f32) {
    let wrap = |d: f32| (d + 180.0).rem_euclid(360.0) - 180.0;
    let mut yaw = if asked.0 == own.0 { own.0 } else { own.0 + wrap(asked.0 - own.0) };
    if let Some((sent, _)) = last
        && (yaw - sent).abs() > 180.0
    {
        yaw = sent + wrap(yaw - sent);
    }
    (yaw, asked.1.clamp(-90.0, 90.0))
}

// The yaw the next move packet will carry for a look asked now with `yaw`: what a use packet sent with it says.
pub(super) fn wire_yaw(game: &Game<'_>, yaw: f32) -> f32 {
    wire_look(last_look(), (game.yaw, game.pitch), (yaw, 0.0)).0
}

// ---- the fall the server counts
//
// A server adds up the drop of each move told off the ground, and a move told on it costs what that comes to beyond
// three blocks (a point of health for each whole block past that) and starts it again; water, a ladder or creative
// flight start it again too (Entity.checkFallDamage, LivingEntity.handleOnClimbable). Sunset keeps the same count
// of the moves it lets go, so that no move claims the ground with a fall the server would make you pay for that was
// not yours to pay: one it hid the ground you landed on through (No Fall's No ground, the Criticals hop), or one
// Flight, Glide or No Fall would claim the ground on. That fall is kept: every move goes off the ground until
// something starts the count again, or you die.

pub(super) struct Fall {
    // The server's count, in blocks.
    pub(super) counted: f64,
    // The height the last move told, or the last teleport put you at; None at first.
    pub(super) last: Option<f64>,
    // Part of the count is not yours to pay.
    pub(super) kept: bool,
    // The count before the last move: what the server has surely counted by the time a hit sent next tick arrives
    // (Horizon's counts a tick's moves after its packets, so the last move may come in with the hit).
    pub(super) before: f64,
}

static FALL: Mutex<Fall> = Mutex::new(Fall { counted: 0.0, last: None, kept: false, before: 0.0 });

// The most the server lets you land with for nothing: four blocks costs a point (LivingEntity.calculateFallPower adds
// 1.0E-6 to the fall). A move at full falling speed drops 3.92.
const FREE_FALL: f64 = 3.99;

pub(super) fn fall<R>(f: impl FnOnce(&mut Fall) -> R) -> R {
    f(&mut FALL.lock().unwrap_or_else(|e| e.into_inner()))
}

// What the server will have counted after this move, if it goes off the ground.
fn fall_counted(m: &Move) -> f64 {
    fall(|f| f.counted + drop_of(f, m))
}

fn drop_of(f: &Fall, m: &Move) -> f64 {
    match (f.last, m.at) {
        (Some(last), Some([_, y, _])) => (last - y).max(0.0),
        _ => 0.0,
    }
}

// The count after a move that tells the server `told` of the ground (`real` is what the game would tell it, and
// `claimed` a module claiming it, or No Fall on): the ground the move should tell. Gliding, a move that drops less
// than half a block brings the count down to one (LivingEntity.travelFallFlying), but a steep dive adds up: No Fall
// can't claim the ground in the glide (the server would end it), so the landing is kept.
fn settle_fall(game: &Game<'_>, m: &Move, real: bool, claimed: bool, owned: bool) -> bool {
    let (Some(p), Some(_)) = (game.player.as_ref(), game.remote.as_ref()) else {
        return m.flags & 1 != 0;
    };
    let cleared = fall_cleared(game);
    fall(|f| {
        let drop = drop_of(f, m);
        let counted = if p.fall_flying && drop < 0.5 { (f.counted + drop).min(1.0) } else { f.counted + drop };
        f.before = f.counted;
        if let Some([_, y, _]) = m.at {
            f.last = Some(y);
        }
        let mut told = m.flags & 1 != 0;
        if cleared {
            (f.counted, f.kept, f.before) = (0.0, false, 0.0);
            return told;
        }
        if told && counted >= FREE_FALL && (f.kept || claimed) && !owned {
            told = false;
            f.kept = true;
        } else if !told && real && counted > 0.0 && !owned {
            f.kept = true;
        }
        if told {
            (f.counted, f.kept) = (0.0, false);
        } else {
            f.counted = counted;
        }
        told
    })
}

// Water, a ladder or creative flight: the server starts the count again each tick there, moves or no moves.
pub(super) fn fall_cleared(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    p.fluids.was_touching_water
        || (game.creative && p.flying)
        || game.spectator()
        || game.block_in_tag("minecraft:climbable", &game.block_name(feet))
}

// Nothing of a fall is counted (Damage fly's self damage adds exactly its own).
pub(super) fn fall_clean() -> bool {
    fall(|f| f.counted == 0.0 && !f.kept)
}

// The tick a smash hit went (0: none waiting for its answer).
static SMASH_SENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// The server's answer to a smash hit: your motion set to 0.01 up at once (MaceItem.hurtEnemy), or Wind Burst's
// explosion pushing you up (it only goes off after a smash landed). A smash ends the fall, so Sunset's count ends
// too; else a big smash hop's fall would stay counted: every move would keep off the ground (settle_fall) for
// nothing, and a break off the ground is five times slower.
fn smash_confirmed(game: &Game<'_>, confirms: bool) {
    let sent = SMASH_SENT.load(Ordering::Relaxed);
    if sent == 0 {
        return;
    }
    if game.ticks.saturating_sub(sent) > 5 {
        SMASH_SENT.store(0, Ordering::Relaxed);
    } else if confirms {
        SMASH_SENT.store(0, Ordering::Relaxed);
        fall(|f| (f.counted, f.kept, f.before) = (0.0, false, 0.0));
    }
}

// Death: the next body has fallen nowhere.
pub(super) fn fall_died() {
    fall(|f| *f = Fall { counted: 0.0, last: None, kept: false, before: 0.0 });
}

// Flight's Packet anti kick on the move: while it is due and you are in the air, the height sinks 0.0313 below the
// last one sent (a move without a position gets the player's).
fn fly_down(game: &Game<'_>, m: &mut Move, here: [f64; 3]) {
    let mut at = m.at.unwrap_or(here);
    let bits = FLY_LAST_Y.load(Ordering::Relaxed);
    let last = f64::from_bits(bits);
    let sinking = at[1] >= last || last - at[1] < 0.03130;
    if FLY_DOWN.load(Ordering::Relaxed) && bits != u64::MAX && sinking && in_the_air(game) {
        at[1] = last - 0.03130;
        m.at = Some(at);
    } else {
        FLY_LAST_Y.store(at[1].to_bits(), Ordering::Relaxed);
    }
}

// ---- the look the server sees
//
// Rotate in Kill Aura, Nuker, Scaffold and the rest: a module asks for a look and hands over what to do once the server
// has it. The look rides on the tick's own move packet, or in a tick without one on the single one Sunset sends at
// its end, and what was asked runs after that packet has gone (Sunset's Rotations). Servers kick for a second move
// packet in a tick, and some check the look of a click.

struct Deed {
    pub(super) guard: Option<AttackGuard>,
    pub(super) critical: CriticalCompletion,
    pub(super) run: Box<dyn for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send>,
}
pub(super) struct CriticalCompletion(pub(super) bool);
impl Drop for CriticalCompletion {
    fn drop(&mut self) { if self.0 { crit_done(); } }
}

pub(super) struct Ask {
    who: &'static str,
    pub(super) look: (f32, f32),
    deeds: Vec<Deed>,
    // Asked this tick: another ask of the same module joins it instead of replacing it.
    pub(super) fresh: bool,
}

pub(super) struct Turns {
    // Oldest first, one a module; one look goes out a tick.
    pub(super) queue: Vec<Ask>,
    // This tick's move packet carried the first ask's look.
    pub(super) carried: bool,
    // A look went out that is not the player's own: the next move packet puts the player's back.
    pub(super) restore: bool,
}

static TURNS: Mutex<Turns> = Mutex::new(Turns { queue: Vec::new(), carried: false, restore: false });

pub(super) fn turns<R>(f: impl FnOnce(&mut Turns) -> R) -> R {
    f(&mut TURNS.lock().unwrap_or_else(|e| e.into_inner()))
}

// The packets that came since the last tick, to what Crystal Aura and Anchor Aura keep track of.
pub(super) fn combat_packets(game: &Game<'_>, packets: &[horizon_protocol::packets::play::Clientbound]) {
    use horizon_protocol::packets::play::Clientbound as C;
    for packet in packets {
        super::sunset_loot_guard::packet(game, packet);
        super::sunset_prediction::packet(game, packet);
        super::sunset_crystal_aura::packet(game, packet);
        if matches!(packet, C::Login(_) | C::Respawn(_) | C::PlayerPosition(_)) {
            super::sunset_combat_wire::reset_look();
        }
        if matches!(packet, C::Login(_) | C::Respawn(_)) {
            super::sunset_flight::reset();
        }
        match packet {
            C::DamageEvent(p) if game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) => super::sunset_flight::hurt(),
            C::SetHealth(p) => super::sunset_flight::health(p.health),
            _ => {}
        }
        super::sunset_combat_inventory::packet(game, packet);
        super::sunset_gear::packet(game, packet);
        super::sunset_throws::packet(game, packet);
        super::sunset_effects::packet(game, packet);
        super::recipe_book::sunset_recipes::packet(packet);
        if let C::Explode(e) = packet
            && e.knockback.is_some_and(|k| k[1] > 0.0)
        {
            smash_confirmed(game, true);
        }
        if let C::SetEntityMotion(p) = packet {
            super::sunset_melee::note_velocity(game, p.id, p.velocity);
            if game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                smash_confirmed(game, (p.velocity[1] - 0.01).abs() < 0.002);
                if with(|s| s.module("Speed").is_some_and(|m| m.on && m.p("Mode") == "Watchdog")) {
                    with(|s| s.watchdog.knocked(game.ticks, p.velocity));
                }
                super::sunset_flight::knocked(p.velocity);
                super::sunset_lag::knocked();
                super::sunset_render::no_slow_knocked();
            }
        }
    }
}

// Crystal Aura, Anchor Aura and the combat macros let go of what they hold: the hand they switched, a look waiting.
pub(super) fn combat_cleanup(game: &mut Game<'_>) {
    reset_targets(game);
    super::sunset_combat_control::prepare_cleanup(game);
    super::sunset_combat_macros::reset(game);
    super::sunset_pvp_bot::release(game);
    drop_strikes();
    super::sunset_melee::cancel(game);
    super::sunset_auto_mace::cancel();
    super::sunset_aura::disabled(game, false);
    super::sunset_combat_inventory::cancel(game);
    super::sunset_combat_control::reset();
    super::sunset_crystal_aura::disabled(game);
    super::sunset_anchor_aura::disabled(game);
    super::sunset_cart::reset(game);
    super::sunset_throws::reset(game);
    super::sunset_anchor_catch::reset(game);
}

// A scene change invalidates target caches as well as actions carrying old IDs.
pub(super) fn reset_targets(game: &mut Game<'_>) {
    super::sunset_autoclicker::reset();
    drop_strikes();
    super::sunset_aura::clear_targets(game);
    super::sunset_combat::reset_aim();
    super::sunset_click_fix::reset();
    for who in ["Attack", "Hit flick", "Kill Aura"] { cancel_turns(who); }
    CRIT_STAGE.store(0, Ordering::Relaxed);
    CRIT_SPRINT.store(0, Ordering::Relaxed);
    CRIT_JUMP.store(false, Ordering::Relaxed);
    with(|s| {
        s.crit_target = None;
        s.crit_look = None;
        s.trigger_sticky = None;
        s.trigger_missed = None;
    });
}

pub(super) fn revalidate_targets(game: &Game<'_>) {
    super::sunset_click_fix::revalidate(game);
    let pending = with(|s| s.crit_target);
    if pending.is_some_and(|guard| !guard.valid(game)) {
        with(|s| { s.crit_target = None; s.crit_look = None; });
        CRIT_STAGE.store(0, Ordering::Relaxed);
        CRIT_JUMP.store(false, Ordering::Relaxed);
        crit_done();
        cancel_turns("Attack");
    }
    // Validate outside the queue lock: eligibility consults the shared module
    // settings and the classifier, neither of which may call a held deed.
    let mut queue = turns(|t| std::mem::take(&mut t.queue));
    for ask in &mut queue {
        if ask.deeds.first().is_some_and(|deed| deed.guard.is_some_and(|guard| !guard.valid(game))) {
            ask.deeds.clear();
            continue;
        }
        ask.deeds.retain(|deed| deed.guard.is_none_or(|guard| guard.valid(game)));
    }
    let head_kept = queue.first().is_none_or(|ask| !ask.deeds.is_empty());
    queue.retain(|ask| !ask.deeds.is_empty());
    turns(|t| {
        t.queue = queue;
        if !head_kept { t.carried = false; }
    });
}

// turn_then only when no look waits: Crystal Aura, Anchor Aura and the macros switch the hand in `then`, so an ask
// refused changes nothing. Never while Blink holds the moves.
pub(super) fn try_turn_then(who: &'static str, yaw: f32, pitch: f32,
    then: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) -> bool {
    if !running() { return false; }
    if !yaw.is_finite() || !pitch.is_finite() || BLINK.load(Ordering::Relaxed) {
        return false;
    }
    let look = (yaw, pitch.clamp(-90.0, 90.0));
    turns(|t| {
        if t.carried || !t.queue.is_empty() {
            return false;
        }
        t.queue.push(Ask { who, look, deeds: vec![Deed { guard: None, critical: CriticalCompletion(false), run: Box::new(then) }], fresh: true });
        true
    })
}

// A look is waiting to go out, or on its way.
pub(super) fn turn_pending() -> bool {
    turns(|t| t.carried || !t.queue.is_empty())
}

// What `who` asked to look at, and do, is dropped.
pub(super) fn cancel_turns(who: &str) {
    turns(|t| {
        if t.queue.first().is_some_and(|ask| ask.who == who) {
            t.carried = false;
        }
        t.queue.retain(|ask| ask.who != who);
    });
}

// Has the server see the player look at (yaw, pitch), then does `then`. `who` names the module: what it asks again in
// the same tick goes along under its first look, and an ask from an earlier tick still waiting is replaced.
pub(super) fn turn_then(who: &'static str, yaw: f32, pitch: f32, then: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) {
    queue_turn(who, yaw, pitch, None, false, then);
}
pub(super) fn turn_then_checked(who: &'static str, yaw: f32, pitch: f32, guard: AttackGuard,
    then: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) {
    queue_turn(who, yaw, pitch, Some(guard), false, then);
}
pub(super) fn queue_turn(who: &'static str, yaw: f32, pitch: f32, guard: Option<AttackGuard>, critical: bool,
    then: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) {
    if !running() { return; }
    let critical = CriticalCompletion(critical);
    if !yaw.is_finite() || !pitch.is_finite() {
        return;
    }
    let look = (yaw, pitch.clamp(-90.0, 90.0));
    let deed = Deed { guard, critical, run: Box::new(then) };
    turns(|t| match t.queue.iter_mut().position(|a| a.who == who) {
        Some(i) if t.queue[i].fresh => t.queue[i].deeds.push(deed),
        Some(i) => {
            if i == 0 && t.carried {
                // Its look is on the way already: the new ask waits behind it.
                t.queue.push(Ask { who, look, deeds: vec![deed], fresh: true });
            } else {
                t.queue[i] = Ask { who, look, deeds: vec![deed], fresh: true };
            }
        }
        None => t.queue.push(Ask { who, look, deeds: vec![deed], fresh: true }),
    });
}

// A look alone (Anti AFK's spin).
pub(super) fn turn(who: &'static str, yaw: f32, pitch: f32) {
    turn_then(who, yaw, pitch, |_| {});
}

// Runs what waited for the look this tick's move packet carried.
pub(super) fn turned(game: &mut Game<'_>) {
    let ask = turns(|t| std::mem::take(&mut t.carried).then(|| t.queue.remove(0)));
    for mut deed in ask.map_or_else(Vec::new, |a| a.deeds) {
        if !running() { break; }
        if deed.guard.is_none_or(|guard| guard.valid(game)) {
            deed.critical.0 = false;
            (deed.run)(game);
        }
    }
}

// The end of the tick: with no move packet in it, one to carry the first look asked for (or the player's own back),
// and what waited for it.
pub(super) fn end_turns(game: &mut Game<'_>) {
    // The look that packet would carry is the one the server has already: nothing to send, and what waited for it goes.
    // Your own look back waits for the next tick, as it would after a packet (Quiver keeps the look up across its
    // let-go).
    let own = (game.yaw, game.pitch);
    let have = last_look();
    let quiet = !MOVED_THIS_TICK.load(Ordering::Relaxed) && !BLINK.load(Ordering::Relaxed);
    let already = quiet
        && have.is_some()
        && turns(|t| match t.queue.first().filter(|_| !t.carried).map(|head| wire_look(have, own, head.look)) {
            Some(look) if Some(look) == have => {
                t.carried = true;
                t.restore = look != own;
                true
            }
            None if t.restore && Some(own) == have => {
                t.restore = false;
                true
            }
            _ => false,
        });
    if already {
        turned(game);
    } else if quiet && turns(|t| !t.queue.is_empty() || t.restore) {
        let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
        // outgoing puts the look in.
        game.send(S::MovePlayerRot(play::MovePlayerRot { yaw: game.yaw, pitch: game.pitch, flags: u8::from(ground) }));
        turned(game);
    }
    turns(|t| {
        for a in &mut t.queue {
            a.fresh = false;
        }
    });
}

fn in_the_air(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let b = p.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let f = |v: f64| v.floor() as i32;
    for x in f(lo.x - 0.0625)..=f(hi.x + 0.0625) {
        for y in f(lo.y - 0.0625 - 0.55)..=f(hi.y + 0.0625) {
            for z in f(lo.z - 0.0625)..=f(hi.z + 0.0625) {
                if !super::sunset_util::is_air(game, [x, y, z]) {
                    return false;
                }
            }
        }
    }
    true
}

// Where the player is, as one position packet: what Blink held back is a
// single move to here, as the server takes one position packet a tick.
pub(super) fn flush_blink(game: &mut Game<'_>) {
    if !BLINK_HELD.swap(false, Ordering::Relaxed) || MOVED_THIS_TICK.load(Ordering::Relaxed) {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let flags = u8::from(p.ground.on_ground) | u8::from(p.collision.horizontal) << 1;
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }));
}
