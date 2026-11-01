// How a look moves toward where it is going, a tick at a time, shared by the modules that aim. Degrees.

const NORMAL_SPEED: f32 = 45.0;
const SMOOTH_SHARE: f32 = 0.35;
const SMOOTH_LEAST: f32 = 1.0;

fn toward(from: f32, delta: f32, share: f32, least: f32, limit: f32) -> f32 {
    let wanted = (delta.abs() * share).max(least).min(delta.abs()).min(limit);
    from + wanted.copysign(delta)
}

// The next (yaw, pitch). Snap arrives at once; Normal turns at a steady speed; Smooth covers a share of what is left;
// Custom covers (1 - smoothing) of what is left, at most its speeds.
pub(super) fn step(mode: &str, from: (f32, f32), to: (f32, f32), yaw_speed: f32, pitch_speed: f32, smoothing: f32) -> (f32, f32) {
    if !from.0.is_finite() || !from.1.is_finite() || !to.0.is_finite() || !to.1.is_finite() {
        return from;
    }
    let dy = (to.0 - from.0 + 180.0).rem_euclid(360.0) - 180.0;
    let dp = to.1 - from.1;
    let (yaw, pitch) = match mode {
        "Snap" => (from.0 + dy, to.1),
        "Normal" => (toward(from.0, dy, 1.0, 0.0, NORMAL_SPEED), toward(from.1, dp, 1.0, 0.0, NORMAL_SPEED)),
        "Smooth" => (toward(from.0, dy, SMOOTH_SHARE, SMOOTH_LEAST, f32::MAX), toward(from.1, dp, SMOOTH_SHARE, SMOOTH_LEAST, f32::MAX)),
        _ => {
            let share = 1.0 - smoothing.clamp(0.0, 0.9);
            (toward(from.0, dy, share, SMOOTH_LEAST, yaw_speed.max(0.0)), toward(from.1, dp, share, SMOOTH_LEAST, pitch_speed.max(0.0)))
        }
    };
    (yaw, pitch.clamp(-90.0, 90.0))
}

// A step toward a look that is itself moving: `was` is where it was last tick, and the look first moves as far as it
// did, so a mode's smoothing eats into how far behind you are and not into keeping up. Normal and Custom still turn no
// faster than their speeds.
pub(super) fn track(mode: &str, from: (f32, f32), to: (f32, f32), was: Option<(f32, f32)>, yaw_speed: f32, pitch_speed: f32, smoothing: f32) -> (f32, f32) {
    let wrap = |a: f32| (a + 180.0).rem_euclid(360.0) - 180.0;
    let carried = was.map_or(from, |w| (from.0 + wrap(to.0 - w.0), from.1 + (to.1 - w.1)));
    let next = step(mode, carried, to, yaw_speed, pitch_speed, smoothing);
    let (most_yaw, most_pitch) = match mode {
        "Normal" => (NORMAL_SPEED, NORMAL_SPEED),
        "Custom" => (yaw_speed.max(0.0), pitch_speed.max(0.0)),
        _ => return next,
    };
    (from.0 + wrap(next.0 - from.0).clamp(-most_yaw, most_yaw), (from.1 + (next.1 - from.1).clamp(-most_pitch, most_pitch)).clamp(-90.0, 90.0))
}

// The way a (yaw, pitch) looks, length 1. Yaw 0 faces south (+z).
pub(super) fn direction(look: (f32, f32)) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(look.0).to_radians(), f64::from(look.1).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_arrives_at_once() {
        assert_eq!(step("Snap", (0.0, 0.0), (90.0, 30.0), 1.0, 1.0, 0.5), (90.0, 30.0));
    }

    #[test]
    fn normal_turns_at_a_steady_speed_and_stops_on_the_target() {
        assert_eq!(step("Normal", (0.0, 0.0), (90.0, 10.0), 1.0, 1.0, 0.0), (45.0, 10.0));
    }

    #[test]
    fn turns_take_the_short_way_round() {
        let (yaw, _) = step("Custom", (179.0, 0.0), (-179.0, 0.0), 180.0, 180.0, 0.0);
        assert!((yaw - 181.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_reaches_the_target_without_passing_it() {
        let mut look = (0.0, 0.0);
        for _ in 0..60 {
            look = step("Smooth", look, (120.0, -40.0), 0.0, 0.0, 0.0);
            assert!(look.0 <= 120.001 && look.1 >= -40.001);
        }
        assert!((look.0 - 120.0).abs() < 1e-3 && (look.1 + 40.0).abs() < 1e-3);
    }

    #[test]
    fn custom_keeps_its_separate_speeds() {
        assert_eq!(step("Custom", (0.0, 0.0), (90.0, 60.0), 20.0, 5.0, 0.0), (20.0, 5.0));
    }

    #[test]
    fn tracking_stays_on_a_moving_look() {
        for mode in ["Snap", "Normal", "Smooth", "Custom"] {
            let (mut look, mut was) = ((0.0, 0.0), None);
            for n in 0..60 {
                let to = (n as f32 * 6.0, 10.0);
                look = track(mode, look, to, was, 60.0, 60.0, 0.5);
                was = Some(to);
            }
            assert!((look.0 - 354.0).abs() < 1e-2 && (look.1 - 10.0).abs() < 1e-2, "{mode}: {look:?}");
        }
    }

    #[test]
    fn tracking_only_smooths_what_is_behind() {
        // The target look moves 6 degrees and you are 90 behind: 6 kept up, then Smooth's share of the rest.
        let look = track("Smooth", (0.0, 0.0), (96.0, 0.0), Some((90.0, 0.0)), 0.0, 0.0, 0.0);
        assert!((look.0 - (6.0 + 90.0 * SMOOTH_SHARE)).abs() < 1e-3);
    }

    #[test]
    fn tracking_keeps_to_the_speed_limit() {
        assert_eq!(track("Custom", (0.0, 0.0), (90.0, 60.0), Some((80.0, 50.0)), 20.0, 5.0, 0.0), (20.0, 5.0));
        assert_eq!(track("Normal", (0.0, 0.0), (170.0, 0.0), Some((100.0, 0.0)), 0.0, 0.0, 0.0), (45.0, 0.0));
    }

    #[test]
    fn tracking_a_new_target_is_a_plain_step() {
        assert_eq!(track("Smooth", (0.0, 0.0), (40.0, 0.0), None, 0.0, 0.0, 0.0), step("Smooth", (0.0, 0.0), (40.0, 0.0), 0.0, 0.0, 0.0));
    }

    #[test]
    fn direction_matches_the_game_axes() {
        let south = direction((0.0, 0.0));
        assert!((south[2] - 1.0).abs() < 1e-9);
        assert!((direction((0.0, 90.0))[1] + 1.0).abs() < 1e-9);
    }
}
