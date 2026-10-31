"""Measure a scene-module graph, source-free linking, and cached package output.

Use a new ignored output directory. Every declared scene is visited; source and
artifact files are retained for an independent runtime/checkpoint verification.
"""

import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path

from module_workload import write_scene_project
from process_measurement import measure


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("compiler", type=Path)
arguments.add_argument("output_directory", type=Path)
arguments.add_argument("--scenes", type=int, default=510)
options = arguments.parse_args()
if options.scenes < 1:
    arguments.error("--scenes must be positive")
root = options.output_directory.resolve()
if root.exists() and any(root.iterdir()):
    arguments.error("the workload output directory must be empty")
compiler = options.compiler.resolve()
sources = write_scene_project(root, options.scenes)
report = {
    "compiler_sha256": checksum(compiler),
    "scenes": options.scenes,
    "modules": len(sources),
    "source_bytes": sum(len(text.encode()) for text in sources.values()),
    "source_lines": sum(len(text.splitlines()) for text in sources.values()),
    "runs": {},
    "publication": {},
}
output = root / "startup.txt"
source = root / "src/rooms" / f"r{options.scenes - 1:04d}.bfl"
command = [str(compiler), "build", str(root), "--out", str(output)]


def observe(label, command):
    result = measure(command)
    report["runs"][label] = result
    print(label, result["status"], round(result["wall_seconds"], 3), flush=True)
    if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
        raise RuntimeError(label + ": " + result["stderr"])
    return result


try:
    for label in ["populate", "unchanged", "last_scene_edit"]:
        if label == "last_scene_edit":
            before = source.read_text()
            old = f"total += {options.scenes - 1}\n"
            new = f"total += {options.scenes}\n"
            assert before.count(old) == 1
            metadata = source.stat()
            after = before.replace(old, new, 1)
            source.write_text(after)
            os.utime(source, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))
            report["edit_preserved_size"] = len(before.encode()) == len(after.encode())
            report["edit_preserved_mtime"] = source.stat().st_mtime_ns == metadata.st_mtime_ns
        result = observe(label, command)
        result["program_sha256"] = checksum(output)
        if label == "populate":
            shutil.copyfile(output, root / "original.startup.txt")
        elif result["wall_seconds"] >= 10:
            raise RuntimeError(label + " exceeded ten seconds")
    runs = report["runs"]
    assert runs["populate"]["program_sha256"] == runs["unchanged"]["program_sha256"]
    assert runs["populate"]["program_sha256"] != runs["last_scene_edit"]["program_sha256"]
    modules = root / "modules"
    modules.mkdir()
    order = ["stats", *[f"rooms.r{index:04d}" for index in range(options.scenes)], "main"]
    for index, module in enumerate(order):
        path = root / "src" / (module.replace(".", "/") + ".bfl")
        artifact = modules / (module + ".bfm.json")
        result = measure([
            str(compiler), "module", str(path), "--name", module,
            "--module-path", str(modules), "--out", str(artifact),
        ])
        report["publication"][module] = result
        if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
            raise RuntimeError(module + ": " + result["stderr"])
        path.unlink()
        if (index + 1) % 64 == 0 or index + 1 == len(order):
            print("published", index + 1, "of", len(order), flush=True)
    source_free = root / "source-free.txt"
    observe("source_free", [str(compiler), "build", str(root), "--out", str(source_free), "--clean"])
    assert checksum(source_free) == checksum(output)
    packaged = root / "packaged/startup.txt"
    package_command = [str(compiler), "build", str(root), "--out", str(packaged), "--package"]
    observe("package_populate", [*package_command, "--clean"])
    populated = observe("package_cache", package_command)
    if populated["wall_seconds"] >= 10:
        raise RuntimeError("package cache population exceeded ten seconds")
    cached = observe("package_unchanged", package_command)
    if cached["wall_seconds"] >= 10:
        raise RuntimeError("unchanged package exceeded ten seconds")
    report["artifact_bytes"] = sum(path.stat().st_size for path in modules.glob("*.bfm.json"))
    report["compiler_unchanged"] = checksum(compiler) == report["compiler_sha256"]
    assert report["compiler_unchanged"]
finally:
    (root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
