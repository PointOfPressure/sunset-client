// Runtime part (see runtime.rs): the . commands, Panic, profiles and presets.

use super::Game;
use super::sunset_util::Sweep;
use std::sync::atomic::Ordering;
use winit::keyboard::KeyCode;
use winit::event::MouseButton;
use super::sunset::*;

// ---------------------------------------------------------------- commands

pub(super) fn chat(game: &mut Game<'_>, message: &str) -> bool {
    if !game.native_window() || !game.native_on(ID) {
        return false;
    }
    // After Panic the chat is the game's own: no command is read, .panic included, until restart.
    if !running() {
        finish_shutdown(game);
        return false;
    }
    let Some(rest) = message.strip_prefix('.') else {
        return false;
    };
    // `..text` sends `.text` as a chat message.
    if rest.starts_with('.') || rest.trim().is_empty() {
        return false;
    }
    ensure_loaded(game);
    game.ui.chat.add_recent(message);
    let args: Vec<&str> = rest.split_whitespace().collect();
    REPLYING.store(true, std::sync::atomic::Ordering::Relaxed);
    run(game, &args);
    REPLYING.store(false, std::sync::atomic::Ordering::Relaxed);
    true
}

const HELP: &[(&str, &str)] = &[
    (".menu", "open the click GUI (or press Right Shift)"),
    (".pages", "the same settings as pages in Options > Mods"),
    (".toggle <module>", "turn a module on or off (.t)"),
    (".modules", "list every module"),
    (".menu [search]", "open the click menu, searching for the text"),
    (".settings <module>", "list a module's settings"),
    (".set <module> <setting> <value>", "change a setting"),
    (".seed [status|auto|clear|restart|seed text]", "Ore Sim's seed, automatic discovery and progress; manual text always wins"),
    (".movementpreset <subtle|balanced|aggressive>", "apply explicit movement settings"),
    (".bind <module> <key|mouse4|none>", "bind a key or mouse button to a module"),
    (".binds", "list the binds"),
    (".reset <module>", "put a module back to its defaults"),
    (".panic", "stop Sunset until the game restarts (nothing brings it back sooner)"),
    (".profile save|load|delete|export|import|list [name]", "keep the modules' settings under a name and bring them back (files in the mod's data folder; .profile load before-load undoes a load)"),
    (".preset [name] [apply]", "the pre-made profiles: a name says what it turns on and off, apply takes it (.profile load before-load undoes it)"),
    (".debug [chat lines]", "copy a debug report (chat, anticheat flags, the client's state, the last packets sent) to the clipboard and its configured Save folder"),
    (".scan [plugins|details|channels|commands|status|run] [page]", "show server-reported findings; run optional Extended checks manually"),
    (".auradebug", "why Kill Aura did what it did last, and who has the hand"),
    (".antibotdebug [entity id]", "show cached decoy confidence and supporting observations"),
    (".plugins", "the plugins loaded, their modules and commands"),
    (".friend add|remove|list [name]", "Kill Aura, Trigger Bot and Aim Assist skip friends"),
    (".waypoint add|remove|list [name]", "save a place (.wp)"),
    (".vclip <blocks>", "move up or down"),
    (".hclip <blocks>", "move forward"),
    (".goto <x> <y> <z>|stop", "walk there through loaded terrain, up to 128 blocks (by Rust); a movement key or a screen stops it"),
    (".b / .baritone status|goto|mine|stop|off|targets|help", "Full Baritone and bounded visible/Ore Sim mining, bundled with Sunset"),
];

// Every module off and their pending work dropped (Panic, and the plugin API's stop_everything).
pub(super) fn panic_off_state(s: &mut State) -> Option<(f32, f32)> {
    for m in &mut s.modules {
        m.on = false;
    }
    s.strafe = Strafe::default();
    s.bhop.reset();
    s.ncp.reset();
    s.watchdog.reset();
    s.legacy_recovery = Default::default();
    s.knockback_ticks = 0;
    s.velocity = None;
    s.blast = [0.0; 3];
    s.scaffold_stack = None;
    s.inputs.suspend();
    s.binding = None;
    s.tool_wait = None;
    s.mc_use = None;
    s.mc_back = None;
    s.crit_target = None;
    s.crit_look = None;
    s.trigger_sticky = None;
    s.trigger_missed = None;
    s.eat_paused_aura = false;
    s.scaffold_placed.clear();
    s.blocks = Sweep::new();
    s.holes = Sweep::new();
    s.unsneaked = false;
    s.blink_ticks = 0;
    s.fly_delay = 0;
    s.fly_off = 0;
    s.nuke = None;
    s.nuke_skip.clear();
    s.nuke_interacted.clear();
    s.nuke_box = None;
    s.nuke_broken.clear();
    take_scaffold_rotation(&mut s.scaffold_rotation)
}

