// Bits of the connection the combat modules read and send: the ping, the server's sprint, a player's game mode, a
// menu's state, a swap click.
// A repeat is a queued click, not a physical press/release: toggle state and
// macro user-input hooks must stay untouched. The engine consumes it this tick.
type InputSession = std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<horizon_protocol::connection::Counters>>>>;

#[derive(Default)]
struct OwnedInput {
    session: Option<InputSession>,
    clicks: [u32; 3],
    buttons: [Option<horizon_client::keymapping::KeyState>; 2],
    playing: Option<bool>,
}

static OWNED_INPUT: std::sync::Mutex<OwnedInput> = std::sync::Mutex::new(OwnedInput { session: None, clicks: [0; 3], buttons: [None, None], playing: None });
thread_local! { static OWN_MOUSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

fn input_owner<R>(game: &crate::game::Game<'_>, run: impl FnOnce(&mut OwnedInput) -> R) -> R {
    let mut owned = OWNED_INPUT.lock().unwrap_or_else(|e| e.into_inner());
    let session = game.remote.as_ref().map(|r| r.session.counters.clone());
    let same = owned.session.as_ref().zip(session.as_ref()).is_some_and(|(a, b)| std::sync::Arc::ptr_eq(a, b));
    if !same { *owned = OwnedInput { session, playing: Some(game.was_playing), ..OwnedInput::default() }; }
    run(&mut owned)
}

pub(crate) fn input_consumed() {
    OWNED_INPUT.lock().unwrap_or_else(|e| e.into_inner()).clicks = [0; 3];
}

pub(crate) fn input_screen(game: &crate::game::Game<'_>) {
    input_owner(game, |o| {
        if o.playing != Some(game.was_playing) { o.buttons = [None, None]; }
        o.playing = Some(game.was_playing);
    });
}

pub(crate) fn restore_buttons(game: &mut crate::game::Game<'_>) {
    let buttons = input_owner(game, |o| o.buttons.clone());
    for (i, saved) in buttons.into_iter().enumerate() {
        if let Some(saved) = saved { game.toggles.0[i + 2].is_down = saved.is_down; }
    }
}

pub(crate) fn cancel_input(game: &mut crate::game::Game<'_>) {
    restore_buttons(game);
    let clicks = input_owner(game, |o| { o.buttons = [None, None]; std::mem::take(&mut o.clicks) });
    if let Some(remote) = game.remote.as_mut() {
        horizon_client::interaction_input::sunset_owned_clicks::cancel(&mut remote.input, clicks);
    }
}

// Track native presses separately from module-generated presses, including
// keyboard-to-mouse bindings and toggle options.
pub(crate) fn native_mouse(game: &crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    if OWN_MOUSE.with(std::cell::Cell::get) { return; }
    let (i, toggle) = match button {
        winit::event::MouseButton::Left => (0, game.settings.toggle_attack),
        winit::event::MouseButton::Right => (1, game.settings.toggle_use),
        _ => return,
    };
    input_owner(game, |o| {
        if crate::game::sunset_hands::rf_generated() {
            o.buttons[i].get_or_insert_with(|| game.toggles.0[i + 2].clone());
            if pressed && game.remote.is_some() && game.sleeping.is_none() && !crate::game::sunset::gui_open()
                && crate::game::sunset::running() { o.clicks[i] = o.clicks[i].saturating_add(1); }
            return;
        }
        if let Some(native) = o.buttons[i].as_mut() { native.set_down(pressed, toggle); }
    });
}

pub(crate) fn mouse(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    if pressed && !crate::game::sunset::running() { return; }
    let i = match button { winit::event::MouseButton::Left => 0, winit::event::MouseButton::Right => 1, _ => return };
    input_owner(game, |o| { o.buttons[i].get_or_insert_with(|| game.toggles.0[i + 2].clone()); });
    let before = game.remote.as_ref().map(|r| horizon_client::interaction_input::sunset_owned_clicks::counts(&r.input));
    struct MouseScope(bool);
    impl Drop for MouseScope {
        fn drop(&mut self) { OWN_MOUSE.with(|flag| flag.set(self.0)); }
    }
    let scope = MouseScope(OWN_MOUSE.with(|flag| flag.replace(true)));
    let _ = game.play_mouse(button, pressed);
    drop(scope);
    if let Some((before, after)) = before.zip(game.remote.as_ref().map(|r| horizon_client::interaction_input::sunset_owned_clicks::counts(&r.input))) {
        input_owner(game, |o| { for j in 0..3 { o.clicks[j] = o.clicks[j].saturating_add(after[j].saturating_sub(before[j])); } });
    }
}

pub(crate) fn click(game: &mut crate::game::Game<'_>, attack: bool) {
    if !crate::game::sunset::running() { return; }
    if let Some(remote) = game.remote.as_mut() {
        remote.input.press(if attack { horizon_client::interaction_input::Button::Attack }
            else { horizon_client::interaction_input::Button::Use });
        input_owner(game, |o| o.clicks[usize::from(!attack)] += 1);
    }
}

pub(crate) fn latency(game: &crate::game::Game<'_>) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let milliseconds = remote.tab_players.get(&remote.profile)?.latency;
    (milliseconds >= 0).then_some(milliseconds)
}
pub(crate) fn sprint_synced(game: &crate::game::Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|player| player.sprinting == crate::game::sunset_combat_wire::sent_sprinting(game))
}

