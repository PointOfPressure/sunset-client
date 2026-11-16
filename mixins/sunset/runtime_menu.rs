// Runtime part (see runtime.rs): the menu on the Mods page, toggling modules, keys and binds.

use super::Game;
use super::sunset_bind_state::Held;
use std::sync::atomic::Ordering;
use winit::keyboard::KeyCode;
use super::sunset::*;

// ---------------------------------------------------------------- the menu

// Sunset's row in Options > Mods; none after Panic, when the list is made again (`unlist` took the one it had).
pub(super) fn listed(mods: &mut Vec<super::native_hooks::NativeMod>) {
    if !running() { return; }
    super::sunset_session::sweep();
    mods.push(super::native_hooks::NativeMod {
        id: ID.into(),
        title: format!("{NAME} {VERSION}"),
        description: DESCRIPTION.into(),
        keys: vec![
            [MENU_KEY.into(), "Open Sunset menu".into(), MENU_DEFAULT.into()],
            [MENU2_KEY.into(), "Open Sunset menu (2nd key)".into(), MENU2_DEFAULT.into()],
        ],
    });
}

pub(super) fn mod_index(game: &Game<'_>) -> Option<usize> {
    game.settings.mods.installed.iter().position(|m| m.id == ID)
}

pub(super) fn refresh(game: &mut Game<'_>) {
    if let Some(i) = mod_index(game) {
        game.native_settings_refresh(i);
    }
}

// Either of the two keys that open the click GUI.
pub(super) fn is_menu_key(game: &Game<'_>, key: &str) -> bool {
    key == game.native_mod_key(ID, MENU_KEY, MENU_DEFAULT) || key == game.native_mod_key(ID, MENU2_KEY, MENU2_DEFAULT)
}

pub(super) fn open_menu(game: &mut Game<'_>) {
    if !running() { return; }
    release_binds();
    let Some(i) = mod_index(game) else {
        say(game, "Turn Sunset on in Options > Mods first.");
        return;
    };
    with(|s| {
        s.page = Page::Main;
        s.binding = None;
    });
    game.open_options_at(false, horizon_client::options_ui::Page::NativeMod(i));
    game.native_settings_refresh(i);
}

pub(super) fn buttons(game: &Game<'_>, out: &mut Vec<String>) {
    if !running() { return; }
    ensure_loaded(game);
    with(|s| match s.page {
        Page::Main => {
            let listed = s.listed();
            for c in CATEGORIES {
                let all: Vec<&Module> = s.modules.iter().filter(|m| m.category == c && listed(m)).collect();
                let on = all.iter().filter(|m| m.on).count();
                out.push(format!("{} ({on} of {} on)", c.label(), all.len()));
            }
            out.push(format!("Friends: {}", if s.friends.is_empty() { "none".into() } else { s.friends.join(", ") }));
            out.push("Type .help in chat for commands".into());
        }
        Page::Category(c) => {
            out.push("< Back".into());
            let listed = s.listed();
            for m in s.modules.iter().filter(|m| m.category == c && listed(m)) {
                out.push(format!("{}: {}", m.name, on_off(m.on)));
            }
        }
        Page::Module(i) => {
            let m = &s.modules[i];
            out.push(format!("< Back to {}", m.category.label()));
            out.push(format!("{}: {}", m.name, on_off(m.on)));
            if s.binding == Some(i) {
                out.push("Keybind: > press a key (Backspace: none) <".into());
            } else {
                out.push(format!("Keybind: {}", key_label(m.key.as_deref())));
            }
            for setting in &m.settings {
                out.push(setting.shown());
            }
            let mut about = m.about.to_owned();
            if m.settings.iter().any(|s| matches!(s.value, Value::Num { .. })) {
                about.push_str(" Shift-click lowers a number.");
            }
            out.push(about);
        }
    });
}

