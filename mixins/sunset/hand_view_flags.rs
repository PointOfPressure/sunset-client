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
// Swing animation (an index into SWINGS; 0 the game's own), Slow swing, No equip drop.
pub static SWING: AtomicU32 = AtomicU32::new(0);
pub static SLOW_SWING: AtomicBool = AtomicBool::new(false);
pub static NO_EQUIP_DROP: AtomicBool = AtomicBool::new(false);
// Kill Aura's Legacy AutoBlock has someone to hit: the sword in the main hand shows the block (Swing animation 3's).
pub static SWORD_BLOCK: AtomicBool = AtomicBool::new(false);
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

// ---- Animations: 1.9+ swings in the game's own pose of the held item, and 1.8 swings and blocks in 1.8's.
//
// 1.9+: the game poses a held item as the arm's place, then its swing (`game_swing`), then the item model's first-person
// display transform. A 1.9+ animation swaps that swing for its own.
//
// 1.8: a flat item (a sword, a tool, any item drawn from its sprite with the game's display) is posed as 1.8.9 posed
// it, as Horizon's own Combat feel Legacy does (first_person::legacy_item_pose): the arm's place (the same as the
// game's), 1.8's turn of the item in the hand and the swing's rotations, scaled to 0.4, the animation (a block's turn
// of the sword included), then 1.8's first-person display; the game's display of the item is taken back out, so the
// item ends where 1.8 put it. Whatever the game drew first (1.9+, or Horizon's 1.8.9 with Combat feel Legacy) is
// undone. Blocks, special models and a resource pack's own displays keep the game's pose.
//
// Every animation is written in degrees and blocks of the pose it is in. `w` is the swing's wave (sin of pi times the
// square root of its progress: up and back down, fast at first), `l` its lift (sin of pi times the progress squared:
// slow at first); `side` is 1 for the right hand and -1 for the left.

use glam::{Mat4, Vec3};

// The choices, in the order the menu shows them; the settings store an index into them.
pub const SWINGS: &[&str] = &["Vanilla", "1", "2", "3", "Punch", "Chop", "Slide", "Wave", "Stab", "Spin", "Lazy", "Smooth", "Push"];
pub const SWINGS_18: &[&str] = &["1.8", "Punch", "Shove", "Smooth", "Chop", "Slide", "Wave", "Stab", "Spin", "Lazy", "Push", "Swank"];
pub const BLOCKS_18: &[&str] = &[
    "1.8", "1.7", "Push", "Slide", "Swank", "Swirl", "Tap", "Stab", "Spin", "Smooth", "Down", "Lift", "Wave", "Chop", "Roll",
];
pub const BLOCKS: &[&str] = &["Across", "1.8 style"];

// Hand View's Animations: 1.8 (else 1.9+), and the index of each choice.
pub static LEGACY: AtomicBool = AtomicBool::new(false);
pub static SWING_18: AtomicU32 = AtomicU32::new(0);
pub static BLOCK_18: AtomicU32 = AtomicU32::new(0);
pub static BLOCK: AtomicU32 = AtomicU32::new(0);

// The index of `name` among `choices` (0 when it is none of them).
pub fn index_of(choices: &[&str], name: &str) -> u32 {
    choices.iter().position(|c| *c == name).unwrap_or(0) as u32
}

fn r(axis: Vec3, deg: f32) -> Mat4 {
    let axis = axis.normalize_or_zero();
    if axis == Vec3::ZERO { Mat4::IDENTITY } else { Mat4::from_axis_angle(axis, deg.to_radians()) }
}

