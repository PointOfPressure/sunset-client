import unittest
import json
import os
from pathlib import Path
import tempfile
import zipfile

from plugin_release import inputs, require_passes, retain_times, validate_package


class RequiredResults(unittest.TestCase):
    def test_only_exactly_once_passed_checks_succeed(self):
        require_passes("PASS One - ok\nPASS Two - ok\n2/2 passed\n", ("One", "Two"))

    def test_missing_skipped_and_duplicate_results_fail(self):
        for log in ("0/0 passed\n", "SKIP One - absent\n", "PASS One - ok\nPASS One - repeated\n"):
            with self.subTest(log=log), self.assertRaises(RuntimeError):
                require_passes(log, ("One",))

    def test_failure_is_not_hidden_by_a_later_pass(self):
        with self.assertRaises(RuntimeError):
            require_passes("FAIL One - error\nPASS One - retried\n", ("One",))

    def test_manifest_and_packer_changes_are_bound_to_the_receipt(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "horizon-native.json"
            manifest.write_text('{"version":"1"}')
            before = inputs({"manifest": manifest})
            self.assertIn("manifest", before)
            manifest.write_text('{"version":"2"}')
            self.assertNotEqual(before, inputs({"manifest": manifest}))

    def test_stale_package_cannot_satisfy_a_new_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            package = Path(tmp) / "sunset.hnmod"
            manifest = {"id": "sunset", "version": "1"}
            with zipfile.ZipFile(package, "w") as archive:
                archive.writestr("horizon-native.json", json.dumps(manifest))
            validate_package(package, manifest)
            with self.assertRaises(RuntimeError):
                validate_package(package, {**manifest, "version": "2"})

    def test_cached_stage_keeps_only_unchanged_source_timestamps(self):
        with tempfile.TemporaryDirectory() as tmp:
            old, new = (Path(tmp) / name for name in ("old", "new"))
            old.mkdir()
            new.mkdir()
            for name in ("same.rs", "changed.rs"):
                (old / name).write_text("old source")
                os.utime(old / name, (1600000000, 1600000000))
            (new / "same.rs").write_text("old source")
            (new / "changed.rs").write_text("new source")
            changed_time = (new / "changed.rs").stat().st_mtime_ns
            retain_times(new, old)
            self.assertEqual((new / "same.rs").stat().st_mtime_ns, (old / "same.rs").stat().st_mtime_ns)
            self.assertEqual((new / "changed.rs").stat().st_mtime_ns, changed_time)
            self.assertEqual((new / "changed.rs").read_text(), "new source")


if __name__ == "__main__":
    unittest.main()
