"""Shared fixtures for compiler-tool integration tests."""

from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parent


def load_tool(name):
    spec = importlib.util.spec_from_file_location("bifoil_test_" + name.replace("-", "_"), TOOLS / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class NativeCompilerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        selected = os.environ.get("BIFOIL_TEST_COMPILER")
        if not selected:
            raise unittest.SkipTest("set BIFOIL_TEST_COMPILER to a built native compiler")
        cls.compiler = Path(selected).resolve()
        if not cls.compiler.is_file():
            raise ValueError("BIFOIL_TEST_COMPILER must name an existing executable")
        if not sys.platform.startswith("linux") or shutil.which("time") is None:
            raise unittest.SkipTest("resource observers require Linux and GNU Time")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bifoil-tools-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def run_tool(self, name, arguments, expected=0):
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(sys, "argv", [str(TOOLS / name), *map(str, arguments)]):
            with redirect_stdout(output), redirect_stderr(errors):
                try:
                    runpy.run_path(str(TOOLS / name), run_name="__main__")
                    status = 0
                except SystemExit as error:
                    status = error.code or 0
        message = output.getvalue() + errors.getvalue()
        if expected is None:
            self.assertNotEqual(status, 0, message)
        else:
            self.assertEqual(status, expected, message)
        return message

    def read_report(self, directory, name="measurement.json"):
        return json.loads((directory / name).read_text())

    def compile_sources(self, sources, name="project"):
        project = self.root / name
        (project / "src").mkdir(parents=True)
        (project / "bifoil.toml").write_text('entry = "main.start"\n')
        for filename, text in sources.items():
            path = project / "src" / filename
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        output = project / "startup.txt"
        result = subprocess.run(
            [str(self.compiler), "build", str(project), "--out", str(output)],
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        line = next(line for line in output.read_text().splitlines() if line.startswith("*bifoil "))
        program = json.loads(line.removeprefix("*bifoil "))
        self.assertEqual(program["entry"], "main.start")
        return program