fn t(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

fn sc(k: f32) -> Mat4 {
    Mat4::from_scale(Vec3::splat(k))
}

fn wave(s: f32) -> f32 {
    (s.max(0.0).sqrt() * std::f32::consts::PI).sin()
}

fn lift(s: f32) -> f32 {
    (s * s * std::f32::consts::PI).sin()
}

// The game's swing of a held item at progress `attack` (0 to 1), after its place in the hand: across, up and in,
// turned about y, z, x and back about y.
fn game_swing(side: f32, attack: f32) -> Mat4 {
    let (pi, root) = (std::f32::consts::PI, attack.max(0.0).sqrt());
    t(side * -0.4 * (root * pi).sin(), 0.2 * (root * std::f32::consts::TAU).sin(), -0.2 * (attack * pi).sin())
        * r(Vec3::Y, side * (45.0 - lift(attack) * 20.0))
        * r(Vec3::Z, side * wave(attack) * -20.0)
        * r(Vec3::X, wave(attack) * -80.0)
        * r(Vec3::Y, side * -45.0)
}

// The 1.9+ swings in the game's swing's place (`style` indexes SWINGS; 0 is the game's own). 1: no slide across, half
// the roll. 2: a short chop. 3: a sword in the right hand held across, blocking, swung with a roll (the Across block).
fn styled_swing(style: u32, side: f32, s: f32, sword: bool) -> Mat4 {
    let (w, l) = (wave(s), lift(s));
    let (pi, root) = (std::f32::consts::PI, s.max(0.0).sqrt());
    let back = r(Vec3::Y, side * -45.0);
    let turn = r(Vec3::Y, side * 45.0);
    match style {
        1 => r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * w * -10.0) * r(Vec3::X, w * -80.0) * back,
        2 => turn * r(Vec3::Z, side * w * -10.0) * r(Vec3::X, w * -40.0) * back,
        3 if sword && side > 0.0 => across(side, w),
        3 => r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * w * -20.0) * r(Vec3::X, w * -80.0) * back,
        // Punch: straight out and back, the blade barely dipping.
        4 => t(0.0, 0.02 * w, -0.32 * w) * r(Vec3::X, -12.0 * w),
        // Chop: straight down, no twist.
        5 => turn * r(Vec3::X, -95.0 * w) * back,
        // Slide: across the screen with half the drop.
        6 => t(side * -0.5 * w, 0.0, -0.05 * w) * r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::X, -40.0 * w) * back,
        // Wave: a sideways sweep.
        7 => turn * r(Vec3::Z, side * w * -55.0) * r(Vec3::X, w * -15.0) * back,
        // Stab: the tip thrust forward, raised a little.
        8 => t(side * -0.05 * w, 0.08 * w, -0.42 * w) * r(Vec3::X, 22.0 * w),
        // Spin: one turn over through the swing.
        9 => turn * r(Vec3::X, -360.0 * s) * back,
        // Lazy: the game's swing at half its size.
        10 => {
            t(side * -0.2 * (root * pi).sin(), 0.1 * (root * std::f32::consts::TAU).sin(), -0.1 * (s * pi).sin())
                * r(Vec3::Y, side * (45.0 - l * 10.0))
                * r(Vec3::Z, side * w * -10.0)
                * r(Vec3::X, w * -40.0)
                * back
        }
        // Smooth: the game's swing eased in (the lift, not the wave, drives it).
        11 => r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * l * -20.0) * r(Vec3::X, l * -80.0) * back,
        // Push: forward and down, the item tipped.
        12 => t(0.0, -0.1 * w, -0.26 * w) * r(Vec3::X, -32.0 * w),
        _ => game_swing(side, s),
    }
}

// The Across block: a sword held across in front, swung with a roll.
fn across(side: f32, w: f32) -> Mat4 {
    t(0.0, 0.1, -0.2)
        * r(Vec3::Y, side * 65.0)
        * r(Vec3::X, side * -100.0)
        * r(Vec3::Z, side * 20.0)
        * r(Vec3::Y, side * -220.0)
        * r(Vec3::Z, side * w * -40.0)
        * r(Vec3::Y, side * -45.0)
}

// What turns a held item's pose from the game's swing (at `attack`) into Swing animation's at `s` (None with the game's
// own): the game's swing undone, the chosen one after. `block`: a blocking sword, held Across whatever the style.
pub fn swing_swap(left: bool, attack: f32, s: f32, sword: bool, block: bool) -> Option<Mat4> {
    let style = if block && sword { 3 } else { SWING.load(Ordering::Relaxed) };
    let side = if left { -1.0 } else { 1.0 };
    (style != 0).then(|| game_swing(side, attack).inverse() * styled_swing(style, side, s, sword))
}

