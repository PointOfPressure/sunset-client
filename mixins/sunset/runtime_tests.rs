// Runtime part (see runtime.rs): tests of the runtime parts.

#[cfg(test)]
mod criticals_tests {
    use super::super::sunset::*;

    // The server's fall count over the hop's moves, after `moves` of them (the drop of each move told off the ground).
    fn counted(hop: f64, drop: bool, moves: u8) -> f64 {
        let mut last = 0.0;
        let mut fall = 0.0;
        for stage in 1..=moves {
            let at = crit_lift(stage, hop, drop);
            fall += (last - at).max(0.0);
            last = at;
        }
        fall
    }

    #[test]
    fn smash_hop_is_counted_a_tick_before_its_hit() {
        // Moves 1-3 go before the hit; Horizon may count the third together with the hit, so two must do.
        assert!(counted(1.501, true, 2) > 1.5);
        assert!(counted(1.501, true, 3) > 1.5);
        // The crit hop drops on both moves after its top, and ends where it started.
        assert!(counted(0.0625, false, 2) > 0.0);
        assert!(crit_lift(3, 0.0625, false) < crit_lift(2, 0.0625, false));
        assert_eq!(crit_lift(3, 0.0625, false), 0.0);
        assert_eq!(crit_lift(CRIT_LAND, 1.501, true), 0.0);
    }
}

#[cfg(test)]
mod baritone_runtime_tests {
    use super::super::{sunset::*, sunset_baritone as b, sunset_baritone_actions as actions};
    use serde_json::json;

