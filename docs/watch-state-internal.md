# Watch evidence engine integration

`mixins/sunset/watch_state.rs` is an independent std-only state machine. It does not import game types, send packets, read system time, infer permissions, inspect sounds, or combine evidence across players. It implements functional observation rules independently; no Anubis source is included.

## Lifecycle and input contracts

Create one `WatchState` per runtime. Call `configure(Config, &[String])` with the enabled module state, passive TAB option, completion option, and watched names. Passive TAB defaults to true and completion defaults to false. Effective configuration changes reset the connection evidence and account bindings. Name order and ASCII case alone do not count as changes. Configuration considers at most the first 128 names, accepts only 1–16 ASCII letters/digits/underscores, and deduplicates names ignoring case.

Call `reset()` on disconnect, server transfer, and profile change, including changes between profiles with identical options. Never feed a disconnect's empty roster into the previous connection. Identity, pending removals, baseline, evidence, alert deduplication, per-person probe cooldowns, and the in-flight probe reset. The monotonic clock watermark, global request rate limit, and request-ID watermark intentionally survive reset, preventing delayed responses or rapid toggles from bypassing those guards.

All time inputs use the same process-monotonic millisecond epoch, including across connections. Calls older than the last accepted time are ignored. Equal timestamps are allowed. The engine never reads wall-clock time. Read-only `rows()` clamps a backwards time to the last accepted observation time.

`update_roster(now_ms, players, known_leaves)` requires a **complete** snapshot, including server roster entries whose `listed` field is false. An empty connected snapshot is valid; an unavailable, disconnected, or truncated snapshot must not be submitted. Inputs over 4096 players or containing duplicate UUIDs are ignored. UUID and unique name indexes are built once per accepted snapshot, making updates proportional to roster plus watcher count. Spectator state is descriptive and does not itself produce evidence.

Names resolve to a UUID only from an unambiguous authoritative roster entry. Once resolved, the entry follows that UUID for the rest of the connection. Reusing its name under another UUID does not rebind it. A name update on the already bound UUID is authoritative and updates the display/probe name. The same UUID cannot bind two configured entries. Configuration or lifecycle reset deliberately discards these bindings.

`known_leaves` contains explicit departed UUIDs, when available; at most 4096 are considered. An absent explicitly departed account clears observations and cancels removal evidence. It is not probed until it returns. Without an explicit leave signal, disappearance remains unconfirmed.

## Evidence and alerts

The first roster establishes a baseline. An initially absent or unlisted name produces no passive disappearance alert. A player must have remained listed for at least 1500 ms before a transition can produce passive evidence:

- Listed to present-but-unlisted: `Hidden from TAB`.
- Listed to missing: start a 1000 ms grace period. A return cancels it; otherwise emit `TAB disappearance; unconfirmed`.

Neither statement establishes that the player enabled a vanish feature. Normal departures can cause the unconfirmed statement when no explicit leave signal exists. A listed return clears active evidence. Passive observations can be disabled while roster identity and opt-in completion checks continue working.

`update_nearby(now_ms, local_position, entities)` accepts a separate complete snapshot of actually loaded player entities. Root must adapt real remote entity UUIDs and positions, not TAB entries or generic activity. Only an already resolved UUID that is off TAB, with finite local/entity coordinates and Euclidean 3D distance at most 64 metres, produces `Nearby entity while off TAB`. An unresolved name, a different UUID, non-finite coordinates, and a distance beyond 64 metres produce no proximity evidence. A subsequent valid nearby snapshot that lacks the entity clears current proximity evidence. Oversized nearby snapshots are ignored.

Per-name evidence expires 60 seconds after its observation. Presentation priority, highest first, is nearby entity, hidden from TAB, unconfirmed disappearance, then suggested while off TAB. This is an enum ordering rather than an aggregate score. Each update returns alerts for the current strongest evidence. Repeated or weaker alerts for an entry are suppressed for 60 seconds; escalation can alert immediately. Memory is bounded by configured watchers and current bounded snapshots. `rows(now_ms)` provides display name, optional UUID, presence/listed/spectator status, and the strongest unexpired evidence.

## Completion probe wiring

Completion is explicitly opt-in. The adapter must also suppress scheduling while chat or a command-block editor is open, and while no stable connected session exists.

1. Ask `probe_candidate(now_ms)` for an off-TAB watched name.
2. Allocate an ID from the adapter's reserved negative namespace. The planned adapter begins at `-1073741824` and decreases. Never reuse an ID, including after reset, profile changes, or reconnect. Stop probing rather than wrapping at `i32::MIN`.
3. Send the completion request for that exact name and call `probe_sent(now_ms, id, name)`. A failed send must not be reported as sent. Check its boolean result; state or candidate changes may reject the reservation. The adapter should perform these operations together on its runtime event thread.
4. Route replies in the reserved namespace to `completion_reply(now_ms, id, &[String])`. Consume those replies before they reach ordinary chat suggestions. Leave normal chat IDs and behavior intact; discard stale replies in the reserved namespace.

The engine accepts only negative IDs strictly lower than every previous accepted probe ID. It allows at most one in-flight request, at most one request per 1000 ms globally, and at most one per 10 seconds per configured person, rotating through candidates. Requests time out after 5000 ms; a reply exactly at 5000 ms is still valid. An unrelated ID does not consume the outstanding request.

A reply is evidence only if its ID matches the outstanding request, its age is within 5000 ms, the entry is still configured and off TAB, and one of the first 128 results is an exact ASCII-case-insensitive name match. Prefixes, suffixes, command syntax, whitespace, and a match beyond the bounded results are rejected. A return to listed TAB cancels the request. A name listed under another UUID, an ambiguous same-name roster, or a known conflicting UUID also blocks probing and reply attribution.

The only completion statement is `Possibly online: suggested while off TAB`. It does **not** confirm physical proximity, entity presence, server-wide online status, or vanish. A server may return cached, synthetic, permission-filtered, or otherwise unreliable suggestions.

## Offline verification

Run `python tools/test_watch_state.py`. The runner uses the pinned standalone Rust compiler by default, supports `SUNSET_TEST_RUSTC` for an explicit alternative, writes test artifacts into a temporary directory, and does not use the shared Cargo target, launch a game, or connect to any server.

The fixture harness covers baseline enablement, stability/grace boundaries, rejoins and explicit departures, case-insensitive binding, authoritative renames and conflicting UUIDs, passive options, spectator metadata, reconnect/profile/config resets, stale/out-of-order/unsolicited IDs, normal chat ID isolation, request/result bounds, rate limits, per-person fairness, timeout boundaries, backwards clocks, evidence expiry, escalation/deduplication, finite distance/UUID proximity checks, and mass removals remaining unconfirmed.
