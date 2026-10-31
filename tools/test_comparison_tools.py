"""Compare real compiler outputs and prove that changed artifacts are rejected."""

from contextlib import redirect_stdout
import io
import shutil
import sys
from unittest.mock import patch

from test_support import NativeCompilerTest, load_tool


class ComparisonTools(NativeCompilerTest):
    def compare_code(self, after, destination):
        tool = load_tool("compare-code.py")
        workload = {"small": ["--modules", "2", "--routines", "3", "--additions", "2"]}
        arguments = ["compare-code.py", "--before", str(self.compiler), "--after", str(after),
                     "--output", str(destination), "--workloads", "small"]
        with patch.dict(tool.WORKLOADS, workload, clear=True), patch.object(sys, "argv", arguments):
            with redirect_stdout(io.StringIO()):
                status = tool.main()
        return status, self.read_report(destination, "comparison.json")

    def test_code_comparison_checks_artifacts_and_retains_child_failures(self):
        status, report = self.compare_code(self.compiler, self.root / "identical")
        self.assertEqual(status, 0)
        self.assertTrue(report["successful"])
        self.assertTrue(report["workloads"]["small"]["exact_artifact_bytes"])
        self.assertGreater(report["workloads"]["small"]["compared_artifacts"], 0)
        status, failed = self.compare_code(shutil.which("false"), self.root / "failed")
        self.assertEqual(status, 1)
        self.assertFalse(failed["successful"])
        self.assertNotEqual(failed["workloads"]["small"]["after"]["observer_status"], 0)

    def test_code_comparison_rejects_semantically_equivalent_but_different_output_bytes(self):
        wrapper = self.root / "annotating-compiler"
        wrapper.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, subprocess, sys\n"
            f"result = subprocess.run([{str(self.compiler)!r}, *sys.argv[1:]])\n"
            "if result.returncode == 0 and sys.argv[1] == 'build':\n"
            "    output = pathlib.Path(sys.argv[sys.argv.index('--out') + 1])\n"
            "    text = output.read_text()\n"
            "    output.write_text('*comment changed by test adapter\\n' + text)\n"
            "sys.exit(result.returncode)\n"
        )
        wrapper.chmod(0o700)
        status, report = self.compare_code(wrapper, self.root / "different")
        self.assertEqual(status, 1)
        self.assertFalse(report["workloads"]["small"]["exact_artifact_bytes"])
        self.assertFalse(report["successful"])

    def test_typing_comparison_preserves_control_and_source_free_outputs(self):
        project = self.root / "typing"
        self.run_tool("compare-typing.py", ["--before", self.compiler, "--after", self.compiler,
                                           "--output", project, "--control", "scoped-vals",
                                           "--workload", "parameter-vals"])
        report = self.read_report(project, "comparison.json")
        self.assertTrue(report["successful"])
        self.assertTrue(report["exact_control_original_and_edited_bytes"])
        self.assertTrue(report["exact_source_free_edited_inline_bytes"])
        self.assertEqual(set(report["source_free_links"]), {"inline", "package"})
        for result in report["source_free_links"].values():
            self.assertEqual(result["status"], 0)

    def test_typing_comparison_retains_failed_control_measurements(self):
        project = self.root / "failed-typing"
        self.run_tool("compare-typing.py", ["--before", shutil.which("false"), "--after", self.compiler,
                                           "--output", project, "--control", "scoped-vals",
                                           "--workload", "parameter-vals"], expected=None)
        report = self.read_report(project, "comparison.json")
        self.assertFalse(report["successful"])
        self.assertNotEqual(report["workloads"]["control-before"]["runs"]["populate"]["status"], 0)

    def test_comparison_arguments_reject_missing_compilers_and_reused_outputs(self):
        common = ["--before", self.compiler, "--after", self.compiler, "--output", self.root / "unused"]
        self.run_tool("compare-code.py", [*common, "--workloads", "control", "control"], expected=None)
        for name, extra in [("compare-code.py", []),
                            ("compare-typing.py", ["--control", "scoped-vals", "--workload", "parameter-vals"])]:
            with self.subTest(name=name):
                self.run_tool(name, ["--before", self.root / "missing", "--after", self.compiler,
                                     "--output", self.root / "unused", *extra], expected=None)
                self.run_tool(name, ["--before", self.compiler, "--after", self.compiler,
                                     "--output", self.root, *extra], expected=None)

    def test_streamed_comparison_detects_a_late_difference_and_inventory_ignores_noise(self):
        tool = load_tool("compare-code.py")
        left, right = self.root / "left", self.root / "right"
        payload = b"x" * (tool.BUFFER_BYTES + 17)
        left.write_bytes(payload)
        right.write_bytes(payload)
        self.assertTrue(tool.equal_bytes(left, right))
        self.assertEqual(tool.checksum(left), tool.checksum(right))
        right.write_bytes(payload[:-1] + b"y")
        self.assertFalse(tool.equal_bytes(left, right))
        (self.root / "startup.txt").write_text("root")
        (self.root / "noise.json").write_text("not an artifact")
        self.assertEqual({path.as_posix() for path in tool.artifact_paths(self.root)}, {"startup.txt"})
