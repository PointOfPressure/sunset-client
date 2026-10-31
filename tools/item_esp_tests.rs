// Standalone offline harness. Compile with rustc --edition=2024 --test tools/item_esp_tests.rs.
// Native host compilation and in-game screenshots remain separate integration checks.
#![allow(dead_code)]
extern crate self as horizon_client;

#[path = "../mixins/sunset/item_esp.rs"]
mod sunset_item_esp;
#[path = "../mixins/sunset/item_esp_math.rs"]
mod sunset_item_esp_math;
#[path = "../mixins/sunset/visual.rs"]
mod sunset_visual;

use std::cell::Cell;
use std::collections::HashMap;
use std::marker::PhantomData;
thread_local! { static NAME_CALLS: Cell<usize> = const { Cell::new(0) }; }

#[derive(Clone, Default)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}
pub struct Stack {
    item: String,
    count: u32,
    name: String,
}
struct Item {
    health: i32,
    stack: Stack,
    previous: Vec3,
    position: Vec3,
}
struct Clock;
impl Clock {
    fn camera_fraction(&self) -> f64 {
        1.0
    }
}
struct Names;
impl Names {
    fn display_get(&self, _: &str) -> Option<String> {
        None
    }
}
struct Pack {
    names: Names,
}
struct Game<'a> {
    dropped: Vec<Item>,
    clock: Clock,
    pack: Pack,
    marker: PhantomData<&'a ()>,
}
impl Game<'_> {
    fn native_text_width(&self, text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }
}
pub mod item_tooltip {
    pub fn name_line(stack: &super::Stack, _: &dyn Fn(&str) -> Option<String>) -> String {
        super::NAME_CALLS.with(|n| n.set(n.get() + 1));
        stack.name.clone()
    }
}

