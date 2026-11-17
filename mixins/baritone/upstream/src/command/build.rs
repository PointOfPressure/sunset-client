// baritone.command.defaults: build, litematica, tunnel and sel. Names, aliases, descriptions and
// messages are Baritone 1.21.4's (BuildCommand, LitematicaCommand, TunnelCommand, SelCommand).
use super::baritone::{Baritone, ID};
use super::baritone_argument::{Args, CommandError, CommandResult};
use super::baritone_command::{feet, relative_pos, Command};
use super::baritone_format as format;
use super::baritone_goals::{rc, GoalStrictDirection};
use super::baritone_schematic::{
    BlockSpec, CompositeSchematic, FillSchematic, ReplaceSchematic, Schematic, Shape, ShapeSchematic, StaticSchematic,
};
use super::baritone_selection::{Selection, TransformTarget};
use super::baritone_utils::{direction_vector, facing_step, pos_label, Pos};
use super::Game;
use std::{path::PathBuf, rc::Rc};

pub const COMMANDS: &[Command] = &[
    Command {
        names: &["build"],
        short: "Build a schematic",
        long: &[
            "Build a schematic from a file.",
            "",
            "Usage:",
            "> build <filename> - Loads and builds '<filename>.schematic'",
            "> build <filename> <x> <y> <z> - Custom position",
        ],
        alias: "",
        run: build,
    },
    Command {
        names: &["litematica"],
        short: "Builds the loaded schematic",
        long: &[
            "Build a schematic currently open in Litematica.",
            "",
            "Usage:",
            "> litematica - Build the first schematic placement",
            "> litematica <n> - Build the n-th schematic placement",
        ],
        alias: "",
        run: litematica,
    },
    Command {
        names: &["tunnel"],
        short: "Set a goal to tunnel in your current direction",
        long: &[
            "The tunnel command sets a goal that tells Baritone to mine completely straight in the direction that you're facing.",
            "",
            "Usage:",
            "> tunnel - No arguments, mines in a 1x2 radius.",
            "> tunnel <height> <width> <depth> - Tunnels in a user defined height, width and depth.",
        ],
        alias: "",
        run: tunnel,
    },
    Command {
        names: &["sel", "selection", "s"],
        short: "WorldEdit-like commands",
        long: &[
            "The sel command allows you to manipulate Baritone's selections, similarly to WorldEdit.",
            "",
            "Using these selections, you can clear areas, fill them with blocks, or something else.",
            "",
            "The expand/contract/shift commands use a kind of selector to choose which selections to target. Supported ones are a/all, n/newest, and o/oldest.",
            "",
            "Usage:",
            "> sel pos1/p1/1 - Set position 1 to your current position.",
            "> sel pos1/p1/1 <x> <y> <z> - Set position 1 to a relative position.",
            "> sel pos2/p2/2 - Set position 2 to your current position.",
            "> sel pos2/p2/2 <x> <y> <z> - Set position 2 to a relative position.",
            "",
            "> sel clear/c - Clear the selection.",
            "> sel undo/u - Undo the last action (setting positions, creating selections, etc.)",
            "> sel set/fill/s/f [block] - Completely fill all selections with a block.",
            "> sel walls/w [block] - Fill in the walls of the selection with a specified block.",
            "> sel shell/shl [block] - The same as walls, but fills in a ceiling and floor too.",
            "> sel sphere/sph [block] - Fills the selection with a sphere bounded by the sides.",
            "> sel hsphere/hsph [block] - The same as sphere, but hollow.",
            "> sel cylinder/cyl [block] <axis> - Fills the selection with a cylinder bounded by the sides, oriented about the given axis. (default=y)",
            "> sel hcylinder/hcyl [block] <axis> - The same as cylinder, but hollow.",
            "> sel cleararea/ca - Basically 'set air'.",
            "> sel replace/r <blocks...> <with> - Replaces blocks with another block.",
            "> sel copy/cp <x> <y> <z> - Copy the selected area relative to the specified or your position.",
            "> sel paste/p <x> <y> <z> - Build the copied area relative to the specified or your position.",
            "",
            "> sel expand <target> <direction> <blocks> - Expand the targets.",
            "> sel contract <target> <direction> <blocks> - Contract the targets.",
            "> sel shift <target> <direction> <blocks> - Shift the targets (does not resize).",
        ],
        alias: "",
        run: sel,
    },
];

