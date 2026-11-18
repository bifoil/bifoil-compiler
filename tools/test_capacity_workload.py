"""Independent dimension, ownership, and compiler checks for capacity inputs."""

import hashlib
import os
import unittest
from unittest.mock import patch

from capacity_workload import NarrativeCapacity
from test_support import NativeCompilerTest


class CapacityDescriptions(unittest.TestCase):
    def test_descriptions_reach_targets_without_constructing_full_rooms(self):
        with patch.object(NarrativeCapacity, "room_source", side_effect=AssertionError("materialized body")):
            for scenes, rows in [(100, 10_000_000), (1000, 100_000_000)]:
                spec = NarrativeCapacity(scenes)
                description = spec.describe()
                self.assertEqual(description["prose_rows"], rows)
                self.assertEqual(description["modules"], scenes + 2)
                self.assertEqual(description["import_edges"], 2 * scenes + 1)
                self.assertGreater(description["source_lines"], rows)
                self.assertLess(description["maximum_source_bytes"], 1024 * 1024)
                self.assertFalse(description["capacity_qualified"])

    def test_invalid_dimensions_reject_before_generation(self):
        for scenes, rows in [(0, 1), (1023, 1), (1, 0), (1, 100001)]:
            with self.subTest(scenes=scenes, rows=rows), self.assertRaises(ValueError):
                NarrativeCapacity(scenes, rows)


class CapacityProjects(NativeCompilerTest):
    def test_source_identity_counts_and_reachable_room_calls_compile(self):
        spec = NarrativeCapacity(3, 4)
        directory = self.root / "owned"
        manifest = spec.write(directory)
        sources = {}
        for name, expected in manifest["source_files"].items():
            path = directory / "src" / name
            raw = path.read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), expected["sha256"])
            self.assertEqual(raw.count(b"\n"), expected["lines"])
            self.assertEqual(len(raw), expected["bytes"])
            sources[name] = raw.decode("utf8")
        program = self.compile_sources(sources)
        self.assertEqual(len(program["modules"]), 5)
        destinations = {
            op["name"] for op in program["functions"]["main.start"]["code"] if op["op"] == "load"
        }
        for index in range(3):
            self.assertIn(f"rooms.r{index:04d}.enter", destinations)
            code = program["functions"][f"rooms.r{index:04d}.enter"]["code"]
            constants = [op["value"] for op in code if op["op"] == "constant" and op["ty"]["kind"] == "string"]
            self.assertTrue(any("begin" in text and "end" in text and text.count(" x") == 4 for text in constants))
            self.assertTrue(any(op["op"] == "choose" for op in code))

    def test_grouped_routes_keep_every_destination_reachable_below_choice_guard(self):
        spec = NarrativeCapacity(129, 1)
        main = spec.main_source()
        self.assertEqual(main.count('option "Group '), 2)
        self.assertEqual(main.count('option "Room '), 129)
        sources = {"main.bfl": main, "stats.bfl": "export shared var visits: Int = 0\n"}
        for index in range(spec.scenes):
            sources[f"rooms/r{index:04d}.bfl"] = spec.room_source(index)
        program = self.compile_sources(sources)
        self.assertEqual(len(program["modules"]), 131)
        destinations = [op["name"] for op in program["functions"]["main.start"]["code"] if op["op"] == "load"]
        for index in range(spec.scenes):
            self.assertIn(f"rooms.r{index:04d}.enter", destinations)

    def test_foreign_files_and_disk_reserve_are_preserved(self):
        spec = NarrativeCapacity(1, 1)
        directory = self.root / "foreign"
        directory.mkdir()
        marker = directory / "do-not-touch.bin"
        marker.write_bytes(b"\x00foreign")
        with self.assertRaisesRegex(ValueError, "must be empty"):
            spec.write(directory)
        self.assertEqual(marker.read_bytes(), b"\x00foreign")
        empty = self.root / "too-small"
        with self.assertRaisesRegex(ValueError, "disk reserve"):
            spec.write(empty, minimum_free_bytes=10**30)
        self.assertFalse(empty.exists())

    def test_a_missing_parent_is_rejected_before_creating_any_workload_files(self):
        directory = self.root / "absent-parent/project"
        workload = NarrativeCapacity(1, 1)
        with self.assertRaisesRegex(ValueError, "output parent must already exist"):
            workload.write(directory)
        self.assertFalse(directory.parent.exists())


class CapacityBenchmark(NativeCompilerTest):
    def test_small_workflow_retains_roots_and_publishes_source_free_modules(self):
        project = self.root / "benchmark"
        self.run_tool("benchmark-capacity.py", [self.compiler, project, "--scenes", "2", "--rows", "3", "--publish"])
        report = self.read_report(project, "capacity-measurement.json")
        self.assertTrue(report["experiment_passed"])
        self.assertFalse(report["capacity_qualified"])
        self.assertTrue(report["leaf_edit"]["same_size"])
        self.assertTrue(report["leaf_edit"]["same_mtime"])
        self.assertEqual(len(report["publication"]), 4)
        self.assertNotEqual(report["runs"]["populate"]["root_sha256"], report["runs"]["leaf_edit"]["root_sha256"])
        self.assertEqual(report["runs"]["leaf_edit"]["root_sha256"], report["runs"]["source_free_populate"]["root_sha256"])
        self.assertEqual(list((project / "src").rglob("*.bfl")), [])
        self.assertTrue((project / "compiled/original.capacity-measurement.startup.txt").is_file())
        for module, observed in report["publication"].items():
            self.assertEqual(checksum(project / "modules" / (module + ".bfm.json")), observed["artifact_sha256"])

    def test_existing_observations_and_changed_sources_reject_without_overwrite(self):
        project = self.root / "preserved"
        NarrativeCapacity(1, 1).write(project)
        report = project / "capacity-measurement.json"
        report.write_text("retained observation")
        with self.assertRaisesRegex(ValueError, "preserve the existing report"):
            self.run_tool("benchmark-capacity.py", [self.compiler, project, "--scenes", "1", "--rows", "1"])
        self.assertEqual(report.read_text(), "retained observation")
        room = project / "src/rooms/r0000.bfl"
        room.write_text("user modification")
        with self.assertRaisesRegex(ValueError, "generated source changed"):
            self.run_tool("benchmark-capacity.py", [self.compiler, project, "--scenes", "1", "--rows", "1",
                                                   "--report-name", "new-observation.json"])
        self.assertEqual(room.read_text(), "user modification")
        self.assertFalse((project / "new-observation.json").exists())
        self.assertFalse((project / "compiled").exists())

    def test_aliased_source_and_report_path_traversal_reject_before_measurement(self):
        project = self.root / "aliases"
        NarrativeCapacity(1, 1).write(project)
        room = project / "src/rooms/r0000.bfl"
        outside = self.root / "outside.bfl"
        original = room.read_bytes()
        outside.write_bytes(original)
        room.unlink()
        os.link(outside, room)
        with self.assertRaisesRegex(ValueError, "generated source is aliased"):
            self.run_tool("benchmark-capacity.py", [self.compiler, project, "--scenes", "1", "--rows", "1"])
        self.assertEqual(outside.read_bytes(), original)
        self.assertFalse((project / "capacity-measurement.json").exists())
        self.run_tool("benchmark-capacity.py", [self.compiler, project, "--report-name", "../outside.json"], expected=2)
        self.assertFalse((self.root / "outside.json").exists())


def checksum(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()
