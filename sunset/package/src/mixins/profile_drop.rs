// A file dropped on the game's window (before the game's own pack drops, which act on the pack screens only): the
// Profiles menu takes .profile files while the click GUI is open.
fn sunset_profile_drop(game: &mut crate::game::Game<'_>, path: &std::path::Path) {
    if ::sunset_core::lifecycle::RUNTIME.running() {
        crate::game::sunset::profile_dropped(game, path);
    }
}
