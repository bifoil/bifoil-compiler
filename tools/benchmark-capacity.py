"""Measure a branching narrative with bounded generation and explicit stages.

Compiler jobs use the Linux native RSS observer. Input generation, controller
bookkeeping, and runtime checks are outside those jobs. Results do not certify
aggregate memory, other source families, or full-language capacity.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from capacity_workload import NarrativeCapacity
from process_measurement import measure


def checksum(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def node_path(path, node):
    path = str(Path(path).resolve())
    if node.suffix.lower() != ".exe":
        return path
    if not path.startswith("/mnt/c/"):
        raise ValueError("Windows Node verification requires inputs on the C drive")
    return "C:/" + path[7:]


def checked_inputs(spec, project):
    if not project.exists():
        return spec.write(project)
    inputs = json.loads((project / "generated-inputs.json").read_text())
    if inputs.get("format") != "bifoil.capacity-input.v1" or inputs["description"] != spec.describe():
        raise ValueError("existing input manifest does not match this workload")
    names = {"stats.bfl", "main.bfl", *[f"rooms/r{index:04d}.bfl" for index in range(spec.scenes)]}
    if set(inputs["source_files"]) != names:
        raise ValueError("input manifest contains unexpected source paths")
    for name, facts in inputs["source_files"].items():
        source = project / "src" / name
        if source.is_symlink() or source.stat().st_nlink != 1 or not source.resolve().is_relative_to(project):
            raise ValueError("generated source is aliased: " + name)
        if checksum(source) != facts["sha256"]:
            raise ValueError("generated source changed before measurement: " + name)
    return inputs


class CapacityExperiment:
    def __init__(self, options):
        self.options = options
        self.spec = NarrativeCapacity(options.scenes, options.rows)
        self.project = options.project.resolve()
        self.report_path = self.project / options.report_name
        if self.report_path.exists() or self.report_path.is_symlink():
            raise ValueError("preserve the existing report and choose a new report name or project")
        self.inputs = checked_inputs(self.spec, self.project)
        self.compiler = options.compiler.resolve()
        self.output = self.project / "compiled/startup.txt"
        self.command = [str(self.compiler), "build", str(self.project), "--out", str(self.output), "--package"]
        self.leaf_name = f"rooms/r{self.spec.scenes - 1:04d}.bfl"
        self.leaf = self.project / "src" / self.leaf_name
        self.edited_digest = None
        self.original = None
        self.edited = None
        self.report = {
            "compiler_sha256": checksum(self.compiler), "input_description": self.inputs["description"],
            "runs": {}, "publication": {}, "verification": {},
            "capacity_qualified": False, "aggregate_resource_acceptance": False,
            "memory_metric": "individual compiler process peak RSS; Linux address-space guard remains separate",
            "content": "synthetic repeated x prose with per-room markers; other source families are not qualified",
            "tool_sha256": {file.name: checksum(file) for file in [
                Path(__file__), Path(__file__).with_name("capacity_workload.py"),
                Path(__file__).with_name("narrative_workload.py"),
            ]},
        }
        # Anchor the timestamp BEFORE populating caches. The later edit must
        # preserve exactly the timestamp observed by the populated build.
        anchor = self.leaf.stat().st_mtime_ns // 1_000_000_000 * 1_000_000_000
        os.utime(self.leaf, ns=(anchor, anchor))
        self.report["initial_leaf_mtime_ns"] = self.leaf.stat().st_mtime_ns

    def record(self):
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf8", dir=self.project,
                    prefix=".capacity-report-", suffix=".json", delete=False) as output:
                temporary = Path(output.name)
                json.dump(self.report, output, indent=2)
                output.write("\n")
                output.flush()
                os.fsync(output.fileno())
            os.replace(temporary, self.report_path)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)

    def observe(self, label, arguments, *, incremental=False, artifact=False):
        arguments = list(map(str, arguments))
        result = measure(arguments)
        self.report["publication" if artifact else "runs"][label] = result
        destination = Path(arguments[arguments.index("--out") + 1])
        if destination.exists():
            result["artifact_sha256" if artifact else "root_sha256"] = checksum(destination)
        self.record()
        print(label, result["status"], round(result["wall_seconds"], 3), result["compiler_peak_rss_bytes"], flush=True)
        if result["status"] != 0:
            raise RuntimeError(label + ": " + result["stderr"])
        if result["compiler_peak_rss_bytes"] >= 250_000_000:
            raise RuntimeError(label + " exceeded the observed compiler RSS threshold")
        if incremental and result["wall_seconds"] >= 10:
            raise RuntimeError(label + " exceeded ten seconds")
        return result

    def verify(self, label, path, edited):
        if self.options.runtime is None:
            return
        config = self.project / (self.report_path.stem + "." + label + ".verification.json")
        config.write_text(json.dumps({
            "scenes": self.spec.scenes, "rows": self.spec.rows_per_scene,
            "routing_group_size": 128 if self.spec.scenes > 128 else None,
            "rooms": sorted({0, self.spec.scenes // 2, self.spec.scenes - 1}),
            "edited_room": self.spec.scenes - 1 if edited else None,
        }, indent=2) + "\n")
        node = self.options.node.resolve()
        script = self.options.runtime / "tools/verify-narrative-capacity.mjs"
        result = subprocess.run([str(node), node_path(script, node), node_path(path, node), node_path(config, node)],
                                text=True, capture_output=True)
        self.report["verification"][label] = {
            "status": result.returncode, "stdout": result.stdout, "stderr": result.stderr,
            "verifier_sha256": checksum(script),
        }
        self.record()
        if result.returncode != 0:
            raise RuntimeError(label + ": " + result.stderr)
        checked = json.loads(result.stdout)
        if not checked["passed"]:
            raise RuntimeError(label + ": runtime verification rejected the generated program")
        self.report["verification"][label]["checked"] = checked

    def populate(self):
        self.original = self.observe("populate", self.command)
        original_path = self.output.with_name("original." + self.report_path.stem + ".startup.txt")
        if original_path.exists():
            raise ValueError("preserve the existing original root: " + str(original_path))
        shutil.copy2(self.output, original_path)
        for iteration in range(3):
            same = self.observe("unchanged_" + str(iteration), self.command, incremental=True)
            if same["root_sha256"] != self.original["root_sha256"]:
                raise RuntimeError("unchanged compilation altered the root")
        self.verify("original", original_path, False)

    def edit_leaf(self):
        before = self.leaf.read_text()
        marker = f"end{self.spec.scenes - 1:04d}\n"
        if before.count(marker) != 1:
            raise RuntimeError("ambiguous leaf edit")
        metadata = self.leaf.stat()
        self.leaf.write_text(before.replace(marker, "E" + marker[1:], 1))
        os.utime(self.leaf, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))
        self.edited_digest = checksum(self.leaf)
        facts = self.report["leaf_edit"] = {
            "source": self.leaf_name, "original_sha256": self.inputs["source_files"][self.leaf_name]["sha256"],
            "edited_sha256": self.edited_digest, "same_size": self.leaf.stat().st_size == metadata.st_size,
            "same_mtime": self.leaf.stat().st_mtime_ns == metadata.st_mtime_ns,
            "original_mtime_ns": metadata.st_mtime_ns, "edited_mtime_ns": self.leaf.stat().st_mtime_ns,
        }
        if not facts["same_size"] or not facts["same_mtime"]:
            raise RuntimeError("the adversarial edit did not preserve size and timestamp")
        self.edited = self.observe("leaf_edit", self.command, incremental=True)
        if self.edited["root_sha256"] == self.original["root_sha256"]:
            raise RuntimeError("same-size/mtime source edit was missed")
        self.verify("edited", self.output, True)
        self.observe("edited_unchanged", self.command, incremental=True)

    def publish_modules(self):
        modules = self.project / "modules"
        modules.mkdir(exist_ok=True)
        order = ["stats", *[f"rooms.r{index:04d}" for index in range(self.spec.scenes)], "main"]
        for module in order:
            name = module.replace(".", "/") + ".bfl"
            source = self.project / "src" / name
            expected = self.edited_digest if name == self.leaf_name else self.inputs["source_files"][name]["sha256"]
            if checksum(source) != expected:
                raise RuntimeError("source changed before publication: " + name)
            self.observe(module, [self.compiler, "module", source, "--name", module,
                                 "--module-path", modules, "--out", modules / (module + ".bfm.json")], artifact=True)
            if checksum(source) != expected:
                raise RuntimeError("source changed during publication: " + name)
            source.unlink()

    def source_free(self):
        # Source publication is a separate completed phase. Retain its inactive
        # disposable cache, so the first artifact-only link is genuinely cold.
        cache = self.project / ".bifoil/cache/v1"
        archive = cache.with_name("source-" + self.report_path.stem)
        if archive.exists() or archive.is_symlink():
            raise ValueError("preserve the existing source-cache archive")
        expected = {"build.sha256", "completed.json"}
        expected.update(name.removesuffix(".bfl").replace("/", ".") + ".bfm.json"
                        for name in self.inputs["source_files"])
        if cache.is_symlink() or {path.name for path in cache.iterdir()} != expected:
            raise ValueError("source cache contains unexpected files")
        identities = {}
        for path in sorted(cache.iterdir()):
            metadata = path.stat()
            if path.is_symlink() or not path.is_file() or metadata.st_nlink != 1 or metadata.st_uid != os.getuid():
                raise ValueError("source-cache file is aliased or not owned: " + path.name)
            identities[path.name] = {"bytes": metadata.st_size, "sha256": checksum(path)}
        cache.rename(archive)
        self.report["source_cache_control"] = {
            "archived_to": str(archive.relative_to(self.project)), "files": identities,
            "action": "rename inactive owned source cache before first artifact-only link; no files deleted",
        }
        self.record()
        output = self.project / "source-free/startup.txt"
        command = [self.compiler, "build", self.project, "--out", output, "--package"]
        result = self.observe("source_free_populate", command)
        if result["root_sha256"] != self.edited["root_sha256"]:
            raise RuntimeError("source-free publication changed the linked program")
        for iteration in range(3):
            self.observe("source_free_unchanged_" + str(iteration), command, incremental=True)
        self.verify("source_free", output, True)

    def run(self):
        started = time.monotonic()
        try:
            self.populate()
            self.edit_leaf()
            if self.options.publish:
                self.publish_modules()
                self.source_free()
            self.report["experiment_passed"] = True
        except BaseException as error:
            self.report["experiment_passed"] = False
            self.report["failure"] = str(error)
            raise
        finally:
            self.report["elapsed_experiment_seconds"] = time.monotonic() - started
            self.record()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("project", type=Path)
    parser.add_argument("--scenes", type=int, default=100)
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--node", type=Path)
    parser.add_argument("--report-name", default="capacity-measurement.json")
    options = parser.parse_args()
    if (options.runtime is None) != (options.node is None):
        parser.error("--runtime and --node are required together")
    if Path(options.report_name).name != options.report_name or Path(options.report_name).suffix != ".json":
        parser.error("--report-name must be one JSON filename inside the project")
    CapacityExperiment(options).run()


if __name__ == "__main__":
    main()
