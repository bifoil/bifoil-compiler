"""Exercise formatting, artifact-only linking, and packaged execution together."""

import os
from pathlib import Path
from unittest.mock import patch

from test_support import NativeCompilerTest, load_tool


class NativeRunnerContracts(NativeCompilerTest):
    def test_formatted_directory_case_precompiles_its_dependency_and_executes_a_package(self):
        runtime, node = os.environ.get("BIFOIL_TEST_RUNTIME"), os.environ.get("BIFOIL_TEST_NODE")
        if not runtime or not node:
            self.skipTest("supply explicit runtime and Node dependencies for execution")
        runner = load_tool("run-conformance.py")
        directory = self.root / "original"
        directory.mkdir()
        (directory / "main.bfl").write_text(
            "import helper.value\nscene start():\n\tprose:\n\t\t${value()}\n"
        )
        (directory / "helper.bfl").write_text("export fun value() -> Int = 7\n")
        before = {source.name: source.read_bytes() for source in directory.iterdir()}
        real_child = runner.run_child
        phases = []

        def child(phase, command, timeout):
            phases.append(phase)
            if phase == "compilation":
                self.assertIn("--package", command)
                project = Path(command[2])
                self.assertFalse((project / "src/helper.bfl").exists())
                self.assertTrue((project / "modules/helper.bfm.json").is_file())
            return real_child(phase, command, timeout)

        with patch.object(runner, "run_child", side_effect=child):
            failure = runner.run_case(
                {"id": "source-free", "directory": "original", "precompile": ["helper"], "output": ["7"]},
                self.root / "cases.json", self.compiler, Path(runtime), node, False, None,
                packaged=True, formatted=True,
            )
        self.assertIsNone(failure)
        self.assertEqual(phases.count("source formatting idempotence"), 2)
        self.assertIn("module precompilation", phases)
        self.assertEqual(phases[-1], "execution")
        self.assertEqual(before, {source.name: source.read_bytes() for source in directory.iterdir()})