// Every module off (Panic, and the plugin API's stop_everything).
pub(super) fn mapped_input(
    bindings: &horizon_client::keybinds::KeyBinds,
    keys: &std::collections::HashSet<KeyCode>,
    buttons: &std::collections::HashSet<MouseButton>,
    toggles: [Option<bool>; 4],
) -> (std::collections::HashSet<KeyCode>, std::collections::HashSet<MouseButton>) {
    let mut held = (std::collections::HashSet::new(), std::collections::HashSet::new());
    for action in keys.iter().flat_map(|k| bindings.translate_all(*k))
        .chain(buttons.iter().flat_map(|b| bindings.translate_mouse_all(*b))) {
        match action { Ok(k) => { held.0.insert(k); } Err(b) => { held.1.insert(b); } }
    }
    for (slot, key) in [KeyCode::ShiftLeft, KeyCode::ControlLeft].into_iter().enumerate() {
        if let Some(down) = toggles[slot] {
            if down { held.0.insert(key); } else { held.0.remove(&key); }
        }
    }
    for (i, button) in [MouseButton::Left, MouseButton::Right].into_iter().enumerate() {
        if let Some(down) = toggles[i + 2] {
            if down { held.1.insert(button); } else { held.1.remove(&button); }
        }
    }
    held
}

pub(super) fn native_held_input(game: &Game<'_>) -> (std::collections::HashSet<KeyCode>, std::collections::HashSet<MouseButton>) {
    let options = [game.settings.toggle_crouch, game.settings.toggle_sprint, game.settings.toggle_attack, game.settings.toggle_use];
    let toggles = std::array::from_fn(|i| (options[i] || game.toggles.is_down(i)).then(|| game.toggles.is_down(i)));
    let mut held = mapped_input(&game.settings.keys, &game.physical_keys, &game.physical_buttons, toggles);
    for (mapping, key) in [("key.forward", KeyCode::KeyW), ("key.back", KeyCode::KeyS), ("key.left", KeyCode::KeyA),
        ("key.right", KeyCode::KeyD), ("key.jump", KeyCode::Space), ("key.sneak", KeyCode::ShiftLeft), ("key.sprint", KeyCode::ControlLeft)] {
        if game.pad.holds_action(mapping) { held.0.insert(key); }
    }
    for (mapping, button) in [("key.attack", MouseButton::Left), ("key.use", MouseButton::Right)] {
        if game.pad.holds_action(mapping) { held.1.insert(button); }
    }
    held
}

