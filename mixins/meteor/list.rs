// Meteor's row in Options > Mods, with its menu key.
fn meteor_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::meteor::listed(mods);
}