pub(crate) fn carried_synced(game: &crate::game::Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|r| r.carried == Some(game.selected))
}

// The entity is a player the server told of (not you).
pub(crate) fn is_player(game: &crate::game::Game<'_>, id: i32) -> bool {
    game.remote.as_ref().is_some_and(|remote| id != remote.entity_id && remote.level_players.contains_key(&id))
}

// Kept profile names outlive tab-list removal, so they cannot establish that
// a player is still listed or that their current mode permits targeting.
pub(crate) fn player_mode(game: &crate::game::Game<'_>, id: u32) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let uuid = remote.level_players.get(&(id as i32))?;
    Some(remote.tab_players.get(uuid)?.game_mode)
}

// The UUID of a player the server told of.
pub(crate) fn player_uuid(game: &crate::game::Game<'_>, id: u32) -> Option<u128> {
    game.remote.as_ref()?.level_players.get(&(id as i32)).copied()
}

// Presence in player-info differs from the optional "listed in tab" flag.
pub(crate) fn player_has_info(game: &crate::game::Game<'_>, id: u32) -> bool {
    player_uuid(game, id).is_some_and(|uuid| game.remote.as_ref().is_some_and(|r| r.tab_players.contains_key(&uuid)))
}

// The last position applied from the network, before client interpolation.
pub(crate) fn entity_position(game: &crate::game::Game<'_>, id: u32) -> Option<[f64; 3]> {
    Some(game.remote.as_ref()?.entities.get(&(id as i32))?.position)
}

// The menu's state id, as the last server packet left it.
pub(crate) fn container_state(game: &crate::game::Game<'_>) -> Option<i32> {
    Some(game.remote.as_ref()?.state_id)
}

// The last input sent had no movement keys and no sprint: a menu click now is one a standing player makes.
pub(crate) fn quiet(game: &crate::game::Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|remote| !remote.was_sprinting && remote.last_input & 0b0011_1111 == 0)
}

// A swap click (inventory slot `from` with hotbar slot `to`, or the offhand as 504) in `menu`, sent without changing
// the slots here first: the server's slot packets then say what really happened.
pub(crate) fn swap_unpredicted(game: &mut crate::game::Game<'_>, menu: &(i32, String), from: usize, to: usize) -> bool {
    let locked = |slot: usize| crate::game::sunset_loadouts::locked(slot);
    if game.carried.is_some() || from == to || &game.remote_menu() != menu || !(to < 9 || to == 504) || locked(from) || locked(to) {
        return false;
    }
    let Some(state) = game.remote.as_ref().map(|r| r.state_id) else {
        return false;
    };
    let Some(slot) = crate::game::Game::menu_slot_index(&menu.1, from).and_then(|slot| i16::try_from(slot).ok()) else {
        return false;
    };
    game.send(horizon_protocol::packets::play::Serverbound::ContainerClick(horizon_protocol::packets::play::ContainerClick {
        container: menu.0,
        state,
        slot,
        button: if to == 504 { 40 } else { to as i8 },
        input: super::input_ordinal(horizon_client::menu::Input::Swap),
        changed: Vec::new(),
        carried: None,
    }));
    true
}
