# Plugins

Plugins compile into Sunset. The plugin API is `client/mixins/sunset/api.rs` (built into Sunset, so it stays beside
the mod); its reference, template and example plugins are in
[sunset-plugins](https://github.com/PointOfPressure/sunset-plugins). Keep them aligned when the API changes.

- `plugcheck/`: the test plugin verify.py's `Plugin API:` checks drive (staged next to Sunset on the test machine
  only, never shipped).
- `plugsmoke.py`: builds Sunset with the SDK's example plugins and plugcheck, then runs those checks. Run it before a
  release; its options are at the top of the script.
