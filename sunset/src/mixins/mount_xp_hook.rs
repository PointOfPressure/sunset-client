// Mount HUD: the XP bar while riding, in the jump bar's place while no jump charges.
fn sunset_mount_xp(
    value: crate::game::sunset_hunger::InfoBar,
    has_experience: bool,
) -> crate::game::sunset_hunger::InfoBar {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return value;
    }
    crate::game::sunset_hunger::mount_xp_bar(value, has_experience)
}