// Hand View's own swing for its animations, 0 to 1: each swing plays out in full (its length in ticks) before the next
// starts, and a hit made while one plays starts the next right after it. The game starts a swing again from nothing at
// a hit past half the last one (1.8 did too), which shows as a jump back on every hit when hits come fast. `started`:
// the game's count of swings started; `time` in seconds; called once a frame (the main hand's item).
#[derive(Clone, Copy, Debug, Default)]
struct Clock {
    started: u64,
    progress: f32,
    playing: bool,
    queued: bool,
    at: f32,
}

static CLOCK: std::sync::Mutex<Clock> = std::sync::Mutex::new(Clock { started: 0, progress: 0.0, playing: false, queued: false, at: 0.0 });

fn clock_step(c: &mut Clock, started: u64, ticks: i32, time: f32) -> f32 {
    let dt = (time - c.at).clamp(0.0, 0.1);
    c.at = time;
    if started != c.started {
        c.started = started;
        if c.playing {
            c.queued = true;
        } else {
            (c.playing, c.progress) = (true, 0.0);
            return 0.0;
        }
    }
    if c.playing {
        c.progress += dt / (ticks.max(1) as f32 / 20.0);
        if c.progress >= 1.0 {
            if c.queued {
                c.queued = false;
                c.progress = (c.progress - 1.0).min(0.5);
            } else {
                (c.playing, c.progress) = (false, 0.0);
            }
        }
    }
    if c.playing { c.progress } else { 0.0 }
}

pub fn swing_clock(started: u64, ticks: i32, time: f32) -> f32 {
    clock_step(&mut CLOCK.lock().unwrap_or_else(|e| e.into_inner()), started, ticks, time)
}

// ---- 1.8

// 1.8's turn of an item in the hand and its swing's rotations, scaled to 0.4 (`transformFirstPersonItem` after the
// arm's place).
fn hand_18(s: f32) -> Mat4 {
    r(Vec3::Y, 45.0) * r(Vec3::Y, lift(s) * -20.0) * r(Vec3::Z, wave(s) * -20.0) * r(Vec3::X, wave(s) * -80.0) * sc(0.4)
}

// 1.8's swing across, up and in (`doItemUsedTransformations`).
fn used_18(s: f32) -> Mat4 {
    let root = s.max(0.0).sqrt();
    t(-0.4 * (root * std::f32::consts::PI).sin(), 0.2 * (root * std::f32::consts::TAU).sin(), -0.2 * (s * std::f32::consts::PI).sin())
}

// 1.8's sword block (`doBlockTransformations`).
fn block_18() -> Mat4 {
    t(-0.5, 0.2, 0.0) * r(Vec3::Y, 30.0) * r(Vec3::X, -80.0) * r(Vec3::Y, 60.0)
}

// What follows 1.8's pose of a flat item, Horizon's Combat feel Legacy's (first_person::legacy_item_pose): 1.8's
// first-person display (moved 0, 4, 2 pixels, turned -135 about y and 25 about z, scaled 1.7) and RenderItem's 0.5,
// then the game's flat-item display (which the renderer puts after the pose) undone.
fn native_tail() -> Mat4 {
    t(0.0, 0.25, 0.125) * r(Vec3::Y, -135.0) * r(Vec3::Z, 25.0) * sc(1.7) * sc(0.5) * super::modern_flat_display(false).inverse()
}

// Hand View's: the same, after RenderItem's doubling of an item that is not a 3D model (`preTransform`), which Horizon's
// leaves out (its flat items are half 1.8's size).
fn tail_18() -> Mat4 {
    sc(2.0) * native_tail()
}

