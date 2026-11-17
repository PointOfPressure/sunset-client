// baritone.command.defaults: help, set (and its aliases modified and reset), version, repack, gc,
// render, reloadall, saveall, waypoints (and its aliases sethome and home). Names, aliases,
// descriptions and messages are Baritone 1.21.4's: SetCommand, WaypointsCommand and the rest.
use super::baritone::{Baritone, SETTINGS_DEFAULT_NAME, VERSION};
use super::baritone_argument::{Args, CommandError, CommandResult};
use super::baritone_command::{alias_run, clickable, dimension, feet, help, linked, now_ms, paginate, relative_pos, run_event, Command, CLICK};
use super::baritone_goals::{rc, GoalBlock};
use super::baritone_settings::{self as settings, Def, Kind};
use super::baritone_utils::pos_label;
use super::baritone_waypoints::{Tag, Waypoint};
use super::Game;
use horizon_client::text::{self, ClickEvent, Style, Text};

pub const COMMANDS: &[Command] = &[
    Command {
        names: &["help", "?"],
        short: "View all commands or help on specific ones",
        long: &[
            "Using this command, you can view detailed help information on how to use certain commands of Baritone.",
            "",
            "Usage:",
            "> help - Lists all commands and their short descriptions.",
            "> help <command> - Displays help information on a specific command.",
        ],
        alias: "",
        run: help,
    },
    Command {
        names: &["set", "setting", "settings"],
        short: "View or change settings",
        long: &[
            "Using the set command, you can manage all of Baritone's settings. Almost every aspect is controlled by these settings - go wild!",
            "",
            "Usage:",
            "> set - Same as `set list`",
            "> set list [page] - View all settings",
            "> set modified [page] - View modified settings",
            "> set <setting> - View the current value of a setting",
            "> set <setting> <value> - Set the value of a setting",
            "> set reset all - Reset ALL SETTINGS to their defaults",
            "> set reset <setting> - Reset a setting to its default",
            "> set toggle <setting> - Toggle a boolean setting",
            "> set save - Save all settings (this is automatic tho)",
            "> set load - Load settings from settings.txt",
            "> set load [filename] - Load settings from another file in your minecraft/baritone",
        ],
        alias: "",
        run: set,
    },
    Command {
        names: &["modified", "mod", "baritone", "modifiedsettings"],
        short: "List modified settings",
        long: &[],
        alias: "set modified",
        run: alias_run,
    },
    Command {
        names: &["reset"],
        short: "Reset all settings or just one",
        long: &[],
        alias: "set reset",
        run: alias_run,
    },
    Command {
        names: &["version"],
        short: "View the Baritone version",
        long: &[
            "The version command prints the version of Baritone you're currently running.",
            "",
            "Usage:",
            "> version - View version information, if present",
        ],
        alias: "",
        run: version,
    },
    Command {
        names: &["repack", "rescan"],
        short: "Re-cache chunks",
        long: &["Repack chunks around you. This basically re-caches them.", "", "Usage:", "> repack - Repack chunks."],
        alias: "",
        run: repack,
    },
    Command {
        names: &["gc"],
        short: "Call System.gc()",
        long: &["Calls System.gc().", "", "Usage:", "> gc"],
        alias: "",
        run: gc,
    },
    Command {
        names: &["render"],
        short: "Fix glitched chunks",
        long: &["The render command fixes glitched chunk rendering without having to reload all of them.", "", "Usage:", "> render"],
        alias: "",
        run: render,
    },
    Command {
        names: &["reloadall"],
        short: "Reloads Baritone's cache for this world",
        long: &["The reloadall command reloads Baritone's world cache.", "", "Usage:", "> reloadall"],
        alias: "",
        run: reload_all,
    },
    Command {
        names: &["saveall"],
        short: "Saves Baritone's cache for this world",
        long: &["The saveall command saves Baritone's world cache.", "", "Usage:", "> saveall"],
        alias: "",
        run: save_all,
    },
    Command {
        names: &["waypoints", "waypoint", "wp"],
        short: "Manage waypoints",
        long: &[
            "The waypoint command allows you to manage Baritone's waypoints.",
            "",
            "Waypoints can be used to mark positions for later. Waypoints are each given a tag and an optional name.",
            "",
            "Note that the info, delete, and goal commands let you specify a waypoint by tag. If there is more than one waypoint with a certain tag, then they will let you select which waypoint you mean.",
            "",
            "Missing arguments for the save command use the USER tag, creating an unnamed waypoint and your current position as defaults.",
            "",
            "Usage:",
            "> wp [l/list] - List all waypoints.",
            "> wp <l/list> <tag> - List all waypoints by tag.",
            "> wp <s/save> - Save an unnamed USER waypoint at your current position",
            "> wp <s/save> [tag] [name] [pos] - Save a waypoint with the specified tag, name and position.",
            "> wp <i/info/show> <tag/name> - Show info on a waypoint by tag or name.",
            "> wp <d/delete> <tag/name> - Delete a waypoint by tag or name.",
            "> wp <restore> <n> - Restore the last n deleted waypoints.",
            "> wp <c/clear> <tag> - Delete all waypoints with the specified tag.",
            "> wp <g/goal> <tag/name> - Set a goal to a waypoint by tag or name.",
            "> wp <goto> <tag/name> - Set a goal to a waypoint by tag or name and start pathing.",
        ],
        alias: "",
        run: waypoints,
    },
    Command {
        names: &["sethome"],
        short: "Sets your home waypoint",
        long: &[],
        alias: "waypoints save home",
        run: alias_run,
    },
    Command {
        names: &["home"],
        short: "Path to your home waypoint",
        long: &[],
        alias: "waypoints goto home",
        run: alias_run,
    },
];

