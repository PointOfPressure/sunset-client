// SPDX-License-Identifier: MIT
// Tests the production editor directly, without Horizon or Cargo dependencies.
// rustc --test --edition=2024 tools/test_text_edit.rs -o text-edit-tests.exe
#[path = "../mixins/solstice/text_edit.rs"]
mod text_edit;
