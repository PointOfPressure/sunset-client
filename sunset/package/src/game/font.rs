// Sunset's fonts on the game's side: the glyphs drawn at the screen's own pixel size, and text in the chosen font. The
// switches and the files are in sunset_core::font.
pub use ::sunset_core::font::{BOLD, READY, SMOOTH, STYLE, enabled, style};
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
            tune(set, size, gui_scale, step_of(id));
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
    let name = name.split_once('#').map_or(name, |(name, _)| name);
    let (name, factor) = match name.split_once('@') {
        Some((name, q)) => (name, q.parse::<f32>().ok()? / 20.0),
        None => (name, 1.0),
    };
    FONTS.iter().find(|f| f.0 == name.strip_suffix("-bold").unwrap_or(name)).map(|f| f.1 * factor)
}

// A set's glyphs drawn for a size in GUI pixels at a GUI scale: an em of whole screen pixels, capitals a whole
// number of pixels tall.
// `step`: the quarter of a pixel its glyphs are drawn at (smooth text's sets "<set>#1" to "#3"; 0 is the set itself).
fn tune(set: &mut horizon_client::font::FontSet, size: f32, gui_scale: f32, step: u32) {
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
            tt.shift = (step as f32 / STEPS as f32, 0.0);
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
        let sized = atlas.font.cache.borrow().sets.iter().filter(|(set, _)| set.contains('@') && !set.contains('#')).count();
        let stem = base.strip_prefix("sunset:").unwrap_or(&base);
        let made = (sized < SIZED_SETS).then(|| true_type(stem)).flatten();
        let Some(mut set) = made else {
            stretched(batch);
            return;
        };
        if let Some(size) = set_size(&id) {
            tune(&mut set, size, batch.scale, 0);
        }
        atlas.font.cache.borrow_mut().sets.push((id.clone(), set));
    }
    let [a, r, g, b] = argb.to_be_bytes();
    let a = if a == 0 { 0xFF } else { a };
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    if smooth(game, batch, &id, text, [x, y], [r, g, b, a], shadow) {
        return;
    }
    let styled = horizon_client::text::Text::styled(text, horizon_client::text::Style { font: Some(id), ..Default::default() });
    if shadow {
        let shade = horizon_client::hud::rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8);
        batch.text_styled(atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, shade, false);
    }
    batch.text_styled(atlas, &styled, x, y, horizon_client::hud::rgba(r, g, b, a), false);
}

// ---- Smooth text (HUD > Smooth text)
//
// The game's text puts every glyph on a whole pixel and rounds every advance to whole pixels, which is sharp but
// uneven: at a menu's sizes half a pixel is a tenth of a letter, so some letters crowd and others gap. Smooth text
// keeps each advance as the font gives it and draws each glyph at the nearest quarter of a pixel of where it then
// falls: three more sets of each font ("<set>#1" to "#3"), each with its glyphs drawn a quarter of a pixel further
// right (the game's TrueType shift), and one glyph at a time taken from the set its place asks for.

// The parts of a pixel a glyph can sit at.
const STEPS: u32 = 4;

// The step of a set by its name.
fn step_of(id: &str) -> u32 {
    id.split_once('#').and_then(|(_, step)| step.parse().ok()).unwrap_or(0)
}

#[derive(Default)]
struct Smooth {
    // The GUI scale the advances were measured at (f32 bits): another one measures them again.
    scale: u32,
    sets: std::collections::HashMap<String, SmoothSet>,
}

#[derive(Default)]
struct SmoothSet {
    // A character's advance in screen pixels, unrounded; None for one the font does not have (the game's fallback
    // draws it).
    advances: std::collections::HashMap<char, Option<f32>>,
    // A character as a text of its own in each step's set, made once.
    texts: std::collections::HashMap<char, [Option<horizon_client::text::Text>; STEPS as usize]>,
}

thread_local! {
    static SMOOTHED: std::cell::RefCell<Smooth> = std::cell::RefCell::new(Smooth::default());
}

// A set's three shifted copies, made when it is first drawn smooth. False when one can't be made (the text is then
// drawn the old way).
fn smooth_sets(font: &horizon_client::font::Font, id: &str, gui_scale: f32) -> bool {
    let Some(size) = set_size(id) else {
        return false;
    };
    let stem = id.strip_prefix("sunset:").map(|name| name.split_once('@').map_or(name, |(name, _)| name));
    for step in 1..STEPS {
        let shifted = format!("{id}#{step}");
        if font.cache.borrow().sets.iter().any(|(set, _)| *set == shifted) {
            continue;
        }
        let Some(mut set) = stem.and_then(true_type) else {
            return false;
        };
        tune(&mut set, size, gui_scale, step);
        font.cache.borrow_mut().sets.push((shifted, set));
    }
    true
}

