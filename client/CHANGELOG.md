# Changelog

What changed in each Sunset release, one `## <version>` section each, newest first. The release scripts copy the
section of the version they build into the signed manifest, and the launcher shows it on Overview.

- Sections: `### New`, `### Changed`, `### Fixed`, `### Removed`.
- One short line per change (`- ...`); the GitHub release notes keep the long form.
- An important change is written in bold (`- **...**`): the launcher highlights it.

## 2.1.0

### New
- **The launcher updates itself: when a release has a newer launcher, it downloads it, checks its signature as it does Sunset's files, puts it in place and restarts. 2.1.0 is the last launcher you download by hand**
- **Bed Breaker (World): breaks the nearest bed in reach that is not yours, mining through what covers it (or digging the bed through its cover); spares the bed beside you when it comes on**
- Profiles: `.profile copy <name>` puts a profile's text on the clipboard and `.profile paste <name>` keeps the one on it (to pass a config through Discord); `.profile import` alone lists the imports folder; a name that is taken asks whether to keep both or replace
- **HUD layout editor: each of Sunset's elements (watermark, module list, coordinates, FPS, speed, facing, totems, compass, Target HUD) has a size: with the chat open, drag the handle at the corner of its frame; a right click puts its size and place back. Text is drawn at the new size, not stretched, so it stays sharp**
- Effects: Kill effect (a burst or a pillar of light where what you hit dies), Crit sparks, Break bursts (chips off a block as it breaks) and Target aura (a ring or two lights round what you last hit), each with its own switch
- Effects > Hide behind blocks (on by default): effects no longer show through walls, and they fade with distance
- Fake Lag > Mode Repel: a hit on a target that is still red from the last one waits until it can be hurt again
- Fake Lag > Mode Hold has Breadcrumbs (the way you went while it holds) and Direction Bi-directional (what the server sends waits too)
- Fast Place > Speed goes to 9: speeds 6 to 9 place 30, 40, 60 and 80 blocks a second, more than one a tick (Anticheat Grim stays at one a tick)
- Fast Use > Uses per tick (1 to 4, with Cooldown 0)
- HUD > No auto pause: the pause menu stays shut when the game window loses focus (asked by Martin)
- **Typing `.` in the chat lists Sunset's commands, then each argument's choices as you type (modules, settings and their values, profiles, players...): Tab completes, Up/Down choose, Escape hides, as for / commands**
- ClickGUI > Style has a third menu, Sidebar: tabs down the side, modules in groups, each module a card (by Ace)
- Villager Roller, Sign Historian and Use Tracker; Speed > Mode Grim collide; Base Finder > Reasons
- Gooning (Client, off by default): a joke that times your session in chat, on your screen only
- **HUD > Smooth text (on by default): Sunset's text is spaced as its font spaces it, each letter drawn at the nearest quarter of a pixel, instead of every letter snapped to a whole pixel with its width rounded; turn it off for the old, harder look. Minecraft's font is unchanged**
- Debug Report has a Settings file section: the folder your settings are kept in, how they were read at this launch and how the saves since went (for settings that come back reset)
- Debug Report has a Trigger Bot section: of the ticks it ran since it was turned on, how many had something under the crosshair, how many of those hit, and what held each of the others (the cooldown, an item in use, Anti bot, Miss chance...), to compare two players' fights