pub(super) fn all_off(game: &mut Game<'_>) {
    super::sunset_cyanite_utilities::shutdown();
    super::sunset_cyanite_economy::stopped(game);
    super::sunset_cyanite_drill::shutdown();
    super::sunset_cyanite_storage::shutdown();
    super::sunset_baritone::stop(game, super::sunset_baritone::StopReason::Disabled);
    if let Some(lease) = super::sunset_movement_owner::current() { super::sunset_movement_owner::release(game, lease); }
    watchdog_stop(game, true);
    super::sunset_hands::release_rf(game);
    super::remote::sunset_combat_remote::restore_buttons(game);
    let native_input = native_held_input(game);
    let sprint_owned = with(|s| std::mem::take(&mut s.sprint_owned));
    // Turn modules off before any release method can send or consult settings.
    let restore = with(panic_off_state);
    GUI_TOP.lock().unwrap_or_else(|e| e.into_inner()).0.clear();
    SHAPES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    with(sync_flags);
    let injected = with(|s| std::mem::take(&mut s.injected));
    for k in injected {
        if !native_input.0.contains(&k) { game.keys.remove(&k); }
    }
    super::sunset_server_scan::disabled();
    horizon_client::net::sunset_client_brand::next(None);
    super::agent::sunset_goto::cancel(game, "stopped by Panic");
    super::sunset_gui_move::shutdown(game);
    combat_cleanup(game);
    legit_sneak(game, false);
    super::sunset_scaffold_bridge::stop(game);
    super::sunset_scaffold_normal::stop(game);
    super::sunset_scaffold_telly::stop(game);
    super::sunset_lag::shutdown(game);
    super::sunset_backtrack::shutdown();
    super::sunset_knockback_delay::shutdown();
    super::sunset_packet_mine::toggled(game, "Packet Mine", false);
    super::sunset_vein_miner::stop(game);
    super::sunset_duel::shutdown(game);
    super::sunset_swap_keys::shutdown(game);
    super::sunset_takeoff::shutdown(game);
    super::sunset_drain::shutdown(game);
    super::sunset_hands::shutdown(game);
    if with(|s| s.eating) { eat_stop(game); }
    super::sunset_snap_tap::post_tick(game);
    auto_tool_off(game);
    with(|s| { s.tool_back = None; s.tool_pulled = None; s.eating = false; s.eat_slot = None; });
    turns(|t| { t.queue.clear(); t.carried = false; t.restore = false; });
    BLINK_HELD.store(false, Ordering::Relaxed);
    super::sunset_flight::stop(game);
    super::sunset_flight::damage_reset();
    FLY_DOWN.store(false, Ordering::Relaxed);
    FLY_LAST_Y.store(u64::MAX, Ordering::Relaxed);
    set_last_look(None);
    super::sunset_sky::real(game);
    super::sunset_sky::forget();
    super::sunset_atmosphere::sync(game);
    super::sunset_render::shutdown(game);
    super::sunset_parity::shutdown(game);
    super::sunset_player::shutdown(game);
    super::sunset_extras::shutdown(game);
    super::sunset_refill::shutdown();
    super::sunset_world_parity::tick(game, &[]);
    super::sunset_vanish::tick(game, None);
    super::sunset_base_finder::tick(game, None);
    super::sunset_hunting::tick(game, &[]);
    super::sunset_light_scan::tick(game, &[]);
    super::sunset_sound_detector::tick(game, None);
    super::sunset_gear::tick(game, &[]);
    super::sunset_new_chunks::tick(game, &[]);
    super::sunset_seedcracker::stop();
    super::sunset_ore_sim::stop();
    super::sunset_hud_bars::tick(game, &[], false);
    super::sunset_hud_bars::chat_closed();
    super::sunset_streamer_mode::frame(game);
    if let Some((yaw, pitch)) = restore {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    horizon_client::motion::sunset_motion::set_across(None);
    horizon_client::motion::sunset_motion::set_up(None);
    horizon_client::motion::sunset_motion::set_strafe_dir(None);
    bhop_body(game, None);
    horizon_client::motion::sunset_motion::set_safe_walk(false);
    horizon_client::input::sunset_flags::NO_SLOW.store(false, Ordering::Relaxed);
    horizon_client::motion::sunset_motion::reset();
    let (flying, stepping, jumping) = with(|s| {
        let old = (s.flying, s.stepping, s.high_jumping);
        s.flying = false; s.stepping = false; s.high_jumping = false;
        old
    });
    let spectator = game.spectator();
    if let Some(p) = game.player.as_mut() {
        if flying {
            p.flying = (game.creative && p.flying) || spectator;
            p.may_fly = game.creative || spectator;
            p.flying_speed = 0.05;
        }
        if stepping { let _ = p.set_modifier("minecraft:step_height", "sunset:step", 0.0, "add_value", false); }
        if jumping { let _ = p.set_modifier("minecraft:jump_strength", "sunset:high_jump", 0.0, "add_multiplied_total", false); }
        if sprint_owned && !native_input.0.contains(&KeyCode::ControlLeft) { let _ = p.set_sprinting(false); }
    }
    save(game);
    if gui_open() {
        close_gui(game);
    }
    super::remote::sunset_combat_remote::cancel_input(game);
    // Release methods may temporarily take a real key/button up. Restore the
    // physical state without synthesizing new clicks or replaying queued input.
    if game.native_playing() {
        for key in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::Space, KeyCode::ShiftLeft, KeyCode::ControlLeft] {
            if native_input.0.contains(&key) { game.keys.insert(key); } else { game.keys.remove(&key); }
        }
        game.left = native_input.1.contains(&MouseButton::Left);
        game.right = native_input.1.contains(&MouseButton::Right);
    }
}

// Panic (the module, its key, .panic, or a plugin turning it on): Sunset stops until the game restarts.
pub(super) fn panic_off(game: &mut Game<'_>) {
    shutdown(game, "Panic");
    super::sunset_toast::clear();
}

