// What Baritone needs from the agent module: aim at a block, hold the attack button, click.
use super::Game;
use winit::event::MouseButton;

impl Game<'_> {
    pub(in crate::game) fn bar_attack(&mut self, down: bool) {
        let _ = self.agent_mouse(MouseButton::Left, down);
    }
    /// Use what is in the hand: a firework rocket while gliding.
    pub(in crate::game) fn bar_use(&mut self) {
        let _ = self.agent_mouse(MouseButton::Right, true);
        let _ = self.agent_mouse(MouseButton::Right, false);
    }
    /// The block under the crosshair, within 64 blocks.
    pub(in crate::game) fn bar_look_target(&self) -> Option<[i32; 3]> {
        let eye = self.eye_position()?;
        let dir = super::super::view_vector(self.yaw, self.pitch);
        self.clip_ray(eye, dir, 64.0).map(|hit| hit.pos)
    }
    /// GuiClick.render: the block under the mouse, as far as the view goes (the far plane the Java unprojects to).
    pub(in crate::game) fn bar_mouse_block(&mut self) -> Option<[i32; 3]> {
        let partial = self.clock.camera_fraction() as f32;
        let (camera, yaw, pitch) = self.camera_view(partial)?;
        let origin = camera.to_array();
        let fov = self.fov_degrees(partial, origin);
        let dir = super::super::baritone_gui_click::ray_through(self.cursor, self.gui_size(), fov, yaw, pitch);
        let reach = f64::from(self.settings.render_distance) * 16.0 * 4.0;
        self.clip_ray(origin, dir, reach).map(|hit| hit.pos)
    }
    /// The player's eyes, and how far a hand reaches.
    pub(in crate::game) fn bar_eye(&self) -> Option<[f64; 3]> {
        self.eye_position()
    }
    /// The block an aim hits from `eye` within `reach`, and the face hit (RayTraceUtils.rayTraceTowards).
    pub(in crate::game) fn bar_ray(&self, eye: [f64; 3], yaw: f32, pitch: f32, reach: f64) -> Option<([i32; 3], [i32; 3])> {
        let dir = super::super::view_vector(yaw, pitch);
        self.clip_ray(eye, dir, reach).map(|hit| (hit.pos, hit.face))
    }
}