// ---------------------------------------------------------------------------------------
// set

fn type_name(d: &Def) -> &'static str {
    // SettingsUtil.settingTypeToString: the Java type, `Boolean`, `Double`, `List<Block>`.
    settings::java_type(d.name)
}

fn setting_by_name(args: &Args, name: &str) -> CommandResult<&'static Def> {
    settings::find(name).ok_or_else(|| args.invalid_type("a valid setting"))
}

fn set_line(b: &Baritone, d: &Def) -> Text {
    let prefix = b.core.settings.text("prefix");
    let hover = Text::colored(
        format!("{}\nType: {}\n\nValue:\n{}\n\nDefault Value:\n{}", d.name, type_name(d), b.core.settings.get(d.name), d.default),
        text::GRAY,
    );
    linked(
        vec![(d.name.to_owned(), text::GRAY), (format!(" ({})", type_name(d)), text::DARK_GRAY)],
        ClickEvent::SuggestCommand(format!("{prefix}set {} ", d.name)),
        hover,
    )
}

fn set(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    let arg = if args.has_any() { args.string("an argument")?.to_ascii_lowercase() } else { "list".to_owned() };
    if arg == "s" || arg == "save" {
        b.core.save_settings(game);
        b.core.say(game, "Settings saved");
        return Ok(());
    }
    if arg == "load" || arg == "ld" {
        let file = if args.has_any() { args.string("a file")? } else { SETTINGS_DEFAULT_NAME.to_owned() };
        for line in b.core.load_settings(game, &file) {
            b.core.say(game, &line);
        }
        b.core.say(game, &format!("Settings reloaded from {file}"));
        return Ok(());
    }
    let view_modified = ["m", "mod", "modified"].contains(&arg.as_str());
    let view_all = ["all", "l", "list"].contains(&arg.as_str());
    if view_modified || view_all {
        let search = match args.peek() {
            Some(w) if w.parse::<i64>().is_err() => args.optional_string().unwrap_or_default(),
            _ => String::new(),
        };
        args.require_max(1)?;
        let needle = search.to_ascii_lowercase();
        let mut chosen: Vec<&'static Def> = settings::DEFS
            .iter()
            .filter(|d| !view_modified || b.core.settings.modified(d))
            .filter(|d| d.name.to_ascii_lowercase().contains(&needle))
            .collect();
        chosen.sort_by_key(|d| d.name.to_ascii_lowercase());
        let entries: Vec<Text> = chosen.iter().map(|d| set_line(b, d)).collect();
        let header = if search.is_empty() {
            format!("All {}settings:", if view_modified { "modified " } else { "" })
        } else {
            format!("All {}settings containing the string '{search}':", if view_modified { "modified " } else { "" })
        };
        return paginate(b, game, args, entries, &header, &format!("set {arg} {search}").trim_end().to_owned());
    }
    args.require_max(1)?;
    let resetting = arg == "reset";
    let toggling = arg == "toggle";
    let doing_something = resetting || toggling;
    if resetting {
        if !args.has_any() {
            b.core.say(game, "Please specify 'all' as an argument to reset to confirm you'd really like to do this");
            b.core.say(game, "ALL settings will be reset. Use the 'set modified' or 'modified' commands to see what will be reset.");
            b.core.say(game, "Specify a setting name instead of 'all' to only reset one setting");
        } else if args.peek().is_some_and(|w| w.eq_ignore_ascii_case("all")) {
            let changed: Vec<&'static Def> = settings::DEFS.iter().filter(|d| b.core.settings.modified(d)).collect();
            for d in changed {
                b.core.reset_setting(game, d);
            }
            b.core.say(game, "All settings have been reset to their default values");
            b.core.save_settings(game);
            return Ok(());
        }
    }
    if toggling {
        args.require_min(1)?;
    }
    let name = if doing_something { args.string("a setting")? } else { arg.clone() };
    let d = setting_by_name(args, &name)?;
    if !doing_something && !args.has_any() {
        b.core.say(game, &format!("Value of setting {}:", d.name));
        let value = b.core.settings.get(d.name).to_owned();
        b.core.say(game, &value);
    } else {
        let old = b.core.settings.get(d.name).to_owned();
        if resetting {
            b.core.reset_setting(game, d);
        } else if toggling {
            if d.kind != Kind::Bool {
                return Err(args.invalid_type_got("a toggleable setting", "some other setting"));
            }
            let flipped = if b.core.settings.flag(d.name) { "false" } else { "true" };
            b.core.set_setting(game, d.name, flipped).map_err(CommandError)?;
            let now = b.core.settings.get(d.name).to_owned();
            b.core.say(game, &format!("Toggled setting {} to {now}", d.name));
        } else {
            let value = args.string("a value")?;
            if b.core.set_setting(game, d.name, &value).is_err() {
                return Err(args.invalid_type("a valid value"));
            }
        }
        if !toggling {
            let now = b.core.settings.get(d.name).to_owned();
            b.core.say(game, &format!("Successfully {} {} to {now}", if resetting { "reset" } else { "set" }, d.name));
        }
        let undo = clickable(
            &format!("Old value: {old}"),
            text::GRAY,
            run_event(&format!("set {} {old}", d.name)),
            Text::colored("Click to set the setting back to this value", text::GRAY),
        );
        b.core.say_text(game, undo);
        let s = &b.core.settings;
        if (d.name == "chatControl" && !s.flag("chatControl") && !s.flag("chatControlAnyway"))
            || (d.name == "chatControlAnyway" && !s.flag("chatControlAnyway") && !s.flag("chatControl"))
        {
            b.core.say_red(game, "Warning: Chat commands will no longer work. If you want to revert this change, use prefix control (if enabled) or click the old value listed above.");
        } else if d.name == "prefixControl" && !s.flag("prefixControl") {
            b.core.say_red(game, "Warning: Prefixed commands will no longer work. If you want to revert this change, use chat control (if enabled) or click the old value listed above.");
        }
    }
    b.core.save_settings(game);
    Ok(())
}

