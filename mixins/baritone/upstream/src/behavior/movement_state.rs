// baritone.api.utils.input.Input, baritone.api.pathing.movement.MovementStatus and
// baritone.pathing.movement.MovementState / MovementOption: what a movement asks of the player in one
// tick (the keys to hold, the aim to turn to) and how far along it is.
use super::baritone_rotation::Rotation;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    ClickLeft,
    ClickRight,
    Jump,
    Sneak,
    Sprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementStatus {
    Prepping,
    Waiting,
    Running,
    Success,
    Unreachable,
    #[allow(dead_code)]
    Failed,
    #[allow(dead_code)]
    Canceled,
}

impl MovementStatus {
    pub fn is_complete(self) -> bool {
        matches!(self, Self::Success | Self::Unreachable | Self::Failed | Self::Canceled)
    }
}

/// An aim, and whether the player has to be turned to it at once (`forceRotations`) or may be turned
/// there smoothly.
#[derive(Clone, Copy, Debug, Default)]
pub struct MovementTarget {
    pub rotation: Option<Rotation>,
    pub force_rotations: bool,
}

impl MovementTarget {
    pub fn new(rotation: Rotation, force_rotations: bool) -> Self {
        Self { rotation: Some(rotation), force_rotations }
    }
}

#[derive(Clone, Debug)]
pub struct MovementState {
    pub status: MovementStatus,
    pub target: MovementTarget,
    pub inputs: HashMap<Input, bool>,
}

impl MovementState {
    pub fn new() -> Self {
        Self { status: MovementStatus::Prepping, target: MovementTarget::default(), inputs: HashMap::new() }
    }

    pub fn set_status(&mut self, status: MovementStatus) -> &mut Self {
        self.status = status;
        self
    }

    pub fn set_target(&mut self, target: MovementTarget) -> &mut Self {
        self.target = target;
        self
    }

    pub fn set_input(&mut self, input: Input, forced: bool) -> &mut Self {
        self.inputs.insert(input, forced);
        self
    }
}

impl Default for MovementState {
    fn default() -> Self {
        Self::new()
    }
}

/// One way of holding the movement keys, with the motion it gives (MovementOption).
pub struct MovementOption {
    pub input1: Option<Input>,
    pub input2: Option<Input>,
    pub motion_x: f32,
    pub motion_z: f32,
}

impl MovementOption {
    const SPRINT_MULTIPLIER: f32 = 1.3;

    pub fn set_inputs(&self, state: &mut MovementState) {
        if let Some(i) = self.input1 {
            state.set_input(i, true);
        }
        if let Some(i) = self.input2 {
            state.set_input(i, true);
        }
    }

    pub fn distance_to_sq(&self, other_x: f32, other_z: f32) -> f32 {
        (self.motion_x - other_x).abs() + (self.motion_z - other_z).abs()
    }

    pub fn get_options(motion_x: f32, motion_z: f32, can_sprint: bool) -> [MovementOption; 8] {
        let (fx, fz) = if can_sprint { (motion_x * Self::SPRINT_MULTIPLIER, motion_z * Self::SPRINT_MULTIPLIER) } else { (motion_x, motion_z) };
        let one = |i, x, z| MovementOption { input1: Some(i), input2: None, motion_x: x, motion_z: z };
        let two = |i, j, x, z| MovementOption { input1: Some(i), input2: Some(j), motion_x: x, motion_z: z };
        [
            one(Input::MoveForward, fx, fz),
            one(Input::MoveBack, -motion_x, -motion_z),
            one(Input::MoveLeft, -motion_z, motion_x),
            one(Input::MoveRight, motion_z, -motion_x),
            two(Input::MoveForward, Input::MoveLeft, fx - motion_z, fz + motion_x),
            two(Input::MoveForward, Input::MoveRight, fx + motion_z, fz - motion_x),
            two(Input::MoveBack, Input::MoveLeft, -motion_x - motion_z, -motion_z + motion_x),
            two(Input::MoveBack, Input::MoveRight, -motion_x + motion_z, -motion_z - motion_x),
        ]
    }
}
