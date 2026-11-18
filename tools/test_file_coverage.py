"""Exercise the exact per-file CI policy with original coverage reports and a real source inventory."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / ".github/workflows/sonar.yml").is_file())


class PerFileCoverage(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bifoil-file-coverage-")
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        self.root = directory / "project"
        self.root.mkdir()
        workflow = (ROOT / ".github/workflows/sonar.yml").read_text()
        step = workflow.split("      - name: Require tests and at least 80%", 1)[1]
        body = step.split("python3 - <<'PY'\n", 1)[1].split("\n          PY\n", 1)[0]
        self.runner = directory / "check.py"
        self.runner.write_text(textwrap.dedent(body))
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True, capture_output=True)
        self.properties = {"sonar.sources": "src", "sonar.tests": "test"}

    def write(self, name, contents):
        file = self.root / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(contents)
        return file

    def source(self, name="src/original.mjs"):
        self.write(name, "\n".join(["original source"] * 12) + "\n")
        return name

    def lcov(self, name, hits, branches=()):
        lines = ["SF:" + name]
        lines.extend(f"DA:{index},{count}" for index, count in enumerate(hits, 1))
        lines.extend(f"BRDA:1,0,{index},{count}" for index, count in enumerate(branches))
        return "\n".join([*lines, "end_of_record", ""])

    def report(self, key, contents, name="report.xml"):
        self.properties[key] = name
        self.write(name, contents)

    def check(self, expected=0):
        self.write("sonar-project.properties", "\n".join(f"{key}={value}" for key, value in self.properties.items()))
        result = subprocess.run(
            [sys.executable, str(self.runner)],
            cwd=self.root,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
            env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"),
        )
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result.stdout + result.stderr

    def test_exact_eighty_passes_and_a_larger_project_average_cannot_hide_one_file(self):
        name = self.source()
        self.report("sonar.javascript.lcov.reportPaths", self.lcov(name, [1, 1, 1, 1, 0]), "coverage.lcov")
        self.assertIn("80.00% (4/5)", self.check())
        other = self.source("src/other.mjs")
        self.report(
            "sonar.javascript.lcov.reportPaths", self.lcov(name, [1, 0]) + self.lcov(other, [1] * 12), "coverage.lcov"
        )
        self.assertIn("LOW COVERAGE " + name, self.check(1))

    def test_combined_coverage_counts_line_and_condition_outcomes(self):
        name = self.source()
        self.report("sonar.javascript.lcov.reportPaths", self.lcov(name, [1, 0], [1, 1, 1]), "coverage.lcov")
        self.assertIn("80.00% (4/5)", self.check())
        self.report("sonar.javascript.lcov.reportPaths", self.lcov(name, [1, 0], [1, "-", 1]), "coverage.lcov")
        self.assertIn("LOW COVERAGE", self.check(1))

    def test_missing_or_unexecuted_sources_are_failures(self):
        name = self.source()
        self.assertIn("UNMEASURED " + name, self.check(1))
        self.report("sonar.javascript.lcov.reportPaths", self.lcov(name, [0] * 5), "coverage.lcov")
        self.assertIn("UNTESTED " + name, self.check(1))

    def test_multiple_line_observations_do_not_inflate_the_denominator(self):
        name = self.source()
        self.report(
            "sonar.javascript.lcov.reportPaths",
            self.lcov(name, [1, 1, 1, 0, 0]) + self.lcov(name, [1, 0, 0, 1, 0]),
            "coverage.lcov",
        )
        self.assertIn("80.00% (4/5)", self.check())

    def test_absolute_and_backslash_source_paths_are_normalized(self):
        name = self.source()
        outside = str(self.root.parent / "outside.mjs")
        self.report(
            "sonar.javascript.lcov.reportPaths",
            self.lcov(str(self.root / name), [1]) + self.lcov(outside, [1]) + self.lcov(name.replace("/", "\\"), [1]),
            "coverage.lcov",
        )
        self.assertIn("100.00% (1/1)", self.check())

    def test_lcov_rejects_negative_execution_counts_and_duplicate_conditions(self):
        name = self.source()
        for contents in (
            self.lcov(name, [-1]),
            self.lcov(name, [1], [-1]),
            self.lcov(name, [1], [1, 0]).replace("BRDA:1,0,1", "BRDA:1,0,0"),
            self.lcov(name, [1]).replace("DA:1,1", "DA:1,1.5"),
            self.lcov(name, [1]).replace("DA:1,1", "DA:invalid,1"),
            self.lcov(name, [1], [1]).replace("BRDA:1,", "BRDA:invalid,"),
            "SF:src/other.mjs\n" + self.lcov(name, [1]),
        ):
            with self.subTest(contents=contents):
                self.report("sonar.javascript.lcov.reportPaths", contents, "coverage.lcov")
                self.check(1)

    def test_cobertura_resolves_its_source_base_and_uses_exact_condition_counts(self):
        self.source("src/original.py")
        self.report(
            "sonar.python.coverage.reportPaths",
            """<coverage><sources><source>src</source></sources>
