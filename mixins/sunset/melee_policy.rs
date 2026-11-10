// The melee modules' rules that need no world (Rust's PR #16): when a borrowed hotbar slot goes back, distances to a
// box, whether a shield faces an attacker, attack speed, W Tap and Jump Reset's triggers.

// More living things than this nearby and the melee modules sit out the tick.
pub(super) const POPULATION_LIMIT: usize = 512;

// A hotbar slot a melee module took for a hit, and the one it goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SlotLease {
    pub owner: &'static str,
    pub previous: usize,
    pub selected: usize,
    pub until: u64,
    pub return_enabled: bool,
}

impl SlotLease {
    // The slot to go back to: never once you changed slot yourself.
    pub(super) fn restore(self, selected: usize, now: u64, still_owner: bool) -> Option<usize> {
        (still_owner && self.return_enabled && selected == self.selected && self.previous < 9 && now >= self.until)
            .then_some(self.previous)
    }
    pub(super) fn expired(self, selected: usize, now: u64, still_owner: bool) -> bool {
        !still_owner || selected != self.selected || now >= self.until
    }
}

// An item that is there and not broken.
pub(super) fn usable(count: u32, damage: u32, max_damage: u32) -> bool {
    count > 0 && (max_damage == 0 || damage < max_damage)
}

// From the eyes to the nearest point of a box.
pub(super) fn box_distance(eye: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    if !eye.iter().chain(lo.iter()).chain(hi.iter()).all(|n| n.is_finite()) || (0..3).any(|i| lo[i] > hi[i]) {
        return None;
    }
    Some((0..3).map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2)).sum::<f64>().sqrt())
}

// A raised shield facing the attacker within the shield's own angle (its blocks_attacks component).
pub(super) fn facing_attack(defender: [f64; 3], yaw: f32, attacker: [f64; 3], angle: f32) -> bool {
    if !defender.iter().chain(attacker.iter()).all(|n| n.is_finite()) || !yaw.is_finite() || !angle.is_finite() || angle < 0.0 {
        return false;
    }
    let dx = attacker[0] - defender[0];
    let dz = attacker[2] - defender[2];
    let length = dx.hypot(dz);
    if length < 1.0e-4 {
        return angle >= 90.0;
    }
    let y = f64::from(yaw).to_radians();
    let dot = (-y.sin() * dx + y.cos() * dz) / length;
    dot.clamp(-1.0, 1.0).acos().to_degrees() <= f64::from(angle.min(180.0))
}

// W Tap: a charged hit while you sprint forward at something alive.
pub(super) fn should_tap(sprinting: bool, forward: bool, strength: f32, living_target: bool) -> bool {
    sprinting && forward && living_target && strength.is_finite() && strength > 0.9
}

// A key W Tap or Jump Reset changed for a tick, after it: held while you hold it on the keyboard, or while something
// else held it before (a toggled sprint, Sprint's or Auto Walk's key); let go otherwise.
pub(super) fn key_back(physical_now: bool, held_before: bool, physical_before: bool) -> bool {
    physical_now || (held_before && !physical_before)
}

// S Tap's Spacing: how close to the edge of your reach it keeps the target before it backs off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spacing {
    Conservative,
    Balanced,
    Aggressive,
}

impl Spacing {
    pub(super) fn named(name: &str) -> Self {
        match name {
            "Conservative" => Self::Conservative,
            "Aggressive" => Self::Aggressive,
            _ => Self::Balanced,
        }
    }
    // How far inside your reach the gap may get (blocks) before S goes down.
    fn margin(self) -> f64 {
        match self {
            Self::Conservative => 0.0,
            Self::Balanced => 0.3,
            Self::Aggressive => 0.6,
        }
    }
    // How fast the gap must be closing (blocks a tick) for S to go down: Conservative backs off unless the gap is
    // clearly opening already.
    fn closing(self) -> f64 {
        match self {
            Self::Conservative => -0.05,
            Self::Balanced => 0.02,
            Self::Aggressive => 0.08,
        }
    }
}

// The gap to the target S Tap works from: your eyes to its box, your reach, and how far each of you moved along the
// line between you last tick (yours towards it, its own away from you).
#[derive(Clone, Copy, Debug)]
pub(super) struct Gap {
    pub distance: f64,
    pub reach: f64,
    pub mine: f64,
    pub theirs: f64,
    pub ground: bool,
}

// Ticks S Tap looks ahead: a key's move shows from the tick it is pressed, and two ticks cover most of your momentum.
const LOOKAHEAD: f64 = 2.0;

