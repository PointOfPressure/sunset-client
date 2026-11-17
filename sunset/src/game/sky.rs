// Time Changer and Weather Changer. The server's packets arrive once a frame, between ticks, and write the time and the
// weather straight into the world: the world gets the real ones back before them, and what you asked for again before
// anything is drawn, so a frame never shows the server's and turning a module off gives back what the server last said.

use super::sunset_sky_math::{Cover, Levels, kept, lay, lift, shown_time, shown_weather};
use super::Game;
use std::sync::Mutex;

struct Sky {
    time: Option<Cover<i64>>,
    weather: Option<Cover<Levels>>,
}

static SKY: Mutex<Sky> = Mutex::new(Sky { time: None, weather: None });

fn sky<R>(f: impl FnOnce(&mut Sky) -> R) -> R {
    f(&mut SKY.lock().unwrap_or_else(|e| e.into_inner()))
}

fn levels(game: &Game<'_>) -> Levels {
    let w = &game.world.weather;
    Levels { raining: w.raining, thundering: w.thundering, rain: w.rain_level, thunder: w.thunder_level }
}

fn set_levels(game: &mut Game<'_>, l: Levels) {
    let w = &mut game.world.weather;
    w.raining = l.raining;
    w.thundering = l.thundering;
    w.rain_level = l.rain;
    w.thunder_level = l.thunder;
}

// Before the frame's packets.
pub(super) fn real(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    let (time, weather) = (game.world.day_time, levels(game));
    let (time, weather) = sky(|s| (lift(&mut s.time, time), lift(&mut s.weather, weather)));
    game.world.day_time = time;
    set_levels(game, weather);
}

// After the packets, and again before drawing. Only over a server's world (single player's is one too): with the world
// in this `Game`, what is here is what gets saved.
pub(super) fn shown(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    if game.title.is_some() || game.remote.is_none() {
        forget();
        return;
    }
    real(game);
    let on = game.native_on(super::sunset::ID);
    let (time, weather) = super::sunset::with(|s| {
        let get = |name| s.module(name).filter(|m| on && m.on);
        (get("Time Changer").map(|m| m.n("Time") as i64), get("Weather Changer").map(|m| shown_weather(m.p("Weather"))))
    });
    let (real_time, real_weather) = (game.world.day_time, levels(game));
    let (time, weather) = sky(|s| {
        (lay(&mut s.time, real_time, time.map(|t| shown_time(real_time, t))), lay(&mut s.weather, real_weather, weather))
    });
    game.world.day_time = time;
    set_levels(game, weather);
}

// The end of a tick: the clock counts on between the server's updates, the shown time does not.
pub(super) fn keep_shown(game: &mut Game<'_>) {
    game.world.day_time = sky(|s| kept(&s.time, game.world.day_time));
}

// Another world is coming: what was under the cover belongs to this one.
pub(super) fn forget() {
    sky(|s| *s = Sky { time: None, weather: None });
}
