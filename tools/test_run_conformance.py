"""Process-level failure controls for the conformance acceptance gate."""

import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RUNNER = Path(__file__).with_name("run-conformance.py")


@unittest.skipUnless(
    os.name == "posix", "executable/signal controls require a POSIX host"
)
class ChildFailures(unittest.TestCase):
    def run_gate(
        self,
        phase,
        status,
        *,
        precompile=False,
        expected_error=False,
        formatted=False,
        diagnostic_before_signal=False,
        hang=False,
        phase_timeout=None,
    ):
        with tempfile.TemporaryDirectory(prefix="bifoil-runner-test-") as directory:
            root = Path(directory)
            stub = root / "child"
            lines = [
                f"#!{sys.executable}",
                "import json, os, signal, sys, time",
                "phase = 'execution' if sys.argv[1].endswith('.mjs') else sys.argv[1]",
                "if phase == 'format' and '--check' in sys.argv: phase = 'format_check'",
            ]
            if expected_error and phase != "build":
                lines.extend(
                    [
                        "if phase == 'build':",
                        "    print('expected diagnostic', file=sys.stderr)",
                        "    sys.exit(1)",
                    ]
                )
            lines.append(f"if phase == {phase!r}:")
            if diagnostic_before_signal or (
                expected_error and phase == "build" and status >= 0
            ):
                lines.append(
                    "    print('expected diagnostic', file=sys.stderr, flush=True)"
                )
            if hang:
                lines.append(
                    f"    open({str(root / 'child.pid')!r}, 'w').write(str(os.getpid()))"
                )
                lines.append("    time.sleep(30)")
            elif status < 0:
                lines.append(f"    os.kill(os.getpid(), {abs(status)})")
            else:
                lines.append(f"    sys.exit({status})")
            lines.extend(
                [
                    "if phase == 'execution':",
                    "    print(json.dumps({'output': [], 'options': [], 'error': None}))",
                ]
            )
            if hang:
                # A native sleeper keeps Python coverage startup outside the
                # half-second timeout under test. exec preserves the direct
                # child's PID, so the existing reap assertion remains meaningful.
                shell = shutil.which("sh")
                sleeper = shutil.which("sleep")
                self.assertIsNotNone(shell)
                self.assertIsNotNone(sleeper)
                lines = [
                    "#!" + shell,
                    'phase="$1"',
                    'case "$phase" in *.mjs) phase=execution ;; esac',
                    'if [ "$phase" = format ]; then',
                    '    for argument do',
                    '        if [ "$argument" = --check ]; then phase=format_check; fi',
                    '    done',
                    'fi',
                    'if [ "$phase" = ' + shlex.quote(phase) + ' ]; then',
                ]
                if expected_error:
                    lines.append("    printf '%s\\n' 'expected diagnostic' >&2")
                lines.extend([
                    "    printf '%s' \"$$\" > " + shlex.quote(str(root / "child.pid")),
                    "    exec " + shlex.quote(sleeper) + " 30",
                    "fi",
                    'if [ "$phase" = execution ]; then',
                    "    printf '%s\\n' '{\"output\":[],\"options\":[],\"error\":null}'",
                    "fi",
                ])
            stub.write_text("\n".join(lines) + "\n")
            stub.chmod(0o700)
            case = {"id": "child-control", "source": "scene start():\n    Unit\n"}
            if precompile:
                case = {
                    "id": "child-control",
                    "files": {"main.bfl": "", "helper.bfl": ""},
                    "precompile": ["helper"],
                }
            if expected_error:
                case["compile_error"] = "expected diagnostic"
            index = root / "cases.json"
            index.write_text(json.dumps([case]))
            result = subprocess.run(
                [
                    sys.executable,
                    str(RUNNER),
                    "--compiler",
                    str(stub),
                    "--node",
                    str(stub),
                    "--runtime",
                    str(root / "runtime"),
                    "--cases",
                    str(index),
                    *(["--format-sources"] if formatted else []),
                    *(
                        ["--phase-timeout", str(phase_timeout)]
                        if phase_timeout is not None
                        else []
                    ),
                ],
                text=True,
                capture_output=True,
                timeout=15,
            )
            if hang:
                child = int((root / "child.pid").read_text())
                with self.assertRaises(ProcessLookupError):
                    os.kill(child, 0)
            return result

    def test_silent_failures_and_signals_fail_every_phase(self):
        for phase in ("format", "format_check", "module", "build", "execution"):
            for status in (1, -signal.SIGTERM):
                with self.subTest(phase=phase, status=status):
                    result = self.run_gate(
                        phase,
                        status,
                        precompile=phase == "module",
                        formatted=phase.startswith("format"),
                    )
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("FAIL child-control", result.stdout)
                    self.assertIn(
                        "signal 15" if status < 0 else "exit 1", result.stdout
                    )
                    self.assertNotIn("PASS ", result.stdout)
                    if phase == "module":
                        self.assertIn("module precompilation failed", result.stdout)

    def test_expected_errors_cannot_hide_failed_formatting(self):
        for status in (1, -signal.SIGTERM):
            with self.subTest(status=status):
                result = self.run_gate(
                    "format", status, expected_error=True, formatted=True
                )
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("source formatting failed", result.stdout)

    def test_expected_diagnostic_cannot_hide_a_terminated_compiler(self):
        result = self.run_gate(
            "build", -signal.SIGTERM, expected_error=True, diagnostic_before_signal=True
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("signal 15", result.stdout)

    def test_deadlines_fail_every_phase_and_reap_the_direct_child(self):
        for phase in ("format", "format_check", "module", "build", "execution"):
            with self.subTest(phase=phase):
                result = self.run_gate(
                    phase,
                    0,
                    hang=True,
                    phase_timeout=0.5,
                    formatted=phase.startswith("format"),
                    precompile=phase == "module",
                )
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("FAIL child-control", result.stdout)
                self.assertIn("timed out after 0.5 seconds", result.stdout)
                self.assertNotIn("PASS ", result.stdout)

    def test_expected_diagnostic_cannot_hide_a_timeout(self):
        result = self.run_gate(
            "build", 0, hang=True, phase_timeout=0.5, expected_error=True
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("compilation timed out", result.stdout)
        self.assertNotIn("PASS ", result.stdout)

    def test_invalid_deadlines_fail_before_starting_children(self):
        for value in ("nan", "inf", "0", "-1", "words"):
            with self.subTest(value=value):
                result = self.run_gate("unused", 0, phase_timeout=value)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("timeout must be a positive finite number", result.stderr)
                self.assertNotIn("PASS ", result.stdout)

    def test_success_is_still_success(self):
        result = self.run_gate("unused", 1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS child-control", result.stdout)

    def test_expected_compile_failure_still_requires_its_diagnostic(self):
        result = self.run_gate("build", 1, expected_error=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        silent = self.run_gate("build", 1)
        self.assertNotEqual(silent.returncode, 0)


class EmptySelections(unittest.TestCase):
    def run_index(self, cases, extra=()):
        with tempfile.TemporaryDirectory(prefix="bifoil-runner-index-") as directory:
            root = Path(directory)
            index = root / "cases.json"
            index.write_text(json.dumps(cases))
            # Neither executable can process Bifoil commands. Configuration must
            # reject these inputs before any child is invoked.
            return subprocess.run(
                [
                    sys.executable,
                    str(RUNNER),
                    "--compiler",
                    sys.executable,
                    "--node",
                    sys.executable,
                    "--runtime",
                    str(root),
                    "--cases",
                    str(index),
                    *extra,
                ],
                text=True,
                capture_output=True,
                timeout=10,
            )

    def test_empty_index_cannot_report_a_passing_suite(self):
        result = self.run_index([])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no conformance cases selected", result.stderr)
        self.assertNotIn("cases passed", result.stdout)

    def test_unknown_selection_and_duplicate_ids_fail_before_execution(self):
        case = {"id": "one", "source": "scene start():\n    Unit\n"}
        for cases, extra, message in [
            ([case], ["--only", "missing"], "unknown conformance case IDs"),
            ([case, case], [], "duplicate conformance case ID"),
        ]:
            with self.subTest(message=message):
                result = self.run_index(cases, extra)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertNotIn("PASS ", result.stdout)


if __name__ == "__main__":
    unittest.main()