    #[test]
    fn baritone_commands_validate_before_start_and_bound_diagnostics() {
        assert_eq!(b::parse(&[]).unwrap(), b::Command::Status);
        assert_eq!(b::parse(&["stop"]).unwrap(), b::Command::Stop);
        assert_eq!(b::parse(&["off"]).unwrap(), b::Command::Off);
        assert_eq!(b::parse(&["mine"]).unwrap(), b::Command::Mine);
        assert_eq!(b::parse(&["targets"]).unwrap(), b::Command::Targets(8));
        assert_eq!(b::parse(&["targets", "999"]).unwrap(), b::Command::Targets(64));
        assert_eq!(b::parse(&["goto", "-10", "-64", "20"]).unwrap(), b::Command::Goto([-10, -64, 20]));
        assert_eq!(b::parse(&["mine", "oresim", "diamond,iron"]).unwrap(), b::Command::Predicted(Some("diamond,iron".into())));
        for args in [vec!["goto", "-2147483648", "0", "0"], vec!["goto", "NaN", "0", "0"],
            vec!["mine", "oresim", "typo"], vec!["stop", "extra"], vec!["targets", "0"]] {
            assert!(b::parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn baritone_old_profiles_load_off_defaults_and_new_preferences_round_trip() {
        let mut old = State::new();
        old.load_text("sprint.on=true\noresim.seed=8675309\noresim.hideshownores=false\n");
        let module = old.module("Baritone").unwrap();
        assert!(!module.on && module.b("Allow break") && !module.b("Allow place"));
        assert_eq!(module.t("Ores"), "diamond");
        assert_eq!((module.n("Min Y"), module.n("Max Y"), module.n("Radius"), module.n("Max targets")), (-64.0, 319.0, 128.0, 64.0));
        old.load_profile("baritone.on=true\nbaritone.ores=iron,diamond\nbaritone.radius=48\nbaritone.path=false\n");
        let profile = old.profile_text();
        for forbidden in ["job_generation", "lease_generation", "targetqueue=", "activepath", "replay="] {
            assert!(!profile.lines().any(|line| line.starts_with(&format!("baritone.{forbidden}"))));
        }
        let mut loaded = State::new();
        loaded.load_profile(&profile);
        assert_eq!(old.module("Baritone"), loaded.module("Baritone"));
        let mut legacy_profile = State::new();
        legacy_profile.load_profile("sprint.on=true\noresim.seed=0\n");
        assert!(!legacy_profile.is_on("Baritone"));
        panic_off_state(&mut loaded);
        assert!(!loaded.is_on("Baritone"));
        old.load_profile("sprint.on=false\noresim.seed=0\n");
        assert_eq!(old.module("Baritone"), Some(&b::defaults()), "legacy profiles load Baritone off/default even after a new profile");
    }

    #[test]
    fn baritone_stale_engine_frames_and_invalid_actions_fail_closed() {
        let reply = json!({"schema":1,"world":"4","dimension":"minecraft:overworld",
            "job_generation":"5","lease_generation":"6","tick":"7"});
        assert!(b::response_matches(&reply, 4, "minecraft:overworld", 5, 6, 7));
        assert!(!b::response_matches(&reply, 4, "minecraft:overworld", 8, 6, 7));
        assert!(!b::response_matches(&reply, 4, "minecraft:the_nether", 5, 6, 7));
        assert!(!b::response_matches(&reply, 4, "minecraft:overworld", 5, 9, 7));
        assert!(!b::response_matches(&reply, 4, "minecraft:overworld", 5, 6, 8));
        let bad = actions::Intent { yaw: Some(f32::NAN), ..Default::default() };
        assert!(actions::validate(&bad).is_err());
        assert!(actions::movement(128).is_err());
        assert!(actions::movement(15).unwrap().is_empty());
        let bad = actions::Intent { selected_slot: Some(9), ..Default::default() };
        assert!(actions::validate(&bad).is_err());
    }

    #[test]
    fn baritone_render_edits_preserve_jobs_and_execution_edits_cancel() {
        let before = b::defaults();
        let mut state = State::new();
        state.load_profile("baritone.path=false\nbaritone.goal=false\nbaritone.targets=false\nbaritone.fillopacity=0.1\n");
        assert!(!b::execution_changed(&before, state.module("Baritone").unwrap()));
        state.load_profile("baritone.allowbreak=false\n");
        assert!(b::execution_changed(&before, state.module("Baritone").unwrap()));
    }
}

#[cfg(test)]
mod ore_sim_config_tests {
    use super::super::sunset::*;
    use super::super::sunset_ore_sim::manual_seed;

    #[test]
    fn old_ore_sim_config_and_profiles_keep_all_manual_fields() {
        let old = "oresim.on=true\noresim.key=key.keyboard.o\noresim.seed=-8675309\noresim.chunks=2\noresim.ores=diamond,debris\noresim.hideshownores=true\noresim.max=512\n";
        let mut state = State::new();
        state.load_text(old);
        let module = state.module("Ore Sim").unwrap();
        assert!(module.on && module.b("Hide shown ores"));
        assert_eq!(module.key.as_deref(), Some("key.keyboard.o"));
        assert_eq!(manual_seed(module), Some(-8675309));
        assert_eq!((module.n("Chunks"), module.n("Max")), (2.0, 512.0));
        assert_eq!(module.t("Ores"), "diamond,debris");
        assert_eq!(module.p("Seed source"), "Manual");
        assert!(!module.b("Database fetch"));
        let profile = format!("{}\n{old}", super::super::sunset_profiles::HEADER);
        let checked = state.check_profile(&profile).unwrap();
        assert!(checked.skipped.is_empty(), "{:?}", checked.skipped);
        let mut from_profile = State::new();
        from_profile.load_profile(&checked.body);
        let mut loaded = State::new();
        loaded.load_text(&state.save_text());
        assert_eq!(from_profile.module("Ore Sim"), Some(module));
        assert_eq!(loaded.module("Ore Sim"), Some(module));
        from_profile.load_profile("oresim.on=false\n");
        assert!(!from_profile.is_on("Ore Sim"));
        assert_eq!(manual_seed(from_profile.module("Ore Sim").unwrap()), Some(-8675309));
    }

    #[test]
    fn automatic_settings_round_trip_and_old_profiles_overlay_them() {
        let mut state = State::new();
        let defaults = state.module("Ore Sim").unwrap();
        let shown = |module: &Module, name: &str| module.setting_visible(module.settings.iter().position(|s| s.name == name).unwrap());
        assert!(!defaults.on && !defaults.b("Database fetch"));
        assert!(shown(defaults, "Seed") && shown(defaults, "Seed source"));
        assert!(!shown(defaults, "Database fetch") && !shown(defaults, "Cracker threads"));
        state.load_text("oresim.seedsource=Auto-crack\noresim.databasefetch=true\noresim.crackerthreads=3\n");
        let module = state.module("Ore Sim").unwrap();
        assert!(shown(module, "Seed") && shown(module, "Database fetch") && shown(module, "Cracker threads"));
        assert_eq!(manual_seed(module), None);
        let mut loaded = State::new();
        loaded.load_text(&state.save_text());
        let mut from_profile = State::new();
        from_profile.load_profile(&state.profile_text());
        assert_eq!(loaded.module("Ore Sim"), Some(module));
        assert_eq!(from_profile.module("Ore Sim"), Some(module));
        // An old profile has no new fields: its nonempty Seed still overrides an explicitly selected Auto mode.
        from_profile.load_profile("oresim.seed=0\noresim.chunks=4\noresim.on=true\n");
        let overlay = from_profile.module("Ore Sim").unwrap();
        assert_eq!(overlay.p("Seed source"), "Auto-crack");
        assert!(overlay.b("Database fetch"));
        assert_eq!(overlay.n("Cracker threads"), 3.0);
        assert_eq!(manual_seed(overlay), Some(0));
        assert_eq!(overlay.info().as_deref(), Some("Manual override"));
        assert!(from_profile.profile_text().contains("oresim.seed=0\n"));
    }

    #[test]
    fn seed_command_preserves_enable_state_and_manual_java_seed_parsing() {
        let mut state = State::new();
        for (text, value) in [("0", 0), ("-9223372036854775808", i64::MIN), ("A😀", 1_835_364)] {
            seed_settings(&mut state, "auto");
            seed_settings(&mut state, text);
            let module = state.module("Ore Sim").unwrap();
            assert!(!module.on, "setting a seed does not enable Ore Sim");
            assert_eq!(module.p("Seed source"), "Manual");
            assert_eq!(module.t("Seed"), text);
            assert_eq!(manual_seed(module), Some(value));
        }
        State::set_field(&mut state.modules, "oresim.on", "true");
        seed_settings(&mut state, "auto");
        let module = state.module("Ore Sim").unwrap();
        assert!(module.on);
        assert_eq!(module.p("Seed source"), "Auto-crack");
        assert_eq!(manual_seed(module), None);
        State::set_field(&mut state.modules, "oresim.seed", "0");
        assert_eq!(state.module("Ore Sim").unwrap().info().as_deref(), Some("Manual override"));
        seed_settings(&mut state, "clear");
        assert_eq!(state.module("Ore Sim").unwrap().p("Seed source"), "Auto-crack");
        seed_settings(&mut state, "123");
        seed_settings(&mut state, "clear");
        let module = state.module("Ore Sim").unwrap();
        assert_eq!(module.p("Seed source"), "Manual");
        assert_eq!(manual_seed(module), None);
        assert!(module.on, "clearing a seed does not toggle Ore Sim");
    }
}

#[cfg(test)]
mod movement_config_tests {
    use super::super::sunset::*;
    use winit::keyboard::KeyCode;
    use winit::event::MouseButton;

    #[test]
    fn legacy_autoblock_preserves_old_shield_profiles_and_explicit_new_choices() {
        let mut state = State::new();
        state.load_text("killaura.shield=true\n");
        assert_eq!(state.module("Kill Aura").unwrap().p("AutoBlock"), "Modern");
        state.load_text("killaura.shield=true\nkillaura.autoblock=Legacy\n");
        let aura = state.module("Kill Aura").unwrap();
        assert_eq!(aura.p("AutoBlock"), "Legacy");
        assert!(aura.setting_visible(aura.settings.iter().position(|s| s.name == "Legacy reblock delay").unwrap()));
        assert!(!aura.setting_visible(aura.settings.iter().position(|s| s.name == "Shield").unwrap()));
        assert!(!aura.setting_visible(aura.settings.iter().position(|s| s.name == "Shield range").unwrap()));
        state.load_text("killaura.shield=true\nkillaura.autoblock=Off\n");
        assert_eq!(state.module("Kill Aura").unwrap().p("AutoBlock"), "Off");
        let saved = state.module_text();
        let mut loaded = State::new();
        loaded.load_text(&saved);
        assert_eq!(loaded.module("Kill Aura").unwrap().p("AutoBlock"), "Off");
    }

    #[test]
    fn auto_weapon_modes_show_only_their_weapons_settings() {
        let mut state = State::new();
        let weapon = state.modules.iter_mut().find(|m| m.name == "Auto Weapon").unwrap();
        let shown = |m: &Module, name: &str| m.setting_visible(m.settings.iter().position(|s| s.name == name).unwrap());
        // Custom first: a profile from before Mode keeps its sections.
        assert_eq!(weapon.p("Mode"), "Custom");
        for name in ["Best weapon", "Axe on shields", "Prefer axes"] {
            assert!(shown(weapon, name), "Custom shows {name}");
        }
        for (mode, on, off) in [
            ("Sword", &["Totem hit", "Holding", "Swap back"][..], &["Best weapon", "Prefer axes", "Shield mode"][..]),
            ("Axe", &["Totem hit", "Shield mode"][..], &["Prefer axes", "Axe on shields"][..]),
        ] {
            weapon.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse(mode);
            for name in on {
                assert!(shown(weapon, name), "{mode} uses {name}");
            }
            for name in off {
                assert!(!shown(weapon, name), "{mode} does not use {name}");
            }
        }
        // Mode decides, whatever the Custom switches say.
        weapon.settings.iter_mut().find(|s| s.name == "Axe on shields").unwrap().parse("false");
        assert!(super::super::sunset_melee::weapon_on(weapon, "Axe on shields"));
        weapon.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse("Sword");
        assert!(!super::super::sunset_melee::weapon_on(weapon, "Axe on shields"));
        weapon.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse("Custom");
        assert!(!super::super::sunset_melee::weapon_on(weapon, "Axe on shields"));
        // The spear's settings are Auto Spear's now, Extended reach's shown with it.
        let spear = state.modules.iter_mut().find(|m| m.name == "Auto Spear").unwrap();
        assert!(shown(spear, "Lunge key") && !shown(spear, "Reach chance (%)"));
        spear.settings.iter_mut().find(|s| s.name == "Extended reach").unwrap().parse("true");
        assert!(shown(spear, "Reach chance (%)") && shown(spear, "Reach weapons") && shown(spear, "Works with Trigger Bot"));
    }

    #[test]
    fn hud_places_and_inventory_blur_show_with_what_they_move() {
        let mut state = State::new();
        let shown = |m: &Module, name: &str| m.setting_visible(m.settings.iter().position(|s| s.name == name).unwrap());
        let set = |m: &mut Module, name: &str, value: &str| m.settings.iter_mut().find(|s| s.name == name).unwrap().parse(value);
        let hud = state.modules.iter_mut().find(|m| m.name == "HUD").unwrap();
        assert!(shown(hud, "Coordinates position") && !shown(hud, "Coordinates x") && !shown(hud, "Compass position"));
        set(hud, "Coordinates position", "Custom");
        set(hud, "Compass", "true");
        assert!(shown(hud, "Coordinates x") && shown(hud, "Coordinates y") && shown(hud, "Compass position") && !shown(hud, "Compass x"));
        let gui = state.modules.iter_mut().find(|m| m.name == "ClickGUI").unwrap();
        assert!(shown(gui, "Inventory blur"), "Menu dim is on by default");
        set(gui, "Menu dim", "false");
        assert!(!shown(gui, "Inventory blur"));
        set(gui, "Menu dim", "true");
        assert!(shown(gui, "Inventory blur"));
        let esp = state.modules.iter_mut().find(|m| m.name == "ESP").unwrap();
        assert!(!shown(esp, "Murderer items") && !shown(esp, "Callout messages"));
        set(esp, "Murderer finder", "true");
        assert!(shown(esp, "Murderer items") && shown(esp, "Callout") && !shown(esp, "Callout messages"));
        set(esp, "Callout", "true");
        assert!(shown(esp, "Callout messages") && shown(esp, "Callout delay (ms)"));
    }

    #[test]
    fn scaffold_modes_show_only_their_own_controls() {
        let mut state = State::new();
        let scaffold = state.modules.iter_mut().find(|m| m.name == "Scaffold").unwrap();
        scaffold.settings.iter_mut().find(|s| s.name == "Advanced").unwrap().parse("true");
        let shown = |m: &Module, name: &str| m.setting_visible(m.settings.iter().position(|s| s.name == name).unwrap());
        for (mode, on, off) in [
            ("Legit", &["Shortest sneak delay (ms)", "Longest sneak delay (ms)", "Blocks", "Block counter"][..],
             &["Activation blocks", "Auto switch", "Rotate", "Delay (ticks)", "Air place", "Safe walk", "Render", "Fast tower"][..]),
            ("Godbridge", &["Activation blocks", "Auto switch", "Blocks"][..],
             &["Shortest sneak delay (ms)", "Rotate", "Rotation speed", "Delay (ticks)", "Ray cast", "Same Y", "Swap back",
               "Ledge sneak", "Late click", "Render"][..]),
            ("Telly", &["Telly rotations", "Start speed min", "Normal speed max", "Keep Y", "Keep Y on press", "Tower",
                        "Telly sprint", "Ground motion (%)", "Air motion (%)", "Speed motion (%)", "Multi place", "Swing",
                        "Reject wait (ticks)", "Delay (ticks)", "Auto switch", "Swap back", "Block counter"][..],
             &["Rotations", "Telly ticks", "Same Y", "Ray cast", "Fast tower", "Safe walk", "Ledge sneak"][..]),
            ("Normal", &["Rotate", "Rotations", "Look", "Rotation speed", "Min rotation speed", "Ray cast", "Movement correction",
                         "Sprint", "Same Y", "Expand", "Ledge sneak", "Delay (ticks)", "Safe walk",
                         "Max per second", "Render", "Auto switch", "Swap back"][..],
             &["Shortest sneak delay (ms)", "Activation blocks", "Telly ticks", "Tower speed", "Air place"][..]),
        ] {
            scaffold.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse(mode);
            for name in on {
                assert!(shown(scaffold, name), "{mode} uses {name}");
            }
            for name in off {
                assert!(!shown(scaffold, name), "{mode} does not use {name}");
            }
        }
        // Normal's controls that hang on others.
        let set = |m: &mut Module, name: &str, value: &str| m.settings.iter_mut().find(|s| s.name == name).unwrap().parse(value);
        assert!(!shown(scaffold, "Late click"), "Normal's Ray cast (the default) needs no Late click");
        set(scaffold, "Ray cast", "Strict");
        assert!(shown(scaffold, "Late click"), "Late click backs up Strict's look");
        set(scaffold, "Rotations", "Telly");
        set(scaffold, "Ray cast", "Off");
        set(scaffold, "Look", "Visible");
        set(scaffold, "Fast tower", "true");
        for name in ["Telly ticks", "Air place", "Tower speed", "While moving"] {
            assert!(shown(scaffold, name), "Normal uses {name} here");
        }
        assert!(!shown(scaffold, "Late click"), "Late click backs up Strict's look only");
        assert!(!shown(scaffold, "Movement correction"), "a visible look needs no correction");
        set(scaffold, "Rotate", "false");
        for name in ["Rotations", "Ray cast", "Rotation speed", "Movement correction", "Late click"] {
            assert!(!shown(scaffold, name), "no look, no {name}");
        }
        assert!(shown(scaffold, "Air place"));
        scaffold.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse("Godbridge");
        let mut restored = State::new();
        restored.load_text(&state.save_text());
        assert_eq!(restored.module("Scaffold").unwrap().p("Mode"), "Godbridge");
        assert_eq!(restored.module("Scaffold").unwrap().n("Longest sneak delay (ms)"), 200.0);
    }

    #[test]
    fn scaffold_settings_from_before_normal_s_rework_load() {
        // Settings Normal no longer has are passed over; the ones it kept load as they were.
        let mut state = State::new();
        state.load_text(concat!(
            "scaffold.mode=Normal\n",
            "scaffold.aheaddistance=0.5\n",
            "scaffold.radius=2\n",
            "scaffold.blockspertick=4\n",
            "scaffold.validateray=true\n",
            "scaffold.duplicatecooldown=3\n",
            "scaffold.rotationspeed=120\n",
            "scaffold.delayticks=1\n",
            "scaffold.airplace=true\n",
            "scaffold.closestblockrange=8\n",
            "scaffold.maxplacementdistance=5\n",
        ));
        let m = state.module("Scaffold").unwrap();
        assert_eq!(m.p("Mode"), "Normal");
        assert_eq!(m.n("Rotation speed"), 120.0);
        assert_eq!(m.n("Delay (ticks)"), 1.0);
        assert!(m.b("Air place"));
        assert_eq!(m.n("Closest block range"), 8.0);
        assert_eq!(m.n("Max placement distance"), 5.0);
        // The movement presets still name settings Scaffold has.
        for style in ["subtle", "balanced", "aggressive"] {
            let mut s = State::new();
            assert!(apply_movement_preset(&mut s, style));
        }
        let scaffold = State::new();
        let names: Vec<&str> = scaffold.module("Scaffold").unwrap().settings.iter().map(|s| s.name).collect();
        for name in ["Delay (ticks)", "Rotate", "Rotation speed"] {
            assert!(names.contains(&name), "{name}");
        }
    }

    #[test]
    fn offhand_totem_settings_from_before_auto_totem_load() {
        let mut state = State::new();
        state.load_text("offhand.item=None
offhand.health=14
offhand.refill=Hotbar Swap
offhand.swapdelay=3
offhand.doublehand=true
");
        let m = state.module("Offhand").unwrap();
        assert_eq!(m.p("Item"), "Totem");
        assert!(m.b("Silent open") && m.b("Open inventory"));
        assert_eq!((m.n("Shortest totem delay (ms)"), m.n("Longest totem delay (ms)")), (150.0, 150.0));
        let mut state = State::new();
        state.load_text("offhand.item=Crystal
offhand.health=10
offhand.refill=Inventory
");
        let m = state.module("Offhand").unwrap();
        assert_eq!(m.p("Item"), "Crystal");
        assert!(!m.b("Silent open"));
        // A new config's Item None stays None.
        let mut state = State::new();
        state.load_text("offhand.item=None
");
        assert_eq!(state.module("Offhand").unwrap().p("Item"), "None");
    }

    #[test]
    fn offhand_shows_auto_totem_settings_with_item_totem() {
        let mut state = State::new();
        let m = state.modules.iter_mut().find(|m| m.name == "Offhand").unwrap();
        let shown = |m: &Module, name: &str| m.setting_visible(m.settings.iter().position(|s| s.name == name).unwrap());
        assert!(shown(m, "Random slot") && shown(m, "Close inventory") && !shown(m, "Shortest silent move delay (ms)"));
        m.settings.iter_mut().find(|s| s.name == "Silent open").unwrap().parse("true");
        assert!(shown(m, "Longest silent move delay (ms)") && !shown(m, "Close inventory"));
        m.settings.iter_mut().find(|s| s.name == "Item").unwrap().parse("Crystal");
        assert!(!shown(m, "Random slot") && !shown(m, "Silent open") && shown(m, "Delay (ticks)"));
        assert!(!shown(m, "Hotbar slot"));
        // Show totem count: the count beside Offhand in the module list.
        assert_eq!(m.info(), None);
        m.settings.iter_mut().find(|s| s.name == "Show totem count").unwrap().parse("true");
        assert!(m.info().is_some_and(|count| count.parse::<u32>().is_ok()));
    }

    #[test]
    fn existing_movement_settings_still_load() {
        let mut state = State::new();
        state.load_text("speed.mode=Strafe\nspeed.vanillaspeed=7.2\nvelocity.knockbackhorizontal=0.4\nscaffold.delayticks=2\n");
        assert_eq!(state.module("Speed").unwrap().p("Mode"), "Strafe");
        assert_eq!(state.module("Speed").unwrap().n("Vanilla speed"), 7.2);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback horizontal"), 0.4);
        assert_eq!(state.module("Scaffold").unwrap().n("Delay (ticks)"), 2.0);
    }

    #[test]
    fn local_pre_rename_movement_values_migrate() {
        let mut state = State::new();
        state.load_text("speed.mode=Ground\nspeed.speed=8\nvelocity.horizontal=70\nvelocity.vertical=85\nvelocity.airhorizontal=50\nscaffold.rotation=None\nscaffold.maxdistance=4\n");
        assert_eq!(state.module("Speed").unwrap().p("Mode"), "Vanilla");
        assert_eq!(state.module("Speed").unwrap().n("Vanilla speed"), 8.0);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback horizontal"), 0.7);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback vertical"), 0.85);
        assert_eq!(state.module("Velocity").unwrap().n("Air knockback horizontal"), 0.5);
        assert!(!state.module("Scaffold").unwrap().b("Rotate"));
        assert_eq!(state.module("Scaffold").unwrap().n("Max placement distance"), 4.0);
    }

    #[test]
    fn presets_round_trip_through_config_and_profile() {
        let mut state = State::new();
        assert!(apply_movement_preset(&mut state, "aggressive"));
        let config = state.save_text();
        let profile = state.profile_text();
        let mut restored = State::new();
        restored.load_text(&config);
        assert_eq!(restored.module("Speed").unwrap().p("Mode"), "Custom");
        assert_eq!(restored.module("Speed").unwrap().n("Vanilla speed"), 10.0);
        assert_eq!(restored.module("Velocity").unwrap().n("Knockback horizontal"), 0.0);
        let mut from_profile = State::new();
        from_profile.load_profile(&profile);
        assert_eq!(from_profile.module("No Slow").unwrap().n("Ground %"), 100.0);
        assert!(!apply_movement_preset(&mut state, "unknown"));
    }

    #[test]
    fn air_knockback_and_blend_have_distinct_factors() {
        let mut state = State::new();
        let velocity = state.modules.iter_mut().find(|m| m.name == "Velocity").unwrap();
        for (name, value) in [("Mode", "Blend"), ("Use air values", "true"),
            ("Knockback horizontal", "0"), ("Air knockback horizontal", "0.5"), ("Blend %", "50")] {
            velocity.settings.iter_mut().find(|s| s.name == name).unwrap().parse(value);
        }
        assert_eq!(knockback_shares(velocity, true)[0], 0.5);
        assert_eq!(knockback_shares(velocity, false)[0], 0.75);
    }

    #[test]
    fn advanced_click_rows_follow_mode() {
        let mut state = State::new();
        let speed = state.modules.iter_mut().find(|m| m.name == "Speed").unwrap();
        let index = speed.settings.iter().position(|s| s.name == "Acceleration").unwrap();
        assert!(!speed.setting_visible(index));
        speed.settings.iter_mut().find(|s| s.name == "Advanced").unwrap().parse("true");
        speed.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse("Smooth");
        assert!(speed.setting_visible(index));
    }

    #[test]
    fn fast_place_speed_survives_config_round_trip() {
        let mut state = State::new();
        state.load_text("fastplace.speed=3\n");
        assert_eq!(state.module("Fast Place").unwrap().n("Speed"), 3.0);

        let mut restored = State::new();
        restored.load_text(&state.save_text());
        assert_eq!(restored.module("Fast Place").unwrap().n("Speed"), 3.0);
    }

    #[test]
    fn scaffold_rotation_restore_consumes_the_saved_view_once() {
        let mut saved = Some((135.0, -20.0));
        assert_eq!(take_scaffold_rotation(&mut saved), Some((135.0, -20.0)));
        assert_eq!(take_scaffold_rotation(&mut saved), None);
    }

    #[test]
    fn panic_cleanup_disables_modules_and_returns_scaffold_rotation() {
        let mut state = State::new();
        for m in &mut state.modules { m.on = true; }
        state.scaffold_rotation = Some((135.0, -20.0));
        state.tool_wait = Some((2, 3));
        state.mc_use = Some((1, "ender_pearl"));
        state.mc_back = Some((0, 4));
        state.eat_paused_aura = true;
        assert_eq!(panic_off_state(&mut state), Some((135.0, -20.0)));
        assert!(state.modules.iter().all(|m| !m.on));
        assert!(state.tool_wait.is_none() && state.mc_use.is_none() && state.mc_back.is_none());
        assert!(!state.eat_paused_aura);
        assert_eq!(panic_off_state(&mut state), None);
        assert!(state.modules.iter().all(|m| !m.on));
    }

    #[test]
    fn panic_input_uses_actions_instead_of_raw_device_buttons() {
        let mut bindings = horizon_client::keybinds::KeyBinds::default();
        // J has no default action (P is also Social Interactions', which stays held as P).
        bindings.set("key.forward", "key.keyboard.j");
        bindings.set("key.attack", "key.keyboard.q");
        bindings.set("key.use", "key.keyboard.z");
        bindings.set("key.jump", "key.mouse.right");
        let keys = [KeyCode::KeyJ, KeyCode::KeyQ].into_iter().collect();
        let buttons = [MouseButton::Right].into_iter().collect();
        let (held, mouse) = mapped_input(&bindings, &keys, &buttons, [None; 4]);
        assert!(held.contains(&KeyCode::KeyW) && held.contains(&KeyCode::Space));
        assert!(!held.contains(&KeyCode::KeyJ));
        assert!(mouse.contains(&MouseButton::Left));
        assert!(!mouse.contains(&MouseButton::Right));
        let (held, mouse) = mapped_input(&bindings, &keys, &buttons, [None, None, Some(false), Some(true)]);
        assert!(held.contains(&KeyCode::KeyW));
        assert!(!mouse.contains(&MouseButton::Left));
        assert!(mouse.contains(&MouseButton::Right));
    }
}

#[cfg(test)]
mod velocity_merge_tests {
    use super::super::sunset::merged_modules;
    use std::collections::BTreeMap;

    fn fields(text: &str) -> BTreeMap<String, String> {
        merged_modules(text).lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect()
    }

    #[test]
    fn delay_alone_turns_velocity_on_without_its_pushes() {
        let out = fields("velocity.on=false\nvelocity.key=none\nknockbackdelay.on=true\nknockbackdelay.key=key.keyboard.k\nknockbackdelay.chance=80\nknockbackdelay.onlynearenemies=false\n");
        for (field, value) in [("on", "true"), ("knockbackdelay", "true"), ("key", "key.keyboard.k"), ("delaychance", "80"),
            ("delayonlynearenemies", "false"), ("knockback", "false"), ("explosions", "false")] {
            assert_eq!(out[&format!("velocity.{field}")], value, "{field}");
        }
        assert!(!out.keys().any(|k| k.starts_with("knockbackdelay.")));
    }

    // Ghost Hand's Hits or Cut through comes back as Hit Through Walls, on only if Ghost Hand was; Blocks stays.
    #[test]
    fn ghost_hand_hits_move_to_hit_through_walls() {
        let out = fields("ghosthand.on=true\nghosthand.blocks=false\nghosthand.hits=true\nghosthand.cutthrough=true\n");
        assert_eq!(out["hitthroughwalls.on"], "true");
        assert_eq!(out["hitthroughwalls.mode"], "Any block");
        assert_eq!(out["hitthroughwalls.ignorepassive"], "false");
        assert_eq!(out["ghosthand.blocks"], "false");
        assert!(!out.contains_key("ghosthand.hits") && !out.contains_key("ghosthand.cutthrough"));
        let out = fields("ghosthand.on=false\nghosthand.hits=false\nghosthand.cutthrough=true\n");
        assert_eq!(out["hitthroughwalls.on"], "false");
        assert_eq!(out["hitthroughwalls.mode"], "Passable only");
        let out = fields("ghosthand.on=true\nghosthand.hits=false\nghosthand.cutthrough=false\n");
        assert!(!out.contains_key("hitthroughwalls.on") && !out.contains_key("ghosthand.hits"));
        // A config that already has the module keeps its own lines.
        let out = fields("ghosthand.on=true\nghosthand.hits=true\nhitthroughwalls.on=false\nhitthroughwalls.mode=Passable only\n");
        assert_eq!(out["hitthroughwalls.on"], "false");
        assert_eq!(out["hitthroughwalls.mode"], "Passable only");
    }

    #[test]
    fn velocity_already_on_keeps_its_settings_and_key() {
        let out = fields("velocity.on=true\nvelocity.key=key.keyboard.v\nvelocity.knockback=true\nknockbackdelay.on=true\nknockbackdelay.key=key.keyboard.k\n");
        assert_eq!(out["velocity.knockbackdelay"], "true");
        assert_eq!(out["velocity.knockback"], "true");
        assert_eq!(out["velocity.key"], "key.keyboard.v");
        let current = "velocity.on=true\nvelocity.knockbackdelay=false\n";
        assert!(matches!(merged_modules(current), std::borrow::Cow::Borrowed(_)));
    }
}

#[cfg(test)]
mod section_merge_tests {
    use super::super::sunset::merged_modules;
    use std::collections::BTreeMap;

    fn fields(text: &str) -> BTreeMap<String, String> {
        merged_modules(text).lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect()
    }

    #[test]
    fn crystal_macro_alone_is_crystal_auras_macro_with_its_settings() {
        let out = fields("crystalaura.on=false\ncrystalaura.key=none\ncrystalaura.placedelay=0\ncrystalaura.switchback=true\n\
            crystalmacro.on=true\ncrystalmacro.key=key.mouse.4\ncrystalmacro.placedelay=3\ncrystalmacro.switchback=false\n\
            crystalmacro.obsidiandelay=5\n");
        for (field, value) in [("on", "true"), ("key", "key.mouse.4"), ("aura", "false"), ("macro", "true"), ("placedelay", "3"),
            ("switchback", "false"), ("obsidiandelay", "5")] {
            assert_eq!(out[&format!("crystalaura.{field}")], value, "{field}");
        }
        assert!(!out.keys().any(|k| k.starts_with("crystalmacro.")));
    }

    #[test]
    fn crystal_aura_on_keeps_its_shared_settings_and_key() {
        let out = fields("crystalaura.on=true\ncrystalaura.key=key.keyboard.c\ncrystalaura.placedelay=1\n\
            crystalmacro.on=true\ncrystalmacro.key=key.mouse.4\ncrystalmacro.placedelay=3\n");
        for (field, value) in [("on", "true"), ("key", "key.keyboard.c"), ("aura", "true"), ("macro", "true"), ("placedelay", "1")] {
            assert_eq!(out[&format!("crystalaura.{field}")], value, "{field}");
        }
    }

    #[test]
    fn off_modules_keep_default_sections_unless_only_a_macro_was_bound() {
        let both_off = fields("anchoraura.on=false\nanchoraura.key=none\nanchormacro.on=false\nanchormacro.key=none\nanchormacro.mode=Double\n");
        assert_eq!(both_off["anchoraura.on"], "false");
        assert_eq!(both_off["anchoraura.mode"], "Double");
        assert!(!both_off.contains_key("anchoraura.aura") && !both_off.contains_key("anchoraura.macro"));
        let bound = fields("anchoraura.on=false\nanchoraura.key=none\nanchormacro.on=false\nanchormacro.key=key.keyboard.r\n");
        for (field, value) in [("key", "key.keyboard.r"), ("aura", "false"), ("macro", "true")] {
            assert_eq!(bound[&format!("anchoraura.{field}")], value, "{field}");
        }
    }

    #[test]
    fn anvil_modules_become_its_sections() {
        let out = fields("antianvil.on=true\nantianvil.key=none\nantianvil.rotate=false\nselfanvil.on=false\nselfanvil.key=key.keyboard.v\n\
            autoanvil.on=true\nautoanvil.key=none\nautoanvil.height=4\nautoanvil.rotate=true\n");
        for (field, value) in [("on", "true"), ("key", "key.keyboard.v"), ("antianvil", "true"), ("selfanvil", "false"),
            ("autoanvil", "true"), ("rotate", "false"), ("height", "4")] {
            assert_eq!(out[&format!("anvil.{field}")], value, "{field}");
        }
        assert!(!out.keys().any(|k| ["antianvil.", "selfanvil.", "autoanvil."].iter().any(|p| k.starts_with(p))));
    }

    #[test]
    fn anchor_lines_go_and_anchor_aura_stays() {
        let out = fields("anchor.on=true\nanchor.key=none\nanchor.maxheight=10\nanchoraura.on=true\nanchoraura.mindamage=6\n");
        assert!(!out.keys().any(|k| k.starts_with("anchor.")));
        assert_eq!(out["anchoraura.on"], "true");
        assert_eq!(out["anchoraura.mindamage"], "6");
        assert!(!out.contains_key("anchoraura.macro"));
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::super::sunset::*;
    use std::sync::atomic::Ordering;
    use std::collections::BTreeMap;

    fn fields(text: &str) -> BTreeMap<String, String> {
        merged_modules(text).lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect()
    }

    #[test]
    fn renamed_modules_keep_their_settings() {
        let out = fields("autosteal.on=true\nautosteal.key=key.keyboard.j\nnominingtrace.on=false\nnominingtrace.onlywithapickaxe=false\n");
        assert_eq!(out["cheststealer.on"], "true");
        assert_eq!(out["cheststealer.key"], "key.keyboard.j");
        assert_eq!(out["minethroughmobs.onlywithapickaxe"], "false");
        assert!(!out.keys().any(|k| k.starts_with("autosteal.") || k.starts_with("nominingtrace.")));
        // Mine Through Mobs is hidden; it was off and unbound, so it stays out of the menu.
        assert!(!out.contains_key("modules.minethroughmobs"));
    }

    #[test]
    fn reverse_step_is_steps_section_and_potion_saver_is_gone() {
        let out = fields("step.on=false\nstep.key=none\nreversestep.on=true\nreversestep.key=key.keyboard.r\nreversestep.speed=2\npotionsaver.on=true\n");
        for (field, value) in [("on", "true"), ("key", "key.keyboard.r"), ("stepup", "false"), ("reversestep", "true"), ("fallspeed", "2")] {
            assert_eq!(out[&format!("step.{field}")], value, "{field}");
        }
        assert_eq!(out["modules.step"], "true");
        assert!(!out.keys().any(|k| k.starts_with("reversestep.") || k.starts_with("potionsaver.")));
    }

    #[test]
    fn extras_in_use_are_listed_and_current_configs_are_left_alone() {
        let out = fields("jesus.on=true\njesus.key=none\nslippy.on=false\nslippy.key=key.keyboard.p\nselfweb.on=false\nselfweb.key=none\n");
        assert_eq!(out["modules.jesus"], "true");
        assert_eq!(out["modules.slippy"], "true");
        assert!(!out.contains_key("modules.selfweb"));
        let current = "jesus.on=true\nmodules.jesus=false\n";
        assert!(matches!(merged_modules(current), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn extra_modules_lines_are_modules_and_pearl_catch_shows() {
        let out = fields("extramodules.jesus=true\nextramodules.step=false\nextramodules.pearlcatch=false\n");
        assert_eq!(out["modules.jesus"], "true");
        assert_eq!(out["modules.step"], "false");
        assert!(!out.keys().any(|k| k.starts_with("extramodules.")));
        assert!(!out.contains_key("modules.pearlcatch"));
        assert_eq!(fields("extramodules.pearlcatch=true\n")["modules.pearlcatch"], "true");
    }

    #[test]
    fn every_module_has_a_switch_and_modules_always_shows() {
        let mut state = State::new();
        let list = state.module("Modules").unwrap();
        for m in state.modules.iter().filter(|m| m.name != "Modules") {
            assert!(list.settings.iter().any(|s| s.name == m.name), "{} has a switch", m.name);
        }
        let shown = |state: &State, name: &str| state.listed()(&state.modules[state.find(name).unwrap()]);
        assert!(shown(&state, "Pearl Catch") && shown(&state, "Kill Aura") && !shown(&state, "Jesus"));
        State::set_field(&mut state.modules, "modules.killaura", "false");
        assert!(!shown(&state, "Kill Aura"));
        State::set_field(&mut state.modules, "modules.modules", "false");
        assert!(shown(&state, "Modules"));
    }

    #[test]
    fn rotation_and_no_rotate_are_rotations() {
        let out = fields("rotation.on=true\nrotation.key=key.keyboard.r\nrotation.yawlock=Smart\nrotation.yawangle=90\nnorotate.on=false\n");
        for (field, value) in [("on", "true"), ("key", "key.keyboard.r"), ("lock", "true"), ("norotate", "false"), ("yawlock", "Smart"), ("yawangle", "90")] {
            assert_eq!(out[&format!("rotations.{field}")], value, "{field}");
        }
        let out = fields("norotate.on=true\nrotation.on=false\n");
        assert_eq!((out["rotations.on"].as_str(), out["rotations.norotate"].as_str(), out["rotations.lock"].as_str()), ("true", "true", "false"));
        assert!(!out.keys().any(|k| k.starts_with("rotation.") || k.starts_with("norotate.")));
        let out = fields("bowspam.on=true\nantibed.on=true\n");
        assert!(!out.keys().any(|k| k.starts_with("bowspam.") || k.starts_with("antibed.")));
    }

    #[test]
    fn presence_is_menu_only_and_shares_by_default() {
        let mut state = State::new();
        let i = state.find("Sunset Presence").unwrap();
        let m = &state.modules[i];
        assert!(m.on && m.b("Anonymous analytics") && m.b("Share in-game presence") && !m.in_gui);
        assert!(state.listed()(m) && !state.in_gui()(m), "in Options > Mods and .pages, not the ClickGUI");
        assert!(state.modules.iter().filter(|m| m.name != "Sunset Presence").all(|m| m.in_gui));
        // A config saved before the default changed keeps its false; one without the key gets the new default.
        state.load_text("sunsetpresence.on=true\nsunsetpresence.shareingamepresence=false\n");
        assert!(!state.modules[i].b("Share in-game presence"));
        let mut fresh = State::new();
        fresh.load_text("sunsetpresence.on=true\nsunsetpresence.anonymousanalytics=false\n");
        let m = &fresh.modules[i];
        assert!(m.b("Share in-game presence") && !m.b("Anonymous analytics"));
        // Turned off in Modules: gone from the Options page too, and still never in the ClickGUI.
        State::set_field(&mut fresh.modules, "modules.sunsetpresence", "false");
        assert!(!fresh.listed()(&fresh.modules[i]) && !fresh.in_gui()(&fresh.modules[i]));
    }

    #[test]
    fn extras_are_modules_and_hidden_until_listed() {
        let mut state = State::new();
        for name in super::super::sunset_extra_modules::HIDDEN {
            assert!(state.find(name).is_some(), "{name} is a module");
        }
        let jesus = state.find("Jesus").unwrap();
        assert!(!state.listed()(&state.modules[jesus]));
        State::set_field(&mut state.modules, "modules.jesus", "true");
        assert!(state.listed()(&state.modules[jesus]));
        let gone = ["Potion Saver", "Reverse Step", "Auto Steal", "No Mining Trace", "Bow Spam", "Anti Bed", "Rotation", "No Rotate", "Extra Modules"];
        assert!(gone.iter().all(|name| state.module(name).is_none()));
        assert_eq!(state.module("Reach").map(|m| m.category), Some(Category::Combat));
        assert_eq!(state.module("Friend Guard").map(|m| m.category), Some(Category::Client));
        assert_eq!(state.module("Ore Sim").map(|m| m.category), Some(Category::World));
        assert_eq!(state.module("Unlock All Recipes").map(|m| m.category), Some(Category::World));
    }

    #[test]
    fn presets_are_valid_profiles_that_cut_down() {
        use super::super::sunset_presets::{PRESETS, find};
        let mut state = State::new();
        State::set_field(&mut state.modules, "killaura.on", "true");
        for (i, preset) in PRESETS.iter().enumerate() {
            assert_eq!(find(preset.id), Some(i));
            let preview = state.preview(preset).unwrap_or_else(|e| panic!("{}: {e}", preset.id));
            assert!(!preview.on.is_empty(), "{} turns something on", preset.id);
            assert!(!preview.off.contains(&"HUD") && !preview.on.contains(&"HUD"));
            assert_eq!(preview.off.contains(&"Kill Aura"), !preset.text.contains("killaura.on=true"), "{}", preset.id);
        }
        assert!(find("pvp-cart").is_some());
    }

    #[test]
    fn antibot_defaults_protect_combat_and_saved_optouts_still_load() {
        let mut state = State::new();
        assert!(state.module("Anti Bot").is_none());
        for name in ["Kill Aura", "Aim Assist", "Trigger Bot"] {
            assert!(state.module(name).unwrap().b("Anti bot"), "{name}");
            assert_eq!(state.module(name).unwrap().p("Anti bot sensitivity"), "Balanced");
        }
        state.load_text(concat!(
            "killaura.antibot=false\n", "aimassist.antibot=false\n", "triggerbot.antibot=false\n",
            "antibot.on=false\n", "antibot.sensitivity=Conservative\n",
            "killaura.antibotsensitivity=Conservative\n", "aimassist.antibotsensitivity=Sensitive\n",
        ));
        for name in ["Kill Aura", "Aim Assist", "Trigger Bot"] {
            assert!(!state.module(name).unwrap().b("Anti bot"), "{name}");
        }
        assert!(state.module("Anti Bot").is_none());
        assert_eq!(state.module("Kill Aura").unwrap().p("Anti bot sensitivity"), "Conservative");
        assert_eq!(state.module("Aim Assist").unwrap().p("Anti bot sensitivity"), "Sensitive");
    }

    #[test]
    fn cancelled_critical_look_restores_the_stopped_sprint() {
        CRIT_SPRINT.store(CRIT_SPRINT_STOPPED, Ordering::Relaxed);
        drop(CriticalCompletion(true));
        assert_eq!(CRIT_SPRINT.swap(0, Ordering::Relaxed), CRIT_SPRINT_DUE);
    }
    #[test]
    fn auto_clicker_cps_defaults_keep_saved_tick_delay_and_filters() {
        let mut state = State::new();
        let m = state.module("Auto Clicker").unwrap();
        assert_eq!(m.p("Mode"), "CPS");
        assert_eq!(m.p("CPS pattern"), "Smooth");
        assert!(!m.b("Respect miss cooldown"));
        state.load_text("autoclicker.mode=Delay\nautoclicker.delayticks=7\nautoclicker.rightowncps=true\nautoclicker.stoponblocks=true\n");
        let m = state.module("Auto Clicker").unwrap();
        assert_eq!(m.p("Mode"), "Delay");
        assert_eq!(m.n("Delay (ticks)"), 7.0);
        assert!(m.b("Right own CPS") && m.b("Stop on blocks"));
    }
}

#[cfg(test)]
mod scaffold_preset_tests {
    use super::super::sunset::*;

    #[test]
    fn flight_holds_still_for_its_setback_pause_then_flies() {
        let mut left = 3;
        assert_eq!([fly_paused(&mut left), fly_paused(&mut left), fly_paused(&mut left), fly_paused(&mut left)], [true, true, true, false]);
        assert_eq!(left, 0);
        let mut none = 0;
        assert!(!fly_paused(&mut none), "no setback, no pause");
    }

    // An Auto Armor module's lines become Inventory Manager's Auto Armor settings, its on/off independent of the
    // manager's own.
    #[test]
    fn auto_armor_settings_move_into_inventory_manager() {
        let old = "inventorycleaner.on=false
autoarmor.on=true
autoarmor.key=key.keyboard.g
autoarmor.delayticks=4
autoarmor.mode=Key
autoarmor.armorkey=key.keyboard.r
autoarmor.avoidcursed=true
";
        let text = merged_modules(old).into_owned();
        let mut state = State::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                State::set_field(&mut state.modules, k, v);
            }
        }
        let m = state.module("Inventory Manager").unwrap();
        assert!(!m.on, "the manager stays off");
        assert!(m.b("Auto Armor"), "Auto Armor stays on");
        assert_eq!(key_from_text(m.t("Auto Armor key")).as_deref(), Some("key.keyboard.g"));
        assert_eq!(m.n("Armor delay (ticks)"), 4.0);
        assert_eq!(m.p("Armor mode"), "Key");
        assert!(m.b("Avoid cursed"));
        assert!(state.module("Auto Armor").is_none(), "no second module");
        assert!(!text.contains("autoarmor."), "{text}");
    }

    // A profile saved before Auto Armor joined Inventory Manager and before Inventory Cleaner's rename, with the
    // Modules list's lines for both: it is accepted, and both land on Inventory Manager.
    #[test]
    fn old_profiles_with_modules_list_lines_still_load() {
        let state = State::new();
        let body = "modules.on=false
modules.autoarmor=true
modules.inventorycleaner=true
autoarmor.on=true
autoarmor.mode=Inventory open
inventorycleaner.on=true
inventorycleaner.delayticks=2
";
        let text = format!("{}
{body}", super::super::sunset_profiles::HEADER);
        assert!(state.check_profile(&text).is_ok(), "{:?}", state.check_profile(&text));
        let merged = merged_modules(body).into_owned();
        assert!(merged.contains("modules.inventorymanager=true") && !merged.contains("modules.autoarmor"), "{merged}");
        assert!(merged.contains("inventorymanager.autoarmor=true") && merged.contains("inventorymanager.armormode=Inventory open"), "{merged}");
    }

    // A profile saved before Burrow, Hole Filler, Auto City, Key Throw, Arrow Dodge, Hold to Eat and Target ESP went
    // (and PvP Bot's City with Auto City): their lines, Modules list entries included, go, and the rest loads.
    #[test]
    fn profiles_with_removed_modules_still_load() {
        let state = State::new();
        let body = "modules.on=false
modules.killaura=true
modules.burrow=false
modules.autocity=false
burrow.on=false
burrow.block=Obsidian
holefiller.on=true
autocity.breakrange=4.5
keythrow.pearlkey=G
arrowdodge.hitfireballs=true
holdtoeat.anyfood=false
targetesp.style=Ghost
pvpbot.city=true
pvpbot.on=false
killaura.on=true
";
        let text = format!("{}
{body}", super::super::sunset_profiles::HEADER);
        assert!(state.check_profile(&text).is_ok(), "{:?}", state.check_profile(&text));
        let merged = merged_modules(body).into_owned();
        for gone in ["burrow", "holefiller", "autocity", "keythrow", "arrowdodge", "holdtoeat", "targetesp", "pvpbot.city"] {
            assert!(!merged.contains(gone), "{gone}: {merged}");
        }
        assert!(merged.contains("modules.killaura=true") && merged.contains("killaura.on=true") && merged.contains("pvpbot.on=false"));
    }

    // Every preset field names one of Scaffold's settings and takes the value it gives.
    #[test]
    fn scaffold_presets_set_real_settings() {
        for (preset, fields) in SCAFFOLD_PRESETS {
            let mut state = State::new();
            for (name, value) in fields.iter() {
                State::set_field(&mut state.modules, &format!("scaffold.{}", norm(name)), value);
            }
            let m = state.module("Scaffold").unwrap();
            for (name, value) in fields.iter() {
                let setting = m.setting(name).unwrap_or_else(|| panic!("{preset}: no setting {name}"));
                let got = match &setting.value {
                    Value::Bool(b) => b.to_string(),
                    Value::Num { v, .. } => num(*v),
                    Value::Pick { i, of } => of[*i].to_owned(),
                    Value::Text(t) => t.clone(),
                };
                assert_eq!(got, *value, "{preset}: {name}");
            }
        }
    }
}

#[cfg(test)]
mod profile_robustness_tests {
    use super::super::sunset::*;
    use super::super::sunset_profiles::HEADER;

    // Every fine-stepped number (HUD positions by 0.001, frictions by 0.005) comes back from a save exactly.
    #[test]
    fn fine_numbers_survive_a_save_and_a_profile() {
        let mut state = State::new();
        let mut set = 0;
        for m in &mut state.modules {
            for s in &mut m.settings {
                if let Value::Num { v, min, max, step } = &mut s.value
                    && *step < 0.01
                {
                    *v = round_to((*min + (*max - *min) * 0.37).clamp(*min, *max), *step);
                    set += 1;
                }
            }
        }
        assert!(set > 0);
        let mut loaded = State::new();
        loaded.load_text(&state.save_text());
        let mut from_profile = State::new();
        from_profile.load_profile(&state.profile_text());
        for (a, (b, c)) in state.modules.iter().zip(loaded.modules.iter().zip(&from_profile.modules)) {
            for (x, (y, z)) in a.settings.iter().zip(b.settings.iter().zip(&c.settings)) {
                assert_eq!(x.value, y.value, "{}: {} after the config", a.name, x.name);
                assert_eq!(x.value, z.value, "{}: {} after a profile", a.name, x.name);
            }
        }
    }

    // A line break typed or pasted into a text setting can't split the config into a line of its own.
    #[test]
    fn text_values_stay_on_one_line() {
        let mut state = State::new();
        let i = state.find("ClickGUI").unwrap();
        let j = state.modules[i].settings.iter().position(|s| matches!(s.value, Value::Text(_))).unwrap();
        state.modules[i].settings[j].value = Value::Text("a\nkillaura.on=true\r\nb".into());
        let text = state.save_text();
        assert!(!text.lines().any(|l| l == "killaura.on=true"), "{text}");
        let mut loaded = State::new();
        loaded.load_text(&text);
        assert!(!loaded.is_on("Kill Aura"));
    }

    // Friends and Panic are never a profile's: saving leaves them out, loading never touches them.
    #[test]
    fn profiles_keep_friends_and_panic_out() {
        let mut state = State::new();
        state.set_friends(vec!["Alice".into()]);
        let text = state.profile_text();
        assert!(!text.contains("clickgui.friends") && !text.contains("panic.on"), "{text}");
        let hostile = format!("{HEADER}\nclickgui.friends=Mallory\npanic.on=true\nsprint.on=true\n");
        let checked = state.check_profile(&hostile).unwrap();
        assert!(checked.skipped.is_empty(), "{:?}", checked.skipped);
        state.load_profile(&checked.body);
        sync_flags(&mut state);
        assert_eq!(state.friends, vec!["Alice".to_owned()]);
        assert!(!state.is_on("Panic") && state.is_on("Sprint"));
    }

    // ClickGUI's message switches moved to Notifications with their values; Sunset Presence's switches are never a
    // profile's (a shared profile can't turn presence on).
    #[test]
    fn message_switches_move_and_presence_stays_out_of_profiles() {
        let mut state = State::new();
        let n = state.find("Notifications").unwrap();
        assert!(state.modules[n].on && state.modules[n].b("Module toggles") && state.modules[n].b("Module messages"));
        state.load_text("clickgui.togglemessages=false\nclickgui.modulemessages=true\n");
        assert!(!state.modules[n].b("Module toggles") && state.modules[n].b("Module messages"));
        let saved = state.save_text();
        assert!(saved.contains("notifications.moduletoggles=false") && !saved.contains("togglemessages"), "{saved}");

        let p = state.find("Sunset Presence").unwrap();
        assert!(state.modules[p].on && state.modules[p].b("Anonymous analytics") && state.modules[p].b("Share in-game presence"));
        // Turned off by the user: a profile can't turn it back on (nor the module off).
        State::set_field(&mut state.modules, "sunsetpresence.shareingamepresence", "false");
        let text = state.profile_text();
        assert!(!text.contains("sunsetpresence."), "{text}");
        let hostile = format!("{HEADER}\nsunsetpresence.shareingamepresence=true\nsunsetpresence.on=false\nsprint.on=true\n");
        let checked = state.check_profile(&hostile).unwrap();
        state.load_profile(&checked.body);
        assert!(state.modules[p].on && !state.modules[p].b("Share in-game presence") && state.is_on("Sprint"));
    }

    // A profile from another version or edited by hand: what this version doesn't know is left out and named, the
    // rest loads; Windows line ends and a byte order mark are fine; a file of nothing known is refused.
    #[test]
    fn unknown_lines_are_left_out_not_fatal() {
        let state = State::new();
        let text = format!("\u{feff}{HEADER}\r\nsprint.on=true\r\nnosuchmodule.on=true\r\nsprint.nosuchsetting=4\r\nkillaura.range=banana\r\n# a note\r\n");
        let checked = state.check_profile(&text).unwrap();
        assert_eq!(checked.body.trim(), "sprint.on=true");
        assert_eq!(checked.skipped.len(), 3, "{:?}", checked.skipped);
        assert!(checked.skipped_note().contains("3 lines"));
        assert!(state.check_profile(&format!("{HEADER}\nnosuchmodule.on=true\n")).is_err());
        assert!(state.check_profile("sprint.on=true\n").is_err());
    }

    // What a saved profile holds loads back to the same modules, keys and settings.
    #[test]
    fn a_saved_profile_loads_back_the_same() {
        let mut state = State::new();
        let i = state.find("Kill Aura").unwrap();
        state.modules[i].on = true;
        state.modules[i].key = Some("key.keyboard.r".into());
        let text = format!("{HEADER}\n{}\n", state.profile_text());
        let mut other = State::new();
        let checked = other.check_profile(&text).unwrap();
        assert!(checked.skipped.is_empty(), "{:?}", checked.skipped);
        other.load_profile(&checked.body);
        assert_eq!(other.profile_text(), state.profile_text());
    }
}

#[cfg(test)]
mod hud_place_tests {
    use super::super::sunset::*;

    // The layout editor drags an element by name: `X position` (with Custom), `X x` and `X y` in one module only, which
    // is where a drag writes and a right click resets.
    #[test]
    fn every_draggable_element_has_one_place() {
        let modules = defaults();
        let mut owners: Vec<(&str, &str)> = Vec::new();
        for m in &modules {
            for st in &m.settings {
                let Some(base) = st.name.strip_suffix(" position") else { continue };
                let Value::Pick { of, .. } = &st.value else { continue };
                if !of.contains(&"Custom") {
                    continue;
                }
                for axis in ["x", "y"] {
                    let name = format!("{base} {axis}");
                    assert!(
                        m.settings.iter().any(|s| s.name == name && matches!(s.value, Value::Num { .. })),
                        "{}: {name}",
                        m.name
                    );
                }
                assert!(owners.iter().all(|o| o.0 != base), "{base} is placed by {} and {}", m.name, owners.iter().find(|o| o.0 == base).unwrap().1);
                owners.push((base, m.name));
            }
        }
        for element in ["FPS", "Speed", "Facing", "Totems", "Shield status", "Block counter", "Murderer list", "Damage indicator", "Radar"] {
            assert!(owners.iter().any(|o| o.0 == element), "{element} has no place");
        }
    }

    fn hud(state: &mut State) -> &mut Module {
        state.modules.iter_mut().find(|m| m.name == "HUD").unwrap()
    }

    fn set(m: &mut Module, name: &str, value: &str) {
        assert!(m.settings.iter_mut().find(|s| s.name == name).unwrap().parse(value), "{name}={value}");
    }

    fn get(m: &Module, name: &str) -> String {
        m.settings.iter().find(|s| s.name == name).unwrap().stored()
    }

    // The defaults are the Default layout: watermark and coordinates top left, the list at the top of its side, no low
    // health warning.
    #[test]
    fn defaults_keep_the_old_layout() {
        let mut state = State::new();
        let m = hud(&mut state);
        assert_eq!(m.p("Watermark position"), "Top left");
        assert_eq!(m.p("Module list position"), "Top");
        assert_eq!(m.p("Coordinates position"), "Top left");
        assert!(!m.b("Low health warning"));
    }

    // Positions, Custom places and the low health warning come back from the config and from a profile.
    #[test]
    fn hud_places_survive_the_config_and_profiles() {
        let mut state = State::new();
        let m = hud(&mut state);
        for (name, value) in [
            ("Watermark position", "Custom"),
            ("Watermark x", "0.25"),
            ("Watermark y", "0.731"),
            ("Module list position", "Bottom"),
            ("Module list side", "Left"),
            ("Module list x", "0.9"),
            ("Coordinates position", "Bottom right"),
            ("Low health warning", "true"),
            ("Low health below", "8"),
            ("Low health style", "Flash"),
        ] {
            set(m, name, value);
        }
        let want: Vec<(String, String)> = m.settings.iter().map(|s| (s.name.to_owned(), s.stored())).collect();
        let mut from_config = State::new();
        from_config.load_text(&state.save_text());
        let mut from_profile = State::new();
        from_profile.load_profile(&state.profile_text());
        for loaded in [&mut from_config, &mut from_profile] {
            let m = hud(loaded);
            for (name, value) in &want {
                assert_eq!(&get(m, name), value, "{name}");
            }
        }
    }

    // A config written before keeps its coordinates where they were; a value no longer offered (1.2's "Default" too)
    // leaves the default, which is the same place.
    #[test]
    fn older_configs_load() {
        let mut state = State::new();
        state.load_text("hud.coordinatesposition=Custom\nhud.coordinatesx=0.4\nhud.watermarkposition=Middle\nhud.modulelistposition=Default\n");
        let m = hud(&mut state);
        assert_eq!(m.p("Coordinates position"), "Custom");
        assert_eq!(get(m, "Coordinates x"), "0.4");
        assert_eq!(m.p("Watermark position"), "Top left");
        assert_eq!(m.p("Module list position"), "Top");
    }

    // x and y show in the menu only with Custom, the low health settings only with the warning on.
    #[test]
    fn place_settings_show_with_what_they_move() {
        let mut state = State::new();
        let m = hud(&mut state);
        let shown = |m: &Module, name: &str| m.setting_visible(m.settings.iter().position(|s| s.name == name).unwrap());
        assert!(shown(m, "Watermark position") && !shown(m, "Watermark x") && !shown(m, "Module list y"));
        assert!(!shown(m, "Low health below") && !shown(m, "Low health style"));
        set(m, "Watermark position", "Custom");
        set(m, "Module list position", "Custom");
        set(m, "Low health warning", "true");
        assert!(shown(m, "Watermark x") && shown(m, "Watermark y") && shown(m, "Module list x") && shown(m, "Module list y"));
        assert!(shown(m, "Low health below") && shown(m, "Low health style") && shown(m, "Low health strength"));
        set(m, "Watermark", "false");
        assert!(!shown(m, "Watermark position") && !shown(m, "Watermark x"));
    }
}
