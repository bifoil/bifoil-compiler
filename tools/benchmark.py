"""Generate and clean-compile original bounded compiler workloads on Linux.

Usage: uv run --no-project tools/benchmark.py COMPILER OUTPUT_DIRECTORY
Optional --baseline runs an earlier executable first and requires identical output.
Generated projects and reports belong under ignored .bifoil/development/.
"""

import argparse
import hashlib
import json
import platform
from pathlib import Path

from process_measurement import measure
from workload_source import program_source


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(65536):
            digest.update(block)
    return digest.hexdigest()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("compiler", type=Path)
parser.add_argument("output_directory", type=Path)
parser.add_argument("--baseline", type=Path)
parser.add_argument(
    "--workload",
    choices=[
        "functions",
        "branches",
        "shared",
        "nominals",
        "inference",
        "types",
        "generics",
    ],
    default="functions",
)
options = parser.parse_args()
root = options.output_directory.resolve()
(root / "src").mkdir(parents=True, exist_ok=True)
(root / "bifoil.toml").write_text('entry = "main.start"\n')
source = program_source(options.workload)
(root / "src/main.bfl").write_text(source)
output = root / "startup.txt"
compilers = [("current", options.compiler)]
if options.baseline:
    compilers.insert(0, ("baseline", options.baseline))
runs = {}
for name, compiler in compilers:
    output.unlink(missing_ok=True)
    result = measure(
        [str(compiler.resolve()), "build", str(root), "--out", str(output), "--clean"]
    )
    result["compiler_sha256"] = checksum(compiler)
    if result["status"] == 0:
        result["output_sha256"] = checksum(output)
    runs[name] = result

# Keep artifact decoding outside compiler timing and memory observation.
report = {
    "workload": options.workload,
    "profile": None,
    "platform": platform.system() + " " + platform.machine(),
    "source_bytes": len(source.encode()),
    "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
    **runs["current"],
}
if "baseline" in runs:
    report["baseline"] = runs["baseline"]
    report["output_matches_baseline"] = (
        runs["current"].get("output_sha256") == runs["baseline"].get("output_sha256")
        and "output_sha256" in runs["current"]
    )
if output.exists():
    program = json.loads(
        next(
            line[8:]
            for line in output.read_text().splitlines()
            if line.startswith("*bifoil ")
        )
    )
    report.update(
        profile=program["profile"],
        functions=len(program["functions"]),
        instructions=sum(len(fn["code"]) for fn in program["functions"].values()),
        output_bytes=output.stat().st_size,
    )
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
if any(
    run["status"] or run["compiler_peak_rss_bytes"] >= 250_000_000
    for run in runs.values()
):
    raise SystemExit(1)
if options.baseline and not report["output_matches_baseline"]:
    raise SystemExit("comparison compilers produced different program bytes")
