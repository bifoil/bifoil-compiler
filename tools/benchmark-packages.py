"""Compare inline and packaged builds using independent, bounded code modules.

Usage: uv run --no-project tools/benchmark-packages.py COMPILER OUTPUT_DIRECTORY
Pass --runtime and --node together to execute both outputs through restored waits.
The output directory must not exist. Measurements require Linux and GNU Time.
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from urllib.parse import quote

from process_measurement import measure


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(65536):
            digest.update(block)
    return digest.hexdigest()


def windows_path(path):
    path = path.resolve()
    if str(path).startswith("/mnt/c/"):
        return "C:/" + str(path)[7:]
    raise ValueError("Windows Node requires inputs on the C drive")


def inventory(directory):
    return {
        path.name: {
            "sha256": checksum(path),
            "bytes": path.stat().st_size,
            "modified_ns": path.stat().st_mtime_ns,
        }
        for path in sorted(directory.glob("*.txt"))
    }


def program(path):
    line = next(line for line in path.read_text().splitlines() if line.startswith("*bifoil "))
    return json.loads(line.removeprefix("*bifoil "))


def generate(root, modules, functions):
    source = root / "src"
    source.mkdir(parents=True)
    (root / "bifoil.toml").write_text('entry = "main.start"\n')
    for index in range(modules):
        (source / f"part{index:02}.bfl").write_text(
            "".join(
                f"export fun f{number:04}(value: Int) -> Int = value" + " + 1" * 10 + "\n"
                for number in range(functions)
            )
        )
    (source / "main.bfl").write_text(
        "".join(f"import part{index:02}.f0000 as call{index:02}\n" for index in range(modules))
        + "scene start():\n    var total = 0\n"
        + "".join(f"    total += call{index:02}(1)\n" for index in range(modules))
        + '    choice:\n        option "Continue":\n            prose:\n                ${total}\n'
    )


def execute(output, expected, config, runtime, node):
    path = windows_path if node.suffix.lower() == ".exe" else lambda value: str(value.resolve())
    result = subprocess.run(
        [str(node), path(runtime / "tools/execute-conformance.mjs"), path(output), path(config)],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    actual = json.loads(result.stdout)
    wanted = {"output": [str(expected)], "options": [["Continue"]], "error": None}
    if actual != wanted:
        raise RuntimeError(f"unexpected restored execution: {actual}")
    return actual


def benchmark(compiler, root, packaged, options, config):
    generate(root, options.modules, options.functions)
    output = root / "scenes/startup.txt"
    command = [str(compiler), "build", str(root), "--out", str(output)]
    if packaged:
        command.append("--package")
    results = {}
    initial_files = None
    for phase in ["populate", "unchanged", "leaf_edit"]:
        if phase == "leaf_edit":
            source = root / "src/part00.bfl"
            source.write_text(source.read_text().replace("value + 1", "value + 2", 1))
        previous = inventory(output.parent)
        result = measure(command)
        results[phase] = result
        if result["status"]:
            raise RuntimeError(f"{phase} failed: {result['stderr']}")
        current = inventory(output.parent)
        written = {name: record for name, record in current.items() if previous.get(name) != record}
        result["written_files"] = sorted(written)
        result["written_bytes"] = sum(record["bytes"] for record in written.values())
        result["root_sha256"] = current["startup.txt"]["sha256"]
        result["root_bytes"] = current["startup.txt"]["bytes"]
        if phase == "populate":
            initial_files = current
        if phase == "unchanged" and result["written_files"]:
            raise RuntimeError("unchanged build replaced output files")
        if phase == "leaf_edit" and packaged:
            changed_code = [name for name in written if name.startswith("bifoil_code_")]
            if len(changed_code) != 1 or "startup.txt" not in written:
                raise RuntimeError("leaf edit did not replace exactly one code scene and its root")
            if any(current.get(name) != record for name, record in initial_files.items() if name != "startup.txt"):
                raise RuntimeError("immutable code scenes changed after publication")
        if options.runtime:
            expected = options.modules * 11 + (1 if phase == "leaf_edit" else 0)
            result["execution"] = execute(output, expected, config, options.runtime, options.node)
        if result["compiler_peak_rss_bytes"] >= 250_000_000:
            raise RuntimeError("bounded workload exceeded the memory contract")
        if phase != "populate" and result["wall_seconds"] >= 10:
            raise RuntimeError("bounded incremental build exceeded ten seconds")
        expected_cache = None
        if phase == "unchanged":
            expected_cache = f"0 compiled, {options.modules + 1} reused"
        elif phase == "leaf_edit":
            expected_cache = f"2 compiled, {options.modules - 1} reused"
        if expected_cache and expected_cache not in result["stdout"]:
            raise RuntimeError("unexpected module reuse summary")
    return results


def verify_baseline(report, before, after):
    for phase in ["populate", "unchanged", "leaf_edit"]:
        old = report["runs"][before][phase]["root_sha256"]
        new = report["runs"][after][phase]["root_sha256"]
        if old != new:
            raise RuntimeError(f"{after} output changed relative to the baseline compiler")


def compare_programs(root, runtime, node):
    """Compare wire data, excluding the intentionally different build identities."""
    windows = node.suffix.lower() == ".exe"
    path = windows_path if windows else lambda value: str(value.resolve())
    loader = path(runtime / "src/node-program.mjs")
    loader_url = "file:///" + quote(loader, safe="/:") if windows else Path(loader).as_uri()
    script = f"""
        import {{readCompiledProgram}} from {json.dumps(loader_url)};
        import {{isDeepStrictEqual}} from 'node:util';
        const inline = await readCompiledProgram(process.argv[1]);
        const packaged = await readCompiledProgram(process.argv[2]);
        inline.build = packaged.build = '';
        // Package function dictionaries deliberately have no prototype. Compare
        // protocol data rather than host object prototypes.
        const data = value => {{
            if (Array.isArray(value)) return value.map(data);
            if (value !== null && typeof value === 'object') {{
                return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, data(item)]));
            }}
            return value;
        }};
        if (!isDeepStrictEqual(data(inline), data(packaged))) {{
            throw Error('inline/package programs differ');
        }}
    """
    subprocess.run(
        [
            str(node), "--input-type=module", "-e", script,
            path(root / "inline/scenes/startup.txt"),
            path(root / "package/scenes/startup.txt"),
        ],
        check=True,
        timeout=30,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("--baseline", type=Path, help="compare the previous executable's inline output")
    parser.add_argument("--package-baseline", type=Path, help="compare earlier package build performance")
    parser.add_argument("--runtime", type=Path, help="explicit runtime checkout for restored execution")
    parser.add_argument("--node", type=Path, help="explicit Node executable")
    parser.add_argument("--modules", type=int, default=63)
    parser.add_argument("--functions", type=int, default=60)
    options = parser.parse_args()
    if bool(options.runtime) != bool(options.node):
        parser.error("--runtime and --node must be supplied together")
    valid_modules = 1 <= options.modules <= 63
    valid_functions = options.functions >= 1 and options.modules * options.functions <= 4000
    if not valid_modules or not valid_functions:
        parser.error("workload must fit the current bounded module/function profile")
    options.compiler = options.compiler.resolve()
    if options.runtime:
        options.runtime = options.runtime.resolve()
        options.node = options.node.resolve()
    root = options.output_directory.resolve()
    root.mkdir(parents=True, exist_ok=False)
    config = root / "execution.json"
    config.write_text(json.dumps({"choices": [0]}))
    report = {
        "modules": options.modules + 1,
        "functions": options.modules * options.functions + 1,
        "compiler_sha256": checksum(options.compiler),
        "runs": {},
    }
    modes = [("inline", options.compiler, False), ("package", options.compiler, True)]
    if options.baseline:
        modes.insert(0, ("baseline_inline", options.baseline.resolve(), False))
    if options.package_baseline:
        modes.insert(0, ("baseline_package", options.package_baseline.resolve(), True))
    for mode, compiler, packaged in modes:
        report["runs"][mode] = benchmark(compiler, root / mode, packaged, options, config)
        (root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
    if options.baseline:
        verify_baseline(report, "baseline_inline", "inline")
        report["inline_output_matches_baseline"] = True
    if options.package_baseline:
        verify_baseline(report, "baseline_package", "package")
        report["package_output_matches_baseline"] = True
    if options.runtime:
        compare_programs(root, options.runtime, options.node)
        report["assembled_code_and_metadata_match"] = True
    linked = program(root / "inline/scenes/startup.txt")
    report["instructions"] = sum(len(function["code"]) for function in linked["functions"].values())
    report["source_bytes"] = sum(path.stat().st_size for path in (root / "inline/src").glob("*.bfl"))
    report["source_lines"] = sum(len(path.read_text().splitlines()) for path in (root / "inline/src").glob("*.bfl"))
    (root / "measurement.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
