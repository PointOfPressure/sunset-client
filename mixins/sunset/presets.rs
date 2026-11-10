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
        about: "Crystal Aura, a totem in hand, a surround, and an anchor where an enemy's pearl lands.",
        text: "sunset-profile=1
crystalaura.on=true
anchoraura.on=true
anchoraura.aura=false
anchoraura.pearlcatch=true
offhand.on=true
surround.on=true
inventorymanager.autoarmor=true
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
        about: "Kill Aura with criticals, the best weapon for each hit and the mace while you fall.",
        text: "sunset-profile=1
killaura.on=true
criticals.on=true
autoweapon.on=true
automace.on=true
wtap.on=true
friendguard.on=true
offhand.on=true
inventorymanager.autoarmor=true
autogap.on=true
velocity.on=true
nametags.on=true
targetesp.on=true
fullbright.on=true
sprint.on=true
autorespawn.on=true
",
    },
    // Sayer's mace setup (7 Nov 2026), his non-default lines without the key binds and the modules presets leave alone.
    Preset {
        id: "pvp-mace",
        title: "PvP: mace",
        about: "Aim Assist, W-Tap, Jump Reset, Wind Jump and Pearl Catch for mace fights; Auto Mace set up but left off.",
        text: "sunset-profile=1
aimassist.on=true
aimassist.distance=3.5
aimassist.pull=1.2
aimassist.stickiness=0.3
wtap.on=true
wtap.longesthold=2
wtap.reactionticks=1
wtap.chance=80
jumpreset.on=true
jumpreset.chance=80
windjump.on=true
windjump.minpitch=40
pearlcatch.on=true
automace.horizontalrange=4
automace.minfall=5
automace.stunslam=true
automace.stunminfall=1.5
automace.axeholdticks=1
automace.swapback=true
nojumpdelay.on=true
fastplace.on=true
fastplace.speed=4
breakdelay.cooldownticks=1
autotool.antibreak=true
autotool.switchdelay=1
scaffold.mode=Telly
scaffold.activationblocks=1
guimove.mode=GrimAC
nobreakreset.on=true
serverscan.on=true
serverscan.mode=Extended
fullbright.on=true
timechanger.on=true
timechanger.time=9000
clearwater.on=true
handview.on=true
handview.totempopscale=0.8
handview.totempopspeed=2
handview.leftscale=0.7
norender.on=true
norender.hideallparticles=true
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
inventorymanager.autoarmor=true
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
inventorymanager.autoarmor=true
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
inventorymanager.autoarmor=true
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
