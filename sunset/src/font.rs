// Sunset's own fonts, so its menu and HUD don't use Minecraft's bitmap one. Horizon reads TrueType fonts from
// the resources, so each file and its definition are written there before the fonts load.
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::font_data::FONTS;

// The HUD's Font choice: 0 is the game's own font, 1 and up the fonts of ::sunset_core::font_data::FONTS in order.
pub static STYLE: AtomicU8 = AtomicU8::new(1);
// Set while a header is drawn: it takes the font's bolder weight.
pub static BOLD: AtomicBool = AtomicBool::new(false);
pub static READY: AtomicBool = AtomicBool::new(false);
// The GUI scale the glyphs were last drawn for (f32 bits).
pub static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// The STYLE for a name HUD > Font offers: Minecraft's own is 0, a font its place in the list, and anything else (a
// choice from an older version) the first.
pub fn style(label: &str) -> u8 {
    match crate::font_data::LABELS.iter().position(|l| *l == label) {
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

pub fn enabled() -> bool {
    STYLE.load(Ordering::Relaxed) != 0 && READY.load(Ordering::Relaxed)
}

// The resource name of the chosen font (bold while a header is drawn).
pub fn name() -> String {
    let name = FONTS.get(usize::from(STYLE.load(Ordering::Relaxed)).wrapping_sub(1)).map_or(FONTS[0].0, |f| f.0);
    let bold = if BOLD.load(Ordering::Relaxed) { "-bold" } else { "" };
    format!("sunset:{name}{bold}")
}