pub(super) fn click(game: &mut Game<'_>, button: usize) {
    if !running() { return; }
    ensure_loaded(game);
    let shift = game.modifiers.shift_key();
    let mut toggled = None;
    let changed = with(|s| match s.page {
        Page::Main => {
            if let Some(&c) = CATEGORIES.get(button) {
                s.page = Page::Category(c);
            }
            false
        }
        Page::Category(c) => {
            if button == 0 {
                s.page = Page::Main;
                return false;
            }
            let listed = s.listed();
            let idx: Vec<usize> = (0..s.modules.len()).filter(|&i| s.modules[i].category == c && listed(&s.modules[i])).collect();
            if let Some(&i) = idx.get(button - 1) {
                s.page = Page::Module(i);
            }
            false
        }
        Page::Module(i) => match button {
            0 => {
                s.binding = None;
                s.page = Page::Category(s.modules[i].category);
                false
            }
            1 => {
                toggled = Some(i);
                false
            }
            2 => {
                s.binding = if s.binding == Some(i) { None } else { Some(i) };
                false
            }
            b => match s.modules[i].settings.get_mut(b - 3) {
                Some(setting) => {
                    setting.click(shift);
                    true
                }
                None => false,
            },
        },
    });
    if let Some(i) = toggled {
        toggle(game, i, false);
    } else if changed {
        save(game);
    }
}

pub(super) fn toggle(game: &mut Game<'_>, i: usize, announce: bool) {
    if !running() { return; }
    if with(|s| s.modules[i].name == "Panic" && !s.modules[i].on) {
        panic_off(game);
        return;
    }
    let (name, on) = with(|s| {
        // Turned on or off by hand: a Hold bind no longer has it.
        s.inputs.take_where(|held| held.module == i);
        let m = &mut s.modules[i];
        m.on = !m.on;
        (m.name, m.on)
    });
    switched_effects(game, name, on);
    save(game);
    if announce && name != super::sunset_debug::NAME {
        announce_toggle(name, on);
    }
}

// What a module going on or off does besides its switch: by hand (toggle) or as a profile or preset loads.
pub(super) fn switched_effects(game: &mut Game<'_>, name: &'static str, on: bool) {
    if name == "Panic" {
        return;
    }
    super::sunset_combat_macros::toggled(game, name, on);
    super::sunset_combat_inventory::toggled(game, name);
    super::sunset_pvp_bot::toggled(name);
    super::sunset_packet_mine::toggled(game, name, on);
    if name == "Friend Guard" {
        super::sunset_melee::reset_friend_feedback();
    }
    if !on {
        match name {
            "Scaffold" => {
                with(|s| s.scaffold_stack = None);
                restore_scaffold_rotation(game);
                legit_sneak(game, false);
                super::sunset_scaffold_bridge::stop(game);
                super::sunset_scaffold_telly::stop(game);
            }
            "Speed" => {
                watchdog_stop(game, true);
                with(|s| { s.strafe = Strafe::default(); s.bhop.reset(); s.ncp.reset(); s.knockback_ticks = 0; });
                with(|s| s.legacy_recovery = Default::default());
                horizon_client::motion::sunset_motion::set_across(None);
                horizon_client::motion::sunset_motion::set_up(None);
            },
            "Velocity" => with(|s| { s.velocity = None; s.blast = [0.0; 3]; }),
            "Crystal Aura" => super::sunset_crystal_aura::disabled(game),
            "Cart" => super::sunset_cart::reset(game),
            "Kill Aura" => {
                super::sunset_hands::stop_kill_aura_block(game);
                let swap_back = with(|s| s.module("Kill Aura").is_some_and(|m| m.b("Swap back")));
                super::sunset_aura::disabled(game, swap_back);
            }
            "Anchor Aura" => super::sunset_anchor_aura::disabled(game),
            "No Slow" => {
                horizon_client::input::sunset_flags::NO_SLOW.store(false, Ordering::Relaxed);
                horizon_client::motion::sunset_motion::set_no_slow_skip(false);
                horizon_client::motion::sunset_motion::set_no_slow_grim(false);
            },
            "Server Scan" => super::sunset_server_scan::disabled(),
            _ => {},
        }
    }
}

