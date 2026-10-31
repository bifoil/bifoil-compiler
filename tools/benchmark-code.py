"""Measure native builds and source-free packages with many executable instructions.

Use a new ignored output directory. Every declared routine is referenced, called,
and retained through an input checkpoint by the generated original program.
"""

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from code_workload import write_code_project
from function_workload import write_function_project
from process_measurement import measure


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    arguments = argparse.ArgumentParser(description=__doc__)
    arguments.add_argument("compiler", type=Path)
    arguments.add_argument("output_directory", type=Path)
    arguments.add_argument("--workload", choices=["code", "function-graph"], default="code")
    arguments.add_argument("--modules", type=int)
    arguments.add_argument("--routines", type=int)
    arguments.add_argument("--additions", type=int)
    arguments.add_argument("--private-routines", action="store_true", help="keep generated graph routines private")
    arguments.add_argument("--suspend-routines", action="store_true", help="use suspending arithmetic routines")
    options = arguments.parse_args()
    graph = options.workload == "function-graph"
    if options.modules is None:
        options.modules = 10 if graph else 5
    if options.routines is None:
        options.routines = 8190 if graph else 3000
    if options.private_routines and not graph:
        raise ValueError("--private-routines applies to the function-graph workload")
    if graph and options.suspend_routines:
        raise ValueError("--suspend-routines applies to the code workload")
    if graph and options.additions is not None:
        raise ValueError("--additions applies to the code workload")
    if options.additions is None:
        options.additions = 13
    root = options.output_directory.resolve()
    if root.exists():
        raise ValueError("use a new output directory")
    compiler = options.compiler.resolve()
    sources = (
        write_function_project(root, options.modules, options.routines, export_routines=not options.private_routines)
        if graph else write_code_project(
            root, options.modules, options.routines, options.additions, suspending=options.suspend_routines
        )
    )
    report = {
        "workload": options.workload,
        "compiler_sha256": checksum(compiler),
        "source_bytes": sum(len(text.encode()) for text in sources.values()),
        "source_lines": sum(len(text.splitlines()) for text in sources.values()),
        "modules": len(sources),
        "routines": options.modules * options.routines,
        "runs": {},
        "publication": {},
    }

    if graph:
        report["exported_routines"] = not options.private_routines
    else:
        report["additions_per_routine"] = options.additions
        if options.suspend_routines:
            report["suspending_routines"] = True

    def observe(label, command, destination=None, incremental=False):
        result = measure(list(map(str, command)))
        destination = report["runs"] if destination is None else destination
        destination[label] = result
        if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
            raise RuntimeError(label + ": " + result["stderr"])
        if incremental and result["wall_seconds"] >= 10:
            raise RuntimeError(label + " exceeded ten seconds")
        if destination is report["runs"]:
            result["root_sha256"] = checksum(root / "startup.txt")
        return result

    output = root / "startup.txt"
    command = [compiler, "build", root, "--out", output, "--package"]
    try:
        observe("populate", command)
        original = root / "original"
        original.mkdir()
        for path in root.glob("*.txt"):
            shutil.copyfile(path, original / path.name)
        observe("unchanged", command, incremental=True)
        if report["runs"]["populate"]["root_sha256"] != report["runs"]["unchanged"]["root_sha256"]:
            raise RuntimeError("unchanged build altered the package root")
        last = options.modules * options.routines - 1
        if graph:
            leaf = root / "src/routines" / f"r{options.modules - 1:02d}.bfl"
            before = f"f{last:05d}() -> Int = {last}"
            after = f"f{last:05d}() -> Int = {last + 1}"
        else:
            leaf = root / "src/code" / f"m{options.modules - 1:02d}.bfl"
            before = f"= value + {last}"
            after = f"= value + {last + 1}"
        text = leaf.read_text()
        if text.count(before) != 1:
            raise RuntimeError("expected one editable routine")
        leaf.write_text(text.replace(before, after))
        observe("last_routine_edit", command, incremental=True)
        edited = checksum(output)
        if edited == report["runs"]["populate"]["root_sha256"]:
            raise RuntimeError("the routine edit did not change the package")
        modules = root / "modules"
        modules.mkdir()
        order = (
            ["stats", *[f"routines.r{index:02d}" for index in range(options.modules)], "main"]
            if graph else [*[f"code.m{index:02d}" for index in range(options.modules)], "main"]
        )
        for module in order:
            source = root / "src" / (module.replace(".", "/") + ".bfl")
            observe(
                module,
                [
                    compiler, "module", source, "--name", module, "--module-path", modules,
                    "--out", modules / (module + ".bfm.json"),
                ],
                destination=report["publication"],
            )
            source.unlink()
        observe("source_free", [*command, "--clean"])
        observe("source_free_cache", command, incremental=True)
        observe("source_free_unchanged", command, incremental=True)
        if any(
            report["runs"][label]["root_sha256"] != edited
            for label in ["source_free", "source_free_cache", "source_free_unchanged"]
        ):
            raise RuntimeError("source-free linking changed the edited package")
        report["exact_source_free_root_bytes"] = True
        report["artifact_bytes"] = sum(path.stat().st_size for path in modules.glob("*.bfm.json"))
        report["compiler_unchanged"] = checksum(compiler) == report["compiler_sha256"]
        if not report["compiler_unchanged"]:
            raise RuntimeError("compiler changed during measurement")
    finally:
        (root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