### Changed
- Target HUD is a module of its own (Render) instead of a switch in HUD: its switch, mode and place carry over, and it shows with HUD off too
- **Blink is now Fake Lag's Hold mode: your Blink key, switch and Max ticks carry over, and `.t blink` still works**
- HUD's position, x and y settings are no longer rows in the menus: open the chat and drag an element (right click puts it back); saved layouts stay as they are
- **Sunset runs on Horizon main of 23 Nov 2026 (`2e7d2c6`, through Sunset's Horizon fork `9defaa0`) and horizon-launcher 0.2.108**
- **Plugins must be pinned to Horizon `9defaa0`; plugins made for 2.0 need that pin (and new targets for their own hooks on split game functions)**
- The launcher downloads the stock horizon-launcher from Sunset's own release, as Horizon's GitHub repository is gone
- **Crystal Aura is rewritten: one sequence at a time on the nearest enemy (obsidian, crystal, hit, again), bases scored by damage, knockback direction, turn and distance, silent look turning up to 90 degrees a tick (by vs.)**
- Crystal Aura: Break mode (Attack, Swap attack, Manual), randomised delays in ms, target and self prediction, and boxes for the target, the base and the next candidates
- Crystal Aura and Anchor Aura target players missing from the player list, wait for Scaffold only while it places, and say why nothing starts (by vs.)
- HUD > List color mode starts as Static (gradient off), in the same order as Watermark color mode
- Launcher: the Horizon import offer wraps and can be put off; pages follow a maximised or fullscreen window (by vs.)

### Fixed
- Hand View: a blocking sword no longer jumps between two poses on a 1.8 server (the game's own block while the block is held, Hand View's before it and between hits): it is Hand View's block all the way, whatever the Block animation (reported by vs.)
- No Render > Hide titles no longer lets a title flash for a moment when it arrives: it is not drawn at all (reported by pressure)
- Auto Fish, Anti AFK, Spam, Auto Shearer, Auto Nametag, Auto Breed, Auto Mount, Flamethrower, Liquid Filler and Spawn Proofer keep working with chat or a server's Esc menu open; Base Finder, Spawner Alerts, Sign Historian and Use Tracker behind any screen
- Packet Order (Client, on by default) is back: 2.0 had lost it, so actions made after the tick's move packet went out late (Grim's Post check). With it on, the look Sunset sends for a player standing still goes before the tick's end, not after it
- The launcher's Plugins tab builds again on a Horizon base it set up from Sunset's package
- Trajectories shows every loaded crossbow projectile at its Multishot angle, including all eleven arrows from Multishot V (by Rust)
- **New installs work again: the launcher sets up Horizon from Sunset's signed package, never from Horizon's releases (Horizon deleted them)**
- Cancel stops Install while it prepares Minecraft's files, and the next Install prepares them again
- Install and Repair prepare Minecraft's files again when an earlier prepare stopped half way ("is not prepared yet" at Play) (by vs.)
- Minecraft's download from Mojang is tried again when the connection drops, and its errors no longer repeat the file counter (by vs.)
- Offline play works before the first signed-in Play ("cannot acquire launcher cache leases") (by vs.)
- When the game crashes, the launcher says why (from the game's log) instead of keeping Play's last step (by vs.)
- Windows: the launcher opens without a console window (by vs.)
- Client Spoofer works again, and old Client Spoofer profiles keep to their own sessions (by vs.)
- Offhand: Auto Totem's inventory takes no other clicks, and its swap waits until the server knows you stand still (by vs.)
- Auto Clicker skips a repeat when the same action is already queued and follows Horizon's mapped buttons; Trigger Bot checks the held attack and the screen again before a queued hit (by vs.)
- Launcher: a failed sign-in reads plainly and doesn't count as played (by vs.)

## 2.0.0

### New
- **Sunset is a .hmod package for Horizon Nightly 2026-11-18 (`0b14f8a`)**
- **The Sunset launcher: sign-in, signed install, update and repair, Play, import, offline play for owners**
- **Plugins: the launcher builds them into Sunset on this computer (Plugins tab)**
- Kill Aura: 1.8 / 1.9+ version, Hit select, Velocity boost, Miss swing, Advanced rotation, Movement correction, Cooldown jitter
- Anti Void: Blink mode; Anti Vanish: HorizonSMP mode; Client Spoofer: Fake resource packs
- .relog, survival tools, auction modules, ESP villager filters, Sound Detector markers
- Effects: landing ripples and explosion shockwaves

### Fixed
- Loading a profile no longer re-applies module presets over its settings
- Settings are read before joining and never saved before they are read
- Keys Sunset holds are held as the keyboard holds them again

## 1.2.1

### Fixed
- **Sunset's text (ClickGUI, HUD, module list) draws again instead of boxes**

## 1.2.0

### New
- HUD layout editor: drag any HUD element with the chat open; right click puts it back
- HUD: Text case, Text shadow, Static, Fade, Breathe and Rainbow themes and an animated module list
- ClickGUI search: best match first, finds initials, small typos and setting names; Enter toggles the top result
- Geode Finder, Light ESP, Sound Detector and Base Finder
- Anti Vanish: spectators, invisible and unlisted players
- Inventory Manager: Slots mode, a classic 1.8 fixed-slot manager (the new default)
- Hand View: 1.9+ and 1.8 animation sets
- Speed: NCP Hop; Flight: Old NCP (both experimental)
- Trigger Bot has its own Combat version; Profiles has a Save button

### Changed
- Kill Aura: quicker hits, 1.8-style Legacy swings and NoCheatPlus reach; leaves the look to Scaffold while it places
- Scaffold: Disable with Kill Aura pauses only while Kill Aura has a target; new defaults
- Profiles and the config: exact numbers, safe writes with a backup, unknown lines skipped
- The module list stays clear of the status effect icons

### Removed
- **Burrow, Hole Filler, Auto City, Key Throw, Arrow Dodge, Hold to Eat and Target ESP (profiles with them still load)**

### Fixed
- Profiles with Fake Lag's old Range choke value load again
- Streamer Mode no longer slows down text drawing
