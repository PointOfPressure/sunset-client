// As the fonts load, Solstice's goes into the resources.
fn solstice_font_hook(root: &Path) {
    solstice_font::install(root);
}
