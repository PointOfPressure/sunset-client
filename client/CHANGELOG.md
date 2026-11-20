# Changelog

What changed in each Sunset release, one `## <version>` section each, newest first. The release scripts copy the
section of the version they build into the signed manifest, and the launcher shows it on Overview.

- Sections: `### New`, `### Changed`, `### Fixed`, `### Removed`.
- One short line per change (`- ...`); the GitHub release notes keep the long form.
- An important change is written in bold (`- **...**`): the launcher highlights it.

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
