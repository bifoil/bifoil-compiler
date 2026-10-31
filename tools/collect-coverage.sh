#!/usr/bin/env bash
# Collect compiler and tooling coverage before running sonar-scanner.
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

report_directory=.bifoil/development/sonar
mkdir -p "$report_directory"
# A failed collection must not leave an earlier report available to the scanner.
rm -f "$report_directory/rust.lcov" "$report_directory/python-coverage.xml"

cargo llvm-cov --version
command -v uv >/dev/null
temporary_directory="$(mktemp -d "$report_directory/coverage.XXXXXX")"
trap 'rm -rf -- "$temporary_directory"' EXIT

# Retain coverage mappings despite the production profile's stripped binaries.
export CARGO_PROFILE_RELEASE_DEBUG=2
export CARGO_PROFILE_RELEASE_STRIP=none
cargo llvm-cov --release --locked --all-targets --remap-path-prefix \
    --lcov --output-path "$temporary_directory/rust.lcov" -- --test-threads=2

# The conformance-runner tests execute Python subprocesses; include those runs.
coverage_command=(uv run --no-project --with coverage==7.17.0 --python python3 python -m coverage)
"${coverage_command[@]}" erase
"${coverage_command[@]}" run -m unittest discover -s tools -p 'test_*.py'
"${coverage_command[@]}" combine
"${coverage_command[@]}" xml -o "$temporary_directory/python-coverage.xml"

mv "$temporary_directory/rust.lcov" "$report_directory/rust.lcov"
mv "$temporary_directory/python-coverage.xml" "$report_directory/python-coverage.xml"
printf 'Coverage reports ready in %s\n' "$report_directory"
