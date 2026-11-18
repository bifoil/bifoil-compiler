"""Real benchmark runs must reject corrupted reuse, publication, and output observations."""

import shutil
import tempfile
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
        project = Path(tempfile.mkdtemp(prefix=script.removesuffix(".py") + "-", dir=self.root)) / "project"

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

    def test_dependency_edits_cannot_change_the_consumer_source(self):
        def corrupt_consumer(command, _result):
            source = Path(command[2]) / "src/main.bfl"
            source.write_text(source.read_text() + "\n")
        project = self.corrupt_phase(
            "benchmark-cache-import.py", ["--workload", "nominal-consumer"], 4,
            corrupt_consumer, "changed its consumer source",
        )
        report = self.read_report(project)
        self.assertNotEqual(report["runs"]["populate"]["source_sha256"]["main.bfl"],
                            report["runs"]["dependency_edit"]["source_sha256"]["main.bfl"])

    def test_code_measurements_reject_a_changed_owned_compiler_copy(self):
        compiler = self.root / "owned-compiler"
        shutil.copy2(self.compiler, compiler)
        original = process_measurement.measure
        calls = 0

        def measured(command):
            nonlocal calls
            result = original(command)
            self.assertEqual(result["status"], 0, result["stderr"])
            calls += 1
            if calls == 8:
                with compiler.open("ab") as executable:
                    executable.write(b"owned test identity change")
            return result

        project = self.root / "compiler-identity"
        shared_identity = self.compiler.read_bytes()
        with (
            patch.object(process_measurement, "measure", side_effect=measured),
            self.assertRaisesRegex(RuntimeError, "compiler changed during measurement"),
        ):
            self.run_tool("benchmark-code.py", [compiler, project, "--modules", 1, "--routines", 2, "--additions", 1])
        self.assertFalse(self.read_report(project)["compiler_unchanged"])
        self.assertEqual(self.compiler.read_bytes(), shared_identity)

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


    def test_module_publication_rejections_preserve_the_unpublished_source(self):
        for script, flags in [
            ("benchmark-modules.py", ["--scenes", 1]),
            ("benchmark-narrative.py", ["--scenes", 1, "--lines", 1]),
        ]:
            for field, value in [("status", 1), ("compiler_peak_rss_bytes", 250_000_000)]:
                def reject(_command, result, field=field, value=value):
                    result[field] = value
                    result["stderr"] = "original publication fault"
                with self.subTest(script=script, field=field):
                    project = self.corrupt_phase(script, flags, 4, reject, "stats: original publication fault")
                    self.assertTrue((project / "src/stats.bfl").exists())
                    self.assertIn("stats", self.read_report(project)["publication"])

    def test_package_cache_and_unchanged_deadlines_reject_the_exact_limit(self):
        def exceed_deadline(_command, result):
            result["wall_seconds"] = 10
        for phase, message in [(9, "package cache population exceeded"), (10, "unchanged package exceeded")]:
            with self.subTest(phase=phase):
                project = self.corrupt_phase("benchmark-modules.py", ["--scenes", 1], phase, exceed_deadline, message)
                self.assertTrue((project / "measurement.json").exists())

    def test_code_measurements_reject_ambiguous_edits_and_changed_source_free_bytes(self):
        flags = ["--modules", 1, "--routines", 2, "--additions", 1]
        def ambiguous_source(command, _result):
            project = Path(command[2])
            source = project / "src/code/m00.bfl"
            source.write_text(source.read_text().replace("= value + 1", "= value + 7"))
        def missed_edit(command, _result):
            output = Path(command[command.index("--out") + 1])
            output.write_bytes((output.parent / "original/startup.txt").read_bytes())
        for phase, corrupt, message in [
            (2, ambiguous_source, "one editable routine"),
            (3, missed_edit, "routine edit did not change"),
            (7, append_output, "source-free linking changed"),
        ]:
            with self.subTest(phase=phase):
                self.corrupt_phase("benchmark-code.py", flags, phase, corrupt, message)
