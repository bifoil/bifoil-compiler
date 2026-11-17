"""Check ring topology, workload accounting, and legacy control source bytes."""

import unittest
from cyclic_workload import module_source, sources


class CyclicWorkload(unittest.TestCase):
    def test_ring_sources_have_one_entry_and_the_requested_function_count(self):
        for modules, functions in [(2, 7), (8, 65), (64, 129)]:
            with self.subTest(modules=modules):
                program = sources(modules, functions)
                names = list(program)
                self.assertEqual(len(names), modules)
                self.assertEqual(
                    sum(
                        text.count("fun f") + text.count("scene start")
                        for text in program.values()
                    ),
                    functions,
                )
                self.assertEqual(
                    sum(text.count("scene start") for text in program.values()), 1
                )
                for index, text in enumerate(program.values()):
                    self.assertTrue(
                        text.startswith(
                            f"import {names[(index + 1) % modules]}.f0000 as peerFirst\n"
                        )
                    )
                if modules == 2:
                    self.assertEqual(program["alpha"], module_source("beta", 3, entry=True))
                    self.assertEqual(program["beta"], module_source("alpha", 3))

    def test_invalid_shapes_are_rejected_before_source_generation(self):
        for modules, functions in [(1, 10), (513, 1024), (8, 8), (2, 40_000), (8, 81_921)]:
            with self.subTest(modules=modules, functions=functions), self.assertRaises(ValueError):
                sources(modules, functions)
