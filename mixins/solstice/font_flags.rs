// Solstice's own font, so its menu and HUD don't use Minecraft's bitmap one. Horizon reads TrueType fonts from
// the resources, so the file and its definition are written there before the fonts load.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub static ON: AtomicBool = AtomicBool::new(true);
pub static READY: AtomicBool = AtomicBool::new(false);
const ID: &str = "solstice:ui";
const DEFINITION: &str = r#"{"providers":[{"type":"ttf","file":"solstice:ui.ttf","size":8.0,"oversample":4.0,"shift":[0.0,0.0]}]}"#;

fn put(path: &Path, bytes: &[u8]) -> bool {
    if std::fs::read(path).is_ok_and(|have| have == bytes) {
        return true;
    }
    std::fs::write(path, bytes).is_ok()
}

// Called as the game's fonts load: puts the font where they look.
pub fn install(root: &Path) {
    let dir = root.join("assets").join("solstice").join("font");
    let ok = std::fs::create_dir_all(&dir).is_ok()
        && put(&dir.join("ui.json"), DEFINITION.as_bytes())
        && put(&dir.join("ui.ttf"), super::solstice_fontdata::UI);
    READY.store(ok, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed) && READY.load(Ordering::Relaxed)
}

// A line of text in Solstice's font.
pub fn text(s: &str) -> crate::text::Text {
    crate::text::Text::styled(
        s,
        crate::text::Style {
            font: Some(ID.to_owned()),
            ..Default::default()
        },
    )
}
