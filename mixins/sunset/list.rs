// Sunset's row in Options > Mods, with its menu key.
fn sunset_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::sunset::listed(mods);
}
