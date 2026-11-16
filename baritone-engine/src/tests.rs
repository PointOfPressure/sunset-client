use crate::baritone_costs::*;
use crate::baritone_goals::*;
use crate::baritone_utils::*;

fn block(p: Pos) -> GoalRef {
    rc(GoalBlock(p))
}

// ---- costs and goals

#[test]
fn costs_match_baritones() {
    assert!((WALK_ONE_BLOCK_COST - 4.6328).abs() < 1e-3);
    assert!((SPRINT_ONE_BLOCK_COST - 3.5638).abs() < 1e-3);
    assert!(fall_cost(1) < fall_cost(2) && fall_cost(2) < fall_cost(20));
    assert!(fall_cost(0) == 0.0);
    // A jump up one block is dearer than a step, cheaper than a long fall.
    assert!(jump_one_block_cost() > 2.0 && jump_one_block_cost() < 5.0, "{}", jump_one_block_cost());
}

#[test]
fn goal_shapes() {
    assert!(GoalNear([5, 64, 5], 2).is_in_goal([4, 64, 4]));
    assert!(!GoalNear([5, 64, 5], 1).is_in_goal([3, 64, 5]));
    assert!(GoalXZ(3, 4).is_in_goal([3, 70, 4]));
    assert!(GoalYLevel(70).is_in_goal([9, 70, 9]));
    let any = GoalComposite(vec![block([10, 64, 0]), block([0, 64, 10])]);
    assert!(any.is_in_goal([10, 64, 0]) && any.is_in_goal([0, 64, 10]) && !any.is_in_goal([5, 64, 5]));
    assert!(any.heuristic([9, 64, 0]) < any.heuristic([5, 64, 5]));
}

#[test]
fn get_to_block_is_next_to_it_or_in_its_column() {
    let g = GoalGetToBlock([5, 64, 5]);
    assert!(g.is_in_goal([4, 64, 5]) && g.is_in_goal([5, 63, 5]) && g.is_in_goal([5, 62, 5]) && g.is_in_goal([5, 65, 5]));
    assert!(!g.is_in_goal([4, 64, 4]) && !g.is_in_goal([3, 64, 5]));
    assert!(g.heuristic([0, 64, 0]) > g.heuristic([4, 64, 5]));
}

#[test]
fn two_blocks_is_the_block_and_the_one_under_it() {
    let g = GoalTwoBlocks([2, 64, 2]);
    assert!(g.is_in_goal([2, 64, 2]) && g.is_in_goal([2, 63, 2]) && !g.is_in_goal([2, 65, 2]) && !g.is_in_goal([3, 64, 2]));
}

#[test]
fn inverted_run_away_axis_and_strict_direction() {
    let near = block([0, 64, 0]);
    let away = GoalInverted(near);
    assert!(!away.is_in_goal([50, 64, 0]));
    assert!(away.heuristic([1, 64, 0]) > away.heuristic([30, 64, 0]), "nearer the thing is worse");
    let run = GoalRunAway { from: vec![[0, 64, 0]], distance: 10, maintain_y: None };
    assert!(!run.is_in_goal([5, 64, 0]) && run.is_in_goal([10, 64, 0]) && run.is_in_goal([-8, 64, 8]));
    let axis = GoalAxis(120);
    assert!(axis.is_in_goal([0, 120, 9]) && axis.is_in_goal([7, 120, 7]) && axis.is_in_goal([-7, 120, 7]) && !axis.is_in_goal([3, 120, 9]) && !axis.is_in_goal([0, 64, 9]));
    let east = GoalStrictDirection { origin: [0, 64, 0], dx: 1, dz: 0 };
    assert!(east.heuristic([10, 64, 0]) < east.heuristic([0, 64, 0]) && east.heuristic([10, 64, 3]) > east.heuristic([10, 64, 0]));
}

#[test]
fn directions_and_yaw() {
    assert_eq!(facing_step(0.0), (0, 1));
    assert_eq!(facing_step(90.0), (-1, 0));
    assert_eq!(facing_step(180.0), (0, -1));
    assert_eq!(facing_step(-90.0), (1, 0));
}

// ---- search

// ---- settings, selection, schematics, arguments

mod extras {
    use crate::baritone_argument::*;
    use crate::baritone_schematic::*;
    use crate::baritone_selection::*;
    use crate::baritone_settings::*;
    use crate::baritone_utils::*;
    use crate::baritone_waypoints::{Tag, Waypoint, Waypoints};