pub(super) fn take_scaffold_rotation(saved: &mut Option<(f32, f32)>) -> Option<(f32, f32)> {
    saved.take()
}

pub(super) fn restore_scaffold_rotation(game: &mut Game<'_>) {
    if let Some((yaw, pitch)) = with(|s| take_scaffold_rotation(&mut s.scaffold_rotation)) {
        game.yaw = yaw;
        game.pitch = pitch;
    }
}

// ---------------------------------------------------------------- keys

// Modules Hold binds had on, back as they were (left off if something turned them off meanwhile).
fn restore_holds(s: &mut State, held: Vec<Held>) {
    if held.is_empty() {
        return;
    }
    for h in held {
        if let Some(m) = s.modules.get_mut(h.module)
            && m.on
        {
            m.on = h.previous;
        }
    }
    sync_flags(s);
}

// Holds whose module went off, or whose bind or Bind mode changed, end.
pub(super) fn release_changed_binds(s: &mut State) {
    let modules = &s.modules;
    let held = s.inputs.take_where(|h| {
        modules
            .get(h.module)
            .is_none_or(|m| !m.on || m.key.as_deref() != Some(h.key.as_str()) || m.p("Bind mode") != "Hold")
    });
    restore_holds(s, held);
}

// A screen, the menu or another window has the input: every hold ends.
pub(super) fn release_binds() {
    with(|s| {
        let held = s.inputs.suspend();
        restore_holds(s, held);
    });
}

pub(super) fn set_bind(s: &mut State, module: usize, key: Option<String>) {
    let held = s.inputs.take_where(|h| h.module == module);
    restore_holds(s, held);
    s.modules[module].key = key;
}

// Each tick, whatever the screen: keys let go of where no release came (from what is physically down), and no holds
// while the menu or a screen has the input (another window taking it is the window's Focused event, in bind_event).
pub(super) fn sync_binds(game: &Game<'_>) {
    if !game.native_window() {
        return;
    }
    with(|s| {
        let held = s.inputs.missing(|name| {
            game.physical_keys.iter().any(|k| horizon_client::keybinds::key_name(*k) == Some(name))
                || game.physical_buttons.iter().any(|b| horizon_client::keybinds::mouse_name(*b) == name)
        });
        restore_holds(s, held);
        release_changed_binds(s);
        if !game.native_on(ID) || game.player.is_none() || !game.native_playing() || s.gui.open {
            let held = s.inputs.suspend();
            restore_holds(s, held);
        }
    });
}