// The 1.8 swings of a held item (`style` indexes SWINGS_18), from the arm's place.
fn swing_18(style: u32, s: f32) -> Mat4 {
    let (w, l) = (wave(s), lift(s));
    let still = hand_18(0.0);
    match style {
        // Punch: out and back, upright.
        1 => t(0.0, 0.0, -0.35 * w) * still * r(Vec3::X, -15.0 * w),
        // Shove: the swing's turn in place, no slide.
        2 => hand_18(s),
        // Smooth: half the slide, an eased turn.
        3 => {
            let root = s.max(0.0).sqrt();
            t(-0.2 * (root * std::f32::consts::PI).sin(), 0.1 * (root * std::f32::consts::TAU).sin(), -0.1 * (s * std::f32::consts::PI).sin())
                * r(Vec3::Y, 45.0)
                * r(Vec3::Y, l * -10.0)
                * r(Vec3::Z, l * -15.0)
                * r(Vec3::X, l * -60.0)
                * sc(0.4)
        }
        // Chop: straight down.
        4 => still * r(Vec3::X, -75.0 * w),
        // Slide: across and tipped.
        5 => t(-0.3 * w, 0.0, 0.0) * still * r(Vec3::new(1.0, 0.5, 0.0), -50.0 * w),
        // Wave: a sideways sweep.
        6 => still * r(Vec3::Z, 40.0 * w) * r(Vec3::X, -20.0 * w),
        // Stab: thrust forward.
        7 => t(0.0, 0.05 * w, -0.45 * w) * still * r(Vec3::Y, -20.0 * w),
        // Spin: one turn about the blade's plane.
        8 => still * r(Vec3::Z, -360.0 * s),
        // Lazy: the 1.8 swing at half its size.
        9 => {
            let root = s.max(0.0).sqrt();
            t(-0.2 * (root * std::f32::consts::PI).sin(), 0.1 * (root * std::f32::consts::TAU).sin(), -0.1 * (s * std::f32::consts::PI).sin())
                * r(Vec3::Y, 45.0)
                * r(Vec3::Y, l * -10.0)
                * r(Vec3::Z, w * -10.0)
                * r(Vec3::X, w * -40.0)
                * sc(0.4)
        }
        // Push: forward and down, tipped.
        10 => t(0.0, -0.1 * w, -0.25 * w) * still * r(Vec3::X, -30.0 * w),
        // Swank: a diagonal swirl.
        11 => still * r(Vec3::new(-1.0, 0.0, 1.2), -45.0 * w) * r(Vec3::new(1.0, -0.5 * w, 0.0), -35.0 * w),
        _ => used_18(s) * hand_18(s),
    }
}

// The 1.8 blocks of a sword (`style` indexes BLOCKS_18), from the arm's place; `time` in seconds (Spin).
fn block_anim_18(style: u32, s: f32, time: f32) -> Mat4 {
    let (w, l) = (wave(s), lift(s));
    let still = hand_18(0.0);
    let block = block_18();
    match style {
        // 1.7: the block swung with the whole 1.8 swing (blockhitting).
        1 => hand_18(s) * block,
        // Push: the block thrust forward on a hit.
        2 => still * t(0.0, 0.0, -0.6 * w) * block,
        // Slide: tipped down and across on a hit.
        3 => still * r(Vec3::new(1.0, 0.6, 0.0), -45.0 * w) * block,
        // Swank: a diagonal swirl.
        4 => still * r(Vec3::new(-8.0, 0.0, 9.0), -30.0 * w) * r(Vec3::new(1.0, -0.5 * w, 0.0), -40.0 * w) * block,
        // Swirl: the sword turned about its own diagonal.
        5 => still * block * r(Vec3::new(1.0, 1.0, 0.0), -60.0 * w),
        // Tap: a short nod.
        6 => still * r(Vec3::X, -25.0 * w) * block,
        // Stab: up and forward.
        7 => still * t(0.0, 0.25 * w, -0.5 * w) * block,
        // Spin: turning over and over while it blocks.
        8 => still * block * r(Vec3::X, -(time * 180.0) % 360.0),
        // Smooth: an eased nod.
        9 => still * r(Vec3::Y, -15.0 * l) * r(Vec3::X, -35.0 * l) * block,
        // Down: held lower, nodding.
        10 => still * t(0.0, -0.25, 0.0) * r(Vec3::X, -30.0 * w) * block,
        // Lift: raised and rolled on a hit.
        11 => still * t(0.0, 0.3 * w, 0.0) * r(Vec3::Z, -20.0 * w) * block,
        // Wave: rocked sideways.
        12 => still * block * r(Vec3::Z, 35.0 * w),
        // Chop: a deep nod.
        13 => still * r(Vec3::X, -60.0 * w) * block,
        // Roll: turned about the arm.
        14 => still * block * r(Vec3::Y, -50.0 * w),
        _ => still * block,
    }
}

