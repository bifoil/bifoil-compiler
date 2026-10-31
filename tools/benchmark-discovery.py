"""Measure source retention along a deep acyclic import chain.

Usage: uv run --no-project tools/benchmark-discovery.py COMPILER OUTPUT_DIRECTORY
An optional --baseline executable runs the identical project before the current one.
The destination must not exist; generated files belong under .bifoil/development/.
"""

import argparse
import hashlib
import json
from pathlib import Path

from process_measurement import measure


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(65536):
            digest.update(block)
    return digest.hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--baseline", type=Path)
options = arguments.parse_args()
root = options.output_directory.resolve()
(root / "src").mkdir(parents=True)
(root / "bifoil.toml").write_text('entry = "level000.start"\n')
module_count = 60
payload = "original narrative " * 12000
source_bytes = 0
for index in range(module_count):
    dependency = (
        f"import level{index + 1:03d}.payload as nextPayload\n"
        if index + 1 < module_count
        else ""
    )
    source = dependency + f'export fun payload() -> String = "{payload}"\n'
    if index == 0:
        source += "scene start():\n    return\n"
    source_bytes += len(source.encode())
    (root / f"src/level{index:03d}.bfl").write_text(source)
output = root / "startup.txt"
report = {"modules": module_count, "source_bytes": source_bytes, "runs": {}}
compilers = [("current", options.compiler)]
if options.baseline:
    compilers.insert(0, ("baseline", options.baseline))
for name, compiler in compilers:
    output.unlink(missing_ok=True)
    result = measure(
        [str(compiler.resolve()), "build", str(root), "--clean", "--out", str(output)]
    )
    if result["status"] == 0:
        result["output_sha256"] = checksum(output)
        result["output_bytes"] = output.stat().st_size
    report["runs"][name] = result
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
if any(run["status"] for run in report["runs"].values()):
    raise SystemExit("dependency-depth compilation failed")
if len({run["output_sha256"] for run in report["runs"].values()}) != 1:
    raise SystemExit("dependency-depth compilers produced different program bytes")
if any(
    run["compiler_peak_rss_bytes"] >= 250_000_000 for run in report["runs"].values()
):
    raise SystemExit("dependency-depth workload exceeded the memory contract")
