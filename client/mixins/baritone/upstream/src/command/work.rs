// baritone.command.defaults: the commands that start a job: mine, pickup, farm, follow, explore,
// explorefilter, blacklist, find, elytra. Names, aliases, descriptions and
// messages are Baritone 1.21.4's.
use super::baritone::Baritone;
use super::baritone_argument::{Args, CommandError, CommandResult};
use super::baritone_cmd_build::resolve_file;
use super::baritone_command::{block_word, clickable, dimension, feet, here, relative, run_event, Command, CLICK};
use super::baritone_goals::Goal;
use super::baritone_gworld::{bare, targets};
use super::baritone_p_explore::read_filter;
use super::baritone_p_follow::Filter;
use super::baritone_utils::pos_label;
use super::Game;
use horizon_client::text::{self, ClickEvent, Style, Text};

pub const COMMANDS: &[Command] = &[
    Command {
        names: &["mine"],
        short: "Mine some blocks",
        long: &[
            "The mine command allows you to tell Baritone to search for and mine individual blocks.",
            "",
            "The specified blocks can be ores, or any other block.",
            "",
            "Also see the legitMine settings (see #set l legitMine).",
            "",
            "Usage:",
            "> mine <blocks...> - Mine blocks.",
            "> mine <n> <blocks...> - Mine blocks until you have n of their drops.",
            "",
            "Examples:",
            "> mine diamond_ore deepslate_diamond_ore - Mines all diamonds it can find.",
            "> mine 64 diamond_ore - Mines diamond ore until you have 5 diamonds.",
        ],
        alias: "",
        run: mine,
    },
    Command {
        names: &["pickup"],
        short: "Pickup items",
        long: &["Usage:", "> pickup - Pickup anything", "> pickup <item1> <item2> <...> - Pickup certain items"],
        alias: "",
        run: pickup,
    },
    Command {
        names: &["farm"],
        short: "Farm nearby crops",
        long: &[
            "The farm command starts farming nearby plants. It harvests mature crops and plants new ones.",
            "",
            "Usage:",
            "> farm - farms every crop it can find.",
            "> farm <range> - farm crops within range from the starting position.",
            "> farm <range> <waypoint> - farm crops within range from waypoint.",
        ],
        alias: "",
        run: farm,
    },
    Command {
        names: &["follow"],
        short: "Follow entity things",
        long: &[
            "The follow command tells Baritone to follow certain kinds of entities.",
            "",
            "Usage:",
            "> follow entities - Follows all entities.",
            "> follow entity <entity1> <entity2> <...> - Follow certain entities (for example 'skeleton', 'horse' etc.)",
            "> follow players - Follow players",
            "> follow player <username1> <username2> <...> - Follow certain players",
        ],
        alias: "",
        run: follow,
    },
    Command {
        names: &["explore"],
        short: "Explore things",
        long: &[
            "Tell Baritone to explore chunks it does not have cached yet by distance.",
            "If you used explorefilter before this, it will be applied.",
            "",
            "Usage:",
            "> explore - Explore from your current position.",
            "> explore <x> <z> - Explore from the specified X and Z position.",
        ],
        alias: "",
        run: explore,
    },
    Command {
        names: &["explorefilter"],
        short: "Explore chunks from a json",
        long: &[
            "Apply an explore filter before using explore, which tells the explore process which chunks have been explored/not explored.",
            "",
            "The JSON file will follow this format: [{\"x\":0,\"z\":0},...]",
            "",
            "If 'invert' is specified, the chunks listed will be considered NOT explored, rather than explored.",
            "",
            "Usage:",
            "> explorefilter <path> [invert] - Load the JSON file referenced by the specified path. If invert is specified, it must be the literal word 'invert'.",
        ],
        alias: "",
        run: explore_filter,
    },
    Command {
        names: &["blacklist"],
        short: "Blacklist closest block",
        long: &[
            "While going to a block this command blacklists the closest block so that Baritone won't attempt to get to it.",
            "",
            "Usage:",
            "> blacklist",
        ],
        alias: "",
        run: blacklist,
    },
    Command {
        names: &["find"],
        short: "Find positions of a certain block",
        long: &[
            "The find command searches through Baritone's cache and attempts to find the location of the block.",
            "Tab completion will suggest only cached blocks and uncached blocks can not be found.",
            "",
            "Usage:",
            "> find <block> [...] - Try finding the listed blocks",
        ],
        alias: "",
        run: find,
    },
    Command {
        names: &["elytra"],
        short: "elytra time",
        long: &[
            "The elytra command tells baritone to, in the nether, automatically fly to the current goal.",
            "",
            "Usage:",
            "> elytra - fly to the current goal",
            "> elytra reset - Resets the state of the process, but will try to keep flying to the same goal.",
            "> elytra repack - Queues all of the chunks in render distance to be given to the native library.",
            "> elytra supported - Tells you if baritone ships a native library that is compatible with your PC.",
        ],
        alias: "",
        run: elytra,
    },
];