<packages><package><classes><class filename="original.py"><lines>
<line number="1" hits="2" branch="true" condition-coverage="80% (4/5)"/>
<line number="2" hits="0"/></lines></class></classes></package></packages></coverage>""",
        )
        self.assertIn("71.43% (5/7)", self.check(1))

    def test_cobertura_missing_exact_condition_counts_cannot_pass(self):
        self.source("src/original.py")
        for attributes in (
            'branch="true"',
            'branch="true" condition-coverage="100%"',
            'branch="true" condition-coverage="100% (3/2)"',
        ):
            with self.subTest(attributes=attributes):
                self.report(
                    "sonar.python.coverage.reportPaths",
                    '<coverage><class filename="src/original.py"><lines>'
                    '<line number="1" hits="1" ' + attributes + "/></lines></class></coverage>",
                )
                self.check(1)

    def test_a_displayed_eighty_is_rejected_when_the_actual_fraction_is_lower(self):
        self.source("src/original.c")
        self.report(
            "sonar.coverageReportPaths",
            """<coverage version="1"><file path="src/original.c">
<lineToCover lineNumber="1" covered="true" coveredBranches="799999999" branchesToCover="1000000000"/>
</file></coverage>""",
        )
        self.assertIn("80.00%", self.check(1))

    def test_generic_counts_merge_with_go_lines_without_double_counting(self):
        self.source("src/original.go")
        self.write("go.mod", "module original.example/project\n")
        self.report(
            "sonar.go.coverage.reportPaths",
            "mode: atomic\noriginal.example/project/src/original.go:1.1,4.9 2 1\n"
            "original.example/project/src/original.go:5.1,5.9 1 0\n",
            "go.out",
        )
        self.report(
            "sonar.coverageReportPaths",
            """<coverage version="1"><file path="src/original.go">
