"""Process-level failure controls for the conformance acceptance gate."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest


RUNNER = Path(__file__).with_name("run-conformance.py")


@unittest.skipUnless(os.name == "posix", "executable/signal controls require a POSIX host")
class ChildFailures(unittest.TestCase):
    def run_gate(
        self, phase, status, *, precompile=False, expected_error=False,
        formatted=False, diagnostic_before_signal=False,
    ):
        with tempfile.TemporaryDirectory(prefix="bifoil-runner-test-") as directory:
            root = Path(directory)
            stub = root / "child"
            lines = [
                f"#!{sys.executable}",
                "import json, os, signal, sys",
                "phase = 'execution' if sys.argv[1].endswith('.mjs') else sys.argv[1]",
            ]
            if expected_error and phase != "build":
                lines.extend([
                    "if phase == 'build':",
                    "    print('expected diagnostic', file=sys.stderr)",
                    "    sys.exit(1)",
                ])
            lines.append(f"if phase == {phase!r}:")
            if diagnostic_before_signal or (expected_error and phase == "build" and status >= 0):
                lines.append("    print('expected diagnostic', file=sys.stderr, flush=True)")
            if status < 0:
                lines.append(f"    os.kill(os.getpid(), {abs(status)})")
            else:
                lines.append(f"    sys.exit({status})")
            lines.extend([
                "if phase == 'execution':",
                "    print(json.dumps({'output': [], 'options': [], 'error': None}))",
            ])
            stub.write_text("\n".join(lines) + "\n")
            stub.chmod(0o700)
            case = {"id": "child-control", "source": "scene start():\n    Unit\n"}
            if precompile:
                case = {"id": "child-control", "files": {"main.bfl": "", "helper.bfl": ""},
                        "precompile": ["helper"]}
            if expected_error:
                case["compile_error"] = "expected diagnostic"
            index = root / "cases.json"
            index.write_text(json.dumps([case]))
            return subprocess.run(
                [sys.executable, str(RUNNER), "--compiler", str(stub),
                 "--node", str(stub), "--runtime", str(root / "runtime"),
                 "--cases", str(index), *(["--format-sources"] if formatted else [])], text=True, capture_output=True,
            )

    def test_silent_failures_and_signals_fail_every_phase(self):
        for phase in ("module", "build", "execution"):
            for status in (1, -signal.SIGTERM):
                with self.subTest(phase=phase, status=status):
                    result = self.run_gate(phase, status, precompile=phase == "module")
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("FAIL child-control", result.stdout)
                    self.assertIn("signal 15" if status < 0 else "exit 1", result.stdout)
                    self.assertNotIn("PASS ", result.stdout)
                    if phase == "module":
                        self.assertIn("module precompilation failed", result.stdout)

    def test_expected_errors_cannot_hide_failed_formatting(self):
        for status in (1, -signal.SIGTERM):
            with self.subTest(status=status):
                result = self.run_gate("format", status, expected_error=True, formatted=True)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("source formatting failed", result.stdout)

    def test_expected_diagnostic_cannot_hide_a_terminated_compiler(self):
        result = self.run_gate("build", -signal.SIGTERM, expected_error=True, diagnostic_before_signal=True)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("signal 15", result.stdout)

    def test_success_is_still_success(self):
        result = self.run_gate("unused", 1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS child-control", result.stdout)

    def test_expected_compile_failure_still_requires_its_diagnostic(self):
        result = self.run_gate("build", 1, expected_error=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        silent = self.run_gate("build", 1)
        self.assertNotEqual(silent.returncode, 0)


if __name__ == "__main__":
    unittest.main()
