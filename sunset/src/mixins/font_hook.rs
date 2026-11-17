// As the fonts load, Sunset's goes into the resources.
fn sunset_font_hook(root: &Path) {
    ::sunset_core::font::install(root);
}
