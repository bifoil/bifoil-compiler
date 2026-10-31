"""Measure module reuse and a leaf edit, with separate per-process Linux peak RSS.

Usage: uv run --no-project tools/benchmark-incremental.py COMPILER OUTPUT_DIRECTORY
Generated inputs and reports belong under ignored .bifoil/development/.
"""

import argparse
import json
from pathlib import Path

from process_measurement import measure

arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
options = arguments.parse_args()
root = options.output_directory.resolve()
source = root / "src"
source.mkdir(parents=True, exist_ok=True)
(root / "bifoil.toml").write_text('entry = "main.start"\n')
for module in range(32):
    (source / f"part{module:02}.bfl").write_text(
        "".join(
            f"export fun f{number:03}(value: Int) -> Int = value + {number}\n"
            for number in range(100)
        )
    )
(source / "main.bfl").write_text(
    "".join(f"import part{module:02}.f000 as p{module:02}\n" for module in range(32))
    + "scene start():\n    prose:\n        ${p00(0)}\n"
)
command = [
    str(options.compiler.resolve()),
    "build",
    str(root),
    "--out",
    str(root / "startup.txt"),
]
report = {"modules": 33, "functions": 3201, "runs": {}}
report["runs"]["clean"] = measure([*command, "--clean"])
report["runs"]["populate"] = measure(command)
report["runs"]["unchanged"] = measure(command)
path = source / "part00.bfl"
path.write_text(path.read_text().replace("value + 0\n", "value + 1\n", 1))
report["runs"]["leaf_edit"] = measure(command)
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for name, result in report["runs"].items():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit(1)
    if name in ["unchanged", "leaf_edit"] and result["wall_seconds"] >= 10:
        raise SystemExit(1)
if "0 compiled, 33 reused" not in report["runs"]["unchanged"]["stdout"]:
    raise SystemExit("unchanged build did not reuse every module")
if "2 compiled, 31 reused" not in report["runs"]["leaf_edit"]["stdout"]:
    raise SystemExit("leaf edit did not rebuild exactly the leaf and its client")
