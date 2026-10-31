"""Measure native compilation of one large literal and a same-size content edit.

Usage: uv run --no-project tools/benchmark-text.py COMPILER OUTPUT_DIRECTORY
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


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--alphabet", choices=["ascii", "emoji"], default="ascii")
arguments.add_argument("--literal-bytes", type=int, default=900000)
options = arguments.parse_args()
if not 8 <= options.literal_bytes <= 1048000:
    arguments.error("literal bytes must be between 8 and the current source-file limit minus header space")
character = "a" if options.alphabet == "ascii" else "😀"
width = len(character.encode())
if (options.literal_bytes - 8) % width:
    arguments.error("literal byte count must fit the prefix, suffix, and selected scalar width")
payload = "START" + character * ((options.literal_bytes - 8) // width) + "END"
root = options.output_directory.resolve()
root.mkdir(parents=True)
(root / "src").mkdir()
(root / "bifoil.toml").write_text('entry = "main.start"\n')
source = root / "src/main.bfl"
source.write_text(
    f'shared val payload = "{payload}"\n'
    + "scene start():\n    prose:\n"
    + "        ${payload.length}|${payload.substring(0, 5)}|"
    + "${payload.substring(payload.length - 3, payload.length)}\n"
)
del payload
output = root / "startup.txt"
command = [str(options.compiler.resolve()), "build", str(root), "--out", str(output)]
report = {
    "alphabet": options.alphabet,
    "literal_bytes": options.literal_bytes,
    "literal_scalars": 8 + (options.literal_bytes - 8) // width,
    "source_bytes": source.stat().st_size,
    "compiler_sha256": checksum(options.compiler),
    "runs": {},
}
for phase in ["clean", "populate", "unchanged", "literal_edit"]:
    if phase == "literal_edit":
        original = source.read_text()
        assert original.count('END"') == 1
        source.write_text(original.replace('END"', 'FIN"', 1))
        del original
    result = measure([*command, "--clean"] if phase == "clean" else command)
    report["runs"][phase] = result
    if result["status"]:
        break
    result["output_sha256"] = checksum(output)
(root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
for phase, result in report["runs"].items():
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise SystemExit("literal workload failed or exceeded the memory contract")
    if phase in ["unchanged", "literal_edit"] and result["wall_seconds"] >= 10:
        raise SystemExit("incremental literal workload exceeded ten seconds")
    expected = "0 compiled, 1 reused" if phase == "unchanged" else "1 compiled, 0 reused"
    if phase != "clean" and expected not in result["stdout"]:
        raise SystemExit("unexpected literal workload cache result")
runs = report["runs"]
if len({runs[phase]["output_sha256"] for phase in ["clean", "populate", "unchanged"]}) != 1:
    raise SystemExit("unchanged literal builds produced different output")
if runs["literal_edit"]["output_sha256"] == runs["unchanged"]["output_sha256"]:
    raise SystemExit("literal edit did not change the output")
