"""Match Python process exit semantics in the in-process CLI test harness."""
import unittest
from unittest.mock import patch
from test_support import NativeCompilerTest

class ToolExitStatus(unittest.TestCase):
    def test_none_integers_booleans_and_messages_match_process_exit_status(self):
        harness = NativeCompilerTest()
        for value, expected in [(None, 0), (0, 0), (False, 0), (True, 1), (2, 2), ("", 1), ("0", 1), ("failure", 1)]:
            with self.subTest(value=value), patch("test_support.runpy.run_path", side_effect=SystemExit(value)):
                harness.run_tool("synthetic-exit.py", [], expected=expected)