impl Gap {
    // How much the gap shrinks over the next ticks with no move key: your move fades with the ground's drag (block
    // friction 0.6 times 0.91) or the air's, the target keeps moving as it did.
    fn shrinks(self) -> f64 {
        let drag = if self.ground { 0.546 } else { 0.91 };
        self.mine * (drag + drag * drag) - self.theirs * LOOKAHEAD
    }
}

// Whether S goes down this tick: the gap would end up inside the Spacing's margin of your reach while it closes,
// unless your hit is charged again with the target in reach (that is the next hit; backing off would waste it).
pub(super) fn wants_back(spacing: Spacing, gap: Gap, ready: bool) -> bool {
    let Gap { distance, reach, mine, theirs, .. } = gap;
    if ![distance, reach, mine, theirs].iter().all(|n| n.is_finite()) || reach <= 0.0 {
        return false;
    }
    if ready && distance <= reach {
        return false;
    }
    let shrinks = gap.shrinks();
    shrinks > spacing.closing() && distance - shrinks < reach - spacing.margin()
}

// One tick of an S Tap, `elapsed` ticks after it started: S down (forward let go), forward let go only, or over.
// Forward stays let go for at least `shortest` ticks (the sprint reset) and nothing lasts past `longest`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TapStep {
    Back,
    Release,
    Done,
}

pub(super) fn tap_step(elapsed: u64, shortest: u64, longest: u64, back: bool) -> TapStep {
    if elapsed >= longest.max(shortest) {
        TapStep::Done
    } else if back {
        TapStep::Back
    } else if elapsed < shortest {
        TapStep::Release
    } else {
        TapStep::Done
    }
}

// Why an S Tap stops before its time. The first that applies wins; None: it goes on.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TapCheck {
    pub s_tap_mode: bool,
    pub forward_held: bool,
    pub back_pressed: bool,
    pub target_valid: bool,
    pub movement_taken: bool,
}

pub(super) fn tap_stop(c: TapCheck) -> Option<&'static str> {
    if !c.s_tap_mode {
        Some("W Tap off or in W Tap mode")
    } else if c.back_pressed {
        Some("you pressed back")
    } else if !c.forward_held {
        Some("forward let go")
    } else if !c.target_valid {
        Some("target gone")
    } else if c.movement_taken {
        Some("another module moves you")
    } else {
        None
    }
}

// Jump Reset: knockback that throws you up and sideways.
pub(super) fn jump_knockback(velocity: [f64; 3]) -> bool {
    velocity.iter().all(|n| n.is_finite()) && velocity[1] > 0.0 && velocity[0].hypot(velocity[2]) >= 0.1
}

// The attack speed a hand item would give: 4 plus the item's modifier, times the effects' multipliers.
pub(super) fn attack_speed(item_modifier: f64, total_modifiers: impl IntoIterator<Item = f64>) -> Option<f64> {
    if !item_modifier.is_finite() {
        return None;
    }
    let mut value = 4.0 + item_modifier;
    for modifier in total_modifiers {
        value *= 1.0 + modifier;
        if !value.is_finite() {
            return None;
        }
    }
    Some(value.clamp(0.0, 1024.0))
}

// A key press kept for a few ticks (Auto Spear's Lunge key).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Request {
    deadline: Option<u64>,
}

