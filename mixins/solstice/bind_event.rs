// SPDX-License-Identifier: MIT
// Observe releases/focus before screens consume input. AI-assisted contribution.
fn solstice_bind_event(event: &winit::event::WindowEvent) {
    crate::game::solstice::bind_event(event);
}
