// Pre-made profiles (the Profiles row's Presets tab, or .preset): each text is a plain profile file, as .profile save
// writes one, with what one way of playing needs turned on. Taking one turns every other module off (but those
// `untouched` leaves as they are), then reads the file; the menu and .preset show what that turns on and off first.
// A preset is one entry in PRESETS, and a module joins one with one line in its text.

use super::sunset::{Category, Module};

pub(super) struct Preset {
    pub(super) id: &'static str,
    pub(super) title: &'static str,
    pub(super) about: &'static str,
    pub(super) text: &'static str,
}

// Most presets keep these on: Fullbright, Sprint and Auto Respawn.
pub(super) const PRESETS: &[Preset] = &[
    Preset {
        id: "pvp-crystal",
        title: "PvP: crystal",
        about: "Crystal Aura, a totem in hand and a surround.",
        text: "sunset-profile=1
crystalaura.on=true
offhand.on=true
surround.on=true
autoarmor.on=true
autoreplenish.on=true
velocity.on=true
holeesp.on=true
nametags.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    Preset {
        id: "pvp-sword",
        title: "PvP: sword and mace",
        about: "Kill Aura with criticals and the best weapon for each hit.",
        text: "sunset-profile=1
killaura.on=true
criticals.on=true
autoweapon.on=true
wtap.on=true
friendguard.on=true
offhand.on=true
autoarmor.on=true
autogap.on=true
velocity.on=true
nametags.on=true
targetesp.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    // Cart PvP: the general fight set and Cart.
    Preset {
        id: "pvp-cart",
        title: "PvP: carts",
        about: "Minecart fights.",
        text: "sunset-profile=1
cart.on=true
offhand.on=true
autoarmor.on=true
autoreplenish.on=true
velocity.on=true
nametags.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    Preset {
        id: "anarchy",
        title: "Anarchy and base hunting",
        about: "Finds bases, stashes and new chunks; marks players and storage.",
        text: "sunset-profile=1
basefinder.on=true
stashfinder.on=true
newchunks.on=true
activatedspawnerdetector.on=true
storageesp.on=true
tracers.on=true
nametags.on=true
logoutspots.on=true
waypoints.on=true
notifier.on=true
offhand.on=true
autoarmor.on=true
autoeat.on=true
nofall.on=true
velocity.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    Preset {
        id: "building",
        title: "Building",
        about: "Places and breaks faster, in the air too.",
        text: "sunset-profile=1
fastplace.on=true
airplace.on=true
breakdelay.on=true
autotool.on=true
autoreplenish.on=true
blockoverlay.on=true
buildheight.on=true
nofall.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    Preset {
        id: "legit",
        title: "Legit (Grim-safe)",
        about: "Only modules that passed GrimAC in the anticheat sweep.",
        text: "sunset-profile=1
aimassist.on=true
hitboxes.on=true
autoweapon.on=true
shield.on=true
autotool.on=true
autoeat.on=true
nametags.on=true
storageesp.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    Preset {
        id: "survival",
        title: "Survival QoL",
        about: "Tools, food, armor and stacks looked after.",
        text: "sunset-profile=1
autotool.on=true
autoeat.on=true
autoarmor.on=true
autoreplenish.on=true
offhand.on=true
deathlocation.on=true
waypoints.on=true
trajectories.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
];

// What a preset leaves as it is: the menu's and the HUD's look, Panic and the Client modules.
pub(super) fn untouched(m: &Module) -> bool {
    m.category == Category::Client || matches!(m.name, "ClickGUI" | "HUD" | "Panic")
}

// A preset by its id or its title.
pub(super) fn find(name: &str) -> Option<usize> {
    let n = super::sunset::norm(name);
    PRESETS.iter().position(|p| super::sunset::norm(p.id) == n || super::sunset::norm(p.title) == n)
}
