# Optional native visuals

This development contribution adds three Render modules. All start disabled and
use the existing native HUD batch. Open the menu with Right Shift or backtick,
or type `.menu crosshair`, `.menu block overlay`, or `.menu target esp` in chat.

## Crosshair

Choose Cross or Dot, foreground and outline colors, length, gap, thickness and
an optional center dot. Dimensions are physical pixels, independent of GUI scale.
Set Outline to zero to remove it. The original attack indicator stays intact.

Custom drawing follows the native first-person, spectator and debug-crosshair
rules. It hides with F1 and leaves the ordinary drawing path in menus. Turning
the module off restores the vanilla sprite in the same frame.

## Block Overlay

Colors the block under the engine's current pick using its actual selection
shape, including contextual bounds and model offsets. Outline and Fill have
independent switches and opacity; line width is a multiplier. Disabling the
module, hiding the HUD, entering a menu, or making both styles transparent
restores the native selection stroke. Removed targets leave no retained marker.

The custom selection geometry is projected into the HUD rather than depth-tested
against terrain. Shapes crossing the near plane can be omitted. Fill uses the
host's component boxes; overlapping parts may blend darker.

## Target ESP

Draws corner brackets around one current Kill Aura target or the living entity
under the native crosshair. Target selects Combat or crosshair, Combat, or
Crosshair. Range, players-only filtering, color, padding, corner length,
thickness and outline are configurable. Invisible entities, armor stands,
removed entities, dead targets and the local player are excluded.

Through walls starts off. Visibility uses the host's shape-aware block ray to
the target center once per tick; this is not a per-pixel depth test. Crosshair
selection uses native interaction reach, independently of the maximum range.
Targets crossing the camera's near plane disappear until they can project safely.

The module has no manual last-hit memory, animations or target history. Its
cache checks level identity and entity identity before drawing, and emits at
most sixteen rectangles. It does not scan the entity list every rendered frame.

## Scope and validation

This is a focused visual contribution, not a complete port of another client's
visual effects. It adds no shaders, particle systems, textures or extra rendering
passes. Runtime evidence and exact pinned build results are recorded separately
in [native validation](visual-validation.md); pure geometry tests alone do not
establish in-game appearance or performance improvements.

See [source research notes](visuals-source-notes.md) for reference credit.
