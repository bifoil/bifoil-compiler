"""Validate observer exit/output records and malformed native metrics."""

from pathlib import Path
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

import process_measurement


class ProcessMeasurement(unittest.TestCase):
    @unittest.skipUnless(sys.platform.startswith("linux"), "GNU Time protocol requires Linux")
    def test_success_failure_and_signal_preserve_outputs(self):
        # Native shell children keep Python's coverage hooks out of the measured streams.
        for code, expected in [
            ("printf hello; printf details >&2", 0),
            ("printf failed; exit 7", 7),
            ("kill -TERM $$", 143),
        ]:
            with self.subTest(expected=expected):
                result = process_measurement.measure([shutil.which("sh"), "-c", code])
                self.assertEqual(result["status"], expected)
                self.assertGreater(result["compiler_peak_rss_bytes"], 0)
                self.assertGreaterEqual(result["wall_seconds"], 0)
                if expected == 0:
                    self.assertEqual(result["stdout"], "hello")
                    self.assertEqual(result["stderr"], "details")
                elif expected == 7:
                    self.assertEqual(result["stdout"], "failed")

    def test_unsupported_hosts_and_missing_observers_report_the_reason(self):
        with patch.object(process_measurement.sys, "platform", "darwin"):
            with self.assertRaisesRegex(RuntimeError, "requires Linux"):
                process_measurement.measure(["unused"])
        with patch.object(process_measurement.sys, "platform", "linux"), patch.object(
            process_measurement.shutil, "which", return_value=None
        ):
            with self.assertRaisesRegex(RuntimeError, "requires GNU Time"):
                process_measurement.measure(["unused"])

    def test_empty_negative_and_malformed_peak_records_are_errors(self):
        for record in [b"", b"-1\n", b"not a number\n"]:
            with self.subTest(record=record):
                def write_record(arguments, **_):
                    destination = next(item.removeprefix("--output=") for item in arguments
                                       if item.startswith("--output="))
                    Path(destination).write_bytes(record)
                    return subprocess.CompletedProcess(arguments, 0)
                with patch.object(process_measurement.sys, "platform", "linux"), patch.object(
                    process_measurement.shutil, "which", return_value="time"
                ), patch.object(process_measurement.subprocess, "run", side_effect=write_record):
                    with self.assertRaisesRegex(RuntimeError, "did not report compiler peak RSS"):
                        process_measurement.measure(["unused"])
