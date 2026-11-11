// rustc --edition 2024 --test -O tools/inventory_tests.rs -o <temporary-test-binary>
// Execute that binary to check the production planner/executor without native Horizon dependencies.
#![allow(dead_code)]

#[path = "../mixins/sunset/inventory_plan.rs"] mod inventory_plan;
#[path = "../mixins/sunset/inventory_exec.rs"] mod inventory_exec;