// How the game drew a right hand's flat item before Hand View changes it: the 1.9+ pose, or Horizon's own 1.8.9 one
// (Combat feel Legacy), blocking (a sword held up while use is held) or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    Modern,
    Legacy { block: bool },
}

// What the game put after the arm's place (the camera's and the arm's place are the same either way).
fn drawn(drawn: Drawn, attack: f32) -> Mat4 {
    match drawn {
        Drawn::Modern => game_swing(1.0, attack),
        Drawn::Legacy { block: false } => used_18(attack) * hand_18(attack) * native_tail(),
        Drawn::Legacy { block: true } => hand_18(attack) * block_18() * native_tail(),
    }
}

// A right hand's flat item posed the 1.8 way: `pose` is the game's (drawn as `how`, its swing at `attack`), `s` the
// swing Hand View's animation plays (swing_clock). 1.8 applies with Hand View's 1.8 Animations, with Horizon's Combat
// feel Legacy, or for a 1.8-style block; otherwise None. `view`: Hand View's own move of the hand, put where the hand
// is (before the item's turn in it).
pub fn legacy_pose(pose: Mat4, attack: f32, s: f32, block: bool, time: f32, how: Drawn, view: Mat4) -> Option<Mat4> {
    let native = matches!(how, Drawn::Legacy { .. });
    if !LEGACY.load(Ordering::Relaxed) && !native && !(block && BLOCK.load(Ordering::Relaxed) == 1) {
        return None;
    }
    let held = if block {
        block_anim_18(BLOCK_18.load(Ordering::Relaxed), s, time)
    } else {
        swing_18(SWING_18.load(Ordering::Relaxed), s)
    };
    let back = drawn(how, attack).inverse();
    back.is_finite().then(|| pose * back * view * held * tail_18())
}

