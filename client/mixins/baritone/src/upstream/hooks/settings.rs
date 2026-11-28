// Baritone's settings page.
fn baritone_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    use crate::game::baritone as baritone;
    if mod_id != baritone::ID {
        return;
    }
    buttons.extend(baritone::page_labels(game));
}
