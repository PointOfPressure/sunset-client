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
pub fn text(s: &str) -> horizon_client::text::Text {
    horizon_client::text::Text::styled(
        s,
        horizon_client::text::Style {
            font: Some(::sunset_core::font::name()),
            ..Default::default()
        },
    )
}
