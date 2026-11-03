// Extra Modules (Client): modules the menu leaves out until their switch here is on. Binds, commands, profiles and
// saved settings reach them all the same; only the ClickGUI, its search and the Options pages skip them. A module
// joins the list with one line in EXTRAS.

use super::sunset::{Category, Module};

pub(super) const NAME: &str = "Extra Modules";

// The extra modules, in the order Extra Modules lists their switches.
pub(super) const EXTRAS: &[&str] = &[
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
    "Pearl Catch",
    "Wind Jump",
    "Hold to Eat",
    "Mine Through Mobs",
    "RF Click",
    "Fake Player",
];

pub(super) fn defaults() -> Vec<Module> {
    let mut m = Module::new(NAME, Category::Client, "Lists extra modules in the menu.");
    for name in EXTRAS {
        m = m.bool(name, false);
    }
    vec![m]
}

// The extras the menu leaves out now.
pub(super) fn hidden(modules: &[Module]) -> Vec<&'static str> {
    let shown = modules.iter().find(|m| m.name == NAME);
    EXTRAS.iter().copied().filter(|name| !shown.is_some_and(|m| m.b(name))).collect()
}
