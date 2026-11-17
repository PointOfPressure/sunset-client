// baritone.api.command.manager.CommandManager and registry.Registry: the commands, found by name,
// and the lines of chat that list them. Every command is a function of the instance, the game and
// its arguments; it either does its work or returns the sentence that says why not. The commands, their
// names and aliases, their descriptions and what they say come from Baritone 1.21.4's
// baritone.command.defaults.
use super::baritone::Baritone;
use super::baritone_argument::{Args, CommandError, CommandResult, Paginator};
pub use super::baritone_argument::{looks_relative, relative, relative_coordinate};
use super::baritone_cmd_build as build;
use super::baritone_cmd_meta as meta;
use super::baritone_cmd_nav as nav;
use super::baritone_cmd_work as work;
use super::baritone_goals::{rc, GoalBlock, GoalRef, GoalXZ, GoalYLevel};
use super::baritone_gworld::player_pos;
use super::baritone_settings as settings;
use super::baritone_utils::Pos;
use super::Game;
use std::panic::{catch_unwind, AssertUnwindSafe};
use horizon_client::text::{self, ClickEvent, Style, Text};

/// The command a clickable chat line runs.
pub const CLICK: &str = "@baritone";

pub type Run = fn(&mut Baritone, &mut Game<'_>, &mut Args) -> CommandResult;

pub struct Command {
    pub names: &'static [&'static str],
    pub short: &'static str,
    pub long: &'static [&'static str],
    /// A command that is only another one: what it runs (`set modified`).
    pub alias: &'static str,
    pub run: Run,
}

/// The run of an alias, which the manager never calls: it runs the target.
pub fn alias_run(_b: &mut Baritone, _game: &mut Game<'_>, _args: &mut Args) -> CommandResult {
    Ok(())
}

/// DefaultCommands.createAll's order.
const ORDER: &[&str] = &[
    "help", "set", "modified", "reset", "goal", "goto", "path", "proc", "eta", "version", "repack", "build",
    "litematica", "come", "axis", "forcecancel", "gc", "invert", "tunnel", "render", "farm", "follow", "pickup",
    "explorefilter", "reloadall", "saveall", "explore", "blacklist", "find", "mine", "click", "surface",
    "thisway", "waypoints", "sethome", "home", "sel", "elytra", "pause", "resume", "paused", "cancel",
];

/// Every command, in registration order.
pub fn registry() -> Vec<&'static Command> {
    let mut all: Vec<&'static Command> = nav::COMMANDS
        .iter()
        .chain(work::COMMANDS.iter())
        .chain(build::COMMANDS.iter())
        .chain(meta::COMMANDS.iter())
        .collect();
    all.sort_by_key(|c| ORDER.iter().position(|n| *n == c.names[0]).unwrap_or(usize::MAX));
    all
}

pub fn find(name: &str) -> Option<&'static Command> {
    registry().into_iter().find(|c| c.names.iter().any(|n| n.eq_ignore_ascii_case(name)))
}

/// `ExampleBaritoneControl.runCommand`: a line typed after the prefix (`goto 10 64 -20`). A setting's name is a command too
/// (`allowBreak` toggles a boolean, `pathCutoffFactor 0.4` sets), and the command is echoed to the chat when echoCommands says so.
pub fn run(b: &mut Baritone, game: &mut Game<'_>, line: &str) {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("damn") {
        b.core.say(game, "daniel");
        run_inner(b, game, line);
        return;
    }
    if trimmed.eq_ignore_ascii_case("orderpizza") {
        super::baritone_notification::open_uri("https://www.dominos.com/en/pages/order/");
        run_inner(b, game, line);
        return;
    }
    let (command, rest) = match trimmed.find(char::is_whitespace) {
        Some(i) => (&trimmed[..i], &trimmed[i..]),
        None => (trimmed, ""),
    };
    if !command.is_empty() {
        let after = Args::parse(rest);
        if let Some(def) = settings::find(command) {
            if !after.has_any() {
                log_ran_command(b, game, command, rest);
                let line = if def.kind == settings::Kind::Bool { format!("set toggle {}", def.name) } else { format!("set {}", def.name) };
                run_inner(b, game, &line);
                return;
            }
            if after.has_exactly(1) {
                log_ran_command(b, game, command, rest);
                let line = format!("set {} {}", def.name, after.peek().unwrap_or(""));
                run_inner(b, game, &line);
                return;
            }
        }
        // If the command exists, then handle echoing the input.
        if find(command).is_some() {
            log_ran_command(b, game, command, rest);
        }
    }
    run_inner(b, game, line);
}

