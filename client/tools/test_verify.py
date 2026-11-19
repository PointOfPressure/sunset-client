"""Check verifier reporting without starting a game or importing its test suite."""
import ast
import contextlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import traceback
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


SOURCE = Path(__file__).with_name("verify.py")
TREE = ast.parse(SOURCE.read_text(encoding="utf-8"))


def extracted(node):
    return compile(ast.Module(body=[node], type_ignores=[]), str(SOURCE), "exec")


class VerifyTests(unittest.TestCase):
    def load_function(self, name, **namespace):
        node = next(n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name == name)
        exec(extracted(node), namespace)
        return namespace[name]

    def api_matches(self, text, expected=None):
        env = {} if expected is None else {"SUNSET_PLUGIN_API_VERSION": expected}
        return self.load_function("plugin_api_matches", re=re, os=SimpleNamespace(environ=env))(text)

    def test_ctl_supports_explicit_socket_and_legacy_wrapper(self):
        for socket in (None, "D:/isolated run/c.sock"):
            with self.subTest(socket=socket):
                env = {} if socket is None else {"SUNSET_VERIFY_SOCKET": socket}
                run = Mock(return_value=SimpleNamespace(stdout='{"ok": true}'))
                control = self.load_function("ctl", CTL="D:/bin/horizon-ctl.exe", json=json,
                                             os=SimpleNamespace(environ=env), subprocess=SimpleNamespace(run=run))
                self.assertEqual(control("state", timeout=42), {"ok": True})
                expected = ["D:/bin/horizon-ctl.exe"]
                if socket:
                    expected += ["--socket", socket]
                run.assert_called_once_with([*expected, "state"], capture_output=True, text=True, timeout=42)

    def test_session_supports_explicit_path_and_legacy_pointer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "session").write_text("D:/legacy session\n", encoding="utf-8")
            for session in (None, "D:/explicit session"):
                with self.subTest(session=session):
                    env = {} if session is None else {"SUNSET_VERIFY_SESSION": session}
                    session_dir = self.load_function("session_dir", CTL=str(root / "ctl.exe"),
                                                     os=SimpleNamespace(environ=env, path=os.path))
                    self.assertEqual(session_dir(), session or "D:/legacy session")

    def test_default_accepts_compatible_host_headers(self):
        for version in (2, 3, 10):
            with self.subTest(version=version):
                self.assertTrue(self.api_matches(f"Plugins (4, plugin API {version}):"))
        self.assertFalse(self.api_matches("Plugins (4, plugin API 1):"))
        self.assertFalse(self.api_matches("plugin API 3 in a plugin description"))

    def test_release_requires_exact_api_version(self):
        self.assertTrue(self.api_matches("Plugins (4, plugin API 2):", "2"))
        self.assertTrue(self.api_matches("Plugins (4, plugin API 3):", "3"))
        self.assertFalse(self.api_matches("Plugins (4, plugin API 3):", "2"))
        self.assertFalse(self.api_matches("Plugins (4, plugin API 20):", "2"))
        self.assertFalse(self.api_matches("Plugins (4, plugin API 2):", "3"))
        self.assertFalse(self.api_matches("No plugins", "3"))
        self.assertTrue(self.api_matches("x | Plugins (3, plugin API 3): | hello 0.2.0", ""))

    def run_verifier(self, outcomes, selected=(), extra=()):
        calls = []
        tests = []
        for name, outcome in outcomes:
            def check(value=outcome):
                return value, "fixture"
            check.test_name = name
            tests.append(check)
        tests += extra
        namespace = {
            "__name__": "__main__", "sys": sys, "traceback": traceback, "ONLY": selected, "TESTS": tests,
            "results": [], "kicks": [], "arena": lambda: calls.append("arena"),
            "rejoin": lambda name: None, "clean": lambda: None, "light_clean": lambda: None, "overran": lambda *_: None,
        }
        main = next(n for n in TREE.body if isinstance(n, ast.If) and ast.unparse(n.test) == "__name__ == '__main__'")
        output = io.StringIO()
        with patch.dict(sys.modules, {"signal": SimpleNamespace()}), contextlib.redirect_stdout(output), \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as stopped:
                exec(extracted(main), namespace)
        return stopped.exception.code, output.getvalue(), calls

    def test_failed_check_exits_nonzero(self):
        code, output, _ = self.run_verifier([("pass", True), ("fail", False)])
        self.assertEqual(code, 1)
        self.assertIn("FAIL fail - fixture", output)

    def test_unknown_selector_fails_before_touching_game(self):
        code, _, calls = self.run_verifier([("known", True)], ["typo"])
        self.assertIn("Unknown test name(s): typo", code)
        self.assertEqual(calls, [])

    def test_full_run_can_skip_with_passing_results(self):
        code, output, _ = self.run_verifier([("pass", True), ("optional", None)])
        self.assertEqual(code, 0)
        self.assertIn("SKIP optional - fixture", output)
        self.assertIn("1/1 passed", output)

    def test_all_passing_selected_run_exits_zero(self):
        code, output, calls = self.run_verifier([("a", True), ("b", True), ("c", True)], ["a", "c"])
        self.assertEqual(code, 0)
        self.assertIn("2/2 passed", output)
        self.assertNotIn("PASS b", output)
        self.assertEqual(calls, ["arena"])

    def test_error_in_check_exits_nonzero(self):
        def boom():
            raise RuntimeError("fixture")
        boom.test_name = "boom"
        code, output, _ = self.run_verifier([("pass", True)], extra=[boom])
        self.assertEqual(code, 1)
        self.assertIn("FAIL boom - error", output)

    def test_no_executed_checks_exits_nonzero(self):
        for outcomes in ([], [("optional", None)]):
            with self.subTest(outcomes=outcomes):
                code, output, _ = self.run_verifier(outcomes)
                self.assertEqual(code, 1)
                self.assertIn("0/0 passed", output)


if __name__ == "__main__":
    unittest.main()