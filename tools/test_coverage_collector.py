"""Exercise the real shell collector with isolated external command boundaries."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class CoverageCollector(unittest.TestCase):
    def test_shell_collection_promotes_only_complete_observations(self):
        with tempfile.TemporaryDirectory(prefix="bifoil collector contract Ω ") as temporary:
            root = Path(temporary)
            tools = root / "tools"
            tools.mkdir()
            script = tools / "collect-coverage.sh"
            shutil.copyfile(ROOT / "tools/collect-coverage.sh", script)
            language, runtime, engine = root / "language", root / "runtime", root / "scene.js"
            (language / "conformance").mkdir(parents=True)
            for name in ["original.json", "second.json", "steering.json"]:
                (language / "conformance" / name).write_text("[]")
            (runtime / "tools").mkdir(parents=True)
            (runtime / "tools/execute-conformance.mjs").write_text("// Original command-boundary fixture.\n")
            engine.write_text("// Original unexecuted engine-path fixture.\n")
            commands = root / "commands"
            commands.mkdir()
            boundary = f"#!{sys.executable}\n" + r'''
import json, os, pathlib, sys
command = pathlib.Path(sys.argv[0]).name
arguments = sys.argv[1:]
with open(os.environ["BIFOIL_COLLECTOR_TRACE"], "a", encoding="utf-8") as trace:
    trace.write(json.dumps({"command": command, "args": arguments,
        "bootstrap": os.environ.get("RUSTC_BOOTSTRAP"), "profile": os.environ.get("LLVM_PROFILE_FILE")}) + "\n")
phase = os.environ.get("BIFOIL_COLLECTOR_FAIL")
if (phase == "native" and "--branch" in arguments or
        phase == "conformance" and command == "python3" or
        phase == "python" and command == "uv" and "run" in arguments[9:]):
    raise SystemExit(9)
if command == "cargo" and "--output-path" in arguments:
    pathlib.Path(arguments[arguments.index("--output-path") + 1]).write_text("Original native report\n")
if command == "uv" and "xml" in arguments:
    pathlib.Path(arguments[arguments.index("-o") + 1]).write_text("Original Python report\n")
'''
            for name in ["cargo", "uv", "node", "python3"]:
                file = commands / name
                file.write_text(boundary)
                file.chmod(0o755)
            trace = root / "calls.jsonl"
            environment = dict(os.environ, PATH=str(commands) + os.pathsep + os.environ["PATH"], BIFOIL_COLLECTOR_TRACE=str(trace))
            environment.pop("RUSTC_BOOTSTRAP", None)
            reports = root / ".bifoil/development/sonar"
            for phase in ["native", "conformance", "python", "success"]:
                with self.subTest(phase=phase):
                    reports.mkdir(parents=True, exist_ok=True)
                    for name in ["rust.lcov", "python-coverage.xml"]:
                        (reports / name).write_text("Stale report")
                    trace.write_text("")
                    environment["BIFOIL_COLLECTOR_FAIL"] = phase
                    result = subprocess.run(["bash", str(script), str(language), str(runtime), str(engine)], cwd=root,
                        env=environment, capture_output=True, text=True, timeout=15, check=False)
                    self.assertFalse(list(reports.glob("coverage.*")), result.stderr)
                    if phase != "success":
                        self.assertEqual(result.returncode, 9, result.stderr)
                        self.assertFalse((reports / "rust.lcov").exists())
                        self.assertFalse((reports / "python-coverage.xml").exists())
                        continue
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual((reports / "rust.lcov").read_text(), "Original native report\n")
                    self.assertEqual((reports / "python-coverage.xml").read_text(), "Original Python report\n")
                    calls = [json.loads(row) for row in trace.read_text().splitlines()]
                    build = next(call for call in calls if call["command"] == "cargo" and "build" in call["args"])
                    native = next(call for call in calls if "--branch" in call["args"])
                    conformance = next(call for call in calls if call["command"] == "python3")
                    self.assertIsNone(build["bootstrap"])
                    self.assertEqual(native["bootstrap"], "1")
                    self.assertIn(str(root / "target/llvm-cov-target/release/bifoilc"), conformance["args"])
                    self.assertTrue(conformance["profile"].endswith("conformance-%4m.profraw"))
                    self.assertIn(str(language / "conformance/original.json"), conformance["args"])
                    self.assertNotIn(str(language / "conformance/steering.json"), conformance["args"])

    def test_shell_collection_requires_explicit_dependencies_before_touching_reports(self):
        for arguments in [[], ["one"], ["one", "two"], ["one", "two", "three", "four"]]:
            with self.subTest(arguments=arguments):
                result = subprocess.run(["bash", str(ROOT / "tools/collect-coverage.sh"), *arguments],
                    capture_output=True, text=True, check=False, timeout=10)
                self.assertEqual(result.returncode, 64)
                self.assertIn("Usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
