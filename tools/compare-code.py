"""Compare compiler resources and exact artifacts using isolated generated projects.

Each workload runs sequentially. The output directory must be new; reports and
failed observations remain available even when a compiler exceeds its budget.
"""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

WORKLOADS = {
    "control": ["--workload", "function-graph", "--modules", "8", "--routines", "2000"],
    "code": [],
    "suspend": ["--suspend-routines"],
    "large": ["--workload", "function-graph", "--modules", "10", "--routines", "8190"],
    "private": ["--workload", "function-graph", "--modules", "10", "--routines", "8190", "--private-routines"],
}
BUFFER_BYTES = 65536


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while data := source.read(BUFFER_BYTES):
            digest.update(data)
    return digest.hexdigest()


def artifact_paths(project):
    return {
        path.relative_to(project)
        for pattern in ("*.txt", "original/*.txt", "modules/**/*.bfm.json")
        for path in project.glob(pattern)
    }


def equal_bytes(first, second):
    with first.open("rb") as left, second.open("rb") as right:
        while True:
            a, b = left.read(BUFFER_BYTES), right.read(BUFFER_BYTES)
            if a != b:
                return False
            if not a:
                return True


def main():
    arguments = argparse.ArgumentParser(description=__doc__)
    arguments.add_argument("--before", required=True, type=Path)
    arguments.add_argument("--after", required=True, type=Path)
    arguments.add_argument("--output", required=True, type=Path)
    arguments.add_argument("--workloads", nargs="+", choices=WORKLOADS, default=["control", "code", "large", "private"])
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
    observers = ["compare-code.py", "benchmark-code.py", "function_workload.py", "code_workload.py", "process_measurement.py"]
    report = {
        "observer_sources_sha256": {name: checksum(tools / name) for name in observers},
        "compiler_sha256": {name: checksum(path) for name, path in compilers.items()},
        "workloads": {},
    }

    def save():
        (output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")

    benchmark = Path(__file__).with_name("benchmark-code.py").resolve()
    successful = True
    save()
    try:
        for name in options.workloads:
            rows = report["workloads"][name] = {}
            for revision, compiler in compilers.items():
                project = output / f"{name}-{revision}"
                with (output / f"{name}-{revision}.log").open("w") as log:
                    result = subprocess.run(
                        [sys.executable, str(benchmark), str(compiler), str(project), *WORKLOADS[name]],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                    )
                measurement = project / "measurement.json"
                row = json.loads(measurement.read_text()) if measurement.is_file() else {}
                row["observer_status"] = result.returncode
                rows[revision] = row
                successful &= result.returncode == 0
                save()
                print(f"{name} {revision}: status {result.returncode}", flush=True)
            if any(rows[revision]["observer_status"] for revision in compilers):
                continue
            before, after = (output / f"{name}-{revision}" for revision in compilers)
            paths = artifact_paths(before)
            if paths != artifact_paths(after) or not all(equal_bytes(before / path, after / path) for path in paths):
                rows["exact_artifact_bytes"] = False
                successful = False
            else:
                rows["exact_artifact_bytes"] = True
                rows["compared_artifacts"] = len(paths)
            save()
        report["compilers_unchanged"] = all(
            checksum(path) == report["compiler_sha256"][name] for name, path in compilers.items()
        )
        report["observers_unchanged"] = all(
            checksum(tools / name) == report["observer_sources_sha256"][name] for name in observers
        )
        successful &= report["compilers_unchanged"] and report["observers_unchanged"]
        report["successful"] = successful
    finally:
        save()
    return 0 if successful else 1


if __name__ == "__main__":
    raise SystemExit(main())
