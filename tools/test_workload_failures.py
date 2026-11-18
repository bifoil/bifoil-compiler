"""Workload transformations must reject missing or ambiguous original source shapes."""

import unittest
from unittest.mock import patch

import workload_source


class WorkloadShapeFailures(unittest.TestCase):
    def test_transforms_reject_an_incompatible_source_before_generating_a_workload(self):
        cases = [
            ("framed_forward_val_source", "scoped_val_source", "", "typed initializer"),
            ("mutable_forward_val_source", "framed_forward_val_source", "", "initializer cell"),
            ("repeated_forward_val_source", "mutable_forward_val_source", "", "before expansion"),
            ("loop_forward_val_source", "mutable_forward_val_source", "", "loop expansion"),
            ("range_forward_val_source", "loop_forward_val_source", "", "one break"),
            ("stored_range_val_source", "range_forward_val_source", "", "storing its progression"),
            ("unit_forward_val_source", "grouped_forward_val_source", "", "reader declaration"),
            (
                "unit_forward_val_source",
                "grouped_forward_val_source",
                "shared val readers: MutableList<() -> Int> = mutable []\n",
                "grouped forward call",
            ),
            ("grouped_forward_val_source", "scoped_val_source", "", "forward initializer call"),
            ("immutable_list_val_source", "range_forward_val_source", "", "storing its list"),
            ("exclusive_forward_val_source", "framed_forward_val_source", "", "splitting its alternatives"),
            ("loop_exit_val_source", "framed_forward_val_source", "", "terminal traversal"),
        ]
        for function, dependency, source, message in cases:
            with (
                self.subTest(function=function, message=message),
                patch.object(workload_source, dependency, return_value=source),
                self.assertRaisesRegex(ValueError, message),
            ):
                getattr(workload_source, function)()

    def test_forward_calls_require_both_a_unique_call_and_its_definition(self):
        call = "        initialize0000(count)\n"
        header = "        fun initialize0000(value: Int, increment: Int = 1):\n"
        for source in [call, header, call + call + header]:
            with (
                self.subTest(source=source),
                patch.object(workload_source, "scoped_val_source", return_value=source),
                self.assertRaisesRegex(ValueError, "one typed initializer call and definition"),
            ):
                workload_source.framed_forward_val_source()
