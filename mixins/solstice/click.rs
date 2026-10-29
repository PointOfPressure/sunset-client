// A click on Solstice's page.
fn solstice_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::solstice::ID {
        crate::game::solstice::click(game, button);
    }
}
