"""Compare two compilers on scoped initialization workloads, one process at a time.

The output directory must be new. Reports retain failures, compiler identities,
source-observer identities, and exact control/source-free output comparisons.
"""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from process_measurement import measure
from workload_source import program_source

WORKLOADS = [
    "scoped-vals",
    "parameter-vals",
    "structured-vals",
    "generic-vals",
    "returning-vals",
    "expression-vals",
    "default-vals",
    "assignment-vals",
    "indexed-vals",
    "compound-vals",
    "operand-vals",
    "prose-vals",
    "forward-vals",
    "grouped-forward-vals",
    "unit-forward-vals",
    "frame-forward-vals",
    "mutable-forward-vals",
    "repeated-forward-vals",
    "loop-forward-vals",
    "range-forward-vals",
    "stored-range-vals",
    "immutable-list-vals",
    "exclusive-forward-vals",
    "loop-exit-vals",
]
TOOLS = Path(__file__).resolve().parent


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--control", required=True, choices=WORKLOADS)
    parser.add_argument("--workload", required=True, choices=WORKLOADS)
    parser.add_argument("--before-error", help="Expected diagnostic when the older compiler rejects the new form")
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if output.exists():
        parser.error("the output directory must not already exist")
    before = arguments.before.resolve()
    after = arguments.after.resolve()
    if not before.is_file() or not after.is_file():
        parser.error("both compiler executables must exist")
    output.mkdir(parents=True)
    observers = ["compare-typing.py", "benchmark-typing.py", "workload_source.py", "process_measurement.py"]
    report = {
        "before_sha256": checksum(before),
        "compiler_sha256": checksum(after),
        "observers": {name: checksum(TOOLS / name) for name in observers},
        "workloads": {},
        "successful": False,
    }

    def retain():
        (output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")

    def benchmark(name, compiler, workload):
        directory = output / name
        with (output / f"{name}.log").open("w") as log:
            result = subprocess.run(
                [
                    sys.executable, "-u", str(TOOLS / "benchmark-typing.py"), str(compiler),
                    str(directory), "--workload", workload, "--keep-original",
                ],
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        metadata = directory / "measurement.json"
        report["workloads"][name] = (
            json.loads(metadata.read_text()) if metadata.is_file() else {"status": result.returncode}
        )
        retain()
        if result.returncode:
            raise RuntimeError(f"{name} failed; see {output / (name + '.log')}")
        print(name + ": all native measurements passed", flush=True)

    def observe(command):
        result = measure(command)
        if result["compiler_peak_rss_bytes"] >= 250_000_000:
            result["memory_contract_exceeded"] = True
        return result

    def require_success(result):
        retain()
        if result["status"] or result.get("memory_contract_exceeded"):
            raise RuntimeError("compiler operation failed; see comparison.json")

    retain()
    benchmark("control-before", before, arguments.control)
    benchmark("control-after", after, arguments.control)
    report["exact_control_original_and_edited_bytes"] = all(
        (output / "control-before" / name).read_bytes() == (output / "control-after" / name).read_bytes()
        for name in ["original.startup.txt", "startup.txt"]
    )
    retain()
    if not report["exact_control_original_and_edited_bytes"]:
        raise RuntimeError("the compilers produced different control bytes")

    if arguments.before_error:
        rejected = output / "workload-before"
        (rejected / "src").mkdir(parents=True)
        (rejected / "bifoil.toml").write_text('entry = "main.start"\n')
        (rejected / "src/main.bfl").write_text(program_source(arguments.workload))
        result = observe([str(before), "build", str(rejected), "--out", str(rejected / "startup.txt")])
        report["previous_compiler_rejection"] = result
        retain()
        diagnostic = result["stdout"] + "\n" + result["stderr"]
        if (
            not result["status"]
            or result.get("memory_contract_exceeded")
            or arguments.before_error not in diagnostic
            or (rejected / "startup.txt").exists()
        ):
            raise RuntimeError("the older compiler did not produce the expected rejection")
    else:
        benchmark("workload-before", before, arguments.workload)

    benchmark("workload-after", after, arguments.workload)
    project = output / "workload-after"
    module = project / "modules/main.bfm.json"
    result = observe([
        str(after), "module", str(project / "src/main.bfl"), "--name", "main", "--out", str(module),
    ])
    report["module_publication"] = result
    require_success(result)
    (project / "src/main.bfl").unlink()
    report["source_free_links"] = {}
    for mode in ["inline", "package"]:
        result = observe([
            str(after), "build", str(project), "--clean", "--module-path", str(project / "modules"),
            "--out", str(project / f"source-free-{mode}.txt"), *(["--package"] if mode == "package" else []),
        ])
        report["source_free_links"][mode] = result
        require_success(result)
    report["exact_source_free_edited_inline_bytes"] = (
        (project / "source-free-inline.txt").read_bytes() == (project / "startup.txt").read_bytes()
    )
    report["functions"] = report["workloads"]["workload-after"]["functions"]
    report["compilers_unchanged"] = (
        checksum(before) == report["before_sha256"] and checksum(after) == report["compiler_sha256"]
    )
    report["observers_unchanged"] = all(
        checksum(TOOLS / name) == identity for name, identity in report["observers"].items()
    )
    report["successful"] = all(report[key] for key in [
        "exact_source_free_edited_inline_bytes", "compilers_unchanged", "observers_unchanged",
    ])
    retain()
    if not report["successful"]:
        raise RuntimeError("source-free output or executable/observer identities changed")
    print("Publication and source-free builds passed; control and source-free bytes matched.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        raise SystemExit(str(error)) from error
