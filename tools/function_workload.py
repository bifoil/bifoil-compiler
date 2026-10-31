"""Original function graph with every routine called and retained across visits."""

from pathlib import Path


def function_modules(scene_count=10, routines_per_scene=8190, *, export_routines=True):
    """Keep each source unit bounded while exercising a wide linked interface."""
    if min(scene_count, routines_per_scene) <= 0:
        raise ValueError("scene and routine counts must be positive")
    # The widened accumulator stays exact at this scale without overflowing Int.
    sources = {
        "stats.bfl": (
            "export shared val callbacks: MutableList<() -> Int> = []\n"
            "export shared val results: MutableList<Int> = []\n"
            "export shared var total: Float = 0\n"
            "export shared var visits: Int = 0\n"
            "export fun routineCount() -> Int = callbacks.size\n"
            "export fun resultCount() -> Int = results.size\n"
        )
    }
    visibility = "export " if export_routines else ""
    imports = (
        "import stats.callbacks\nimport stats.results\n"
        "import stats.total\nimport stats.visits\n"
    )
    main = [imports, "import stats.routineCount\nimport stats.resultCount\n"]
    for scene in range(scene_count):
        module = f"routines.r{scene:02d}"
        main.append(f"import {module}.enter as room{scene:02d}\n")
        start = scene * routines_per_scene
        stop = start + routines_per_scene
        lines = [imports]
        lines.extend(
            f"{visibility}fun f{index:05d}() -> Int = {index}\n"
            for index in range(start, stop)
        )
        lines.append("export scene enter():\n")
        batches = []
        for offset in range(start, stop, 1024):
            name = f"batch{len(batches)}"
            batches.append(name)
            lines.append(f"    val {name} = [\n")
            lines.extend(
                f"        ::f{index:05d},\n"
                for index in range(offset, min(offset + 1024, stop))
            )
            lines.append("    ]\n")
        lines.append("    val batches = [" + ", ".join(batches) + "]\n")
        lines.append(
            "    for batch in batches:\n"
            "        for routine in batch:\n"
            "            callbacks.add(routine)\n"
            "            val value = routine()\n"
            "            results.add(value)\n"
            "            total += value\n"
            "    visits += 1\n"
        )
        sources[module.replace(".", "/") + ".bfl"] = "".join(lines)
    main.append("scene start():\n    val destinations = [\n")
    main.extend(f"        ::room{scene:02d},\n" for scene in range(scene_count))
    main.append(
        "    ]\n    for destination in destinations:\n        visit destination()\n"
        '    choice:\n        option "Inspect ${resultCount()} functions":\n            Unit\n'
        "    prose:\n        ${visits}|${routineCount()}|${resultCount()}|${total}\n"
    )
    sources["main.bfl"] = "".join(main)
    return sources


def write_function_project(directory, scene_count=10, routines_per_scene=8190, *, export_routines=True):
    """Write only generated project paths under a caller-selected directory."""
    directory = Path(directory)
    (directory / "src").mkdir(parents=True, exist_ok=True)
    (directory / "bifoil.toml").write_text('entry = "main.start"\n')
    sources = function_modules(scene_count, routines_per_scene, export_routines=export_routines)
    for name, text in sources.items():
        path = directory / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return sources