// .profile save|load|delete|list <name>: the modules' settings kept under a name.
// Profiles (`.profile`): files in the mod's data folder (sunset_profiles), with a backup of the settings before every
// load. Profiles from before 0.9 (kept in the mod's settings) move into files the first time.
fn profile_command(game: &mut Game<'_>, args: &[&str]) {
    let result = profile_action(game, args);
    say(game, &result.unwrap_or_else(|e| format!("Profile: {e}.")));
}

pub(super) fn profile_action(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    use super::sunset_profiles::{self as profiles, BACKUP, HEADER};
        let store = profiles::Store::new(game.native_data_dir(ID))?;
        let old: Vec<String> = game.native_setting(ID, "profiles").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(str::to_owned).collect();
        for n in &old {
            let key = format!("profile-{}", norm(n));
            if let (Ok(file), Some(text)) = (profiles::name(n), game.native_setting(ID, &key))
                && !store.exists(&file)
            {
                store.write(&file, &format!("{HEADER}\n{text}\n"))?;
            }
            game.native_set_setting(ID, &key, "");
        }
        if !old.is_empty() {
            game.native_set_setting(ID, "profiles", "");
        }
        let sub = args.first().map(|a| a.to_ascii_lowercase()).unwrap_or_default();
        if sub == "list" {
            let all = store.list()?;
            let in_use = with(|s| s.selection.label());
            return Ok(if all.is_empty() {
                "No profiles yet. .profile save <name> keeps the current settings.".to_owned()
            } else {
                format!("Profiles: {}. In use: {in_use}", all.join(", "))
            });
        }
        let Some(name) = args.get(1..).filter(|rest| !rest.is_empty()).map(|rest| profiles::name(&rest.join(" "))).transpose()? else {
            return Err("use .profile save|load|delete|export|import <name>, or .profile list".into());
        };
        if name == BACKUP && sub != "load" {
            return Err(format!("{BACKUP} is the settings before the last load; .profile load {BACKUP} puts them back"));
        }
        match sub.as_str() {
            "save" => {
                let text = with(|s| s.profile_text());
                store.write(&name, &format!("{HEADER}\n{text}\n"))?;
                with(|s| s.select(&name));
                save(game);
                Ok(format!("Saved profile {name}."))
            }
            "load" => {
                let text = store.read(&name)?;
                let checked = with(|s| s.check_profile(&text))?;
                let before = with(|s| s.profile_text());
                store.write(BACKUP, &format!("{HEADER}\n{before}\n"))?;
                apply_profile(game, &checked.body, &name);
                Ok(format!("Loaded profile {name}. .profile load {BACKUP} puts the settings back as they were.{}", checked.skipped_note()))
            }
            "delete" => {
                store.delete(&name)?;
                with(|s| s.selection.deleted(&name));
                save(game);
                Ok(format!("Deleted profile {name}."))
            }
            "export" => Ok(format!("Profile {name} is in {}", store.export(&name)?.display())),
            "import" => {
                let text = store.import(&name).map_err(|e| format!("{e} (put {name}.profile in {})", store.imports().display()))?;
                let checked = with(|s| s.check_profile(&text))?;
                store.write(&name, &text)?;
                Ok(format!("Imported profile {name}; .profile load {name} uses it.{}", checked.skipped_note()))
            }
            _ => Err("use .profile save|load|delete|export|import <name>, or .profile list".into()),
        }
}

// .preset [name] [apply]: the pre-made profiles. A name alone says what taking it turns on and off; apply takes it.
fn preset_command(game: &mut Game<'_>, args: &[&str]) {
    use super::sunset_presets::{self as presets, PRESETS};
    let apply = args.len() > 1 && args.last().is_some_and(|a| a.eq_ignore_ascii_case("apply"));
    let name = args[..args.len() - usize::from(apply)].join(" ");
    if name.is_empty() {
        let list: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        return say(game, &format!("Presets: {}. .preset <name> says what it turns on and off.", list.join(", ")));
    }
    let Some(i) = presets::find(&name) else {
        return say(game, &format!("Preset: there is none called {name}."));
    };
    if apply {
        let message = take_preset(game, i).unwrap_or_else(|e| format!("Preset: {e}."));
        return say(game, &message);
    }
    match with(|s| s.preview(&PRESETS[i])) {
        Ok(preview) => {
            for line in preview_lines(&PRESETS[i], &preview) {
                say(game, &line);
            }
            say(game, &format!(".preset {} apply takes it.", PRESETS[i].id));
        }
        Err(e) => say(game, &format!("Preset: {e}.")),
    }
}

