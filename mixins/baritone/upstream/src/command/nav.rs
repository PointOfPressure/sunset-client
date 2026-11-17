// baritone.command.defaults: the commands that go somewhere or stop going: goal, goto, path, proc, eta,
// come, axis, forcecancel, invert, click, surface, thisway, and the execution control commands pause,
// resume, paused and cancel. Names, aliases, descriptions and messages are Baritone 1.21.4's.
use super::baritone::Baritone;
use super::baritone_argument::{Args, CommandError, CommandResult};
use super::baritone_command::{feet, relative_goal, Command};
use super::baritone_goals::{rc, GoalAxis, GoalBlock, GoalInverted, GoalRef, GoalXZ};
use super::baritone_gworld::is_air_at;
use super::baritone_utils::{column_ahead, up};
use super::Game;


pub const COMMANDS: &[Command] = &[
    Command {
        names: &["goal"],
        short: "Set or clear the goal",
        long: &[
            "The goal command allows you to set or clear Baritone's goal.",
            "",
            "Wherever a coordinate is expected, you can use ~ just like in regular Minecraft commands. Or, you can just use regular numbers.",
            "",
            "Usage:",
            "> goal - Set the goal to your current position",
            "> goal <reset/clear/none> - Erase the goal",
            "> goal <y> - Set the goal to a Y level",
            "> goal <x> <z> - Set the goal to an X,Z position",
            "> goal <x> <y> <z> - Set the goal to an X,Y,Z position",
        ],
        alias: "",
        run: goal,
    },
    Command {
        names: &["goto"],
        short: "Go to a coordinate or block",
        long: &[
            "The goto command tells Baritone to head towards a given goal or block.",
            "",
            "Wherever a coordinate is expected, you can use ~ just like in regular Minecraft commands. Or, you can just use regular numbers.",
            "",
            "Usage:",
            "> goto <block> - Go to a block, wherever it is in the world",
            "> goto <y> - Go to a Y level",
            "> goto <x> <z> - Go to an X,Z position",
            "> goto <x> <y> <z> - Go to an X,Y,Z position",
        ],
        alias: "",
        run: goto,
    },
    Command {
        names: &["path"],
        short: "Start heading towards the goal",
        long: &["The path command tells Baritone to head towards the current goal.", "", "Usage:", "> path - Start the pathing."],
        alias: "",
        run: path,
    },
    Command {
        names: &["proc"],
        short: "View process state information",
        long: &[
            "The proc command provides miscellaneous information about the process currently controlling Baritone.",
            "",
            "You are not expected to understand this if you aren't familiar with how Baritone works.",
            "",
            "Usage:",
            "> proc - View process information, if present",
        ],
        alias: "",
        run: proc_command,
    },
    Command {
        names: &["eta"],
        short: "View the current ETA",
        long: &[
            "The ETA command provides information about the estimated time until the next segment.",
            "and the goal",
            "",
            "Be aware that the ETA to your goal is really unprecise",
            "",
            "Usage:",
            "> eta - View ETA, if present",
        ],
        alias: "",
        run: eta,
    },
    Command {
        names: &["come"],
        short: "Start heading towards your camera",
        long: &[
            "The come command tells Baritone to head towards your camera.",
            "",
            "This can be useful in hacked clients where freecam doesn't move your player position.",
            "",
            "Usage:",
            "> come",
        ],
        alias: "",
        run: come,
    },
    Command {
        names: &["axis", "highway"],
        short: "Set a goal to the axes",
        long: &[
            "The axis command sets a goal that tells Baritone to head towards the nearest axis. That is, X=0 or Z=0 or X=Z or X=-Z.",
            "The target y level is configured using the axisHeight setting.",
            "",
            "Usage:",
            "> axis",
        ],
        alias: "",
        run: axis,
    },
    Command {
        names: &["forcecancel"],
        short: "Force cancel",
        long: &["Like cancel, but more forceful.", "", "Usage:", "> forcecancel"],
        alias: "",
        run: force_cancel,
    },
    Command {
        names: &["invert"],
        short: "Run away from the current goal",
        long: &[
            "The invert command tells Baritone to head away from the current goal rather than towards it.",
            "",
            "Usage:",
            "> invert - Invert the current goal.",
        ],
        alias: "",
        run: invert,
    },
    Command {
        names: &["click"],
        short: "Open click",
        long: &[
            "Opens a GUI to select positions using your mouse.",
            "- Right click to walk onto a block.",
            "- Left click to walk into a block.",
            "- Left click drag to create a selection for #selection.",
            "",
            "Usage:",
            "> click",
        ],
        alias: "",
        run: click,
    },
    Command {
        names: &["surface", "top"],
        short: "Used to get out of caves, mines, ...",
        long: &[
            "The surface/top command tells Baritone to head towards the closest surface-like area.",
            "",
            "This can be the surface or the highest available air space, depending on circumstances.",
            "",
            "Usage:",
            "> surface - Used to get out of caves, mines, ...",
            "> top - Used to get out of caves, mines, ...",
        ],
        alias: "",
        run: surface,
    },
    Command {
        names: &["thisway", "forward"],
        short: "Travel in your current direction",
        long: &[
            "Creates a GoalXZ some amount of blocks in the direction you're currently looking",
            "",
            "Usage:",
            "> thisway <distance> - makes a GoalXZ distance blocks in front of you",
        ],
        alias: "",
        run: thisway,
    },
    Command {
        names: &["pause", "p", "paws"],
        short: "Pauses Baritone until you use resume",
        long: &[
            "The pause command tells Baritone to temporarily stop whatever it's doing.",
            "",
            "This can be used to pause pathing, building, following, whatever. A single use of the resume command will start it right back up again!",
            "",
            "Usage:",
            "> pause",
        ],
        alias: "",
        run: pause,
    },
    Command {
        names: &["resume", "r", "unpause", "unpaws"],
        short: "Resumes Baritone after a pause",
        long: &["The resume command tells Baritone to resume whatever it was doing when you last used pause.", "", "Usage:", "> resume"],
        alias: "",
        run: resume,
    },
    Command {
        names: &["paused"],
        short: "Tells you if Baritone is paused",
        long: &["The paused command tells you if Baritone is currently paused by use of the pause command.", "", "Usage:", "> paused"],
        alias: "",
        run: paused,
    },
    Command {
        names: &["cancel", "c", "stop"],
        short: "Cancel what Baritone is currently doing",
        long: &["The cancel command tells Baritone to stop whatever it's currently doing.", "", "Usage:", "> cancel"],
        alias: "",
        run: cancel,
    },
];

