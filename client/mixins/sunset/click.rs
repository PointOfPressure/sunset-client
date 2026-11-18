// A click on Sunset's page.
fn sunset_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::sunset::ID {
        crate::game::sunset::click(game, button);
    }
}
