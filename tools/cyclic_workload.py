"""Original cyclic source workloads shared by resource observers and controls."""

MAX_MODULES = 512
MAX_FUNCTIONS = 81_920
MAX_MODULE_FUNCTIONS = 16_384
MAX_NAME_PADDING = 160


def module_source(peer, count, entry=False):
    expression = "value" + " + 1" * 10
    source = f"import {peer}.f0000 as peerFirst\n"
    source += "".join(
        ("export " if number == 0 else "")
        + f"fun f{number:04d}(value: Int) -> Int = {expression}\n"
        for number in range(count)
    )
    if entry:
        source += "scene start():\n    prose:\n        ${f0000(0)}|${peerFirst(0)}\n"
    return source


def sources(module_count, function_count, name_padding=0):
    if not 2 <= module_count <= MAX_MODULES:
        raise ValueError(f"modules must be between 2 and {MAX_MODULES}")
    if not module_count + 1 <= function_count <= MAX_FUNCTIONS:
        raise ValueError(
            f"functions must be between modules + 1 and {MAX_FUNCTIONS}"
        )
    if not 0 <= name_padding <= MAX_NAME_PADDING:
        raise ValueError(f"name padding must be between 0 and {MAX_NAME_PADDING}")
    names = (
        ["alpha", "beta"]
        if module_count == 2
        else [f"part{index:03d}" for index in range(module_count)]
    )
    if name_padding:
        prefix = "m" * name_padding + "_"
        names = [prefix + name for name in names]
    count, remainder = divmod(function_count - 1, module_count)
    result = {}
    for index, name in enumerate(names):
        functions = count + (index >= module_count - remainder)
        if functions + (index == 0) > MAX_MODULE_FUNCTIONS:
            raise ValueError("per-module function budget exceeded")
        result[name] = module_source(
            names[(index + 1) % module_count], functions, entry=index == 0
        )
    return result
