"""Measure cache population and a body edit in a large cyclic import component.

Usage: uv run --no-project tools/benchmark-cyclic.py COMPILER OUTPUT_DIRECTORY
The destination must not exist. Generated files belong under .bifoil/development/.
"""

import argparse
import hashlib
import json
from pathlib import Path

from process_measurement import measure


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(65536):
            digest.update(block)
    return digest.hexdigest()


def module_source(peer, count, entry=False):
    expression = "value" + " + 1" * 10
    source = f"import {peer}.f0000 as peerFirst\n"
    source += "".join(
        ("export " if number == 0 else "")
        + f"fun f{number:04d}(value: Int) -> Int = {expression}\n"
        for number in range(count)
    )
    if entry:
        source += "scene start():\n    prose:\n        ${f0000(0)}|${peerFirst(0)}\n"
    return source


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--functions", type=int, default=4096)
options = arguments.parse_args()
if not 3 <= options.functions <= 4096:
    arguments.error("functions must be between 3 and the current 4,096-function limit")
root = options.output_directory.resolve()
root.mkdir(parents=True)
(root / "src").mkdir()
(root / "bifoil.toml").write_text('entry = "alpha.start"\n')
alpha_count = (options.functions - 1) // 2
beta_count = options.functions - 1 - alpha_count
alpha = root / "src/alpha.bfl"
alpha.write_text(module_source("beta", alpha_count, entry=True))
(root / "src/beta.bfl").write_text(module_source("alpha", beta_count))
output = root / "startup.txt"
command = [str(options.compiler.resolve()), "build", str(root), "--out", str(output)]
report = {
    "modules": 2,
    "functions": options.functions,
    "compiler_sha256": checksum(options.compiler),
    "source_bytes": sum(path.stat().st_size for path in (root / "src").glob("*.bfl")),
    "runs": {},
}
for phase in ["clean", "populate", "unchanged", "member_edit"]:
    if phase == "member_edit":
        source = alpha.read_text()
        assert "value + 1" in source
        alpha.write_text(source.replace("value + 1", "value + 2", 1))
    result = measure([*command, "--clean"] if phase == "clean" else command)
    report["runs"][phase] = result
    if result["status"]:
        break
    result["output_sha256"] = checksum(output)
    result["output_bytes"] = output.stat().st_size
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for phase, result in report["runs"].items():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit("cyclic component compilation failed or exceeded the memory contract")
    if phase in ["unchanged", "member_edit"] and result["wall_seconds"] >= 10:
        raise SystemExit("incremental cyclic component build exceeded ten seconds")
    expected = "0 compiled, 2 reused" if phase == "unchanged" else "2 compiled, 0 reused"
    if phase != "clean" and expected not in result["stdout"]:
        raise SystemExit("unexpected cyclic group cache result")
runs = report["runs"]
if len({runs[phase]["output_sha256"] for phase in ["clean", "populate", "unchanged"]}) != 1:
    raise SystemExit("unchanged cyclic builds produced different output")
if runs["member_edit"]["output_sha256"] == runs["unchanged"]["output_sha256"]:
    raise SystemExit("cyclic member edit did not change the output")
