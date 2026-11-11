// Run from the repository root:
// rustc --edition 2024 --test -O tools/legacy_pvp_tests.rs -o <temporary-test-binary>
// Then execute that binary. Tests use production source without native Horizon/GPU dependencies.
#![allow(dead_code)]

#[path = "../mixins/sunset/legacy_speed.rs"] mod legacy_speed;
#[path = "../mixins/sunset/movement.rs"] mod movement;
#[path = "../mixins/sunset/autoblock.rs"] mod autoblock;
#[path = "../mixins/sunset/aura_timing.rs"] mod sunset_aura_timing;
