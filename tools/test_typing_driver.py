"""Typing benchmark report and cache contracts, independent of native performance measurements."""

import hashlib
import io
import json
import runpy
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import process_measurement


class TypingDriverTests(unittest.TestCase):
    def test_successful_reports_preserve_workload_dimensions_and_distinguish_edits_from_cache_reuse(self):
        cases = [
            ("fields", {"fields": 32}),
            ("generic-parameters", {"fields": 512, "generic_parameters": 512}),
            ("continue-branches", {"facts": 4000, "branches": 1024}),
        ]
        for workload, dimensions in cases:
            with (
                self.subTest(workload=workload),
                tempfile.TemporaryDirectory(prefix="bifoil-typing-driver-") as directory,
            ):
                root = Path(directory)
                compiler = root / "compiler-fixture"
                compiler.write_bytes(b"original synthetic driver fixture")
                project = root / "project"
                calls = []

                def observe(command, project=project, calls=calls):
                    # Exercise the benchmark's file/report decisions without claiming native resource observations.
                    source = project / "src/main.bfl"
                    calls.append(source.read_bytes())
                    output = Path(command[command.index("--out") + 1])
                    output.write_bytes(hashlib.sha256(calls[-1]).hexdigest().encode())
                    return {
                        "status": 0,
                        "stdout": "0 compiled, 1 reused" if len(calls) == 2 else "1 compiled, 0 reused",
                        "stderr": "",
                        "compiler_peak_rss_bytes": 1,
                        "wall_seconds": 0,
                    }

                arguments = [
                    "benchmark-typing.py",
                    str(compiler),
                    str(project),
                    "--workload",
                    workload,
                    "--keep-original",
                ]
                script = Path(__file__).with_name("benchmark-typing.py")
                with (
                    patch.object(sys, "argv", arguments),
                    patch.object(process_measurement, "measure", observe),
                    redirect_stdout(io.StringIO()),
                ):
                    runpy.run_path(str(script), run_name="__main__")
                report = json.loads((project / "measurement.json").read_text())
                self.assertEqual({key: report[key] for key in dimensions}, dimensions)
                self.assertEqual(len(calls), 3)
                self.assertEqual(calls[0], calls[1])
                self.assertNotEqual(calls[1], calls[2])
                self.assertTrue(report["unchanged_output"])
                self.assertTrue(report["edited_output_changed"])
                self.assertEqual(
                    (project / "original.startup.txt").read_bytes(), hashlib.sha256(calls[0]).hexdigest().encode()
                )
