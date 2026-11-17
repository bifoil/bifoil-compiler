"""Measure cache population and a body edit in a large cyclic import component.

Usage: uv run --no-project tools/benchmark-cyclic.py COMPILER OUTPUT_DIRECTORY
The destination must not exist. Generated files belong under .bifoil/development/.
"""

import argparse
import hashlib
import json
from pathlib import Path

from cyclic_workload import sources
from process_measurement import measure


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(65536):
            digest.update(block)
    return digest.hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--functions", type=int, default=4096)
arguments.add_argument("--modules", type=int, default=2)
arguments.add_argument("--name-padding", type=int, default=0)
arguments.add_argument("--package", action="store_true")
arguments.add_argument("--keep-original", action="store_true")
options = arguments.parse_args()
try:
    modules = sources(options.modules, options.functions, options.name_padding)
except ValueError as error:
    arguments.error(str(error))
root = options.output_directory.resolve()
root.mkdir(parents=True)
(root / "src").mkdir()
for name, source in modules.items():
    (root / "src" / (name + ".bfl")).write_text(source)
entry = next(iter(modules))
(root / "bifoil.toml").write_text(f'entry = "{entry}.start"\n')
alpha = root / "src" / (entry + ".bfl")
output = root / "startup.txt"
command = [str(options.compiler.resolve()), "build", str(root), "--out", str(output)]
if options.package:
    command.append("--package")
report = {
    "modules": options.modules,
    "name_padding": options.name_padding,
    "mode": "package" if options.package else "inline",
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
    if phase == "populate" and options.keep_original:
        (root / "original.startup.txt").write_bytes(output.read_bytes())
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for phase, result in report["runs"].items():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit(
            "cyclic component compilation failed or exceeded the memory contract"
        )
    if phase in ["unchanged", "member_edit"] and result["wall_seconds"] >= 10:
        raise SystemExit("incremental cyclic component build exceeded ten seconds")
    expected = (
        f"0 compiled, {options.modules} reused"
        if phase == "unchanged"
        else f"{options.modules} compiled, 0 reused"
    )
    if phase != "clean" and expected not in result["stdout"]:
        raise SystemExit("unexpected cyclic group cache result")
runs = report["runs"]
if (
    len({runs[phase]["output_sha256"] for phase in ["clean", "populate", "unchanged"]})
    != 1
):
    raise SystemExit("unchanged cyclic builds produced different output")
if runs["member_edit"]["output_sha256"] == runs["unchanged"]["output_sha256"]:
    raise SystemExit("cyclic member edit did not change the output")