/// The folder schematics are read from.
pub fn schematics_dir(game: &Game<'_>) -> PathBuf {
    game.native_data_dir(ID).join("schematics")
}

/// RelativeFile: a path as given if it is absolute, else inside the schematics folder.
pub fn relative_file(game: &Game<'_>, name: &str) -> PathBuf {
    let given = PathBuf::from(name);
    if given.is_absolute() { given } else { schematics_dir(game).join(name) }
}

/// A file named for `explorefilter`: as given, else beside the schematics.
pub fn resolve_file(game: &Game<'_>, name: &str) -> PathBuf {
    relative_file(game, name)
}

const FORMATS: &str = "schematic, schem, litematic, nbt";

fn build(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    let word = args.string("a file")?;
    let first = relative_file(game, &word);
    let mut file = first.clone();
    if file.extension().is_none() {
        let ext = b.core.settings.text("schematicFallbackExtension");
        file = PathBuf::from(format!("{}.{ext}", file.display()));
    }
    if !file.exists() {
        if first.exists() {
            return Err(CommandError::new(format!(
                "Cannot load {} because I do not know which schematic format that is. Please rename the file to include the correct file extension.",
                file.display()
            )));
        }
        return Err(CommandError::new(format!("Cannot find {}", file.display())));
    }
    let ext = file.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !["schematic", "schem", "litematic", "nbt"].contains(&ext.as_str()) {
        return Err(CommandError::new(format!("Unsupported schematic format. Reckognized file extensions are: {}", FORMATS.replace(", ", ", "))));
    }
    let origin = feet(game)?;
    let build_origin = if args.has_any() {
        args.require_max(3)?;
        relative_pos(args, origin)?
    } else {
        args.require_max(0)?;
        origin
    };
    let schematic = format::read(&file).map_err(|_| {
        CommandError::new("Couldn't load the schematic. Either your schematic is corrupt or this is a bug.")
    })?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Baritone { core, procs, .. } = b;
    procs.build.build_file(core, game, name, schematic, build_origin);
    b.core.say(game, &format!("Successfully loaded schematic for building\nOrigin: {}", pos_label(build_origin)));
    Ok(())
}

fn litematica(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(1)?;
    if args.has_any() {
        args.int("a schematic number")?;
    }
    b.core.say(game, "Litematica is not present");
    Ok(())
}

