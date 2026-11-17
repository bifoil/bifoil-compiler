"""Package observers must reject stale, rewritten, or semantically unequal output."""
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from test_support import NativeCompilerTest, load_tool

packages = load_tool("benchmark-packages.py")


class PackagePathTests(unittest.TestCase):
    def test_windows_translation_requires_the_mounted_drive(self):
        self.assertEqual(packages.windows_path(Path("/mnt/c/example")), "C:/example")
        with self.assertRaisesRegex(ValueError, "C drive"):
            packages.windows_path(Path("/tmp/example"))

    def test_restored_execution_mismatch_is_not_a_successful_observation(self):
        with patch.object(packages.subprocess, "run", return_value=subprocess.CompletedProcess(["node"], 0, '{"error":"failed"}', "")):
            with self.assertRaisesRegex(RuntimeError, "unexpected restored execution"):
                packages.execute(Path("startup.txt"), 11, Path("execution.json"), Path("runtime"), Path("node"))

    def test_baseline_comparison_checks_each_publication_phase(self):
        before = {phase: {"root_sha256": phase} for phase in ["populate", "unchanged", "leaf_edit"]}
        for phase in before:
            after = {name: dict(value) for name, value in before.items()}
            after[phase]["root_sha256"] = "changed"
            with self.subTest(phase=phase), self.assertRaisesRegex(RuntimeError, "relative to the baseline"):
                packages.verify_baseline({"runs": {"before": before, "after": after}}, "before", "after")


class PackageObserverControls(NativeCompilerTest):
    def test_runtime_options_and_out_of_profile_dimensions_reject_before_generation(self):
        for arguments in [["--runtime", self.root], ["--modules", "0"], ["--modules", "64"],
                          ["--functions", "0"], ["--functions", "4001"]]:
            target = self.root / "not-created"
            self.run_tool("benchmark-packages.py", [self.compiler, target, *arguments], expected=2)
            self.assertFalse(target.exists())

    def test_replaced_unchanged_outputs_are_rejected(self):
        original = packages.measure
        def measure(command):
            result = original(command)
            if "0 compiled" in result["stdout"]:
                output = Path(command[command.index("--out") + 1])
                output.write_bytes(output.read_bytes() + b"\n")
            return result
        options = SimpleNamespace(modules=1, functions=1, runtime=None, node=None)
        with patch.object(packages, "measure", side_effect=measure):
            with self.assertRaisesRegex(RuntimeError, "unchanged build replaced"):
                packages.benchmark(self.compiler, self.root / "unchanged", False, options, None)

    def test_missing_module_reuse_summary_is_a_failed_measurement(self):
        original = packages.measure
        def measure(command):
            result = original(command)
            result["stdout"] = ""
            return result
        options = SimpleNamespace(modules=1, functions=1, runtime=None, node=None)
        with patch.object(packages, "measure", side_effect=measure):
            with self.assertRaisesRegex(RuntimeError, "unexpected module reuse"):
                packages.benchmark(self.compiler, self.root / "summary", False, options, None)

    def test_a_leaf_edit_must_publish_exactly_one_new_code_scene_and_keep_previous_bytes(self):
        original = packages.measure
        for mode in ["missing-new-code", "missing-existing-code"]:
            directory = self.root / mode
            phase = 0
            def measure(command):
                nonlocal phase
                phase += 1
                result = original(command)
                if phase == 3:
                    output = Path(command[command.index("--out") + 1])
                    code = sorted(output.parent.glob("bifoil_code_*.txt"))
                    if mode == "missing-new-code":
                        # Restore the directory's pre-edit inventory while retaining
                        # the real changed root; the observer must notice no new code.
                        for path in code:
                            if path.name not in initial:
                                path.unlink()
                    else:
                        path = output.parent / next(iter(initial))
                        path.unlink()
                if not initial:
                    output = Path(command[command.index("--out") + 1])
                    initial.update(path.name for path in output.parent.glob("bifoil_code_*.txt"))
                return result
            initial = set()
            options = SimpleNamespace(modules=1, functions=1, runtime=None, node=None)
            with self.subTest(mode=mode), patch.object(packages, "measure", side_effect=measure):
                with self.assertRaisesRegex(RuntimeError, "exactly one code scene|immutable code scenes"):
                    packages.benchmark(self.compiler, directory, True, options, None)


if __name__ == "__main__":
    unittest.main()
