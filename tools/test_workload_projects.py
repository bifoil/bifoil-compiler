"""Compile generated workloads at small sizes and validate their public contracts."""

import unittest

from code_workload import code_modules, write_code_project
from function_workload import function_modules, write_function_project
from module_workload import write_scene_project
from narrative_workload import write_project
import workload_source as workloads
from test_support import NativeCompilerTest


class ProjectWriters(NativeCompilerTest):
    def test_code_and_function_projects_preserve_owned_paths_and_compile(self):
        cases = [
            ("code", lambda root: write_code_project(root, 2, 3, 2)),
            ("suspend", lambda root: write_code_project(root, 2, 3, 2, suspending=True)),
            ("public", lambda root: write_function_project(root, 2, 3)),
            ("private", lambda root: write_function_project(root, 2, 3, export_routines=False)),
            ("scenes", lambda root: write_scene_project(root, 2)),
            ("prose", lambda root: write_project(root, scenes=2, lines=3)),
        ]
        for name, writer in cases:
            with self.subTest(name=name):
                project = self.root / name
                project.mkdir()
                sentinel = project / "unrelated.txt"
                sentinel.write_text("preserve this file")
                sources = writer(project)
                self.assertEqual(sentinel.read_text(), "preserve this file")
                self.assertEqual(
                    {path.relative_to(project / "src").as_posix() for path in (project / "src").rglob("*.bfl")},
                    set(sources),
                )
                for filename, text in sources.items():
                    self.assertEqual((project / "src" / filename).read_text(), text)
                program = self.compile_sources(sources, name + "-compiled")
                self.assertGreater(len(program["functions"]), 1)
                self.assertIn("choose", {instruction["op"] for function in program["functions"].values()
                                         for instruction in function["code"]})

    def test_small_typing_workloads_have_resolved_references(self):
        cases = [
            ("aliases", workloads.alias_graph_source(3)),
            ("scopes", workloads.alias_scope_source(3)),
            ("captured", workloads.capture_initializer_source(3)),
            ("reader", workloads.capture_reader_source(3)),
            ("callbacks", workloads.callback_initializer_source(3)),
            ("writes", workloads.shared_write_source(3)),
            ("calls", workloads.shared_call_source(3)),
            ("dependencies", workloads.shared_dependency_source(3, 3)),
            ("normalization", workloads.normalization_scope_source(3)),
            ("generic", workloads.generic_name_scope_source(3)),
            ("methods", workloads.method_body_source(3, 2)),
            ("constructors", workloads.named_constructor_source(3, 2)),
            ("defaults", workloads.calling_default_source(3)),
            ("forward", workloads.forward_alias_source(3)),
            ("branches", workloads.conditional_chain_source(3)),
            ("references", workloads.function_reference_source(3)),
            ("inferred", workloads.function_reference_source(3, inferred=True)),
            ("parameters", workloads.parameter_call_source(3)),
            ("type-parameters", workloads.generic_parameter_source(3)),
            ("index", workloads.index_guard_source(helpers=2, keys=2)),
            ("mutable-index", workloads.index_guard_source(True, helpers=2, keys=2)),
            ("compound-index", workloads.compound_index_source(helpers=2, keys=2)),
            ("computed-index", workloads.computed_index_guard_source(helpers=2, keys=2)),
            ("text-index", workloads.text_computed_index_guard_source(helpers=2, keys=2)),
            ("continue", workloads.continue_fact_source(3, 2)),
        ]
        cases.extend(("facts-" + form, workloads.assignment_fact_source(3, 2, form=form))
                     for form in ["conditional", "when", "choice"])
        cases.append(("delayed-facts", workloads.assignment_fact_source(3, 2, delayed=True)))
        cases.extend(("break-" + form, workloads.loop_break_source(3, 2, form=form))
                     for form in ["while", "for", "do", "labeled"])
        for name, source in cases:
            with self.subTest(name=name):
                program = self.compile_sources({"main.bfl": source}, name)
                self.assertIn("main.start", program["functions"])


class InvalidWorkloads(unittest.TestCase):
    def test_invalid_dimensions_and_modes_are_rejected(self):
        for dimensions in [(0, 1, 1), (1, 0, 1), (1, 1, 0), (-1, 2, 3)]:
            with self.subTest(dimensions=dimensions), self.assertRaisesRegex(ValueError, "positive"):
                code_modules(*dimensions)
        for dimensions in [(0, 1), (1, 0), (-1, 2)]:
            with self.subTest(dimensions=dimensions), self.assertRaisesRegex(ValueError, "positive"):
                function_modules(*dimensions)
        for generate, arguments in [
            (workloads.program_source, ["unknown"]),
            (workloads.scoped_val_source, ["unknown"]),
        ]:
            with self.subTest(generate=generate.__name__), self.assertRaisesRegex(ValueError, "unknown"):
                generate(*arguments)
        with self.assertRaisesRegex(ValueError, "unknown branch"):
            workloads.assignment_fact_source(form="unknown")
        with self.assertRaisesRegex(ValueError, "unknown loop"):
            workloads.loop_break_source(form="unknown")