// A preview as lines: the preset, then what it turns on and off and how many settings it changes.
pub(super) fn preview_lines(preset: &super::sunset_presets::Preset, preview: &Preview) -> Vec<String> {
    let names = |list: &[&str]| if list.is_empty() { "nothing".to_owned() } else { list.join(", ") };
    let mut lines = vec![
        format!("{} - {}", preset.title, preset.about),
        format!("Turns on: {}", names(&preview.on)),
        format!("Turns off: {}", names(&preview.off)),
    ];
    if preview.settings > 0 {
        lines.push(format!("Changes {} setting{}", preview.settings, if preview.settings == 1 { "" } else { "s" }));
    }
    lines
}

// Takes preset `i`, after a backup of the settings as they are (.profile load before-load brings them back).
pub(super) fn take_preset(game: &mut Game<'_>, i: usize) -> Result<String, String> {
    use super::sunset_profiles::{self as profiles, BACKUP, HEADER};
    let preset = &super::sunset_presets::PRESETS[i];
    let store = profiles::Store::new(game.native_data_dir(ID))?;
    let text = with(|s| s.preset_text(preset))?;
    let before = with(|s| s.profile_text());
    store.write(BACKUP, &format!("{HEADER}\n{before}\n"))?;
    apply_profile(game, &text, preset.id);
    Ok(format!("Took preset {}. .profile load {BACKUP} puts the settings back as they were.", preset.title))
}

// A profile's (or preset's) checked lines taken as `name`: what turning each module on or off does by hand happens for
// those it switches (Scaffold gives back your look, Kill Aura stops blocking and swaps back, Speed lets go of its
// motion...), Hold binds of modules it changed end, and the settings are saved.
fn apply_profile(game: &mut Game<'_>, body: &str, name: &str) {
    super::sunset_baritone::stop(game, super::sunset_baritone::StopReason::User);
    let before: Vec<bool> = with(|s| s.modules.iter().map(|m| m.on).collect());
    let switched = with(|s| {
        s.load_profile(body);
        release_changed_binds(s);
        let snapshot = s.profile_text();
        s.selection.loaded(name, &snapshot);
        s.modules.iter().zip(&before).filter(|(m, was)| m.on != **was).map(|(m, _)| (m.name, m.on)).collect::<Vec<_>>()
    });
    for (module, on) in switched {
        switched_effects(game, module, on);
    }
    save(game);
}

pub(super) fn apply_movement_preset(state: &mut State, style: &str) -> bool {
    let changes: &[(&str, &str, &str)] = match norm(style).as_str() {
        "subtle" => &[
            ("Speed", "Mode", "Smooth"), ("Speed", "Vanilla speed", "3"),
            ("Speed", "Only on ground", "true"), ("Speed", "Acceleration", "0.025"),
            ("Speed", "Deceleration", "0.04"), ("Speed", "Strafe strength", "0.5"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "1"),
            ("Velocity", "Knockback vertical", "1"), ("Velocity", "Explosions horizontal", "1"),
            ("Velocity", "Explosions vertical", "1"),
            ("Scaffold", "Delay (ticks)", "2"), ("Scaffold", "Rotate", "true"),
            ("No Slow", "Mode", "Scale"), ("No Slow", "Ground %", "20"), ("No Slow", "Air %", "20"),
        ],
        "balanced" => &[
            ("Speed", "Mode", "Strafe"), ("Speed", "Strafe speed", "1.3"),
            ("Speed", "Speed limit", "true"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "0.7"),
            ("Velocity", "Knockback vertical", "0.85"),
            ("Scaffold", "Delay (ticks)", "1"), ("Scaffold", "Rotate", "true"),
            ("No Slow", "Mode", "Scale"), ("No Slow", "Ground %", "70"), ("No Slow", "Air %", "70"),
        ],
        "aggressive" => &[
            ("Speed", "Mode", "Custom"), ("Speed", "Vanilla speed", "10"),
            ("Speed", "Only on ground", "false"), ("Speed", "Ground multiplier", "1.2"),
            ("Speed", "Air multiplier", "1.1"), ("Speed", "Acceleration", "0.15"),
            ("Speed", "Deceleration", "0.1"), ("Speed", "Strafe strength", "1"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "0"),
            ("Velocity", "Knockback vertical", "0"),
            ("Scaffold", "Delay (ticks)", "0"), ("Scaffold", "Rotation speed", "180"),
            ("No Slow", "Mode", "Scale"), ("No Slow", "Ground %", "100"), ("No Slow", "Air %", "100"),
        ],
        _ => return false,
    };
    for &(module, setting, value) in changes {
        if let Some(s) = state.modules.iter_mut().find(|m| m.name == module)
            .and_then(|m| m.settings.iter_mut().find(|s| s.name == setting)) {
            s.parse(value);
        }
    }
    true
}