/// `logRanCommand`: `> goto 1 2 3`, a link that runs it again (`> goto ...` when censorRanCommands is on).
fn log_ran_command(b: &mut Baritone, game: &mut Game<'_>, command: &str, rest: &str) {
    if !b.core.settings.flag("echoCommands") {
        return;
    }
    let message = format!("{command}{rest}");
    let shown = if b.core.settings.flag("censorRanCommands") { format!("{command} ...") } else { message.clone() };
    let line = clickable(&format!("> {shown}"), text::WHITE, ClickEvent::RunCommand(format!("{CLICK} {message}")), Text::literal("Click to rerun command"));
    b.core.say_text(game, line);
}

fn run_inner(b: &mut Baritone, game: &mut Game<'_>, line: &str) {
    let mut args = Args::parse(line);
    let Some(name) = args.optional_string() else {
        let _ = help(b, game, &mut Args::parse(""));
        return;
    };
    let Some(command) = find(&name) else {
        b.core.say_red(game, &format!("Command not found: {name}"));
        return;
    };
    args.forget_consumed();
    if !command.alias.is_empty() {
        let target = format!("{} {}", command.alias, args.rest_joined());
        return run_inner(b, game, target.trim());
    }
    let label = command.names[0];
    // CommandUnhandledException: a bug in a command costs the command, not the game.
    let Ok(outcome) = catch_unwind(AssertUnwindSafe(|| (command.run)(b, game, &mut args))) else {
        b.core.say_red(game, "An unhandled exception occurred. The error is in your game's log, please report this at https://github.com/luhnox/horizon-baritone/issues");
        return;
    };
    if let Err(CommandError(why)) = outcome {
        let _ = label;
        if b.core.settings.flag("verboseCommandExceptions") {
            // The Java prints the stack of the exception a bad argument raised; there is no stack here, so the game's log gets the reason.
            eprintln!("[Baritone] {label}: {why}");
        }
        b.core.say_red(game, &why);
    }
}

/// A line of several colours that is all one link: the click and the hover sit on every piece, because
/// the chat finds what is under the pointer by the piece's own style.
pub fn linked(parts: Vec<(String, u32)>, click: ClickEvent, hover: Text) -> Text {
    let mut out = Text::literal("");
    for (label, color) in parts {
        out = out.append(Text::styled(label, Style { click: Some(click.clone()), hover: Some(Box::new(hover.clone())), ..Style::color(color) }));
    }
    out
}

pub fn clickable(label: &str, color: u32, click: ClickEvent, hover: Text) -> Text {
    Text::styled(label, Style { click: Some(click), hover: Some(Box::new(hover)), ..Style::color(color) })
}

/// A click that runs a Baritone command.
pub fn run_event(command: &str) -> ClickEvent {
    ClickEvent::RunCommand(format!("{CLICK} {command}"))
}

/// Print a page of `entries`, then empty lines to fill it, then the pager. `prefix` is the command a
/// click on << or >> runs, without the page number.
pub fn paginate(
    b: &Baritone,
    game: &mut Game<'_>,
    args: &mut Args,
    entries: Vec<Text>,
    header: &str,
    prefix: &str,
) -> CommandResult {
    let pag = Paginator::new(entries.len());
    let page = pag.page_from(args)?;
    b.core.say(game, header);
    let (range, blank) = pag.range(page);
    for e in &entries[range] {
        b.core.say_text(game, e.clone());
    }
    for _ in 0..blank {
        b.core.say_text(game, Text::colored("--", text::DARK_GRAY));
    }
    let link = |label: &str, to: usize, ok: bool, hover: &str| {
        if ok {
            clickable(label, text::GRAY, run_event(&format!("{prefix} {to}")), Text::colored(hover, text::GRAY))
        } else {
            Text::colored(label, text::DARK_GRAY)
        }
    };
    let prev = link("<<", page.saturating_sub(1), pag.valid_page(page.saturating_sub(1)), "Click to view previous page");
    let next = link(">>", page + 1, pag.valid_page(page + 1), "Click to view next page");
    let pager = prev.append(Text::colored(" | ", text::GRAY)).append(next).append(Text::colored(format!(" {page}/{}", pag.max_page()), text::GRAY));
    b.core.say_text(game, pager);
    Ok(())
}

