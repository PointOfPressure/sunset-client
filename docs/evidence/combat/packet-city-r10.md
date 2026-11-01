# R10 native packet, City, Trap and Optimizer outcomes

[Sanitized observations and original image hashes](packet-city-r10.json) use the
exact [R10 client checkpoint](checkpoint-r10.json), with two actual native players
on the private stock Java26.3 fixture.

All **eight packet scenarios** now have passing R10 outcomes:500/1000ms movement
holding, action/menu/disable flushes, same-world correction, actual sword
knockback delay and a same-connection Nether round trip. The first500ms attempt
remains a recorded sampling failure: a slow console query straddled its cutoff.
A separate warm rerun passes. Query start/end bounds and native-before/after
positions are retained; these are observations, not exact packet-arrival times.

The1000ms case first observed server movement at1.266s, replacing the early
R7 result0.443s. Both knockback arms took1.3999996 actual damage from20HP with
ResistanceIV. Upward motion began0.135s after the baseline click and0.933s with
800ms configured, approximately0.815s after observed health loss. Native state
sampling gaps stayed below52ms.

City/Trap/Optimizer passed **13/13**. City opened one actual surround block and
restored the original slot. The real quick mouse press/release now cancels it.
Auto Trap completes all eight ring cells, one outside scaffold and the roof:
exactly ten obsidian consumed, actual airborne native motion and slot9 restored.
[The completed R10 enclosure](trap-complete-r10.png) is separate from the retained
[R7 stalled enclosure](trap-stalled-r7.png). Off/friend/manual-input controls pass.

Direct inspection of four original1280×720 captures verifies rejected-hit
Optimizer behavior with a real server-only invulnerable crystal:

- [Before](optimizer-before-r10.png): crystal present.
- [After the rejected hit](optimizer-masked-r10.png): crystal hidden.
- [After timeout](optimizer-timeout-r10.png): crystal visible again.
- [Repeated hit](optimizer-repeat-r10.png): crystal stays visible.

The same authoritative ID remains in the client list and on the server, health
stays unchanged and there is no removal ACK. These PNGs were copied byte-for-byte
without editing, resizing or compositing. Both players were restored to20HP with
empty inventories/effects; all modules and logical/physical inputs were off,
official control streams closed, and the pair explicitly handed off.
