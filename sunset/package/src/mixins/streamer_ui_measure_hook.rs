// Streamer Mode: an item's tooltip lines with the names replaced, before a mod's tooltip hooks see them.
fn sunset_streamer_tooltip(lines: &mut Vec<(horizon_client::text::Text, u32)>) {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return;
    }
    for (line, _) in lines.iter_mut() {
        if let Some(shown) = crate::game::sunset_streamer::protect(line) {
            *line = shown;
        }
    }
}
