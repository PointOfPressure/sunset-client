// Run through perf_visual_tests.py, which includes the current production
// Sweep and clear_line implementations using a small read-only world stub.
#![allow(dead_code)]
use std::{cell::Cell, collections::HashMap};

#[path = "../mixins/sunset/radar_math.rs"]
mod radar_math;

struct State { full_collision: bool, fluid_level: u8 }
struct Facts;
impl Facts {
    fn is_air(&self, state: u32) -> bool { state == 0 }
    fn get(&self, state: u32) -> &'static State {
        static STATES: [State; 5] = [
            State { full_collision: false, fluid_level: 0 },
            State { full_collision: true, fluid_level: 0 },
            State { full_collision: false, fluid_level: 0 },
            State { full_collision: false, fluid_level: 1 },
            State { full_collision: true, fluid_level: 1 },
        ];
        &STATES[state as usize]
    }
}
struct Resources;
impl Resources {
    fn facts(&self) -> &Facts { &Facts }
}
#[derive(Default)]
struct World { blocks: HashMap<[i32; 3], u32>, reads: Cell<usize> }
impl World {
    fn block(&self, x: i32, y: i32, z: i32) -> u32 {
        self.reads.set(self.reads.get() + 1);
        *self.blocks.get(&[x, y, z]).unwrap_or(&0)
    }
}
struct Game<'a> { world: World, res: Resources, eye: Option<[f64; 3]>, _life: std::marker::PhantomData<&'a ()> }
impl Game<'_> {
    fn eye_position(&self) -> Option<[f64; 3]> { self.eye }
}
mod production {
    use super::Game;
    include!(env!("SUNSET_PERF_UTIL_FRAGMENT"));
}

mod visual_production {
    use super::production::dist;
    pub(super) struct Camera;
    impl Camera {
        fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> { Some([at[0] as f32, at[1] as f32]) }
        fn sees(&self, _: [f64; 3], _: f64) -> bool { true }
        fn on_screen(&self, _: [f32; 2], _: f32) -> bool { true }
        fn eye(&self) -> [f64; 3] { [-1.0; 3] }
        fn points(&self, _: [f64; 3], _: [f64; 3]) -> [Option<[f32; 2]>; 8] { [Some([1.0; 2]); 8] }
        fn corners(&self, _: [f64; 3], _: [f64; 3]) -> Option<[[f32; 2]; 8]> { Some([[1.0; 2]; 8]) }
        fn bounds(&self, _: &[[f32; 2]; 8]) -> Option<[f32; 4]> { Some([0.0, 0.0, 10.0, 10.0]) }
    }
    pub(super) trait Canvas {
        fn fill(&mut self, rect: [f32; 4], argb: u32);
        fn quad(&mut self, points: [[f32; 2]; 4], argb: u32);
    }
    #[derive(Default)]
    struct Counter { fills: usize, quads: usize }
    impl Canvas for Counter {
        fn fill(&mut self, _: [f32; 4], _: u32) { self.fills += 1; }
        fn quad(&mut self, _: [[f32; 2]; 4], _: u32) { self.quads += 1; }
    }
    fn line(out: &mut impl Canvas, a: [f32; 2], b: [f32; 2], color: u32, _: f32) {
        out.quad([a, b, b, a], color);
    }
    fn outline(out: &mut impl Canvas, r: [f32; 4], color: u32, _: f32, _: bool) {
        for _ in 0..4 { out.fill(r, color); }
    }
    include!(env!("SUNSET_PERF_VISUAL_FRAGMENT"));
    pub(super) fn count(box_2d: bool, opacity: f64, lines: bool, argb: u32) -> (bool, usize, usize) {
        let look = Look { draw: if box_2d { Draw::Box2d } else { Draw::Box3d }, sides: true, lines, fill: opacity };
        let mut out = Counter::default();
        let visible = draw_box(&Camera, &mut out, &look, [0.0; 3], [1.0; 3], argb, 1.0);
        (visible, out.fills, out.quads)
    }
}

