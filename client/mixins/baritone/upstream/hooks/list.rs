// Baritone's row in Options > Mods.
fn baritone_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    if !crate::game::sunset_api::running() { return; }

    use crate::game::baritone as baritone;
    mods.push(crate::game::native_hooks::NativeMod {
        id: baritone::ID.into(),
        title: format!("Baritone {}", baritone::VERSION),
        description: baritone::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
