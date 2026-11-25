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
    use std::sync::atomic::Ordering::Relaxed;
    ensure(font);
    if f32::from_bits(SCALE.load(Relaxed)) == gui_scale {
        return;
    }
    SCALE.store(gui_scale.to_bits(), Relaxed);
    {
        let mut cache = font.cache.borrow_mut();
        for (id, set) in cache.sets.iter_mut() {
            let Some(size) = set_size(id) else {
                continue;
            };
            tune(set, size, gui_scale);
        }
    }
    font.reset_cache();
}

// A Sunset font set's size in GUI pixels by its name: a font's own, the icons', or, for a set made for a HUD element's
// size ("sunset:<font>@<twentieths>", draw_sized), the font's times that size.
fn set_size(id: &str) -> Option<f32> {
    if id == ICON_FONT {
        return Some(ICON_SIZE);
    }
    let name = id.strip_prefix("sunset:")?;
    let (name, factor) = match name.split_once('@') {
        Some((name, q)) => (name, q.parse::<f32>().ok()? / 20.0),
        None => (name, 1.0),
    };
    FONTS.iter().find(|f| f.0 == name.strip_suffix("-bold").unwrap_or(name)).map(|f| f.1 * factor)
}

// A set's glyphs drawn for a size in GUI pixels at a GUI scale: an em of whole screen pixels, capitals a whole
// number of pixels tall.
fn tune(set: &mut horizon_client::font::FontSet, size: f32, gui_scale: f32) {
    use ab_glyph::Font as _;
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

// The most font sets made for sizes (each size of each font used is one): past it, text is stretched instead.
const SIZED_SETS: usize = 24;

// A HUD element's text at the element's size (the layout editor's, in twentieths): in the chosen font from a set of
// its own made for that size at the screen's pixels, so it is as sharp as any other text; Minecraft's font, or too many
// sizes at once, is stretched as before.
pub fn draw_sized(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool, size: f32, bold: bool) {
    let q = (size * 20.0).round().clamp(4.0, 120.0) as u32;
    if q == 20 {
        BOLD.store(bold, std::sync::atomic::Ordering::Relaxed);
        draw(game, batch, text, at, argb, shadow);
        BOLD.store(false, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let atlas = &game.gui.atlas;
    ensure(&atlas.font);
    let stretched = |batch: &mut horizon_client::hud::HudBatch| {
        let start = batch.vertices.len();
        BOLD.store(bold, std::sync::atomic::Ordering::Relaxed);
        draw(game, batch, text, at, argb, shadow);
        BOLD.store(false, std::sync::atomic::Ordering::Relaxed);
        batch.scale_since(start, at[0], at[1], q as f32 / 20.0, q as f32 / 20.0);
    };
    if !enabled() {
        stretched(batch);
        return;
    }
    sync(&atlas.font, batch.scale);
    BOLD.store(bold, std::sync::atomic::Ordering::Relaxed);
    let base = ::sunset_core::font::name();
    BOLD.store(false, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{base}@{q}");
    let known = atlas.font.cache.borrow().sets.iter().any(|(set, _)| *set == id);
    if !known {
        let sized = atlas.font.cache.borrow().sets.iter().filter(|(set, _)| set.contains('@')).count();
        let stem = base.strip_prefix("sunset:").unwrap_or(&base);
        let made = (sized < SIZED_SETS).then(|| true_type(stem)).flatten();
        let Some(mut set) = made else {
            stretched(batch);
            return;
        };
        if let Some(size) = set_size(&id) {
            tune(&mut set, size, batch.scale);
        }
        atlas.font.cache.borrow_mut().sets.push((id.clone(), set));
    }
    let [a, r, g, b] = argb.to_be_bytes();
    let a = if a == 0 { 0xFF } else { a };
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    let styled = horizon_client::text::Text::styled(text, horizon_client::text::Style { font: Some(id), ..Default::default() });
    if shadow {
        let shade = horizon_client::hud::rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8);
        batch.text_styled(atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, shade, false);
    }
    batch.text_styled(atlas, &styled, x, y, horizon_client::hud::rgba(r, g, b, a), false);
}

// One of Sunset's fonts (by its file's stem) as a font set of its own.
fn true_type(stem: &str) -> Option<horizon_client::font::FontSet> {
    let (_, data) = ::sunset_core::font_data::FILES.iter().find(|(s, _)| *s == stem)?;
    let base = stem.strip_suffix("-bold").unwrap_or(stem);
    let size = FONTS.iter().find(|f| f.0 == base).map_or(8.0, |f| f.1);
    let mut files = std::collections::BTreeMap::new();
    files.insert(format!("assets/sunset/font/{stem}.ttf"), horizon_assets::resource_source::Resource::bytes(data.to_vec()));
    let source = horizon_assets::resource_source::ResourceSource::indexed(files);
    let definition = serde_json::json!({"type": "ttf", "file": format!("sunset:{stem}.ttf"), "size": size, "oversample": 4.0});
    let tt = horizon_client::font::load_true_type(&source, &definition)?;
    Some(horizon_client::font::FontSet {
        providers: vec![horizon_client::font::Provider::TrueType(horizon_client::font::Filter::default(), Box::new(tt))],
        glyphs: Default::default(),
        non_fishy: Default::default(),
        by_width: None,
    })
}

// The menu's icon font (Lucide), its size like the text's.
pub const ICON_FONT: &str = "sunset:icons";
const ICON_SIZE: f32 = 8.0;

// An icon (one of Lucide's characters) at `at` in `argb`, `size` times the text's height.
pub fn icon(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, glyph: &str, at: [f32; 2], argb: u32, size: f32) {
    let [a, r, g, b] = argb.to_be_bytes();
    let a = if a == 0 { 0xFF } else { a };
    let atlas = &game.gui.atlas;
    ensure(&atlas.font);
    sync(&atlas.font, batch.scale);
    let text = horizon_client::text::Text::styled(glyph, horizon_client::text::Style { font: Some(ICON_FONT.to_owned()), ..Default::default() });
    let start = batch.vertices.len();
    batch.text_styled(atlas, &text, at[0], at[1], horizon_client::hud::rgba(r, g, b, a), false);
    if (size - 1.0).abs() > 0.001 {
        batch.scale_since(start, at[0], at[1], size, size);
    }
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
    // The menu's icons, as a font of their own.
    files.insert("assets/sunset/font/icons.ttf".to_owned(), horizon_assets::resource_source::Resource::bytes(::sunset_core::font_data::ICONS.to_vec()));
    definitions.push((ICON_FONT.to_owned(), serde_json::json!({"type": "ttf", "file": "sunset:icons.ttf", "size": ICON_SIZE, "oversample": 4.0})));
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
