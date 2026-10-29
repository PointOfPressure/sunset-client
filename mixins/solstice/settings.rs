// The buttons of Solstice's page: the menu page the player is on.
fn solstice_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == crate::game::solstice::ID {
        crate::game::solstice::buttons(game, buttons);
    }
}
