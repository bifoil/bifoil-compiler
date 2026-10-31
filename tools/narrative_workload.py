"""Original prose workload: every narrative line is compiled and rendered."""

from pathlib import Path


def scene_source(index, lines):
    return (
        "import stats.visits\nexport scene enter():\n    visits += 1\n    prose:\n"
        + f"        begin{index:04d}\n"
        + "        x\n" * lines
        + f"        end{index:04d}\n"
    )


def write_project(directory, scenes=67, lines=100_000):
    directory = Path(directory)
    sources = {"stats.bfl": "export shared var visits: Int = 0\n"}
    main = "import stats.visits\n"
    for index in range(scenes):
        main += f"import rooms.r{index:04d}.enter as room{index:04d}\n"
        sources[f"rooms/r{index:04d}.bfl"] = scene_source(index, lines)
    main += "scene start():\n"
    for index in range(scenes):
        main += f"    visit room{index:04d}()\n"
    main += (
        '    choice:\n        option "Read ${visits}":\n            Unit\n'
        "    prose:\n        Finished ${visits}\n"
    )
    sources["main.bfl"] = main
    (directory / "bifoil.toml").write_text('entry = "main.start"\n')
    for name, text in sources.items():
        path = directory / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return sources
