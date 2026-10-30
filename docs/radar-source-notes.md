# Radar source notes

Radar is newly written Rust for Horizon and uses this repository's MIT license.
It was developed with AI assistance. The original upstream license is unchanged.

Feature research included Anubis (`anubisplus`, 0.9.9.3+mc1.21.11), whose supplied
metadata credits **4ldenz** and declares GPL-3.0-only. Its reconstructed
`RadarModule` and `HudRenderer.drawRadar` were read to understand radar behavior.
That reference is credited here and in source comments; its name does not appear
in the module's display name, client watermark or version.

The renderer, Horizon adapter, input validation and tests are newly written Rust.
The renderer uses cached vector geometry and physical-pixel edge coverage.
Shared behavior consists of ordinary player filters, planar rotation/distance
math and common radar conventions. No Java files, original assets, fonts,
binaries or dependency implementations from that reference are included.
This is not described as a clean-room process.

The reference's source URL is recorded from its metadata:
<https://github.com/4ldenz/Anubis-client>. A matching original-source revision was
not obtained. This note records development provenance, rather than assigning
ownership of general radar ideas, mathematical formulas or functional behavior.
