// The buttons of Meteor's page: the menu page the player is on.
fn meteor_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == crate::game::meteor::ID {
        crate::game::meteor::buttons(game, buttons);
    }
}