    #[test]
    fn settings_have_defaults_parse_and_reset() {
        let mut s = Settings::load(|_| None);
        assert!(s.flag("allowBreak") && !s.flag("allowParkour") && s.int("maxFallHeightNoWater") == 3);
        assert_eq!(s.text("prefix"), "#");
        assert!(s.set("allowParkour", "on").is_ok() && s.flag("allowParkour"));
        assert!(s.set("allowParkour", "maybe").is_err());
        assert!(s.set("nope", "1").is_err());
        let (d, v) = s.set("colorGoalBox", "#00ff00").unwrap();
        assert_eq!((d.name, v.as_str()), ("colorGoalBox", "00FF00"));
        assert_eq!(s.color("colorGoalBox"), 0xFF00FF00);
        s.set("buildIgnoreBlocks", "minecraft:Dirt, stone").unwrap();
        assert_eq!(s.list("buildIgnoreBlocks"), vec!["dirt", "stone"]);
        let d = find("allowparkour").unwrap();
        assert!(s.modified(d));
        s.reset(d);
        assert!(!s.modified(d));
        // A stored value that no longer parses falls back to the default.
        let s = Settings::load(|n| (n == "maxFallHeightNoWater").then(|| "lots".to_owned()));
        assert_eq!(s.int("maxFallHeightNoWater"), 3);
    }