// The window's presses and releases before any screen has them: a release a screen swallows still ends its hold, a
// key pressed in the menu or by the window coming back into focus is no bind press, and lost focus ends every hold.
pub(super) fn bind_event(event: &winit::event::WindowEvent) {
    use winit::event::{ElementState, WindowEvent};
    use winit::keyboard::{Key, NamedKey, PhysicalKey};
    if matches!(event, WindowEvent::Focused(false) | WindowEvent::CloseRequested | WindowEvent::Destroyed) {
        release_binds();
    }
    let (name, pressed, synthetic) = match event {
        WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
            // Escape and Caps Lock by their meaning, as Horizon's keymap takes them.
            let code = match &event.logical_key {
                Key::Named(NamedKey::Escape) => KeyCode::Escape,
                Key::Named(NamedKey::CapsLock) => KeyCode::CapsLock,
                _ => match event.physical_key {
                    PhysicalKey::Code(code) => code,
                    PhysicalKey::Unidentified(_) => return,
                },
            };
            let Some(name) = horizon_client::keybinds::key_name(code) else {
                return;
            };
            (name.to_owned(), event.state == ElementState::Pressed, *is_synthetic)
        }
        WindowEvent::MouseInput { state, button, .. } => (horizon_client::keybinds::mouse_name(*button), *state == ElementState::Pressed, false),
        _ => return,
    };
    // RF Click's R/F clicks are clicks for the game only: no bind on that mouse button fires from them.
    let rf = matches!(event, WindowEvent::MouseInput { .. }) && super::sunset_hands::rf_generated();
    with(|s| {
        if !pressed {
            let held = s.inputs.release(&name);
            restore_holds(s, held);
            return;
        }
        if s.gui.open || synthetic {
            s.inputs.block(&name);
            let held = s.inputs.suspend();
            restore_holds(s, held);
        }
        if synthetic || rf {
            s.inputs.synthetic(&name);
        }
    });
}

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    finish_shutdown(game);
    if !running() { return; }
    if !game.native_window() {
        return;
    }
    ensure_loaded(game);
    if pressed && game.native_on(ID) && game.native_playing() && !gui_open()
        && with(|s| !s.inputs.is_synthetic(key) && s.module("Panic").is_some_and(|m| m.key.as_deref() == Some(key)))
        && with(|s| s.inputs.press(key))
    {
        panic_off(game);
        return;
    }
    if game.native_on(ID) {
        super::sunset_api::key(game, key, pressed);
    }
    if !running() { return; }
    if pressed {
        super::agent::sunset_goto::key(game, key);
    }
    super::sunset_melee::key(game, key, pressed);
    super::sunset_swap_keys::key(game, key, pressed);
    super::sunset_combat::web_key(game, key, pressed);
    super::sunset_duel::key(game, key, pressed);
    super::sunset_cart::key(game, key, pressed);
    super::sunset_throws::key(game, key, pressed);
    if pressed {
        armor_switch(game, key);
    }
    super::sunset_refill::key(key, pressed);
    super::sunset_takeoff::key(key, pressed);
    if !pressed {
        with(|s| {
            let held = s.inputs.release(key);
            restore_holds(s, held);
        });
        return;
    }
    if !game.native_on(ID) {
        release_binds();
        return;
    }
    if with(|s| s.inputs.is_synthetic(key)) {
        return;
    }
    ensure_loaded(game);
    if gui_open() {
        gui_key(game, key);
        return;
    }
    if let Some(i) = with(|s| s.binding) {
        let chosen = match key {
            "key.keyboard.escape" => {
                with(|s| s.binding = None);
                refresh(game);
                return;
            }
            "key.keyboard.backspace" | "key.keyboard.delete" => None,
            k => Some(k.to_owned()),
        };
        with(|s| {
            set_bind(s, i, chosen);
            s.binding = None;
        });
        save(game);
        refresh(game);
        return;
    }
    if !game.native_playing() || game.player.is_none() {
        with(|s| s.inputs.block(key));
        release_binds();
        return;
    }
    // The chat key opens a text screen: a bind on the same key is not pressed by it.
    if !is_menu_key(game, key) && (key == game.settings.keys.get("key.chat") || key == game.settings.keys.get("key.command")) {
        with(|s| s.inputs.block(key));
        release_binds();
        return;
    }
    // A fresh press only: not one a screen or the menu had when it went down.
    if !with(|s| s.inputs.press(key)) {
        return;
    }
    if is_menu_key(game, key) {
        open_gui(game);
        return;
    }
    if key == "key.mouse.middle" && with(|s| s.is_on("Middle Click Extra")) {
        middle_click(game);
    }
    if key == "key.mouse.right" {
        super::sunset_player::right_click(game);
    }
    let bound: Vec<usize> = with(|s| {
        (0..s.modules.len())
            .filter(|&i| s.modules[i].key.as_deref() == Some(key))
            .collect()
    });
    for i in bound {
        if !running() { break; }
        if with(|s| s.modules[i].p("Bind mode") == "Hold") {
            // On while the key is down, then as it was.
            with(|s| {
                let previous = s.modules[i].on;
                s.inputs.hold(i, key, previous);
                s.modules[i].on = true;
                sync_flags(s);
            });
        } else {
            toggle(game, i, true);
        }
    }
    // Air Jump: the jump key in mid-air jumps again.
    if key == "key.keyboard.space" && with(|s| s.is_on("Air Jump")) {
        if let Some(p) = game.player.as_mut()
            && !p.ground.on_ground
            && !p.flying
        {
            p.velocity.y = 0.42;
            p.fall_distance = 0.0;
        }
        remember_velocity(game);
    }
}
