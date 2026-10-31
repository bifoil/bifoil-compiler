"""Original multi-module code workload with every routine executed and retained."""

from pathlib import Path


def code_modules(module_count=5, routines_per_module=3000, additions=13, *, suspending=False):
    """Use explicit runtime arithmetic so source size represents executable code."""
    if min(module_count, routines_per_module, additions) <= 0:
        raise ValueError("module, routine, and arithmetic counts must be positive")
    sources = {}
    effect = "suspend " if suspending else ""
    main = []
    routine_count = module_count * routines_per_module
    for module in range(module_count):
        declarations = []
        for local in range(routines_per_module):
            index = module * routines_per_module + local
            name = f"f{index:05d}"
            expression = f"value + {index}" + " + 0" * (additions - 1)
            declarations.append(f"export {effect}fun {name}(value: Int) -> Int = {expression}\n")
            main.append(f"import code.m{module:02d}.{name}\n")
        sources[f"code/m{module:02d}.bfl"] = "".join(declarations)
    main.append("scene start():\n")
    batches = []
    for start in range(0, routine_count, 1024):
        name = f"batch{len(batches)}"
        batches.append(name)
        main.append(f"    val {name} = [\n")
        for index in range(start, min(start + 1024, routine_count)):
            main.append(f"        ::f{index:05d},\n")
        main.append("    ]\n")
    main.append("    val batches = [" + ", ".join(batches) + "]\n")
    main.append(
        "    val results: MutableList<Int> = []\n"
        "    var total = 0\n"
        "    for batch in batches:\n"
        "        for routine in batch:\n"
        "            val value = routine(0)\n"
        "            results.add(value)\n"
        "            total += value\n"
        '    choice:\n        option "Inspect ${results.size}":\n            Unit\n'
        "    prose:\n        ${results.size}|${total}\n"
    )
    sources["main.bfl"] = "".join(main)
    return sources


def write_code_project(directory, module_count=5, routines_per_module=3000, additions=13, *, suspending=False):
    """Write only workload-owned paths under a caller-selected project directory."""
    directory = Path(directory)
    (directory / "src").mkdir(parents=True, exist_ok=True)
    (directory / "bifoil.toml").write_text('entry = "main.start"\n')
    sources = code_modules(module_count, routines_per_module, additions, suspending=suspending)
    for name, text in sources.items():
        path = directory / "src" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return sources