// `text` in set `id` as glyphs: each with the whole pixels from the text's start and its step, and the text's width,
// all in screen pixels. None when the set is not one of Sunset's TrueType ones.
fn smooth_layout(font: &horizon_client::font::Font, id: &str, text: &str, gui_scale: f32, mut glyph: impl FnMut(char, f32, usize)) -> Option<f32> {
    use ab_glyph::{Font as _, ScaleFont as _};
    SMOOTHED.with(|smoothed| {
        let mut smoothed = smoothed.borrow_mut();
        if smoothed.scale != gui_scale.to_bits() {
            smoothed.scale = gui_scale.to_bits();
            smoothed.sets.values_mut().for_each(|set| set.advances.clear());
        }
        if !smoothed.sets.contains_key(id) {
            smoothed.sets.insert(id.to_owned(), SmoothSet::default());
        }
        let set = smoothed.sets.get_mut(id)?;
        let cache = font.cache.borrow();
        let tt = cache.sets.iter().find(|(set, _)| *set == id).and_then(|(_, set)| {
            set.providers.iter().find_map(|p| match p {
                horizon_client::font::Provider::TrueType(_, tt) => Some(tt),
                _ => None,
            })
        })?;
        let scaled = tt.font.as_scaled(tt.scale);
        let mut pen = 0.0f32;
        for c in text.chars() {
            // A character the font lacks: the whole text is the game's to draw, with its fallback glyph.
            let advance = (*set.advances.entry(c).or_insert_with(|| {
                let glyph = tt.font.glyph_id(c);
                (glyph.0 != 0).then(|| scaled.h_advance(glyph))
            }))?;
            let whole = pen.floor();
            let step = ((pen - whole) * STEPS as f32).round() as usize;
            if step == STEPS as usize {
                glyph(c, whole + 1.0, 0);
            } else {
                glyph(c, whole, step);
            }
            pen += advance;
        }
        Some(pen)
    })
}

// `text` drawn smooth in set `id` from `at` (GUI pixels, on a whole screen pixel). False when it was not (Smooth text
// off, a set that is not Sunset's, a character the font lacks): the caller draws it as before.
fn smooth(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, id: &str, text: &str, at: [f32; 2], rgba: [u8; 4], shadow: bool) -> bool {
    if !SMOOTH.load(std::sync::atomic::Ordering::Relaxed) || text.is_empty() {
        return false;
    }
    let atlas = &game.gui.atlas;
    let scale = batch.scale;
    if !smooth_sets(&atlas.font, id, scale) {
        return false;
    }
    let mut glyphs: Vec<(char, f32, usize)> = Vec::with_capacity(text.len());
    if smooth_layout(&atlas.font, id, text, scale, |c, whole, step| glyphs.push((c, whole, step))).is_none() {
        return false;
    }
    let [r, g, b, a] = rgba;
    let shade = horizon_client::hud::rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8);
    let color = horizon_client::hud::rgba(r, g, b, a);
    // The glyphs' texts are taken out while they are drawn, so nothing of this is borrowed while the game draws.
    let mut texts = SMOOTHED.with(|smoothed| smoothed.borrow_mut().sets.get_mut(id).map(|set| std::mem::take(&mut set.texts))).unwrap_or_default();
    // The shadow first, a screen pixel down and right, then the text over it.
    let passes: &[bool] = if shadow { &[true, false] } else { &[false] };
    for &shaded in passes {
        for &(c, whole, step) in &glyphs {
            let one = texts.entry(c).or_insert_with(|| std::array::from_fn(|_| None))[step].get_or_insert_with(|| {
                let font = if step == 0 { id.to_owned() } else { format!("{id}#{step}") };
                horizon_client::text::Text::styled(c.to_string(), horizon_client::text::Style { font: Some(font), ..Default::default() })
            });
            let x = at[0] + whole / scale;
            if shaded {
                batch.text_styled(atlas, one, x + 1.0 / scale, at[1] + 1.0 / scale, shade, false);
            } else {
                batch.text_styled(atlas, one, x, at[1], color, false);
            }
        }
    }
    SMOOTHED.with(|smoothed| {
        if let Some(set) = smoothed.borrow_mut().sets.get_mut(id) {
            set.texts = texts;
        }
    });
    true
}

// The width of `text` in the chosen font in GUI pixels, as it is drawn: smooth, the sum of its unrounded advances.
pub fn width(font: &horizon_client::font::Font, text: &str) -> f32 {
    let scale = f32::from_bits(SCALE.load(std::sync::atomic::Ordering::Relaxed));
    if SMOOTH.load(std::sync::atomic::Ordering::Relaxed) && scale > 0.0
        && let Some(pixels) = smooth_layout(font, &::sunset_core::font::name(), text, scale, |_, _, _| {})
    {
        return pixels / scale;
    }
    font.width_text(&self::text(text))
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
    if smooth(game, batch, &::sunset_core::font::name(), text, [x, y], [r, g, b, a], shadow) {
        return;
    }
    let styled = text_styled(text);
    if shadow {
        let shade = horizon_client::hud::rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8);
        batch.text_styled(atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, shade, false);
    }
    batch.text_styled(atlas, &styled, x, y, horizon_client::hud::rgba(r, g, b, a), false);
}
