"""Reject malformed acceptance inputs before starting compiler/runtime processes."""
import argparse
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_support import load_tool

runner = load_tool("run-conformance.py")

class CaseValidation(unittest.TestCase):
    def test_valid_source_forms_and_expectations(self):
        for form in ({"source": ""}, {"files": {"main.bfl": ""}}, {"directory": "cases/example"}):
            case = {"id": "example-1", **form, "decisions": ["A01"], "choices": [0, "input"],
                    "output": ["text"], "options": [["choice"]], "precompile": [],
                    "legacy_stats": {}, "quantum": 1}
            self.assertEqual(runner.validate_case(case), "example-1")
        for field in ("compile_error", "runtime_error"):
            self.assertEqual(runner.validate_case({"id": "example", "source": "", field: "expected"}), "example")

    def test_invalid_structure_identifiers_and_ambiguous_forms(self):
        invalid = [None, [], {}, {"id": "Upper", "source": ""},
                   {"id": "ok"}, {"id": "ok", "source": "", "files": {}},
                   {"id": "ok", "source": "", "unknown": True},
                   {"id": "ok", "source": "", "compile_error": "a", "runtime_error": "b"}]
        for case in invalid:
            with self.subTest(case=case), self.assertRaises(ValueError):
                runner.validate_case(case)

    def test_field_shapes_and_array_elements(self):
        base = {"id": "ok", "source": ""}
        invalid = [(key, 4) for key in ("source", "compile_error", "runtime_error")]
        invalid += [(key, "not an array") for key in ("decisions", "choices", "output", "options", "precompile")]
        invalid += [(key, [4]) for key in ("decisions", "output", "precompile")]
        invalid += [("options", ["not a group"]), ("options", [[4]]), ("files", []),
                    ("files", {4: "source"}), ("files", {"main.bfl": 4}), ("legacy_stats", []),
                    ("compile_error", ""), ("runtime_error", ""), ("quantum", 0),
                    ("quantum", -1), ("quantum", True), ("quantum", "1")]
        for key, value in invalid:
            case = dict(base)
            if key == "files":
                case.pop("source")
            case[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                runner.validate_case(case)
        with self.assertRaises(ValueError):
            runner.validate_case({"id": "ok", "directory": 4})

    def test_deadline_parser_rejects_nonfinite_or_nonpositive_values(self):
        self.assertEqual(runner.positive_timeout("2.5"), 2.5)
        for value in ("bad", "nan", "inf", "-inf", "0", "-0.5"):
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                runner.positive_timeout(value)

    def test_source_inventory_is_contained_and_retains_relative_names(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "cases/nested"
            source.mkdir(parents=True)
            (source / "main.bfl").write_text("main")
            (source / "child").mkdir()
            (source / "child/dependency.bfl").write_text("dependency")
            (source / "ignored.txt").write_text("not Bifoil")
            index = root / "cases.json"
            self.assertEqual(runner.source_files({"directory": "cases/nested"}, index),
                             {"child/dependency.bfl": "dependency", "main.bfl": "main"})
            with self.assertRaisesRegex(ValueError, "stay under"):
                runner.source_files({"directory": "../escape"}, index)
            self.assertEqual(runner.source_files({"files": {"a.bfl": "a"}}, index), {"a.bfl": "a"})
            self.assertEqual(runner.source_files({"source": "inline"}, index), {"main.bfl": "inline"})
            self.assertEqual(runner.source_files({}, index), {"main.bfl": ""})

    def test_project_writer_never_escapes_its_source_or_legacy_root(self):
        for legacy in (False, True):
            for name in ("../outside.bfl", "/outside.bfl"):
                with self.subTest(legacy=legacy, name=name), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary) / "project"
                    root.mkdir()
                    files = {name: "escape"}
                    with self.assertRaisesRegex(ValueError, "relative"):
                        runner.write_project(root, {}, {} if legacy else files, files if legacy else {})
                    self.assertFalse((Path(temporary) / "outside.bfl").exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runner.write_project(root, {"id": "ok"}, {"nested/main.bfl": "source"}, {"nested/scene.txt": "legacy"})
            self.assertEqual((root / "src/nested/main.bfl").read_text(), "source")
            self.assertEqual((root / "legacy/nested/scene.txt").read_text(), "legacy")
            self.assertIn('legacy_path = "legacy"', (root / "bifoil.toml").read_text())

    def test_explicit_windows_translation_and_native_fallback(self):
        runner.native_path.cache_clear()
        with patch.object(runner.subprocess, "check_output", return_value="C:\\project\\source\n") as convert:
            self.assertEqual(runner.native_path("/mnt/c/project/source", True), "C:\\project\\source")
            convert.assert_called_once()
        with patch.object(runner.subprocess, "check_output", side_effect=AssertionError("unexpected translation")):
            self.assertEqual(runner.native_path("/tmp/project", True), "/tmp/project")
            self.assertEqual(runner.native_path("/tmp/project", False), "/tmp/project")
        runner.native_path.cache_clear()

    def test_failed_process_diagnostics_keep_signal_status_and_output(self):
        for status, stderr, stdout, expected in [(1, "", "", "no output"),
                                                (-15, "details", "", "signal 15"),
                                                (2, "", "output", "output")]:
            result = subprocess.CompletedProcess(["compiler"], status, stdout, stderr)
            self.assertIn(expected, runner.child_failure("compilation", result))

if __name__ == "__main__":
    unittest.main()