fn mine(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    here(game)?;
    let quantity = args.try_int().unwrap_or(0);
    args.require_min(1)?;
    let mut boms = Vec::new();
    while let Some(word) = args.optional_string() {
        boms.push(super::baritone_command::block_optional_meta(b, game, args, &word)?);
    }
    b.core.say(game, &format!("Mining [{}]", boms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")));
    let Baritone { core, procs, .. } = b;
    procs.mine.mine(core, game, quantity, Some(super::baritone_bom::BlockOptionalMetaLookup::new(boms)));
    Ok(())
}

fn pickup(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    here(game)?;
    let mut items: Vec<String> = Vec::new();
    while let Some(w) = args.optional_string() {
        let full = if w.contains(':') { w.to_ascii_lowercase() } else { format!("minecraft:{}", w.to_ascii_lowercase()) };
        if game.res.items().get(&full).is_none() {
            return Err(args.invalid_datatype("ItemById"));
        }
        if !items.contains(&full) {
            items.push(full);
        }
    }
    if items.is_empty() {
        b.core.say(game, "Picking up all items");
        b.procs.follow.pickup(Vec::new());
    } else {
        b.core.say(game, "Picking up these items:");
        for i in &items {
            b.core.say(game, i);
        }
        b.procs.follow.pickup(items.iter().map(|i| bare(i).to_owned()).collect());
    }
    Ok(())
}

fn farm(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(2)?;
    let mut range = 0;
    let mut origin = feet(game)?;
    if args.has_at_least(1) {
        range = args.int("a range")?;
    }
    if args.has_at_least(1) {
        let word = args.string("a waypoint")?;
        let dim = dimension(game);
        let found: Vec<_> = b.core.waypoints.lookup(&word, &dim).into_iter().map(|w| w.pos).collect();
        origin = match found.as_slice() {
            [] => return Err(CommandError::new("No waypoints found")),
            [one] => *one,
            _ => return Err(CommandError::new("Multiple waypoints were found")),
        };
    }
    b.procs.farm.farm(range, origin);
    b.core.say(game, "Farming");
    Ok(())
}

fn follow(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    here(game)?;
    args.require_min(1)?;
    if args.has_exactly(1) {
        let group = args.one_of(&["entities", "players"], "FollowGroup")?;
        b.procs.follow.follow(if group == "entities" { Filter::Entities } else { Filter::Players });
        b.core.say(game, &format!("Following all {group}"));
        return Ok(());
    }
    args.require_min(2)?;
    let list = args.one_of(&["entity", "player"], "FollowList")?;
    if list == "entity" {
        let mut kinds = Vec::new();
        while let Some(w) = args.optional_string() {
            let id = if w.contains(':') { w.to_ascii_lowercase() } else { format!("minecraft:{}", w.to_ascii_lowercase()) };
            if !game.res.mob_types().contains_key(&id) {
                return Err(args.invalid_datatype("EntityClassById"));
            }
            kinds.push(id);
        }
        b.core.say(game, "Following these types of entities:");
        for k in &kinds {
            b.core.say(game, k);
        }
        b.procs.follow.follow(Filter::Kinds(kinds));
    } else {
        // Players, by name: the ones in range now.
        let words = args.rest();
        while args.optional_string().is_some() {}
        let here_players: Vec<String> = targets(game).into_iter().filter(|t| t.kind == "minecraft:player").filter_map(|t| t.name).collect();
        let named: Vec<String> = words.iter().filter(|w| here_players.iter().any(|p| p.eq_ignore_ascii_case(w))).cloned().collect();
        if named.is_empty() {
            return Err(CommandError::new("No valid entities in range!"));
        }
        b.core.say(game, "Following these entities:");
        for n in &named {
            b.core.say(game, n);
        }
        b.procs.follow.follow(Filter::Named(named));
    }
    Ok(())
}

fn explore(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    if args.has_any() {
        args.require_exactly(2)?;
    } else {
        args.require_max(0)?;
    }
    let at = feet(game)?;
    let (x, z) = if args.has_any() {
        let wx = args.string("x")?;
        let x = relative(&wx, at[0]).ok_or_else(|| args.invalid_datatype("RelativeGoalXZ"))?;
        let wz = args.string("z")?;
        (x, relative(&wz, at[2]).ok_or_else(|| args.invalid_datatype("RelativeGoalXZ"))?)
    } else {
        (at[0], at[2])
    };
    b.procs.explore.explore(x, z);
    b.core.say(game, &format!("Exploring from {}", super::baritone_goals::GoalXZ(x, z).label()));
    Ok(())
}

fn explore_filter(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(2)?;
    let file = args.string("a file")?;
    let mut invert = false;
    if args.has_any() {
        let word = args.string("invert")?;
        if word.eq_ignore_ascii_case("invert") {
            invert = true;
        } else {
            return Err(args.invalid_type("either \"invert\" or nothing"));
        }
    }
    let chunks = read_filter(&resolve_file(game, &file)).map_err(CommandError)?;
    b.procs.explore.set_filter(chunks, invert);
    b.core.say(game, &format!("Explore filter applied. Inverted: {invert}"));
    Ok(())
}

fn blacklist(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(0)?;
    if !b.procs.get_to_block.running() {
        return Err(CommandError::new("GetToBlockProcess is not currently active"));
    }
    let Baritone { core, procs, .. } = b;
    if procs.get_to_block.blacklist_closest(core, game) {
        b.core.say(game, "Blacklisted closest instances");
        Ok(())
    } else {
        Err(CommandError::new("No known locations, unable to blacklist"))
    }
}

fn find(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_min(1)?;
    let at = feet(game)?;
    let mut found = Vec::new();
    while let Some(w) = args.optional_string() {
        let name = block_word(b, game, args, "BlockById", &w)?;
        found.extend(b.core.cached_locations_of(name.trim_start_matches("minecraft:"), usize::MAX, at, 4));
    }
    if found.is_empty() {
        b.core.say(game, "No positions known, are you sure the blocks are cached?");
        return Ok(());
    }
    for p in found {
        let text = format!("{} {} {}", p[0], p[1], p[2]);
        let line = clickable(&pos_label(p), text::GRAY, run_event(&format!("goal {text}")), Text::colored("Click to set goal to this position", text::GRAY));
        b.core.say_plain(game, line);
    }
    Ok(())
}

const OLD_2B2T_SEED: i64 = -4_100_785_268_875_389_365;
const NEW_2B2T_SEED: i64 = 146_008_555_100_680;

fn on_2b2t(game: &Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|r| r.session.address.to_lowercase().contains("2b2t.org"))
}

/// A piece of a chat component with a style: the Java's `MutableComponent` with `setStyle`.
fn piece(text: impl Into<String>, style: Style) -> Text {
    Text::styled(text, style)
}

/// The text a hover shows: the commands that a setting suggestion would run.
fn hover(text: String) -> Option<Box<Text>> {
    Some(Box::new(Text::literal(text)))
}

/// `ElytraCommand.suggest2b2tSeeds`: the advice, with the two seeds as links that set them.
fn suggest_2b2t_seeds(prefix: &str) -> Text {
    let link = |label: &str, seed: i64| {
        piece(
            label,
            Style {
                underlined: Some(true),
                bold: Some(true),
                hover: hover(format!("{prefix}set elytraNetherSeed {seed}")),
                click: Some(ClickEvent::RunCommand(format!("{CLICK} set elytraNetherSeed {seed}"))),
                ..Style::default()
            },
        )
    };
    Text::literal("Within a few hundred blocks of spawn/axis/highways/etc, the terrain is too fragmented to be predictable. Baritone Elytra will still work, just with backtracking. ")
        .append(Text::literal("However, once you get more than a few thousand blocks out, you should try "))
        .append(link("the older seed (click here)", OLD_2B2T_SEED))
        .append(Text::literal(". Once you're further out into newer terrain generation (this includes everything up through 1.12), you should try "))
        .append(link("the newer seed (click here)", NEW_2B2T_SEED))
        .append(Text::literal(". Once you get into 1.19 terrain, the terrain becomes unpredictable again, due to custom non-vanilla generation, and you should set #elytraPredictTerrain to false. "))
}

/// `ElytraCommand.gatekeep`: what the first #elytra says until elytraTermsAccepted is on, as the one message the Java makes of it.
fn elytra_gatekeep(b: &mut Baritone, game: &mut Game<'_>) {
    let prefix = b.core.settings.text("prefix");
    let red = || Style { color: Some(text::RED), underlined: Some(true), bold: Some(true), ..Style::default() };
    let mut message = Text::literal("To disable this message, enable the setting elytraTermsAccepted\n")
        .append(Text::literal("Baritone Elytra is an experimental feature. It is intended for long distance travel in the Nether but will also work in the Overworld, using fireworks for vanilla boost. It will not work with any other mods (\"hacks\") for non-vanilla boost. "))
        .append(piece(
            "If you want Baritone to attempt to take off from the ground for you, you can enable the elytraAutoJump setting (not advisable on laggy servers!). ",
            Style { hover: hover(format!("{prefix}set elytraAutoJump true")), ..Style::default() },
        ))
        .append(piece(
            "If you want Baritone to go slower, enable the elytraConserveFireworks setting and/or decrease the elytraFireworkSpeed setting. ",
            Style {
                hover: hover(format!("{prefix}set elytraConserveFireworks true\n{prefix}set elytraFireworkSpeed 0.6\n(the 0.6 number is just an example, tweak to your liking)")),
                ..Style::default()
            },
        ))
        .append(
            Text::literal("Baritone Elytra for use in the ")
                .append(piece("Nether", red()))
                .append(Text::literal(", "))
                .append(piece("wants to know the seed", red()))
                .append(Text::literal(" of the world you are in. If it doesn't have the correct seed, it will frequently backtrack. It uses the seed to generate terrain far beyond what you can see, since terrain obstacles in the Nether can be much larger than your render distance. ")),
        )
        .append(Text::literal("\n"));
    let seed = b.core.settings.long("elytraNetherSeed");
    let predict = b.core.settings.flag("elytraPredictTerrain");
    if on_2b2t(game) {
        let mut on = Text::literal("It looks like you're on 2b2t. Terrain prediction can be used but new nether terrain can not be predicted on 2b2t. ").append(suggest_2b2t_seeds(&prefix));
        if !predict {
            on = on.append(Text::literal(format!("{prefix}elytraPredictTerrain is currently disabled. ")));
        } else if seed == NEW_2B2T_SEED {
            on = on.append(Text::literal("You are using the newer seed. "));
        } else if seed == OLD_2B2T_SEED {
            on = on.append(Text::literal("You are using the older seed. "));
        } else {
            on = on.append(Text::literal("Defaulting to the newer seed. "));
            let _ = b.core.settings.set("elytraNetherSeed", &NEW_2B2T_SEED.to_string());
        }
        message = message.append(on);
    } else if seed == NEW_2B2T_SEED {
        message = message.append(Text::literal(format!("Baritone doesn't know the seed of your world. Set it with: {prefix}set elytraNetherSeed seedgoeshere\nFor the time being, elytraPredictTerrain is defaulting to false since the seed is unknown.")));
        let _ = b.core.settings.set("elytraPredictTerrain", "false");
    } else if predict {
        message = message.append(Text::literal(format!("Baritone Elytra is predicting terrain assuming that {seed} is the correct seed. Change that with {prefix}set elytraNetherSeed seedgoeshere, or disable it with {prefix}set elytraPredictTerrain false")));
    } else {
        message = message.append(Text::literal(format!("Baritone Elytra is not predicting terrain. If you don't know the seed, this is the correct thing to do. If you do know the seed, input it with {prefix}set elytraNetherSeed seedgoeshere, and then enable it with {prefix}set elytraPredictTerrain true")));
    }
    b.core.say_text(game, message);
}

fn elytra(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    if args.has_exactly(1) && args.peek() == Some("supported") {
        b.core.say(game, "yes");
        return Ok(());
    }
    if !args.has_any() {
        if b.core.settings.flag("elytraTermsAccepted") {
            if on_2b2t(game) && b.core.settings.flag("elytraPredictTerrain") {
                let seed = b.core.settings.long("elytraNetherSeed");
                if seed != NEW_2B2T_SEED && seed != OLD_2B2T_SEED {
                    b.core.say_text(game, Text::literal("It looks like you're on 2b2t, but elytraNetherSeed is incorrect."));
                    let prefix = b.core.settings.text("prefix");
                    b.core.say_text(game, suggest_2b2t_seeds(&prefix));
                }
            }
        } else {
            elytra_gatekeep(b, game);
        }
        let goal = b.procs.goal.most_recent_goal().cloned().ok_or_else(|| CommandError::new("No goal has been set"))?;
        here(game)?;
        let Baritone { core, procs, .. } = b;
        return procs.elytra.path_to(core, game, &goal).map_err(CommandError::new);
    }
    let action = args.string("an action")?;
    match action.as_str() {
        "reset" => {
            here(game)?;
            let Baritone { core, procs, .. } = b;
            procs.elytra.reset_state(core, game).map_err(CommandError::new)?;
            b.core.say(game, "Reset state but still flying to same goal");
        }
        "repack" => {
            b.procs.elytra.repack(game);
            b.core.say(game, "Queued all loaded chunks for repacking");
        }
        _ => return Err(CommandError::new("Invalid action")),
    }
    Ok(())
}
