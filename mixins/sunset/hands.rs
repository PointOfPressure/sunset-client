// RF Click (R and F as the mouse buttons) and Kill Aura's and Trigger Bot's Shield (a shield kept up, lowered for each
// of your hits).
use super::sunset::{Category, Module, gui_open, with};
use super::sunset_util as util;
use super::{Game, Screen};
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
    lowered: Option<u64>,
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
    if !shielding(game) || !(hits || game.attack_clicks > 0) {
        return;
    }
    game.send(S::PlayerAction(play::PlayerAction { action: 6, pos: [0; 3], direction: 0, sequence: 0 }));
    game.using = None;
    let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
    hands(|h| {
        h.raised = false;
        h.lowered = Some(game.ticks);
    });
}

pub(super) fn tick(game: &mut Game<'_>) {
    // Kill Aura's Shield, else Trigger Bot's: the first of them on with it.
    let m = ["Kill Aura", "Trigger Bot"]
        .into_iter()
        .find_map(|name| module(name).filter(|m| m.b("Shield")))
        .filter(|_| game.native_on(super::sunset::ID));
    let want = m.as_ref().is_some_and(|m| wanted(game, m));
    let raised = hands(|h| h.raised);
    if !want {
        if raised {
            hands(|h| h.raised = false);
            if !game.physical_buttons.contains(&MouseButton::Right) {
                let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
            }
        }
        return;
    }
    let delay = m.as_ref().map_or(0, |m| m.n("Shield re-raise delay") as u64);
    let rested = hands(|h| h.lowered.is_none_or(|t| game.ticks >= t.saturating_add(delay)));
    if !raised && rested && game.using.is_none() && !super::sunset::strike_waiting() {
        let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, true);
        hands(|h| h.raised = true);
    }
}
