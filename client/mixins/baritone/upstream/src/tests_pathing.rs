// Tests of the pathfinder ported from Baritone's Java (BlockState, CalculationContext, MovementHelper,
// the eight movements, Moves, AStarPathFinder, Path), on a world held in a map.
use crate::baritone_astar::{AStarPathFinder, Status};
use crate::baritone_block_state::{parse_props, BlockState, Class, FluidKind};
use crate::baritone_block_state_interface::BlockStateInterface;
use crate::baritone_calculation_context::{BareHands, CalculationContext, PlayerFacts};
use crate::baritone_costs::*;
use crate::baritone_goals::{rc, GoalBlock};
use crate::baritone_moves::Moves;
use crate::baritone_settings::Settings;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

fn state(name: &str, classes: &[&str], full: bool, hardness: f32) -> Rc<BlockState> {
    Rc::new(BlockState {
        id: 0,
        name: Arc::from(format!("minecraft:{name}")),
        classes: Class::flags_of(classes.iter().copied()),
        props: parse_props(name),
        fluid: FluidKind::None,
        fluid_amount: 0,
        hardness,
        replaceable: name == "air",
        full_collision: full,
        sturdy_top: full,
        pathfindable: !full,
    })
}

struct World {
    blocks: HashMap<(i32, i32, i32), Rc<BlockState>>,
    air: Rc<BlockState>,
    stone: Rc<BlockState>,
}

impl World {
    fn new() -> Self {
        Self { blocks: HashMap::new(), air: state("air", &["AirBlock"], false, 0.0), stone: state("stone", &[], true, 1.5) }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, s: &Rc<BlockState>) {
        self.blocks.insert((x, y, z), s.clone());
    }

    /// A stone floor at y = 63 (feet at 64) from x0 to x1 and z from -3 to 3.
    fn floor(&mut self, x0: i32, x1: i32) {
        let stone = self.stone.clone();
        for x in x0..=x1 {
            for z in -3..=3 {
                self.set(x, 63, z, &stone);
            }
        }
    }
}

impl BlockStateInterface for World {
    fn get0(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        self.blocks.get(&(x, y, z)).cloned().unwrap_or_else(|| self.air.clone())
    }
    fn is_loaded(&self, _x: i32, _z: i32) -> bool {
        true
    }
    fn min_y(&self) -> i32 {
        0
    }
    fn max_y(&self) -> i32 {
        256
    }
}

fn settings() -> Settings {
    Settings::load(|_| None)
}

fn ctx<'a>(s: &Settings, w: &'a World, tools: &'a BareHands, facts: &PlayerFacts) -> CalculationContext<'a> {
    CalculationContext::new(s, w, tools, facts)
}

#[test]
fn a_walk_on_a_flat_floor_is_a_sprinted_block() {
    let mut w = World::new();
    w.floor(0, 10);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let cost = Moves::TraverseEast.cost(&c, 0, 64, 0);
    assert!((cost - WALK_ONE_BLOCK_COST * SPRINT_MULTIPLIER).abs() < 1e-9, "{cost}");
}

#[test]
fn without_food_to_sprint_a_walk_is_a_walked_block() {
    let mut w = World::new();
    w.floor(0, 10);
    let s = settings();
    let facts = PlayerFacts { can_sprint: false, ..PlayerFacts::default() };
    let c = ctx(&s, &w, &BareHands, &facts);
    assert!((Moves::TraverseEast.cost(&c, 0, 64, 0) - WALK_ONE_BLOCK_COST).abs() < 1e-9);
}

#[test]
fn an_ascend_costs_a_jump_and_its_penalty() {
    let mut w = World::new();
    w.floor(0, 10);
    let stone = w.stone.clone();
    w.set(1, 64, 0, &stone);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let cost = Moves::AscendEast.cost(&c, 0, 64, 0);
    let expected = jump_one_block_cost().max(WALK_ONE_BLOCK_COST) + s.float("jumpPenalty");
    assert!((cost - expected).abs() < 1e-9, "{cost} {expected}");
}

