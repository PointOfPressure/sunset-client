# Checked rival-history storage

The Crystal PvP Bot's pending port saved a bounded history snapshot on a worker
thread, but used `create_dir_all`, a predictable `<uuid>.tmp` and ordinary file
reads. Existing symlinks or Windows junctions could redirect those operations.
Checking a path only before starting a worker would also miss a link installed
while that worker waited.

`mixins/sunset/rival_store.rs` moves filesystem checks into the actual read/save
operations. It accepts an engine-owned data directory and numeric UUIDs, with no
filename or path supplied by a rival record.

- Validate each ancestor with `symlink_metadata`, rejecting symlinks, reparse
  points and wrong file types before creating the next directory.
- On Windows, open checked directories without following reparse points and
  keep their list/read handles open without delete sharing during the operation.
- Read at most 16 KiB plus one byte from a checked regular file; reject oversized
  data and invalid UTF-8. Reads never create directories.
- Create temporary files exclusively, write and sync them, recheck the final
  filename and rename. An occupied temporary name is never truncated or removed.
  A failed replacement preserves the previous complete record.
- Save at most 64 records. Pruning examines at most 256 directory entries,
  ignores linked/non-file/unrelated entries and retains 64 matching plain records
  among those examined. This is bounded retention, not an exact global cap when
  more than 256 entries already exist.

The existing worker still owns only a snapshot and a path, keeps disk work off
the game tick, and releases its busy flag with its existing drop guard. There is
no fallback to unchecked I/O on error.

## Port integration

This PR starts at upstream main `d8cf3c7`, which contains part 1 of PR #16 but not
the Crystal PvP Bot. It intentionally does not reintroduce the Bot or modify #16.

[The integration patch](patches/crystal-bot-rival-store.patch) registers the
helper and replaces the Bot's read/save/prune code. It applies to the unchanged
PR #16 source at `649fc3ff6d888184ad5131cc2a2a9a8232da0489`; apply those two hunks
while porting the Bot, adjusting only if the port moves its module/file names:

```sh
git apply --check docs/patches/crystal-bot-rival-store.patch
git apply docs/patches/crystal-bot-rival-store.patch
```

Until those integration hunks are applied, the helper is not registered or active
in main. The schema and learned-rival policy are unchanged.

## Validation

```sh
python tools/rival_store_tests.py --out rival-store-result.json
```

[The Windows receipt](evidence/rival-store-windows.json) records **20 passing
tests**, compiled with `rustc -D warnings`, using the unchanged production module
and real isolated filesystem fixtures. These cover junctions at the data,
rivals, ancestor and record paths; a delayed worker; Windows directory swapping;
hard-linked temporary collisions; reads/limits; replacement and pruning.
No test is ignored. Generated fixtures are removed after each test.

An actual locked destination also forces replacement to fail after writing the
temporary file: the old record remains complete, the owned temporary is removed,
and a retry succeeds once the lock closes.

The development run caught two Windows details: attribute-only directory handles
did not prevent renames, and `absolute()` normalizes parent components before
inspection. The final implementation uses directory-list access and checks the
input components before normalization. The final 20-test run includes both
regressions. This is filesystem evidence, not an in-game behavior or FPS test.

Windows directory swapping is tested here. Unix gets component/file preflight
checks; this patch does not claim descriptor-relative protection against a
concurrent hostile Unix directory rename. Unix execution was not tested here.
