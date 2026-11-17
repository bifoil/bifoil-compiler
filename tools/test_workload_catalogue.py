"""Every advertised typing workload must produce parseable Bifoil source."""
import subprocess
from test_support import NativeCompilerTest
from workload_source import program_source

WORKLOADS = (
    "loop-exit-vals exclusive-forward-vals immutable-list-vals stored-range-vals "
    "range-forward-vals loop-forward-vals repeated-forward-vals mutable-forward-vals "
    "frame-forward-vals unit-forward-vals grouped-forward-vals forward-vals prose-vals "
    "operand-vals compound-vals indexed-vals assignment-vals default-vals expression-vals "
    "returning-vals generic-vals structured-vals parameter-vals scoped-vals single-use-vals "
    "projected-inputs generic-parameters function-parameters function-tables inferred-function-tables "
    "continue-branches continue-facts loop-breaks path-joins assignment-facts conditional-chains "
    "forward-aliases calling-defaults named-constructors method-bodies shared-dependencies "
    "shared-calls generic-name-scopes normalization-scopes compound-indexes text-computed-indexes "
    "computed-indexes indexes mutable-indexes aliases alias-scopes capture-initializers "
    "capture-readers callback-initializers shared-writes captures functions branches inference "
    "generics types fields mutable-fields shared nominals"
).split()

class WorkloadCatalogue(NativeCompilerTest):
    def test_every_public_selector_produces_one_parseable_entry_scene(self):
        for name in WORKLOADS:
            with self.subTest(workload=name):
                source = program_source(name)
                self.assertEqual(source.count("scene start():"), 1)
                self.assertTrue(source.endswith("\n"))
                self.assertNotIn("\x00", source)
                path = self.root / (name + ".bfl")
                path.write_text(source)
                result = subprocess.run([str(self.compiler), "format", str(path)],
                                        capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("scene start():", result.stdout)
                self.assertEqual(path.read_text(), source)
