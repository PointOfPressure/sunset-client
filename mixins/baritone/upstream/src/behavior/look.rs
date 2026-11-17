// baritone.behavior.LookBehavior: where the player faces. A movement or a process asks for a rotation with
// `update_target`; the behavior decides how it is shown (Target.Mode) and applies it around the player's own
// update: `pre` before the motion reads the rotation, `post` once the tick has gone to the server. CLIENT leaves
// the camera turned, SERVER hands the camera back to the player (freeLook), NONE touches nothing.
//
// The Java works on the player entity; this works on rotations and returns the one to set, so the game side is two
// small calls (`baritone::look_pre`, `baritone::look_post`) and everything here can be tested on paper.
use super::baritone_aim::{AimProcessor, AimSettings};
use super::baritone_rotation::Rotation;
use super::baritone_settings::Settings;
use std::collections::VecDeque;

/// `Target.Mode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Set client-side and visible to the player.
    Client,
    /// Set for the tick only, so the server sees it and the player's view does not move.
    Server,
    /// Unaffected on both.
    None,
}

impl Mode {
    /// `Target.Mode.resolve`.
    pub fn resolve(settings: &Settings, fall_flying: bool, swimming: bool, block_interact: bool) -> Mode {
        let anti_cheat = settings.flag("antiCheatCompatibility");
        let block_free_look = settings.flag("blockFreeLook");
        if fall_flying {
            // Always need to set angles while flying.
            return if settings.flag("elytraFreeLook") { Mode::Server } else { Mode::Client };
        }
        if settings.flag("allowSwimming") && swimming {
            // Pitch steers while swimming, so the angles are always set. Mining needs the crosshair on the block, or
            // nothing is ever hit (the same rule as the freeLook branch below).
            if block_interact && !block_free_look {
                return Mode::Client;
            }
            return if settings.flag("freeLook") { Mode::Server } else { Mode::Client };
        }
        if settings.flag("freeLook") {
            // A block interaction needs the player's rotation set somehow, or the crosshair would be wherever the
            // player happens to look; settle for setting it silently.
            if block_interact {
                return if block_free_look { Mode::Server } else { Mode::Client };
            }
            return if anti_cheat { Mode::Server } else { Mode::None };
        }
        // All the freeLook settings are off, so the angles are set.
        Mode::Client
    }
}

struct Target {
    rotation: Rotation,
    mode: Mode,
}

#[derive(Default)]
pub struct LookBehavior {
    /// The current look target, if any.
    target: Option<Target>,
    /// The rotation known to the server: what the last tick of the player was sent with.
    server_rotation: Option<Rotation>,
    /// The player's rotation before `pre` set the target's, to put back (freeLook).
    prev_rotation: Option<Rotation>,
    smooth_yaw: VecDeque<f32>,
    smooth_pitch: VecDeque<f32>,
}

impl LookBehavior {
    /// `updateTarget`.
    pub fn update_target(&mut self, rotation: Rotation, mode: Mode) {
        self.target = Some(Target { rotation, mode });
    }

    /// `getEffectiveRotation`: with freeLook the player's view is not what the server knows, so the movements read the
    /// server's.
    pub fn effective_rotation(&self, settings: &Settings) -> Option<Rotation> {
        if settings.flag("freeLook") {
            self.server_rotation
        } else {
            None
        }
    }

    /// `onPlayerUpdate` PRE: the rotation to give the player now, if the target asks for one. `current` is the player's
    /// rotation as it is.
    pub fn pre(&mut self, current: Rotation, aim: &AimProcessor, aim_settings: &AimSettings, settings: &Settings) -> Option<Rotation> {
        if self.prev_rotation.is_some() {
            // Already applied this tick (the singleplayer and the remote tick both reach a hook).
            return None;
        }
        let target = self.target.as_ref()?;
        if target.mode == Mode::None {
            return None;
        }
        self.prev_rotation = Some(current);
        let prev = self.effective_rotation(settings).unwrap_or(current);
        Some(aim.peek_rotation(target.rotation, prev, aim_settings))
    }

    /// `onPlayerUpdate` POST, once the tick has been sent: `current` is the rotation the tick used. The rotation to put
    /// back, if the view has to change. The target is spent.
    pub fn post(&mut self, current: Rotation, fall_flying: bool, settings: &Settings) -> Option<Rotation> {
        // `onSendPacket`: the server knows the rotation the tick was sent with.
        self.server_rotation = Some(current);
        let target = self.target.take()?;
        let prev = self.prev_rotation.take()?;
        let ticks = settings.int("smoothLookTicks").max(0) as usize;
        push_capped(&mut self.smooth_yaw, target.rotation.yaw, ticks);
        push_capped(&mut self.smooth_pitch, target.rotation.pitch, ticks);
        if target.mode == Mode::Server {
            return Some(prev);
        }
        if if fall_flying { settings.flag("elytraSmoothLook") } else { settings.flag("smoothLook") } {
            let yaw = average(&self.smooth_yaw).unwrap_or(prev.yaw);
            let pitch = if fall_flying { average(&self.smooth_pitch).unwrap_or(prev.pitch) } else { current.pitch };
            return Some(Rotation::new(yaw, pitch));
        }
        None
    }

    /// `onWorldEvent`.
    pub fn on_world_event(&mut self) {
        self.server_rotation = None;
        self.target = None;
    }
}

