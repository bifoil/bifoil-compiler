# Bifoil Compiler

A native Rust ahead-of-time compiler maintained by **chinay** for the Bifoil Foundation.

The compiler reads Bifoil source, constructs an AST, checks the supported static rules, and emits typed instructions
embedded in extended ChoiceScript. It also produces independently compiled module artifacts. The current development
profile is `bifoil-vm.v2`.

Full implementation of the language steering record is in progress. This repository does not yet implement the full
specification or establish first-milestone compliance or release readiness. See the [implementation
status](../bifoil-language/implementation-status.md).

## Build and test

Install a Rust toolchain supporting edition 2024:

```sh
cargo test --locked
cargo clippy --locked -- -D warnings
cargo fmt --check
cargo build --release --locked
target/release/bifoilc --help
```

The compiler build uses this repository and its locked Rust dependencies. A neighboring runtime or ChoiceScript checkout
is not required. Building the compiler executable is separate from the language compilation time requirement.

Read [ARCHITECTURE.md](ARCHITECTURE.md) for module responsibilities, ownership, and the maintenance workflow. Lexical
scope snapshots use the locked [rpds](https://github.com/orium/rpds) ordered persistent map; unchanged tree nodes and
entries remain shared. Large temporary function-location indexes use pinned [redb
4.4.0](https://github.com/cberner/redb) with a 4 MiB database cache. The database is private disposable compiler
storage; public modules remain canonical JSON. redb requires Rust 1.90 or later and retains its upstream MIT/Apache-2.0
licensing. Dependency provenance and licenses remain with their upstream authors.

## Format source indentation

```sh
target/release/bifoilc format src/main.bfl
target/release/bifoilc format src/main.bfl --out formatted.bfl
target/release/bifoilc format src/main.bfl --check
```

The default command writes formatted source to stdout. `--out` publishes a completed output file atomically; `--check`
returns failure when formatting would change the source. These options cannot be combined. No manifest or dependency
checkout is required.

The formatter validates syntax, then normalizes code indentation to spaces using the selected four-space/tab level rule.
It preserves alignment spaces, comments, line endings, and both `elif`/`else if` spellings. Prose content and continued
literal/interpolation rows keep their original bytes. It does not wrap lines or rewrite token spacing. Malformed source
and output exceeding the source-byte budget receive diagnostics before output publication.

## Compile a project

A project contains `bifoil.toml` and path-derived source modules under `src/`:

```toml
entry = "main.start"
module_path = "modules"
# Optional unchanged ChoiceScript sources:
legacy_path = "legacy"
```

The manifest uses standard TOML syntax, including literal and basic strings, multiline strings, quoted keys, and inline
comments. `entry` is required; `module_path` and `legacy_path` are optional. All three values must be strings. Unknown
keys, duplicate keys, invalid entry names, and manifests larger than 16 KiB receive a diagnostic without replacing a
completed build. `--module-path` overrides the manifest's module path.

```sh
target/release/bifoilc build examples/checkpoint --out dist/startup.txt
```

The output requires the explicit Bifoil runtime extension. Installation, checkpoint hosting, and instruction formats are
documented in [Bifoil Runtime](../bifoil-runtime/README.md) and its [protocol](../bifoil-runtime/PROTOCOL.md).

## Incremental project builds

Project builds keep disposable compiled modules under `.bifoil/cache/v1/`. Reuse requires the same compiler executable,
identical source bytes, a valid module digest, and unchanged dependency digests. Source hashes detect changes even when
file sizes and timestamps match. Corrupt or obsolete cache records are rebuilt; source-free artifacts retain their
separate validation and failure behavior. The summary reports modules compiled and reused.

```sh
target/release/bifoilc build examples/checkpoint --out dist/startup.txt
target/release/bifoilc build examples/checkpoint --out dist/startup.txt --clean
```

`--clean` bypasses the disposable cache. Clean builds scale with input; warmed incremental builds retain deadline checks
at 9.5 seconds. An unchanged completed build can skip decoding and linking after byte verification of its inputs, module
caches, native source inventory, and existing output. Changed builds still resolve/link the reachable program; the
large-project capacity target remains unmet.

Set `BIFOIL_TRACE_TIMINGS=1` to write elapsed module-loading, compilation, linking, and package-publication phases to
stderr. This diagnostic keeps the ordinary output and deadline checks intact:

```sh
BIFOIL_TRACE_TIMINGS=1 target/release/bifoilc build examples/checkpoint --out dist/startup.txt --package
```

## Compiled code packages

Add `--package` to emit a small `startup.txt` manifest and one content-addressed code scene per module:

```sh
target/release/bifoilc build examples/checkpoint --out dist/startup.txt --package
```

Deploy the root and every code scene named by that root together. This format requires `bifoil.package.v1` support in
Bifoil Runtime; an older adapter cannot execute it. The generated root uses ordinary `*gosub_scene` commands to load
code before starting the compiled story. It contains no Bifoil source to compile during play.

A module's code filename depends on its signatures, instructions, and source locations. Changed dependency digests can
recompile a client without replacing its identical code scene. Completed chunks publish before the root. Large compiler
inputs finish cleanup before the final deadline check and root replacement; failed builds preserve the previous
completed root. Rebuilding repairs missing or damaged referenced chunks. Existing immutable code scenes remain available
for older roots and concurrent builds, so the output directory is a reusable cache. Distribute only the selected root
and its referenced scenes; automatic cache garbage collection is not implemented.

Unchanged package builds can reuse a completed build after byte checks of the root, every referenced code scene, source
inputs, module caches, and native source inventory. Missing or changed files invalidate that shortcut. Changed builds
still verify and serialize reachable code; runtime loading remains eager. This reduces rewritten output after an edit
and avoids decoding unchanged builds; large-project indexing and lazy code loading remain unfinished. Inline output
remains the default and retains its existing bytes and build identity. Package builds have their own build identities
and same-build checkpoints.

## Independently compiled modules

The current artifact format is `bifoil.module.v3`. Recompile artifacts produced by earlier compilers from their original
source. Version 3 requires ordered operand checking; a new client cannot repair unchecked bodies stored in an old
artifact. The VM instruction profile remains `bifoil-vm.v2`. Disposable caches automatically invalidate when the
compiler
executable changes.


```sh
target/release/bifoilc module examples/checkpoint/src/helpers.bfl \
  --name helpers --out dist/modules/helpers.bfm.json
target/release/bifoilc build examples/checkpoint \
  --module-path dist/modules --out dist/startup.txt
```

Source takes precedence. If absent, the compiler loads path-derived artifacts such as `pkg/helpers.bfm.json`, with flat
dotted filenames retained as a compatibility fallback. Standalone module compilation infers the source root when the
declared dotted name matches its source path. Pass `--module-path` explicitly when compiling against artifacts outside
that source root. CLI regressions compile a client after removing its dependency's source, verify deterministic output,
and reject corrupted or mismatched artifacts.

Artifacts contain typed lowered code, interfaces, source locations, and dependency identities. Defaults execute in the
compiled callee. Digests detect changes; they do not authenticate a publisher. Treat modules as trusted build inputs.
Canonical hashes and bounded output writes stream through writers without allocating a second complete serialized
program. Function bodies use scoped temporary storage; pending cached parents and completed components do not retain all
their decoded instructions in memory. Artifact input validates and spools one decoded function at a time, retaining
encoded bodies for one module. One cyclic component and declaration metadata can still remain resident. Output
replacement happens only after successful compilation and a completed temporary-file write. After semantic checks and
initialization planning, linked function headers become compact checksums before final output encoding. Stored payloads
retain their byte, instruction-count, and header validation. This reduces late metadata lifetime; reachable declarations
remain resident during compilation.

## Current coverage

Regression and end-to-end cases exercise primitive and nullable types, checked numeric operations, Unicode text,
collections and conversions, ranges and loops, functions and captures, defaults and variadics, lambdas, conditional
expressions, prose directives, choices, suspension, scene visits/transfers, data types, enums, sealed variants, overload
sets, contextual references, recursive capabilities, conservative projected receivers, and selected generic inference,
constraints, and variance rules. Projected receivers expose safe callbacks and covariant container inputs using checked
upper/lower views. Hidden values and invariant mutable inputs remain restricted; defaults, empty variadics, and explicit
nullable inputs retain their safe paths. Shared state initializes through a checked dependency plan before scenes.
Captured initialization and callback publication have separate call-site summaries; ordinary local reads preserve their
existing deferred-initialization behavior. Ready callback, collection, data, and scene-reference writes can establish
captured var initialization; captured stores enforce the same publication checks as other escaping storage.

Proven local helpers can initialize delayed captured val bindings through typed and generic calls, defaults, and
suspension. Helpers follow the protected declarations in the same complete lexical body; forward calls require the
protected cells to exist. Multiple source call sites and selected overloads retain checked frames. Every reachable call
must prove its protected vals still unwritten, and every normal return must initialize them. Compact scalar helpers
require one source call and a unique declaration. References that can expose the writer remain rejected. Conditional,
choice, prose, and loop-body sites qualify. Loop backedges enforce at-most-once writes; while-condition calls retain
their separate repetition restriction.

Value-returning helpers may supply a binding initializer such as val status = initialize(). These calls retain their
frames and result checks; returned closures keep initialized cells alive through supported checkpoints. Block and
expression bodies share the completed-write proof. Supplied arguments run before the protected-state check. Callable
defaults use ordinary selected evaluation and effect/readiness checks, including suspension where permitted; skipped
defaults do not run. Real reads, early returns without writes, extra calls, escapes, and competing writes remain
rejected. Named, member, and indexed assignments, compound updates, and direct returns also qualify. Compound reads
retain their checked value/type before the RHS, including across suspension. Target receivers and keys are part of the
reference inventory and run before RHS arguments. Proven pure, nongeneric, parameterless helpers also expand when an
expression consumes their Unit result; other helpers retain their frames and checks. Calls within ordinary operands,
arguments, collection entries, interpolation, and assignment targets also qualify. Structured expressions, conditional
headers/bodies, prose, choice preparation, and selected option bodies also qualify. For iterables run once; repeated
loop regions and deferred callable bodies retain their separate restrictions.

Typed legacy globals and subroutine/transfer bridges support primitives, plain enums, and nullable forms. Enum values
cross as checked case-name Strings. These paths are exercised against an explicitly supplied unchanged ChoiceScript
interpreter, including independently compiled interfaces. Native choices, page breaks, text input, and number input
preserve checkpoints without replay. See the [boundary
grammar](../bifoil-language/language-specification-draft.md#saves-legacy-boundaries-and-failure).

Circular imports resolve declaration interfaces before lowering bodies. Anchored return inference, mutually recursive
functions, scene visits, nominal fields, generic methods, and aliases have targeted coverage across modules. Shared
initialization cycles remain errors. Cyclic artifacts carry a whole-group fingerprint; distribute every member together.
An edit to any member or external dependency invalidates the group, and mixed artifact versions are rejected.

These feature families have partial coverage. Composite legacy boundaries, wider native UI commands, and broader generic
projection, bounds, inference, and type checking remain unfinished. Conservative dependency analysis may reject safe
initializers whose indirect calls cannot be resolved. The compiler still restricts identifiers to ASCII and permits one
scene declaration per source module. The [steering conformance ledger](../bifoil-language/conformance/steering.json)
does not certify full conformance.

The explicit-path [conformance runner](tools/run-conformance.py) accepts one or more case indexes through `--cases`.
Case programs live as readable `.bfl` files in the language repository. The runner owns and removes its temporary
projects after each case. Legacy cases additionally require `--engine /path/to/choicescript/web/scene.js`. Cases may
request independent module compilation and source removal before the client build, so artifact reuse is exercised
through actual execution. Use `--jobs 4` for bounded parallel case execution; the default is one worker. Run semantic
unit tests with `cargo test --locked --bin bifoilc`. Run deadline-sensitive CLI tests separately using the production
profile: `cargo test --release --locked --test cli -- --test-threads=2`. Selection and index errors are rejected before
any case runs, and output retains index order. Resource benchmarks remain isolated. Run the acceptance-gate process
controls with `python -m unittest discover -s tools -p test_run_conformance.py`; these controls require a POSIX host.
Silent or terminated child processes fail, including formatting and expected-error paths.

Canonical fingerprint serialization uses a fixed 1 KiB hash buffer, with the logical-byte guard checked before
buffering. All 613 native/CLI tests and 1,100 formatted cases in both output modes pass. Source/cache/artifact discovery
retains identical roots within the current module format. Older module formats require regeneration. Validated pending
bodies also skip redundant dependency decoding when neither analysis nor
legacy collection needs them. Current resource observations and their mixed timings are recorded in RESOURCES.md.

## Resources and boundaries

[RESOURCES.md](RESOURCES.md) describes the bounded workload, measured results, and enforcement limits. Measurements are
evidence for that workload and reference host; they do not prove every language feature or operating system.

The sequential `tools/compare-code.py` resource observer compares two compiler executables with `--before` and
`--after`, writing retained measurements under a new `--output` directory. `--workloads` selects control, code, large,
private, or suspend function graphs. It records compiler and observer identities, preserves failures, and compares every
generated code scene and module byte. These controls qualify bounded workloads; the full source-line target remains
open.

The sequential tools/compare-typing.py observer compares scoped initialization workloads with explicit --before,
--after, --output, --control, and --workload arguments. Its output directory must be new. Supply --before-error with an
expected diagnostic when the earlier compiler rejects the new form. It records executable/observer identities, control
bytes, reusable module publication, and source-free inline/package results; failures remain in its report.

Compiler metadata reuses empty parameter/type lists and common immutable result shapes, with normal copy-on-write
specialization and current-header type derivation. Canonical artifacts and existing resource guards remain unchanged.
The [resource measurements](RESOURCES.md#shared-empty-header-and-type-storage) record reduced peak RSS and mixed timing
samples; larger interfaces still need paging for the intended capacity target.

Compiler implementation belongs here; language decisions remain in Bifoil Language and execution belongs in Bifoil
Runtime. No vendored ChoiceScript source is modified or incorporated. Bifoil is independent of Choice of Games and
upstream ChoiceScript. No release, remote, license selection, or third-party relicensing is established here.

## Sonar analysis

[SonarQube Cloud](https://sonarcloud.io/dashboard?id=bifoil_bifoil-compiler) analyzes this public repository.
[Scanner settings](sonar-project.properties) separate original production sources from tests. Run the standard
`sonar-scanner` with `SONAR_TOKEN` supplied through the environment; never save a token in source or command arguments.
Scanner output stays under ignored `.bifoil/development/sonar/`. Use scanner-based analysis with automatic analysis
disabled for this project. A completed analysis and its quality gate are separate from passing local tests.

Install the developer coverage tools once, with `uv` available on PATH:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked --version 0.9.1
```

Collect coverage and Clippy diagnostics before scanning:

```sh
bash tools/collect-coverage.sh
cargo clippy --locked --all-targets --message-format=json -- -D warnings > .bifoil/development/sonar/clippy.jsonl
sonar-scanner
```

The coverage collector runs all Rust unit and CLI tests in the release profile with LLVM instrumentation and two test
threads. Debug information is retained for coverage mappings. Reported source paths are relative to this checkout, so
the LCOV report can also be imported by a scanner running on another host. Coverage collection is a developer check;
instrumented execution is not a resource-performance measurement.

The same command runs the Python tooling tests with pinned Coverage.py through `uv`, including subprocess coverage.
The process-failure controls require a POSIX host. Rust LCOV and Python XML reports stay under the ignored development
directory and are published only after both test runs succeed. Ordinary Rust test sources are omitted by
cargo-llvm-cov's defaults; Python test files are omitted from the production report. Unexecuted production code remains
in the coverage denominator. Scanner settings import both reports; Clippy diagnostics are separate from coverage.

Sonar's default gate evaluates new code. Its passing status alone does not certify overall compiler coverage, full
language conformance, or release readiness. Inspect the measured overall coverage separately.

The 2026-10-31 analysis reports 94.9% Rust line coverage: 16,917 covered source lines out of 17,821. Overall repository
coverage is 83.0%, including the Python tooling and its mostly untested benchmark helpers. This stable-toolchain
workflow collects Rust line coverage; it does not measure Rust branch coverage.

The benchmark tools are local operator commands. The caller selects trusted compiler/Node executables and an output
workspace. Tools pass separate argument entries without a shell, and fixed report names stay below the selected
workspace. These explicit executable and output arguments are part of the tools' interface.