#[test]
fn a_one_block_drop_is_a_descend() {
    let mut w = World::new();
    w.floor(0, 10);
    let stone = w.stone.clone();
    // A ledge at x = 0 (stone at y 64, so the feet are at 65); the floor at x = 1 is one lower.
    w.set(0, 64, 0, &stone);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let mut res = crate::baritone_calculation_context::MutableMoveResult::default();
    Moves::DescendEast.apply(&c, 0, 65, 0, &mut res);
    assert_eq!((res.x, res.y, res.z), (1, 64, 0));
    let expected = WALK_OFF_BLOCK_COST + fall_cost(1).max(CENTER_AFTER_FALL_COST);
    assert!((res.cost - expected).abs() < 1e-9, "{} {}", res.cost, expected);
}

#[test]
fn a_bridge_costs_a_placed_block() {
    let mut w = World::new();
    w.floor(0, 0);
    let stone = w.stone.clone();
    w.set(2, 63, 0, &stone);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    // x = 1 has no floor: a block goes there, against the floor of x = 2 (a side place, no sneaking).
    let cost = Moves::TraverseEast.cost(&c, 0, 64, 0);
    let expected = WALK_ONE_BLOCK_COST + s.float("blockPlacementPenalty");
    assert!((cost - expected).abs() < 1e-9, "{cost} {expected}");
}

#[test]
fn a_bridge_with_nothing_beside_it_is_a_backplace_and_sneaks() {
    let mut w = World::new();
    w.floor(0, 0);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    // Nothing at x = 2: the only block to place against is the one stood on.
    let cost = Moves::TraverseEast.cost(&c, 0, 64, 0);
    let expected = SNEAK_ONE_BLOCK_COST + s.float("blockPlacementPenalty");
    assert!((cost - expected).abs() < 1e-9, "{cost} {expected}");
}

#[test]
fn nothing_to_place_means_no_bridge() {
    let mut w = World::new();
    w.floor(0, 0);
    let s = settings();
    let facts = PlayerFacts { has_throwaway: false, ..PlayerFacts::default() };
    let c = ctx(&s, &w, &BareHands, &facts);
    assert!(Moves::TraverseEast.cost(&c, 0, 64, 0) >= COST_INF);
}

#[test]
fn a_star_walks_a_straight_line_across_a_floor() {
    let mut w = World::new();
    w.floor(-2, 20);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let mut finder = AStarPathFinder::new([0, 64, 0], [0, 64, 0], rc(GoalBlock([10, 64, 0])), HashMap::new(), 1000, true);
    let path = loop {
        match finder.run(&c, 500) {
            Status::Running => {}
            Status::Done(p) => break p.post_process(&c),
            Status::Exhausted => panic!("no path"),
        }
    };
    assert_eq!(path.dest(), [10, 64, 0]);
    assert_eq!(path.length(), 11);
    let total: f64 = path.movements.iter().map(|m| m.known_cost()).sum();
    assert!((total - 10.0 * WALK_ONE_BLOCK_COST * SPRINT_MULTIPLIER).abs() < 1e-6, "{total}");
}

#[test]
fn a_star_cuts_a_corner_with_a_diagonal() {
    let mut w = World::new();
    w.floor(-2, 20);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let mut finder = AStarPathFinder::new([0, 64, 0], [0, 64, 0], rc(GoalBlock([3, 64, 3])), HashMap::new(), 1000, true);
    let path = loop {
        match finder.run(&c, 500) {
            Status::Running => {}
            Status::Done(p) => break p.post_process(&c),
            Status::Exhausted => panic!("no path"),
        }
    };
    assert_eq!(path.length(), 4); // three diagonals
}

#[test]
fn a_star_bridges_a_gap_when_it_has_blocks() {
    let mut w = World::new();
    w.floor(-2, 2);
    w.floor(6, 12);
    let s = settings();
    let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
    let mut finder = AStarPathFinder::new([0, 64, 0], [0, 64, 0], rc(GoalBlock([8, 64, 0])), HashMap::new(), 1000, true);
    let mut result = None;
    for _ in 0..200 {
        match finder.run(&c, 500) {
            Status::Running => {}
            Status::Done(p) => {
                result = Some(p.post_process(&c));
                break;
            }
            Status::Exhausted => break,
        }
    }
    let path = result.expect("a path over the gap");
    assert_eq!(path.dest(), [8, 64, 0]);
}

