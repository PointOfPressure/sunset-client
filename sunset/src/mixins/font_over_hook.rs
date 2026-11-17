// The same for the HUD atlas, which loads its fonts over a pack overlay since nightly 9f42064.
fn sunset_font_over_hook(root: &Path) {
    ::sunset_core::font::install(root);
}