// Setting edits shared by the command and config compatibility tests. Automatic results never use this path.
pub(super) fn seed_settings(state: &mut State, text: &str) {
    match text.to_ascii_lowercase().as_str() {
        "auto" => {
            State::set_field(&mut state.modules, "oresim.seed", "");
            State::set_field(&mut state.modules, "oresim.seedsource", "Auto-crack");
        }
        "clear" => State::set_field(&mut state.modules, "oresim.seed", ""),
        _ => {
            State::set_field(&mut state.modules, "oresim.seed", text);
            State::set_field(&mut state.modules, "oresim.seedsource", "Manual");
        }
    }
}

fn seed_command(game: &mut Game<'_>, args: &[&str]) {
    let text = args.join(" ");
    match text.to_ascii_lowercase().as_str() {
        "" | "status" => {}
        "restart" => {
            super::sunset_baritone::source_changed(game);
            super::sunset_seedcracker::restart();
            super::sunset_ore_sim::stop();
            say(game, "Ore Sim seed: automatic observations cleared; the next eligible tick starts a fresh run. Manual text is unchanged.");
            return;
        }
        _ => {
            super::sunset_baritone::source_changed(game);
            with(|s| seed_settings(s, &text));
            super::sunset_seedcracker::stop();
            super::sunset_ore_sim::stop();
            save(game);
        }
    }
    let module = with(|s| s.module(super::sunset_ore_sim::NAME).cloned());
    if let Some(module) = module {
        for line in super::sunset_ore_sim::status(game, &module) { say(game, &line); }
    }
}

