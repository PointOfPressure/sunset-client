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