fn goto(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    // A number first: coordinates. There is no reason to want to go where you already are, so no
    // arguments at all is not a place.
    if args.peek().is_some_and(super::baritone_command::looks_relative) {
        args.require_max(3)?;
        let origin = feet(game)?;
        let goal = relative_goal(args, origin)?;
        b.core.say(game, &format!("Going to: {}", goal.label()));
        b.procs.goal.set_goal_and_path(goal);
        return Ok(());
    }
    args.require_max(1)?;
    let word = args.string("a block")?;
    let bom = super::baritone_command::block_optional_meta(b, game, args, &word)?;
    let Baritone { core, procs, .. } = b;
    procs.get_to_block.get_to_block(core, game, bom);
    Ok(())
}

fn goal(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    if args.peek().is_some_and(|w| ["reset", "clear", "none"].contains(&w)) {
        args.require_max(1)?;
        if b.procs.goal.clear_goal() {
            b.core.say(game, "Cleared goal");
        } else {
            b.core.say(game, "There was no goal to clear");
        }
        return Ok(());
    }
    args.require_max(3)?;
    let origin = feet(game)?;
    let goal = relative_goal(args, origin)?;
    b.core.say(game, &format!("Goal: {}", goal.label()));
    b.procs.goal.set_goal(goal);
    Ok(())
}

fn path(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    if b.procs.goal.goal().is_none() {
        b.core.say(game, "No goal set");
        return Ok(());
    }
    b.procs.goal.path();
    b.core.say(game, "Now pathing");
    Ok(())
}

fn proc_command(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let (name, priority, temporary, display) = b.describe().ok_or_else(|| CommandError::new("No process in control"))?;
    let last = b.last_command.clone().unwrap_or_else(|| "None".to_owned());
    b.core.say(game, &format!("Class: {name}\nPriority: {priority:.6}\nTemporary: {temporary}\nDisplay name: {display}\nLast command: {last}"));
    Ok(())
}