pub(super) fn run(game: &mut Game<'_>, args: &[&str]) {
    let cmd = args[0].to_ascii_lowercase();
    let module_arg = |game: &mut Game<'_>, name: Option<&&str>| -> Option<usize> {
        let Some(name) = name else {
            say(game, "Name a module.");
            return None;
        };
        let found = with(|s| s.find(name));
        if found.is_none() {
            say(game, &format!("No module called {name}. Try .modules"));
        }
        found
    };
    match cmd.as_str() {
        "seed" => seed_command(game, &args[1..]),
        "auradebug" => say(game, &super::sunset_aura::diagnostics()),
        "elytratarget" => say(game, &super::sunset_elytra_target::diagnostics()),
        "antibotdebug" => {
            let id = args.get(1).and_then(|id| id.parse::<u32>().ok());
            for line in super::sunset_antibot::diagnostics(game, id) { say(game, &line); }
        }
        "scan" => super::sunset_server_scan::command(game, &args[1..]),
        "goto" => {
            let text = super::agent::sunset_goto::command(game, &args[1..]).unwrap_or_else(|e| e);
            say(game, &text);
        }
        "baritone" | "b" => {
            let text = super::sunset_baritone::command(game, &args[1..]).unwrap_or_else(|e| e);
            say(game, &text);
        }
        "help" | "commands" => {
            say(game, "Commands:");
            for (c, d) in HELP {
                say(game, &format!("{c} - {d}"));
            }
            for (c, d) in super::sunset_api::help() {
                say(game, &format!("{c} - {d}"));
            }
        }
        "menu" | "gui" => {
            // `.menu esp` opens it with that search typed.
            let search: String = args[1..].join(" ").chars().take(20).collect();
            with(|s| s.gui.search = search);
            open_gui(game);
        }
        "pages" => open_menu(game),
        "toggle" | "t" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                toggle(game, i, true);
            }
        }
        "modules" | "list" => {
            let lines: Vec<String> = with(|s| {
                CATEGORIES
                    .iter()
                    .map(|&c| {
                        let names: Vec<String> = alphabetical(&s.modules, (0..s.modules.len()).filter(|&i| s.modules[i].category == c))
                            .into_iter()
                            .map(|i| &s.modules[i])
                            .map(|m| if m.on { format!("{}*", m.name) } else { m.name.to_owned() })
                            .collect();
                        format!("{}: {}", c.label(), names.join(", "))
                    })
                    .collect()
            });
            for l in lines {
                say(game, &l);
            }
            say(game, "* is on.");
        }
        "settings" | "get" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                let lines: Vec<String> = with(|s| s.modules[i].settings.iter().map(Setting::shown).collect());
                if lines.is_empty() {
                    say(game, "It has no settings.");
                }
                for l in lines {
                    say(game, &l);
                }
            }
        }
        "set" => {
            let Some(i) = module_arg(game, args.get(1)) else {
                return;
            };
            if args.len() < 4 {
                say(game, "Use .set <module> <setting> <value>");
                return;
            }
            // The setting's name is the first words that name one (exactly, else
            // by its start); the rest is the value, spaces and all.
            let (found, value) = with(|s| {
                let m = &s.modules[i];
                for exact in [true, false] {
                    for k in (1..args.len() - 2).rev() {
                        let name = norm(&args[2..2 + k].join(""));
                        let at = m.settings.iter().position(|st| {
                            if exact { norm(st.name) == name } else { norm(st.name).starts_with(&name) }
                        });
                        if let Some(at) = at {
                            return (Some(at), args[2 + k..].join(" "));
                        }
                    }
                }
                (None, String::new())
            });
            let value = value.as_str();
            let result = found.and_then(|at| {
                with(|s| {
                    let m = &mut s.modules[i];
                    let setting = &mut m.settings[at];
                    let alias = m.name == "Kill Aura" && setting.name == "Shield";
                    let seed_changed = m.name == "Ore Sim" && matches!(setting.name, "Seed" | "Seed source" | "Database fetch" | "Cracker threads");
                    let parsed = setting.parse(value);
                    let shown = setting.shown();
                    if parsed && alias {
                        let mode = if m.b("Shield") { "Modern" } else { "Off" };
                        if let Some(s) = m.settings.iter_mut().find(|s| s.name == "AutoBlock") { s.parse(mode); }
                    }
                    Some((parsed, shown, seed_changed))
                })
            });
            match result {
                None => say(game, "No such setting. Try .settings <module>"),
                Some((false, shown, _)) => say(game, &format!("Could not read {value}. Now {shown}")),
                Some((true, shown, seed_changed)) => {
                    if seed_changed {
                        super::sunset_baritone::source_changed(game);
                        super::sunset_seedcracker::stop();
                        super::sunset_ore_sim::stop();
                    }
                    save(game);
                    say(game, &shown);
                }
            }
        }
        "movementpreset" => {
            if let Some(style) = args.get(1) && with(|s| apply_movement_preset(s, style)) {
                save(game);
                say(game, &format!("{style} movement preset applied. Use .settings <module> to inspect values."));
            } else {
                say(game, "Use .movementpreset subtle, balanced or aggressive");
            }
        }
        "bind" => {
            let Some(i) = module_arg(game, args.get(1)) else {
                return;
            };
            let Some(k) = args.get(2) else {
                say(game, "Use .bind <module> <key or mouse1 to mouse5>, or none");
                return;
            };
            let key = if k.eq_ignore_ascii_case("none") {
                None
            } else {
                match key_from_text(k) {
                    Some(key) => Some(key),
                    None => {
                        say(game, &format!("Unknown key {k}."));
                        return;
                    }
                }
            };
            let name = with(|s| {
                set_bind(s, i, key.clone());
                s.modules[i].name
            });
            save(game);
            say(game, &format!("{name} bound to {}", key_label(key.as_deref())));
        }
        "binds" => {
            let lines: Vec<String> = with(|s| {
                s.modules
                    .iter()
                    .filter_map(|m| {
                        let hold = if m.p("Bind mode") == "Hold" { " (hold)" } else { "" };
                        m.key.as_deref().map(|k| format!("{}: {}{hold}", m.name, key_label(Some(k))))
                    })
                    .collect()
            });
            if lines.is_empty() {
                say(game, "No key binds yet.");
            }
            for l in lines {
                say(game, &l);
            }
        }
        "panic" => panic_off(game),
        "debug" => match args.get(1).map(|n| n.parse::<usize>()) {
            Some(Err(_)) => say(game, "Usage: .debug [chat lines]"),
            lines => super::sunset_debug::copy(game, lines.and_then(Result::ok)),
        },
        "profile" | "profiles" | "config" | "cfg" => profile_command(game, &args[1..]),
        "preset" | "presets" => preset_command(game, &args[1..]),
        "reset" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                let name = with(|s| {
                    s.inputs.take_where(|h| h.module == i);
                    let fresh = fresh_module(s, i);
                    s.modules[i] = fresh;
                    s.modules[i].name
                });
                if name == "Server Scan" { super::sunset_server_scan::disabled(); }
                if name == "Ore Sim" {
                    super::sunset_seedcracker::stop();
                    super::sunset_ore_sim::stop();
                }
                save(game);
                say(game, &format!("{name} reset."));
            }
        }
        "friend" | "friends" | "f" => {
            let sub = args.get(1).map(|s| s.to_ascii_lowercase());
            match (sub.as_deref(), args.get(2)) {
                (Some("add"), Some(name)) => {
                    with(|s| {
                        if !s.friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                            s.flip_friend(name);
                        }
                    });
                    save(game);
                    say(game, &format!("{name} is a friend."));
                }
                (Some("remove" | "del" | "rm"), Some(name)) => {
                    with(|s| {
                        if s.friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                            s.flip_friend(name);
                        }
                    });
                    save(game);
                    say(game, &format!("{name} is no longer a friend."));
                }
                _ => {
                    let list = with(|s| s.friends.join(", "));
                    say(game, &format!("Friends: {}", if list.is_empty() { "none" } else { &list }));
                }
            }
        }
        "loadout" | "loadouts" => super::sunset_loadouts::command(game, args),
        "waypoint" | "waypoints" | "wp" => {
            let sub = args.get(1).map(|a| a.to_ascii_lowercase());
            match (sub.as_deref(), args.get(2)) {
                (Some("add" | "set"), Some(name)) => {
                    let Some(p) = game.native_player() else {
                        say(game, "Join a world first.");
                        return;
                    };
                    let w = Waypoint {
                        name: (*name).replace(['|', '='], ""),
                        pos: [p.x.floor() + 0.5, p.y.floor() + 1.0, p.z.floor() + 0.5],
                        dim: p.dimension.to_owned(),
                    };
                    with(|s| {
                        s.waypoints.retain(|o| !o.name.eq_ignore_ascii_case(&w.name) || o.dim != w.dim);
                        s.waypoints.push(w.clone());
                        if let Some(i) = s.find("Waypoints") {
                            s.modules[i].on = true;
                        }
                    });
                    save(game);
                    say(game, &format!("Saved {} at {} {} {}", w.name, w.pos[0].floor(), w.pos[1].floor(), w.pos[2].floor()));
                }
                (Some("remove" | "del" | "rm"), Some(name)) => {
                    with(|s| s.waypoints.retain(|o| !o.name.eq_ignore_ascii_case(name)));
                    save(game);
                    say(game, &format!("Removed {name}."));
                }
                _ => {
                    let lines: Vec<String> = with(|s| {
                        s.waypoints
                            .iter()
                            .map(|w| format!("{}: {} {} {} ({})", w.name, w.pos[0].floor(), w.pos[1].floor(), w.pos[2].floor(), w.dim.trim_start_matches("minecraft:")))
                            .collect()
                    });
                    if lines.is_empty() {
                        say(game, "No waypoints. Use .waypoint add <name>");
                    }
                    for l in lines {
                        say(game, &l);
                    }
                }
            }
        }
        "vclip" | "hclip" => {
            let Some(d) = args.get(1).and_then(|a| a.parse::<f64>().ok()) else {
                say(game, &format!("Use .{cmd} <blocks>"));
                return;
            };
            let yaw = f64::from(game.yaw).to_radians();
            if let Some(p) = game.player.as_mut() {
                if cmd == "vclip" {
                    p.position.y += d;
                } else {
                    p.position.x += -yaw.sin() * d;
                    p.position.z += yaw.cos() * d;
                }
                p.velocity = horizon_core::geometry::Vec3::ZERO;
            }
        }
        "plugins" => super::sunset_api::list(game),
        _ => {
            if !super::sunset_api::command(game, &cmd, &args[1..]) {
                say(game, &format!("Unknown command .{cmd}. Try .help"));
            }
        }
    }
}