#[test]
fn a_star_finds_nothing_across_a_gap_with_no_blocks_and_no_parkour() {
    let mut w = World::new();
    w.floor(-2, 2);
    w.floor(30, 40);
    let s = settings();
    let facts = PlayerFacts { has_throwaway: false, ..PlayerFacts::default() };
    let c = ctx(&s, &w, &BareHands, &facts);
    let mut finder = AStarPathFinder::new([0, 64, 0], [0, 64, 0], rc(GoalBlock([35, 64, 0])), HashMap::new(), 1000, true);
    loop {
        match finder.run(&c, 5000) {
            Status::Running => {}
            Status::Done(_) => panic!("crossed a gap it cannot"),
            Status::Exhausted => break,
        }
    }
}

#[test]
fn a_star_runs_in_a_small_stack() {
    // The game's main thread has a small stack: a search must not need a large one.
    std::thread::Builder::new()
        .stack_size(200 * 1024)
        .spawn(|| {
            let mut w = World::new();
            w.floor(-2, 40);
            let s = settings();
            let c = ctx(&s, &w, &BareHands, &PlayerFacts::default());
            let mut finder = AStarPathFinder::new([0, 64, 0], [0, 64, 0], rc(GoalBlock([30, 64, 2])), HashMap::new(), 1000, true);
            let path = loop {
                match finder.run(&c, 500) {
                    Status::Running => {}
                    Status::Done(p) => break p.post_process(&c),
                    Status::Exhausted => panic!("no path"),
                }
            };
            assert_eq!(path.dest(), [30, 64, 2]);
        })
        .unwrap()
        .join()
        .unwrap();
}

// ---- the whole behavior against a player on paper

mod paper {
    use super::*;
    use crate::baritone_movement_state::Input;
    use crate::baritone_pathing_behavior::{PathingBehavior, PathingCommand};
    use crate::baritone_player_context::PlayerContext;
    use crate::baritone_rotation::Rotation;
    use crate::baritone_utils::Pos;
    use std::cell::{Cell, RefCell};