fn tunnel(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    args.require_max(3)?;
    let at = feet(game)?;
    let step = facing_step(game.yaw);
    if args.has_exactly(3) {
        let mut height = args.int("a height")?;
        let mut width = args.int("a width")?;
        let depth = args.int("a depth")?;
        if width < 1 || height < 2 || depth < 1 || height > 319 {
            b.core.say(game, "Width and depth must at least be 1 block; Height must at least be 2 blocks, and cannot be greater than the build limit.");
            return Ok(());
        }
        height -= 1;
        width -= 1;
        let addition = if width % 2 == 0 { 0 } else { 1 };
        let (c1, c2) = match step {
            (1, 0) => ([at[0], at[1], at[2] - width / 2], [at[0] + depth, at[1] + height, at[2] + width / 2 + addition]),
            (-1, 0) => ([at[0], at[1], at[2] + width / 2 + addition], [at[0] - depth, at[1] + height, at[2] - width / 2]),
            (0, -1) => ([at[0] - width / 2, at[1], at[2]], [at[0] + width / 2 + addition, at[1] + height, at[2] - depth]),
            _ => ([at[0] + width / 2 + addition, at[1], at[2]], [at[0] - width / 2, at[1] + height, at[2] + depth]),
        };
        b.core.say(game, &format!("Creating a tunnel {} block(s) high, {} block(s) wide, and {depth} block(s) deep", height + 1, width + 1));
        let Baritone { core, procs, .. } = b;
        procs.build.clear_area(core, game, c1, c2);
    } else {
        let goal = rc(GoalStrictDirection { origin: at, dx: step.0, dz: step.1 });
        let label = goal.label();
        b.procs.goal.set_goal_and_path(goal);
        b.core.say(game, &format!("Goal: {label}"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// sel

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Pos1,
    Pos2,
    Clear,
    Undo,
    Set,
    Walls,
    Shell,
    Sphere,
    HSphere,
    Cylinder,
    HCylinder,
    ClearArea,
    Replace,
    Expand,
    Copy,
    Paste,
    Contract,
    Shift,
}

const ACTIONS: &[(Action, &[&str])] = &[
    (Action::Pos1, &["pos1", "p1", "1"]),
    (Action::Pos2, &["pos2", "p2", "2"]),
    (Action::Clear, &["clear", "c"]),
    (Action::Undo, &["undo", "u"]),
    (Action::Set, &["set", "fill", "s", "f"]),
    (Action::Walls, &["walls", "w"]),
    (Action::Shell, &["shell", "shl"]),
    (Action::Sphere, &["sphere", "sph"]),
    (Action::HSphere, &["hsphere", "hsph"]),
    (Action::Cylinder, &["cylinder", "cyl"]),
    (Action::HCylinder, &["hcylinder", "hcyl"]),
    (Action::ClearArea, &["cleararea", "ca"]),
    (Action::Replace, &["replace", "r"]),
    (Action::Expand, &["expand", "ex"]),
    (Action::Copy, &["copy", "cp"]),
    (Action::Paste, &["paste", "p"]),
    (Action::Contract, &["contract", "ct"]),
    (Action::Shift, &["shift", "sh"]),
];

impl Action {
    fn by_name(word: &str) -> Option<Action> {
        ACTIONS.iter().find(|(_, names)| names.iter().any(|n| n.eq_ignore_ascii_case(word))).map(|(a, _)| *a)
    }

    fn is_fill(self) -> bool {
        matches!(
            self,
            Action::Set | Action::Walls | Action::Shell | Action::Sphere | Action::HSphere | Action::Cylinder | Action::HCylinder | Action::ClearArea | Action::Replace
        )
    }
}

/// ForBlockOptionalMeta: a block, with the properties that were typed.
fn block_spec(b: &mut Baritone, game: &Game<'_>, args: &mut Args) -> CommandResult<BlockSpec> {
    let word = args.string("a block")?;
    spec_of(b, game, args, &word)
}

fn spec_of(b: &mut Baritone, game: &Game<'_>, args: &Args, word: &str) -> CommandResult<BlockSpec> {
    let (bare, props) = match word.split_once('[') {
        Some((n, p)) => (n, format!("[{p}")),
        None => (word, String::new()),
    };
    let full = b
        .core
        .block_named(game, bare)
        .ok_or_else(|| args.invalid_datatype("ForBlockOptionalMeta"))?;
    Ok(BlockSpec::parse(&format!("{full}{props}")))
}

/// ForAxis.
fn axis_of(args: &Args, word: &str) -> CommandResult<usize> {
    match word.to_ascii_lowercase().as_str() {
        "x" => Ok(0),
        "y" => Ok(1),
        "z" => Ok(2),
        _ => Err(args.invalid_datatype("ForAxis")),
    }
}

/// ForDirection.
fn direction_of(args: &Args, word: &str) -> CommandResult<[i32; 3]> {
    direction_vector(word).ok_or_else(|| args.invalid_datatype("ForDirection"))
}

fn lowest(sels: &[Selection]) -> Pos {
    let mut origin = sels[0].min();
    for s in sels {
        let m = s.min();
        origin = [origin[0].min(m[0]), origin[1].min(m[1]), origin[2].min(m[2])];
    }
    origin
}

fn offset(a: Pos, b: Pos) -> Pos {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn sel(b: &mut Baritone, game: &mut Game<'_>, args: &mut Args) -> CommandResult {
    let word = args.string("an action")?;
    let action = Action::by_name(&word).ok_or_else(|| args.invalid_type("an action"))?;
    match action {
        Action::Pos1 | Action::Pos2 => {
            if action == Action::Pos2 && b.core.sel_pos1.is_none() {
                return Err(CommandError::new("Set pos1 first before using pos2"));
            }
            let here = feet(game)?;
            let pos = if args.has_any() { relative_pos(args, here)? } else { here };
            args.require_max(0)?;
            if action == Action::Pos1 {
                b.core.sel_pos1 = Some(pos);
                b.core.say(game, "Position 1 has been set");
            } else {
                let first = b.core.sel_pos1.take().unwrap_or(pos);
                b.core.selections.add(first, pos);
                b.core.say(game, "Selection added");
            }
        }
        Action::Clear => {
            args.require_max(0)?;
            b.core.sel_pos1 = None;
            let n = b.core.selections.remove_all().len();
            b.core.say(game, &format!("Removed {n} selections"));
        }
        Action::Undo => {
            args.require_max(0)?;
            if b.core.sel_pos1.is_some() {
                b.core.sel_pos1 = None;
                b.core.say(game, "Undid pos1");
            } else {
                match b.core.selections.remove_last() {
                    Some(s) => {
                        b.core.sel_pos1 = Some(s.pos1);
                        b.core.say(game, "Undid pos2");
                    }
                    None => return Err(CommandError::new("Nothing to undo!")),
                }
            }
        }
        a if a.is_fill() => {
            let mut spec = if a == Action::ClearArea { BlockSpec::air() } else { block_spec(b, game, args)? };
            let mut from: Vec<String> = Vec::new();
            let mut axis = 1;
            if a == Action::Replace {
                from.push(spec.name.clone());
                while args.has_at_least(2) {
                    from.push(block_spec(b, game, args)?.name);
                }
                spec = block_spec(b, game, args)?;
                args.require_max(0)?;
            } else if matches!(a, Action::Cylinder | Action::HCylinder) {
                args.require_max(1)?;
                if args.has_any() {
                    let word = args.string("an axis")?;
                    axis = axis_of(args, &word)?;
                }
            } else {
                args.require_max(0)?;
            }
            let sels = b.core.selections.selections.clone();
            if sels.is_empty() {
                return Err(CommandError::new("No selections"));
            }
            let origin = lowest(&sels);
            let mut parts: Vec<(Pos, Rc<dyn Schematic>)> = Vec::new();
            let mut hi = origin;
            for s in &sels {
                let size = s.size();
                let part: Rc<dyn Schematic> = match a {
                    Action::Walls => Rc::new(ShapeSchematic { size, shape: Shape::Walls, spec: spec.clone() }),
                    Action::Shell => Rc::new(ShapeSchematic { size, shape: Shape::Shell, spec: spec.clone() }),
                    Action::Sphere => Rc::new(ShapeSchematic { size, shape: Shape::Sphere { hollow: false }, spec: spec.clone() }),
                    Action::HSphere => Rc::new(ShapeSchematic { size, shape: Shape::Sphere { hollow: true }, spec: spec.clone() }),
                    Action::Cylinder => Rc::new(ShapeSchematic { size, shape: Shape::Cylinder { hollow: false, axis }, spec: spec.clone() }),
                    Action::HCylinder => Rc::new(ShapeSchematic { size, shape: Shape::Cylinder { hollow: true, axis }, spec: spec.clone() }),
                    Action::Replace => Rc::new(ReplaceSchematic { size, from: from.clone(), to: spec.clone() }),
                    _ => Rc::new(FillSchematic { size, spec: spec.clone() }),
                };
                let at = offset(s.min(), origin);
                let m = s.max();
                hi = [hi[0].max(m[0]), hi[1].max(m[1]), hi[2].max(m[2])];
                parts.push((at, part));
            }
            let size = [hi[0] - origin[0] + 1, hi[1] - origin[1] + 1, hi[2] - origin[2] + 1];
            let composite = CompositeSchematic { size, parts, name: "Fill".into() };
            let Baritone { core, procs, .. } = b;
            procs.build.build(core, game, "Fill".into(), Rc::new(composite), origin);
            b.core.say(game, "Filling now");
        }
        Action::Copy => {
            let here = feet(game)?;
            let pos = if args.has_any() { relative_pos(args, here)? } else { here };
            args.require_max(0)?;
            let sels = b.core.selections.selections.clone();
            if sels.is_empty() {
                return Err(CommandError::new("No selections"));
            }
            let origin = lowest(&sels);
            let mut parts: Vec<(Pos, Rc<dyn Schematic>)> = Vec::new();
            let mut hi = origin;
            for s in &sels {
                let size = s.size();
                let min = s.min();
                let mut copy = StaticSchematic { size, blocks: vec![None; s.volume() as usize], name: "copy".into() };
                for y in 0..size[1] {
                    for z in 0..size[2] {
                        for x in 0..size[0] {
                            let p = [min[0] + x, min[1] + y, min[2] + z];
                            if !super::baritone_gworld::loaded(game, p) {
                                continue;
                            }
                            let state = game.state_string(game.world.block(p[0], p[1], p[2]));
                            let at = copy.index([x, y, z]);
                            copy.blocks[at] = Some(BlockSpec::parse(&state));
                        }
                    }
                }
                let m = s.max();
                hi = [hi[0].max(m[0]), hi[1].max(m[1]), hi[2].max(m[2])];
                parts.push((offset(min, origin), Rc::new(copy)));
            }
            let size = [hi[0] - origin[0] + 1, hi[1] - origin[1] + 1, hi[2] - origin[2] + 1];
            b.core.clipboard = Some(Rc::new(CompositeSchematic { size, parts, name: "Paste".into() }));
            b.core.clipboard_offset = offset(origin, pos);
            b.core.say(game, "Selection copied");
        }
        Action::Paste => {
            let here = feet(game)?;
            let pos = if args.has_any() { relative_pos(args, here)? } else { here };
            args.require_max(0)?;
            let clip = b.core.clipboard.clone().ok_or_else(|| CommandError::new("You need to copy a selection first"))?;
            let off = b.core.clipboard_offset;
            let Baritone { core, procs, .. } = b;
            procs.build.build(core, game, "Paste".into(), clip, [pos[0] + off[0], pos[1] + off[1], pos[2] + off[2]]);
            b.core.say(game, "Building now");
        }
        Action::Expand | Action::Contract | Action::Shift => {
            args.require_exactly(3)?;
            let target = TransformTarget::by_name(&args.string("a target")?).ok_or_else(|| CommandError::new("Invalid transform type"))?;
            let word = args.string("a direction")?;
            let dir = direction_of(args, &word)?;
            let blocks = args.int("a number of blocks")?;
            let count = b.core.selections.selections.len();
            if count < 1 {
                return Err(CommandError::new("No selections found"));
            }
            let picked = target.indexes(count);
            for i in &picked {
                b.core.selections.transform(*i, |s| match action {
                    Action::Expand => s.expand(dir, blocks),
                    Action::Contract => s.contract(dir, blocks),
                    _ => s.shift(dir, blocks),
                });
            }
            b.core.say(game, &format!("Transformed {} selections", picked.len()));
        }
        _ => {}
    }
    Ok(())
}
