# Radar

Disabled by default. Open the ClickGUI, search for **Radar**, enable it, and
expand its settings:

- **Range:** horizontal radius, 64 blocks by default.
- **Size:** diameter in GUI units, 100 by default; fits smaller windows.
- **Rotate:** heading-up when enabled; north-up when disabled.
- **Players color:** the client's existing color palette.

The center marks you; other dots represent loaded, living, visible,
non-spectator players in range. The module uses the bottom-left HUD corner,
respects F1/debug-chart reservations, and stays hidden while the ClickGUI is open.
Settings use the existing configuration system.

The circle and dots are vector geometry in the existing HUD vertex batch.
Tessellation follows physical radius, and edge coverage uses one device pixel
at the current GUI scale. There is no GUI-resolution scanline image to enlarge,
offscreen radar texture, shader, blur, particle effect or extra rendering pass.
Circle directions are cached once. No block scans or network requests are added.
Labels use the existing client font path.

See [source notes](radar-source-notes.md) for the behavior-reference credit and
[verification](radar-checks.json) for the exact tested scope. Source/test results
are distinct from complete multiplayer or all-module validation.