/// `help`, `help <page>`, `help <command>`.
pub fn help(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(1)?;
    if !args.has_any() || args.peek().is_some_and(|w| w.parse::<usize>().is_ok()) {
        // Newest registration first, as Registry.descendingStream does.
        let mut entries = Vec::new();
        for c in registry().into_iter().rev() {
            let hover = Text::colored(c.names.join("/"), text::WHITE)
                .append(Text::colored(format!("\n{}", c.short), text::GRAY))
                .append(Text::colored("\n\nClick to view full help", text::GRAY));
            let click = run_event(&format!("help {}", c.names[0]));
            entries.push(linked(vec![(c.names[0].to_owned(), text::GRAY), (format!(" - {}", c.short), text::DARK_GRAY)], click, hover));
        }
        return paginate(b, game, args, entries, "All Baritone commands (clickable):", "help");
    }
    let name = args.string("a command")?.to_ascii_lowercase();
    let c = find(&name).ok_or_else(|| CommandError::new(format!("Command not found: {name}")))?;
    let core = &b.core;
    core.say(game, &format!("{} - {}", c.names.join(" / "), c.short));
    core.say(game, "");
    if c.alias.is_empty() {
        for line in c.long {
            core.say(game, line);
        }
    } else {
        core.say(game, &format!("This command is an alias, for: {} ...", c.alias));
    }
    core.say(game, "");
    core.say_text(game, clickable("Click to return to the help menu", text::AQUA, run_event("help"), Text::colored("#help", text::GRAY)));
    Ok(())
}

// ---------------------------------------------------------------------------------------
// What the commands share.

/// Where the player is.
pub fn here(game: &Game<'_>) -> CommandResult<[f64; 3]> {
    player_pos(game).ok_or_else(|| CommandError::new("Not in a world."))
}

/// The block the player's feet are in.
pub fn feet(game: &Game<'_>) -> CommandResult<Pos> {
    super::baritone_gworld::player_feet(game).ok_or_else(|| CommandError::new("Not in a world."))
}

/// One block name (ForBlockOptionalMeta): `stone`, `minecraft:stone`, or `oak_stairs[facing=north]`.
pub fn block_word(b: &mut Baritone, game: &Game<'_>, args: &Args, datatype: &str, word: &str) -> CommandResult<String> {
    let bare = word.split('[').next().unwrap_or(word);
    b.core.block_named(game, bare).ok_or_else(|| args.invalid_datatype(datatype))
}

/// One BlockOptionalMeta (ForBlockOptionalMeta): the block, with the properties typed, and what it drops.
pub fn block_optional_meta(b: &mut Baritone, game: &Game<'_>, args: &Args, word: &str) -> CommandResult<super::baritone_bom::BlockOptionalMeta> {
    let mut resolve = |id: &str| b.core.block_named(game, id);
    super::baritone_scanner::make_bom(game, word, &mut resolve).map_err(|_| args.invalid_datatype("ForBlockOptionalMeta"))
}

/// RelativeGoal: three coordinates are a block, two a column, one a height; none, where you stand.
pub fn relative_goal(args: &mut Args, origin: Pos) -> CommandResult<GoalRef> {
    let mut words = Vec::new();
    while words.len() < 3 && args.peek().is_some_and(looks_relative) {
        words.push(args.optional_string().unwrap_or_default());
    }
    let n = |i: usize, o: i32, words: &[String]| relative(&words[i], o).ok_or_else(|| args.invalid_datatype("RelativeGoal"));
    match words.len() {
        0 => Ok(rc(GoalBlock(origin))),
        1 => Ok(rc(GoalYLevel(n(0, origin[1], &words)?))),
        2 => Ok(rc(GoalXZ(n(0, origin[0], &words)?, n(1, origin[2], &words)?))),
        _ => Ok(rc(GoalBlock([n(0, origin[0], &words)?, n(1, origin[1], &words)?, n(2, origin[2], &words)?]))),
    }
}

/// RelativeBlockPos: three coordinates.
pub fn relative_pos(args: &mut Args, origin: Pos) -> CommandResult<Pos> {
    let mut out = [0; 3];
    for i in 0..3 {
        let w = args.string("a coordinate")?;
        out[i] = relative(&w, origin[i]).ok_or_else(|| args.invalid_datatype("RelativeBlockPos"))?;
    }
    Ok(out)
}

/// Milliseconds since 1970.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The dimension the player is in, as a waypoint remembers it.
pub fn dimension(game: &Game<'_>) -> String {
    game.native_player().map_or("minecraft:overworld", |p| p.dimension).to_owned()
}