impl Request {
    pub(super) fn start(&mut self, now: u64, ticks: u64) {
        self.deadline = Some(now.saturating_add(ticks));
    }
    pub(super) fn active(&mut self, now: u64) -> bool {
        let active = self.deadline.is_some_and(|end| now <= end);
        if !active {
            self.clear();
        }
        active
    }
    pub(super) fn clear(&mut self) {
        self.deadline = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(distance: f64, mine: f64, theirs: f64) -> Gap {
        Gap { distance, reach: 3.0, mine, theirs, ground: true }
    }

    #[test]
    fn keys_come_back_as_you_hold_them() {
        // Held on the keyboard: back. Toggled sprint (held, not on the keyboard): back. Pressed only by W Tap: let go.
        assert!(key_back(true, true, true));
        assert!(key_back(false, true, false));
        assert!(!key_back(false, false, false));
        // Let go on the keyboard during the tick: let go. Pressed on the keyboard during the tick: held.
        assert!(!key_back(false, true, true));
        assert!(key_back(true, false, false));
    }

    #[test]
    fn backs_off_a_closing_target_but_not_one_knocked_away() {
        // You sprint in (0.28 a tick) at a target standing 2.6 blocks off: S.
        assert!(wants_back(Spacing::Balanced, gap(2.6, 0.28, 0.0), false));
        // The knockback carries it away faster than you follow: no S, the combo goes on.
        assert!(!wants_back(Spacing::Balanced, gap(2.6, 0.28, 0.45), false));
        // It runs at you: S, even from further out.
        assert!(wants_back(Spacing::Balanced, gap(3.1, 0.2, -0.28), false));
        // Far out of reach with nobody closing in fast: no S.
        assert!(!wants_back(Spacing::Balanced, gap(4.5, 0.28, 0.0), false));
    }

    #[test]
    fn a_charged_hit_in_reach_ends_the_back_off() {
        assert!(wants_back(Spacing::Conservative, gap(2.5, 0.28, 0.0), false));
        assert!(!wants_back(Spacing::Conservative, gap(2.5, 0.28, 0.0), true));
        // Charged but out of reach: spacing still counts.
        assert!(wants_back(Spacing::Conservative, gap(3.2, 0.2, -0.3), true));
    }

    #[test]
    fn spacing_orders_how_readily_it_backs_off() {
        // A slow drift in at 2.8 blocks: Conservative backs off, Balanced and Aggressive hold their ground.
        let drift = gap(2.8, 0.08, 0.0);
        assert!(wants_back(Spacing::Conservative, drift, false));
        assert!(!wants_back(Spacing::Balanced, drift, false));
        assert!(!wants_back(Spacing::Aggressive, drift, false));
        // A target standing still right in front: Conservative opens the gap; the others don't back away from it.
        let still = gap(2.0, 0.0, 0.0);
        assert!(wants_back(Spacing::Conservative, still, false));
        assert!(!wants_back(Spacing::Balanced, still, false));
        assert!(!wants_back(Spacing::Aggressive, still, false));
        // A hard rush: all three.
        let rush = gap(2.6, 0.28, -0.25);
        assert!([Spacing::Conservative, Spacing::Balanced, Spacing::Aggressive].iter().all(|s| wants_back(*s, rush, false)));
        assert_eq!(Spacing::named("Aggressive"), Spacing::Aggressive);
        assert_eq!(Spacing::named("anything else"), Spacing::Balanced);
    }

    #[test]
    fn odd_numbers_never_back_off() {
        assert!(!wants_back(Spacing::Conservative, gap(f64::NAN, 0.28, 0.0), false));
        assert!(!wants_back(Spacing::Conservative, gap(2.0, f64::INFINITY, 0.0), false));
        assert!(!wants_back(Spacing::Conservative, Gap { reach: 0.0, ..gap(2.0, 0.28, 0.0) }, false));
    }

    #[test]
    fn a_tap_lasts_from_hold_to_longest_hold() {
        // Hold 1, Longest hold 3: S while wanted, up to 3 ticks; forward let go on the first tick whatever happens.
        assert_eq!(tap_step(0, 1, 3, false), TapStep::Release);
        assert_eq!(tap_step(0, 1, 3, true), TapStep::Back);
        assert_eq!(tap_step(1, 1, 3, false), TapStep::Done);
        assert_eq!(tap_step(2, 1, 3, true), TapStep::Back);
        assert_eq!(tap_step(3, 1, 3, true), TapStep::Done);
        // Hold 2: forward stays let go for the second tick without S.
        assert_eq!(tap_step(1, 2, 3, false), TapStep::Release);
        // Hold above Longest hold: Hold wins, as W Tap's own pick does.
        assert_eq!(tap_step(2, 3, 1, false), TapStep::Release);
        assert_eq!(tap_step(3, 3, 1, true), TapStep::Done);
    }

    #[test]
    fn an_s_tap_stops_for_you_and_for_the_mode() {
        let fine = TapCheck { s_tap_mode: true, forward_held: true, back_pressed: false, target_valid: true, movement_taken: false };
        assert_eq!(tap_stop(fine), None);
        assert!(tap_stop(TapCheck { s_tap_mode: false, ..fine }).is_some());
        assert!(tap_stop(TapCheck { forward_held: false, ..fine }).is_some());
        assert!(tap_stop(TapCheck { back_pressed: true, ..fine }).is_some());
        assert!(tap_stop(TapCheck { target_valid: false, ..fine }).is_some());
        assert!(tap_stop(TapCheck { movement_taken: true, ..fine }).is_some());
        // Your own back press is named first, over the forward it lets go of (Snap Tap).
        assert_eq!(tap_stop(TapCheck { back_pressed: true, forward_held: false, ..fine }), Some("you pressed back"));
    }
}
