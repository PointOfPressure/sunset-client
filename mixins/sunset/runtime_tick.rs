// Runtime part (see runtime.rs): the game tick: before and after it, and each module's turn in it.

use super::{Game, Screen};
use super::sunset_util::Sweep;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::atomic::Ordering;
use winit::keyboard::KeyCode;
use super::sunset::*;

// ---------------------------------------------------------------- ticks

// Before the game ticks: keys this mod holds, abilities, step height.
pub(super) fn pre_tick(game: &mut Game<'_>) {
    finish_shutdown(game);
    if !running() { return; }
    sync_binds(game);
    // Single player's own server ticks a game of its own (no window) through here too: it must leave the player's
    // state alone, or it clears Velocity's baseline and Speed's push between the client's ticks.
    if !game.native_window() {
        return;
    }
    if game.player.is_none() {
        fall_died();
    } else {
        say_notices(game);
    }
    super::sunset_presence::tick(game);
    horizon_client::motion::sunset_motion::expire_attack_slow(game.ticks);
    let packets = horizon_client::net::sunset_packets::take();
    super::sunset_antibot::packets(game, &packets);
    super::sunset_gui_move::packets(game, &packets);
    let inventory_stop = super::sunset_gui_move::pre_tick(game);
    horizon_client::motion::sunset_motion::set_freeze(inventory_stop);
    if !game.native_on(ID) || game.player.is_none() {
        super::sunset_hands::stop_autoblock(game);
        combat_cleanup(game);
        horizon_client::motion::sunset_motion::set_across(None);
        horizon_client::motion::sunset_motion::set_up(None);
        horizon_client::motion::sunset_motion::set_strafe_dir(None);
        bhop_body(game, None);
        horizon_client::motion::sunset_motion::set_safe_walk(false);
        horizon_client::motion::sunset_motion::set_screen_keys(None);
        restore_scaffold_rotation(game);
        super::sunset_scaffold_bridge::stop(game);
        super::sunset_scaffold_normal::stop(game);
        super::sunset_scaffold_telly::stop(game);
        with(|s| {
            s.velocity = None; s.blast = [0.0; 3]; s.knockback_ticks = 0; s.ncp.reset();
            s.watchdog.reset(); s.legacy_recovery = Default::default();
        });
        return;
    }
    ensure_loaded(game);
    super::sunset_antibot::tick(game);
    revalidate_targets(game);
    let same_combat_scene = super::sunset_combat_control::same_scene(game);
    if super::sunset_combat_control::begin_tick(game) {
        super::sunset_hands::scene_changed(game, same_combat_scene);
        with(|s| { s.ncp.reset(); s.watchdog.reset(); s.legacy_recovery = Default::default(); });
        super::sunset_scaffold_telly::scene_changed(game);
        drop_strikes();
        super::sunset_melee::cancel(game);
        super::sunset_auto_mace::cancel();
    }
    combat_packets(game, &packets);
    ensure_loaded(game);
    super::sunset_api::pre_tick(game);
    if !running() { return; }
    // The game sends movement later in this tick. A click after that movement trips Grim's PacketOrderO.
    if with(|s| s.module("Chest Swap").is_some_and(|m| m.on && m.p("Mode") == "Swap"))
        && !with(|s| s.is_on("Blink"))
        && !super::sunset_lag::pending()
        && super::sunset_combat_control::owner().is_none()
        && chest_swap(game)
        && let Some(i) = with(|s| s.find("Chest Swap"))
    {
        toggle(game, i, false);
    }
    super::sunset_parity::pre_tick(game);
    MOVED_THIS_TICK.store(false, Ordering::Relaxed);
    crit_sprint_back(game);
    // Slots borrowed for earlier hits come back first, before this tick's move packet and any hit or use in it, one
    // slot change at most, and not while a hit or its weapon waits (send_strikes changes the slot itself).
    let (hits, selected) = (strike_waiting() || super::sunset_melee::due_waiting(), game.selected);
    if !hits && !super::sunset_melee::lease_back(game) && !super::sunset_duel::catch_back(game) {
        strike_weapon_back(game);
    }
    super::sunset_hands::before_hits(game, strike_due() || super::sunset_melee::due_waiting());
    send_strikes(game);
    super::sunset_melee::due(game);
    // Auto Spear's Extended reach: its slot back, or Trigger Bot's stab, in a tick with no hit or slot change before.
    let hits = super::sunset_melee::reach_tick(game, hits || game.selected != selected || super::sunset_melee::acted(game)) || hits;
    // Auto Mace's slot changes, clicks and hits: in a tick with no hit and no slot change before them.
    let hits = super::sunset_auto_mace::pre_tick(game, hits || game.selected != selected) || hits;
    // Pearl Catch's wind charge: in a tick with no hit and no slot change before it.
    super::sunset_duel::catch_throw(game, hits || game.selected != selected);
    // Cart's steps: in a tick with no hit and no slot change before them.
    super::sunset_cart::pre_tick(game, hits || game.selected != selected);
    // Shield and Offhand's Double hand: likewise.
    super::sunset_swap_keys::pre_tick(game, hits || game.selected != selected);
    // Pearl Catch's, Wind Jump's, Auto Gap's and PvP Bot's throws: likewise.
    super::sunset_throws::pre_tick(game, hits || game.selected != selected);
    // Anchor Aura's Pearl catch: likewise.
    super::sunset_anchor_catch::pre_tick(game, hits || game.selected != selected);
    // Liquid Filler's Drain and Chest Swap's Take off: likewise.
    super::sunset_drain::pre_tick(game, hits || game.selected != selected);
    super::sunset_takeoff::pre_tick(game, hits || game.selected != selected);
    // Middle Click Extra's use and slot back: likewise.
    let hits = middle_click_tick(game, hits || game.selected != selected) || hits;
    let mods = snapshot();
    // Auto Tool's slot changes and swaps: before the tick's dig packets, likewise.
    match mods.iter().find(|m| m.name == "Auto Tool" && m.on) {
        Some(m) => auto_tool(game, m, hits || game.selected != selected),
        None => auto_tool_off(game),
    }
    super::sunset_render::pre_tick(game, &mods);
    super::sunset_player::speed_mine(game, mods.iter().find(|m| m.name == "Speed Mine" && m.on));
    super::sunset_player::no_break_reset(game, mods.iter().find(|m| m.name == "No Break Reset" && m.on));
    let on = |n: &str| mods.iter().any(|m| m.name == n && m.on);
    let get = |n: &str| mods.iter().find(|m| m.name == n);
    let playing = game.native_playing();
    // The click GUI holds no movement keys, as a screen would.
    if gui_open() {
        game.keys.clear();
    }
    let mut want = Vec::new();
    if on("Auto Walk") && playing {
        want.push(KeyCode::KeyW);
    }
    if on("Sprint") && playing {
        if game.player.as_ref().is_some_and(|p| !p.sprinting) && !native_held_input(game).0.contains(&KeyCode::ControlLeft) {
            with(|s| s.sprint_owned = true);
        }
        want.push(KeyCode::ControlLeft);
    } else {
        with(|s| s.sprint_owned = false);
    }
    if on("Auto Jump") && playing && move_input(game) != (0.0, 0.0) {
        want.push(KeyCode::Space);
    }
    // Speed's Bhop presses jump for this tick on the ground: the game's own jump, at Hop height.
    let bhop = get("Speed").filter(|m| m.on && m.p("Mode") == "Bhop" && playing);
    if bhop.is_some_and(|m| bhop_hops(game, m)) {
        want.push(KeyCode::Space);
    }
    if on("Sneak") && playing {
        want.push(KeyCode::ShiftLeft);
    }
    // Criticals' Jump mode presses jump for this tick: the game's own jump.
    if CRIT_JUMP.swap(false, Ordering::Relaxed) && playing {
        want.push(KeyCode::Space);
    }
    // Parkour presses jump for this tick: the game's own jump (a sprint jump's push too).
    if let Some(m) = get("Parkour").filter(|m| m.on)
        && playing
        && parkour_jump(game, &m)
    {
        want.push(KeyCode::Space);
    }
    if let Some(m) = get("Anti AFK").filter(|m| m.on)
        && playing
    {
        want.extend(super::sunset_player::afk_keys(game, &m));
    }
    if let Some(m) = get("Safe Walk").filter(|m| m.on && m.p("Mode") == "Sneak")
        && playing
        && edge_sneak(game, &m)
    {
        want.push(KeyCode::ShiftLeft);
    }
    let scaffold_walk = get("Scaffold").is_some_and(|m| m.on && m.p("Mode") == "Normal" && m.b("Safe walk"));
    let stop_walk = get("Safe Walk").is_some_and(|m| m.on && m.p("Mode") == "Stop");
    horizon_client::motion::sunset_motion::set_safe_walk((stop_walk || scaffold_walk) && playing);
    // Snap Tap: of two opposite move keys held, the one pressed last (GUI Move's keys too).
    let snapped = super::sunset_snap_tap::pre_tick(game, get("Snap Tap").filter(|m| m.on));
    // GUI Move: behind a screen the game takes no move keys, so its movement is given the ones held and walks you as
    // it would without the screen (its own speeds, sprint, jumps, and the walk's look in first and third person).
    let held = |k: KeyCode| !snapped.contains(&k) && (game.keys.contains(&k) || game.physical_keys.contains(&k));
    // Flight flies you behind a screen by itself.
    let flying = on("Flight") && super::sunset_flight::damage_open();
    let screen_keys = get("GUI Move").filter(|m| m.on && !flying && !gui_open() && gui_move_screen(game, m)).map(|m| horizon_client::sprint::Keys {
        forward: held(KeyCode::KeyW),
        backward: held(KeyCode::KeyS),
        left: held(KeyCode::KeyA),
        right: held(KeyCode::KeyD),
        jump: m.b("Jump") && held(KeyCode::Space),
        shift: m.b("Sneak") && (m.p("Mode") != "GrimAC" || !game.settings.toggle_crouch) && held(KeyCode::ShiftLeft),
        sprint: m.b("Sprint") && (m.p("Mode") != "GrimAC" || !game.settings.toggle_sprint) && (on("Sprint") || held(KeyCode::ControlLeft)),
        analog: None,
    });
    horizon_client::motion::sunset_motion::set_screen_keys(screen_keys);
    let mut injected = Vec::new();
    for k in want {
        if game.keys.insert(k) {
            injected.push(k);
        }
    }
    if let Some(m) = get("Scaffold") {
        scaffold_preset(game, m);
    }
    // Scaffold's Legit and Godbridge sneak, turn and click before the game's tick moves you.
    let bridging = get("Scaffold")
        .filter(|m| m.on && matches!(m.p("Mode"), "Legit" | "Godbridge") && !scaffold_paused(m, on("Kill Aura") && super::sunset_aura::engaged(), on("Blink")))
        .filter(|_| playing && game.remote.is_some() && super::sunset_combat_control::available("Scaffold"));
    super::sunset_scaffold_bridge::pre_tick(game, bridging);
    // Scaffold's Normal clicks, looks and turns the keys before the game's tick moves you (its look rides the move).
    let normal = get("Scaffold")
        .filter(|m| m.on && m.p("Mode") == "Normal" && !scaffold_paused(m, on("Kill Aura") && super::sunset_aura::engaged(), on("Blink")))
        .filter(|_| playing && game.remote.is_some() && super::sunset_combat_control::available("Scaffold"));
    if normal.is_some() { super::sunset_scaffold_telly::pre_tick(game, None); }
    if get("Scaffold").is_some_and(|m| m.on && m.p("Mode") == "Telly") {
        super::sunset_scaffold_normal::stop(game);
    } else {
        super::sunset_scaffold_normal::pre_tick(game, normal);
    }
    let telly = get("Scaffold")
        .filter(|m| m.on && m.p("Mode") == "Telly" && !scaffold_paused(m, on("Kill Aura") && super::sunset_aura::engaged(), on("Blink")))
        .filter(|_| playing && !inventory_stop && game.remote.is_some() && super::sunset_combat_control::available("Scaffold"));
    if normal.is_none() { super::sunset_scaffold_telly::pre_tick(game, telly); }
    // Inventory Manager's Slots mode: its action goes before the tick's move packet (inventory_slots.rs).
    super::sunset_inventory_slots::pre_tick(game, get("Inventory Manager").filter(|m| m.on && m.p("Mode") == "Slots"));
    // Offhand's Silent open holds your move keys and clicks while its totem moves.
    super::sunset_combat_inventory::pre_tick(game, get("Offhand").filter(|m| m.on && playing));
    velocity(game, get("Velocity").filter(|m| m.on));
    let across = match get("Speed").filter(|m| m.on) {
        Some(m) => speed(game, m),
        None => {
            watchdog_stop(game, true);
            with(|s| { s.ncp.reset(); s.ncp_hop.reset(); s.legacy_recovery = Default::default(); });
            None
        }
    };
    horizon_client::motion::sunset_motion::set_across(across.map(|(xz, _)| xz));
    if get("Speed").is_some_and(|m| m.on && matches!(m.p("Mode"), "NCP" | "Watchdog")) {
        horizon_client::motion::sunset_motion::set_ballistic_jump(across.and_then(|(_, scale)| scale));
        if get("Speed").is_some_and(|m| m.p("Mode") == "Watchdog") {
            horizon_client::motion::sunset_motion::set_legacy_velocity(across.map(|(xz, _)| xz));
        }
    } else {
        horizon_client::motion::sunset_motion::set_up(across.and_then(|(_, y)| y));
    }
    bhop_body(game, bhop);
    // Kill Aura's Target strafe: the move this tick takes goes round its target.
    let strafe = get("Kill Aura").filter(|m| m.on && m.b("Target strafe") && playing);
    horizon_client::motion::sunset_motion::set_strafe_dir(strafe.and_then(|m| super::sunset_target_strafe::direction(game, m, on("Speed"))));
    // Damage fly: Flight on but waiting for a hurt flies as if off (flight.rs).
    let flight = match get("Flight").filter(|m| m.on) {
        Some(m) => Some(m).filter(|m| super::sunset_flight::damage_tick(game, m)),
        None => {
            super::sunset_flight::damage_reset();
            None
        }
    };
    flight_pre(game, flight);
    let step = get("Step").filter(|m| m.on && m.b("Step up"));
    let high_jump = get("High Jump").filter(|m| m.on);
    let (was_flying, was_stepping, was_high_jumping) = with(|s| (s.flying, s.stepping, s.high_jumping));
    let (creative, spectator_mode) = (game.creative, game.spectator());
    let step_to = step.map_or(0.6, |st| step_height(game, st));
    if let Some(p) = game.player.as_mut() {
        if flight.is_none() && was_flying {
            p.flying = creative && p.flying || spectator_mode;
            p.may_fly = creative || spectator_mode;
            p.flying_speed = 0.05;
        }
        if step.is_some() {
            let _ = p.set_modifier("minecraft:step_height", "sunset:step", step_to - 0.6, "add_value", true);
        } else if was_stepping {
            let _ = p.set_modifier("minecraft:step_height", "sunset:step", 0.0, "add_value", false);
        }
        // High Jump multiplies the jump's own speed, so the jump starts higher from its first tick.
        if let Some(h) = &high_jump {
            let _ = p.set_modifier("minecraft:jump_strength", "sunset:high_jump", h.n("Multiplier") - 1.0, "add_multiplied_total", true);
        } else if was_high_jumping {
            let _ = p.set_modifier("minecraft:jump_strength", "sunset:high_jump", 0.0, "add_multiplied_total", false);
        }
    }
    with(|s| {
        s.injected.extend(injected);
        s.flying = flight.is_some_and(|m| m.p("Mode") == "Abilities");
        s.stepping = step.is_some();
        s.high_jumping = high_jump.is_some();
    });
    super::sunset_melee::pre_input(game);
    super::sunset_duel::pre_input(game);
    super::sunset_throws::pre_input(game);
    super::sunset_takeoff::pre_input(game);
}

