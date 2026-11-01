# R7 packet, City, Trap and Optimizer player outcomes

[Sanitized results and image hashes](packet-city-r7.json) refer to the exact
[R7 binary checkpoint](checkpoint-r7.json), tested with two actual native
clients against the private stock Java26.3 server. Raw clocked control and
console replies remain in the private reports identified by filename/hash.

Packet tests passed **7/8**: 500ms movement delay, action/menu/disable flushes,
same-world correction, real sword knockback delay and a same-connection Nether
round trip. Both knockback arms dealt1.3999996 actual health damage from20HP
with ResistanceIV. Upward motion began0.136s after the baseline click; with
Chance100 and800ms configured it began0.932s after click, approximately0.814s
after the observed health loss. Sampling gaps were under52ms. Server-position
query intervals are observations, not exact wire-arrival timestamps.

The **1000ms case failed**: an old idle marker caused fresh movement to flush
early, first observed at0.443s. R9 changes per-entry expiry and drains due
prefixes before capacity checks. This R7 failure remains a failure until a
separate live rerun verifies that fix.

City/Trap/Optimizer passed **11/13**, plus a separate **hotbar override1/1**.
City opened exactly one actual server surround block, accumulated native
mining progress and restored slot9. Off, friend, disable and separate hotbar2
cancel controls passed. The quick native mouse-click cancel failed because
level-only tick polling missed its synchronous press/release; R9 adds an
actual button-edge notification.

Trap off, friend and held-input yield passed. Full completion failed after
four actual obsidian blocks: its near two-high column occluded farther faces.
The unchanged [failure image](trap-stalled-r7.png) is retained. R9 orders each
height farthest-first and removes the rotating start cursor. The renderer-free
`tools/trap_visibility_tests.py` compiles actual native outline traversal and
production face/support/order routines; it reproduces the four blocked R7
cells, then reaches eight lower cells and scaffold+roof using the corrected
order at a jump-reachable eye height. Its immediate-ACK/unit-cube geometry
seams do not replace a live R9 completion test.

Optimizer's ordinary on/off and Weakness controls passed. Its rejected-hit
visual test used a real server-only `Invulnerable:1b` crystal and actual native
attack. Direct inspection of four original1280×720 captures confirms:

- [Before](optimizer-before-r7.png): crystal geometry is present.
- [After rejected hit](optimizer-masked-r7.png): geometry is absent.
- [After timeout](optimizer-timeout-r7.png): geometry returns.
- [Repeated hit](optimizer-repeat-r7.png): geometry stays visible.

The same authoritative crystal ID stayed in the client list and on the server
throughout; player health stayed unchanged and there was no remove ACK. The
images were copied byte-for-byte without resizing, editing or compositing.
Counters supplement the actual images/entity observations, rather than serving
as the visual proof themselves.
