// Kill Aura's attack scheduler, apart from who it targets and how it turns: when the next hit may go.
//
// Two combat models. Modern (1.9+) is the game's own: a hit lands at full strength once the attack cooldown
// (`attack_speed`) has charged, so the hit waits for it (or a share of it, Cooldown). Legacy (1.8) has no cooldown:
// how often you click decides, so hits follow a clicks-per-second clock between Min CPS and Max CPS, at most one a
// client tick (the game sends a tick's packets together, and 20 a second is the most a tick-driven client clicks).
//
// Auto picks Legacy only on evidence: the server raised your attack speed so far that the cooldown is gone (what
// servers that bring back 1.8 combat on a modern protocol do, e.g. OldCombatMechanics sets the attribute to 16 or
// more). A 1.8 server behind protocol translation that leaves the attribute alone looks modern; pick Legacy there.

// Attack speed (attacks a second) at which the cooldown takes 1.25 ticks or less: no cooldown worth waiting for.
const NO_COOLDOWN_SPEED: f64 = 16.0;
// Vanilla's ticks a second.
const TPS: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Version {
    Legacy,
    Modern,
}

// The combat model for the Combat version setting and your current attack speed, and why.
pub(super) fn version(setting: &str, attack_speed: f64) -> (Version, &'static str) {
    match setting {
        "Legacy (1.8)" => (Version::Legacy, "Legacy, picked"),
        "Modern (1.9+)" => (Version::Modern, "Modern, picked"),
        _ if attack_speed.is_finite() && attack_speed >= NO_COOLDOWN_SPEED => (Version::Legacy, "Legacy: the server removed the attack cooldown"),
        _ => (Version::Modern, "Modern: the attack cooldown is in force"),
    }
}

// How Legacy's clicks vary between Min CPS and Max CPS: Random draws each gap afresh, Drift lets the rate wander
// slowly across the range (as a hand speeds up and tires), Steady holds the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pattern {
    Random,
    Drift,
    Steady,
}

impl Pattern {
    pub(super) fn named(name: &str) -> Self {
        match name {
            "Drift" => Self::Drift,
            "Steady" => Self::Steady,
            _ => Self::Random,
        }
    }
}

// Legacy's clock, a tick at a time. `due` counts ticks toward the next click; the gap is in ticks (20 / CPS), so
// 12 CPS clicks on 3 ticks of every 5 with the rest carried over, not rounded to every other tick.
#[derive(Clone, Copy, Debug)]
pub(super) struct Clicks {
    due: f64,
    gap: f64,
    cps: f64,
    roll: u64,
    armed: bool,
}

impl Clicks {
    pub(super) fn new(seed: u64) -> Self {
        Self { due: 0.0, gap: 0.0, cps: 0.0, roll: seed | 1, armed: false }
    }

    fn unit(&mut self) -> f64 {
        self.roll ^= self.roll << 13;
        self.roll ^= self.roll >> 7;
        self.roll ^= self.roll << 17;
        (self.roll >> 11) as f64 / (1u64 << 53) as f64
    }

    fn next_gap(&mut self, min: f64, max: f64, pattern: Pattern) -> f64 {
        let (lo, hi) = (min.min(max).clamp(1.0, TPS), min.max(max).clamp(1.0, TPS));
        let cps = match pattern {
            Pattern::Steady => (lo + hi) / 2.0,
            Pattern::Random => lo + self.unit() * (hi - lo),
            Pattern::Drift => {
                if !(lo..=hi).contains(&self.cps) {
                    self.cps = (lo + hi) / 2.0;
                }
                // A step of up to a tenth of the range a click, turned back at its ends.
                let step = (self.unit() * 2.0 - 1.0) * (hi - lo) * 0.1;
                self.cps = (self.cps + step).clamp(lo, hi);
                self.cps
            }
        };
        TPS / cps
    }

    // A client tick with a target to click at: true when a click is due. The first click after `stop` comes at once;
    // a tick that could not click (out of reach, turning) does not save up clicks for later.
    pub(super) fn tick(&mut self, min: f64, max: f64, pattern: Pattern) -> bool {
        if !self.armed {
            self.armed = true;
            self.due = 0.0;
            self.gap = self.next_gap(min, max, pattern);
            return true;
        }
        self.due += 1.0;
        if self.due + 1e-9 < self.gap {
            return false;
        }
        self.due = (self.due - self.gap).min(1.0);
        self.gap = self.next_gap(min, max, pattern);
        true
    }

