// Sunset's own font, so its menu and HUD don't use Minecraft's bitmap one. Horizon reads TrueType fonts from
// the resources, so the file and its definition are written there before the fonts load.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub static ON: AtomicBool = AtomicBool::new(true);
pub static READY: AtomicBool = AtomicBool::new(false);
// The GUI scale the glyphs were last drawn for (f32 bits).
static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
// The size in GUI pixels, as in DEFINITION.
const SIZE: f32 = 8.0;
const ID: &str = "sunset:ui";
const DEFINITION: &str = r#"{"providers":[{"type":"ttf","file":"sunset:ui.ttf","size":8.0,"oversample":4.0,"shift":[0.0,0.0]}]}"#;

fn put(path: &Path, bytes: &[u8]) -> bool {
    if std::fs::read(path).is_ok_and(|have| have == bytes) {
        return true;
    }
    std::fs::write(path, bytes).is_ok()
}

// Called as the game's fonts load: puts the font where they look.
pub fn install(root: &Path) {
    let dir = root.join("assets").join("sunset").join("font");
    let ok = std::fs::create_dir_all(&dir).is_ok()
        && put(&dir.join("ui.json"), DEFINITION.as_bytes())
        && put(&dir.join("ui.ttf"), super::sunset_fontdata::UI);
    READY.store(ok, Ordering::Relaxed);
}

// Draws the glyphs at the screen's own pixel size: one oversampled pixel for each screen pixel, so the text is
// sharp at any GUI scale instead of being scaled from a fixed bitmap. Called as text is drawn.
pub fn sync(font: &super::Font, gui_scale: f32) {
    use ab_glyph::Font as _;
    use std::sync::atomic::Ordering::Relaxed;
    let want = gui_scale.round().clamp(1.0, 8.0);
    if f32::from_bits(SCALE.load(Relaxed)) == want {
        return;
    }
    SCALE.store(want.to_bits(), Relaxed);
    {
        let mut cache = font.cache.borrow_mut();
        for (id, set) in cache.sets.iter_mut() {
            if id != ID {
                continue;
            }
            for provider in set.providers.iter_mut() {
                if let super::Provider::TrueType(_, tt) = provider {
                    let units = tt.font.units_per_em().unwrap_or(1000.0);
                    tt.oversample = want;
                    tt.scale = ab_glyph::PxScale::from((SIZE * want).round() * tt.font.height_unscaled() / units);
                    tt.shift = (0.0, 0.0);
                }
            }
        }
    }
    font.reset_cache();
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed) && READY.load(Ordering::Relaxed)
}

// A line of text in Sunset's font.
pub fn text(s: &str) -> crate::text::Text {
    crate::text::Text::styled(
        s,
        crate::text::Style {
            font: Some(ID.to_owned()),
            ..Default::default()
        },
    )
}
