// What Baritone needs from the agent module: aim at a block, hold the attack button, click.
// An extension trait on the game (Sunset 2: support modules are compiled outside the game's crate, where an inherent
// impl of the game's type is refused); the modules that call these import it.
use super::Game;
use winit::event::MouseButton;

pub(in crate::game) trait BaritoneBridge {
    fn bar_attack(&mut self, down: bool);
    fn bar_use(&mut self);
    fn bar_look_target(&self) -> Option<[i32; 3]>;
    fn bar_mouse_block(&mut self) -> Option<[i32; 3]>;
    fn bar_eye(&self) -> Option<[f64; 3]>;
    fn bar_ray(&self, eye: [f64; 3], yaw: f32, pitch: f32, reach: f64) -> Option<([i32; 3], [i32; 3])>;
}

/// `Entity.calculateViewVector`: the unit vector a yaw and pitch (degrees) look along.
fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

impl BaritoneBridge for Game<'_> {
    fn bar_attack(&mut self, down: bool) {
        // A newer control-socket task may share the game's Agent mouse source.
        // Letting go of Baritone's old hold must not abort that task's mining.
        if !down && super::agent::sunset_goto::foreign_input(self).1[0] {
            return;
        }
        let _ = self.agent_mouse(MouseButton::Left, down);
    }
    /// Use what is in the hand: a firework rocket while gliding.
    fn bar_use(&mut self) {
        let _ = self.agent_mouse(MouseButton::Right, true);
        let _ = self.agent_mouse(MouseButton::Right, false);
    }
    /// The block under the crosshair, within 64 blocks.
    fn bar_look_target(&self) -> Option<[i32; 3]> {
        let eye = self.eye_position()?;
        let dir = view_vector(self.yaw, self.pitch);
        self.clip_ray(eye, dir, 64.0).map(|hit| hit.pos)
    }
    /// GuiClick.render: the block under the mouse, as far as the view goes (the far plane the Java unprojects to).
    fn bar_mouse_block(&mut self) -> Option<[i32; 3]> {
        let partial = self.clock.camera_fraction() as f32;
        let (camera, yaw, pitch) = self.camera_view(partial)?;
        let origin = camera.to_array();
        let fov = self.fov_degrees(partial, origin);
        let dir = super::baritone_gui_click::ray_through(self.cursor, self.gui_size(), fov, yaw, pitch);
        let reach = f64::from(self.settings.render_distance) * 16.0 * 4.0;
        self.clip_ray(origin, dir, reach).map(|hit| hit.pos)
    }
    /// The player's eyes, and how far a hand reaches.
    fn bar_eye(&self) -> Option<[f64; 3]> {
        self.eye_position()
    }
    /// The block an aim hits from `eye` within `reach`, and the face hit (RayTraceUtils.rayTraceTowards).
    fn bar_ray(&self, eye: [f64; 3], yaw: f32, pitch: f32, reach: f64) -> Option<([i32; 3], [i32; 3])> {
        let dir = view_vector(yaw, pitch);
        self.clip_ray(eye, dir, reach).map(|hit| (hit.pos, hit.face))
    }
}
