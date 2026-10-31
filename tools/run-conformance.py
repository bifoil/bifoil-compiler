"""Run original Bifoil conformance projects against explicit compiler and runtime paths."""

import argparse
import json
import re
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from functools import lru_cache
from pathlib import Path


CASE_FIELDS = {
    "id",
    "directory",
    "files",
    "source",
    "decisions",
    "compile_error",
    "runtime_error",
    "choices",
    "output",
    "options",
    "precompile",
    "legacy_stats",
    "quantum",
}


def validate_case(case):
    """Reject ambiguous or misspelled expectations before invoking any compiler."""
    if not isinstance(case, dict):
        raise ValueError("each conformance case must be an object")
    identifier = case.get("id")
    if not isinstance(identifier, str) or not re.fullmatch(r"[a-z0-9-]+", identifier):
        raise ValueError("case ids use lowercase letters, digits, and hyphens")
    if unknown := case.keys() - CASE_FIELDS:
        raise ValueError(f"{identifier}: unknown case fields: {', '.join(sorted(unknown))}")
    if sum(key in case for key in ("directory", "files", "source")) != 1:
        raise ValueError(f"{identifier}: provide exactly one of directory, files, or source")
    if "compile_error" in case and "runtime_error" in case:
        raise ValueError(f"{identifier}: compile and runtime errors are mutually exclusive")
    for key in ("directory", "source", "compile_error", "runtime_error"):
        if key in case and not isinstance(case[key], str):
            raise ValueError(f"{identifier}: {key} must be a string")
    for key in ("compile_error", "runtime_error"):
        if key in case and not case[key]:
            raise ValueError(f"{identifier}: {key} must be a nonempty expected diagnostic")
    for key in ("decisions", "choices", "output", "options", "precompile"):
        if key in case and not isinstance(case[key], list):
            raise ValueError(f"{identifier}: {key} must be an array")
    for key in ("decisions", "output", "precompile"):
        if any(not isinstance(value, str) for value in case.get(key, [])):
            raise ValueError(f"{identifier}: {key} entries must be strings")
    if any(
        not isinstance(group, list) or any(not isinstance(label, str) for label in group)
        for group in case.get("options", [])
    ):
        raise ValueError(f"{identifier}: options must contain arrays of labels")
    for key in ("files", "legacy_stats"):
        if key in case and not isinstance(case[key], dict):
            raise ValueError(f"{identifier}: {key} must be an object")
    if "files" in case and any(
        not isinstance(name, str) or not isinstance(text, str)
        for name, text in case["files"].items()
    ):
        raise ValueError(f"{identifier}: files must map relative paths to source strings")
    if "quantum" in case and (type(case["quantum"]) is not int or case["quantum"] <= 0):
        raise ValueError(f"{identifier}: quantum must be a positive integer")
    return identifier


@lru_cache(maxsize=64)
def native_path(path, windows_node):
    """Translate generated project paths only when the selected Node is a Windows executable."""
    resolved = str(Path(path).resolve())
    if windows_node and resolved.startswith("/mnt/"):
        return subprocess.check_output(["wslpath", "-w", resolved], text=True).strip()
    return resolved


def source_files(case, index_path):
    if "directory" in case:
        directory = (index_path.parent / case["directory"]).resolve()
        if not directory.is_relative_to(index_path.parent):
            raise ValueError(
                "case directories must stay under the conformance directory"
            )
        return {
            str(path.relative_to(directory)): path.read_text()
            for path in sorted(directory.rglob("*.bfl"))
        }
    return case.get("files", {"main.bfl": case.get("source", "")})


def write_project(project, case, sources, legacy_sources):
    (project / "src").mkdir()
    manifest = 'entry = "main.start"\n' + (
        'legacy_path = "legacy"\n' if legacy_sources else ""
    )
    (project / "bifoil.toml").write_text(manifest)
    for name, text in legacy_sources.items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("legacy source paths must be relative")
        target = project / "legacy" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    (project / "case.json").write_text(json.dumps(case))
    for name, text in sources.items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("case source paths must be relative to src/")
        target = project / "src" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)


