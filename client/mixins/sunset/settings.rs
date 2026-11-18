// The buttons of Sunset's page: the menu page the player is on.
fn sunset_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == crate::game::sunset::ID {
        crate::game::sunset::buttons(game, buttons);
    }
}
