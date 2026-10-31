"""Original multi-module scene workload for compiler and checkpoint acceptance."""

from pathlib import Path


def scene_modules(scene_count=510):
    """Visit every scene and retain every typed destination at a final input wait."""
    sources = {
        "stats.bfl": (
            "export shared var visits: Int = 0\n"
            "export shared var total: Int = 0\n"
        )
    }
    main = "import stats.visits\nimport stats.total\n"
    for index in range(scene_count):
        module = f"rooms.r{index:04d}"
        main += f"import {module}.enter as room{index:04d}\n"
        sources[module.replace(".", "/") + ".bfl"] = (
            "import stats.visits\nimport stats.total\n"
            "export scene enter():\n    visits += 1\n"
            f"    total += {index}\n"
        )
    main += "scene start():\n    val destinations = [\n"
    main += "".join(f"        ::room{index:04d},\n" for index in range(scene_count))
    main += (
        "    ]\n    for destination in destinations:\n        visit destination()\n"
        '    choice:\n        option "Seen ${visits}":\n            Unit\n'
        "    prose:\n        ${visits}|${total}\n"
    )
    sources["main.bfl"] = main
    return sources


def write_scene_project(directory, scene_count=510):
    """Write only workload-owned source paths beneath an explicit project root."""
    directory = Path(directory)
    (directory / "src").mkdir(parents=True, exist_ok=True)
    (directory / "bifoil.toml").write_text('entry = "main.start"\n')
    sources = scene_modules(scene_count)
    for name, text in sources.items():
        path = directory / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return sources