def child_failure(phase, result):
    """Keep process failure distinct from success even when the child is silent."""
    executable = result.args[0]
    status = (
        f"signal {-result.returncode}"
        if result.returncode < 0 else f"exit {result.returncode}"
    )
    output = "\n".join(part.strip() for part in (result.stderr, result.stdout) if part.strip())
    message = f"{phase} failed: {executable} ({status})"
    return message + (": " + output if output else " (no output)")


def format_sources(project, case, compiler):
    """Normalize valid generated inputs and check idempotence before compiling."""
    for source in sorted((project / "src").rglob("*.bfl")):
        result = subprocess.run(
            [str(compiler), "format", str(source)], text=True, capture_output=True
        )
        if result.returncode:
            # Invalid syntax remains untouched for the normal expected-error check.
            if case.get("compile_error") and result.returncode > 0 and result.stderr.strip():
                continue
            return child_failure("source formatting", result)
        source.write_text(result.stdout)
        checked = subprocess.run(
            [str(compiler), "format", str(source), "--check"],
            text=True, capture_output=True,
        )
        if checked.returncode:
            return child_failure("source formatting idempotence", checked)
    return None


def precompile_modules(project, case, compiler):
    """Exercise reusable artifacts by removing their original source before linking."""
    sources = []
    for name in case.get("precompile", []):
        if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*(\.[A-Za-z_][A-Za-z_0-9]*)*", name):
            raise ValueError("precompiled module names must be dotted identifiers")
        relative = Path(*name.split("."))
        source = project / "src" / relative.with_suffix(".bfl")
        output = project / "modules" / relative.with_suffix(".bfm.json")
        output.parent.mkdir(parents=True, exist_ok=True)
        result = subprocess.run(
            [
                str(compiler),
                "module",
                str(source),
                "--name",
                name,
                "--out",
                str(output),
                "--module-path",
                str(project / "modules"),
            ],
            text=True,
            capture_output=True,
        )
        if result.returncode:
            return child_failure("module precompilation", result)
        sources.append(source)
    # Compile a complete group before removing source; cyclic artifacts are distributed together.
    for source in sources:
        source.unlink()
    return None


def compare_result(case, actual):
    expected_error = case.get("runtime_error")
    if expected_error:
        if not actual["error"] or expected_error not in actual["error"]:
            return "expected runtime failure; got " + str(actual)
    elif actual["error"]:
        return actual["error"]
    if actual["output"] != case.get("output", []):
        return f"output {actual['output']!r} != {case.get('output', [])!r}"
    if "options" in case and actual["options"] != case["options"]:
        return "option snapshots differ: " + repr(actual["options"])
    return None


