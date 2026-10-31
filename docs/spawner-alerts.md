# Spawner Alerts

Enable **World > Spawner Alerts**, or use `.toggle spawneralerts`. **Range** is
the three-dimensional discovery distance from the player to the block center
(default 64, configurable 8–256 blocks). **Chat**, **Labels**, and **Max labels**
(default 8, maximum 32) control the output. Block ESP's existing **Spawners**
preset remains available for boxes.

Discovery chat includes coordinates, distance and the current mob type. Mob names
come only from loaded native spawner metadata; missing metadata stays **Unknown**.
Normal and trial spawners are supported. Labels refresh when metadata changes;
the mob type is never inferred from nearby mobs. There are no chunk requests or
forced loads.

A position alerts once per connection and dimension, including after chunk reload,
disable/re-enable, a range change, or returning from another dimension. Disabling
clears pending messages and labels but keeps discovery history. Dimension changes
clear pending output and the scan; returning preserves that dimension's history.
Disconnecting resets the session. A missing player during an active connection
does not reset history. Turning Chat off discards pending messages; discoveries
made while Chat is off do not alert when it is turned back on.

Chat emits at most one discovery message every 40 game ticks. Each message holds
at most three detailed discoveries and a count of the remaining discoveries.
Chat distances describe discovery time; label distances use the current player
position. Labels come from the nearest 32 candidates of the most recent complete
scan, with at most Max labels drawn on screen. Selection can lag movement by one
scan pass. Every rendered label rechecks range, loaded chunk, block state and
metadata, so unloaded/replaced blocks do not leave ghost labels.

## Work and memory bounds

Each native resident ChunkMap instance owns a sparse index of at most 16,384 spawner positions.
Native `Streaming::receive_generated` observes incoming saved/generated block
entities, adding one O(log 16,384) update per incoming entity alongside the engine's
chunk-load work. Native `Game::sync_block_entity` observes the incoming block state
before it synchronously creates or updates the block entity, on block updates and
remote chunk arrival. Candidates are validated against the resulting actual state
and block entity before output. Both multiplayer and supported
threaded singleplayer windows use that remote path. Updating an existing spawner's
metadata or ticking it through native `put` keeps its indexed position. This
does not patch generic BlockEntities operations or add process-wide pointer maps.

Each enabled tick seeks once in the ordered index and validates at most 256
candidates. Unloaded/replaced entries are pruned within that budget. A bounded
nearest list retains at most 32 entries per pass. Each render inspects at most 32
cached entries and emits at most 32 labels; it performs no world scan or sorting.
A full stable 16,384-entry pass takes at most 65 enabled ticks. Work does not grow
with loaded chests or mobs. Disabled modules do no discovery scans; native arrival
hooks still maintain the per-level index.

NBT identifiers are borrowed as UTF-16 slices. The formatted type label is cached
and rebuilt only when its metadata changes. Drawing reuses one distance-text
buffer. Already known candidates outside the nearest list need no decoded label;
history lookup borrows the current dimension key instead of cloning it per visit.

Session history holds at most 65,536 dimension/position keys. At that limit,
additional discoveries stop alerting; history is never evicted to re-alert old
positions. If the native index fills, later new spawners can be omitted until
their chunk/block metadata is loaded or synchronized again with free capacity.
One capacity notice per connection explains these limits. Pending chat stores
only three details plus a count. A new native level or cleared chunk map clears its index; reconnecting
clears history. Arbitrary plugins that insert a new spawner directly into the
native block-entity map without chunk arrival or block synchronization do not
register it with this index.

## Regression tests

With `rustc` on PATH, run from the repository root:

```powershell
rustc --test mixins/sunset/spawner_data.rs -o spawner-data-tests.exe
./spawner-data-tests.exe
rustc --test tools/spawner_index_tests.rs -o spawner-index-tests.exe
./spawner-index-tests.exe
```

The adapter tests compile the actual index adapter against a small representation
of the pinned engine API. Full mixin planning, compile checks and isolated live
world tests validate the actual host separately.
