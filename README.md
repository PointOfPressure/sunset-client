<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="docs/images/logo-light.png">
    <img src="docs/images/logo.png" alt="Sunset logo" width="550">
  </picture>
</p>

<h1 align="center">Sunset</h1>

<p align="center">A native utility client for <a href="https://github.com/horizon-rust/horizon">Horizon</a> on Minecraft 26.3.</p>
<p align="center"><a href="https://sunset-client.pages.dev">Website</a> · <a href="https://github.com/PointOfPressure/sunset-client/releases/latest">Latest release</a> · <a href="#install">Install</a> · <a href="#features">Features</a> · <a href="https://discord.gg/fhD2frFdzm">Discord</a> · <a href="CONTRIBUTING.md">Contribute</a></p>

Sunset brings 100+ modules for combat, movement, building and rendering to Horizon, with a configurable ClickGUI,
key binds, chat commands and saved profiles. It takes inspiration from
[Meteor Client](https://github.com/MeteorDevelopment/meteor-client) and is written in Rust for Horizon.

![Sunset 0.8.1 ClickGUI showing the five module categories](docs/images/gui-overview.png)

*Sunset 0.8.1 in game: Combat, Movement, Player, Render and World panels.*

> Not made by, or affiliated with, the Horizon or Meteor projects. Use it where it's allowed: many servers ban
> modules like these.

## Install

You need the [Horizon launcher](https://playhorizon.dev) installed and opened once. Then one command does
everything (installs Sunset, builds it into the game, starts it):

**Windows** (PowerShell)

```powershell
irm https://github.com/PointOfPressure/sunset-client/releases/latest/download/install.ps1 | iex
```

**Linux and macOS** (terminal)

```sh
curl -fsSL https://github.com/PointOfPressure/sunset-client/releases/latest/download/install.sh | sh
```

The first run builds Horizon with Sunset and can take **10 to 30 minutes**. When it finishes, Horizon starts with
Sunset installed. Later, open the launcher and choose the **Native mods** instance. Press the **backtick/grave** key
(usually below Esc) or **Right Shift** to open the ClickGUI.

On Linux the installer may ask for your password once, to install two small system libraries the build needs.

## Quick start

The ClickGUI groups modules into five panels: Combat, Movement, Player, Render and World.

- **Left click** a module to turn it on or off. **Right click** it for its settings and key bind, in a window of its own
  (drag it by its title; Esc closes it), or in the panel with ClickGUI *Settings: Inline*. Click a slider's number to
  type one.
- **Just type** to search: `esp` shows the modules with ESP in the name. **Enter** turns on the first match (Enter again turns it off). **Ctrl+A** selects the search text, **Esc** clears it, then closes the menu.
- The dark dim and the blur the game puts behind the inventory, chests and the Esc menu are off; **ClickGUI > Menu dim** brings them back.
- **Drag** a panel's header to move it, **right click** a header to fold it, **scroll** a long panel.
- **ClickGUI** sets the menu's look: **GUI scale** makes it bigger, **Panel radius** and **Control radius** round its corners.
- Change a **bind** with the *Bind* row: press a key or a mouse button (right, middle or a side button; Backspace clears
  it). Drawing modules (ESP, Tracers, Zoom, HUD...) have **Bind mode** *Hold*: on only while the bind is held. Both menu
  keys can be rebound under Options > Mods > Sunset.

Everything is saved as you change it.

### Chat commands

They start with `.` and are never sent to the server (type `..hi` to say `.hi`).

| Command | Does |
| --- | --- |
| `.help` | list the commands |
| `.t <module>` | turn a module on or off (`.t killaura`) |
| `.set <module> <setting> <value>` | change a setting: `.set killaura range 4`. The start of a name is enough, and a value can be several words |
| `.settings <module>` | show a module's settings |
| `.bind <module> <key>` | bind a key or mouse button: `.bind flight r`, `.bind zoom mouse4`, or `none` |
| `.reset <module>` | put a module back to its defaults |
| `.modules`, `.binds` | list the modules, the binds |
| `.friend add\|remove\|list <name>` | friends are left alone by Kill Aura, Aim Assist and the other targeting modules |
| `.waypoint add\|remove\|list <name>` | places the Waypoints module shows on screen |
| `.vclip <n>`, `.hclip <n>` | move up or down, forward |
| `.goto <x> <y> <z>`, `.goto stop` | walk there through loaded terrain (up to 128 blocks); a movement key or a menu stops it |
| `.profile save\|load\|delete <name>`, `.profile list` | keep every module's on/off, settings and keys under a name and bring them back (a PvP set-up, a building one); `.profile load before-load` undoes the last load |
| `.profile export\|import <name>` | share a profile: export copies it to the mod's `exports` folder, import takes `<name>.profile` from its `imports` folder |
| `.preset [name] [apply]` | the pre-made profiles (PvP: crystal, PvP: sword and mace, PvP: carts, Anarchy and base hunting, Building, Legit (Grim-safe), Survival QoL): a name says what it turns on and off, `apply` takes it |
| `.debug [chat lines]` | copy a debug report to the clipboard (and save it in the mod's `debug` folder): versions and server, your state, the input and packets sent tick by tick, the modules on with their changed settings, anticheat alerts from chat counted per check, setbacks, kicks and the chat (100 lines by default). Paste it into a bug report or an AI. **Debug Report** (Client) does the same from its row or a key |
| `.panic` | turn every module off and hide Sunset until `.panic` again or restart (or bind the **Panic** module to a key) |
| `.menu [text]` | open the menu, already searching for the text |

List settings (the blocks Xray shows, the mobs Kill Aura attacks) are set the same way:
`.set xray blocks diamond_ore,ancient_debris`.

The HUD shows the profile in use under the watermark ("(modified)" once you change a setting); the **Profiles** module
(Client) lists the saved ones to load, and its Presets tab the pre-made ones: each shows what it turns on and off
before you apply it. HUD > Module list side puts the list of modules on the left or the right.

Some modules are extras the menu leaves out until you switch them on in **Extra Modules** (Client): Slippy, Auto City,
Self Web, Hole Filler, Burrow, Key Throw, Arrow Dodge, Jesus, Trident Boost, Step, Pearl Catch, Wind Jump, Hold to
Eat, Mine Through Mobs, RF Click and Fake Player. Their binds and `.t` work while they are hidden.

## Features

Speed has Vanilla, Strafe, Smooth, Low Hop and Custom modes. Velocity can scale, blend or delay knockback; Scaffold
counts your blocks over the hotbar; Fast Place has five speed levels. ClickGUI has Dropdown and Dashboard styles; its
settings cover scale, spacing, colors and panel shape. Advanced controls appear only for relevant modes.

<details>
<summary>Browse the built-in modules</summary>

| Category | Modules |
| --- | --- |
| **Combat** | Kill Aura, Trigger Bot (both with Shield: your shield kept up between hits), Shield (Auto block: your shield up while a player who can hit you is near, down for an axe or your own crit; Swap key), Aim Assist (Silent aim sends a missed swing to the target beside it, Hit flick looks away and back around a hit), Fake Lag (Dynamic, Always, Timed, Ranged and Knockback; Backtrack holds what the server sends while the player you hit moves away), W Tap, Jump Reset, Reach, Criticals, Crystal Aura and Anchor Aura (each with a Macro section: the whole place and break, or place, charge and set off, on your right click; Anchor Aura's Pearl catch puts a charged anchor where an enemy's pearl lands), Cart (TNT minecart PvP: Aura breaks enemy carts, Auto cart places a rail and cart where your arrow lands or at your crosshair, Xbow sets one off through a fire with a crossbow, Cover blocks carts that would hurt you, Refill, Pearls answers an enemy pearl with a cart), Crystal Optimizer, Offhand (totems included; Double hand puts a hotbar totem in hand when a blast could pop you), Auto Weapon (Totem hit, Spear's damage indicator), Hitboxes, Auto EXP, Bow Spam, Bow Aimbot, Surround, Self Trap, Auto Trap, Self Web, Auto Web (webs where a falling player lands, a Web key), Key Throw (pearl, short pearl and wind charge keys, Silent or Legit; a pearl at your feet when a player hits you; a wind charge where your pearl lands; Pearl follow throws your pearl after an enemy's), Hole Filler, Burrow, Anvil (Anti Anvil, Self Anvil and Auto Anvil), Anti Bed, Quiver, Arrow Dodge |
| **Movement** | Flight, Speed, Sprint (Keep sprint), Sneak, Auto Walk, Auto Jump, Step (with Reverse step), Spider, High Jump, Long Jump, Air Jump, Parkour, Fast Climb, Safe Walk (Sneak sneaks at the edge while you bridge), Anti Void, Elytra Boost, Chest Swap (Take off puts the elytra on, glides off with a rocket and swaps back), Blink, Scaffold (Legit sneaks at the edge, Godbridge bridges backwards without sneaking), No Fall, No Slow, Velocity, Glide, No Jump Delay, GUI Move (walk with the inventory or another menu open), Snap Tap (of A and D, or W and S, the last pressed wins), Click TP, Trident Boost, Elytra Fly, Jesus, Slippy, Wind Jump (Elytra glides and wind-charges up off the ground, Hit charges knocks others' wind charges away), Pearl Catch (a Catch key throws the pearl and the wind charge that catches it: Up, Fast, Offhand, Diagonal) |
| **Player** | Fast Place (speed 1 to 5), Break Delay, Speed Mine, No Break Reset, Fast Use (with FastXP's throwables: every tick or in bursts), Air Place, Auto Armor, Auto Log, Auto Eat, Auto Gap (Auto pot throws healing potions at your feet), Auto Fish, Auto Tool (a tool from the main inventory, or only for the block under you), Auto Replenish (with your inventory open or on a key; potions into an empty hotbar), Auto Clicker, Auto Respawn, Anti AFK, Anti Hunger, Middle Click Extra, Notifier, Spam, Chest Stealer, Inventory Cleaner (PvP, Building, Mining and Survival presets; a Loot key drops spare totems and pearls), Anti Vanish, Server Scan (the software, plugins and channels the server shows), Panic, Rotation, No Rotate, No Status Effects, Mine Through Mobs, Portals, Multitask, Fake Player (single player only), Liquid Interact, No Interact (also Hit fix, No miss delay, Anti action, Ghost place: a web behind the player you click, and Anti trap: trap entities and sign editors dropped) |
| **Render** | ClickGUI, HUD, Radar, ESP (2D corners or brackets with a health bar, boxes coloured by shields), Tracers, Nametags (banded health colours), Storage ESP, Block ESP, Xray, Freecam, Free Look, Zoom, Fullbright, Trajectories (crossbows with multishot, wind charges, pearls in flight with their landing time), Waypoints, Breadcrumbs, Logout Spots, Hole ESP, Tunnel ESP, Void ESP, Light Overlay, Mob Gear ESP, No Render (no fog, water fog, darkness, blindness, fire or other overlays, hurt cam, scoreboard, titles or held item name), Effects (jump circles, footstep and ring trails, hit sparks, rain, snow, fireflies or embers), Target ESP (Brackets, Ghost or Square), Hand View (swing styles, slow swing, no equip drop), Atmosphere (custom fog) |
| **World** | Nuker, Packet Mine, Timer, Time Changer, Weather Changer, Liquid Filler (Drain scoops an enemy's water or lava), Spawn Proofer, Flamethrower, Auto Shearer, Auto Nametag, Auto Mount, Auto Breed, Auto Sign, Build Height, Enderman Look, Spawner Alerts, Base Finder (block tiers, written signs, open portals, bubble columns, sky builds, lone spawners, name tags, traded villagers...), New Chunks (new and old chunks from the server's palettes and liquid flow, kept per server), Player Activity Detector, Stash Finder, Cave Disturbance Detector, Portal Pattern Finder, Activated Spawner Detector, Unlock All Recipes (every recipe in the book, on any server) |
| **Client** | Profiles (saved and pre-made), Client Sort (Sort, Fill Stacks and Restock buttons, as ClientSort), Extra Modules, Debug Report |

</details>

Kill Aura, Trigger Bot and Aim Assist can skip bots (**Anti bot**: players the server's player list doesn't show) and
players on your own team (**Ignore teams**). **HUD > Target HUD** shows who Kill Aura is hitting: name, distance and health.
Kill Aura hits one target (**Single**, with **Target lock** to stay on it), each in turn (**Switch**) or several (**Multi**).
**Rotation mode** is Silent aura (the default, after vape-rs's SilentAura: a look the server alone sees, turned in whole
mouse steps at **Aim speed** and sent in the game's own move packets, hitting only within vanilla reach), Snap, Normal,
Smooth or Custom (its own yaw and pitch speeds), told only to the server or also turning your view (**Rotation view**).
**Ray cast** hits only when the look really meets the target's box.
Aim Assist's **Simple** mode (the default, vape-rs's) turns you toward the target on its own while you hold attack;
**Duelkit** only adds to your own mouse movement. Anti AFK's **Wander** (the default, vape-rs's) makes small turns and steps
once you have been idle a while; **Meteor** spins, jumps and strafes. No Slow's **Skip** (the default, vape-rs's)
leaves the game's item-use slowdown out except in water and after a knockback; **Scale** walks at Ground % / Air %.
It waits while another module has the hand, skips broken weapons, and stabs with spears; a stab never goes through a friend.
Crystal Aura and Anchor Aura (Rust's, after Anubis's) take the hotbar from whatever you hold (a sword, pickaxe, food or
blocks; not a golden apple) and give it back after; they wait while you use the item. They put obsidian down on plain
ground first. They never set off a crystal or anchor that would deal you more than **Max self damage** or leave you near
death: without armour with Protection every spot in reach would, so they wait.

Not here (no clean way to do it in Horizon yet): Jesus, Click TP, and the Baritone-based
Highway Builder and Infinity Miner.

## Updating and removing

**Update:** run the same command again. Sunset was called Solstice before 0.8.1: the command removes the old
`solstice.hnmod` and Sunset takes your saved settings over on its first start.

**Remove:** delete `sunset.hnmod` from the `horizon-native-mods` folder in Horizon's game folder (Windows:
`%APPDATA%\horizon`, Linux: `~/.local/share/horizon`, macOS: `~/Library/Application Support/horizon`, and the same
folder inside `instances\native-mods`), then play the plain **Horizon** instance.

Each Sunset release is built for one Horizon release. The **Native mods** instance stays on that revision until you
update Sunset with the installer again.

## If something goes wrong

- **"I couldn't find the Horizon launcher"**: install it from [playhorizon.dev](https://playhorizon.dev), open it once, run the command again.
- **The build fails on Linux** with a message about `alsa` or `libudev`: install `libasound2-dev libudev-dev pkg-config` (Debian, Ubuntu), or your system's equivalents, and run the command again.
- **The launcher says "These mods were made for different Horizon revisions and can't be built together"**: you have another native mod (for example `mousetricks`) made for a different Horizon version than Sunset's. Native mods are built into the game together, so all of them must match. Turn the other one off on the launcher's Mods page (or delete its `.hnmod` from `horizon-native-mods`), then run the command again. You don't need to install the Nightly named in the release notes: the build downloads its source itself, so it's fine that it isn't on the Versions page.
- **Windows: "Compiling the game failed" with `windres: preprocessing failed` and a path cut off at a space**: your Windows user name has a space in it, and Horizon's own build can't handle that. The installer now moves the launcher's files to `C:\horizon-launcher` for you (you sign in again); older copies of the installer don't, so run the current command. You can also set the `HORIZON_LAUNCHER_HOME` environment variable to a folder without spaces yourself.
- **The command did nothing on Windows**: run it in *PowerShell*, not Command Prompt.
- **No Sunset in game**: you played the plain **Horizon** instance. Pick **Native mods**.
- **The first build is slow**: yes. It compiles the whole game once.
- **A module misbehaves or a server flags you**: type `.debug` right after and paste the report (it's on your clipboard) into the ticket.
- **Installer or launch still fails**: open a ticket on the [Discord](https://discord.gg/fhD2frFdzm) or an
  [issue](https://github.com/PointOfPressure/sunset-client/issues) with the installer output, your OS and the Horizon version.

## Plugins

Other native mods can add modules to Sunset. The API docs, an example plugin and a pack script are in
[sunset-plugins](https://github.com/PointOfPressure/sunset-plugins) (MIT).

## Building from source

`python3 tools/pack.py` writes `dist/sunset-<version>.hnmod`. To compile-check it against the Horizon Nightly it
pins, stage it with [horizon-mixins](https://github.com/horizon-rust/horizon-mixins)
(`horizon-mixins apply --workspace <horizon at that commit> --mods mixins --out stage`) and run
`cargo check -p horizon-client` in the stage. `tools/verify.py` runs the modules in a test arena in a headless
game; `tools/audit.py` lists which of Meteor's settings are still missing.

## Feature planning

See the [feature roadmap](docs/feature-roadmap.md) for what has landed, what is next and what each contribution needs.

## Credits

Made by [PointOfPressure](https://github.com/PointOfPressure). Contributors: vs. ([vstxx](https://github.com/vstxx): movement modes, Velocity, Scaffold controls and block counter, Scaffold Legit and Godbridge modes, Offhand's Auto Totem and its clicks between ticks, Kill Aura, Time and Weather Changer fixes, the Dashboard menu style and smooth scrolling, Server Scan, the Crystal Aura and Anchor Aura Macro sections and Anvil merge, this README), Rust ([zackmsa777-a11y](https://github.com/zackmsa777-a11y): Radar, profile files, Spawner Alerts, Item ESP names and filter, Anti Vanish, the roadmap, Crosshair, Block Overlay, Target ESP, Atmosphere skies, Crystal Aura, Anchor Aura (from Anubis), Crystal Macro and Anchor Macro (their Macro sections), Shield Breaker, Mace and Spear Swap (Auto Weapon's Axe on shields, Mace and Spear), W Tap, Jump Reset, Friend Guard, Auto Inventory Totem (Offhand's old Refill) and Hover Totem (Offhand's Hover), FastXP (Fast Use's Throwables), Key Throw, world labels sharing the menu's glyph scale, faster Atmosphere skies, .goto, Knockback Delay in Velocity). Want to help? See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

Sunset 0.8.1 and later is under the [PolyForm Strict License 1.0.0](LICENSE): you can use it for yourself (not commercially) and read the source; you can't change it or share copies. Plugins are fine, and the plugin docs and example in [sunset-plugins](https://github.com/PointOfPressure/sunset-plugins) are MIT. Versions up to 0.8.0 stay under the MIT licence they were released with. Many modules follow the behaviour and settings of [Meteor Client](https://github.com/MeteorDevelopment/meteor-client) (GPL-3.0), written again in Rust for Horizon. Scaffold's Legit and Godbridge modes, Offhand's Auto Totem, Kill Aura's Silent aura mode, Aim Assist's Simple mode, Anti AFK's Wander mode and No Slow's Skip mode follow [vape-rs](https://github.com/rdbtCVS/vape-rs) by rdbtCVS (MIT, notice in [LICENSE](LICENSE)). Crystal Aura, Anchor Aura and Protect loot follow Anubis 1.0 by 4ldenz (Horus) (MIT, notice in [LICENSE](LICENSE)), ported by Rust. To contribute, see [CONTRIBUTING.md](CONTRIBUTING.md).

<details>
<summary>Font credits</summary>

The menu and HUD fonts are [Sora](https://github.com/sora-xor/sora-font), [Inter](https://rsms.me/inter/), [Poppins](https://github.com/itfoundry/Poppins), [Outfit](https://github.com/Outfitio/Outfit-Fonts), [Manrope](https://github.com/googlefonts/manrope), [Geist](https://github.com/vercel/geist-font), [Montserrat](https://github.com/JulietaUla/Montserrat), [Plus Jakarta Sans](https://github.com/tokotype/PlusJakartaSans), [Lexend](https://github.com/googlefonts/lexend), [Rubik](https://github.com/googlefonts/rubik), [Space Grotesk](https://github.com/floriankarsten/space-grotesk), [Red Hat Display](https://github.com/RedHatOfficial/RedHatFont), [Comfortaa](https://github.com/alexeiva/comfortaa), [Quicksand](https://github.com/andrew-paglinawan/QuicksandFamily), [Nunito](https://github.com/googlefonts/nunito) and [JetBrains Mono](https://www.jetbrains.com/lp/mono/) (all SIL Open Font License 1.1, the licences are in `assets/licenses/`), cut down to the characters Sunset draws (`assets/*.ttf`, made by `tools/font.py`). Sora is the default; choose with **HUD > Font** (*Minecraft* is the game's own).

</details>