#[test]
fn transparent_box_fills_emit_no_geometry_and_preserve_lines_and_max_accounting() {
    for opacity in [0.0, 0.1, 0.49] {
        assert_eq!(visual_production::count(false, opacity, true, 0x01FF_FFFF), (true, 0, 12));
        assert_eq!(visual_production::count(false, opacity, false, 0x01FF_FFFF), (true, 0, 0));
        assert_eq!(visual_production::count(true, opacity, true, 0x01FF_FFFF), (true, 4, 0));
    }
    // Round-to-nearest must happen before deciding whether alpha is zero.
    assert_eq!(visual_production::count(false, 0.5, true, 0x01FF_FFFF), (true, 0, 15));
    assert_eq!(visual_production::count(true, 0.5, true, 0x01FF_FFFF), (true, 5, 0));
    assert_eq!(visual_production::count(false, 1.0, true, 0x00FF_FFFF), (true, 0, 12));
    println!("zero-alpha 3D boxes: 15 -> 12 emitted quads, sides-only 3 -> 0; 2D fill omitted, outlines preserved");
}

fn game(eye: Option<[f64; 3]>) -> Game<'static> {
    Game { world: World::default(), res: Resources, eye, _life: std::marker::PhantomData }
}

fn original_clear_line(game: &Game<'_>, to: [f64; 3], except: [i32; 3]) -> bool {
    let Some(eye) = game.eye_position() else { return false; };
    let steps = (production::dist(eye, to) * 8.0).ceil().max(1.0) as i32;
    for i in 1..steps {
        let t = f64::from(i) / f64::from(steps);
        let at = std::array::from_fn(|axis| (eye[axis] + (to[axis] - eye[axis]) * t).floor() as i32);
        if at != except {
            let state = game.world.block(at[0], at[1], at[2]);
            if !game.res.facts().is_air(state) && game.res.facts().get(state).full_collision {
                return false;
            }
        }
    }
    true
}

#[test]
fn sampled_rays_match_original_across_negative_coordinates_and_obstacles() {
    let mut game = game(Some([-4.125, 0.25, 1.75]));
    for x in -10..=10 {
        for y in -5..=5 {
            for z in -10..=10 {
                if (x * 31 + y * 17 + z * 13_i32).rem_euclid(11) == 0 {
                    game.world.blocks.insert([x, y, z], if x % 2 == 0 { 1 } else { 2 });
                }
            }
        }
    }
    let mut comparisons = 0;
    for x in -12..=12 {
        for y in [-3.25, 0.0, 4.75] {
            for z in [-10.1, -0.25, 0.0, 5.99, 10.5] {
                let end = [f64::from(x) + 0.375, y, z];
                for except in [[i32::MIN; 3], [-4, 0, 1], [x, 0, 0]] {
                    game.world.reads.set(0);
                    let expected = original_clear_line(&game, end, except);
                    let old_reads = game.world.reads.get();
                    game.world.reads.set(0);
                    assert_eq!(production::clear_line(&game, end, except), expected, "{end:?} except {except:?}");
                    assert!(game.world.reads.get() <= old_reads);
                    comparisons += 1;
                }
            }
        }
    }
    println!("ray equivalence: {comparisons} obstacle/coordinate/exclusion cases");
}

#[test]
fn rays_read_each_sampled_voxel_once_and_keep_endpoint_exclusion() {
    let mut game = game(Some([0.5, 0.5, 0.5]));
    let end = [8.5, 0.5, 0.5];
    assert!(original_clear_line(&game, end, [i32::MIN; 3]));
    let before = game.world.reads.replace(0);
    assert!(production::clear_line(&game, end, [i32::MIN; 3]));
    let after = game.world.reads.get();
    assert_eq!((before, after), (63, 9));
    println!("8-block empty ray world lookups: {before} -> {after}");
    game.world.blocks.insert([4, 0, 0], 1);
    assert!(!production::clear_line(&game, end, [i32::MIN; 3]));
    assert!(production::clear_line(&game, end, [4, 0, 0]));
    game.eye = None;
    assert!(!production::clear_line(&game, end, [4, 0, 0]));
}

