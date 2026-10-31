"""Measure and compare source and source-free builds of the maximum-function workload.

Usage: uv run --no-project tools/benchmark-precompiled.py COMPILER OUTPUT_DIRECTORY
All generated files belong under ignored .bifoil/development/.
"""

import argparse
import hashlib
import json
from pathlib import Path

from process_measurement import measure
from workload_source import program_source


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(65536):
            digest.update(chunk)
    return digest.hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
options = arguments.parse_args()
root = options.output_directory.resolve()
(root / "src").mkdir(parents=True, exist_ok=True)
(root / "modules").mkdir(exist_ok=True)
(root / "bifoil.toml").write_text('entry = "main.start"\n')
source = root / "src/main.bfl"
source.write_text(program_source("functions"))
compiler = str(options.compiler.resolve())
output = root / "startup.txt"
artifact = root / "modules/main.bfm.json"
report = {"functions": 4096, "instructions": 98288, "runs": {}}
report["runs"]["source"] = measure(
    [compiler, "build", str(root), "--out", str(output), "--clean"]
)
if report["runs"]["source"]["status"]:
    raise SystemExit(report["runs"]["source"]["stderr"])
expected = checksum(output)
report["runs"]["module"] = measure(
    [compiler, "module", str(source), "--name", "main", "--out", str(artifact)]
)
if report["runs"]["module"]["status"]:
    raise SystemExit(report["runs"]["module"]["stderr"])
source.unlink()
output.unlink()
report["artifact_bytes"] = artifact.stat().st_size
report["runs"]["source_free"] = measure(
    [compiler, "build", str(root), "--out", str(output), "--clean"]
)
report["identical_output"] = output.is_file() and checksum(output) == expected
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for result in report["runs"].values():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit(1)
if not report["identical_output"]:
    raise SystemExit(
        "source-free compilation did not preserve the linked artifact bytes"
    )
