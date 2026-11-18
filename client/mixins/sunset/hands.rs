// RF Click (R and F as the mouse buttons) and Kill Aura's and Trigger Bot's Shield (a shield kept up, lowered for each
// of your hits).
use super::sunset::{Category, Module, gui_open, with};
use super::sunset_util as util;
use super::{Game, Screen};
use super::sunset_autoblock::{Action, Legacy};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::PhysicalKey;

const RF: &str = "RF Click";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(RF, Category::Player, "R and F act as mouse buttons.")
            .text("Left key", "R")
            .text("Right key", "F"),
    ]
}

#[derive(Default)]
struct Hands {
    // RF Click: the key names for the left and right button (None: off), whether you are typing as of the last frame,
    // and which of the two is down as a button.
    keys: [Option<String>; 2],
    typing: bool,
    down: [bool; 2],
    physical: [bool; 2],
    // Shield: it holds use, and the tick it last lowered the shield for a hit.
    raised: bool,
    modern_owner: Option<&'static str>,
    lowered: Option<u64>,
    legacy: Legacy,
}

thread_local! { static OWNED_BLOCK_PACKET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

pub(super) fn owned_block_packet() -> bool { OWNED_BLOCK_PACKET.with(std::cell::Cell::get) }

fn send_owned(game: &mut Game<'_>, packet: S) {
    if matches!(&packet, S::PlayerAction(a) if a.action == 6) {
        horizon_client::motion::sunset_motion::set_block_use_scale(None);
    }
    struct Scope(bool);
    impl Drop for Scope { fn drop(&mut self) { OWNED_BLOCK_PACKET.with(|f| f.set(self.0)); } }
    let _scope = Scope(OWNED_BLOCK_PACKET.with(|f| f.replace(true)));
    game.send(packet);
}

fn release_packet() -> S {
    // The current protocol's native release-use action, as used by Horizon's shield handling.
    S::PlayerAction(play::PlayerAction { action: 6, pos: [0; 3], direction: 0, sequence: 0 })
}

pub(super) fn scene_changed(game: &mut Game<'_>, same_scene: bool) {
    horizon_client::motion::sunset_motion::set_block_use_scale(None);
    if same_scene { stop_autoblock(game); }
    let raised = hands(|h| { h.legacy.stop(); h.lowered = None; h.modern_owner = None; std::mem::take(&mut h.raised) });
    if raised && !game.physical_buttons.contains(&MouseButton::Right) {
        // A new scene cannot receive the old scene's release packet; only let go of our synthetic button.
        super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
    }
}

pub(super) fn stop_autoblock(game: &mut Game<'_>) {
    stop_owned_block(game, false);
}

pub(super) fn stop_kill_aura_block(game: &mut Game<'_>) {
    stop_owned_block(game, true);
}

fn stop_owned_block(game: &mut Game<'_>, kill_aura_only: bool) {
    horizon_client::motion::sunset_motion::set_block_use_scale(None);
    let (legacy, raised) = hands(|h| {
        let take_modern = !kill_aura_only || h.modern_owner == Some("Kill Aura");
        let raised = take_modern && std::mem::take(&mut h.raised);
        if take_modern { h.modern_owner = None; }
        (h.legacy.stop(), raised)
    });
    if legacy && super::sunset_combat_control::same_scene(game) { send_owned(game, release_packet()); }
    if raised && !game.physical_buttons.contains(&MouseButton::Right) {
        if game.using.is_some_and(|(slot, _)| slot == util::OFFHAND) {
            send_owned(game, release_packet());
            game.using = None;
        }
        super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
    }
}

pub(super) fn autoblock_status() -> &'static str {
    hands(|h| if h.legacy.active() { "Legacy use requested (unconfirmed)" }
        else if h.raised { "Modern shield requested" } else { "idle" })
}

pub(super) fn legacy_using() -> bool { hands(|h| h.legacy.active()) }

