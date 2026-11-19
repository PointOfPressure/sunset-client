"""plugsmoke.py's verdict on verify.py output, without building or starting a game."""
import ast
from pathlib import Path
import re
import shutil
import tempfile
import tomllib
import unittest

SOURCE = Path(__file__).with_name("plugsmoke.py")
TREE = ast.parse(SOURCE.read_text(encoding="utf-8"))
class Refused(Exception):
    pass


def refuse(msg):
    raise Refused(msg)


NS = {"re": re, "shutil": shutil, "tomllib": tomllib, "fail": refuse, "api": 3,
      "sunset": {"api": "0.2", "version": "1.2.1"}, "native": {"game": "26.3", "horizon": "0b14", "mixins": "e59d"}}
for node in TREE.body:
    if isinstance(node, ast.FunctionDef) and node.name in ("results", "judge", "wrap"):
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(SOURCE), "exec"), NS)
judge = NS["judge"]
wrap = NS["wrap"]
PLUGCHECK = Path(__file__).with_name("plugcheck")

TESTS = ["Plugin API: a", "Plugin API: b"]


def log(*lines, done=True):
    return list(lines) + (["", f"{sum(l.startswith('PASS') for l in lines)}/{len(lines)} passed"] if done else [])


class JudgeTests(unittest.TestCase):
    def test_all_passed_once(self):
        ok, retried, bad = judge(TESTS, log("PASS Plugin API: a - x", "PASS Plugin API: b - y"), [])
        self.assertEqual((ok, retried, bad), (TESTS, [], []))

    def test_missing_result_fails(self):
        ok, _, bad = judge(TESTS, log("PASS Plugin API: a - x"), [])
        self.assertEqual(ok, ["Plugin API: a"])
        self.assertEqual(bad, ["Plugin API: b - no result"])

    def test_failure_passed_on_rerun(self):
        ok, retried, bad = judge(TESTS, log("PASS Plugin API: a - x", "FAIL Plugin API: b - late"),
                                 log("PASS Plugin API: b - y"))
        self.assertEqual((ok, retried, bad), (TESTS, ["Plugin API: b"], []))

    def test_failure_failed_again_on_rerun(self):
        ok, _, bad = judge(TESTS, log("PASS Plugin API: a - x", "FAIL Plugin API: b - late"),
                           log("FAIL Plugin API: b - again"))
        self.assertEqual(ok, ["Plugin API: a"])
        self.assertEqual(bad, ["FAIL Plugin API: b - late"])

    def test_duplicate_or_fail_then_pass_is_not_hidden(self):
        for lines in (("PASS Plugin API: a - x", "PASS Plugin API: a - x"),
                      ("FAIL Plugin API: a - x", "PASS Plugin API: a - x")):
            with self.subTest(lines=lines):
                ok, _, bad = judge(TESTS[:1], log(*lines), [])
                self.assertEqual(ok, [])
                self.assertEqual(len(bad), 1)

    def test_skip_kick_and_early_stop_are_problems(self):
        _, _, bad = judge(TESTS, log("PASS Plugin API: a - x", "SKIP Plugin API: b - no CTL2"), [])
        self.assertIn("SKIP Plugin API: b - no CTL2", bad)
        _, _, bad = judge(TESTS[:1], log("PASS Plugin API: a - x", "KICKED during Plugin API: a: Disconnected"), [])
        self.assertTrue(any(l.startswith("KICKED") for l in bad))
        _, _, bad = judge(TESTS, log("PASS Plugin API: a - x", "Traceback", done=False), [])
        self.assertIn("verify.py stopped early: Traceback", bad)


class WrapTests(unittest.TestCase):
    def test_plugcheck_becomes_an_hmod_package_on_sunsets_pins(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "plugcheck"
            wrap(dest, PLUGCHECK)
            toml = tomllib.loads((dest / "horizon-mod.toml").read_text())
            self.assertEqual((toml["id"], toml["version"]), ("plugcheck", "0.1.0"))
            self.assertEqual({k: toml["native"][k] for k in ("game", "horizon", "mixins")}, NS["native"])
            self.assertEqual(toml["dependencies"], {"sunset": "^1.2"})
            self.assertEqual([m["name"] for m in toml["native"]["module"]], ["plugcheck_plugin"])
            self.assertEqual((dest / "src/game/plugin.rs").read_text(), (PLUGCHECK / "plugin.rs").read_text())
            self.assertIn("crate::game::plugcheck_plugin::tick(game)", (dest / "src/mixins/tick.rs").read_text())
            self.assertTrue(toml["native"]["mixin"][0]["target"].endswith("native_client_tick_post"))
            tomllib.loads((dest / "Cargo.toml").read_text())

    def fixture(self, tmp, plugin_rs, extra=""):
        src = Path(tmp) / "src"
        shutil.copytree(PLUGCHECK, src)
        (src / "plugin.rs").write_text(plugin_rs)
        (src / "horizon-mixin.toml").write_text((PLUGCHECK / "horizon-mixin.toml").read_text() + extra)
        return src

    def test_a_plugin_needing_a_newer_api_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = self.fixture(tmp, "const _: () = api::needs(4);\n")
            with self.assertRaisesRegex(Refused, "needs plugin API 4"):
                wrap(Path(tmp) / "out", src)
            ok = self.fixture(Path(tmp) / "b", "const _: () = api::needs(3);\n")
            wrap(Path(tmp) / "out-b", ok)

    def test_other_injections_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = self.fixture(tmp, "", '\n[[injection]]\nid = "more"\nkind = "HEAD"\ntarget = "x"\nhelper = "tick.rs"\n')
            with self.assertRaisesRegex(Refused, "one HEAD tick injection"):
                wrap(Path(tmp) / "out", src)


if __name__ == "__main__":
    unittest.main()