    pub struct Paper<'w> {
        pub world: &'w World,
        pub pos: Cell<[f64; 3]>,
        pub yaw: Cell<f32>,
        pub held: RefCell<Vec<Input>>,
    }

    impl<'w> Paper<'w> {
        pub fn new(world: &'w World, pos: [f64; 3]) -> Self {
            Self { world, pos: Cell::new(pos), yaw: Cell::new(0.0), held: RefCell::new(Vec::new()) }
        }

        /// One tick of walking on flat ground: the keys held move the player along the yaw.
        pub fn physics(&self) {
            let held = self.held.borrow().clone();
            let mut p = self.pos.get();
            if held.contains(&Input::MoveForward) {
                let speed = if held.contains(&Input::Sprint) { 0.28 } else { 0.21 };
                let yaw = f64::from(self.yaw.get()).to_radians();
                p[0] += -yaw.sin() * speed;
                p[2] += yaw.cos() * speed;
            }
            // Stepping up onto a block in the way (a jump, collapsed into one tick).
            let feet = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
            if self.world.get0(feet[0], feet[1], feet[2]).full_collision {
                p[1] = f64::from(feet[1] + 1);
            }
            if !self.world.get0(feet[0], feet[1] - 1, feet[2]).full_collision && !self.world.get0(feet[0], feet[1], feet[2]).full_collision {
                p[1] = f64::from(feet[1] - 1) + 0.0;
            }
            self.pos.set(p);
        }
    }

    impl PlayerContext for Paper<'_> {
        fn bsi(&self) -> &dyn BlockStateInterface {
            self.world
        }
        fn player_feet(&self) -> Pos {
            let p = self.pos.get();
            [p[0].floor() as i32, (p[1] + 0.0001).floor() as i32, p[2].floor() as i32]
        }
        fn position(&self) -> [f64; 3] {
            self.pos.get()
        }
        fn player_head(&self) -> [f64; 3] {
            let p = self.pos.get();
            [p[0], p[1] + 1.62, p[2]]
        }
        fn delta_movement(&self) -> [f64; 3] {
            [0.0; 3]
        }
        fn player_rotations(&self) -> Rotation {
            Rotation::new(self.yaw.get(), 0.0)
        }
        fn on_ground(&self) -> bool {
            true
        }
        fn is_in_water(&self) -> bool {
            false
        }
        fn is_swimming(&self) -> bool {
            false
        }
        fn is_under_water(&self) -> bool {
            false
        }
        fn is_crouching(&self) -> bool {
            false
        }
        fn horizontal_collision(&self) -> bool {
            false
        }
        fn is_in_wall(&self) -> bool {
            false
        }
        fn food_level(&self) -> i32 {
            20
        }
        fn air_supply(&self) -> i32 {
            300
        }
        fn max_air_supply(&self) -> i32 {
            300
        }
        fn water_fluid_height(&self) -> f64 {
            0.0
        }
        fn set_sprinting(&self, _on: bool) {}
        fn is_nether(&self) -> bool {
            false
        }
        fn is_sprinting(&self) -> bool {
            self.held.borrow().contains(&Input::Sprint)
        }
        fn is_in_lava(&self) -> bool {
            false
        }
        fn on_climbable(&self) -> bool {
            false
        }
        fn has_unsupported_motion_effects(&self) -> bool {
            false
        }
        fn movement_speed(&self) -> f64 {
            0.13
        }
        fn is_looking_at(&self, _pos: Pos) -> bool {
            false
        }
        fn selected_block(&self) -> Option<Pos> {
            None
        }
        fn selected_face(&self) -> Option<[i32; 3]> {
            None
        }
        fn block_reach_distance(&self) -> f64 {
            4.5
        }
        fn reachable(&self, _pos: Pos, _reach: f64, _would_sneak: bool) -> Option<Rotation> {
            None
        }
        fn reachable_offset(&self, _pos: Pos, _offset: [f64; 3], _reach: f64, _would_sneak: bool) -> Option<Rotation> {
            None
        }
        fn select_item(&self, _select: bool, _wanted: &dyn Fn(&str) -> bool) -> bool {
            false
        }
        fn ray_trace_towards(&self, _rotation: Rotation, _reach: f64, _would_sneak: bool) -> Option<(Pos, [i32; 3])> {
            None
        }
        fn infer_sneaking_eye_position(&self) -> [f64; 3] {
            self.player_head()
        }
        fn peek_rotation(&self, rotation: Rotation) -> Rotation {
            rotation
        }
        fn calculation_facts(&self) -> PlayerFacts {
            PlayerFacts { has_throwaway: false, has_water_bucket: false, can_sprint: true, ..PlayerFacts::default() }
        }
        fn has_generic_throwaway(&self) -> bool {
            false
        }
        fn select_throwaway_for_location(&self, _select: bool, _x: i32, _y: i32, _z: i32) -> bool {
            false
        }
        fn switch_to_best_tool_for(&self, _pos: Pos) {}
        fn water_bucket_hotbar_slot(&self) -> Option<usize> {
            None
        }
        fn empty_bucket_hotbar_slot(&self) -> Option<usize> {
            None
        }
        fn select_hotbar_slot(&self, _slot: usize) {}
        fn falling_block_near(&self, _pos: Pos) -> bool {
            false
        }
        fn state_for_placement(&self, _slot: usize, _clicked: Pos, _face: [i32; 3], _hit: [f64; 3], _rotation: Rotation) -> Option<Rc<BlockState>> {
            None
        }
        fn would_place(&self, _slot: usize, _clicked: Pos, _face: [i32; 3], _hit: [f64; 3], _rotation: Rotation) -> Option<Rc<BlockState>> {
            None
        }
        fn approx_placeable(&self, size: usize) -> Vec<Rc<BlockState>> {
            vec![self.world.air.clone(); size]
        }
        fn shape_bounds(&self, _pos: Pos) -> Option<[f64; 6]> {
            Some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0])
        }
        fn can_survive(&self, _pos: Pos, _state: &BlockState) -> bool {
            true
        }
        fn placement_plausible(&self, _pos: Pos, _state: &BlockState) -> bool {
            true
        }
        fn mobs_to_avoid(&self) -> Vec<Pos> {
            Vec::new()
        }
        fn mob_spawners_near(&self) -> Vec<Pos> {
            Vec::new()
        }
        fn update_look_target(&self, rotation: Rotation, _force: bool) {
            self.yaw.set(rotation.yaw);
        }
        fn clear_all_keys(&self) {
            self.held.borrow_mut().clear();
        }
        fn set_input_force_state(&self, input: Input, forced: bool) {
            let mut h = self.held.borrow_mut();
            h.retain(|i| *i != input);
            if forced {
                h.push(input);
            }
        }
        fn is_input_forced_down(&self, input: Input) -> bool {
            self.held.borrow().contains(&input)
        }
        fn stop_breaking_block(&self) {}
    }

    #[test]
    fn the_behavior_walks_a_straight_path_to_its_goal() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let mut w = World::new();
                w.floor(-2, 40);
                let s = settings();
                let player = Paper::new(&w, [0.5, 64.0, 0.5]);
                let mut pb = PathingBehavior::new();
                let goal = rc(GoalBlock([12, 64, 0]));
                let mut arrived = false;
                for tick in 0..400u64 {
                    pb.begin_tick(&player, &s, &BareHands);
                    let command = PathingCommand::SetGoalAndPath(goal.clone());
                    pb.pre_tick(&player, &s, Some(&command), None, false, tick);
                    pb.run_tick(&player, &s, &BareHands, tick);
                    pb.post_tick(&player, &s, Some(&command), None, tick);
                    player.physics();
                    if player.player_feet() == [12, 64, 0] {
                        arrived = true;
                        break;
                    }
                }
                assert!(arrived, "stopped at {:?}: {:?}", player.player_feet(), pb.log);
                assert!(!pb.log.iter().any(|l| l.contains("keeps changing")), "{:?}", pb.log);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn the_behavior_goes_over_hills_and_down_again() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let mut w = World::new();
                w.floor(-2, 40);
                let stone = w.stone.clone();
                for x in 4..=7 {
                    for z in -3..=3 {
                        w.set(x, 64, z, &stone);
                    }
                }
                for x in 6..=7 {
                    for z in -3..=3 {
                        w.set(x, 65, z, &stone);
                    }
                }
                let s = settings();
                let player = Paper::new(&w, [0.5, 64.0, 0.5]);
                let mut pb = PathingBehavior::new();
                let goal = rc(GoalBlock([14, 64, 0]));
                let mut arrived = false;
                for tick in 0..600u64 {
                    pb.begin_tick(&player, &s, &BareHands);
                    let command = PathingCommand::SetGoalAndPath(goal.clone());
                    pb.pre_tick(&player, &s, Some(&command), None, false, tick);
                    pb.run_tick(&player, &s, &BareHands, tick);
                    pb.post_tick(&player, &s, Some(&command), None, tick);
                    player.physics();
                    if player.player_feet() == [14, 64, 0] {
                        arrived = true;
                        break;
                    }
                }
                assert!(arrived, "stopped at {:?}: {:?}", player.player_feet(), pb.log.iter().rev().take(14).collect::<Vec<_>>());
                assert!(!pb.log.iter().any(|l| l.contains("keeps changing")), "{:?}", pb.log);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn the_behavior_drops_down_a_ledge_and_cuts_the_corner() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let mut w = World::new();
                w.floor(-2, 40);
                let stone = w.stone.clone();
                // A plateau one block higher over x 0..=3; the goal is down and to the side.
                for x in 0..=3 {
                    for z in -3..=3 {
                        w.set(x, 64, z, &stone);
                    }
                }
                let s = settings();
                let player = Paper::new(&w, [0.5, 65.0, 0.5]);
                let mut pb = PathingBehavior::new();
                let goal = rc(GoalBlock([8, 64, 3]));
                let mut arrived = false;
                for tick in 0..600u64 {
                    pb.begin_tick(&player, &s, &BareHands);
                    let command = PathingCommand::SetGoalAndPath(goal.clone());
                    pb.pre_tick(&player, &s, Some(&command), None, false, tick);
                    pb.run_tick(&player, &s, &BareHands, tick);
                    pb.post_tick(&player, &s, Some(&command), None, tick);
                    player.physics();
                    if player.player_feet() == [8, 64, 3] {
                        arrived = true;
                        break;
                    }
                }
                assert!(arrived, "stopped at {:?}: {:?}", player.player_feet(), pb.log.iter().rev().take(14).collect::<Vec<_>>());
                assert!(!pb.log.iter().any(|l| l.contains("keeps changing")), "{:?}", pb.log);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