// Hand View's move of a hand (Right/Left X, Y, Z, turns and scale), or none.
pub fn view_of(left: bool) -> Mat4 {
    match VIEW.read() {
        Ok(view) => view.map_or(Mat4::IDENTITY, |v| v[usize::from(left)]),
        Err(_) => Mat4::IDENTITY,
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;

    fn close(a: Mat4, b: Mat4) -> bool {
        a.abs_diff_eq(b, 1e-4)
    }

    #[test]
    fn the_swing_clock_plays_each_swing_out_and_chains_fast_hits() {
        let mut c = Clock::default();
        // A swing of 6 ticks (0.3 s) at 100 frames a second.
        let frames = |c: &mut Clock, started: u64, from: f32, n: usize| -> Vec<f32> {
            (0..n).map(|i| clock_step(c, started, 6, from + i as f32 * 0.01)).collect()
        };
        let one = frames(&mut c, 1, 0.0, 40);
        assert_eq!(one[0], 0.0);
        assert!(one.windows(2).take(29).all(|w| w[1] > w[0]), "it plays forward: {one:?}");
        assert_eq!(*one.last().unwrap(), 0.0, "and rests once played");
        // Hits every 0.1 s (a third of a swing): never a jump back, one long run.
        let mut c = Clock::default();
        let mut last = 0.0;
        let mut drops = 0;
        for i in 0..90 {
            let v = clock_step(&mut c, 1 + i / 10, 6, i as f32 * 0.01);
            if v + 0.5 < last {
                drops += 1;
            }
            last = v;
        }
        assert!(drops <= 3, "a swing ends before the next begins: {drops}");
    }

    #[test]
    fn every_animation_is_finite_through_the_swing() {
        for i in 0..=20 {
            let s = i as f32 / 20.0;
            for style in 0..SWINGS.len() as u32 {
                for side in [1.0, -1.0] {
                    assert!(styled_swing(style, side, s, true).is_finite(), "1.9+ {style} at {s}");
                }
            }
            for style in 0..SWINGS_18.len() as u32 {
                assert!(swing_18(style, s).is_finite(), "1.8 swing {style} at {s}");
            }
            for style in 0..BLOCKS_18.len() as u32 {
                assert!(block_anim_18(style, s, 3.7).is_finite(), "1.8 block {style} at {s}");
            }
        }
    }

    #[test]
    fn at_rest_the_swings_hold_still_and_the_game_s_own_is_unchanged() {
        assert!(close(game_swing(1.0, 0.0), Mat4::IDENTITY));
        for style in [1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
            assert!(close(styled_swing(style, 1.0, 0.0, false), Mat4::IDENTITY), "1.9+ {style}");
        }
        for style in 0..SWINGS_18.len() as u32 {
            assert!(close(swing_18(style, 0.0), hand_18(0.0)), "1.8 {}", SWINGS_18[style as usize]);
        }
        // Every 1.8 block but Spin rests in the 1.8 block.
        for style in (0..BLOCKS_18.len() as u32).filter(|&s| s != 8 && s != 10) {
            assert!(close(block_anim_18(style, 0.0, 0.0), hand_18(0.0) * block_18()), "block {}", BLOCKS_18[style as usize]);
        }
    }

    #[test]
    fn the_1_8_pose_matches_horizon_s_own_and_undoes_whatever_the_game_drew() {
        let arm = t(0.56, -0.52 - 0.3 * 0.6, -0.72);
        let s = 0.4;
        // The 1.8 swing and block are Horizon's Combat feel Legacy poses exactly.
        let native = |block: bool| {
            super::super::legacy_item_pose(false, 0.3, s, super::super::ItemPose::Swing, block).unwrap()
        };
        assert!(close(arm * drawn(Drawn::Legacy { block: false }, s), native(false)));
        assert!(close(arm * drawn(Drawn::Legacy { block: true }, s), native(true)));
        LEGACY.store(true, Ordering::Relaxed);
        SWING_18.store(0, Ordering::Relaxed);
        BLOCK_18.store(0, Ordering::Relaxed);
        // From the 1.9+ pose and from Horizon's own: the same 1.8 swing, at 1.8's size (twice Horizon's).
        let want = arm * used_18(s) * hand_18(s) * tail_18();
        let from_modern = legacy_pose(arm * game_swing(1.0, s), s, s, false, 0.0, Drawn::Modern, Mat4::IDENTITY).unwrap();
        let from_native = legacy_pose(native(false), s, s, false, 0.0, Drawn::Legacy { block: false }, Mat4::IDENTITY).unwrap();
        assert!(close(from_modern, want) && close(from_native, want));
        // A 1.8 block over Horizon's swing (Kill Aura's AutoBlock while use is not held), at Hand View's own swing.
        let blocked = legacy_pose(native(false), s, 0.0, true, 0.0, Drawn::Legacy { block: false }, Mat4::IDENTITY).unwrap();
        assert!(close(blocked, arm * hand_18(0.0) * block_18() * tail_18()));
        LEGACY.store(false, Ordering::Relaxed);
        BLOCK.store(0, Ordering::Relaxed);
        assert!(legacy_pose(arm * game_swing(1.0, s), s, s, true, 0.0, Drawn::Modern, Mat4::IDENTITY).is_none());
        BLOCK.store(1, Ordering::Relaxed);
        assert!(legacy_pose(arm * game_swing(1.0, s), s, s, true, 0.0, Drawn::Modern, Mat4::IDENTITY).is_some());
        BLOCK.store(0, Ordering::Relaxed);
        assert_eq!(index_of(BLOCKS_18, "Swank"), 4);
        assert_eq!(index_of(SWINGS, "3"), 3);
        assert_eq!(index_of(SWINGS, "nonsense"), 0);
    }
}