// Called only for packets that passed the normal filters. A real attack must release our legacy use immediately
// before the attack packet, even if it came from Criticals, WTap, a native click or a different combat module.
// The returned cleanup packet is delivered by the shared send hook; no game call occurs under the Hands lock.
pub(super) fn before_outgoing(game: &Game<'_>, packet: &S) -> Option<S> {
    // A 1.8-order hit's swing goes before its attack: the release goes before the swing then.
    let strike_swing = matches!(packet, S::Punch(_)) && STRIKE_SWING.with(std::cell::Cell::get);
    if owned_block_packet()
        || !(strike_swing || matches!(packet, S::Attack(_) | S::SetCarriedItem(_) | S::UseItem(_) | S::PlayerAction(_)))
    {
        return None;
    }
    if !hands(|h| h.legacy.active()) { return None; }
    if !super::sunset_combat_control::same_scene(game) {
        hands(|h| { h.legacy.stop(); });
        return None;
    }
    let attack = strike_swing || matches!(packet, S::Attack(_)) || matches!(packet, S::PlayerAction(a) if a.action == 8);
    if attack {
        let delay = module("Kill Aura").map_or(1, |m| m.n("Legacy reblock delay") as u64);
        let released = hands(|h| h.legacy.attack(game.ticks, delay));
        if released { horizon_client::motion::sunset_motion::set_block_use_scale(None); }
        return released.then(release_packet);
    }
    if matches!(packet, S::SetCarriedItem(_) | S::UseItem(_)) || matches!(packet, S::PlayerAction(a) if a.action == 6) {
        // A native/foreign use or slot packet owns the replacement state; do not later release it as ours.
        hands(|h| h.legacy.interrupted(game.ticks));
        horizon_client::motion::sunset_motion::set_block_use_scale(None);
    }
    None
}

