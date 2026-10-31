# Atmosphere

Atmosphere changes Minecraft's world sky. Enable it under **Render**, or type
`.menu atmosphere` to open its settings. It starts disabled.

- **Space** draws a magenta/blue nebula, dark dust lanes and a directional star field.
- **Aurora** draws folded green/violet light curtains over the stars.
- **Ringed Planet** adds a shaded planet and tilted rings to the space sky.

The effects follow world-camera direction. Terrain, buildings, hands and the
ordinary interface keep their existing rendering. F1 hides the interface while
the sky stays visible; the effect also remains behind menus.

## Controls

**Brightness**, **Effect strength**, **Stars** and **Speed** control the look.
Speed zero freezes procedural motion. **Daytime strength** controls visibility
in daylight; zero leaves the daylight sky ordinary, while one keeps the full
effect during the day. Day/night blending follows solar height, independently
of rain. The custom sky remains visible in rain; weather particles, fog and
server weather state keep their normal behavior.

**Quality** selects Performance, Balanced or Detailed. All render at the native
framebuffer resolution. Quality changes procedural detail rather than image
resolution: cloud layers are capped at two/three/five noise octaves, star
layers at one/two/three and aurora curtains at one/two/three. Balanced is the default.

Setting changes settle for 250 ms before rebuilding the specialized pipeline,
so dragging sliders does not compile a pipeline for every intermediate value.
The first enable and a new settled configuration can incur shader compilation.

## Compatibility

This initial version supports the Overworld. Titles, loading frames, other
dimensions, blocked-sky effects, fluids and active shader packs use the ordinary
sky path. Existing sun, moon, clouds and terrain lighting are retained. A shader
validation failure also falls back to the ordinary pipeline.

The effect reuses the existing sky pass and Globals buffer. It adds no textures,
offscreen targets, render passes or extra sky draw calls. Procedural fragments
still cost GPU time; no universal FPS improvement is claimed. A bounded cache
keeps the original and active effect pipelines. Surface release and the normal
application-exit callback clear cached GPU resources before renderer teardown.

The module is a focused original implementation of three visual directions. It
does not implement the reference client's hand aura, knife skins, black hole,
meteors, terrain grading or its complete settings catalogue.

See [source research](atmosphere-source-notes.md) for credit and
[native validation](atmosphere-validation.md) for screenshots, timings and pins.
