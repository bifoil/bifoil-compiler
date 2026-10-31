"""Measure native incremental builds of bounded typing workloads.

Usage: uv run --no-project tools/benchmark-typing.py COMPILER OUTPUT_DIRECTORY
Generated projects and raw reports belong under ignored .bifoil/development/.
"""

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from process_measurement import measure
from workload_source import program_source


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(65536):
            digest.update(block)
    return digest.hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument(
    "--keep-original", action="store_true", help="retain the populated program for execution checks"
)
arguments.add_argument(
    "--workload",
    choices=[
        "nominals",
        "projected-inputs",
        "single-use-vals",
        "scoped-vals",
        "parameter-vals",
        "structured-vals",
        "generic-vals",
        "returning-vals",
        "expression-vals",
        "default-vals",
        "assignment-vals",
        "indexed-vals",
        "compound-vals",
        "operand-vals",
        "prose-vals",
        "forward-vals",
        "grouped-forward-vals",
        "unit-forward-vals",
        "frame-forward-vals",
        "mutable-forward-vals",
        "repeated-forward-vals",
        "loop-forward-vals",
        "range-forward-vals",
        "stored-range-vals",
        "immutable-list-vals",
        "exclusive-forward-vals",
        "loop-exit-vals",
        "method-bodies",
        "named-constructors",
        "calling-defaults",
        "forward-aliases",
        "conditional-chains",
        "assignment-facts",
        "path-joins",
        "loop-breaks",
        "continue-facts",
        "continue-branches",
        "function-tables",
        "inferred-function-tables",
        "function-parameters",
        "generic-parameters",
        "branches",
        "inference",
        "types",
        "fields",
        "mutable-fields",
        "indexes",
        "mutable-indexes",
        "computed-indexes",
        "text-computed-indexes",
        "compound-indexes",
        "generics",
        "captures",
        "aliases",
        "alias-scopes",
        "capture-initializers",
        "capture-readers",
        "callback-initializers",
        "shared-writes",
        "shared-calls",
        "shared-dependencies",
        "normalization-scopes",
        "generic-name-scopes",
    ],
    default="nominals",
)
options = arguments.parse_args()
root = options.output_directory.resolve()
(root / "src").mkdir(parents=True, exist_ok=True)
(root / "bifoil.toml").write_text('entry = "main.start"\n')
source = root / "src/main.bfl"
original = program_source(options.workload)
nominal = options.workload == "nominals"
edit_name, before, after = {
    "forward-vals": ("forward_val_edit", "x1023 = count + 1", "x1023 = count + 2"),
    "grouped-forward-vals": ("grouped_forward_val_edit", "x1023 = count + 1", "x1023 = count + 2"),
    "unit-forward-vals": ("unit_forward_val_edit", "x1023 = count + 1", "x1023 = count + 2"),
    "frame-forward-vals": ("frame_forward_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "mutable-forward-vals": ("mutable_forward_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "repeated-forward-vals": ("repeated_forward_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "loop-forward-vals": ("loop_forward_val_edit", "initialize1023(iteration1023)", "initialize1023(iteration1023, increment = 2)"),
    "range-forward-vals": ("range_forward_val_edit", "initialize1023(iteration1023)", "initialize1023(iteration1023, increment = 2)"),
    "stored-range-vals": ("stored_range_val_edit", "initialize1023(iteration1023)", "initialize1023(iteration1023, increment = 2)"),
    "immutable-list-vals": ("immutable_list_val_edit", "initialize1023(iteration1023)", "initialize1023(iteration1023, increment = 2)"),
    "exclusive-forward-vals": ("exclusive_forward_val_edit", "initialize1023(count, increment = 1)", "initialize1023(count, increment = 2)"),
    "loop-exit-vals": ("loop_exit_val_edit", "initialize1023(iteration1023)", "initialize1023(iteration1023, increment = 2)"),
    "prose-vals": ("prose_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "operand-vals": ("operand_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "compound-vals": ("compound_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "indexed-vals": ("indexed_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "assignment-vals": ("assignment_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "default-vals": ("default_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "expression-vals": ("expression_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "returning-vals": ("returning_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "generic-vals": ("generic_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "structured-vals": ("structured_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "parameter-vals": ("parameter_val_edit", "initialize1023(count)", "initialize1023(count, increment = 2)"),
    "scoped-vals": ("scoped_val_edit", "x1023 = count + 1", "x1023 = count + 2"),
    "single-use-vals": (
        "single_use_val_edit",
        "x1023 = x1022 + 1",
        "x1023 = x1022 + 2",
    ),
    "projected-inputs": (
        "projected_call_edit",
        "val result1023 = hidden.map(::record)",
        "val result1023 = hidden.map(::record) + 1",
    ),
    "generic-parameters": ("last_generic_argument_edit", "Last = Int", "Last = Float"),
    "function-parameters": ("last_named_argument_edit", "p00000 = mark(0)", "p00000 = mark(1)"),
    "inferred-function-tables": ("last_routine_edit", "f16382() = 16382", "f16382() = 16383"),
    "function-tables": ("last_routine_edit", "f16382() -> Int = 16382", "f16382() -> Int = 16383"),
    "continue-branches": ("selector_edit", "selector = -1", "selector = 1023"),
    "continue-facts": ("selector_edit", "selector = -1", "selector = 127"),
    "loop-breaks": ("selector_edit", "selector = -1", "selector = 127"),
    "path-joins": ("selector_edit", "selector = -1", "selector = 127"),
    "assignment-facts": ("selector_edit", "selector = -1", "selector = 127"),
    "conditional-chains": ("selector_edit", "selector = -1", "selector = 8191"),
    "forward-aliases": ("alias_terminal_edit", "A32767 = Int", "A32767 = Float"),
    "calling-defaults": ("default_helper_edit", "delta: Int = 1", "delta: Int = 2"),
    "named-constructors": ("named_argument_edit", "field2047 = 2062", "field2047 = 2063"),
    "method-bodies": ("method_field_edit", "offset: Int = 0", "offset: Int = 1"),
    "nominals": ("field_edit", "field0000: Int = 0", "field0000: Int = 1"),
    "branches": ("branch_edit", "return 1", "return 3"),
    "inference": ("inference_edit", "return input.length", "return input.length + 1"),
    "types": ("literal_edit", "val empty: Nested = []", "val empty: Nested = [[]]"),
    "fields": ("field_proof_edit", "return total", "return total + 1"),
    "mutable-fields": ("field_proof_edit", "return total", "return total + 1"),
    "indexes": ("lookup_edit", "return total", "return total + 1"),
    "mutable-indexes": ("lookup_edit", "return total", "return total + 1"),
    "computed-indexes": ("lookup_edit", "return total", "return total + 1"),
    "text-computed-indexes": ("lookup_edit", "return total", "return total + 1"),
    "compound-indexes": ("compound_edit", "return total", "return total + 1"),
    "generics": ("generic_call_edit", 'f0000("Ada")', 'f0000("four")'),
    "captures": ("capture_call_edit", "read(7)", "read(8)"),
    "aliases": ("alias_leaf_edit", "typealias A0 = Int", "typealias A0 = Float"),
    "alias-scopes": ("alias_argument_edit", "A2048<Int>", "A2048<Float>"),
    "normalization-scopes": ("consumer_edit", "f1899(4)", "f1899(5)"),
    "generic-name-scopes": ("generic_consumer_edit", "f1899(4, 0)", "f1899(5, 0)"),
    "capture-initializers": ("captured_store_edit", "x1899 = 1", "x1899 = 2"),
    "capture-readers": ("captured_reader_edit", "x1899 = 1", "x1899 = 2"),
    "shared-writes": ("shared_store_edit", "x4089 += 1", "x4089 += 2"),
    "shared-calls": ("shared_result_edit", "value1899: Int = f1899(false)", "value1899: Int = 2"),
    "shared-dependencies": ("shared_dependency_edit", "value1499: Int = read()", "value1499: Int = read() + 1"),
    "callback-initializers": ("callback_value_edit", "cb4089 = () => 4089", "cb4089 = () => 4090"),
}[options.workload]
assert before in original
source.write_text(original)
output = root / "startup.txt"
command = [str(options.compiler.resolve()), "build", str(root), "--out", str(output)]
report = {
    "workload": options.workload,
    "source_bytes": len(original.encode()),
    "source_lines": len(original.splitlines()),
    "functions": {
        "single-use-vals": 1028,
        "scoped-vals": 2053,
        "parameter-vals": 2053,
        "structured-vals": 2053,
        "generic-vals": 2053,
        "returning-vals": 2054,
        "expression-vals": 2054,
        "default-vals": 2055,
        "assignment-vals": 2054,
        "indexed-vals": 2054,
        "compound-vals": 2054,
        "operand-vals": 2054,
        "prose-vals": 2054,
        "forward-vals": 2053,
        "grouped-forward-vals": 2053,
        "unit-forward-vals": 2054,
        "frame-forward-vals": 2053,
        "mutable-forward-vals": 2053,
        "repeated-forward-vals": 2053,
        "loop-forward-vals": 2053,
        "range-forward-vals": 2053,
        "stored-range-vals": 2053,
        "immutable-list-vals": 2053,
        "exclusive-forward-vals": 2053,
        "loop-exit-vals": 2053,
        "projected-inputs": 9,
        "generic-parameters": 6,
        "function-parameters": 5,
        "inferred-function-tables": 16384,
        "function-tables": 16384,
        "assignment-facts": 1,
        "path-joins": 1,
        "loop-breaks": 1,
        "continue-facts": 1,
        "continue-branches": 1,
        "conditional-chains": 4,
        "forward-aliases": 1,
        "calling-defaults": 7,
        "named-constructors": 3,
        "method-bodies": 1027,
        "nominals": 515,
        "fields": 67,
        "mutable-fields": 67,
        "indexes": 65,
        "mutable-indexes": 65,
        "computed-indexes": 65,
        "text-computed-indexes": 65,
        "compound-indexes": 66,
        "captures": 4092,
        "aliases": 1,
        "alias-scopes": 1,
        "capture-initializers": 2,
        "capture-readers": 2,
        "callback-initializers": 4092,
        "shared-writes": 4092,
        "shared-calls": 3802,
        "shared-dependencies": 1631,
        "normalization-scopes": 1901,
        "generic-name-scopes": 1901,
    }.get(options.workload, 4096),
    "compiler_sha256": checksum(options.compiler),
    "runs": {},
}
if options.workload in [
    "assignment-facts",
    "path-joins",
    "loop-breaks",
    "continue-facts",
    "continue-branches",
]:
    report["facts"] = 4000
    report["branches"] = 1024 if options.workload == "continue-branches" else 128
if options.workload == "generic-parameters":
    report["generic_parameters"] = 512
    report["fields"] = 512
if options.workload == "function-parameters":
    report["parameters"] = 2048
    report["calls"] = 3
if options.workload == "conditional-chains":
    report["branches"] = 8192
if options.workload == "forward-aliases":
    report["aliases"] = 32768
if options.workload == "calling-defaults":
    report["fields"] = 2048
    report["default_calls"] = 2048
if options.workload == "named-constructors":
    report["fields"] = 2048
    report["constructor_calls"] = 16
    report["named_arguments"] = 32768
if options.workload == "method-bodies":
    report["declared_methods"] = 512
    report["parameters_per_method"] = 16
    report["method_local_closures"] = 512
if nominal or options.workload in ["fields", "mutable-fields"]:
    report["fields"] = 768 if nominal else 32
if options.workload == "compound-indexes":
    report["compound_updates"] = 2048
if options.workload == "text-computed-indexes":
    report["lookup_guards"] = 1536
if options.workload in ["indexes", "mutable-indexes", "computed-indexes"]:
    report["lookup_guards"] = 2048
if options.workload == "single-use-vals":
    report["single_use_initializers"] = 1024
if options.workload == "projected-inputs":
    report["projected_call_sites"] = 1024
if options.workload == "generic-name-scopes":
    report["visible_aliases"] = 1901
    report["local_aliases"] = 1900
    report["branch_guards"] = 3800
if options.workload == "normalization-scopes":
    report["alias_count"] = 1901
    report["branch_guards"] = 3800
if options.workload == "aliases":
    report["alias_levels"] = 28
if options.workload == "alias-scopes":
    report["alias_count"] = 2049
if options.workload in ["capture-initializers", "capture-readers"]:
    report["initialized_captures"] = 1900
if options.workload == "callback-initializers":
    report["initialized_callbacks"] = 4090
if options.workload == "shared-writes":
    report["updated_globals"] = 4090
if options.workload == "shared-calls":
    report["shared_initializers"] = 1900
    report["helper_chain"] = 1900
if options.workload == "shared-dependencies":
    report["shared_seeds"] = 128
    report["shared_consumers"] = 1500
    report["dependency_fanout"] = 128
checksums = {}
for name in ["populate", "unchanged", edit_name]:
    if name == edit_name:
        source.write_text(original.replace(before, after, 1))
    result = measure(command)
    result["source_sha256"] = checksum(source)
    report["runs"][name] = result
    if result["status"]:
        break
    checksums[name] = checksum(output)
    result["program_sha256"] = checksums[name]
    if options.keep_original and name == "populate":
        shutil.copyfile(output, root / "original.startup.txt")
report["unchanged_output"] = "populate" in checksums and checksums.get("populate") == checksums.get("unchanged")
report["edited_output_changed"] = (
    edit_name in checksums and checksums[edit_name] != checksums.get("populate")
)
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for name, result in report["runs"].items():
    expected = "0 compiled, 1 reused" if name == "unchanged" else "1 compiled, 0 reused"
    if result["status"] or expected not in result["stdout"]:
        raise SystemExit("unexpected build or cache result")
    if result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit("workload exceeded the memory contract")
    if name != "populate" and result["wall_seconds"] >= 10:
        raise SystemExit("incremental build exceeded ten seconds")
if not report["unchanged_output"] or not report["edited_output_changed"]:
    raise SystemExit("build output did not reflect validated source changes")