mod sunset {
    use super::*;
    #[derive(Clone, Copy)]
    pub(super) enum Kind {
        Player,
        Hostile,
        Passive,
    }
    pub(super) enum Value {
        Pick {
            i: usize,
            of: &'static [&'static str],
        },
    }
    pub(super) struct Setting {
        pub(super) name: &'static str,
        pub(super) value: Value,
    }
    pub(super) struct Module {
        bools: HashMap<&'static str, bool>,
        nums: HashMap<&'static str, f64>,
        mode: &'static str,
        ids: String,
        pub(super) settings: Vec<Setting>,
    }
    impl Module {
        pub(super) fn b(&self, key: &str) -> bool {
            self.bools.get(key).copied().unwrap_or(false)
        }
        pub(super) fn n(&self, key: &str) -> f64 {
            self.nums.get(key).copied().unwrap_or(0.0)
        }
        pub(super) fn setting(&self, key: &str) -> Option<&Setting> {
            self.settings.iter().find(|s| s.name == key)
        }
        pub(super) fn p(&self, key: &str) -> &'static str {
            if key == "Item filter" {
                return self.mode;
            }
            match self.setting(key) {
                Some(Setting {
                    value: Value::Pick { i, of },
                    ..
                }) => of[*i],
                _ => "",
            }
        }
        pub(super) fn t(&self, _: &str) -> &str {
            &self.ids
        }
        pub(super) fn fixture() -> Self {
            Self {
                bools: HashMap::from([
                    ("Item boxes", true),
                    ("Item labels", true),
                    ("Item tracers", true),
                    ("Item label background", true),
                ]),
                nums: HashMap::from([
                    ("Items max", 256.),
                    ("Item label max", 32.),
                    ("Range", 256.),
                    ("Fade distance", 3.),
                    ("Fill opacity", 0.3),
                ]),
                mode: "All",
                ids: String::new(),
                settings: vec![
                    Setting {
                        name: "Mode",
                        value: Value::Pick {
                            i: 0,
                            of: &["3D box", "2D box", "Outline", "Potato"],
                        },
                    },
                    Setting {
                        name: "Shape mode",
                        value: Value::Pick {
                            i: 0,
                            of: &["Lines", "Sides", "Both"],
                        },
                    },
                ],
            }
        }
        pub(super) fn boolean(&mut self, key: &'static str, value: bool) {
            self.bools.insert(key, value);
        }
        pub(super) fn number(&mut self, key: &'static str, value: f64) {
            self.nums.insert(key, value);
        }
        pub(super) fn filter(&mut self, mode: &'static str, ids: &str) {
            self.mode = mode;
            self.ids = ids.to_owned();
        }
        pub(super) fn pick(&mut self, name: &str, choice: &str) {
            let setting = self.settings.iter_mut().find(|s| s.name == name).unwrap();
            let Value::Pick { i, of } = &mut setting.value;
            *i = of.iter().position(|v| *v == choice).unwrap();
        }
    }
    pub(super) struct Camera;
    impl Camera {
        pub(super) fn eye(&self) -> [f64; 3] {
            [0.; 3]
        }
        pub(super) fn viewport(&self) -> [f32; 2] {
            [400., 300.]
        }
        // The native Camera formulas for yaw/pitch zero, vertical FOV 90, 400x300.
        // Keep the real 0.05 projection near plane and sphere/frustum intersection.
        pub(super) fn sees(&self, at: [f64; 3], radius: f64) -> bool {
            let (sh, ch) = (4.0_f64 / 3.0).atan().sin_cos();
            let (sv, cv) = (1.0_f64).atan().sin_cos();
            at[2] > -radius
                && at[2] * sh - at[0].abs() * ch > -radius
                && at[2] * sv - at[1].abs() * cv > -radius
        }
        pub(super) fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
            if at[2] < 0.05 {
                return None;
            }
            Some([
                ((1.0 - at[0] / (at[2] * 4.0 / 3.0)) * 200.0) as f32,
                ((1.0 - at[1] / at[2]) * 150.0) as f32,
            ])
        }
        pub(super) fn on_screen(&self, at: [f32; 2], margin: f32) -> bool {
            at[0] > -margin && at[0] < 400. + margin && at[1] > -margin && at[1] < 300. + margin
        }
        pub(super) fn points(&self, lo: [f64; 3], hi: [f64; 3]) -> [Option<[f32; 2]>; 8] {
            std::array::from_fn(|i| {
                self.project([
                    if i & 1 == 0 { lo[0] } else { hi[0] },
                    if i & 2 == 0 { lo[1] } else { hi[1] },
                    if i & 4 == 0 { lo[2] } else { hi[2] },
                ])
            })
        }
        pub(super) fn corners(&self, lo: [f64; 3], hi: [f64; 3]) -> Option<[[f32; 2]; 8]> {
            let pts = self.points(lo, hi);
            pts.iter()
                .all(Option::is_some)
                .then(|| pts.map(Option::unwrap_or_default))
        }
        pub(super) fn bounds(&self, pts: &[[f32; 2]; 8]) -> Option<[f32; 4]> {
            let r = pts
                .iter()
                .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, &[x, y]| {
                    [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]
                });
            (r[2] > -1600. && r[0] < 1600. && r[3] > -1600. && r[1] < 1600.).then_some(r)
        }
    }
    #[derive(Debug)]
    pub(super) enum Op {
        Line([f32; 2], [f32; 2]),
        Quad,
        Fill,
        Text(String),
    }
    pub(super) trait Canvas {
        fn fill(&mut self, rect: [f32; 4], colour: u32);
        fn quad(&mut self, corners: [[f32; 2]; 4], colour: u32);
        fn text(&mut self, text: &str, at: [f32; 2], colour: u32);
        fn record(&mut self, op: Op);
    }
    impl Canvas for Vec<Op> {
        fn fill(&mut self, _: [f32; 4], _: u32) {
            self.push(Op::Fill);
        }
        fn quad(&mut self, _: [[f32; 2]; 4], _: u32) {
            self.push(Op::Quad);
        }
        fn text(&mut self, text: &str, _: [f32; 2], _: u32) {
            self.push(Op::Text(text.to_owned()));
        }
        fn record(&mut self, op: Op) {
            self.push(op);
        }
    }
    pub(super) fn line(out: &mut impl Canvas, from: [f32; 2], to: [f32; 2], _: u32, _: f32) {
        out.record(Op::Line(from, to));
    }
    pub(super) fn outline(out: &mut impl Canvas, r: [f32; 4], colour: u32, lw: f32, _: bool) {
        line(out, [r[0], r[1]], [r[2], r[1]], colour, lw);
        line(out, [r[2], r[1]], [r[2], r[3]], colour, lw);
        line(out, [r[2], r[3]], [r[0], r[3]], colour, lw);
        line(out, [r[0], r[3]], [r[0], r[1]], colour, lw);
    }
}

mod sunset_util {
    pub(super) fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }
}