thread_local! {
    static STRIKE_SWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// The swing of a hit sent the 1.8 way (swing, then attack): AutoBlock's release goes ahead of it, as of an attack.
pub(super) fn strike_swing(game: &mut Game<'_>) {
    STRIKE_SWING.with(|f| f.set(true));
    game.send(S::Punch(horizon_protocol::packets::Empty));
    STRIKE_SWING.with(|f| f.set(false));
}

static HANDS: Mutex<Option<Hands>> = Mutex::new(None);
static RF_EVENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn rf_generated() -> bool { RF_EVENT.load(std::sync::atomic::Ordering::Relaxed) }

pub(super) fn release_rf(game: &mut Game<'_>) {
    let (down, physical) = hands(|h| (h.down, h.physical));
    for (i, button) in [MouseButton::Left, MouseButton::Right].into_iter().enumerate() {
        if down[i] && !physical[i] { game.physical_buttons.remove(&button); }
    }
}

pub(super) fn shutdown(game: &mut Game<'_>) {
    stop_autoblock(game);
    let down = hands(|h| {
        let down = [h.down[0], h.down[1] || h.raised];
        *h = Hands { physical: h.physical, ..Hands::default() };
        down
    });
    for (owned, button) in down.into_iter().zip([MouseButton::Left, MouseButton::Right]) {
        if owned && !game.physical_buttons.contains(&button) { let _ = super::remote::sunset_combat_remote::mouse(game, button, false); }
    }
}

fn hands<R>(f: impl FnOnce(&mut Hands) -> R) -> R {
    f(HANDS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Hands::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

// A text box or anything else that takes keys has them.
fn typing(game: &Game<'_>) -> bool {
    matches!(game.screen, Screen::Chat | Screen::CommandBlock | Screen::Options | Screen::WorldOptions)
        || game.sign_edit.is_some()
        || game.book.is_some()
        || game.title.is_some()
        || (game.screen == Screen::Creative && game.creative_screen.tab().search)
        || game.recipe_search_focused()
        || game.mod_browser_typing()
        || game.station_typing()
        || game.dialog_open()
        || gui_open()
}

// Every frame: RF Click's keys and whether you are typing.
pub(super) fn frame(game: &Game<'_>) {
    let keys = match module(RF).filter(|_| game.native_on(super::sunset::ID)) {
        Some(m) => [super::sunset::key_from_text(m.t("Left key")), super::sunset::key_from_text(m.t("Right key"))],
        None => [None, None],
    };
    let typing = typing(game);
    hands(|h| {
        h.keys = keys;
        h.typing = typing;
    });
}

// Focus lost: the window sends no releases for what was down, and the game lets go of its buttons by itself.
pub(super) fn rf_focus_lost() {
    hands(|h| h.down = [false; 2]);
}

// The window's key events, before anything sees them: RF Click's keys become mouse button presses and releases (a key
// repeat becomes nothing). A key let go is always the button let go, even if typing started meanwhile.
pub(super) fn rf_event(event: &mut WindowEvent) {
    RF_EVENT.store(false, std::sync::atomic::Ordering::Relaxed);
    if let WindowEvent::MouseInput { state, button, .. } = event {
        let i = match button { MouseButton::Left => Some(0), MouseButton::Right => Some(1), _ => None };
        if let Some(i) = i { hands(|h| h.physical[i] = *state == ElementState::Pressed); }
    }
    if !super::sunset::running() { return; }
    let WindowEvent::KeyboardInput { device_id, event: key, .. } = event else {
        return;
    };
    let PhysicalKey::Code(code) = key.physical_key else {
        return;
    };
    let Some(name) = horizon_client::keybinds::key_name(code) else {
        return;
    };
    let (state, repeat, device_id) = (key.state, key.repeat, *device_id);
    let button = hands(|h| {
        let i = h.keys.iter().position(|k| k.as_deref() == Some(name)).or_else(|| {
            // Turned off while held: its release still lets go.
            (state == ElementState::Released).then(|| h.down.iter().position(|&d| d)).flatten()
        })?;
        match state {
            ElementState::Pressed if h.down[i] => Some(i),
            ElementState::Pressed if h.typing || repeat => None,
            ElementState::Pressed => {
                h.down[i] = true;
                Some(i)
            }
            ElementState::Released if h.down[i] => {
                h.down[i] = false;
                Some(i)
            }
            ElementState::Released => None,
        }
    });
    let Some(i) = button else {
        return;
    };
    RF_EVENT.store(true, std::sync::atomic::Ordering::Relaxed);
    *event = if repeat {
        WindowEvent::HoveredFileCancelled
    } else {
        WindowEvent::MouseInput { device_id, state, button: if i == 0 { MouseButton::Left } else { MouseButton::Right } }
    };
}

// Shield has the shield up (Kill Aura and Trigger Bot hit through it: it comes down for the hit).
pub(super) fn shielding(game: &Game<'_>) -> bool {
    hands(|h| h.raised) && game.using.is_some_and(|(slot, _)| slot == util::OFFHAND)
}

fn offhand_shield(game: &Game<'_>) -> bool {
    game.offhand.as_ref().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.blocks_attacks.is_some())
}

fn wanted(game: &Game<'_>, m: &Module) -> bool {
    let held = game.held().map(|s| util::short(&s.item));
    let weapon = held.is_some_and(|h| h.ends_with("_sword") || h.ends_with("_axe") || h == "mace");
    // Anything else in hand with a use of its own (food, blocks, bows) would be used instead.
    let free = held.is_none() || weapon;
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let near = || {
        let Some(eye) = game.eye_position() else {
            return false;
        };
        game.mobs.iter().any(|mob| {
            mob.alive()
                && !mob.removed
                && Some(mob.id) != own
                && (mob.kind.id == "minecraft:player" || mob.hostile() || mob.kind.category == "monster")
                && util::dist(eye, [mob.position.x, mob.position.y + 1.0, mob.position.z]) <= m.n("Shield range")
        })
    };
    game.native_playing()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
        && super::sunset_combat_control::available(m.name)
        && game.screen == Screen::Playing
        && !gui_open()
        && offhand_shield(game)
        && free
        && (weapon || !m.b("Shield only with a weapon"))
        && (!m.b("Shield only near a target") || near())
}

// Before the tick's hits (Kill Aura's and Trigger Bot's queued ones, your click): the shield comes down, so the hit
// goes in the same tick.
pub(super) fn before_hits(game: &mut Game<'_>, hits: bool) {
    legacy_pre_tick(game, hits || game.attack_clicks > 0);
    if !shielding(game) || !(hits || game.attack_clicks > 0) {
        return;
    }
    send_owned(game, release_packet());
    game.using = None;
    let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
    hands(|h| {
        h.raised = false;
        h.modern_owner = None;
        h.lowered = Some(game.ticks);
    });
}

fn legacy_pre_tick(game: &mut Game<'_>, hits: bool) {
    if !super::sunset_combat_control::same_scene(game) {
        hands(|h| { h.legacy.stop(); });
        return;
    }
    let m = module("Kill Aura").filter(|m| m.p("AutoBlock") == "Legacy" && game.native_on(super::sunset::ID));
    let desired = m.as_ref().filter(|m| {
        game.native_playing() && game.screen == Screen::Playing && !gui_open() && game.remote.is_some()
            && !game.spectator() && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
            && game.using.is_none() && !game.right && !game.physical_buttons.contains(&MouseButton::Right)
            && game.held().is_some_and(|s| s.item.ends_with("_sword"))
            && super::sunset_combat_control::available("Kill Aura")
            && !with(|s| s.is_on("Blink")) && !super::sunset_lag::pending()
            && super::sunset_aura_timing::version(m.p("Combat version"), game.main_hand_attack_speed()).0
                == super::sunset_aura_timing::Version::Legacy
            && super::sunset_aura::target_id().is_some_and(|id| game.mobs.iter().any(|mob| mob.id == id && mob.alive() && !mob.removed))
    }).map(|_| game.selected);
    let delay = m.as_ref().map_or(1, |m| m.n("Legacy reblock delay") as u64);
    if hits && hands(|h| h.legacy.attack(game.ticks, delay)) { send_owned(game, release_packet()); }
    let can_start = !hits && !super::sunset::strike_waiting() && super::remote::sunset_combat_remote::carried_synced(game);
    match hands(|h| h.legacy.update(game.ticks, desired, can_start)) {
        Action::None => {}
        Action::Release => send_owned(game, release_packet()),
        Action::Block => {
            let sequence = util::sequence(game);
            let look = super::sunset::last_look().unwrap_or((game.yaw, game.pitch));
            let yaw = super::sunset::wire_yaw(game, look.0);
            send_owned(game, S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: look.1 }));
        }
    }
    let scale = legacy_using().then(|| {
        // GrimAC's modern item-use method has not been established for translated sword use; keep normal slowdown.
        let no_slow = with(|s| s.module("No Slow").filter(|m| m.on && m.p("Mode") != "GrimAC")
            .map(|m| [m.n("Ground %"), m.n("Air %")]));
        super::sunset_movement::legacy_use_factor(no_slow, game.player.as_ref().is_some_and(|p| p.ground.on_ground))
    });
    horizon_client::motion::sunset_motion::set_block_use_scale(scale);
}

