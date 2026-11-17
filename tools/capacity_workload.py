"""Bounded generator for reachable large narrative projects.

One module is materialized at a time. Each source row contributes to a room's
rendered paragraph; the root selects a room rather than retaining all output.
This describes a workload, not a capacity or resource-compliance result.
"""

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import shutil

from narrative_workload import scene_source


ROOM_PROMPT = '    choice:\n        option "Return ${visits}":\n            Unit\n'
STATS = "export shared var visits: Int = 0\n"
MAX_SCENES = 1022
MAX_ROWS_PER_SCENE = 100_000


@dataclass(frozen=True)
class NarrativeCapacity:
    scenes: int
    rows_per_scene: int = 100_000

    def __post_init__(self):
        if not 1 <= self.scenes <= MAX_SCENES:
            raise ValueError("scene count must be between 1 and 1022")
        if not 1 <= self.rows_per_scene <= MAX_ROWS_PER_SCENE:
            raise ValueError("rows per scene must be between 1 and 100000")

    def main_source(self):
        lines = ["import stats.visits\n"]
        lines.extend(
            f"import rooms.r{index:04d}.enter as room{index:04d}\n"
            for index in range(self.scenes)
        )
        lines.append("scene start():\n    choice:\n")
        if self.scenes <= 128:
            lines.extend(
                f'        option "Room {index:04d}":\n'
                f"            visit room{index:04d}()\n"
                for index in range(self.scenes)
            )
        else:
            for group, start in enumerate(range(0, self.scenes, 128)):
                lines.append(f'        option "Group {group:04d}":\n            choice:\n')
                lines.extend(
                    f'                option "Room {index:04d}":\n'
                    f"                    visit room{index:04d}()\n"
                    for index in range(start, min(start + 128, self.scenes))
                )
        lines.append("    prose:\n        Finished ${visits}\n")
        return "".join(lines)

    def room_source(self, index):
        if not 0 <= index < self.scenes:
            raise ValueError("room index out of bounds")
        return scene_source(index, self.rows_per_scene) + ROOM_PROMPT

    def describe(self):
        # Count repeated rows arithmetically. A 100M-row description never
        # allocates or traverses 100M rows, and does not fabricate measurements.
        main = self.main_source()
        headers = [scene_source(index, 0) + ROOM_PROMPT for index in range(self.scenes)]
        room_bytes = [len(header.encode("utf8")) + 10 * self.rows_per_scene for header in headers]
        return {
            "family": "reachable-branching-narrative",
            "scenes": self.scenes,
            "modules": self.scenes + 2,
            "prose_rows": self.scenes * self.rows_per_scene,
            "source_lines": main.count("\n") + STATS.count("\n")
            + sum(header.count("\n") + self.rows_per_scene for header in headers),
            "source_bytes": len(main.encode("utf8")) + len(STATS.encode("utf8")) + sum(room_bytes),
            "maximum_source_bytes": max([len(main.encode("utf8")), *room_bytes]),
            "import_edges": 2 * self.scenes + 1,
            "scene_declarations": self.scenes + 1,
            "shared_declarations": 1,
            "generic_parameters": 0,
            "static_options": 2 * self.scenes + (0 if self.scenes <= 128 else (self.scenes + 127) // 128),
            "capacity_qualified": False,
            "counts_are": "deterministic input dimensions, not compiler/runtime observations",
        }

    def write(self, directory, *, minimum_free_bytes=5_000_000_000):
        directory = Path(directory)
        if directory.exists() and any(directory.iterdir()):
            raise ValueError("the generated project directory must be empty")
        parent = directory if directory.exists() else directory.parent
        if not parent.is_dir():
            raise ValueError("the output parent must already exist")
        description = self.describe()
        # Include conservative room for caches, immutable chunks, module
        # publication, snapshots, and compiler staging, beyond the sources.
        forecast = 3 * description["source_bytes"] + 100_000_000
        if shutil.disk_usage(parent).free < minimum_free_bytes + forecast:
            raise ValueError("insufficient space for the workload and retained disk reserve")
        directory.mkdir(exist_ok=True)
        files = {}

        def write_source(name, text):
            path = directory / "src" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            contents = text.encode("utf8")
            with path.open("xb") as output:
                output.write(contents)
            files[name] = {
                "bytes": len(contents), "lines": text.count("\n"),
                "sha256": hashlib.sha256(contents).hexdigest(),
            }

        with (directory / "bifoil.toml").open("x", encoding="utf8", newline="\n") as output:
            output.write('entry = "main.start"\n')
        write_source("stats.bfl", STATS)
        for index in range(self.scenes):
            write_source(f"rooms/r{index:04d}.bfl", self.room_source(index))
        write_source("main.bfl", self.main_source())
        assert sum(file["bytes"] for file in files.values()) == description["source_bytes"]
        assert sum(file["lines"] for file in files.values()) == description["source_lines"]
        manifest = {
            "format": "bifoil.capacity-input.v1", "description": description,
            "source_files": files, "generator": "capacity_workload.py + narrative_workload.py",
            "capacity_qualified": False,
        }
        with (directory / "generated-inputs.json").open("x", encoding="utf8", newline="\n") as output:
            json.dump(manifest, output, indent=2)
            output.write("\n")
        return manifest
