"""Reject cache reports that hide unstable output, missing edits, or missing reuse evidence."""
from pathlib import Path
from unittest.mock import patch

import process_measurement
from test_support import NativeCompilerTest

BENCHMARKS = [
    ("benchmark-cyclic.py", ["--modules", "2", "--functions", "3"]),
    ("benchmark-text.py", ["--literal-bytes", "32"]),
    ("benchmark-cache-import.py", []),
]


class BenchmarkIntegrity(NativeCompilerTest):
    def test_independent_cache_and_edit_controls_reject_corrupted_observations(self):
        original = process_measurement.measure
        for script, arguments in BENCHMARKS:
            for fault in ["missing-summary", "unstable-unchanged", "missing-edit"]:
                with self.subTest(script=script, fault=fault):
                    phase = 0
                    previous = None
                    def measure(command, fault=fault):
                        nonlocal phase, previous
                        phase += 1
                        result = original(command)
                        self.assertEqual(result["status"], 0, result["stderr"])
                        output = Path(command[command.index("--out") + 1])
                        if fault == "missing-summary":
                            result["stdout"] = ""
                        elif fault == "unstable-unchanged" and phase in [2, 3]:
                            output.write_bytes(output.read_bytes() + b"\n*comment changed observation\n")
                        elif fault == "missing-edit":
                            if phase == 3:
                                previous = output.read_bytes()
                            elif phase == 4:
                                output.write_bytes(previous)
                        return result
                    directory = self.root / (script.removesuffix(".py") + "-" + fault)
                    with patch.object(process_measurement, "measure", side_effect=measure):
                        message = self.run_tool(script, [self.compiler, directory, *arguments], expected=None)
                    self.assertRegex(message, "cache result|module|different output|changed program bytes|did not change|changed the output")

    def test_discovery_observer_rejects_failed_compilation_and_the_exact_memory_limit(self):
        original = process_measurement.measure
        for field, value in [("status", 2), ("compiler_peak_rss_bytes", 250_000_000)]:
            def measure(command, field=field, value=value):
                result = original(command)
                self.assertEqual(result["status"], 0, result["stderr"])
                result[field] = value
                return result
            with self.subTest(field=field), patch.object(process_measurement, "measure", side_effect=measure):
                self.run_tool("benchmark-discovery.py", [self.compiler, self.root / field], expected=None)

    def test_code_benchmark_preserves_the_original_when_an_unchanged_build_changes_bytes(self):
        original = process_measurement.measure
        phase = 0

        def measure(command):
            nonlocal phase
            phase += 1
            result = original(command)
            self.assertEqual(result["status"], 0, result["stderr"])
            if phase == 2:
                output = Path(command[command.index("--out") + 1])
                output.write_bytes(output.read_bytes() + b"\n*comment original test boundary\n")
            return result

        directory = self.root / "unstable-code"
        with (
            patch.object(process_measurement, "measure", side_effect=measure),
            self.assertRaisesRegex(RuntimeError, "unchanged build altered"),
        ):
            self.run_tool("benchmark-code.py", [self.compiler, directory,
                "--modules", 1, "--routines", 2, "--additions", 1], expected=None)
        self.assertEqual(phase, 2)
        report = self.read_report(directory)
        self.assertNotEqual(report["runs"]["populate"]["root_sha256"],
                            report["runs"]["unchanged"]["root_sha256"])
        self.assertNotEqual((directory / "original/startup.txt").read_bytes(),
                            (directory / "startup.txt").read_bytes())
