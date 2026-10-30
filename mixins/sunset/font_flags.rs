// Sunset's own fonts, so its menu and HUD don't use Minecraft's bitmap one. Horizon reads TrueType fonts from
// the resources, so each file and its definition are written there before the fonts load.
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use super::sunset_fontdata::FONTS;

// The HUD's Font choice: 0 is the game's own font, 1 and up the fonts of sunset_fontdata::FONTS in order.
pub static STYLE: AtomicU8 = AtomicU8::new(1);
// Set while a header is drawn: it takes the font's bolder weight.
pub static BOLD: AtomicBool = AtomicBool::new(false);
pub static READY: AtomicBool = AtomicBool::new(false);
// The GUI scale the glyphs were last drawn for (f32 bits).
static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// The STYLE for a name HUD > Font offers: Minecraft's own is 0, a font its place in the list, and anything else (a
// choice from an older version) the first.
pub fn style(label: &str) -> u8 {
    match super::sunset_fontdata::LABELS.iter().position(|l| *l == label) {
        Some(i) if i < FONTS.len() => i as u8 + 1,
        Some(_) => 0,
        None => 1,
    }
}

fn put(path: &Path, bytes: &[u8]) -> bool {
    if std::fs::read(path).is_ok_and(|have| have == bytes) {
        return true;
    }
    std::fs::write(path, bytes).is_ok()
}

// Called as the game's fonts load: puts the fonts where they look.
pub fn install(root: &Path) {
    let dir = root.join("assets").join("sunset").join("font");
    let mut ok = std::fs::create_dir_all(&dir).is_ok();
    for (name, regular, bold, size) in FONTS {
        for (stem, data) in [(name.to_owned(), regular), (format!("{name}-bold"), bold)] {
            let definition = format!(
                r#"{{"providers":[{{"type":"ttf","file":"sunset:{stem}.ttf","size":{size:.1},"oversample":4.0,"shift":[0.0,0.0]}}]}}"#
            );
            ok = ok && put(&dir.join(format!("{stem}.json")), definition.as_bytes()) && put(&dir.join(format!("{stem}.ttf")), data);
        }
    }
    READY.store(ok, Ordering::Relaxed);
    // The fonts are new: tune them again at the next draw.
    SCALE.store(0, Ordering::Relaxed);
}

// Draws the glyphs at the screen's own pixel size: one oversampled pixel for each screen pixel, so each glyph
// lands on whole screen pixels and the text is sharp at any GUI scale instead of being scaled from a fixed bitmap.
// The height is nudged so the capitals are a whole number of pixels tall, and their top and bottom edges are
// crisp. Called as text is drawn.
pub fn sync(font: &super::Font, gui_scale: f32) {
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
                .map(|f| f.3)
            else {
                continue;
            };
            for provider in set.providers.iter_mut() {
                if let super::Provider::TrueType(_, tt) = provider {
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

pub fn enabled() -> bool {
    STYLE.load(Ordering::Relaxed) != 0 && READY.load(Ordering::Relaxed)
}

// A line of text in the chosen font.
pub fn text(s: &str) -> crate::text::Text {
    let name = FONTS.get(usize::from(STYLE.load(Ordering::Relaxed)).wrapping_sub(1)).map_or(FONTS[0].0, |f| f.0);
    let bold = if BOLD.load(Ordering::Relaxed) { "-bold" } else { "" };
    crate::text::Text::styled(
        s,
        crate::text::Style {
            font: Some(format!("sunset:{name}{bold}")),
            ..Default::default()
        },
    )
}
