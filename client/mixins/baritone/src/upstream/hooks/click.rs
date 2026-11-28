// A click on Baritone's settings page.
fn baritone_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    use crate::game::baritone as baritone;
    if mod_id != baritone::ID {
        return;
    }
    baritone::page_click(game, button);
}
