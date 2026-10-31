# Visual behavior research

The user supplied an Anubis visual demonstration and inert static evidence.
Earlier review observed Crosshair, Block Overlay and Target ESP entries in the
video's visual-module list. Listing alone does not demonstrate their operation.
Functional reference research also inspected the corresponding decompiled
render-module descriptions. Anubis author metadata names **4ldenz**; original
ownership and credit remain with the original authors.

This contribution implements generic crosshair, block-selection and target-marker
behavior in original Rust using pinned Horizon APIs and Sunset's existing batch.
It includes no Anubis Java, shaders, models, fonts, logos or other assets. It does
not translate the reference's animated target glyphs, gradient/flow selection
effects, custom skies or particle systems, and does not claim feature parity.

Horizon code and APIs retain their existing notices. Sunset retains its project
license; no project-wide relicensing is part of this contribution. Reference
credit is recorded here in source documentation rather than in module labels.