// How high Step lets you climb this tick: the setting, or vanilla's 0.6 when
// its conditions say no (Sunset's Step: active when, safe step).
fn step_height(game: &Game<'_>, m: &Module) -> f64 {
    let sneaking = game.keys.contains(&KeyCode::ShiftLeft);
    let active = match m.p("Active when") {
        "Sneaking" => sneaking,
        "Not sneaking" => !sneaking,
        _ => true,
    };
    if !active {
        return 0.6;
    }
    if m.b("Safe step")
        && let Some(p) = game.player.as_ref()
    {
        let health = (p.health.health() + p.health.absorption()) as f64;
        let at = [p.position.x, p.position.y, p.position.z];
        let hurt = super::sunset_util::crystal_damage(game, at);
        // Safe means staying above the limit now and after the nearest crystal goes off.
        if health <= m.n("Step health") || health - hurt <= m.n("Step health") {
            return 0.6;
        }
    }
    m.n("Height")
}

// The screens GUI Move walks you in: the inventory and containers (not an anvil's rename box) and, with "All menus",
// the pause menu, the Options pages with no search box, Statistics, Advancements and the creative tabs but the search. Never
// a screen you type in. (A single-player game stops while the pause menu and the like are open: nothing moves there.)
fn gui_move_screen(game: &Game<'_>, m: &Module) -> bool {
    if game.screen == Screen::Inventory {
        let (id, kind) = game.remote_menu();
        return !game.recipe_search_focused() && super::sunset_util::short(&kind) != "anvil"
            && (m.p("Mode") != "GrimAC" || (id == 0 && kind == "inventory"));
    }
    if m.p("Screens") != "All menus" {
        return false;
    }
    match game.screen {
        Screen::Paused | Screen::Stats | Screen::Advancements => true,
        Screen::Options => {
            use horizon_client::options_ui::Page;
            let options = &game.ui.options;
            options.selected_key.is_none()
                && !matches!(options.page(), Page::Video | Page::Controls | Page::KeyBinds | Page::Language | Page::Packs)
        }
        Screen::Creative => m.p("Mode") != "GrimAC" && game.creative_screen.tabs.get(game.creative_screen.selected).is_some_and(|t| !t.search),
        _ => false,
    }
}