pub(super) fn tick(game: &mut Game<'_>) {
    // Kill Aura's Shield, else Trigger Bot's: the first of them on with it.
    let aura = module("Kill Aura");
    let legacy_selected = aura.as_ref().is_some_and(|m| m.p("AutoBlock") == "Legacy");
    let m = aura.filter(|m| m.p("AutoBlock") == "Modern")
        .or_else(|| (!legacy_selected).then(|| module("Trigger Bot").filter(|m| m.b("Shield"))).flatten())
        .filter(|_| game.native_on(super::sunset::ID));
    let want = m.as_ref().is_some_and(|m| wanted(game, m));
    let raised = hands(|h| h.raised);
    if !want {
        if raised {
            if game.using.is_some_and(|(slot, _)| slot == util::OFFHAND) && !game.physical_buttons.contains(&MouseButton::Right) {
                send_owned(game, release_packet());
                game.using = None;
            }
            hands(|h| { h.raised = false; h.modern_owner = None; });
            if !game.physical_buttons.contains(&MouseButton::Right) {
                let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
            }
        }
        return;
    }
    let delay = m.as_ref().map_or(0, |m| m.n("Shield re-raise delay") as u64);
    if raised { hands(|h| h.modern_owner = m.as_ref().map(|m| m.name)); }
    let rested = hands(|h| h.lowered.is_none_or(|t| game.ticks >= t.saturating_add(delay)));
    if !raised && rested && game.using.is_none() && !super::sunset::strike_waiting() {
        let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, true);
        hands(|h| { h.raised = true; h.modern_owner = m.as_ref().map(|m| m.name); });
    }
}