fn game(items: Vec<Item>) -> Game<'static> {
    Game {
        dropped: items,
        clock: Clock,
        pack: Pack { names: Names },
        marker: PhantomData,
    }
}
fn item(id: &str, count: u32, name: &str, x: f64) -> Item {
    let pos = Vec3 {
        x: 0.,
        y: 0.,
        z: x / 10.,
    };
    Item {
        health: 5,
        stack: Stack {
            item: id.to_owned(),
            count,
            name: name.to_owned(),
        },
        previous: pos.clone(),
        position: pos,
    }
}
fn draw(game: &Game<'_>, module: &sunset::Module) -> Vec<sunset::Op> {
    let mut out = Vec::new();
    sunset_item_esp::draw(game, &sunset::Camera, module, 1., &mut out);
    out
}
fn labels(out: &[sunset::Op]) -> Vec<&str> {
    out.iter()
        .filter_map(|op| {
            if let sunset::Op::Text(t) = op {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect()
}
fn line_count(out: &[sunset::Op]) -> usize {
    out.iter()
        .filter(|op| matches!(op, sunset::Op::Line(..)))
        .count()
}

#[test]
fn rendering_uses_nearest_matching_live_items_and_native_names() {
    let mut m = sunset::Module::fixture();
    m.filter("Include", "diamond");
    m.number("Items max", 1.);
    let game = game(vec![
        item("minecraft:diamond", 64, "Far diamond", 200.),
        item("other:diamond", 1, "Wrong namespace", 50.),
        item("minecraft:diamond", 2, "\u{a7}aNative\n Diamond", 80.),
        item("minecraft:diamond", 0, "Empty", 20.),
    ]);
    let out = draw(&game, &m);
    assert_eq!(labels(&out), ["Native Diamond x2"]);
    assert_eq!(line_count(&out), 13); // One native 3D wire box (12 edges), one tracer.
}

#[test]
fn result_and_label_caps_are_separate_and_overlap_is_omitted() {
    let mut m = sunset::Module::fixture();
    m.number("Items max", 10.);
    m.number("Item label max", 2.);
    let game = game(
        (0..20)
            .map(|_| item("minecraft:stone", 1, "Stone", 100.))
            .collect(),
    );
    let out = draw(&game, &m);
    assert_eq!(line_count(&out), 130); // Ten native wire boxes and ten tracers.
    assert_eq!(labels(&out).len(), 2);
    m.number("Item label max", 64.);
    assert_eq!(labels(&draw(&game, &m)).len(), 4);
}

#[test]
fn no_transient_item_entries_survive_empty_world_death_or_filter_changes() {
    let mut m = sunset::Module::fixture();
    m.filter("Include", "diamond");
    let mut game = game(vec![item("minecraft:diamond", 5, "Diamond", 100.)]);
    assert_eq!(labels(&draw(&game, &m)), ["Diamond x5"]);
    game.dropped[0].health = 0;
    assert!(draw(&game, &m).is_empty());
    game.dropped.clear(); // The native host clears dropped on dimension/disconnect.
    assert!(draw(&game, &m).is_empty());
    game.dropped.push(item("minecraft:stone", 1, "Stone", 100.));
    assert!(draw(&game, &m).is_empty());
    m.filter("Include", "stone");
    assert_eq!(labels(&draw(&game, &m)), ["Stone"]);
    m.filter("All", "diamond");
    assert_eq!(labels(&draw(&game, &m)), ["Stone"]);
    m.filter("Include", "diamond");
    assert!(draw(&game, &m).is_empty());
}

#[test]
fn culling_range_and_optional_features_gate_rendering() {
    let mut m = sunset::Module::fixture();
    let mut behind = item("minecraft:stone", 1, "Behind", 100.);
    behind.position.z = -1.;
    let game = game(vec![behind, item("minecraft:diamond", 1, "Diamond", 100.)]);
    m.number("Range", 8.);
    assert!(draw(&game, &m).is_empty());
    m.number("Range", 256.);
    m.boolean("Item labels", false);
    m.boolean("Item tracers", false);
    let out = draw(&game, &m);
    assert_eq!(out.len(), 12);
    assert_eq!(line_count(&out), 12);
    m.boolean("Item boxes", false);
    assert!(draw(&game, &m).is_empty());
}

#[test]
fn native_name_work_is_bounded_even_when_all_labels_overlap_or_cannot_fit() {
    let mut m = sunset::Module::fixture();
    m.number("Items max", 1024.);
    m.number("Item label max", 8.);
    let mut game = game(
        (0..2000)
            .map(|_| item("minecraft:stone", 64, "Stone", 100.))
            .collect(),
    );
    NAME_CALLS.with(|n| n.set(0));
    let out = draw(&game, &m);
    assert_eq!(labels(&out).len(), 4);
    NAME_CALLS.with(|n| assert_eq!(n.get(), 8));
    for item in &mut game.dropped {
        item.position.y = 9.6; // Visible anchor at y=2.25px, no room above it for a label.
    }
    NAME_CALLS.with(|n| n.set(0));
    assert!(labels(&draw(&game, &m)).is_empty());
    NAME_CALLS.with(|n| assert_eq!(n.get(), 8));
    m.boolean("Item labels", false);
    NAME_CALLS.with(|n| n.set(0));
    draw(&game, &m);
    NAME_CALLS.with(|n| assert_eq!(n.get(), 0));
}

fn at(mut item: Item, position: [f64; 3]) -> Item {
    item.position = Vec3 {
        x: position[0],
        y: position[1],
        z: position[2],
    };
    item.previous = item.position.clone();
    item
}

#[test]
fn edge_sphere_with_offscreen_anchor_cannot_consume_label_or_tracer_cap() {
    let mut m = sunset::Module::fixture();
    m.boolean("Item boxes", false);
    m.number("Items max", 1.);
    let edge = at(
        item("minecraft:stone", 1, "Off-screen anchor", 0.),
        [0., 4.9, 5.],
    );
    let cam = sunset::Camera;
    assert!(cam.sees([0., 5.025, 5.], 0.217));
    assert!(!cam.on_screen(cam.project([0., 5.15, 5.]).unwrap(), 0.));
    let game = game(vec![
        edge,
        item("minecraft:diamond", 1, "Visible diamond", 100.),
    ]);
    let out = draw(&game, &m);
    assert_eq!(labels(&out), ["Visible diamond"]);
    assert_eq!(line_count(&out), 1);
    m.boolean("Item labels", false);
    assert_eq!(line_count(&draw(&game, &m)), 1);
    m.boolean("Item labels", true);
    m.boolean("Item tracers", false);
    assert_eq!(labels(&draw(&game, &m)), ["Visible diamond"]);
}

#[test]
fn near_plane_sphere_with_no_box_primitives_cannot_consume_box_cap() {
    let mut m = sunset::Module::fixture();
    m.boolean("Item labels", false);
    m.boolean("Item tracers", false);
    m.number("Items max", 1.);
    let near = at(
        item("minecraft:stone", 1, "Unprojectable", 0.),
        [0., -0.125, -0.08],
    );
    let cam = sunset::Camera;
    assert!(cam.sees([0., 0., -0.08], 0.217));
    assert!(
        cam.points([-0.125, -0.125, -0.205], [0.125, 0.125, 0.045])
            .iter()
            .all(Option::is_none)
    );
    let game = game(vec![
        near,
        item("minecraft:diamond", 1, "Visible diamond", 100.),
    ]);
    for (mode, lines, fills) in [
        ("3D box", 12, 0),
        ("2D box", 4, 0),
        ("Outline", 4, 0),
        ("Potato", 0, 1),
    ] {
        m.pick("Mode", mode);
        let out = draw(&game, &m);
        assert_eq!(line_count(&out), lines, "{mode}");
        assert_eq!(
            out.iter()
                .filter(|op| matches!(op, sunset::Op::Fill))
                .count(),
            fills,
            "{mode}"
        );
    }
}

#[test]
fn eligibility_uses_enabled_primitives_instead_of_requiring_all_of_them() {
    let mut m = sunset::Module::fixture();
    m.number("Items max", 1.);
    m.pick("Mode", "2D box");
    let near = at(
        item("minecraft:stone", 1, "Visible anchor", 0.),
        [0., -0.25, 0.06],
    );
    let game = game(vec![
        near,
        item("minecraft:diamond", 1, "Far diamond", 100.),
    ]);
    let out = draw(&game, &m);
    assert_eq!(labels(&out), ["Visible anchor"]);
    assert_eq!(line_count(&out), 1); // 2D box straddles near plane; the enabled tracer still draws.
    m.boolean("Item labels", false);
    m.boolean("Item tracers", false);
    assert_eq!(line_count(&draw(&game, &m)), 4); // Box-only mode instead admits the farther box.
}

#[test]
fn box_preflight_agrees_with_native_draw_for_modes_shapes_edges_and_near_plane() {
    let mut m = sunset::Module::fixture();
    let cam = sunset::Camera;
    for mode in ["3D box", "2D box", "Outline", "Potato"] {
        m.pick("Mode", mode);
        for shape in ["Lines", "Sides", "Both"] {
            m.pick("Shape mode", shape);
            let look = sunset_visual::Look::of(&m);
            for x in [-20., -0.125, 0., 0.125, 20.] {
                for y in [-5., -0.25, 0., 5.] {
                    for z in [-1., -0.08, 0., 0.049, 0.05, 0.175, 1., 5., 20.] {
                        let lo = [x - 0.125, y, z - 0.125];
                        let hi = [x + 0.125, y + 0.25, z + 0.125];
                        let visible = sunset_visual::box_visible(&cam, &look, lo, hi, 1.);
                        let mut out = Vec::<sunset::Op>::new();
                        let drawn =
                            sunset_visual::draw_box(&cam, &mut out, &look, lo, hi, 0xFFFFFFFF, 1.);
                        assert_eq!(visible, drawn, "{mode}/{shape} at {x},{y},{z}");
                        assert_eq!(
                            visible,
                            !out.is_empty(),
                            "{mode}/{shape} emitted geometry at {x},{y},{z}"
                        );
                    }
                }
            }
        }
    }
}
