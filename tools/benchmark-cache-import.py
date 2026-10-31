"""Measure incremental edits with large dependencies or local nominal metadata.

Usage: uv run --no-project tools/benchmark-cache-import.py COMPILER NEW_OUTPUT_DIRECTORY
Generated projects and raw measurements belong under ignored .bifoil/development/.
"""

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from process_measurement import measure
from workload_source import nominal_consumer_sources, nominal_import_sources, program_source, shared_import_sources


def checksum(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--workload", choices=["functions", "nominal-import", "shared-import", "nominal-consumer"], default="functions")
arguments.add_argument("--keep-original", action="store_true", help="retain the clean program for independent execution")
options = arguments.parse_args()
root = options.output_directory.resolve()
root.mkdir(parents=True, exist_ok=False)
(root / "src").mkdir()
(root / "bifoil.toml").write_text('entry = "main.start"\n')
if options.workload == "nominal-consumer":
    helpers, entry = nominal_consumer_sources()
    functions = 1029  # Local methods/constructor/copy/scene and two shared-state functions.
elif options.workload == "shared-import":
    helpers, entry = shared_import_sources()
    functions = 4093  # 4,090 initializers, a bootstrap, a reader, and the scene.
elif options.workload == "nominal-import":
    helpers, entry = nominal_import_sources()
    functions = 1027  # 1,024 methods, a constructor, a copy method, and the scene.
else:
    helpers = "export " + program_source("functions").split("scene start():", 1)[0]
    assert helpers.count("fun f") == 4095
    entry = 'import helpers.f0000\nscene start():\n    prose:\n        ${f0000(0)}|END\n'
    functions = 4096
dependency = root / "src/helpers.bfl"
dependency.write_text(helpers)
scene = root / "src/main.bfl"
scene.write_text(entry)
output = root / "startup.txt"
command = [str(options.compiler.resolve()), "build", str(root), "--out", str(output)]
report = {
    "modules": 2,
    "workload": options.workload,
    "functions": functions,
    "compiler_sha256": checksum(options.compiler),
    "source_bytes": sum(path.stat().st_size for path in (root / "src").glob("*.bfl")),
    "runs": {},
}
if options.workload == "shared-import":
    report["shared_bindings"] = 4090
if options.workload in ["nominal-import", "nominal-consumer"]:
    report["declared_methods"] = 1024
    report["parameters_per_method"] = 32
dependency_edit = options.workload == "nominal-consumer"
edit_name = "dependency_edit" if dependency_edit else "scene_edit"
for name in ["clean", "populate", "unchanged", edit_name]:
    if name == edit_name:
        if dependency_edit:
            dependency.write_text(dependency.read_text().replace("Int = 0", "Int = 1", 1))
        else:
            scene.write_text(scene.read_text().replace("|END", "|FIN", 1))
    result = measure([*command, "--clean"] if name == "clean" else command)
    result["source_sha256"] = {path.name: checksum(path) for path in sorted((root / "src").glob("*.bfl"))}
    if result["status"] == 0:
        result["program_sha256"] = checksum(output)
        if options.keep_original and name == "clean":
            shutil.copyfile(output, root / "original.startup.txt")
    report["runs"][name] = result
    if result["status"]:
        break
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
if len(report["runs"]) != 4:
    raise SystemExit("the dependency-reuse workload did not finish")
for name, result in report["runs"].items():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit("build failed or exceeded the memory contract")
    if name in ["unchanged", edit_name] and result["wall_seconds"] >= 10:
        raise SystemExit("incremental build exceeded ten seconds")
expected = report["runs"]["clean"]["program_sha256"]
if any(report["runs"][name]["program_sha256"] != expected for name in ["populate", "unchanged"]):
    raise SystemExit("source population or unchanged reuse changed program bytes")
if report["runs"][edit_name]["program_sha256"] == expected:
    raise SystemExit("the edited scene did not change program bytes")
if "0 compiled, 2 reused" not in report["runs"]["unchanged"]["stdout"]:
    raise SystemExit("unchanged build did not reuse both modules")
expected_reuse = "2 compiled, 0 reused" if dependency_edit else "1 compiled, 1 reused"
if expected_reuse not in report["runs"][edit_name]["stdout"]:
    raise SystemExit("edited build did not follow its expected dependency invalidation")
if dependency_edit:
    before = report["runs"]["populate"]["source_sha256"]["main.bfl"]
    after = report["runs"][edit_name]["source_sha256"]["main.bfl"]
    if before != after:
        raise SystemExit("dependency invalidation workload changed its consumer source")
