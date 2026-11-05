// A button's repeat clock. Deadlines retain sub-tick remainder without replaying
// missed clicks after a stall. It never owns a physical or toggled button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Pattern { Uniform, Smooth }
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mode {
    Delay(u32),
    Cps { min: f64, max: f64, pattern: Pattern },
}
impl Mode {
    pub fn cps(min: f64, max: f64, smooth: bool) -> Self {
        let finite = |n: f64| if n.is_finite() { n.clamp(1.0, 20.0) } else { 1.0 };
        let (a, b) = (finite(min), finite(max));
        Self::Cps { min: a.min(b), max: a.max(b), pattern: if smooth { Pattern::Smooth } else { Pattern::Uniform } }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Config { pub mode: Mode, pub start_delay: f64 }
#[derive(Default)]
pub(super) struct Clock {
    config: Option<Config>,
    due: f64,
    due_tick: u64,
    last_tick: u64,
    rate: Option<f64>,
}
impl Clock {
    pub fn reset(&mut self) { *self = Self::default(); }
    fn interval(&mut self, mode: Mode, roll: f64) -> f64 {
        match mode {
            Mode::Delay(_) => 0.0,
            Mode::Cps { min, max, pattern } => {
                let target = min + (max - min) * roll.clamp(0.0, 1.0);
                let rate = match (pattern, self.rate) {
                    (Pattern::Smooth, Some(rate)) => rate + (target - rate) * 0.25,
                    _ => target,
                }.clamp(min, max);
                self.rate = Some(rate);
                1.0 / rate
            }
        }
    }
    pub fn poll(&mut self, now: f64, tick: u64, config: Config, active: bool, roll: f64) -> bool {
        if !active || !now.is_finite() { self.reset(); return false; }
        if self.config != Some(config) || tick < self.last_tick {
            self.reset();
            self.config = Some(config);
            self.last_tick = tick;
            let period = self.interval(config.mode, roll);
            self.due = now + period.max(config.start_delay.max(0.0));
            self.due_tick = match config.mode { Mode::Delay(n) => tick.saturating_add(u64::from(n.max(1))), _ => tick };
            // The real press already supplies the first click.
            return false;
        }
        if tick == self.last_tick || now + 1e-9 < self.due || tick < self.due_tick { return false; }
        self.last_tick = tick;
        match config.mode {
            Mode::Delay(n) => self.due_tick = tick.saturating_add(u64::from(n.max(1))),
            mode => {
                let period = self.interval(mode, roll);
                let phase = self.due + period;
                self.due = if phase > now + 1e-9 { phase } else { now + period };
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cps(rate: f64) -> Config { Config { mode: Mode::cps(rate, rate, true), start_delay: 0.0 } }
    #[test]
    fn fractional_tick_rates_match_requested_cps() {
        for rate in [7.0, 12.0, 17.0, 20.0] {
            let mut clock = Clock::default();
            let count = (0..=200).filter(|tick| clock.poll(*tick as f64 * 0.05, *tick, cps(rate), true, 0.5)).count();
            assert!((count as f64 - rate * 10.0).abs() <= 1.0, "{rate}: {count}");
        }
    }
    #[test]
    fn no_immediate_extra_click_or_catch_up_burst() {
        let mut clock = Clock::default();
        assert!(!clock.poll(0.0, 0, cps(12.0), true, 0.5));
        assert!(clock.poll(5.0, 100, cps(12.0), true, 0.5));
        assert!(!clock.poll(5.0, 100, cps(12.0), true, 0.5));
        assert!(!clock.poll(5.05, 101, cps(12.0), true, 0.5));
        assert!(clock.poll(5.1, 102, cps(12.0), true, 0.5));
    }
    #[test]
    fn release_and_configuration_changes_rearm_without_debt() {
        let mut clock = Clock::default();
        clock.poll(0.0, 0, cps(12.0), true, 0.5);
        assert!(!clock.poll(1.0, 20, cps(12.0), false, 0.5));
        assert!(!clock.poll(2.0, 40, cps(12.0), true, 0.5));
        assert!(!clock.poll(2.05, 41, cps(20.0), true, 0.5));
        assert!(clock.poll(2.1, 42, cps(20.0), true, 0.5));
    }
    #[test]
    fn buttons_keep_independent_rates_and_start_delays() {
        let (mut left, mut right) = (Clock::default(), Clock::default());
        let r = Config { mode: Mode::cps(7.0, 7.0, false), start_delay: 0.4 };
        let (mut l_count, mut r_count) = (0, 0);
        for tick in 0..=100 {
            let now = tick as f64 * 0.05;
            l_count += usize::from(left.poll(now, tick, cps(17.0), true, 0.2));
            r_count += usize::from(right.poll(now, tick, r, true, 0.8));
            if tick < 8 { assert_eq!(r_count, 0); }
        }
        assert_eq!(l_count, 85);
        assert!((32..=33).contains(&r_count));
    }
    #[test]
    fn delay_mode_stays_in_ticks_and_does_not_replay_a_tick() {
        let mut clock = Clock::default();
        let config = Config { mode: Mode::Delay(4), start_delay: 0.0 };
        for tick in 0..25 {
            let want = tick > 0 && tick % 4 == 0;
            assert_eq!(clock.poll(tick as f64 * 0.09, tick, config, true, 0.5), want);
            assert!(!clock.poll(tick as f64 * 0.09, tick, config, true, 0.5));
        }
    }
    #[test]
    fn smooth_pattern_changes_gradually_inside_sorted_bounds() {
        let mut clock = Clock::default();
        let mode = Mode::cps(15.0, 5.0, true);
        let mut previous: Option<f64> = None;
        for i in 0..100 {
            let interval = clock.interval(mode, f64::from(i % 2));
            let rate = 1.0 / interval;
            assert!((5.0..=15.0).contains(&rate));
            if let Some(before) = previous { assert!((rate - before).abs() <= 2.5 + 1e-9); }
            previous = Some(rate);
        }
        assert_eq!(Mode::cps(f64::NAN, 0.0, false), Mode::cps(1.0, 1.0, false));
    }
}
