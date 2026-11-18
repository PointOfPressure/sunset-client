# Horizon patch series

Patches applied on top of the pinned Horizon (`../horizon-native.json`) for hooks Sunset needs that Horizon lacks.
Apply in file-name order. The patched Horizon source is published on the PointOfPressure/horizon fork, one branch
per pin (Horizon is LGPL-3.0-or-later).

- `launcher-session-hidden.patch`: horizon-launcher side of the hidden-session handling (see `docs/hard-panic.md`).
