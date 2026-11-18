"""Corrupted observations after real native runs must never qualify comparisons."""

import io
import shutil
import sys
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

from test_support import NativeCompilerTest, load_tool


class TypingComparisonFailures(NativeCompilerTest):
    def comparison(self, name, *, observe=None, child=None, identity=None, rejection=False):
        tool = load_tool("compare-typing.py")
        project = self.root / name
        compiler = self.root / (name + "-compiler")
        shutil.copy2(self.compiler, compiler)
        arguments = [
            "compare-typing.py", "--before", str(self.compiler), "--after", str(compiler),
            "--output", str(project), "--control", "scoped-vals", "--workload", "parameter-vals",
        ]
        if rejection:
            arguments += ["--before-error", "original expected rejection"]
        real_measure, real_child, real_checksum = tool.measure, tool.subprocess.run, tool.checksum
        measurements = []

        def measured(command):
            result = real_measure(command)
            self.assertEqual(result["status"], 0, result["stderr"])
            measurements.append(command)
            if observe:
                observe(command, result)
            return result

        def subprocess_run(command, **options):
            result = real_child(command, **options)
            if child:
                child(command, result)
            return result

        identities = {}

        def checksum(path):
            digest = real_checksum(path)
            identities[path] = identities.get(path, 0) + 1
            if identity and identity(Path(path), identities[path], compiler):
                return "0" * 64
            return digest

        with (
            patch.object(sys, "argv", arguments), redirect_stdout(io.StringIO()),
            patch.object(tool, "measure", side_effect=measured),
            patch.object(tool.subprocess, "run", side_effect=subprocess_run),
            patch.object(tool, "checksum", side_effect=checksum),
        ):
            try:
                tool.main()
                failure = None
            except RuntimeError as error:
                failure = str(error)
        self.assertEqual(real_checksum(self.compiler), real_checksum(compiler))
        return project, self.read_report(project, "comparison.json"), failure, measurements

    def test_publication_failures_and_the_exact_memory_limit_keep_failed_reports(self):
        for field, value in [("status", 7), ("compiler_peak_rss_bytes", 250_000_000)]:
            def reject(_command, result, field=field, value=value):
                result[field] = value
            with self.subTest(field=field):
                project, report, failure, measured = self.comparison(field, observe=reject)
                self.assertIn("compiler operation failed", failure)
                self.assertFalse(report["successful"])
                self.assertEqual(report["module_publication"][field], value)
                self.assertTrue((project / "workload-after/src/main.bfl").is_file())
                self.assertEqual(len(measured), 1)

    def test_changed_control_bytes_are_rejected_after_successful_native_runs(self):
        def corrupt(command, result):
            self.assertEqual(result.returncode, 0)
            if "benchmark-typing.py" in command[2] and Path(command[4]).name == "control-after":
                original = Path(command[4]) / "original.startup.txt"
                original.write_bytes(original.read_bytes() + b"\n*comment observation fault\n")
        _, report, failure, measured = self.comparison("control", child=corrupt)
        self.assertIn("different control bytes", failure)
        self.assertFalse(report["exact_control_original_and_edited_bytes"])
        self.assertFalse(report["successful"])
        self.assertEqual(measured, [])

    def test_expected_old_compiler_rejections_require_status_diagnostic_and_no_output(self):
        for scenario in ["success", "memory", "diagnostic", "published", "accepted"]:
            def corrupt(command, result, scenario=scenario):
                if Path(command[2]).name != "workload-before":
                    return
                if scenario == "success":
                    return
                result["status"] = 1
                result["stderr"] = "wrong diagnostic" if scenario == "diagnostic" else "original expected rejection"
                if scenario == "memory":
                    result["compiler_peak_rss_bytes"] = 250_000_000
                if scenario == "accepted":
                    Path(command[command.index("--out") + 1]).unlink()
            with self.subTest(scenario=scenario):
                _, report, failure, measured = self.comparison(scenario, observe=corrupt, rejection=True)
                self.assertEqual(report["successful"], scenario == "accepted")
                if scenario == "accepted":
                    self.assertIsNone(failure)
                    self.assertEqual(len(measured), 4)
                    self.assertTrue(report["exact_source_free_edited_inline_bytes"])
                else:
                    self.assertIn("expected rejection", failure)
                    self.assertEqual(len(measured), 1)

    def test_source_free_output_and_owned_identity_failures_cannot_report_success(self):
        def corrupt_output(command, _result):
            output = Path(command[command.index("--out") + 1])
            if output.name == "source-free-inline.txt":
                output.write_bytes(output.read_bytes() + b"\n*comment observation fault\n")
        for label, options, field in [
            ("source-free", {"observe": corrupt_output}, "exact_source_free_edited_inline_bytes"),
            ("compiler-identity", {"identity": lambda path, count, compiler: path == compiler and count == 2}, "compilers_unchanged"),
            ("observer-identity", {"identity": lambda path, count, _compiler: path.name == "benchmark-typing.py" and count == 2}, "observers_unchanged"),
        ]:
            with self.subTest(label=label):
                _, report, failure, _ = self.comparison(label, **options)
                self.assertIn("identities changed", failure)
                self.assertFalse(report["successful"])
                self.assertFalse(report[field])

    def test_importing_benchmark_entrypoints_does_not_start_work_or_print_measurements(self):
        for name in ["benchmark-code.py", "benchmark-narrative.py", "compare-typing.py"]:
            output = io.StringIO()
            with self.subTest(name=name), redirect_stdout(output):
                tool = load_tool(name)
                self.assertTrue(callable(tool.main))
                self.assertEqual(output.getvalue(), "")
        self.assertEqual(list(self.root.iterdir()), [])
