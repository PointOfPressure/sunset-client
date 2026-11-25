// The UI layer's tests (layers of shapes, layout, scrolling, widgets) and the surfaces laid out with it.
// Run from the repository root:
// rustc --edition 2024 --test -O client/tools/ui_tests.rs -o <temporary-test-binary>
// Then execute that binary. Tests use production source without native Horizon/GPU dependencies.
#![allow(dead_code)]
extern crate self as sunset_core;

#[path = "../sunset/src/ui.rs"] pub mod ui;
#[path = "../sunset/src/game/picker_layout.rs"] mod sunset_picker_layout;