fn format_time(ticks: Option<f64>) -> String {
    const UNITS: [&str; 5] = ["seconds", "minutes", "hours", "days", "weeks"];
    const PER: [i64; 5] = [20, 1200, 72000, 1_728_000, 12_096_000];
    let Some(t) = ticks.filter(|t| t.is_finite()) else {
        return "NaN".to_owned();
    };
    for i in (0..UNITS.len()).rev() {
        let value = t as i64 / PER[i];
        if value >= 2 {
            return format!("{value} {} ({:.0} ticks)", UNITS[i], t);
        }
    }
    format!("{t:.0} ticks")
}

fn eta(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    if b.describe().is_none() {
        return Err(CommandError::new("No process in control"));
    }
    let segment = b.core.ticks_remaining_in_segment();
    let goal = b.core.estimated_ticks_to_goal(game);
    b.core.say(game, &format!("Next segment: {}\n        Goal: {}", format_time(segment), format_time(goal)));
    if goal.is_some_and(|t| t.is_finite() && t >= 24_192_000.0) {
        b.core.say(game, "Please don't do this to me.");
    }
    Ok(())
}

fn come(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    // The camera is where the player's eyes are: the block they are in.
    let at = feet(game)?;
    b.procs.goal.set_goal_and_path(rc(GoalBlock(at)));
    b.core.say(game, "Coming");
    Ok(())
}

fn axis(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let goal: GoalRef = rc(GoalAxis(b.core.settings.int("axisHeight")));
    b.procs.goal.set_goal(goal.clone());
    b.core.say(game, &format!("Goal: {}", goal.label()));
    Ok(())
}

fn force_cancel(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.force_cancel(game);
    b.core.say(game, "ok force canceled");
    Ok(())
}

fn invert(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let goal = b.procs.goal.goal().cloned().ok_or_else(|| CommandError::new("No goal"))?;
    let flipped: GoalRef = match goal.inverted_origin() {
        Some(origin) => origin,
        None => rc(GoalInverted(goal)),
    };
    b.core.say(game, &format!("Goal: {}", flipped.label()));
    b.procs.goal.set_goal_and_path(flipped);
    Ok(())
}

fn click(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.core.click.request_open();
    b.core.say(game, "aight dude");
    Ok(())
}

fn surface(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let player = feet(game)?;
    let sea_level = 63;
    let top = horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT;
    // Already out in the open: above the sea with air over the head.
    if player[1] > sea_level && is_air_at(game, up(player, 1)) {
        b.core.say(game, "Already at surface");
        return Ok(());
    }
    let start = player[1].max(sea_level);
    for y in start..top {
        let p = [player[0], y, player[2]];
        if !is_air_at(game, p) && y > player[1] {
            let goal: GoalRef = rc(GoalBlock(up(p, 1)));
            b.core.say(game, &format!("Going to: {}", goal.label()));
            b.procs.goal.set_goal_and_path(goal);
            return Ok(());
        }
    }
    b.core.say(game, "No higher location found");
    Ok(())
}

fn thisway(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_exactly(1)?;
    let at = feet(game)?;
    let distance = args.float("a distance")?;
    let (x, z) = column_ahead([f64::from(at[0]), f64::from(at[1]), f64::from(at[2])], f64::from(game.yaw), distance);
    let goal: GoalRef = rc(GoalXZ(x, z));
    b.procs.goal.set_goal(goal.clone());
    b.core.say(game, &format!("Goal: {}", goal.label()));
    Ok(())
}

fn pause(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    if b.procs.pause.paused {
        return Err(CommandError::new("Already paused"));
    }
    b.procs.pause.paused = true;
    b.core.say(game, "Paused");
    Ok(())
}

fn resume(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    if !b.procs.pause.paused && !b.procs.build.is_paused() {
        return Err(CommandError::new("Not paused"));
    }
    b.procs.build.resume();
    b.procs.pause.paused = false;
    b.core.say(game, "Resumed");
    Ok(())
}

fn paused(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    let on = b.procs.pause.paused;
    b.core.say(game, &format!("Baritone is {}paused", if on { "" } else { "not " }));
    Ok(())
}

fn cancel(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    b.procs.pause.paused = false;
    b.cancel_everything(game);
    b.core.say(game, "ok canceled");
    Ok(())
}
