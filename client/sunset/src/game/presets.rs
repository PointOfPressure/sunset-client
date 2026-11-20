// Pre-made profiles (the Profiles row's Presets tab, or .preset): each text is a plain profile file, as .profile save
// writes one. Taking one turns every other module off (but those `untouched` leaves as they are), then reads the file;
// the menu and .preset show what that turns on and off first. A preset is one entry in PRESETS.

use super::sunset::{Category, Module};

pub(super) struct Preset {
    pub(super) id: &'static str,
    pub(super) title: &'static str,
    pub(super) about: &'static str,
    pub(super) text: &'static str,
}

// One preset: Sunset's defaults (default_config.rs), every setting as a fresh install has it.
pub(super) const PRESETS: &[Preset] = &[Preset {
    id: "default",
    title: "Default",
    about: "Sunset's defaults: every module and setting as a fresh install has them.",
    text: super::sunset_default_config::TEXT,
}];

// What a preset leaves as it is: the menu's and the HUD's look, Panic and the Client modules.
pub(super) fn untouched(m: &Module) -> bool {
    m.category == Category::Client || matches!(m.name, "ClickGUI" | "HUD" | "Panic")
}

// A preset by its id or its title.
pub(super) fn find(name: &str) -> Option<usize> {
    let n = super::sunset::norm(name);
    PRESETS.iter().position(|p| super::sunset::norm(p.id) == n || super::sunset::norm(p.title) == n)
}
