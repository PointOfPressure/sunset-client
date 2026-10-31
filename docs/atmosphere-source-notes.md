# Atmosphere source research

The user supplied Anubis's visual demonstration and inert decompiled evidence.
Original-author metadata names **4ldenz**. The video shows a magenta/blue Space
sky, Northern Lights and a Ringed Planet, with daytime strength, brightness,
speed, star and effect controls. Functional research inspected
`AtmosphereModule.java` and `AtmosphereShaderSky.java`; supplied code was not
executed. Original ownership and credit remain with their respective authors.

This contribution contains original Rust/WGSL using Horizon's native rendering
APIs. It includes no donor Java, shader expressions, sky textures, models,
logos or other assets. Noise, ray/sphere/ring geometry, stars and aurora are
independent procedural implementations. Source-reference credit is kept here;
the in-game module and modes use generic names.

Important scope differences include Overworld-only support, three bounded
quality choices, retained native celestial bodies/clouds, explicit shader-pack
fallback and a single existing sky pass. Rain visibility is intentional while
native weather remains active. This is not a complete or pixel-identical port.

Existing Horizon and Sunset notices remain unchanged. No project-wide license
change is part of this contribution.
