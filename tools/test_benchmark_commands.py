"""Exercise benchmark report and artifact contracts with actual compiler processes."""

from pathlib import Path
import shutil

from test_support import NativeCompilerTest


class BenchmarkCommands(NativeCompilerTest):
    def benchmark(self, name, extra=()):
        directory = self.root / name.removesuffix(".py")
        self.run_tool(name, [self.compiler, directory, *extra])
        report = self.read_report(directory)
        results = report["runs"].values() if "runs" in report else [
            report, *([report["baseline"]] if "baseline" in report else []),
        ]
        for result in results:
            self.assertEqual(result["status"], 0, result.get("stderr"))
            self.assertGreater(result["compiler_peak_rss_bytes"], 0)
        return directory, report

    def test_clean_baseline_and_source_free_builds_agree(self):
        _, report = self.benchmark("benchmark.py", ["--baseline", self.compiler, "--workload", "functions"])
        self.assertTrue(report["output_matches_baseline"])
        self.assertEqual(report["functions"], 4096)
        _, precompiled = self.benchmark("benchmark-precompiled.py")
        self.assertTrue(precompiled["identical_output"])
        self.assertGreater(precompiled["artifact_bytes"], 0)

    def test_cache_imports_invalidate_only_the_expected_modules(self):
        directory, report = self.benchmark("benchmark-cache-import.py", ["--keep-original"])
        runs = report["runs"]
        self.assertEqual(runs["clean"]["program_sha256"], runs["unchanged"]["program_sha256"])
        self.assertNotEqual(runs["clean"]["program_sha256"], runs["scene_edit"]["program_sha256"])
        self.assertIn("1 compiled, 1 reused", runs["scene_edit"]["stdout"])
        self.assertTrue((directory / "original.startup.txt").is_file())
        for workload in ["shared-import", "nominal-import", "nominal-consumer"]:
            with self.subTest(workload=workload):
                project = self.root / workload
                self.run_tool("benchmark-cache-import.py", [self.compiler, project, "--workload", workload])
                measured = self.read_report(project)
                self.assertEqual(len(measured["runs"]), 4)
                edited = "dependency_edit" if workload == "nominal-consumer" else "scene_edit"
                self.assertNotEqual(measured["runs"]["clean"]["program_sha256"],
                                    measured["runs"][edited]["program_sha256"])

    def test_cyclic_and_acyclic_incremental_reports_keep_cache_evidence(self):
        _, cyclic = self.benchmark("benchmark-cyclic.py", ["--functions", 7])
        self.assertIn("2 compiled, 0 reused", cyclic["runs"]["member_edit"]["stdout"])
        _, incremental = self.benchmark("benchmark-incremental.py")
        self.assertIn("2 compiled, 31 reused", incremental["runs"]["leaf_edit"]["stdout"])

    def test_text_edits_preserve_size_and_change_compiled_output(self):
        for alphabet in ["ascii", "emoji"]:
            with self.subTest(alphabet=alphabet):
                directory = self.root / alphabet
                self.run_tool("benchmark-text.py", [self.compiler, directory, "--alphabet", alphabet,
                                                   "--literal-bytes", 40])
                report = self.read_report(directory)
                self.assertEqual(report["literal_scalars"], 40 if alphabet == "ascii" else 16)
                self.assertEqual(report["runs"]["clean"]["output_sha256"],
                                 report["runs"]["unchanged"]["output_sha256"])
                self.assertNotEqual(report["runs"]["literal_edit"]["output_sha256"],
                                    report["runs"]["unchanged"]["output_sha256"])

    def test_code_graph_scene_and_narrative_artifacts_survive_source_removal(self):
        for name, extra in [
            ("code", ["--modules", 2, "--routines", 3, "--additions", 2]),
            ("suspend", ["--modules", 2, "--routines", 3, "--additions", 2, "--suspend-routines"]),
            ("private", ["--workload", "function-graph", "--modules", 2, "--routines", 3,
                         "--private-routines"]),
        ]:
            with self.subTest(name=name):
                project = self.root / name
                self.run_tool("benchmark-code.py", [self.compiler, project, *extra])
                report = self.read_report(project)
                self.assertTrue(report["exact_source_free_root_bytes"])
                self.assertTrue(report["compiler_unchanged"])
                self.assertFalse(list((project / "src").rglob("*.bfl")))
        _, scenes = self.benchmark("benchmark-modules.py", ["--scenes", 2])
        self.assertTrue(scenes["edit_preserved_mtime"])
        self.assertTrue(scenes["compiler_unchanged"])
        _, prose = self.benchmark("benchmark-narrative.py", ["--scenes", 2, "--lines", 3])
        self.assertTrue(prose["large_source_format_identity"])
        self.assertTrue(prose["edit_preserved_size"])
        self.assertTrue(prose["compiler_unchanged"])

    def test_inline_and_package_baselines_preserve_exact_bytes(self):
        project = self.root / "packages"
        self.run_tool("benchmark-packages.py", [
            self.compiler, project, "--modules", 2, "--functions", 3,
            "--baseline", self.compiler, "--package-baseline", self.compiler,
        ])
        report = self.read_report(project)
        self.assertTrue(report["inline_output_matches_baseline"])
        self.assertTrue(report["package_output_matches_baseline"])
        for mode in report["runs"].values():
            self.assertEqual(mode["unchanged"]["written_files"], [])
            self.assertGreater(mode["leaf_edit"]["written_bytes"], 0)

    def test_dependency_discovery_compares_the_same_source_inventory(self):
        _, report = self.benchmark("benchmark-discovery.py", ["--baseline", self.compiler])
        self.assertEqual(report["modules"], 60)
        self.assertEqual(report["runs"]["current"]["output_sha256"],
                         report["runs"]["baseline"]["output_sha256"])

    def test_invalid_arguments_and_existing_outputs_do_not_publish_reports(self):
        for name, arguments in [
            ("benchmark-cyclic.py", ["--functions", 2]),
            ("benchmark-text.py", ["--literal-bytes", 7]),
            ("benchmark-text.py", ["--literal-bytes", 9, "--alphabet", "emoji"]),
            ("benchmark-modules.py", ["--scenes", 0]),
            ("benchmark-narrative.py", ["--lines", 0]),
            ("benchmark-packages.py", ["--modules", 0]),
            ("benchmark-packages.py", ["--runtime", self.root]),
        ]:
            with self.subTest(name=name, arguments=arguments):
                self.run_tool(name, [self.compiler, self.root / "rejected", *arguments], expected=None)
                self.assertFalse((self.root / "rejected/measurement.json").exists())
        for flags in [["--private-routines"], ["--workload", "function-graph", "--suspend-routines"],
                      ["--workload", "function-graph", "--additions", 2]]:
            with self.subTest(flags=flags), self.assertRaisesRegex(ValueError, "applies"):
                self.run_tool("benchmark-code.py", [self.compiler, self.root / "invalid", *flags])
        with self.assertRaisesRegex(ValueError, "new output"):
            self.run_tool("benchmark-code.py", [self.compiler, self.root])

    def test_failed_compilation_retains_a_failed_measurement(self):
        rejected = Path(shutil.which("false"))
        for name, flags in [("benchmark-code.py", ["--modules", 1, "--routines", 1]),
                            ("benchmark-narrative.py", ["--scenes", 1, "--lines", 1])]:
            with self.subTest(name=name):
                project = self.root / name
                with self.assertRaisesRegex(RuntimeError, "populate"):
                    self.run_tool(name, [rejected, project, *flags])
                report = self.read_report(project)
                self.assertNotEqual(report["runs"]["populate"]["status"], 0)
                self.assertFalse((project / "startup.txt").exists())