// ---------------------------------------------------------------------------------------
// The small ones.

fn version(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.core.say(game, &format!("You are running Baritone v{VERSION}"));
    Ok(())
}

fn repack(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let n = b.core.world_data.repack(game, 40);
    b.core.say(game, &format!("Queued {n} chunks for repacking"));
    Ok(())
}

fn gc(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.core.say(game, "ok called System.gc()");
    Ok(())
}

fn render(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    // `setBlocksDirty` over the render distance and one chunk more, every height: those sections are built again.
    let origin = feet(game)?;
    let reach = game.settings.render_distance + 1;
    let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
    let columns: Vec<(i32, i32)> = game.world.chunks.keys().copied().filter(|(x, z)| (x - cx).abs() <= reach && (z - cz).abs() <= reach).collect();
    for (x, z) in columns {
        for section in 0..horizon_client::chunk::HEIGHT / 16 {
            game.world.dirty_sections.insert((x, section, z));
        }
    }
    b.core.say(game, "Done");
    Ok(())
}

fn reload_all(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.core.world_data.reload_all();
    b.core.say(game, "Reloaded");
    Ok(())
}

fn save_all(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.core.save_settings(game);
    b.core.save_waypoints(game);
    let Baritone { core, .. } = b;
    core.world_data.save(game, &core.settings);
    b.core.say(game, "Saved");
    Ok(())
}

// ---------------------------------------------------------------------------------------
// waypoints

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    List,
    Clear,
    Save,
    Info,
    Delete,
    Restore,
    Goal,
    Goto,
}

