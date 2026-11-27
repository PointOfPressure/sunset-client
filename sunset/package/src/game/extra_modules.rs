// Modules (Client): a switch for every module, by category; the menu leaves out the ones switched off. Binds,
// commands, profiles and saved settings reach them all the same; only the ClickGUI, its search and the Options pages
// skip them. Modules itself always shows, so a switch can always be put back.

use super::sunset::{CATEGORIES, Category, Module};

pub(super) const NAME: &str = "Modules";

// The modules hidden until their switch is on (the extras of Extra Modules, Pearl Catch now shown).
pub(super) const HIDDEN: &[&str] = &[
    "Slippy",
    "Self Web",
    "Jesus",
    "Trident Boost",
    "Step",
    "Wind Jump",
    "Mine Through Mobs",
    "RF Click",
    "Fake Player",
    "Self Trap",
    "Anvil",
    "Quiver",
    "PvP Bot",
    "Middle Click Extra",
    "Enderman Look",
    "Spawner Alerts",
];

// Hidden since 1.3: a config saved before then still lists them shown, so it hides them once (runtime_config.rs).
pub(super) const HIDDEN_SINCE_1_3: &[&str] =
    &["Self Trap", "Anvil", "Quiver", "PvP Bot", "Middle Click Extra", "Enderman Look", "Spawner Alerts"];

// Its switches are every module in `modules`, by category, A to Z as the menu lists them.
pub(super) fn defaults(modules: &[Module]) -> Module {
    let mut m = Module::new(NAME, Category::Client, "Shows or hides each module in the menu.");
    for category in CATEGORIES {
        let page = super::sunset::alphabetical(modules, (0..modules.len()).filter(|&i| modules[i].category == category));
        for name in page.into_iter().map(|i| modules[i].name) {
            m = m.bool(name, !HIDDEN.contains(&name));
        }
    }
    m
}

// The modules the menu leaves out now.
pub(super) fn hidden(modules: &[Module]) -> Vec<&'static str> {
    let Some(list) = modules.iter().find(|m| m.name == NAME) else {
        return HIDDEN.to_vec();
    };
    list.settings.iter().map(|s| s.name).filter(|name| *name != NAME && !list.b(name)).collect()
}
