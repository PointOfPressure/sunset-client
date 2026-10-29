// A click on Meteor's page.
fn meteor_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::meteor::ID {
        crate::game::meteor::click(game, button);
    }
}
