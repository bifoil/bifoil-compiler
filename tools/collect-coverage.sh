#!/usr/bin/env bash
# Collect compiler and tooling coverage before running sonar-scanner.
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

if [[ $# -ne 3 ]]; then
    printf 'Usage: %s LANGUAGE_ROOT RUNTIME_ROOT CHOICESCRIPT_SCENE_JS\n' "$0" >&2
    exit 64
fi
language_root="$(realpath -- "$1")"
runtime_root="$(realpath -- "$2")"
engine="$(realpath -- "$3")"
[[ -d "$language_root/conformance" && -f "$runtime_root/tools/execute-conformance.mjs" && -f "$engine" ]]
command -v node >/dev/null

report_directory=.bifoil/development/sonar
mkdir -p "$report_directory"
# A failed collection must not leave an earlier report available to the scanner.
rm -f "$report_directory/rust.lcov" "$report_directory/python-coverage.xml"

cargo llvm-cov --version
command -v uv >/dev/null
temporary_directory="$(mktemp -d "$report_directory/coverage.XXXXXX")"
trap 'rm -rf -- "$temporary_directory"' EXIT

# Resource-observer tests use the ordinary compiler, without LLVM instrumentation.
cargo build --release --locked

# Retain coverage mappings despite the production profile's stripped binaries.
export CARGO_PROFILE_RELEASE_DEBUG=2
export CARGO_PROFILE_RELEASE_STRIP=none
# Rust 1.99.1 exposes branch counters through an unstable instrumentation flag.
# Scope this opt-in to the coverage observer; the production build above stays stable.
RUSTC_BOOTSTRAP=1 cargo llvm-cov --branch --release --locked --all-targets --remap-path-prefix \
    --lcov --output-path "$temporary_directory/rust.lcov" -- --test-threads=2

# Exercise the published language corpus with the same instrumented compiler.
# Dependencies are explicit arguments, never inferred from sibling checkouts.
case_indexes=()
while IFS= read -r -d '' index; do
    case_indexes+=("$index")
done < <(find "$language_root/conformance" -maxdepth 1 -type f -name '*.json' ! -name steering.json -print0 | sort -z)
if [[ ${#case_indexes[@]} -eq 0 ]]; then
    printf 'No conformance case indexes found.\n' >&2
    exit 1
fi
LLVM_PROFILE_FILE="$repository_root/target/llvm-cov-target/conformance-%4m.profraw" \
    python3 tools/run-conformance.py \
    --compiler "$repository_root/target/llvm-cov-target/release/bifoilc" \
    --runtime "$runtime_root" --node "$(command -v node)" --engine "$engine" \
    --cases "${case_indexes[@]}" --jobs 4
cargo llvm-cov report --release --remap-path-prefix --lcov --output-path "$temporary_directory/rust.lcov"

# The conformance-runner tests execute Python subprocesses; include those runs.
coverage_command=(uv run --no-project --with coverage==7.17.0 --python python3 python -m coverage)
"${coverage_command[@]}" erase
BIFOIL_TEST_COMPILER="$repository_root/target/release/bifoilc" \
    "${coverage_command[@]}" run -m unittest discover -s tools -p 'test_*.py'
"${coverage_command[@]}" combine
"${coverage_command[@]}" xml -o "$temporary_directory/python-coverage.xml"

mv "$temporary_directory/rust.lcov" "$report_directory/rust.lcov"
mv "$temporary_directory/python-coverage.xml" "$report_directory/python-coverage.xml"
printf 'Coverage reports ready in %s\n' "$report_directory"
