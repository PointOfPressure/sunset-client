// baritone.utils.GuiClick, the part that needs no game: where a pixel of the screen points, and what pressing and letting go of a
// mouse button means. A left click walks into a block, a right click walks onto it, and a left drag makes a selection. The Java
// opens an empty screen of its own to free the mouse; a mod here has none, so this is the playing screen with the mouse let go.
// Escape closes it, as it closes the Java's.
use super::baritone_utils::Pos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
}

/// What letting go of a button asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Released {
    /// `setGoalAndPath(new GoalBlock(pos))`.
    GoalAt(Pos),
    /// A selection between the two blocks, in place of all the others.
    Selection(Pos, Pos),
}

#[derive(Default)]
pub struct GuiClick {
    /// `#click` was typed: the chat that typed it has to close before this opens.
    pending_open: bool,
    pub active: bool,
    pub click_start: Option<Pos>,
    /// `currentMouseOver`.
    pub over: Option<Pos>,
}

impl GuiClick {
    pub fn request_open(&mut self) {
        self.pending_open = true;
    }

    /// Whether the screen has to be opened now; from here on the clicks count.
    pub fn take_open(&mut self) -> bool {
        if !std::mem::take(&mut self.pending_open) {
            return false;
        }
        self.active = true;
        self.click_start = None;
        self.over = None;
        true
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// `mouseClicked`.
    pub fn pressed(&mut self) {
        self.click_start = self.over;
    }

    /// `mouseReleased`.
    pub fn released(&mut self, button: Button) -> Option<Released> {
        let start = self.click_start.take();
        let over = self.over?;
        match button {
            Button::Left => match start {
                Some(start) if start != over => Some(Released::Selection(start, over)),
                _ => Some(Released::GoalAt(over)),
            },
            Button::Right => Some(Released::GoalAt([over[0], over[1] + 1, over[2]])),
        }
    }
}

/// `toWorld`, as a direction: the unit vector from the camera through a point of the screen. `cursor` and `size` are in the same
/// units, y going down, `fov` is the vertical field of view in degrees, and a yaw of 0 looks along +z.
pub fn ray_through(cursor: (f32, f32), size: (f32, f32), fov: f32, yaw: f32, pitch: f32) -> [f64; 3] {
    let (ndc_x, ndc_y) = (f64::from(cursor.0 / size.0) * 2.0 - 1.0, 1.0 - f64::from(cursor.1 / size.1) * 2.0);
    let half_height = (f64::from(fov).to_radians() * 0.5).tan();
    let half_width = half_height * f64::from(size.0 / size.1);
    let (sin_yaw, cos_yaw) = f64::from(yaw).to_radians().sin_cos();
    let (sin_pitch, cos_pitch) = f64::from(pitch).to_radians().sin_cos();
    let forward = [-sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch];
    let right = [-cos_yaw, 0.0, -sin_yaw];
    // right x forward.
    let up = [
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    ];
    let along = |i: usize| forward[i] + right[i] * ndc_x * half_width + up[i] * ndc_y * half_height;
    let dir = [along(0), along(1), along(2)];
    let length = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    dir.map(|v| v / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    #[test]
    fn the_middle_of_the_screen_is_where_the_camera_looks() {
        assert!(close(ray_through((50.0, 50.0), (100.0, 100.0), 70.0, 0.0, 0.0), [0.0, 0.0, 1.0]));
        assert!(close(ray_through((50.0, 50.0), (100.0, 100.0), 70.0, 90.0, 0.0), [-1.0, 0.0, 0.0]));
        assert!(close(ray_through((50.0, 50.0), (100.0, 100.0), 70.0, 0.0, 90.0), [0.0, -1.0, 0.0]));
    }

    #[test]
    fn the_edges_of_the_screen_are_half_the_field_of_view_away() {
        // Facing south, the right of the screen is west (-x) and the top is up.
        let s = 0.5_f64.sqrt();
        let right = ray_through((100.0, 50.0), (100.0, 100.0), 90.0, 0.0, 0.0);
        assert!(close(right, [-s, 0.0, s]), "{right:?}");
        let top = ray_through((50.0, 0.0), (100.0, 100.0), 90.0, 0.0, 0.0);
        assert!(close(top, [0.0, s, s]), "{top:?}");
        // A wide window sees further to the sides for the same vertical view.
        let wide = ray_through((200.0, 50.0), (200.0, 100.0), 90.0, 0.0, 0.0);
        assert!(wide[0] < right[0], "{wide:?}");
    }

    #[test]
    fn a_left_click_walks_into_the_block_and_a_right_click_onto_it() {
        let mut c = GuiClick { active: true, over: Some([1, 64, 2]), ..GuiClick::default() };
        c.pressed();
        assert_eq!(c.released(Button::Left), Some(Released::GoalAt([1, 64, 2])));
        c.pressed();
        assert_eq!(c.released(Button::Right), Some(Released::GoalAt([1, 65, 2])));
    }

    #[test]
    fn a_drag_makes_a_selection_and_a_click_into_the_void_does_nothing() {
        let mut c = GuiClick { active: true, over: Some([1, 64, 2]), ..GuiClick::default() };
        c.pressed();
        c.over = Some([5, 70, 9]);
        assert_eq!(c.released(Button::Left), Some(Released::Selection([1, 64, 2], [5, 70, 9])));
        assert_eq!(c.click_start, None);
        c.over = None;
        c.pressed();
        assert_eq!(c.released(Button::Left), None);
    }

    #[test]
    fn opening_waits_to_be_asked_and_happens_once() {
        let mut c = GuiClick::default();
        assert!(!c.take_open());
        c.request_open();
        assert!(!c.active);
        assert!(c.take_open());
        assert!(c.active);
        assert!(!c.take_open());
        c.close();
        assert!(!c.active);
    }
}
