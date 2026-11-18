"""Ownership, atomic-report, and verifier rejection controls for capacity measurements."""
import json
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from capacity_workload import NarrativeCapacity
from test_support import NativeCompilerTest, load_tool

benchmark = load_tool("benchmark-capacity.py")


class CapacityPathControls(unittest.TestCase):
    def test_node_path_translation_never_guesses_a_windows_mount(self):
        self.assertEqual(benchmark.node_path(Path("/tmp/source"), Path("node")), "/tmp/source")
        self.assertEqual(benchmark.node_path(Path("/mnt/c/example/source"), Path("node.EXE")), "C:/example/source")
        with self.assertRaisesRegex(ValueError, "C drive"):
            benchmark.node_path(Path("/tmp/source"), Path("node.exe"))
        for index in [-1, 2]:
            workload = NarrativeCapacity(2, 1)
            with self.assertRaisesRegex(ValueError, "room index"):
                workload.room_source(index)


class CapacityFailureControls(NativeCompilerTest):
    def experiment(self, name="project"):
        options = SimpleNamespace(compiler=self.compiler, project=self.root / name,
                                  scenes=1, rows=2, report_name="capacity.json",
                                  runtime=None, node=None, publish=False)
        return benchmark.CapacityExperiment(options)

    def test_existing_input_metadata_must_match_dimensions_and_exact_source_inventory(self):
        experiment = self.experiment()
        manifest = experiment.project / "generated-inputs.json"
        original = manifest.read_text()
        for change in [lambda value: value.update(format="unknown"),
                       lambda value: value["description"].update(scenes=2),
                       lambda value: value["source_files"].update({"../outside.bfl": {}})]:
            data = json.loads(original)
            change(data)
            manifest.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError, "manifest"):
                benchmark.checked_inputs(experiment.spec, experiment.project)
        manifest.write_text(original)
        self.assertEqual(benchmark.checked_inputs(experiment.spec, experiment.project), experiment.inputs)

    def test_failed_atomic_report_replacement_preserves_the_previous_observation(self):
        experiment = self.experiment()
        experiment.record()
        previous = experiment.report_path.read_bytes()
        experiment.report["new"] = "not committed"
        with (
            patch.object(benchmark.os, "replace", side_effect=OSError("commit failed")),
            self.assertRaisesRegex(OSError, "commit failed"),
        ):
            experiment.record()
        self.assertEqual(experiment.report_path.read_bytes(), previous)
        self.assertEqual(list(experiment.project.glob(".capacity-report-*.json")), [])

    def test_a_preexisting_original_program_is_never_overwritten(self):
        experiment = self.experiment()
        original = experiment.output.with_name("original.capacity.startup.txt")
        original.parent.mkdir()
        original.write_text("retained original")
        with self.assertRaisesRegex(ValueError, "preserve the existing original"):
            experiment.populate()
        self.assertEqual(original.read_text(), "retained original")

    def test_ambiguous_leaf_edits_and_changed_publication_sources_fail_before_compilation(self):
        experiment = self.experiment()
        experiment.leaf.write_text("no unique end marker")
        with self.assertRaisesRegex(RuntimeError, "ambiguous leaf edit"):
            experiment.edit_leaf()
        first = experiment.project / "src/stats.bfl"
        first.write_text("user modification")
        with self.assertRaisesRegex(RuntimeError, "changed before publication"):
            experiment.publish_modules()
        self.assertEqual(first.read_text(), "user modification")

    def test_source_changes_during_module_publication_keep_the_source(self):
        experiment = self.experiment()
        original = experiment.observe
        source = experiment.project / "src/stats.bfl"
        def observe(*args, **kwargs):
            result = original(*args, **kwargs)
            source.write_text("changed after compilation")
            return result
        with (
            patch.object(experiment, "observe", side_effect=observe),
            self.assertRaisesRegex(RuntimeError, "changed during publication"),
        ):
            experiment.publish_modules()
        self.assertEqual(source.read_text(), "changed after compilation")

    def test_foreign_cache_archives_and_unexpected_cache_contents_are_preserved(self):
        experiment = self.experiment()
        cache = experiment.project / ".bifoil/cache/v1"
        cache.mkdir(parents=True)
        archive = cache.with_name("source-capacity")
        archive.mkdir()
        with self.assertRaisesRegex(ValueError, "preserve the existing source-cache"):
            experiment.source_free()
        archive.rmdir()
        foreign = cache / "foreign.bin"
        foreign.write_bytes(b"preserve")
        with self.assertRaisesRegex(ValueError, "unexpected files"):
            experiment.source_free()
        self.assertEqual(foreign.read_bytes(), b"preserve")

    def test_verifier_exit_status_and_negative_acceptance_are_retained_as_failures(self):
        experiment = self.experiment()
        runtime = self.root / "runtime"
        (runtime / "tools").mkdir(parents=True)
        (runtime / "tools/verify-narrative-capacity.mjs").write_text("// authored verifier fixture\n")
        experiment.options.runtime = runtime
        experiment.options.node = self.root / "node"
        for result, message in [(subprocess.CompletedProcess(["node"], 2, "", "rejected"), "rejected"),
                                (subprocess.CompletedProcess(["node"], 0, '{"passed":false}', ""), "runtime verification rejected")]:
            with self.subTest(status=result.returncode), patch.object(benchmark.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(RuntimeError, message):
                    experiment.verify("negative", experiment.output, True)
                observed = json.loads(experiment.report_path.read_text())["verification"]["negative"]
                self.assertEqual(observed["status"], result.returncode)
                self.assertEqual(observed["stdout"], result.stdout)
        with patch.object(benchmark.subprocess, "run", return_value=subprocess.CompletedProcess(["node"], 0, '{"passed":true,"checked":[]}', "")):
            experiment.verify("accepted", experiment.output, False)
        self.assertTrue(experiment.report["verification"]["accepted"]["checked"]["passed"])

    def test_report_creation_failure_preserves_the_previous_report(self):
        experiment = self.experiment()
        experiment.record()
        previous = experiment.report_path.read_bytes()
        with (
            patch.object(benchmark.tempfile, "mkstemp", side_effect=OSError("no temporary file")),
            self.assertRaisesRegex(OSError, "no temporary file"),
        ):
            experiment.record()
        self.assertEqual(experiment.report_path.read_bytes(), previous)
        self.assertEqual(list(experiment.project.glob(".capacity-report-*.json")), [])

    def test_failed_compilation_without_an_output_is_recorded_without_a_fabricated_digest(self):
        experiment = self.experiment()
        failure = {"status": 1, "stderr": "original compiler failure", "wall_seconds": 0,
                   "compiler_peak_rss_bytes": 0}
        with (
            patch.object(benchmark, "measure", return_value=failure),
            self.assertRaisesRegex(RuntimeError, "original compiler failure"),
        ):
            experiment.observe("missing-output", experiment.command)
        recorded = json.loads(experiment.report_path.read_text())["runs"]["missing-output"]
        self.assertEqual(recorded["status"], 1)
        self.assertNotIn("root_sha256", recorded)
        self.assertFalse(experiment.output.exists())

    def test_changed_unchanged_build_is_rejected_and_keeps_the_original_output(self):
        experiment = self.experiment()
        calls = 0

        def measured_compiler(_arguments):
            nonlocal calls
            experiment.output.parent.mkdir(exist_ok=True)
            experiment.output.write_text("original program" if calls == 0 else "unexpected program")
            calls += 1
            return {"status": 0, "stderr": "", "wall_seconds": 0, "compiler_peak_rss_bytes": 0}

        with (
            patch.object(benchmark, "measure", side_effect=measured_compiler),
            self.assertRaisesRegex(RuntimeError, "unchanged compilation altered"),
        ):
            experiment.populate()
        retained = experiment.output.with_name("original.capacity.startup.txt")
        self.assertEqual(retained.read_text(), "original program")
        self.assertEqual(calls, 2)
        self.assertNotEqual(experiment.report["runs"]["populate"]["root_sha256"],
                            experiment.report["runs"]["unchanged_0"]["root_sha256"])

    def test_a_missed_leaf_edit_is_rejected_before_runtime_verification(self):
        experiment = self.experiment()
        experiment.output.parent.mkdir()
        experiment.output.write_text("unchanged compiler output")
        experiment.original = {"root_sha256": benchmark.checksum(experiment.output)}
        success = {"status": 0, "stderr": "", "wall_seconds": 0, "compiler_peak_rss_bytes": 0}
        with (
            patch.object(benchmark, "measure", return_value=success),
            self.assertRaisesRegex(RuntimeError, "source edit was missed"),
        ):
            experiment.edit_leaf()
        self.assertTrue(experiment.report["leaf_edit"]["same_size"])
        self.assertTrue(experiment.report["leaf_edit"]["same_mtime"])
        self.assertEqual(experiment.report["verification"], {})

    def test_a_changed_edit_timestamp_fails_before_running_the_compiler(self):
        experiment = self.experiment()
        previous = experiment.leaf.stat().st_mtime_ns
        real_utime = benchmark.os.utime

        def changed_timestamp(path, *, ns):
            real_utime(path, ns=(ns[0], ns[1] + 2_000_000_000))

        with (
            patch.object(benchmark.os, "utime", side_effect=changed_timestamp),
            patch.object(experiment, "observe") as observe,
            self.assertRaisesRegex(RuntimeError, "preserve size and timestamp"),
        ):
            experiment.edit_leaf()
        observe.assert_not_called()
        self.assertNotEqual(experiment.leaf.stat().st_mtime_ns, previous)
        self.assertTrue(experiment.report["leaf_edit"]["same_size"])
        self.assertFalse(experiment.report["leaf_edit"]["same_mtime"])

    def populated_cache(self, experiment):
        cache = experiment.project / ".bifoil/cache/v1"
        cache.mkdir(parents=True)
        names = {"build.sha256", "completed.json"}
        names.update(name.removesuffix(".bfl").replace("/", ".") + ".bfm.json"
                     for name in experiment.inputs["source_files"])
        for name in names:
            (cache / name).write_text("original owned cache " + name)
        return cache

    def test_hard_linked_cache_files_are_preserved_and_never_archived(self):
        experiment = self.experiment()
        cache = self.populated_cache(experiment)
        source = cache / "completed.json"
        retained = experiment.project / "retained-cache-link"
        retained.hardlink_to(source)
        with self.assertRaisesRegex(ValueError, "aliased or not owned"):
            experiment.source_free()
        self.assertEqual(source.read_bytes(), retained.read_bytes())
        self.assertTrue(cache.is_dir())
        self.assertFalse(cache.with_name("source-capacity").exists())

    def test_source_free_root_mismatch_retains_the_archived_cache(self):
        experiment = self.experiment()
        cache = self.populated_cache(experiment)
        experiment.edited = {"root_sha256": "expected edited program digest"}
        contents = {file.name: file.read_bytes() for file in cache.iterdir()}

        def measured_compiler(arguments):
            output = Path(arguments[arguments.index("--out") + 1])
            output.parent.mkdir(parents=True)
            output.write_text("wrong source-free program")
            return {"status": 0, "stderr": "", "wall_seconds": 0, "compiler_peak_rss_bytes": 0}

        with (
            patch.object(benchmark, "measure", side_effect=measured_compiler),
            self.assertRaisesRegex(RuntimeError, "publication changed the linked program"),
        ):
            experiment.source_free()
        archive = cache.with_name("source-capacity")
        self.assertFalse(cache.exists())
        self.assertEqual(contents, {file.name: file.read_bytes() for file in archive.iterdir()})

    def test_successful_source_workflow_does_not_publish_when_not_requested(self):
        experiment = self.experiment()
        experiment.run()
        report = json.loads(experiment.report_path.read_text())
        self.assertTrue(report["experiment_passed"])
        self.assertEqual(report["publication"], {})
        self.assertFalse((experiment.project / "modules").exists())
        for name in experiment.inputs["source_files"]:
            self.assertTrue((experiment.project / "src" / name).is_file())

    def test_runtime_and_node_options_must_be_supplied_together(self):
        self.run_tool("benchmark-capacity.py", [self.compiler, self.root / "missing", "--runtime", self.root], expected=2)


if __name__ == "__main__":
    unittest.main()