<lineToCover lineNumber="1" covered="true" coveredBranches="1" branchesToCover="2"/>
</file></coverage>""",
        )
        self.assertIn("71.43% (5/7)", self.check(1))

    def test_zero_statement_go_blocks_retain_the_go_importers_line_coverage(self):
        self.source("src/original.go")
        self.write("go.mod", "module original.example/project\n")
        self.report(
            "sonar.go.coverage.reportPaths",
            "mode: set\noriginal.example/project/src/original.go:1.1,1.9 1 1\n"
            "original.example/project/src/original.go:2.1,9.9 0 0\n",
            "go.out",
        )
        self.assertIn("11.11% (1/9)", self.check(1))

    def test_go_boundary_braces_and_blank_lines_do_not_inflate_coverage(self):
        self.write("src/original.go", "func original() {\n\nwork()\n}\nother()\n")
        self.write("go.mod", "module original.example/project\n")
        self.report(
            "sonar.go.coverage.reportPaths",
            "mode: atomic\noriginal.example/project/src/original.go:1.17,4.2 1 1\n"
            "original.example/project/src/original.go:5.1,5.8 1 0\n",
            "go.out",
        )
        self.assertIn("50.00% (1/2)", self.check(1))

    def test_go_rejects_bad_headers_ranges_and_counters(self):
        self.source("src/original.go")
        self.write("go.mod", "module original.example/project\n")
        for contents in (
            "mode: invalid\n",
            "mode: atomic\noriginal.example/project/src/original.go:2.1,1.9 1 1\n",
            "mode: atomic\noriginal.example/project/src/original.go:1.1,2.9 -1 1\n",
            "mode: atomic\noriginal.example/project/src/original.go:1.1,2.9 1 -1\n",
            "mode: atomic\noriginal.example/project/src/original.go:invalid 1 1\n",
        ):
            with self.subTest(contents=contents):
                self.report("sonar.go.coverage.reportPaths", contents, "go.out")
                self.check(1)

    def test_generic_reports_reject_invalid_or_conflicting_counts(self):
        self.source("src/original.c")
        for attributes in (
            'covered="unknown"',
            'covered="true" branchesToCover="2" coveredBranches="3"',
            'covered="true" branchesToCover="-1" coveredBranches="0"',
        ):
            with self.subTest(attributes=attributes):
                self.report(
                    "sonar.coverageReportPaths",
                    '<coverage><file path="src/original.c">'
                    '<lineToCover lineNumber="1" ' + attributes + "/></file></coverage>",
                )
                self.check(1)
        self.report(
            "sonar.coverageReportPaths",
            """<coverage><file path="src/original.c">
<lineToCover lineNumber="1" covered="true" coveredBranches="2" branchesToCover="2"/>
<lineToCover lineNumber="1" covered="true" coveredBranches="1" branchesToCover="2"/></file></coverage>""",
        )
        self.assertIn("Conflicting condition reports", self.check(1))

    def test_jacoco_combines_kotlin_executable_lines_and_branch_outcomes(self):
        self.source("src/example/Original.kt")
        self.report(
            "sonar.coverage.jacoco.xmlReportPaths",
            """<report><package name="example"><sourcefile name="Original.kt">
<line nr="1" mi="0" ci="2" mb="0" cb="2"/><line nr="2" mi="2" ci="0" mb="0" cb="0"/>
<line nr="3" mi="0" ci="1" mb="0" cb="0"/></sourcefile></package></report>""",
        )
        self.assertIn("80.00% (4/5)", self.check())

    def test_jacoco_ambiguous_sources_and_negative_counts_are_rejected(self):
        self.source("src/example/Original.kt")
        report = """<report><package name="example"><sourcefile name="Original.kt">
<line nr="1" mi="0" ci="1" mb="0" cb="0"/></sourcefile></package></report>"""
        self.report("sonar.coverage.jacoco.xmlReportPaths", report.replace('ci="1"', 'ci="-1"'))
        self.assertIn("Invalid JaCoCo", self.check(1))
        self.source("other/example/Original.kt")
        self.report("sonar.coverage.jacoco.xmlReportPaths", report)
        self.assertIn("Ambiguous", self.check(1))

    def test_test_sources_are_excluded_and_empty_production_inventories_cannot_pass(self):
        name = self.source()
        self.source("test/original.test.mjs")
        self.report("sonar.javascript.lcov.reportPaths", self.lcov(name, [1]), "coverage.lcov")
        self.assertNotIn("UNMEASURED", self.check())
        (self.root / name).unlink()
        self.assertIn("No production source files", self.check(1))

    def test_declaration_only_rust_requires_hashed_compilation_test_evidence(self):
        name = "src/original.rs"
        source = self.write(name, "pub struct Original {}\n")
        self.write("test/contract.rs", "#[test]\nfn original_contract() {}\n")
        self.write(
            "source-test-map.json",
            json.dumps(
                {
                    "compile_only": {
                        name: {
                            "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                            "test": "test/contract.rs",
                            "case": "original_contract",
                        }
                    }
                }
            ),
        )
        output = self.check()
        self.assertIn("COMPILE-TESTED", output)
        self.assertIn("N/A " + name, output)
        source.write_text("pub fn original() {}\n")
        self.assertIn("source changed", self.check(1))


if __name__ == "__main__":
    unittest.main()
