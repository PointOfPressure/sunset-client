// Hand View's switches, read where the first-person hands are posed and set by the game: a transform for each arm
// (right, left) put after the game's own pose, whether the hands keep still when the view turns, whether they are
// lit at full brightness, and the totem pop's size, place on the screen and extra ticks.
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub static NO_SWAY: AtomicBool = AtomicBool::new(false);
pub static NO_SHADOW: AtomicBool = AtomicBool::new(false);
pub static VIEW: RwLock<Option<[glam::Mat4; 2]>> = RwLock::new(None);
pub static TOTEM: RwLock<Option<TotemPop>> = RwLock::new(None);
pub static TOTEM_EXTRA_TICKS: AtomicU32 = AtomicU32::new(0);
// Swing animation (0 the game's own, else 1 to 3), Slow swing, No equip drop.
pub static SWING: AtomicU32 = AtomicU32::new(0);
pub static SLOW_SWING: AtomicBool = AtomicBool::new(false);
pub static NO_EQUIP_DROP: AtomicBool = AtomicBool::new(false);
// A slow swing lasts this many ticks (the game's lasts 6).
pub const SLOW_SWING_TICKS: i32 = 10;

// Block light 15 and sky light 15, packed as the hands' vertices take it.
pub const FULL_LIGHT: u32 = 0xF0F0;

// The totem pop's size, and where it flies on the screen (fractions of half the screen, right and up).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TotemPop {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}

// The hand pass's field of view is 70 degrees: half the screen's height, one block out, is tan(35).
const HALF_SCREEN: f32 = 0.700_207_5;

impl TotemPop {
    // The game's pose of the item this frame, sized about its middle and moved by the same share of the screen at
    // any distance (the move grows with how far out the item is).
    pub fn apply(self, pose: glam::Mat4, aspect: f32) -> glam::Mat4 {
        let depth = -pose.w_axis.z;
        let shift = glam::Vec3::new(self.x * HALF_SCREEN * aspect * depth, self.y * HALF_SCREEN * depth, 0.0);
        glam::Mat4::from_translation(shift) * pose * glam::Mat4::from_scale(glam::Vec3::splat(self.scale))
    }
}

pub fn totem_pose(pose: glam::Mat4, aspect: f32) -> glam::Mat4 {
    match TOTEM.read() {
        Ok(t) => t.map_or(pose, |t| t.apply(pose, aspect)),
        Err(_) => pose,
    }
}

// The light the hands are drawn with: the eye block's, or full with No hand shadow.
pub fn hand_light(light: u32) -> u32 {
    if NO_SHADOW.load(Ordering::Relaxed) { FULL_LIGHT } else { light }
}

// A pose with the arm's own transform after it.
pub fn posed(left: bool, pose: glam::Mat4) -> glam::Mat4 {
    match VIEW.read() {
        Ok(view) => view.map_or(pose, |v| pose * v[usize::from(left)]),
        Err(_) => pose,
    }
}

// The game's swing of a held item at progress `attack` (0 to 1), after its place in the hand: across, up and in,
// turned about y, z, x and back about y.
fn game_swing(side: f32, attack: f32) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    let (pi, root) = (std::f32::consts::PI, attack.sqrt());
    Mat4::from_translation(Vec3::new(side * -0.4 * (root * pi).sin(), 0.2 * (root * std::f32::consts::TAU).sin(), -0.2 * (attack * pi).sin()))
        * Mat4::from_rotation_y((side * (45.0 - (attack * attack * pi).sin() * 20.0)).to_radians())
        * Mat4::from_rotation_z((side * (root * pi).sin() * -20.0).to_radians())
        * Mat4::from_rotation_x(((root * pi).sin() * -80.0).to_radians())
        * Mat4::from_rotation_y((side * -45.0f32).to_radians())
}

// Swing animation's swings in its place. 1: no slide across, half the roll. 2: a short chop (half the drop, no
// turn). 3: a sword in the right hand held across, blocking, and swung with a roll; anything else as in 1 with the
// full roll.
fn styled_swing(style: u32, side: f32, attack: f32, sword: bool) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    let (pi, root) = (std::f32::consts::PI, attack.sqrt());
    let (lift, wave) = ((attack * attack * pi).sin(), (root * pi).sin());
    let r = |axis: Vec3, deg: f32| Mat4::from_axis_angle(axis, deg.to_radians());
    let back = r(Vec3::Y, side * -45.0);
    match style {
        2 => r(Vec3::Y, side * 45.0) * r(Vec3::Z, side * wave * -10.0) * r(Vec3::X, wave * -40.0) * back,
        3 if sword && side > 0.0 => {
            Mat4::from_translation(Vec3::new(0.0, 0.1, -0.2))
                * r(Vec3::Y, side * 65.0)
                * r(Vec3::X, side * -100.0)
                * r(Vec3::Z, side * 20.0)
                * r(Vec3::Y, side * -220.0)
                * r(Vec3::Z, side * wave * -40.0)
                * back
        }
        3 => r(Vec3::Y, side * (45.0 - lift * 20.0)) * r(Vec3::Z, side * wave * -20.0) * r(Vec3::X, wave * -80.0) * back,
        _ => r(Vec3::Y, side * (45.0 - lift * 20.0)) * r(Vec3::Z, side * wave * -10.0) * r(Vec3::X, wave * -80.0) * back,
    }
}

// What turns a held item's pose from the game's swing into Swing animation's (None with the game's own): the game's
// swing undone, the chosen one after.
pub fn swing_swap(left: bool, attack: f32, sword: bool) -> Option<glam::Mat4> {
    let style = SWING.load(Ordering::Relaxed);
    let side = if left { -1.0 } else { 1.0 };
    (style != 0).then(|| game_swing(side, attack).inverse() * styled_swing(style, side, attack, sword))
}

thread_local! {
    // An arm is being drawn again in its new pose.
    static POSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub fn posing() -> bool {
    POSING.with(std::cell::Cell::get)
}
pub fn set_posing(v: bool) {
    POSING.with(|p| p.set(v));
}
