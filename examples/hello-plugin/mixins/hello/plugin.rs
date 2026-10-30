// An example Solstice plugin: one module, "Hello", that says something in chat
// every few seconds while it is on.

use super::Game;
use super::solstice_api as api;

pub(crate) fn tick(game: &mut Game<'_>) {
    // Adds the module to the click GUI the first time; later calls do nothing.
    // Its settings (and whether it is on) are saved with Solstice's config.
    api::register(
        api::Module::new("Hello", api::Category::Player, "Says hello in chat now and then (an example plugin).")
            .num("Every (seconds)", 10.0, 1.0, 60.0, 1.0)
            .text("Message", "Hello from a plugin"),
    );
    // None while the player has it off.
    let Some(m) = api::module("Hello") else {
        return;
    };
    if game.ticks % (m.n("Every (seconds)") as u64 * 20) == 0 {
        api::chat(game, m.t("Message"));
    }
}
