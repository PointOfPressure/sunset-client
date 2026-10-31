// Optional world atmosphere, independent of HUD visibility and menu state.
use super::{Dim, Game};
use super::sunset::{Category, ID, Module};
use horizon_client::gfx::sunset_atmosphere_flags::{Config, configure};

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Atmosphere", Category::Render,
        "World-anchored nebula and aurora skies. Overworld only; shader packs keep control.")
        .pick("Mode", &["Space", "Aurora", "Ringed Planet"])
        .pick("Quality", &["Balanced", "Performance", "Detailed"])
        .num("Brightness", 1.0, 0.2, 2.0, 0.1)
        .num("Effect strength", 1.0, 0.0, 2.0, 0.1)
        .num("Stars", 1.0, 0.0, 2.0, 0.1)
        .num("Speed", 1.0, 0.0, 2.0, 0.1)
        .num("Daytime strength", 0.65, 0.0, 1.0, 0.05)]
}

pub(super) fn sync(game: &Game<'_>) {
    if !game.native_window() || !game.native_on(ID) {
        configure(Config::default());
        return;
    }
    super::sunset::ensure_loaded(game);
    let world_supported = game.player.is_some() && game.remote.is_some()
        && game.title.is_none() && game.dim == Dim::Overworld;
    let config = super::sunset::with(|state| {
        let Some(m) = state.module("Atmosphere").filter(|m| m.on) else {
            return Config::default();
        };
        Config {
            enabled: true,
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