    // A due click that could not go (the look does not meet the target): it goes the first tick it can, and the one
    // after it waits a full gap from then.
    pub(super) fn missed(&mut self) {
        self.due = (self.gap - 1.0).max(0.0);
    }

    // No target any more: the next one is clicked at once.
    pub(super) fn stop(&mut self) {
        self.armed = false;
    }
}

// Modern: the hit may go once the attack strength (0 to 1, the game's `getAttackStrengthScale(0.5)`) reaches Cooldown
// (1: a full charge, as vanilla's indicator shows; lower hits sooner, for less).
pub(super) fn modern_ready(strength: f32, cooldown: f64) -> bool {
    f64::from(strength) + 1e-6 >= cooldown.clamp(0.1, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clicks_in(c: &mut Clicks, ticks: usize, min: f64, max: f64, p: Pattern) -> usize {
        (0..ticks).filter(|_| c.tick(min, max, p)).count()
    }

    #[test]
    fn auto_picks_legacy_only_when_the_cooldown_is_gone() {
        assert_eq!(version("Auto", 4.0).0, Version::Modern);
        assert_eq!(version("Auto", 1.6).0, Version::Modern);
        assert_eq!(version("Auto", 1024.0).0, Version::Legacy);
        assert_eq!(version("Auto", f64::NAN).0, Version::Modern);
        assert_eq!(version("Legacy (1.8)", 4.0).0, Version::Legacy);
        assert_eq!(version("Modern (1.9+)", 1024.0).0, Version::Modern);
    }

    #[test]
    fn steady_clicks_keep_their_rate_across_ticks() {
        let mut c = Clicks::new(7);
        // 12 CPS over 100 s of ticks: 1200 clicks, give or take the first.
        let n = clicks_in(&mut c, 2000, 12.0, 12.0, Pattern::Steady);
        assert!((1199..=1201).contains(&n), "{n}");
        let mut c = Clicks::new(7);
        assert_eq!(clicks_in(&mut c, 200, 20.0, 20.0, Pattern::Steady), 200, "20 CPS is every tick");
        let mut c = Clicks::new(7);
        assert_eq!(clicks_in(&mut c, 200, 40.0, 60.0, Pattern::Random), 200, "never more than one a tick");
    }

    #[test]
    fn random_and_drift_stay_inside_the_range() {
        for p in [Pattern::Random, Pattern::Drift] {
            let mut c = Clicks::new(99);
            let n = clicks_in(&mut c, 4000, 8.0, 14.0, p) as f64;
            let cps = n / 200.0;
            assert!((8.0..=14.0).contains(&cps), "{p:?}: {cps}");
        }
        // Swapped bounds are taken in order.
        let mut c = Clicks::new(3);
        let cps = clicks_in(&mut c, 2000, 14.0, 8.0, Pattern::Random) as f64 / 100.0;
        assert!((8.0..=14.0).contains(&cps), "{cps}");
    }

    #[test]
    fn the_first_click_is_at_once_and_a_pause_saves_nothing_up() {
        let mut c = Clicks::new(5);
        assert!(c.tick(10.0, 10.0, Pattern::Steady), "first click at once");
        assert!(!c.tick(10.0, 10.0, Pattern::Steady));
        assert!(c.tick(10.0, 10.0, Pattern::Steady));
        // A missed click goes next tick, and only one.
        assert!(!c.tick(10.0, 10.0, Pattern::Steady));
        c.missed();
        assert!(c.tick(10.0, 10.0, Pattern::Steady));
        assert!(!c.tick(10.0, 10.0, Pattern::Steady));
        // A new target: at once again.
        c.stop();
        assert!(c.tick(10.0, 10.0, Pattern::Steady));
    }

    #[test]
    fn modern_waits_for_the_cooldown_share() {
        assert!(!modern_ready(0.9, 1.0));
        assert!(modern_ready(1.0, 1.0));
        assert!(modern_ready(0.9, 0.9));
        assert!(modern_ready(0.2, 0.0), "a Cooldown below the floor is the floor");
    }
}