const ACTIONS: &[(Action, &[&str])] = &[
    (Action::List, &["list", "get", "l"]),
    (Action::Clear, &["clear", "c"]),
    (Action::Save, &["save", "s"]),
    (Action::Info, &["info", "show", "i"]),
    (Action::Delete, &["delete", "d"]),
    (Action::Restore, &["restore"]),
    (Action::Goal, &["goal", "g"]),
    (Action::Goto, &["goto"]),
];

fn action_name(a: Action) -> &'static str {
    ACTIONS.iter().find(|(x, _)| *x == a).map_or("list", |(_, n)| n[0])
}

/// java.util.Date.toString for a time in milliseconds, in UTC.
fn date_text(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!(
        "{} {} {:02} {:02}:{:02}:{:02} UTC {}",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        day,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        year
    )
}

fn waypoint_line(w: &Waypoint, label: &str, action: Action) -> Text {
    let name = if w.name.is_empty() { "<empty>" } else { w.name.as_str() };
    let name_color = if w.name.is_empty() { text::DARK_GRAY } else { text::GRAY };
    let click = ClickEvent::RunCommand(format!("{CLICK} {label} {} {} @ {}", action_name(action), w.tag.name(), w.created));
    linked(
        vec![
            (format!("{} ", w.tag.name().to_ascii_uppercase()), text::GRAY),
            (name.to_owned(), name_color),
            (format!(" @ {}", date_text(w.created)), text::DARK_GRAY),
        ],
        click,
        Text::colored("Click to select", text::GRAY),
    )
}

fn run_line(label: &str, color: u32, command: String) -> Text {
    Text::styled(label, Style { click: Some(ClickEvent::RunCommand(format!("{CLICK} {command}"))), ..Style::color(color) })
}

