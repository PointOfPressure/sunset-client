// Sunset's fonts on the game's side: the glyphs drawn at the screen's own pixel size, and text in the chosen font. The
// switches and the files are in sunset_core::font.
pub use ::sunset_core::font::{BOLD, READY, STYLE, enabled, style};
use ::sunset_core::font_data::FONTS;
use ::sunset_core::font::SCALE;

// Draws the glyphs at the screen's own pixel size: one oversampled pixel for each screen pixel, so each glyph
// lands on whole screen pixels and the text is sharp at any GUI scale instead of being scaled from a fixed bitmap.
// The height is nudged so the capitals are a whole number of pixels tall, and their top and bottom edges are
// crisp. Called as text is drawn.
pub fn sync(font: &horizon_client::font::Font, gui_scale: f32) {
    use ab_glyph::Font as _;
    use std::sync::atomic::Ordering::Relaxed;
    ensure(font);
    if f32::from_bits(SCALE.load(Relaxed)) == gui_scale {
        return;
    }
    SCALE.store(gui_scale.to_bits(), Relaxed);
    {
        let mut cache = font.cache.borrow_mut();
        for (id, set) in cache.sets.iter_mut() {
            let Some(size) = id
                .strip_prefix("sunset:")
                .and_then(|n| FONTS.iter().find(|f| f.0 == n.strip_suffix("-bold").unwrap_or(n)))
                .map(|f| f.1)
            else {
                continue;
            };
            for provider in set.providers.iter_mut() {
                if let horizon_client::font::Provider::TrueType(_, tt) = provider {
                    let units = tt.font.units_per_em().unwrap_or(1000.0);
                    let k = tt.font.height_unscaled() / units;
                    // The capitals' height as a share of the em: the H at an em of 1000 pixels.
                    let cap = tt
                        .font
                        .outline_glyph(tt.font.glyph_id('H').with_scale(ab_glyph::PxScale::from(1000.0 * k)))
                        .map_or(0.7, |o| o.px_bounds().height() / 1000.0);
                    let em = (size * gui_scale).round().max(1.0);
                    let tall = (cap * em).round().max(1.0) / cap;
                    tt.oversample = gui_scale;
                    tt.scale = ab_glyph::PxScale { x: em * k, y: tall * k };
                    tt.shift = (0.0, 0.0);
                }
            }
        }
    }
    font.reset_cache();
}

// A line of text in the chosen font.
pub fn text_styled(s: &str) -> horizon_client::text::Text {
    text(s)
}

// A line of text in the chosen font.
pub fn text(s: &str) -> horizon_client::text::Text {
    horizon_client::text::Text::styled(
        s,
        horizon_client::text::Style {
            font: Some(::sunset_core::font::name()),
            ..Default::default()
        },
    )
}

// Puts Sunset's fonts into the game's font, once per font (a resource reload builds a new one): the game takes no
// fonts from a package's assets, so each TrueType file is loaded the way the game loads one named by a definition.
pub fn ensure(font: &horizon_client::font::Font) {
    {
        let cache = font.cache.borrow();
        if cache.sets.iter().any(|(id, _)| id.starts_with("sunset:")) {
            if !::sunset_core::font::READY.load(std::sync::atomic::Ordering::Relaxed) {
                ::sunset_core::font::loaded();
            }
            return;
        }
    }
    let mut files = std::collections::BTreeMap::new();
    let mut definitions = Vec::new();
    for (stem, data) in ::sunset_core::font_data::FILES {
        let base = stem.strip_suffix("-bold").unwrap_or(stem);
        let size = FONTS.iter().find(|f| f.0 == base).map_or(8.0, |f| f.1);
        files.insert(
            format!("assets/sunset/font/{stem}.ttf"),
            horizon_assets::resource_source::Resource::bytes(data.to_vec()),
        );
        definitions.push((
            format!("sunset:{stem}"),
            serde_json::json!({"type": "ttf", "file": format!("sunset:{stem}.ttf"), "size": size, "oversample": 4.0}),
        ));
    }
    let source = horizon_assets::resource_source::ResourceSource::indexed(files);
    let mut sets = Vec::new();
    for (id, definition) in definitions {
        let Some(tt) = horizon_client::font::load_true_type(&source, &definition) else {
            continue;
        };
        sets.push((
            id,
            horizon_client::font::FontSet {
                providers: vec![horizon_client::font::Provider::TrueType(
                    horizon_client::font::Filter::default(),
                    Box::new(tt),
                )],
                glyphs: Default::default(),
                non_fishy: Default::default(),
                by_width: None,
            },
        ));
    }
    let count = sets.len();
    font.cache.borrow_mut().sets.extend(sets);
    log::info!(
        "Sunset: {count} fonts put into the game's font ({} sets now), style {}",
        font.cache.borrow().sets.len(),
        STYLE.load(std::sync::atomic::Ordering::Relaxed)
    );
    ::sunset_core::font::loaded();
}

// Sunset's text at `at` in `argb` (an alpha of 0 is opaque: Sunset gives its colours as 0xRRGGBB, and its fading
// rows with their alpha): in the chosen font, each glyph on whole screen pixels with a soft shadow a screen pixel
// away, or in Minecraft's font with its own shadow when the HUD's Font is Minecraft.
pub fn draw(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) {
    let [a, r, g, b] = argb.to_be_bytes();
    let a = if a == 0 { 0xFF } else { a };
    let atlas = &game.gui.atlas;
    ensure(&atlas.font);
    if !enabled() {
        batch.text_styled(atlas, &horizon_client::text::Text::literal(text), at[0], at[1], horizon_client::hud::rgba(r, g, b, a), shadow);
        return;
    }
    sync(&atlas.font, batch.scale);
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    let styled = text_styled(text);
    if shadow {
        let shade = horizon_client::hud::rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8);
        batch.text_styled(atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, shade, false);
    }
    batch.text_styled(atlas, &styled, x, y, horizon_client::hud::rgba(r, g, b, a), false);
}
