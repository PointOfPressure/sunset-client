# Server watchlist configuration interface

`mixins/sunset/watch_config.rs` is pure Rust with no external dependencies, game hooks, network operations, or donor source. `Profile` exposes `id`, `addresses`, `players`, and `command`; `Config` exposes `profiles`. Prefer `Config::upsert(Profile)` to direct mutation: it normalizes a candidate and changes the original only after validation. `remove(id)`, `profile(id)`, and `list()` provide editing support. `Config::defaults()` provides a DonutSMP preset with only `donutsmp.net:25565`, an empty editable roster, and command `msg`.

`Config::resolve(selection, Option<connected_address>)` accepts `auto` or a profile id. Auto returns `None` when disconnected or unmatched. Explicit selection works while disconnected. Matching is exact normalized hostname/IP plus effective port; DNS names are ASCII, lowercase, and may have one terminal dot. Missing ports use 25565. IPv4 and IPv6 use standard-library parsers; bracketed IPv6 supports explicit ports, and bare IPv6 always means the whole address at port 25565. IPv4-mapped IPv6 remains IPv6. There is no DNS lookup, SRV resolution, substring match, brand recognition, IP/hostname equivalence, or automatic server trust. If the host exposes a resolved IP rather than the entered hostname, users can explicitly add that IP as an alias.

Profile ids are 1–32 ASCII letters, digits, `-` or `_`, beginning with a letter or digit; normalization is lowercase and `auto` is reserved. At most 32 profiles and 8 aliases per profile are permitted. Multiple aliases in the same profile are deduplicated; an alias claimed by different profiles is an error. Zero aliases permits explicit-only profiles. Player names are Java-format ASCII letters, digits or underscores, 1–16 characters, with case-insensitive deduplication and first spelling retained. The maximum is 128 players per profile. `parse_players(text)` accepts comma or ASCII whitespace separated editor text up to 4096 bytes. Bedrock-specific names are not currently supported. `valid_player(name)` exposes the same name predicate.

Completion commands are names from the allowlist `msg`, `tell`, `w`. Do not pass arbitrary command text into a profile or append user-provided command arguments. The detector controls opt-in, pacing, and evidence; selecting or editing a watchlist does not perform a completion request.

`Store::new(trusted_mod_data_directory: PathBuf)` uses the fixed file `watchlists-v1` under that directory. No command accepts an input path. `load()` returns defaults only if this file is missing; malformed, oversized, non-UTF-8, linked, or inaccessible files produce a visible error without overwriting them. `save(&config)` validates and serializes first, writes and syncs an exclusive sibling temporary file, then replaces the destination with safe `std::fs::rename`, matching the existing profile store. The old destination is never deleted first. Owned temporary files are removed after failure; preexisting temporary files are not removed. Parent directories, the data directory and existing destination are checked for links/reparse points. These are application checks on a trusted local mod directory, not a substitute for OS isolation against simultaneous hostile filesystem mutation. No multi-file transaction or power-loss durability guarantee is claimed.

For editing, clone the active config, mutate the candidate, `store.save(&candidate)?`, and only then install the candidate. Keep the active config and show the error if saving fails. On load errors, preserve the on-disk file and show the error; do not silently rewrite defaults. The store also refuses to overwrite an existing file that fails to load, so a caller retaining an in-memory fallback cannot accidentally destroy malformed user data. Repair that file manually before saving again. `serialize()` validates the whole config; `parse()` returns a complete valid config or an error. The maximum file size is 128 KiB.

The strict v1 schema is UTF-8, LF terminated, with the exact ordered fields below. Blank lines, unknown fields, duplicate ids, unsupported versions, truncated records and empty list elements are rejected. Empty `addresses=` and `players=` lists are allowed. Values cannot introduce extra lines because their grammars are validated. There is no general string escaping:

```text
sunset-watchlists=1
profile=donutsmp
addresses=donutsmp.net:25565
players=
command=msg
end
```

Run `python tools/test_watch_config.py` for standalone production-code tests using the pinned Rust compiler. `RUSTC` can select an explicit compiler on another machine. Tests cover exact hostname/port and IPv6 matching, aliases, roster validation, transactional editing, strict schema, bounds, failed storage operations, and Windows replacement failure while the original file is locked. They perform no game or server operations.