fn waypoints(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    let action = if args.has_any() {
        let w = args.string("an action")?;
        ACTIONS
            .iter()
            .find(|(_, names)| names.iter().any(|n| n.eq_ignore_ascii_case(&w)))
            .map(|(a, _)| *a)
            .ok_or_else(|| args.invalid_type("an action"))?
    } else {
        Action::List
    };
    let label = "wp";
    let dim = dimension(game);
    let shown = if action == Action::List { Action::Info } else { action };
    match action {
        Action::List => {
            let tag = args.peek().and_then(Tag::by_name);
            if tag.is_some() {
                args.optional_string();
            }
            let found: Vec<Waypoint> = match tag {
                Some(t) => b.core.waypoints.by_tag(t, &dim),
                None => b.core.waypoints.in_dimension(&dim),
            }
            .into_iter()
            .cloned()
            .collect();
            if found.is_empty() {
                args.require_max(0)?;
                return Err(CommandError::new(if tag.is_some() { "No waypoints found by that tag" } else { "No waypoints found" }));
            }
            args.require_max(1)?;
            let entries: Vec<Text> = found.iter().map(|w| waypoint_line(w, label, shown)).collect();
            let header = match tag {
                Some(t) => format!("All waypoints by tag {}:", t.name().to_ascii_uppercase()),
                None => "All waypoints:".to_owned(),
            };
            let prefix = format!("{label} list{}", tag.map_or(String::new(), |t| format!(" {}", t.name())));
            paginate(b, game, args, entries, &header, &prefix)?;
        }
        Action::Save => {
            let tag = match args.peek().and_then(Tag::by_name) {
                Some(t) => {
                    args.optional_string();
                    t
                }
                None => Tag::User,
            };
            let name = if args.has_exactly(1) || args.has_exactly(4) { args.string("a name")? } else { String::new() };
            let here = feet(game)?;
            let pos = if args.has_any() { relative_pos(args, here)? } else { here };
            args.require_max(0)?;
            let w = Waypoint { tag, name, pos, dimension: dim, created: now_ms() };
            b.core.waypoints.add(w.clone());
            b.core.save_waypoints(game);
            let line = Text::colored("Waypoint added: ", text::GRAY).append(waypoint_line(&w, label, Action::Info));
            b.core.say_text(game, line);
        }
        Action::Clear => {
            args.require_max(1)?;
            let word = args.string("a tag")?;
            let tag = Tag::by_name(&word).ok_or_else(|| CommandError::new(format!("Invalid tag, \"{word}\"")))?;
            let removed: Vec<Waypoint> = b.core.waypoints.by_tag(tag, &dim).into_iter().cloned().collect();
            for w in &removed {
                b.core.waypoints.remove(w);
            }
            let stamps: Vec<String> = removed.iter().map(|w| w.created.to_string()).collect();
            b.core.deleted_waypoints.extend(removed.iter().cloned());
            b.core.save_waypoints(game);
            let line = Text::styled(
                format!("Cleared {} waypoints, click to restore them", removed.len()),
                Style { click: Some(ClickEvent::RunCommand(format!("{CLICK} {label} restore @ {}", stamps.join(" ")))), ..Style::color(text::GRAY) },
            );
            b.core.say_text(game, line);
        }
        Action::Restore => {
            let mut restored: Vec<Waypoint> = Vec::new();
            if args.peek() == Some("@") {
                args.optional_string();
                while args.has_any() {
                    let w = args.string("a timestamp")?;
                    let ts: u64 = w.parse().map_err(|_| args.invalid_type("Long"))?;
                    if let Some(found) = b.core.deleted_waypoints.iter().find(|x| x.created == ts) {
                        restored.push(found.clone());
                    }
                }
            } else {
                args.require_exactly(1)?;
                let n = args.int("a number")?.max(0) as usize;
                let size = b.core.deleted_waypoints.len();
                restored = b.core.deleted_waypoints[size - n.min(size)..].to_vec();
            }
            for w in &restored {
                b.core.waypoints.add(w.clone());
            }
            b.core.deleted_waypoints.retain(|w| !restored.contains(w));
            b.core.save_waypoints(game);
            b.core.say(game, &format!("Restored {} waypoints", restored.len()));
        }
        _ => {
            let word = args.string("a waypoint")?;
            let found: Vec<Waypoint> = b.core.waypoints.lookup(&word, &dim).into_iter().cloned().collect();
            let mut chosen: Option<Waypoint> = None;
            if args.peek() == Some("@") {
                args.require_exactly(2)?;
                args.optional_string();
                let w = args.string("a timestamp")?;
                let ts: u64 = w.parse().map_err(|_| args.invalid_type("Long"))?;
                chosen = found.iter().find(|x| x.created == ts).cloned();
                if chosen.is_none() {
                    return Err(CommandError::new("Timestamp was specified but no waypoint was found"));
                }
            } else {
                match found.len() {
                    0 => return Err(CommandError::new("No waypoints found")),
                    1 => chosen = Some(found[0].clone()),
                    _ => {}
                }
            }
            let Some(w) = chosen else {
                args.require_max(1)?;
                let entries: Vec<Text> = found.iter().map(|w| waypoint_line(w, label, shown)).collect();
                let prefix = format!("{label} {} {}", action_name(action), args.consumed_string().split_once(' ').map_or("", |(_, rest)| rest));
                return paginate(b, game, args, entries, "Multiple waypoints were found:", prefix.trim_end());
            };
            match action {
                Action::Info => {
                    b.core.say_text(game, waypoint_line(&w, label, Action::Info));
                    b.core.say(game, &format!("Position: {}", pos_label(w.pos)));
                    let tag = w.tag.name();
                    let ts = w.created;
                    b.core.say_text(game, run_line("Click to delete this waypoint", text::WHITE, format!("{label} delete {tag} @ {ts}")));
                    b.core.say_text(game, run_line("Click to set goal to this waypoint", text::WHITE, format!("{label} goal {tag} @ {ts}")));
                    let prefix = b.core.settings.text("prefix");
                    let recreate = Text::styled(
                        "Click to show a command to recreate this waypoint",
                        Style {
                            click: Some(ClickEvent::SuggestCommand(format!("{prefix}{label} save {tag} {} {} {} {}", w.name, w.pos[0], w.pos[1], w.pos[2]))),
                            ..Style::color(text::WHITE)
                        },
                    );
                    b.core.say_text(game, recreate);
                    b.core.say_text(game, run_line("Click to return to the waypoints list", text::WHITE, format!("{label} list")));
                }
                Action::Delete => {
                    b.core.waypoints.remove(&w);
                    b.core.deleted_waypoints.push(w.clone());
                    b.core.save_waypoints(game);
                    b.core.say_text(
                        game,
                        run_line("That waypoint has successfully been deleted, click to restore it", text::WHITE, format!("{label} restore @ {}", w.created)),
                    );
                }
                Action::Goal => {
                    let goal = rc(GoalBlock(w.pos));
                    let name = goal.label();
                    b.procs.goal.set_goal(goal);
                    b.core.say(game, &format!("Goal: {name}"));
                }
                _ => {
                    let goal = rc(GoalBlock(w.pos));
                    let name = goal.label();
                    b.procs.goal.set_goal_and_path(goal);
                    b.core.say(game, &format!("Going to: {name}"));
                }
            }
        }
    }
    Ok(())
}
