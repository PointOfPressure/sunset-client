// Modules (Client): a switch for every module, by category; the menu leaves out the ones switched off. Binds,
// commands, profiles and saved settings reach them all the same; only the ClickGUI, its search and the Options pages
// skip them. Modules itself always shows, so a switch can always be put back.

use super::sunset::{CATEGORIES, Category, Module};

pub(super) const NAME: &str = "Modules";

// The modules hidden until their switch is on (the extras of Extra Modules, Pearl Catch now shown).
pub(super) const HIDDEN: &[&str] = &[
    "Slippy",
    "Auto City",
    "Self Web",
    "Hole Filler",
    "Burrow",
    "Key Throw",
    "Arrow Dodge",
    "Jesus",
    "Trident Boost",
    "Step",
    "Wind Jump",
    "Hold to Eat",
    "Mine Through Mobs",
    "RF Click",
    "Fake Player",
];

// Its switches are every module in `modules`, by category in the menu's order.
pub(super) fn defaults(modules: &[Module]) -> Module {
    let mut m = Module::new(NAME, Category::Client, "Shows or hides each module in the menu.");
    for category in CATEGORIES {
        for name in modules.iter().filter(|m| m.category == category).map(|m| m.name) {
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