// After the game ticks: let go of held keys, then each module's work.
// After the tick: first what waited for the look its move packet carried, then the modules, then (in a tick that
// sent no move packet) the one that carries the next look.
pub(super) fn post_tick(game: &mut Game<'_>) {
    finish_shutdown(game);
    if !running() { return; }
    super::sunset_scaffold_telly::post_tick(game);
    super::sunset_scaffold_normal::post_tick(game);
    super::sunset_snap_tap::post_tick(game);
    super::sunset_scaffold_bridge::post_tick(game);
    super::sunset_combat_inventory::post_tick(game);
    if !game.native_window() || !game.native_on(ID) {
        return;
    }
    super::sunset_sky::keep_shown(game);
    super::sunset_melee::post_input(game);
    turned(game);
    super::sunset_api::tick(game);
    if !running() { return; }
    module_tick(game);
    end_turns(game);
}

fn module_tick(game: &mut Game<'_>) {
    if !running() { return; }
    ensure_loaded(game);
    let injected = with(|s| std::mem::take(&mut s.injected));
    for k in injected {
        if !game.physical_keys.contains(&k) {
            game.keys.remove(&k);
        }
    }
    let mods = snapshot();
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    // Once a session, in a world: how to get to the menu.
    if game.remote.is_some() && game.player.is_some() && !WELCOMED.swap(true, Ordering::Relaxed) {
        say(game, &format!("{VERSION} is on. Press ` for the menu (or Right Shift), type .help for the commands."));
    }
    let ticks = with(|s| {
        s.ticks += 1;
        s.ticks
    });

    // Fullbright: the renderer's own fullbright, while on or while the
    // player's own extra is.
    let fullbright = get("Fullbright").is_some() || game.settings.extras.fullbright;
    if let Some(r) = game.renderer.as_mut() {
        r.fullbright = fullbright;
    }

    if game.player.is_none() {
        super::sunset_player::inventory_reset();
        return;
    }
    // Debug Report: on for one tick (its button or bind), it copies the report.
    if get(super::sunset_debug::NAME).is_some() {
        if let Some(i) = with(|s| s.find(super::sunset_debug::NAME)) {
            toggle(game, i, false);
        }
        super::sunset_debug::copy(game, None);
    }

    let dead = game.screen == Screen::Dead;
    if let Some(m) = get("Death Location") {
        if dead && !with(|s| s.dead) {
            death_location(game, m);
        } else if !dead && m.b("Clear on reach") {
            reach_deaths(game);
        }
    }
    with(|s| s.dead = dead);
    if get("Auto Respawn").is_some() && game.screen == Screen::Dead {
        let wait = with(|s| {
            s.respawn_wait = s.respawn_wait.saturating_sub(1);
            let w = s.respawn_wait;
            if w == 0 {
                s.respawn_wait = 20;
            }
            w
        });
        if wait == 0 {
            let _ = game.respawn_player();
        }
        return;
    }
    if game.screen == Screen::Dead {
        return;
    }

    if get("Panic").is_some() {
        panic_off(game);
        return;
    }
    if let Some(m) = get("Break Delay") {
        let wait = m.n("Cooldown (ticks)") as u32;
        game.destroy_delay = game.destroy_delay.min(wait);
        super::remote::sunset_remote::clear_destroy_delay(game, wait);
    }

    // GUI Move: the game ignores the keys while a menu is open, so what is held steers by hand. Flight,
    // Speed and the rest read the same keys, so they work with it open too.
    let screen_move = get("GUI Move").filter(|m| !gui_open() && gui_move_screen(game, m));
    let held = |k: KeyCode| game.keys.contains(&k) || game.physical_keys.contains(&k);
    let screen_keys = screen_move.as_ref().map(|m| {
        (
            f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS))),
            f64::from(i8::from(held(KeyCode::KeyA)) - i8::from(held(KeyCode::KeyD))),
            m.b("Jump") && held(KeyCode::Space),
            m.b("Sneak") && held(KeyCode::ShiftLeft),
            m.b("Arrows"),
            [held(KeyCode::ArrowLeft), held(KeyCode::ArrowRight), held(KeyCode::ArrowUp), held(KeyCode::ArrowDown)],
        )
    });
    if let Some((.., true, arrows)) = screen_keys {
        let turn = |neg: bool, pos: bool| f32::from(i8::from(pos) - i8::from(neg));
        let speed = screen_move.as_ref().map_or(6.0, |m| m.n("Rotate speed") as f32);
        let vertical = if screen_move.as_ref().is_some_and(|m| m.p("Mode") == "GrimAC") { speed } else { speed * (2.0 / 3.0) };
        game.yaw += turn(arrows[0], arrows[1]) * speed;
        game.pitch = (game.pitch + turn(arrows[2], arrows[3]) * vertical).clamp(-90.0, 90.0);
    }
    let (yaw, pitch) = (game.yaw, game.pitch);
    let flat = |f: f64, s: f64| {
        let a = f64::from(yaw).to_radians();
        let (x, z) = (-a.sin() * f + a.cos() * s, a.cos() * f + a.sin() * s);
        let len = (x * x + z * z).sqrt();
        (len > 0.0).then(|| [x / len, z / len])
    };
    let dir = move_direction(game).or_else(|| screen_keys.and_then(|(f, s, ..)| flat(f, s)));
    let jump = game.native_playing() && game.keys.contains(&KeyCode::Space) || screen_keys.is_some_and(|k| k.2);
    let sneak = game.native_playing() && game.keys.contains(&KeyCode::ShiftLeft) || screen_keys.is_some_and(|k| k.3);
    let forward = move_input(game).0 > 0.0 || screen_keys.is_some_and(|k| k.0 > 0.0);
    let dim = game.dim.id();
    let flight = get("Flight").filter(|_| super::sunset_flight::damage_open());
    let flight_abilities = flight.is_some_and(|f| f.p("Mode") == "Abilities");
    flight_post(game, flight, jump, sneak, screen_keys.and(dir));

    if let Some(p) = game.player.as_mut() {
        // Behind a screen, Flight's Abilities mode flies by hand at its speed, as creative flight does (about 11 blocks
        // a second at the default flying speed); and GUI Move's Sprint off stops a sprint.
        if let Some((.., sneaking, _, _)) = screen_keys {
            if flight_abilities && p.flying {
                let fly = f64::from(p.flying_speed) / 0.05 * 10.9 / 20.0;
                if let Some([x, z]) = dir {
                    (p.velocity.x, p.velocity.z) = (x * fly, z * fly);
                }
                p.velocity.y = if jump { fly * 0.6 } else if sneaking { -fly * 0.6 } else { 0.0 };
            }
            if p.sprinting && screen_move.as_ref().is_some_and(|m| !m.b("Sprint")) {
                let _ = p.set_sprinting(false);
            }
        }
        if get("No Jump Delay").is_some() {
            p.no_jump_delay = 0;
        }
        if let Some(g) = get("Glide")
            && !p.ground.on_ground
            && !p.flying
            && !p.fall_flying
        {
            p.velocity.y = p.velocity.y.max(-g.n("Fall speed") / 20.0);
            p.fall_distance = 0.0;
        }
        if let Some(sp) = get("Spider")
            && p.collision.horizontal
            && dir.is_some()
        {
            p.velocity.y = sp.n("Climb speed");
            p.fall_distance = 0.0;
        }
        if let Some(e) = get("Elytra Boost")
            && p.fall_flying
            && forward
        {
            let look = horizon_client::gizmos::view_vector(pitch, yaw);
            let a = e.n("Boost");
            p.velocity.x += look[0] * a;
            p.velocity.y += look[1] * a;
            p.velocity.z += look[2] * a;
            let v = &mut p.velocity;
            let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            let max = e.n("Max speed");
            if len > max {
                let k = max / len;
                (v.x, v.y, v.z) = (v.x * k, v.y * k, v.z * k);
            }
        }
        // Elytra Fly (Sunset's Vanilla mode): gliding, the keys set the move outright: Horizontal speed (blocks a tick)
        // the way they point, up or down at Vertical speed with jump or sneak, else a slow sink (Fall multiplier).
        // Auto take off opens the elytra as soon as a jump starts to fall; Stop in water lets water end the glide.
        if let Some(e) = get("Elytra Fly")
            && game.armor[1].as_ref().is_some_and(|s| s.item.ends_with("elytra"))
            && !(e.b("Stop in water") && p.fluids.was_touching_water)
        {
            if e.b("Auto take off") && !p.fall_flying && !p.ground.on_ground && p.velocity.y < 0.0 && jump {
                p.fall_flying = true;
            }
            if p.fall_flying {
                let h = e.n("Horizontal speed");
                let [x, z] = dir.map_or([0.0, 0.0], |[x, z]| [x * h, z * h]);
                let y = if jump {
                    e.n("Vertical speed")
                } else if sneak {
                    -e.n("Vertical speed")
                } else {
                    -e.n("Fall multiplier")
                };
                p.velocity = horizon_core::geometry::Vec3::new(x, y, z);
            }
        }
        // Slippy: on the ground with no move key held, the last tick's slowing was the block's friction; it becomes
        // Friction's (a walk keeps its own pace: the game's push on a slippery block is smaller than this would make).
        if let Some(m) = get("Slippy").filter(|_| p.ground.on_ground && dir.is_none()) {
            let under = game.world.block(p.position.x.floor() as i32, (p.position.y - 0.5).floor() as i32, p.position.z.floor() as i32);
            let was = f64::from(game.res.facts().get(under).friction);
            if was > 0.0 {
                let k = m.n("Friction") / was;
                p.velocity.x *= k;
                p.velocity.z *= k;
            }
        }
        // Jesus (Sunset's Solid mode): on the top of water or lava you stand as on a block, at the liquid's top, unless
        // you sneak (Dip on sneak), are burning in lava's place (Dip if burning) or are already under it.
        if let Some(j) = get("Jesus") {
            let feet = [p.position.x.floor() as i32, (p.position.y - 0.05).floor() as i32, p.position.z.floor() as i32];
            let below = game.world.block(feet[0], feet[1], feet[2]);
            let f = game.res.facts().get(below);
            let name = super::sunset_util::short(&f.name);
            let liquid = (j.b("Water") && name == "water") || (j.b("Lava") && name == "lava");
            let top = f64::from(feet[1] + 1);
            let dip = (j.b("Dip on sneak") && sneak) || (j.b("Dip if burning") && p.fire_ticks > 0 && name == "water");
            if liquid && !dip && p.position.y >= top - 0.4 && p.velocity.y <= 0.0 && !jump {
                p.position.y = top;
                p.velocity.y = 0.0;
                p.ground.on_ground = true;
                p.fall_distance = 0.0;
            }
        }
        // Velocity's Sinking: in water or lava you stay level unless you jump or sneak.
        if get("Velocity").is_some_and(|m| m.b("Sinking"))
            && !jump
            && !sneak
            && (p.fluids.was_touching_water || p.is_in_lava())
            && p.velocity.y < 0.0
        {
            p.velocity.y = 0.0;
        }
        if get("Anti Void").is_some() {
            let floor = if dim == "minecraft:overworld" { -64.0 } else { 0.0 };
            if p.position.y < floor && p.velocity.y < 0.0 {
                p.velocity.y = 0.6;
                p.fall_distance = 0.0;
            }
        }
    }

    remember_velocity(game);
    let alive = game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    if !alive {
        fall_died();
    } else if fall_cleared(game) {
        fall(|f| (f.counted, f.kept) = (0.0, false));
    }
    // Blink: let go of what it held when it turns off, or at its limit.
    {
        let max = get("Blink").map_or(0.0, |m| m.n("Max ticks")) as u32;
        let held = BLINK_HELD.load(Ordering::Relaxed);
        let ticks = with(|s| {
            s.blink_ticks = if get("Blink").is_some() && held { s.blink_ticks + 1 } else { 0 };
            s.blink_ticks
        });
        if held && (get("Blink").is_none() || (max > 0 && ticks >= max)) {
            // Moves go through again from the next tick; this one carries the player.
            let was_on = BLINK.swap(false, Ordering::Relaxed);
            flush_blink(game);
            BLINK.store(was_on && get("Blink").is_some(), Ordering::Relaxed);
            with(|s| s.blink_ticks = 0);
        }
    }
    let (jump_now, ground_now) = (
        game.native_playing() && game.keys.contains(&KeyCode::Space),
        game.player.as_ref().is_some_and(|p| p.ground.on_ground),
    );
    let was_ground = with(|s| std::mem::replace(&mut s.was_ground, ground_now));
    if let Some(p) = game.player.as_mut() {
        if let Some(m) = get("Long Jump")
            && was_ground
            && !ground_now
            && jump_now
            && p.velocity.y > 0.3
        {
            p.velocity.x *= m.n("Multiplier");
            p.velocity.z *= m.n("Multiplier");
        }
        if let Some(m) = get("Step").filter(|m| m.b("Reverse step"))
            && was_ground
            && !ground_now
            && !jump_now
            && p.velocity.y <= 0.0
            && !p.flying
        {
            p.velocity.y = -m.n("Fall speed");
        }
        if let Some(m) = get("Fast Climb")
            && p.last_climbable.is_some()
            && (forward || jump_now)
            && !p.flying
        {
            p.velocity.y = m.n("Speed");
        }
    }
    if alive && let Some(m) = get("Auto Log") && game.remote.is_some()
        && game.player.as_ref().is_some_and(|p| f64::from(p.health.health()) <= m.n("Health"))
    {
        let health = game.player.as_ref().map_or(0.0, |p| p.health.health());
        if let Some(i) = with(|s| s.find("Auto Log")) {
            toggle(game, i, false);
        }
        let _ = game.leave_server(Some(format!("Sunset Auto Log: health was {health}")));
        return;
    }
    scaffold_fade();
    // A combat macro or Crystal/Anchor Aura has the hotbar: the inventory modules wait.
    let combat_hand = super::sunset_combat_control::owner().is_some() || super::sunset_gui_move::inventory_busy();
    // Auto Armor: Inventory Manager's, on by its own switch whether or not the rest of Inventory Manager is.
    if let Some(m) = mods.iter().find(|m| m.name == "Inventory Manager" && m.b("Auto Armor")).filter(|_| !combat_hand) {
        auto_armor(game, m);
    }
    if let Some(m) = get("Auto Replenish").filter(|_| !combat_hand) {
        super::sunset_refill::replenish(game, m);
        if m.p("Mode") == "Silent" {
            auto_replenish(game, m);
        }
    }
    if let Some(m) = get("Notifier") {
        notify(game, m);
    } else {
        with(|s| s.notify = None);
    }
    if let Some(m) = get("Hole ESP") {
        scan_holes(game, m, ticks);
    } else {
        with(|s| s.holes = Sweep::new());
    }
    if let Some(m) = get("Nuker") {
        nuker(game, m);
    } else if let Some((pos, _)) = with(|s| s.nuke.take()) {
        // Turned off in the middle of a block: the server stops digging it too.
        let seq = super::sunset_util::sequence(game);
        game.send(S::PlayerAction(play::PlayerAction { action: 2, pos, direction: 1, sequence: seq }));
    }
    if let Some(m) = get("Logout Spots") {
        track_logouts(game, m);
    } else {
        with(|s| {
            s.spots.clear();
            s.last_pos.clear();
        });
    }
    if let Some(m) = get("Auto Eat").filter(|_| super::sunset_combat_control::available("Auto Eat")) {
        auto_eat(game, m);
    } else if with(|s| s.eating) {
        eat_stop(game);
    }
    // Breadcrumbs: a point every few ticks once the player has moved.
    if let Some(m) = get("Breadcrumbs") {
        let max = m.n("Length") as usize;
        if let Some(p) = game.player.as_ref() {
            let here = [p.position.x, p.position.y + 0.1, p.position.z];
            with(|s| {
                let far = s
                    .crumbs
                    .last()
                    .is_none_or(|l| (l[0] - here[0]).powi(2) + (l[1] - here[1]).powi(2) + (l[2] - here[2]).powi(2) > 0.25);
                if far && ticks.saturating_sub(s.crumb_at) >= 2 {
                    s.crumbs.push(here);
                    s.crumb_at = ticks;
                }
                let over = s.crumbs.len().saturating_sub(max);
                s.crumbs.drain(..over);
            });
        }
    } else {
        with(|s| s.crumbs.clear());
    }
    if let Some(m) = get("Block ESP") {
        scan_blocks(game, m, ticks);
    } else {
        with(|s| s.blocks = Sweep::new());
    }
    super::sunset_combat_macros::tick(game, &mods, game.ticks);
    super::sunset_melee::tick(game);
    super::sunset_click_fix::tick(game);
    super::sunset_click_fix::select_tick(game);
    super::sunset_anti_trap::tick(game);
    super::sunset_combat_inventory::tick(game, &mods);
    super::sunset_lag::tick(game, &mods);
    super::sunset_backtrack::tick(game, &mods);
    super::sunset_knockback_delay::tick(game, &mods);
    super::sunset_pvp_bot::tick(game, &mods);
    super::sunset_duel::tick(game);
    super::sunset_loadouts::tick(game);
    super::sunset_parity::tick(game, &mods);
    super::sunset_murder::tick(game, mods.iter().find(|m| m.name == "ESP" && m.on));
    super::sunset_hands::tick(game);
    super::sunset_combat::tick(game, &mods);
    super::sunset_cart::post_tick(game);
    super::sunset_anchor_catch::post_tick();
    super::sunset_explosive_aura::tick(game, &mods, game.ticks);
    super::sunset_render::tick(game, &mods, ticks);
    super::sunset_effects::tick(game);
    super::sunset_player::tick(game, &mods);
    super::sunset_hud_bars::tick(game, &mods, game.native_on(ID));
    super::sunset_world::tick(game, &mods);
    super::sunset_hunting::tick(game, &mods);
    super::sunset_light_scan::tick(game, &mods);
    super::sunset_sound_detector::tick(game, mods.iter().find(|m| m.name == super::sunset_sound_detector::NAME && m.on));
    super::sunset_gear::tick(game, &mods);
    super::sunset_new_chunks::tick(game, &mods);
    super::sunset_ore_sim::tick(game, &mods);
    super::sunset_world_parity::tick(game, &mods);
    super::sunset_packet_mine::tick(game);
    super::sunset_extras::tick(game, &mods);
    super::sunset_vanish::tick(game, mods.iter().find(|m| m.name == "Anti Vanish" && m.on));
    super::sunset_server_scan::tick(game, mods.iter().find(|m| m.name == "Server Scan" && m.on));
    super::sunset_client_spoofer::tick(game, mods.iter().find(|m| m.name == super::sunset_client_spoofer::NAME && m.on));
    crit_tick(game);
    if let Some(m) = get("Kill Aura") {
        kill_aura(game, m, get("Criticals").is_some());
    }
    if let Some(m) = get("Trigger Bot") {
        trigger_bot(game, m, get("Criticals").is_some_and(|c| !c.b("Only Kill Aura")));
    }
}
