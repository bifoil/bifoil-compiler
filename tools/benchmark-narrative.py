"""Measure complete narrative projects and source-free checkpoint artifacts.

Every prose row contributes to rendered output. Use a new ignored directory;
retain its artifacts until an independent runtime verifier has checked them.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path

from narrative_workload import write_project
from process_measurement import measure


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("--scenes", type=int, default=67)
    parser.add_argument("--lines", type=int, default=100_000)
    options = parser.parse_args()
    if options.scenes < 1 or options.lines < 1:
        parser.error("--scenes and --lines must be positive")
    root = options.output_directory.resolve()
    if root.exists() and any(root.iterdir()):
        parser.error("the output directory must be empty")
    root.mkdir(parents=True, exist_ok=True)
    compiler = options.compiler.resolve()
    sources = write_project(root, options.scenes, options.lines)
    report = {
        "compiler_sha256": checksum(compiler),
        "scenes": options.scenes,
        "narrative_rows_per_scene": options.lines,
        "source_bytes": sum(len(text.encode()) for text in sources.values()),
        "source_lines": sum(len(text.splitlines()) for text in sources.values()),
        "maximum_source_bytes": max(len(text.encode()) for text in sources.values()),
        "runs": {},
        "publication": {},
    }

    def observe(label, command, incremental=False):
        result = measure(command)
        report["runs"][label] = result
        print(label, result["status"], round(result["wall_seconds"], 3), flush=True)
        if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
            raise RuntimeError(label + ": " + result["stderr"])
        if incremental and result["wall_seconds"] >= 10:
            raise RuntimeError(label + " exceeded ten seconds")
        return result

    output = root / "startup.txt"
    command = [str(compiler), "build", str(root), "--out", str(output)]
    last = root / "src/rooms" / f"r{options.scenes - 1:04d}.bfl"
    try:
        for label in ["populate", "unchanged", "last_paragraph_edit"]:
            if label == "last_paragraph_edit":
                before = last.read_text()
                old = f"end{options.scenes - 1:04d}\n"
                assert before.count(old) == 1
                metadata = last.stat()
                after = before.replace(old, "E" + old[1:], 1)
                last.write_text(after)
                os.utime(last, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))
                report["edit_preserved_size"] = len(before.encode()) == len(after.encode())
                report["edit_preserved_mtime"] = last.stat().st_mtime_ns == metadata.st_mtime_ns
            result = observe(label, command, incremental=label != "populate")
            result["program_sha256"] = checksum(output)
            if label == "populate":
                shutil.copyfile(output, root / "original.startup.txt")
        runs = report["runs"]
        assert runs["populate"]["program_sha256"] == runs["unchanged"]["program_sha256"]
        assert runs["populate"]["program_sha256"] != runs["last_paragraph_edit"]["program_sha256"]
        formatted = root / "formatted.bfl"
        subprocess.run([str(compiler), "format", str(last), "--out", str(formatted)], check=True)
        assert formatted.read_bytes() == last.read_bytes()
        report["large_source_format_identity"] = True
        formatted.unlink()
        modules = root / "modules"
        modules.mkdir()
        order = ["stats", *[f"rooms.r{index:04d}" for index in range(options.scenes)], "main"]
        for module in order:
            source = root / "src" / (module.replace(".", "/") + ".bfl")
            artifact = modules / (module + ".bfm.json")
            result = measure([
                str(compiler), "module", str(source), "--name", module,
                "--module-path", str(modules), "--out", str(artifact),
            ])
            report["publication"][module] = result
            if result["status"] or result["compiler_peak_rss_bytes"] >= 250_000_000:
                raise RuntimeError(module + ": " + result["stderr"])
            source.unlink()
        source_free = root / "source-free.txt"
        observe("source_free", [str(compiler), "build", str(root), "--out", str(source_free), "--clean"])
        assert source_free.read_bytes() == output.read_bytes()
        packaged = root / "packaged/startup.txt"
        package = [str(compiler), "build", str(root), "--out", str(packaged), "--package"]
        observe("package_populate", [*package, "--clean"])
        observe("package_cache", package, incremental=True)
        observe("package_unchanged", package, incremental=True)
        report["artifact_bytes"] = sum(path.stat().st_size for path in modules.glob("*.bfm.json"))
        report["compiler_unchanged"] = checksum(compiler) == report["compiler_sha256"]
        assert report["compiler_unchanged"]
    finally:
        (root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
