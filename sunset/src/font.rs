// Sunset's own fonts, so its menu and HUD don't use Minecraft's bitmap one: TrueType fonts in the package's
// assets (assets/sunset/font/), which the game loads with the other resources.
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::font_data::FONTS;

// The HUD's Font choice: 0 is the game's own font, 1 and up the fonts of ::sunset_core::font_data::FONTS in order.
pub static STYLE: AtomicU8 = AtomicU8::new(1);
// Set while a header is drawn: it takes the font's bolder weight.
pub static BOLD: AtomicBool = AtomicBool::new(false);
// The fonts are the package's own assets, there from the first resource load.
pub static READY: AtomicBool = AtomicBool::new(true);
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


pub fn enabled() -> bool {
    STYLE.load(Ordering::Relaxed) != 0 && READY.load(Ordering::Relaxed)
}

// The resource name of the chosen font (bold while a header is drawn).
pub fn name() -> String {
    let name = FONTS.get(usize::from(STYLE.load(Ordering::Relaxed)).wrapping_sub(1)).map_or(FONTS[0].0, |f| f.0);
    let bold = if BOLD.load(Ordering::Relaxed) { "-bold" } else { "" };
    format!("sunset:{name}{bold}")
}
