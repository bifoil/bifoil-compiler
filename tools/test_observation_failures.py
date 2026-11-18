"""Real benchmark runs must reject corrupted reuse, publication, and output observations."""

import shutil
from pathlib import Path
from unittest.mock import patch

import process_measurement
from test_support import NativeCompilerTest


def append_output(command, _result):
    output = Path(command[command.index("--out") + 1])
    output.write_bytes(output.read_bytes() + b"\n*comment original observation fault\n")


class ObservationFailureTests(NativeCompilerTest):
    def corrupt_phase(self, script, arguments, phase, corrupt, expected):
        """Change one observation after its real compiler process finishes successfully."""
        original = process_measurement.measure
        calls = 0
        project = self.root / (script.removesuffix(".py") + "-" + str(phase))

        def observe(command):
            nonlocal calls
            calls += 1
            result = original(command)
            self.assertEqual(result["status"], 0, result["stderr"])
            if calls == phase:
                corrupt(command, result)
            return result

        with patch.object(process_measurement, "measure", side_effect=observe):
            try:
                message = self.run_tool(script, [self.compiler, project, *arguments], expected=None)
            except RuntimeError as error:
                message = str(error)
        self.assertGreaterEqual(calls, phase)
        self.assertRegex(message, expected)
        return project

    def test_typing_reports_reject_unstable_reuse_and_an_unobserved_edit(self):
        original = process_measurement.measure
        for phase in [2, 3]:
            first_output = None
            calls = 0

            def observe(command, phase=phase):
                nonlocal calls, first_output
                calls += 1
                result = original(command)
                self.assertEqual(result["status"], 0, result["stderr"])
                output = Path(command[command.index("--out") + 1])
                if calls == 1:
                    first_output = output.read_bytes()
                elif calls == phase:
                    output.write_bytes(first_output if phase == 3 else output.read_bytes() + b"\n")
                return result

            project = self.root / ("typing-" + str(phase))
            with self.subTest(phase=phase), patch.object(process_measurement, "measure", side_effect=observe):
                message = self.run_tool("benchmark-typing.py", [self.compiler, project, "--workload", "fields"], expected=None)
            self.assertIn("did not reflect validated source changes", message)
            report = self.read_report(project)
            self.assertFalse(report["unchanged_output"] if phase == 2 else report["edited_output_changed"])

    def test_missing_reuse_summaries_do_not_qualify_incremental_measurements(self):
        def remove_summary(_command, result):
            result["stdout"] = ""

        for script, phase, message in [
            ("benchmark-incremental.py", 3, "reuse every module"),
            ("benchmark-incremental.py", 4, "exactly the leaf and its client"),
            ("benchmark-cache-import.py", 4, "expected dependency invalidation"),
        ]:
            with self.subTest(script=script, phase=phase):
                self.corrupt_phase(script, [], phase, remove_summary, message)

    def test_precompiled_publication_failures_retain_the_original_source(self):
        def reject_publication(_command, result):
            result["status"] = 1
            result["stderr"] = "original publication rejection"

        project = self.corrupt_phase(
            "benchmark-precompiled.py", [], 2, reject_publication, "original publication rejection"
        )
        self.assertTrue((project / "src/main.bfl").is_file())
        self.assertTrue((project / "startup.txt").is_file())

    def test_source_free_publication_must_preserve_exact_root_bytes(self):
        project = self.corrupt_phase("benchmark-precompiled.py", [], 3, append_output, "linked artifact bytes")
        self.assertFalse(self.read_report(project)["identical_output"])

    def test_baseline_comparisons_refuse_changed_output_even_when_both_compilers_succeed(self):
        for script in ["benchmark.py", "benchmark-discovery.py"]:
            with self.subTest(script=script):
                self.corrupt_phase(script, ["--baseline", self.compiler], 2, append_output, "different program bytes")

    def test_failed_clean_builds_keep_a_report_without_invented_program_metadata(self):
        compiler = Path(shutil.which("false"))
        project = self.root / "no-output"
        self.run_tool("benchmark.py", [compiler, project], expected=1)
        report = self.read_report(project)
        self.assertNotEqual(report["status"], 0)
        self.assertIsNone(report["profile"])
        self.assertNotIn("functions", report)
        self.assertFalse((project / "startup.txt").exists())

    def test_existing_module_and_narrative_directories_are_preserved(self):
        for script in ["benchmark-modules.py", "benchmark-narrative.py"]:
            project = self.root / script
            project.mkdir()
            retained = project / "retained.txt"
            retained.write_text("original operator-owned file")
            with self.subTest(script=script):
                message = self.run_tool(script, [self.compiler, project], expected=2)
                self.assertIn("must be empty", message)
                self.assertEqual(retained.read_text(), "original operator-owned file")
                self.assertFalse((project / "measurement.json").exists())
