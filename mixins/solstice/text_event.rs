// SPDX-License-Identifier: MIT
// Original implementation assisted by OpenAI Codex (GPT-6.1 Sol).
// Observe actual layout text before follow_keymap; never swallow releases.
fn solstice_text_event(event: &winit::event::WindowEvent) {
    crate::game::solstice::text_event(event);
}