#[test]
fn tunnel_side_matches_original_for_all_air_solid_and_fluid_combinations() {
    let mut game = game(None);
    let through = |state| state == 0 || (Facts.get(state).fluid_level == 0 && !Facts.get(state).full_collision);
    let walk_on = |state| state != 0 && Facts.get(state).fluid_level == 0 && Facts.get(state).full_collision;
    let mut cases = 0;
    for floor in 0..5 {
        for feet in 0..5 {
            for head in 0..5 {
                for roof in 0..5 {
                    game.world.blocks.extend([( [0, -1, 0], floor), ([0, 0, 0], feet), ([0, 1, 0], head), ([0, 2, 0], roof)]);
                    let expected = if walk_on(floor) && through(feet) && !through(roof) && through(head) {
                        0
                    } else if !through(feet) && !through(head) { 1 } else { 2 };
                    assert_eq!(production::side_signature(&game, [0, 0, 0]), expected, "floor={floor} feet={feet} head={head} roof={roof}");
                    cases += 1;
                }
            }
        }
    }
    game.world.blocks.extend([([0, -1, 0], 1), ([0, 0, 0], 1), ([0, 1, 0], 1)]);
    game.world.reads.set(0);
    assert_eq!(production::side_signature(&game, [0, 0, 0]), 1);
    assert_eq!(game.world.reads.get(), 2);
    println!("tunnel side equivalence: {cases} air/solid/fluid combinations; solid wall reads: 4 -> 2");
}

#[test]
fn sweep_partition_keeps_the_same_nearest_distance_set_and_sorted_output() {
    let mut sweep = production::Sweep::new();
    let centre = [-32, 70, 48];
    let distance = |p: &[i32; 3]| (0..3).map(|i| (i64::from(p[i]) - i64::from(centre[i])).pow(2)).sum::<i64>();
    let mut all = Vec::new();
    let mut done = false;
    for tick in 1..=20 {
        done = sweep.tick(tick, centre, 5, 1, 40, 8, |c, x, out| {
            for y in c[1] - 20..=c[1] + 20 {
                for z in c[2] - 50..=c[2] + 50 {
                    out.push(([x, y, z], ()));
                    all.push(([x, y, z], ()));
                }
            }
            4141
        });
        if done { break; }
    }
    assert!(done);
    let mut expected: Vec<_> = all.iter().map(|(p, _)| distance(p)).collect();
    expected.sort_unstable();
    expected.truncate(16_384);
    let actual: Vec<_> = sweep.found.iter().map(|(p, _)| distance(p)).collect();
    assert_eq!(actual, expected);
    assert!(!sweep.tick(21, centre, 5, 1, 40, 8, |_, _, _| panic!("unchanged cache must not scan")));
    let mut visits = 0;
    sweep.tick(22, centre, 5, 2, 40, 8, |_, _, _| { visits += 1; 12_000 });
    assert_eq!(visits, 1, "the per-tick work budget still applies");
    assert!(sweep.found.is_empty(), "changed settings invalidate old results immediately");
}

#[test]
fn nearest_single_selection_preserves_first_equal_distance_target() {
    let candidates = [(3.0_f64, 42), (1.0, 17), (1.0, 99), (2.0, 88)];
    let mut original = candidates.to_vec();
    original.sort_by(|(a, _), (b, _)| a.total_cmp(b));
    let selected = candidates.into_iter().min_by(|(a, _), (b, _)| a.total_cmp(b));
    assert_eq!(selected, original.into_iter().next());
}
