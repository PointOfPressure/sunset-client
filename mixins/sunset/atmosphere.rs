// Atmosphere: a nebula, aurora or ringed planet sky over the Overworld, off by default (a shader pack keeps the sky),
// or the game's own sky (None); Custom fog sets where the fog starts and is full, and its colour (out of water and
// lava), with Cover horizon band ending the sky's fog there too (render.rs turns the game's Fog option off for it).
use super::{Dim, Game};
use super::sunset::{Category, ID, Module};
use horizon_client::gfx::sunset_atmosphere_flags::{Config, configure};
use horizon_client::fog::sunset_fog::CUSTOM as FOG;

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Atmosphere", Category::Render,
        "Custom skies.")
        .pick("Mode", &["Space", "Aurora", "Ringed Planet", "None"])
        .pick("Quality", &["Balanced", "Performance", "Detailed"])
        .num("Brightness", 1.0, 0.2, 2.0, 0.1)
        .num("Effect strength", 1.0, 0.0, 2.0, 0.1)
        .num("Stars", 1.0, 0.0, 2.0, 0.1)
        .num("Speed", 1.0, 0.0, 2.0, 0.1)
        .num("Daytime strength", 0.65, 0.0, 1.0, 0.05)
        .bool("Custom fog", false)
        .num("Fog start", 90.0, 0.0, 200.0, 5.0)
        .num("Fog end", 150.0, 0.0, 200.0, 5.0)
        .colour("Fog color", "White")
        .bool("Cover horizon band", true)]
}

pub(super) fn custom_fog() -> bool {
    FOG.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Fog start" | "Fog end" | "Fog color" | "Cover horizon band" => m.b("Custom fog"),
        "Quality" | "Brightness" | "Effect strength" | "Stars" | "Speed" | "Daytime strength" => m.p("Mode") != "None",
        _ => true,
    }
}


pub(super) fn sync(game: &Game<'_>) {
    if !game.native_window() || !game.native_on(ID) {
        configure(Config::default());
        *FOG.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }
    super::sunset::ensure_loaded(game);
    let world_supported = game.player.is_some() && game.remote.is_some()
        && game.title.is_none() && game.dim == Dim::Overworld;
    let config = super::sunset::with(|state| {
        let Some(m) = state.module("Atmosphere").filter(|m| m.on) else {
            *FOG.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return Config::default();
        };
        *FOG.lock().unwrap_or_else(|e| e.into_inner()) = m.b("Custom fog").then(|| {
            let (lo, hi) = (m.n("Fog start").min(m.n("Fog end")) as f32, m.n("Fog start").max(m.n("Fog end")) as f32);
            let c = m.c("Fog color");
            let ch = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
            (lo, hi, [ch(16), ch(8), ch(0)], m.b("Cover horizon band"))
        });
        Config {
            enabled: m.p("Mode") != "None",
            world_supported,
            mode: match m.p("Mode") { "Aurora" => 1, "Ringed Planet" => 2, _ => 0 },
            quality: match m.p("Quality") { "Performance" => 0, "Detailed" => 2, _ => 1 },
            brightness: m.n("Brightness") as f32,
            strength: m.n("Effect strength") as f32,
            stars: m.n("Stars") as f32,
            speed: m.n("Speed") as f32,
            day_blend: m.n("Daytime strength") as f32,
        }
    });
    configure(config);
}