def run_case(
    case, index_path, compiler, runtime, node, windows_node, engine, *,
    packaged=False, formatted=False,
):
    # TemporaryDirectory owns every generated project, including failure paths.
    # On WSL, a Windows Node process needs a directory on the mounted Windows drive.
    temporary_parent = None
    if windows_node:
        temporary_parent = index_path.parent.parent / ".bifoil/development/conformance"
        temporary_parent.mkdir(parents=True, exist_ok=True)
    identifier = case["id"]
    if not re.fullmatch(r"[a-z0-9-]+", identifier):
        raise ValueError("case ids use lowercase letters, digits, and hyphens")
    with tempfile.TemporaryDirectory(
        prefix=identifier + "-", dir=temporary_parent
    ) as temporary:
        project = Path(temporary)
        sources = source_files(case, index_path)
        legacy_sources = {}
        if "directory" in case:
            legacy_directory = index_path.parent / case["directory"] / "legacy"
            if legacy_directory.exists():
                legacy_sources = {
                    str(path.relative_to(legacy_directory)): path.read_text()
                    for path in sorted(legacy_directory.rglob("*.txt"))
                }
        case = dict(case)
        if engine:
            case["native_engine"] = native_path(engine, windows_node)
        write_project(project, case, sources, legacy_sources)
        if formatted and (failure := format_sources(project, case, compiler)) is not None:
            return failure
        if (failure := precompile_modules(project, case, compiler)) is not None:
            return failure
        startup = project / "startup.txt"
        command = [str(compiler), "build", str(project), "--out", str(startup)]
        if packaged:
            command.append("--package")
        compiled = subprocess.run(
            command,
            text=True,
            capture_output=True,
        )
        if compiled.returncode < 0:
            return child_failure("compilation", compiled)
        if expected_error := case.get("compile_error"):
            if compiled.returncode == 0 or expected_error not in compiled.stderr:
                observed = child_failure("compilation", compiled) if compiled.returncode else "compilation succeeded"
                return f"expected compile error {expected_error}; {observed}"
            return None
        if compiled.returncode:
            return child_failure("compilation", compiled)
        executed = subprocess.run(
            [
                node,
                native_path(runtime / "tools/execute-conformance.mjs", windows_node),
                native_path(startup, windows_node),
                native_path(project / "case.json", windows_node),
            ],
            text=True,
            capture_output=True,
        )
        if executed.returncode:
            return child_failure("execution", executed)
        return compare_result(case, json.loads(executed.stdout))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True)
    parser.add_argument("--runtime", required=True)
    parser.add_argument("--node", default="node")
    parser.add_argument("--cases", required=True, nargs="+")
    parser.add_argument(
        "--engine",
        help="Explicit unmodified ChoiceScript web/scene.js for legacy cases",
    )
    parser.add_argument(
        "--only", nargs="+", help="Run only these case IDs; unknown IDs are errors"
    )
    parser.add_argument(
        "--jobs",
        type=int,
        choices=range(1, 5),
        default=1,
        help="Run up to four isolated cases concurrently; results retain index order",
    )
    parser.add_argument("--package", action="store_true", help="Exercise independently reusable code scene packages")
    parser.add_argument(
        "--format-sources", action="store_true",
        help="Format generated Bifoil inputs and verify idempotence before compilation",
    )
    arguments = parser.parse_args()
    compiler = Path(arguments.compiler).resolve()
    runtime = Path(arguments.runtime).resolve()
    windows_node = arguments.node.lower().endswith(".exe")
    selected = set(arguments.only or [])
    known = set()
    work = []
    # Resolve all index entries and selection errors before starting any compiler.
    # Every worker still owns a separate TemporaryDirectory and child processes.
    for name in arguments.cases:
        index = Path(name).resolve()
        cases = json.loads(index.read_text())
        if not isinstance(cases, list):
            raise ValueError(f"case index must be an array: {index}")
        for case in cases:
            identifier = validate_case(case)
            if identifier in known:
                raise SystemExit("duplicate conformance case ID: " + identifier)
            known.add(identifier)
            if not selected or identifier in selected:
                work.append((case, index))
    if unknown := selected - known:
        raise SystemExit("unknown conformance case IDs: " + ", ".join(sorted(unknown)))

    def execute(entry):
        case, index = entry
        return run_case(
            case,
            index,
            compiler,
            runtime,
            arguments.node,
            windows_node,
            arguments.engine,
            packaged=arguments.package,
            formatted=arguments.format_sources,
        )

    if not work:
        raise ValueError("no conformance cases selected")
    failures = []
    count = len(work)
    with ThreadPoolExecutor(max_workers=arguments.jobs) as executor:
        # map preserves case order even when workers finish in a different order.
        for (case, _), failure in zip(work, executor.map(execute, work)):
            print(
                ("FAIL " if failure is not None else "PASS ")
                + case["id"]
                + (": " + failure if failure is not None else ""),
                flush=True,
            )
            if failure is not None:
                failures.append(case["id"])
    if failures:
        raise SystemExit(f"{len(failures)} of {count} conformance cases failed")
    print(f"{count} conformance cases passed")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, TypeError, ValueError, OSError) as error:
        raise SystemExit(f"conformance configuration failed: {error}") from error