    #[test]
    fn every_setting_default_is_valid_and_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in DEFS {
            assert!(seen.insert(d.name), "{} twice", d.name);
            assert_eq!(parse(d.kind, d.default).as_deref(), Some(d.default), "{}", d.name);
        }
    }

    #[test]
    fn selections_expand_contract_and_shift_like_baritones() {
        let s = Selection::new([0, 64, 0], [4, 66, 2]);
        assert_eq!(s.size(), [5, 3, 3]);
        assert_eq!(s.volume(), 45);
        assert!(s.contains([2, 65, 1]) && !s.contains([5, 65, 1]));
        assert_eq!(s.expand([1, 0, 0], 2).max(), [6, 66, 2]);
        assert_eq!(s.expand([-1, 0, 0], 2).min(), [-2, 64, 0]);
        assert_eq!(s.expand([0, -1, 0], 1).min(), [0, 63, 0]);
        assert_eq!(s.contract([1, 0, 0], 1).min(), [1, 64, 0]);
        assert_eq!(s.contract([-1, 0, 0], 1).max(), [3, 66, 2]);
        assert_eq!(s.shift([0, 0, 1], 3).min(), [0, 64, 3]);
        let r = Selection::new([4, 66, 2], [0, 64, 0]);
        assert_eq!(r.expand([1, 0, 0], 2).max(), [6, 66, 2]);
        let mut m = SelectionManager::default();
        m.add([0, 64, 0], [4, 66, 2]);
        m.add([10, 64, 0], [11, 64, 0]);
        assert_eq!(m.last().unwrap().size(), [2, 1, 1]);
        assert_eq!(m.remove_last().unwrap().pos1, [10, 64, 0]);
        assert_eq!(m.remove_all().len(), 1);
        assert!(m.selections.is_empty());
        assert_eq!(TransformTarget::by_name("n").unwrap().indexes(3), vec![2]);
        assert_eq!(TransformTarget::by_name("oldest").unwrap().indexes(3), vec![0]);
        assert_eq!(TransformTarget::All.indexes(2), vec![0, 1]);
        assert!(TransformTarget::Newest.indexes(0).is_empty());
    }

    #[test]
    fn schematic_shapes() {
        let stone = BlockSpec::named("stone");
        let fill = FillSchematic { size: [3, 2, 3], spec: stone.clone() };
        assert_eq!(fill.desired([1, 1, 1], "minecraft:air"), Some(stone.clone()));
        let walls = ShapeSchematic { size: [5, 3, 5], shape: Shape::Walls, spec: stone.clone() };
        assert!(walls.desired([0, 1, 2], "").is_some() && walls.desired([2, 1, 2], "").is_none());
        let shell = ShapeSchematic { size: [5, 5, 5], shape: Shape::Shell, spec: stone.clone() };
        assert!(shell.desired([2, 0, 2], "").is_some() && shell.desired([2, 2, 2], "").is_none());
        let ball = ShapeSchematic { size: [9, 9, 9], shape: Shape::Sphere { hollow: false }, spec: stone.clone() };
        let shellball = ShapeSchematic { size: [9, 9, 9], shape: Shape::Sphere { hollow: true }, spec: stone.clone() };
        assert!(ball.desired([4, 4, 4], "").is_some() && shellball.desired([4, 4, 4], "").is_none());
        assert!(ball.desired([0, 0, 0], "").is_none(), "the corner is outside");
        assert!(shellball.desired([4, 0, 4], "").is_some() || shellball.desired([4, 1, 4], "").is_some());
        let tube = ShapeSchematic { size: [7, 4, 7], shape: Shape::Cylinder { hollow: true, axis: 1 }, spec: stone.clone() };
        // The Java's cylinder is one smaller than its box: of seven, the wall stands at 1 and 5.
        assert!(tube.desired([3, 2, 3], "").is_none() && tube.desired([1, 2, 3], "").is_some() && tube.desired([0, 2, 3], "").is_none());
        let swap = ReplaceSchematic { size: [2, 2, 2], from: vec!["dirt".into(), "grass_block".into()], to: BlockSpec::named("stone") };
        assert!(swap.desired([0, 0, 0], "minecraft:dirt").is_some() && swap.desired([0, 0, 0], "minecraft:sand").is_none());
    }

    #[test]
    fn block_specs_parse_and_match() {
        let b = BlockSpec::parse("oak_stairs[facing=north,half=bottom]");
        assert_eq!(b.name, "minecraft:oak_stairs");
        assert!(b.matches("minecraft:oak_stairs", "minecraft:oak_stairs[facing=north,half=bottom,shape=straight]"));
        assert!(!b.matches("minecraft:oak_stairs", "minecraft:oak_stairs[facing=south,half=bottom]"));
        assert!(BlockSpec::air().matches("minecraft:cave_air", ""));
    }

    #[test]
    fn the_feet_are_a_little_above_the_position_as_in_player_feet() {
        use crate::baritone_utils::{block_pos_of, feet_of};
        assert_eq!(feet_of([0.5, 64.0, -0.5]), [0, 64, -1]);
        assert_eq!(feet_of([0.5, 64.875, 0.5]), [0, 65, 0], "on soul sand, the block above it");
        assert_eq!(feet_of([0.5, 64.5, 0.5]), [0, 64, 0], "a slab: the world moves it up");
        assert_eq!(block_pos_of([0.5, 64.875, 0.5]), [0, 64, 0]);
    }

    #[test]
    fn an_argument_that_is_not_what_was_asked_for_reads_as_the_java_says_it() {
        use crate::baritone_argument::Args;
        let mut a = Args::parse("goto abc 12 x");
        assert_eq!(a.string("command").unwrap(), "goto");
        a.forget_consumed();
        assert_eq!(a.int("n").unwrap_err().0, "Error at argument #1: Expected Integer");
        assert_eq!(a.string("w").unwrap(), "abc");
        assert_eq!(a.int("n").unwrap(), 12);
        assert_eq!(a.invalid_type("an action").0, "Error at argument #2: Expected an action");
        assert_eq!(a.invalid_datatype("ForAxis").0, "Error at argument #3: Expected ForAxis", "a word still waiting is the one named");
        assert_eq!(a.string("w").unwrap(), "x");
        assert_eq!(a.invalid_datatype("ForAxis").0, "Error at argument #3: Expected ForAxis");
        assert_eq!(a.string("w").unwrap_err().0, "Not enough arguments (expected at least 1)");
        assert_eq!(Args::parse("").invalid_type("an action").0, "Error at argument #<unknown>: Expected an action");
        assert_eq!(a.invalid_type_got("a toggleable setting", "some other setting").0, "Error at argument #3: Expected a toggleable setting, but got some other setting instead");
    }

    #[test]
    fn a_coordinate_reads_as_the_java_reads_it() {
        use crate::baritone_argument::{looks_relative, relative, relative_coordinate};
        assert_eq!(relative("10", 5), Some(10));
        assert_eq!(relative("~", 5), Some(5));
        assert_eq!(relative("~-5", 5), Some(0));
        assert_eq!(relative("10k", 0), Some(10_000));
        assert_eq!(relative("-1.5K", 0), Some(-1500));
        assert_eq!(relative("2m", 0), Some(2_000_000));
        assert_eq!(relative("~1k", 500), Some(1500));
        assert_eq!(relative("+.5", 0), Some(0), "floored");
        assert_eq!(relative("-.5", 0), Some(-1), "floored, not cut");
        assert_eq!(relative_coordinate("1.", 0.0), Some(1.0));
        for bad in ["abc", "1k2", "k", "~k", ".", "1.2.3", "--1", "1e5", "inf", "NaN", "~~"] {
            assert!(!looks_relative(bad), "{bad} is not a coordinate");
        }
    }

    #[test]
    fn arguments() {
        let mut a = Args::parse("goto foo");
        assert_eq!(a.string("command").unwrap(), "goto");
        assert!(a.int("n").is_err());
        assert_eq!(a.string("foo").unwrap(), "foo");
        assert_eq!(a.string("more").unwrap_err().0, "Not enough arguments (expected at least 1)");
        let mut b = Args::parse("3 stone dirt");
        assert_eq!(b.try_int(), Some(3));
        assert_eq!(b.try_int(), None);
        assert_eq!(b.rest(), vec!["stone", "dirt"]);
        assert_eq!(b.one_of(&["Stone"], "block").unwrap(), "Stone");
    }

    #[test]
    fn waypoints_save_find_and_round_trip() {

        let mut w = Waypoints::default();
        let at = |tag: Tag, name: &str, x, created| Waypoint { tag, name: name.into(), pos: [x, 64, 0], dimension: "overworld".into(), created };
        w.add(at(Tag::Home, "base", 10, 1));
        w.add(at(Tag::Home, "", 20, 2));
        w.add(at(Tag::User, "mine", 5, 3));
        assert_eq!(w.in_dimension("overworld").len(), 3);
        assert!(w.in_dimension("nether").is_empty());
        assert_eq!(w.lookup("home", "overworld").len(), 2, "a tag picks every waypoint with it");
        assert_eq!(w.lookup("MINE", "overworld").len(), 1, "anything else is a name");
        assert_eq!(Tag::by_name("H"), Some(Tag::Home));
        assert_eq!(Tag::by_name("nothing"), None);
        let again = Waypoints::from_text(&w.to_text());
        assert_eq!(again.all, w.all);
        let gone = w.all[0].clone();
        w.remove(&gone);
        assert_eq!(w.all.len(), 2);
    }

    #[test]
    fn composite_schematics_lay_their_parts_in_one_box() {
        use std::rc::Rc;
        let stone: Rc<dyn Schematic> = Rc::new(FillSchematic { size: [2, 2, 2], spec: BlockSpec::named("stone") });
        let dirt: Rc<dyn Schematic> = Rc::new(FillSchematic { size: [2, 2, 2], spec: BlockSpec::named("dirt") });
        let both = CompositeSchematic { size: [5, 2, 2], parts: vec![([0, 0, 0], stone), ([3, 0, 0], dirt)], name: "two".into() };
        assert_eq!(both.desired([1, 1, 1], "").unwrap().name, "minecraft:stone");
        assert_eq!(both.desired([4, 0, 0], "").unwrap().name, "minecraft:dirt");
        assert!(both.desired([2, 0, 0], "").is_none(), "the gap between them is left alone");
    }


    #[test]
    fn pages_of_eight() {
        let p = Paginator::new(20);
        assert_eq!(p.max_page(), 3);
        assert!(p.valid_page(3) && !p.valid_page(4) && !p.valid_page(0));
        assert_eq!(p.range(1), (0..8, 0));
        assert_eq!(p.range(3), (16..20, 4), "four entries, four empty lines");
        assert_eq!(Paginator::new(0).max_page(), 1);
        assert_eq!(Paginator::new(0).range(1), (0..0, 8));
        assert_eq!(p.page_from(&mut Args::parse("")).unwrap(), 1);
        assert_eq!(p.page_from(&mut Args::parse("2")).unwrap(), 2);
        assert!(p.page_from(&mut Args::parse("9")).is_err());
        assert!(p.page_from(&mut Args::parse("1 2")).is_err());
    }

    #[test]
    fn argument_requirements() {
        let a = Args::parse("a b");
        assert!(a.require_max(2).is_ok() && a.require_max(1).is_err());
        assert!(a.require_min(2).is_ok() && a.require_min(3).is_err());
        assert!(a.require_exactly(2).is_ok() && a.require_exactly(1).is_err());
        assert!(a.has_exactly(2));
    }

    #[test]
    fn goals_print_the_way_baritone_does() {
        use crate::baritone_goals::*;
        assert_eq!(GoalBlock([1, 2, 3]).label(), "GoalBlock{x=1,y=2,z=3}");
        assert_eq!(GoalXZ(4, 5).label(), "GoalXZ{x=4,z=5}");
        assert_eq!(GoalYLevel(70).label(), "GoalYLevel{y=70}");
        assert_eq!(GoalNear([1, 2, 3], 4).label(), "GoalNear{x=1, y=2, z=3, rangeSq=16}");
        assert_eq!(GoalInverted(rc(GoalXZ(0, 0))).label(), "GoalInverted{GoalXZ{x=0,z=0}}");
        assert_eq!(pos_label([1, 2, 3]), "BetterBlockPos{x=1,y=2,z=3}");
        assert_eq!(direction_vector("EAST"), Some([1, 0, 0]));
        assert_eq!(column_ahead([0.5, 64.0, 0.5], 0.0, 10.0), (0, 10));
        assert_eq!(column_ahead([0.5, 64.0, 0.5], -90.0, 10.0), (10, 0));
    }
}
