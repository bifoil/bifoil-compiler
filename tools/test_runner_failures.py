"""Conformance diagnostics must fail closed and preserve original inputs."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from test_support import load_tool

runner = load_tool("run-conformance.py")


def completed(status=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(["tool"], status, stdout, stderr)


class RunnerFailureTests(unittest.TestCase):
    def test_formatter_failure_does_not_overwrite_source(self):
        for case, result in [({}, completed(1, "replacement", "syntax")),
                             ({"compile_error": "syntax"}, completed(1, "replacement", "")),
                             ({"compile_error": "syntax"}, completed(-9, "replacement", "syntax"))]:
            with self.subTest(case=case, status=result.returncode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "src").mkdir()
                source = root / "src/main.bfl"
                source.write_text("original")
                with patch.object(runner, "run_child", return_value=result):
                    self.assertIsNotNone(runner.format_sources(root, case, "compiler"))
                self.assertEqual(source.read_text(), "original")

    def test_expected_syntax_failures_continue_to_normal_compilation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            source = root / "src/main.bfl"
            source.write_text("original")
            with patch.object(runner, "run_child", return_value=completed(1, stderr="expected syntax")):
                self.assertIsNone(runner.format_sources(root, {"compile_error": "expected"}, "compiler"))
            self.assertEqual(source.read_text(), "original")
            with patch.object(runner, "run_child", side_effect=[completed(stdout="normalized"), completed(1, stderr="not stable")]):
                self.assertIn("idempotence", runner.format_sources(root, {}, "compiler"))
            self.assertEqual(source.read_text(), "normalized")

    def test_precompilation_keeps_all_sources_when_a_later_module_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            for name in ["first", "second"]:
                (root / "src" / (name + ".bfl")).write_text(name)
            with patch.object(runner, "run_child", side_effect=[completed(), completed(1, stderr="module error")]):
                message = runner.precompile_modules(root, {"precompile": ["first", "second"]}, "compiler")
            self.assertIn("module precompilation", message)
            self.assertTrue(all((root / "src" / (name + ".bfl")).is_file() for name in ["first", "second"]))
            for name in ["../outside", "a..b", "1invalid"]:
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, "dotted identifiers"):
                    runner.precompile_modules(root, {"precompile": [name]}, "compiler")

    def test_result_comparison_checks_errors_output_and_option_snapshots_independently(self):
        valid = {"error": None, "output": ["story"], "options": [["choose"]]}
        self.assertIsNone(runner.compare_result({"output": ["story"]}, valid))
        for case, actual, text in [
            ({"runtime_error": "expected"}, valid, "expected runtime failure"),
            ({"runtime_error": "expected"}, {**valid, "error": "other"}, "expected runtime failure"),
            ({}, {**valid, "error": "failure"}, "failure"),
            ({"output": ["different"]}, valid, "output"),
            ({"output": ["story"], "options": [["other"]]}, valid, "snapshots"),
        ]:
            with self.subTest(case=case):
                self.assertIn(text, runner.compare_result(case, actual))
        self.assertIsNone(runner.compare_result({"runtime_error": "expected", "output": ["story"]}, {**valid, "error": "expected failure"}))

    def test_compile_and_execution_failures_never_qualify_as_success(self):
        scenarios = [
            ({}, completed(-9, stderr="signal"), None, "signal 9"),
            ({}, completed(1, stderr="syntax"), None, "compilation failed"),
            ({"compile_error": "expected"}, completed(), None, "compilation succeeded"),
            ({"compile_error": "expected"}, completed(1, stderr="wrong"), None, "expected compile error"),
            ({"compile_error": "expected"}, completed(1, stderr="expected"), None, None),
            ({}, completed(), completed(2, stderr="runtime failure"), "execution failed"),
            ({}, completed(), completed(stdout=json.dumps({"error": None, "output": [], "options": []})), None),
        ]
        with tempfile.TemporaryDirectory() as directory:
            index = Path(directory) / "cases.json"
            for fields, compile_result, execute_result, expected in scenarios:
                results = [compile_result] + ([execute_result] if execute_result is not None else [])
                with self.subTest(fields=fields, expected=expected), patch.object(runner, "run_child", side_effect=results):
                    actual = runner.run_case({"id": "example", "source": "", **fields}, index,
                                             Path("compiler"), Path("runtime"), "node", False, Path("engine/scene.js"))
                if expected is None:
                    self.assertIsNone(actual)
                else:
                    self.assertIn(expected, actual)

    def test_preparation_errors_skip_compiler_and_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            index = Path(directory) / "cases.json"
            args = ({"id": "example", "source": ""}, index, Path("compiler"), Path("runtime"), "node", False, None)
            with patch.object(runner, "format_sources", return_value="format failure"), patch.object(runner, "run_child") as child:
                self.assertEqual(runner.run_case(*args, formatted=True), "format failure")
                child.assert_not_called()
            with patch.object(runner, "precompile_modules", return_value="module failure"), patch.object(runner, "run_child") as child:
                self.assertEqual(runner.run_case(*args), "module failure")
                child.assert_not_called()
            with self.assertRaisesRegex(ValueError, "case ids"):
                runner.run_case({"id": "INVALID"}, *args[1:])

    def test_directory_cases_include_legacy_sources_and_explicit_engine_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = root / "example"
            (case / "legacy").mkdir(parents=True)
            (case / "main.bfl").write_text("scene start():\n    Unit\n")
            (case / "legacy/questions.txt").write_text("*return\n")
            seen = []
            def child(phase, command, timeout):
                if phase == "compilation":
                    project = Path(command[2])
                    seen.append((project / "legacy/questions.txt").read_text())
                    seen.append(json.loads((project / "case.json").read_text())["native_engine"])
                    return completed()
                return completed(stdout=json.dumps({"error": None, "output": [], "options": []}))
            with patch.object(runner, "run_child", side_effect=child):
                self.assertIsNone(runner.run_case({"id": "legacy", "directory": "example"}, root / "cases.json",
                                                  Path("compiler"), Path("runtime"), "node", False, Path("engine/scene.js")))
            self.assertEqual(seen, ["*return\n", str(Path("engine/scene.js").resolve())])


if __name__ == "__main__":
    unittest.main()
