// A click on Delta's page.
fn delta_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::delta::ID {
        crate::game::delta::click(game, button);
    }
}
