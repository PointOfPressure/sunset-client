// The buttons of Delta's page: the menu page the player is on.
fn delta_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == crate::game::delta::ID {
        crate::game::delta::buttons(game, buttons);
    }
}
