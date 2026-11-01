// What Time Changer and Weather Changer show laid over what the server said, so that what it said is never lost.

const DAY: i64 = 24000;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) struct Levels {
    pub(super) raining: bool,
    pub(super) thundering: bool,
    pub(super) rain: (f32, f32),
    pub(super) thunder: (f32, f32),
}

// The time of day asked for on the server's day: the moon's phase comes from the day.
pub(super) fn shown_time(real: i64, time: i64) -> i64 {
    real.div_euclid(DAY) * DAY + time.rem_euclid(DAY)
}

pub(super) fn shown_weather(weather: &str) -> Levels {
    let (rain, thunder) = match weather {
        "Rain" => (true, false),
        "Thunder" => (true, true),
        _ => (false, false),
    };
    let level = |on: bool| if on { (1.0, 1.0) } else { (0.0, 0.0) };
    Levels { raining: rain, thundering: thunder, rain: level(rain), thunder: level(thunder) }
}

#[derive(Clone, Copy)]
pub(super) struct Cover<T> {
    real: T,
    shown: T,
}

// Takes the cover off: what is underneath, unless something has written over what was shown (a new world, say), which
// is then the real thing.
pub(super) fn lift<T: Copy + PartialEq>(cover: &mut Option<Cover<T>>, now: T) -> T {
    match cover.take() {
        Some(c) if now == c.shown => c.real,
        _ => now,
    }
}

// What to show over `real`: `want`, kept under the cover, or `real` itself.
pub(super) fn lay<T: Copy>(cover: &mut Option<Cover<T>>, real: T, want: Option<T>) -> T {
    *cover = want.map(|shown| Cover { real, shown });
    want.unwrap_or(real)
}

// What is shown, whatever the tick has made of it (the clock keeps counting between the server's updates).
pub(super) fn kept<T: Copy>(cover: &Option<Cover<T>>, now: T) -> T {
    cover.map_or(now, |c| c.shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shown_time_keeps_the_day() {
        assert_eq!(shown_time(24000 * 3 + 500, 18000), 24000 * 3 + 18000);
        assert_eq!(shown_time(24000 * 3 + 500, 24000), 24000 * 3);
        assert_eq!(shown_time(100, 6000), 6000);
        assert_eq!(shown_time(-1, 6000), -24000 + 6000);
    }

    #[test]
    fn a_packet_between_lifting_and_laying_is_kept_and_not_shown() {
        let mut cover = None;
        assert_eq!(lay(&mut cover, 100, Some(6000)), 6000);
        // The frame's packets go to the real time, and the next frame still shows the one asked for.
        let real = lift(&mut cover, 6000);
        assert_eq!(real, 100);
        let packet = 120;
        assert_eq!(lay(&mut cover, packet, Some(6000)), 6000);
        assert_eq!(lift(&mut cover, 6000), 120);
    }

    #[test]
    fn turning_it_off_gives_back_what_the_server_said() {
        let rain = shown_weather("Rain");
        let clear = shown_weather("Clear");
        let mut cover = None;
        assert_eq!(lay(&mut cover, rain, Some(clear)), clear);
        let real = lift(&mut cover, clear);
        assert_eq!(lay(&mut cover, real, None), rain);
        assert!(cover.is_none());
    }

    #[test]
    fn something_written_over_the_cover_is_the_real_thing() {
        let mut cover = None;
        lay(&mut cover, 100, Some(6000));
        assert_eq!(lift(&mut cover, 7777), 7777);
    }

    #[test]
    fn a_tick_counting_the_clock_does_not_shift_the_shown_time() {
        let mut cover = None;
        lay(&mut cover, 100, Some(6000));
        assert_eq!(kept(&cover, 6001), 6000);
        assert_eq!(lift(&mut cover, 6000), 100);
        assert_eq!(kept(&cover, 6001), 6001);
    }

    #[test]
    fn thunder_comes_with_rain() {
        let w = shown_weather("Thunder");
        assert!(w.raining && w.thundering && w.rain == (1.0, 1.0) && w.thunder == (1.0, 1.0));
        assert_eq!(shown_weather("Clear"), Levels::default());
    }
}
