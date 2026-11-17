"""Compare cyclic builds sequentially, preserving raw results and exact outputs.

Each compiler gets a new project per workload and output mode. The observer
retains failures and checks its inputs again before declaring a comparison valid.
Peak RSS is compiler-process evidence, not aggregate job memory enforcement.
"""

import argparse
import filecmp
import hashlib
import json
import subprocess
import sys
from pathlib import Path

WORKLOADS = {
    "control": ["--modules", "2", "--functions", "1024"],
    "bodies": ["--modules", "8", "--functions", "4096"],
    "package-bodies": ["--modules", "8", "--functions", "8192"],
    "sparse": ["--modules", "64", "--functions", "1024"],
    "membership": [
        "--modules", "256", "--functions", "257", "--name-padding", "160"
    ],
}
MODES = ["inline", "package"]
# The larger body graph exceeds the current single-file inline output guard.
WORKLOAD_MODES = {"package-bodies": ["package"]}


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def artifacts(project):
    return {path.name: path for path in project.glob("*.txt")}


def main():
    arguments = argparse.ArgumentParser(description=__doc__)
    arguments.add_argument("--before", required=True, type=Path)
    arguments.add_argument("--after", required=True, type=Path)
    arguments.add_argument("--output", required=True, type=Path)
    arguments.add_argument(
        "--workloads", nargs="+", choices=WORKLOADS, default=list(WORKLOADS)
    )
    options = arguments.parse_args()
    compilers = {"before": options.before.resolve(), "after": options.after.resolve()}
    if not all(path.is_file() for path in compilers.values()):
        arguments.error("both compiler paths must name existing files")
    if len(set(options.workloads)) != len(options.workloads):
        arguments.error("workloads must not repeat")
    output = options.output.resolve()
    if output.exists():
        arguments.error("use a new output directory")
    output.mkdir(parents=True)
    tools = Path(__file__).resolve().parent
    observers = [
        "compare-cyclic.py",
        "benchmark-cyclic.py",
        "cyclic_workload.py",
        "process_measurement.py",
    ]
    report = {
        "compiler_sha256": {name: checksum(path) for name, path in compilers.items()},
        "observer_sha256": {name: checksum(tools / name) for name in observers},
        "workloads": {},
    }

    def save():
        (output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")

    successful = True
    completed = False
    save()
    try:
        for name in options.workloads:
            for mode in WORKLOAD_MODES.get(name, MODES):
                rows = report["workloads"][name + "-" + mode] = {}
                for revision, compiler in compilers.items():
                    project = output / f"{name}-{mode}-{revision}"
                    log = output / f"{name}-{mode}-{revision}.log"
                    command = [
                        sys.executable,
                        str(tools / "benchmark-cyclic.py"),
                        str(compiler),
                        str(project),
                        *WORKLOADS[name],
                        "--keep-original",
                    ]
                    if mode == "package":
                        command.append("--package")
                    with log.open("x") as stream:
                        child = subprocess.run(
                            command, stdout=stream, stderr=subprocess.STDOUT
                        )
                    measurement = project / "measurement.json"
                    rows[revision] = {
                        "command": command,
                        "observer_status": child.returncode,
                        "log": log.name,
                        "measurement": (
                            json.loads(measurement.read_text())
                            if measurement.is_file()
                            else None
                        ),
                    }
                    save()
                before = output / f"{name}-{mode}-before"
                after = output / f"{name}-{mode}-after"
                left, right = artifacts(before), artifacts(after)
                rows["exact_artifact_bytes"] = (
                    bool(left)
                    and left.keys() == right.keys()
                    and all(
                        filecmp.cmp(path, right[key], shallow=False)
                        for key, path in left.items()
                    )
                )
                rows["artifact_sha256"] = {
                    revision: {key: checksum(path) for key, path in paths.items()}
                    for revision, paths in [("before", left), ("after", right)]
                }
                rows["exact_source_bytes"] = {
                    path.name: checksum(path) for path in (before / "src").glob("*.bfl")
                } == {
                    path.name: checksum(path) for path in (after / "src").glob("*.bfl")
                }
                rows["passed"] = (
                    all(rows[revision]["observer_status"] == 0 for revision in compilers)
                    and rows["exact_artifact_bytes"]
                    and rows["exact_source_bytes"]
                )
                successful &= rows["passed"]
                save()
                print(f"{name} {mode}: {'PASS' if rows['passed'] else 'FAIL'}", flush=True)
        completed = True
    finally:
        report["compilers_unchanged"] = all(
            checksum(path) == report["compiler_sha256"][name]
            for name, path in compilers.items()
        )
        report["observers_unchanged"] = all(
            checksum(tools / name) == report["observer_sha256"][name]
            for name in observers
        )
        report["successful"] = (
            completed
            and successful
            and report["compilers_unchanged"]
            and report["observers_unchanged"]
        )
        save()
    return 0 if report["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
