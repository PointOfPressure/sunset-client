// Solstice's row in Options > Mods, with its menu key.
fn solstice_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::solstice::listed(mods);
}