fn push_capped(buffer: &mut VecDeque<f32>, value: f32, cap: usize) {
    buffer.push_back(value);
    while buffer.len() > cap {
        buffer.pop_front();
    }
}

fn average(buffer: &VecDeque<f32>) -> Option<f32> {
    (!buffer.is_empty()).then(|| (buffer.iter().map(|v| f64::from(*v)).sum::<f64>() / buffer.len() as f64) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aim() -> (AimProcessor, AimSettings) {
        (AimProcessor::new(1), AimSettings { random_looking: 0.0, random_looking_113: 0.0, sensitivity: 0.5 })
    }

    fn settings(changes: &[(&str, &str)]) -> Settings {
        let mut s = Settings::default();
        for (name, value) in changes {
            s.set(name, value).unwrap();
        }
        s
    }

    #[test]
    fn by_default_walking_is_silent_and_block_work_is_seen() {
        let s = settings(&[]);
        assert_eq!(Mode::resolve(&s, false, false, false), Mode::Server);
        assert_eq!(Mode::resolve(&s, false, false, true), Mode::Client);
        assert_eq!(Mode::resolve(&s, true, false, false), Mode::Server);
    }

    #[test]
    fn the_free_look_settings_pick_the_mode() {
        assert_eq!(Mode::resolve(&settings(&[("freeLook", "false")]), false, false, false), Mode::Client);
        assert_eq!(Mode::resolve(&settings(&[("antiCheatCompatibility", "false")]), false, false, false), Mode::None);
        assert_eq!(Mode::resolve(&settings(&[("blockFreeLook", "true")]), false, false, true), Mode::Server);
        assert_eq!(Mode::resolve(&settings(&[("elytraFreeLook", "false")]), true, false, false), Mode::Client);
        let swim = settings(&[("allowSwimming", "true")]);
        assert_eq!(Mode::resolve(&swim, false, true, true), Mode::Client);
        assert_eq!(Mode::resolve(&swim, false, true, false), Mode::Server);
    }

    #[test]
    fn a_server_target_is_applied_for_the_tick_and_the_view_comes_back() {
        let (aim, aim_settings) = aim();
        let s = settings(&[]);
        let mut look = LookBehavior::default();
        let view = Rotation::new(10.0, 5.0);
        look.update_target(Rotation::new(90.0, 5.0), Mode::Server);
        let applied = look.pre(view, &aim, &aim_settings, &s).expect("a rotation is set");
        assert!((applied.yaw - 90.0).abs() < 1.0, "{}", applied.yaw);
        let back = look.post(applied, false, &s).expect("the view is given back");
        assert_eq!((back.yaw, back.pitch), (10.0, 5.0));
        assert_eq!(look.effective_rotation(&s).map(|r| r.yaw), Some(applied.yaw));
        // The target is spent: nothing happens until the next one.
        assert!(look.pre(view, &aim, &aim_settings, &s).is_none());
    }

    #[test]
    fn a_client_target_leaves_the_camera_turned() {
        let (aim, aim_settings) = aim();
        let s = settings(&[]);
        let mut look = LookBehavior::default();
        look.update_target(Rotation::new(40.0, 0.0), Mode::Client);
        let applied = look.pre(Rotation::new(0.0, 0.0), &aim, &aim_settings, &s).unwrap();
        assert!(look.post(applied, false, &s).is_none());
    }

    #[test]
    fn no_target_and_no_mode_change_nothing() {
        let (aim, aim_settings) = aim();
        let s = settings(&[]);
        let mut look = LookBehavior::default();
        assert!(look.pre(Rotation::new(0.0, 0.0), &aim, &aim_settings, &s).is_none());
        look.update_target(Rotation::new(40.0, 0.0), Mode::None);
        assert!(look.pre(Rotation::new(0.0, 0.0), &aim, &aim_settings, &s).is_none());
        assert!(look.post(Rotation::new(0.0, 0.0), false, &s).is_none());
    }

    #[test]
    fn smooth_look_averages_the_last_targets_and_keeps_the_pitch() {
        let (aim, aim_settings) = aim();
        let s = settings(&[("smoothLook", "true"), ("smoothLookTicks", "2")]);
        let mut look = LookBehavior::default();
        let mut last = None;
        for yaw in [0.0_f32, 30.0, 60.0] {
            look.update_target(Rotation::new(yaw, 0.0), Mode::Client);
            let applied = look.pre(Rotation::new(0.0, 0.0), &aim, &aim_settings, &s).unwrap();
            last = look.post(Rotation::new(applied.yaw, 3.0), false, &s);
        }
        let shown = last.expect("smoothed");
        assert!((shown.yaw - 45.0).abs() < 1e-4, "{}", shown.yaw);
        assert_eq!(shown.pitch, 3.0);
    }

    #[test]
    fn the_server_rotation_is_forgotten_with_the_world() {
        let s = settings(&[]);
        let mut look = LookBehavior::default();
        look.post(Rotation::new(5.0, 5.0), false, &s);
        assert!(look.effective_rotation(&s).is_some());
        look.on_world_event();
        assert!(look.effective_rotation(&s).is_none());
        assert!(look.effective_rotation(&settings(&[("freeLook", "false")])).is_none());
    }
}
