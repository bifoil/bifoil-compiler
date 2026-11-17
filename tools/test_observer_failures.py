"""A benchmark must reject failed processes and invalid resource observations."""
from unittest.mock import patch

import process_measurement
from test_support import NativeCompilerTest

# Small inputs still use the actual native compiler. Only the observer result
# is changed, after compilation has produced its genuine output files.
BENCHMARKS = (
    ("benchmark.py", ["--workload", "functions"]),
    ("benchmark-cache-import.py", []),
    ("benchmark-code.py", ["--modules", "1", "--routines", "1", "--additions", "1"]),
    ("benchmark-cyclic.py", ["--functions", "3"]),
    ("benchmark-incremental.py", []),
    ("benchmark-modules.py", ["--scenes", "2"]),
    ("benchmark-narrative.py", ["--scenes", "2", "--lines", "3"]),
    ("benchmark-packages.py", ["--modules", "1", "--functions", "1"]),
    ("benchmark-precompiled.py", []),
    ("benchmark-text.py", ["--literal-bytes", "32"]),
    ("benchmark-typing.py", ["--workload", "fields"]),
    ("benchmark-capacity.py", ["--scenes", "1", "--rows", "1"]),
)

class ObserverFailures(NativeCompilerTest):
    def reject_observation(self, script, arguments, field, value):
        original = process_measurement.measure
        calls = []
        def observe(command):
            result = original(command)
            self.assertEqual(result["status"], 0, result["stderr"])
            result[field] = value
            calls.append(list(command))
            return result
        project = self.root / (script.removesuffix(".py") + "-" + field)
        with patch.object(process_measurement, "measure", side_effect=observe):
            try:
                self.run_tool(script, [self.compiler, project, *arguments], expected=None)
            except (RuntimeError, ValueError) as error:
                self.assertTrue(str(error))
        self.assertTrue(calls, "the benchmark must inspect the observer result")

    def test_failed_compilations_never_qualify_as_measurements(self):
        for script, arguments in BENCHMARKS:
            with self.subTest(script=script):
                self.reject_observation(script, arguments, "status", 7)

    def test_the_exact_resident_memory_limit_is_a_failure(self):
        for script, arguments in BENCHMARKS:
            with self.subTest(script=script):
                self.reject_observation(script, arguments, "compiler_peak_rss_bytes", 250_000_000)

    def test_incremental_deadlines_are_not_weakened_by_successful_processes(self):
        for script, arguments in BENCHMARKS:
            if script in {"benchmark.py", "benchmark-precompiled.py"}:
                continue
            with self.subTest(script=script):
                self.reject_observation(script, arguments, "wall_seconds", 10)
