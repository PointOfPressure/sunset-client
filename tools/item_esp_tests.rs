// Standalone offline harness. Compile with rustc --edition=2024 --test tools/item_esp_tests.rs.
// Native host compilation and in-game screenshots remain separate integration checks.
#![allow(dead_code)]
extern crate self as horizon_client;

#[path = "../mixins/sunset/item_esp.rs"]
mod sunset_item_esp;
#[path = "../mixins/sunset/item_esp_math.rs"]
mod sunset_item_esp_math;

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
    pub(super) struct Module {
        bools: HashMap<&'static str, bool>,
        nums: HashMap<&'static str, f64>,
        mode: &'static str,
        ids: String,
    }
    impl Module {
        pub(super) fn b(&self, key: &str) -> bool {
            self.bools.get(key).copied().unwrap_or(false)
        }
        pub(super) fn n(&self, key: &str) -> f64 {
            self.nums.get(key).copied().unwrap_or(0.0)
        }
        pub(super) fn c(&self, _: &str) -> u32 {
            0xFFFF_A030
        }
        pub(super) fn p(&self, _: &str) -> &str {
            self.mode
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
                ]),
                mode: "All",
                ids: String::new(),
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
    }
    pub(super) struct Camera;
    impl Camera {
        pub(super) fn eye(&self) -> [f64; 3] {
            [0.; 3]
        }
        pub(super) fn viewport(&self) -> [f32; 2] {
            [400., 300.]
        }
        pub(super) fn sees(&self, at: [f64; 3], _: f64) -> bool {
            at[2] > 0. && at[0] > 0. && at[0] < 400. && at[1] > 0. && at[1] < 300.
        }
        pub(super) fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
            (at[2] > 0.).then_some([at[0] as f32, at[1] as f32])
        }
        pub(super) fn on_screen(&self, at: [f32; 2], _: f32) -> bool {
            at[0] > 0. && at[0] < 400. && at[1] > 0. && at[1] < 300.
        }
    }
    #[derive(Debug)]
    pub(super) enum Op {
        Box,
        Tracer,
        Fill,
        Text(String),
    }
    pub(super) trait Canvas {
        fn fill(&mut self, rect: [f32; 4], colour: u32);
        fn text(&mut self, text: &str, at: [f32; 2], colour: u32);
        fn record(&mut self, op: Op);
    }
    impl Canvas for Vec<Op> {
        fn fill(&mut self, _: [f32; 4], _: u32) {
            self.push(Op::Fill);
        }
        fn text(&mut self, text: &str, _: [f32; 2], _: u32) {
            self.push(Op::Text(text.to_owned()));
        }
        fn record(&mut self, op: Op) {
            self.push(op);
        }
    }
    pub(super) fn line(out: &mut impl Canvas, _: [f32; 2], _: [f32; 2], _: u32, _: f32) {
        out.record(Op::Tracer);
    }
}

mod sunset_visual {
    use super::sunset::*;
    pub(super) struct Look;
    impl Look {
        pub(super) fn of(_: &Module) -> Self {
            Self
        }
    }
    pub(super) fn faded(colour: u32, _: f64, _: f64) -> u32 {
        colour
    }
    pub(super) fn draw_box(
        _: &Camera,
        out: &mut impl Canvas,
        _: &Look,
        _: [f64; 3],
        _: [f64; 3],
        _: u32,
        _: f32,
    ) -> bool {
        out.record(Op::Box);
        true
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
    let pos = Vec3 { x, y: 100., z: 5. };
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
    assert_eq!(
        out.iter()
            .filter(|op| matches!(op, sunset::Op::Box))
            .count(),
        1
    );
    assert_eq!(
        out.iter()
            .filter(|op| matches!(op, sunset::Op::Tracer))
            .count(),
        1
    );
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
    assert_eq!(
        out.iter()
            .filter(|op| matches!(op, sunset::Op::Box))
            .count(),
        10
    );
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
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], sunset::Op::Box));
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
        item.position.y = 5.;
    }
    NAME_CALLS.with(|n| n.set(0));
    assert!(labels(&draw(&game, &m)).is_empty());
    NAME_CALLS.with(|n| assert_eq!(n.get(), 8));
    m.boolean("Item labels", false);
    NAME_CALLS.with(|n| n.set(0));
    draw(&game, &m);
    NAME_CALLS.with(|n| assert_eq!(n.get(), 0));
}
