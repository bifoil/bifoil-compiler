# Compiler resource measurements

The selected contract remains strictly below 250,000,000 bytes for the compiler and helpers, and under ten seconds for
an incremental compile using precompiled modules on reference hardware. Clean compilation scales with total input. This
report covers the Linux development compiler. It does not prove the complete language's eventual performance or every
possible input within the limits.

## Post-audit qualification (2026-10-31)

The repaired release compiler has SHA-256
`2530e8771c9d3978e0648a0f3d8e82bd116fc124811add00fd8fd30a3d4ea8dd`. These observations use the same Ryzen 7 6800H WSL2
host, Windows-mounted filesystem, and native child-RSS observer described below. Builds, tests, and runtime inspection
were isolated from the timed compiler jobs. Module artifacts now use v3; the historical v2/v2 byte comparisons below
are not a claim of identical artifacts across this format change.

| Workload | Functions | Source lines | Edit seconds | Maximum compiler RSS bytes | Clean source-free seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ordinary code | 15,001 | 45,044 | 6.630 | 57,397,248 | 6.488 |
| Exported routines | 81,918 | 164,132 | 9.076 | 86,106,112 | 8.052 |

Every measured incremental operation finishes below ten seconds. Unchanged package reuse takes 1.103 and 1.166 seconds,
respectively. Initial cold population takes 7.310 and 10.319 seconds; clean work scales with total input. Source-free
roots remain byte-identical to the corresponding edited source roots within v3. Peak RSS includes population and
independent module publication, rather than only edits.

Eight original/restored executions check every routine reference, invocation, result, and retained snapshot, with no
replay after restoration. Separate checks create checkpoints from the actual original outputs, accept them against the
same build, and reject them against the actual edited outputs for both workloads. The resource report records compiler
process RSS, not a certification of the unresolved aggregate memory protocol, portable enforcement, or the accepted
tens-to-hundreds-of-millions-of-lines capacity target. Admission guards remain unchanged.

## Supported guards

The compiler runs as one native process and starts no helper processes. It rejects source files above 1 MiB, aggregate
source/artifact input above 64 MiB, more than 512 modules, more than 16,384 functions in one module, more than 81,920
across a program, more than 100,000 lowered instructions in one module, more than 500,000 across a program, output files
above 16 MiB, expressions nested beyond 64, type graphs beyond 32 layers (including callable wrappers), structural
blocks beyond 32, and more than 4,096 statements in one block. Function-signature width uses the source, code, and
output guards; named functions no longer have a separate 32-parameter header limit. The type limit keeps
compiler-produced modules within the bounded JSON loader and is tested by serializing and reloading a maximally nested
callable. Internal canonical hashing rejects serialized expansion above 64 MiB per hash operation; published outputs
retain their 16 MiB limit.

On Linux it also sets its soft virtual-address-space limit to at most 240,000,000 bytes, preserving a stricter existing
limit. Allocation failure is a failed process, with an allocator diagnostic; it cannot publish a successful output. This
is an address-space guard, not a managed-heap setting or a claim about other platforms. macOS/Windows builds have not
been validated and do not yet have corresponding enforcement.

Warmed incremental builds reject work at 9.5 seconds during loading, inference, lowering, and before output replacement.
A cold cache and explicit `--clean` builds do not apply that fixed clean-build deadline. Scheduling and filesystem
stalls can still delay a process; these checks alone are not a hard real-time guarantee.

The capacity target is tens to hundreds of millions of source lines. These small fixed guards do not meet it.
Content-validated module reuse, completed-output reuse, streamed JSON input/output, and disk-backed function bodies are
implemented. Unfinished source ancestors retain paths and hashes rather than buffers or ASTs. Pending cached/artifact
ancestors remain on disk until their import component is checked. Reusable acyclic bodies are summarized and transferred
individually; stale cached bodies are discarded before recompilation. Completed components retain signatures and compact
dependency summaries; final output reads one function at a time. A whole cyclic component, one source AST, and the
reachable declaration metadata can still remain resident. Disk-backed interfaces, larger artifact indexes, lazy runtime
loading, and capacity-scale workloads remain necessary. Raising configured counts would not establish compliance.

## Workload and measurement

`tools/benchmark.py` generates 315,368 source bytes: 4,095 typed functions, each containing ten binary operations, and
one scene. The v2 program contains 4,096 functions and 98,288 instructions; v1 emitted 98,284. It uses no precompiled
dependencies; the clean job includes process startup, reading all source, parsing, static checks, lowering, linking,
SHA-256 build identity, serialization, and completed output replacement.

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/stress
```

A second workload uses --workload shared. It generates 171,989 source bytes with 4,094 explicitly typed shared bindings
in a forward dependency chain, one module bootstrap, and one entry scene. It produces 4,096 functions and 32,760
instructions. This isolates dependency discovery, cycle checks, and stable scheduling from unannotated return-type
inference.

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/stress-shared --workload shared
```

The executable must first be built with `cargo build --release --locked`. Compiler construction, workload generation,
and runtime inspection are outside the measured compilation boundary. The Linux harness requires GNU Time on PATH; its
small native observer starts the compiler and records that child's peak RSS. The compiler has no children to add. A
monotonic wall clock includes the small observer startup/wait overhead. Output pages in the OS cache are not counted as
process RSS. This is explicitly an RSS measurement, distinct from aggregate cgroup memory/commit accounting.

Earlier reports used Python's direct child observation, whose inherited memory floor is retained in their stated
qualifications. The current native observer avoids that floor: with a live 64 MiB parent allocation, a --help check
reported 2,514,944 bytes versus 84,455,424 bytes through direct observation. Exit status and compiler stdout/stderr were
also checked. This validates the observer separation, rather than a language workload or cross-platform resource claim.
Historical numbers below retain their original protocol; only explicitly identified native-observer measurements use the
revised harness.

Reference host: AMD Ryzen 7 6800H, 8 cores / 16 logical CPUs, x86-64 Linux under WSL2. Parsing, checking, lowering, and
linking are sequential; unchanged-build byte verification uses up to four native threads. There is no intentional
competing benchmark. Sources and outputs are on the Windows-mounted filesystem. Output artifacts are removed, but OS
caches are not flushed; the result is a clean project build, not a cold-machine benchmark.

## Observed results

The disk-backed storage/validation revision compiled the function workload from source in 1.313 seconds with 66,879,488
bytes peak RSS. Independently compiling its 8,786,792-byte module took 1.073 seconds / 66,686,976 bytes; linking after
removing the source took 1.634 seconds / 30,945,280 bytes. The source and source-free startup artifacts were
byte-identical. Both linked artifacts loaded 4,096 functions and completed their entry scene.

The shared-dependency workload compiled in 0.571 seconds with 27,271,168 bytes peak RSS and a 3,782,283-byte artifact.
The runtime executed every one of its 4,094 initializers, verified their initialization flags and chain endpoints, and
completed the scene.

`tools/benchmark-precompiled.py` measures source compilation, independent module compilation, and source-free linking as
separate native processes. It checks artifact-byte equality without loading a large parsed program into the parent
before later measurements. The previous 8 MiB aggregate guard rejected this valid maximum-size module; the current 16
MiB guard admits it. That failed measurement is retained alongside successful reports.

```sh
uv run --no-project tools/benchmark-precompiled.py target/release/bifoilc .bifoil/development/precompiled
```

The earlier cyclic-interface revision compiled the v2 function workload in 0.874 seconds wall time with 64,598,016 bytes
peak RSS and an 8,786,911-byte completed artifact. The shared-dependency workload compiled in 0.393 seconds with
24,076,288 bytes peak RSS and a 3,782,283-byte artifact. The runtime executed all 4,094 shared initializers and verified
the first/last dependency-chain values and every initialization flag.

The preceding streamed-artifact revision measured 1.148 seconds / 64,827,392 bytes for the function workload and 0.385
seconds / 24,014,848 bytes for shared initialization. Earlier development revisions measured 0.645 seconds / 80,031,744
bytes and 0.295 seconds / 37,322,752 bytes, respectively. These isolated measurements use the same bounded source
workloads; they are observations, not worst-case guarantees or a general speedup claim.

Older function-workload revisions measured 0.754 seconds / 80,097,280 bytes, 0.606 seconds / 79,872,000 bytes, and 0.661
seconds / 75,874,304 bytes. The v1 implementation measured 0.499 seconds / 67,272,704 bytes, with earlier runs at 0.925
seconds / 67,534,848 bytes and 0.714 seconds / 67,461,120 bytes. Their artifact encodings and compiler revisions differ.
Final native validation used Rust 1.99.1 with locked dependencies.

The first run exposed quadratic scope copying and failed explicitly at the 9.5-second deadline with no successful
artifact. Its full process time was 9.548 seconds and peak RSS was 25,149,440 bytes. Shared immutable symbol scopes
removed that copying. The result above is for the corrected implementation and the identical workload.

Small machine-specific reports remain under ignored `.bifoil/development/reports/`; generated workload projects were
removed after validation and can be regenerated with the harness. Larger grammars, type-inference workloads, dependency
graphs, diagnostic stress, other operating systems, and stricter whole-job accounting still need separate acceptance
work before making a full first-milestone compliance claim.

## Incremental module acceptance

`tools/benchmark-incremental.py` generates 33 modules with 3,201 functions, measures each compiler process separately
with Linux `wait4`, and verifies cache summaries. It runs clean compilation, cache population, unchanged reuse, and a
leaf edit. The leaf edit must rebuild exactly the changed module and its client while reusing the other 31 modules.

```sh
uv run --no-project tools/benchmark-incremental.py target/release/bifoilc .bifoil/development/incremental
```

On the reference host, the disk-backed storage/validation revision measured clean compilation at 0.961 seconds, cache
population at 2.568 seconds, unchanged reuse at 0.297 seconds, and a leaf edit at 1.827 seconds. Each process had
19,320,832 bytes observed peak RSS. Unchanged reuse verified all inputs/cache artifacts, the native inventory, and the
existing output bytes before skipping linking. The leaf edit rebuilt two modules and reused 31.

Linux child peaks can include the observer's inherited memory before exec, so equal low readings can reflect that
measurement floor. They are conservative process peaks, not a separate measurement of the compiler's live heap. These
bounded results pass the selected numeric thresholds for the measured jobs and do not establish capacity at tens to
hundreds of millions of lines.

The earlier cyclic-interface revision measured clean compilation at 1.045 seconds, population at 2.639 seconds,
unchanged reuse at 1.552 seconds, and a leaf edit at 1.578 seconds, with 19,128,320 bytes observed peak RSS. The
preceding streamed-output revision measured 0.896, 2.458, 1.486, and 1.579 seconds, respectively, with 19,144,704 bytes
peak RSS. Report files preserve other intermediate observations; timings are workload-specific measurements.

## Nominal type query acceptance

The nominal workload contains 115,528 source bytes: a data class with 768 defaulted fields, 512 typed functions reading
eight fields each, generated construction/copy code, and one scene. The program contains 515 functions, 20,240
instructions, and 2,058,354 output bytes. It exposes repeated nominal metadata queries instead of approaching the
function-count limit.

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/nominals --workload nominals
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/nominal-incremental
```

Deep-copying the class definition during member lookup took 21.696 seconds / 73,748,480 bytes peak RSS. Shared immutable
catalog entries with copy-on-write method updates compiled identical source in 1.577 seconds / 73,940,992 bytes. Startup
output was byte-identical before and after the change. This measurement demonstrates reduced query cost; it does not
show lower peak RSS.

For the corrected compiler, cache population took 2.278 seconds / 73,990,144 bytes. Unchanged reuse took 0.242 seconds /
21,495,808 bytes, and changing a field's constructor default rebuilt the module in 2.226 seconds / 74,399,744 bytes. The
harness checked that unchanged output matched and the edited default changed the output. Both measured incremental jobs
met the selected numeric bounds. The same platform, timing boundary, and RSS qualifications above apply.

## Import-depth acceptance

`tools/benchmark-discovery.py` generates an acyclic chain of 60 modules with 13,684,487 source bytes, 61 linked
functions, and a 13,713,958-byte output. The optional `--baseline` executable compiles exactly the same project before
the current compiler; the harness requires identical output hashes. Its output directory must not already exist.

```sh
uv run --no-project tools/benchmark-discovery.py target/release/bifoilc .bifoil/development/import-depth
```

Keeping ancestor ASTs took 2.126 seconds / 22,732,800 bytes peak RSS. Deferred parsing with source-hash verification
took 2.975 seconds / 21,667,840 bytes and produced byte-identical output. The linked artifact loaded all 61 functions
and completed its entry scene. The additional source read costs time; this bounded measurement shows only a modest peak
reduction, and the observer's inherited memory floor limits what it can reveal about smaller live allocations.

An earlier 6,844,487-byte version measured 1.742 and 2.676 seconds, with both peaks at the 21,700,608-byte observer
floor. Both reports are retained under ignored development reports. Generated projects and the owned comparison binary
were removed after validation. These jobs do not establish the capacity target or a general performance improvement.

## Branch-analysis acceptance

The branch workload contains 368,615 source bytes: 4,095 typed Boolean functions with two returning branches, plus one
entry scene. It produces 4,096 functions, 53,252 instructions, and 4,604,424 output bytes. Its entry executes opposite
branches of the first and last functions. The comparison harness measures each native process separately and checks
identical output hashes before decoding a large program in the observer.

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/branches --workload branches
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/branch-cache --workload branches
```

Repeatedly copying unchanged global scopes took 22.873 seconds / 52,195,328 bytes peak RSS. Retaining shared scopes and
borrowing bindings during flow joins took 0.745 seconds / 47,759,360 bytes with byte-identical output. The runtime
loaded all 4,096 functions and produced `1|2`.

Cache population took 1.272 seconds / 48,050,176 bytes. Unchanged verified reuse took 0.293 seconds / 22,458,368 bytes;
an actual branch-body edit rebuilt the module in 1.380 seconds / 48,332,800 bytes. The harness checked unchanged and
edited output hashes. The edited runtime produced `3|2`. These measured incremental jobs passed the selected numeric
bounds; all earlier qualifications about hardware, process RSS, and limited workload size still apply.

`benchmark.py` accepts an optional `--baseline` executable to run the identical clean workload before the current
compiler. Reports retain both process measurements and output hashes. Owned comparison binaries and generated projects
were removed after validation; small reports remain in ignored development storage.

## Capture-analysis follow-up

After carrying deferred capture requirements through expressions, scope exits, and loop backedges, the same
4,096-function branch workload populated its cache in 1.016 seconds / 48,025,600 bytes peak RSS. Unchanged reuse took
0.277 seconds / 22,343,680 bytes, and an actual branch edit took 1.443 seconds / 48,443,392 bytes. The cache summaries
and output hashes were checked; the edited artifact independently loaded all functions and produced `3|2`.

The small report is retained as `capture-branches-2026-10-26.json` under the ignored reports directory. Its generated
project was removed after execution. These process-level measurements retain the reference-host and accounting
limitations described above.

## Anonymous capture retention

The original `compact-capture-source-free` conformance source creates two closures sharing one counter after building an
unused 5,000-item list. After collection at its input wait, capture-aware output retained 7 cells, 4 environments, and 1
object, with a 2,673-byte checkpoint. Disabling only the optional capture metadata to exercise the earlier v2 fallback
retained 10 cells, 3 environments, and 2 objects, with a 142,254-byte checkpoint. Both restored stories produced `11|12`
without replaying the factory.

This compares deterministic saved graph sizes rather than process RSS or throughput. The extra compact environment
preserves shared cells while releasing unused enclosing state. The later named-capture case below exercises recursive
functions and forward declarations. Large-project code-loading and memory-accounting requirements remain open. The small
private report is `compact-capture-size-2026-10-26.json`; its generated project was removed after validation.


## Named capture retention

The original `named-capture-source-free` case returns two references to a local function after creating an unused
5,001-element mutable list. Its mutually recursive helpers share one counter. Saving after the factory returns now
retains 8 cells, 5 environments, and 1 object in 3,469 bytes. Removing only optional named-capture metadata to exercise
the older environment-retention fallback retains 11 cells, 3 environments, and 2 objects in 143,182 bytes. Both saved
runs restore and produce `1|2`.

An earlier checkpoint inside the factory occurs before the counter exists. Both paths retain the same 3,203-byte graph
and resume successfully. Capture compaction therefore releases completed scopes without prematurely dropping future
bindings. This is deterministic graph-size evidence, not peak memory or throughput compliance. The private report is
`named-capture-size-2026-10-26.json`; its generated project was removed after validation.


## Guarded return inference

The inference workload contains 368,615 source bytes and 4,096 functions. Its 4,095 ordinary functions infer their
results from an early null guard and a later String-length return. The entry calls both ends of that function set. The
output has 65,537 instructions. Regenerate clean or incremental measurements with:

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/inference --workload inference
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/inference-edits --workload inference
```

On the same Ryzen 7 6800H Linux/WSL reference host, direct lookup of generated method receivers reduced clean
compilation from 4.668 to 1.618 seconds with byte-identical output. Peak compiler RSS was 49,557,504 bytes before and
49,520,640 bytes after. Ordinary result inference no longer searches every declaration to determine whether the current
function has a receiver; local functions continue to inherit an enclosing receiver lexically.

Cache population took 2.122 seconds / 49,528,832 bytes. An unchanged build took 0.360 seconds / 22,454,272 bytes, and a
return-expression edit rebuilt in 2.994 seconds / 49,700,864 bytes. Both completed outputs independently loaded all
4,096 functions and produced `3|0` and `4|0`, respectively. These are isolated process measurements within the existing
small development envelope; they do not establish aggregate-job memory compliance or the large-project capacity target.

The private reports are `inference-comparison-2026-10-26.json` and `inference-incremental-2026-10-26.json`. Generated
projects, the comparison executable, and owned measurement-helper bytecode were removed after validation.


## Shared nested type arguments

The `types` workload contains 176,349 source bytes, 4,096 functions, and 16,405 instructions. It expands an immutable
List alias through sixteen nested layers and uses that type in both parameter and result annotations. Regenerate it
with:

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/types --workload types
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/type-edits --workload types
```

Sharing immutable argument slices reduced clean compiler peak RSS from 77,176,832 to 42,717,184 bytes on the documented
reference host. Clean compilation took 1.976 seconds before and 1.568 seconds after; generated program bytes were
identical. Native module compilation measured 1.160 seconds / 42,606,592 bytes after the change, versus 1.566 seconds /
77,168,640 bytes before. Both produced the same 7,297,852-byte reusable artifact.

The artifact linked without source in 2.086 seconds / 26,693,632 bytes and reproduced the source build exactly. Cache
population measured 2.526 seconds / 42,811,392 bytes; unchanged reuse measured 0.430 seconds / 21,798,912 bytes, and a
nested-literal edit measured 2.613 seconds / 43,159,552 bytes. Independent runtime checks loaded all 4,096 functions and
produced `0|0` before the edit and `1|1` afterward.

These are isolated process observations within the current bounded profile. They do not establish aggregate-job
accounting or the requested large-project capacity. The reports `shared-types-comparison-2026-10-26.json` and
`shared-types-incremental-2026-10-26.json` retain executable/workload checksums. Generated projects, artifacts, the
comparison executable, and owned helper bytecode were removed after validation.

## Immutable field proof workload

The `fields` typing workload has 183,726 source bytes: 32 immutable nullable String fields and 64 functions, each
checking and reading every field. Its 2,048 field guards exercise sparse proof maps and branch joins; generated
construction/copy code and the entry scene bring the program to 67 functions. Regenerate compile/cache measurements
with:

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/field-proofs --workload fields
```

After reassignable-root proof and structural generic-inference changes, cache population took 0.807 seconds, unchanged
reuse took 0.233 seconds, and a return-expression edit rebuilt in 1.008 seconds on the documented reference host. All
three observed child peaks were 21,352,448 bytes, subject to the pre-exec observer floor described above. Independent
execution loaded all 67 functions and produced `64` before the edit and `65` afterward. Every artifact was
byte-identical to the earlier field-analysis revision. Generated projects were removed; the private follow-up report is
`typing-structural-inference-2026-10-26.json`.

The same isolated run repeated the 4,096-function branch and inference workloads. Cache population took 1.194 and 1.748
seconds; edited builds took 1.381 and 2.205 seconds at 48,123,904 and 49,754,112 bytes peak RSS. Unchanged builds took
0.303 and 0.289 seconds. All six artifacts matched the earlier compiler revision byte for byte and executed correctly.
These bounded process observations do not establish aggregate-job accounting or the requested large-project capacity.

## Generic call inference workload

The `generics` workload has 417,688 source bytes, 4,096 functions, and 69,621 instructions. It contains one generic
identity helper, 4,094 generic forwarding functions with String guards, and an entry scene. Guarded calls infer their
arguments against a caller-selected result type. Regenerate clean and incremental measurements with:

```sh
uv run --no-project tools/benchmark.py target/release/bifoilc .bifoil/development/generic-clean --workload generics
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/generic-cache --workload generics
```

On the documented reference host, cache population took 1.968 seconds / 70,336,512 bytes peak child RSS; unchanged reuse
took 0.366 seconds / 21,409,792 bytes. Editing the entry's String argument rebuilt in 2.323 seconds / 71,192,576 bytes.
All three outputs independently loaded the complete function inventory and ran the selected generic calls, producing
`Ada|7` before the edit and `four|7` afterward. Unchanged output was byte-identical; the edit changed its artifact hash.

The buffered-spooling follow-up report is `typing-buffered-code-generics-2026-10-26.json`; all three artifacts matched
the preceding bound-inference revision byte for byte. Generated projects were removed. These are isolated
compiler-process observations within the current bounded profile, subject to the observer floor described above. They do
not establish aggregate-job accounting, every possible generic program, or the requested large-project capacity.

## Iterative dependency traversal

Dependency discovery uses an explicit work stack and one map of discovery marks. Its isolated regression traverses a
100,000-node chain on a 64 KiB thread stack. This tests the graph algorithm; the complete compiler retains the bounded
source, module, instruction, and metadata profile described above.

After this change, the original 60-module, 13.68 MB discovery workload took 3.432 seconds / 22,536,192 bytes observed
peak RSS. The retained comparison executable took 3.179 seconds with the same observed RSS. Both artifacts were
byte-identical, independently loaded all 61 functions, and completed the entry scene. These single observations do not
establish a wall-time improvement; the observer floor limits memory comparisons.

The separate 33-module, 3,201-function workload took 1.528 seconds clean, 3.275 seconds to populate caches, 0.309
seconds unchanged, and 1.892 seconds after a leaf edit. Every phase observed 22,667,264 bytes peak child RSS. Unchanged
reuse retained all 33 modules; the edit rebuilt two and reused 31. All four artifacts executed: the first three produced
0, and the edit produced 1. The first three hashes matched; the edit changed the output.

The private report is `typing-iterative-discovery-2026-10-26.json`. Measurements reused the public discovery and
incremental workload generators and ran without competing builds or tests. Runtime execution occurred after each
compiler measurement. Generated projects and the temporary comparison executable were removed. These remain bounded
compiler-process observations, rather than aggregate-job or large-project capacity acceptance.

## Streamed acyclic artifact reuse

The source-free workload contains 4,096 functions and 98,288 instructions. After per-function transfer, source
compilation took 1.530 seconds / 66,981,888 bytes peak child RSS; independent module compilation took 1.199 seconds /
66,830,336 bytes. Linking without source took 1.877 seconds / 29,954,048 bytes. The retained comparison executable's
source-free phase took 1.741 seconds / 30,822,400 bytes. These are single observations; they do not establish a general
speed or memory improvement.

The 33-module workload took 1.638 seconds clean, 3.345 seconds to populate caches, 0.318 seconds unchanged, and 1.847
seconds after a leaf edit. Every phase observed 23,490,560 bytes peak RSS, subject to the inherited observer floor.
Unchanged builds reused 33 modules, while the edit rebuilt two and reused 31.

Every phase matched the previous compiler's output bytes. All measured startup artifacts were executed; module artifacts
were inspected and then linked without source. Source-free startups loaded all 4,096 functions and 98,288 instructions
and completed the entry scene. Incremental artifacts loaded 3,201 functions and produced 0 before the edit and 1
afterward. Source, independent-module preparation, and source-free linking are reported separately.

The private report is `typing-streamed-reuse-2026-10-26.json`. Measurements reused the public precompiled and
incremental workload generators and ran without competing builds or tests. Generated projects and the temporary
comparison executable were removed. Initial JSON decoding and cyclic validation still materialize active bodies, so this
does not complete streaming input, disk-backed interfaces, or the requested large-project capacity.

## Cyclic cache rebuild workload

`tools/benchmark-cyclic.py` creates a two-module import cycle with 4,096 functions, 315,454 source bytes, and 98,297
instructions. Function signatures are explicit, and the scene calls one function from each module. A body edit changes
one result without changing its signature. Regenerate measurements in a fresh directory with:

```sh
uv run --no-project tools/benchmark-cyclic.py target/release/bifoilc .bifoil/development/cyclic-cache
```

On the reference host, releasing obsolete cached bodies before rebuilding reduced the edited phase's observed peak RSS
from 70,455,296 to 57,561,088 bytes. The edited build took 2.832 seconds, compared with 2.612 seconds for the retained
comparison executable. Clean compilation took 1.503 seconds / 56,823,808 bytes; cache population took 1.989 seconds /
57,085,952 bytes. Unchanged reuse took 0.404 seconds / 23,019,520 bytes and reused both modules. The edit rebuilt both
members of the component.

All eight measured startups loaded the complete function/instruction inventory and executed. The initial phases produced
10|10 and the edited phase produced 11|10. Every phase matched the preceding compiler's bytes. The private report is
`typing-cyclic-release-2026-10-26.json`; generated projects and the temporary comparison executable were removed. These
isolated observations demonstrate this bounded workload's retention change, subject to the observer floor. They do not
establish a wall-time improvement, worst-case bounds, or the requested large-project capacity.

## Encoded artifact input

The earlier artifact-input revision retained encoded body strings and decoded one function at a time for hashing,
validation, and pending storage. The same public precompiled, incremental, and cyclic workload generators were compared
with the preceding compiler on the reference host, without competing builds or tests.

For the 4,096-function / 98,288-instruction source-free workload, observed peak child RSS fell from 29,872,128 to
23,461,888 bytes. Linking took 1.885 seconds versus 1.671 seconds. Source compilation took 1.511 seconds / 67,006,464
bytes, and independent module preparation took 1.245 seconds / 66,617,344 bytes. This trades extra decoding for reduced
body retention; the inherited observer floor limits the measurable memory reduction.

The 33-module / 3,201-function workload took 1.207 seconds clean, 2.747 seconds to populate caches, 0.297 seconds
unchanged, and 1.788 seconds after a leaf edit. Every phase observed 23,461,888 bytes peak RSS. Unchanged builds reused
33 modules; the edit rebuilt two and reused 31. The two-module cyclic workload took 1.725 seconds clean, 2.046 seconds
to populate caches, 0.347 seconds unchanged, and 2.671 seconds after its member edit. Edited peak RSS was 57,901,056
bytes; whole-component validation and replacement compilation retain their existing memory requirements.

All 22 measured artifacts were byte-identical to the retained comparison executable's corresponding phase. All 20
startup artifacts loaded their full function/instruction inventories and executed correctly. The two independent module
artifacts were inspected before their source-free links. The private report is `typing-encoded-input-2026-10-26.json`.
Generated projects, the temporary measurement harness, and the comparison executable were removed. These bounded
observations do not establish fully streaming input, worst-case limits, aggregate cross-platform compliance, or the
requested large-project capacity.

## Stable map-key proof follow-up

After typed List/Map proof paths and normalized primitive keys, the existing 67-function field workload with 2,048
guards populated its cache in 0.994 seconds, reused the unchanged build in 0.250 seconds, and rebuilt its edit in 1.030
seconds. Observed child peak RSS was 22,536,192 bytes initially and 22,798,336 bytes after the edit. The retained
comparison executable took 0.913, 0.249, and 1.133 seconds respectively, with observed peak RSS up to 22,405,120 bytes.
These single observations, subject to the inherited observer floor, establish neither a speed nor a memory improvement.

All six artifacts matched corresponding output bytes across revisions, loaded all 67 functions, and executed the
expected 64 before the edit and 65 afterward. The private report is `typing-map-proof-fields-2026-10-27.json`.
Measurements used the public `benchmark-typing.py --workload fields` generator without competing tests/builds. Generated
projects, the temporary measurement harness, and the comparison executable were removed. Map-specific semantics and
checkpoint restoration have separate conformance cases; this workload checks the shared field path.

## Direct UTF-8 tokenization workload

`tools/benchmark-text.py` creates one 900,000-byte literal in 900,160 source bytes, with three functions and 36 linked
instructions. It measures clean compilation, cache population, unchanged reuse, and a same-size suffix edit. The ASCII
literal contains 900,000 scalar values; the emoji variant contains 225,006, including its ASCII prefix/suffix. Reproduce
in fresh directories with:

```sh
uv run --no-project tools/benchmark-text.py target/release/bifoilc .bifoil/development/text-ascii
uv run --no-project tools/benchmark-text.py target/release/bifoilc .bifoil/development/text-emoji --alphabet emoji
```

Native-observer measurements compared the new byte scanner with the preceding character/offset-array scanner. The ASCII
clean build observed 8,982,528 bytes peak RSS versus 16,793,600 bytes. After the literal edit, peak RSS was 9,326,592
versus 17,784,832 bytes; compilation took 0.416 versus 0.501 seconds. The unchanged build took 0.192 seconds / 3,710,976
bytes. The emoji clean build observed 8,990,720 versus 9,338,880 bytes; its edited build took 0.437 seconds / 9,347,072
bytes, compared with 0.419 seconds / 9,596,928 bytes. These single observations demonstrate a bounded allocation change
without establishing a general speed improvement.

All sixteen artifacts matched the corresponding comparison compiler's bytes, loaded all three functions and 36
instructions, and executed the expected scalar count, START prefix, and END or FIN suffix. The private report is
`typing-byte-lexer-2026-10-27.json`; measurement ran without competing builds or tests. Generated projects, the
temporary inspection harness, and the comparison executable were removed. Source buffers, tokens, and the active AST
still have their documented bounds; this does not establish the requested large-project capacity.

## Transitive wrapper dependency workload

The captures typing workload declares 4,090 local wrappers in one scene, plus a defaulted reader and the scene itself:
4,092 functions and 24,563 instructions. Each wrapper references its predecessor. The compiler checks the full chain;
runtime execution calls only the first wrapper, keeping compiler graph depth separate from the runtime call-depth
budget. The shape stays within the existing block-statement and function guards. Reproduce in a fresh directory with:

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc .bifoil/development/capture-graph --workload captures
```

In the isolated native-observer comparison, bounded empty-graph memoization reduced cache population from 7.288 to 1.199
seconds. The edited build took 1.399 seconds / 29,966,336 bytes peak child RSS, versus 6.396 seconds / 29,810,688 bytes.
Unchanged byte-verified reuse took 0.228 seconds / 3,801,088 bytes. This improves the measured repeated-prefix case
while retaining ordinary checks for variable-dependent graphs.

Every corresponding artifact was byte-identical across compilers. All six startups loaded all 4,092 functions and 24,563
instructions and produced 7 initially and 8 after the edit. The private paired report is
`typing-empty-dependencies-2026-10-27.json`; `typing-capture-graph-2026-10-27.json` records the earlier unpaired run.
Generated projects, both temporary inspection helpers, and the comparison executable were removed. These bounded
compiler observations do not establish larger project capacity, deep runtime recursion, worst-case deadlines, or
aggregate cross-platform compliance.

After adding separate call-publication summaries, the same 4,092-function workload measured 1.313 seconds / 33,964,032
bytes for cache population, 0.271 seconds / 3,850,240 bytes for unchanged reuse, and 1.606 seconds / 34,914,304 bytes
after the call edit. Both executed programs still produced 7 and 8 respectively. The isolated report is
`call-publication-capture-graph-2026-10-27.json`; its temporary project was removed. This is a current bounded
observation, rather than a new paired performance comparison.

The later lexical-key identity check preserved every corresponding program byte in a separate paired run. Cache
population measured 1.280 seconds / 33,923,072 bytes before and 1.296 seconds / 33,865,728 bytes after; edited builds
measured 1.495 seconds / 34,721,792 bytes and 1.594 seconds / 34,902,016 bytes. Both initial and edited programs
executed correctly. `dynamic-keys-comparison-2026-10-27.json` records the measurements and a previously rejected
immutable-key program now producing its expected result. Its temporary projects and comparison executable were removed.
These observations establish artifact stability and bounded workload overhead, rather than a performance improvement.

## Streamed published module input

Published module JSON now decodes each function into a private instruction spool during input. Headers and spool handles
remain resident; hashing and validation read one body at a time. The format and canonical bytes are unchanged. Failed
parses release the private spool before any validated body enters shared pending storage. Disposable cache input still
uses encoded body strings, and cyclic fingerprinting still restores its active component.

An isolated paired observation used the existing 4,096-function / 98,288-instruction workload and the GNU Time native
observer described above. Source-free linking changed from 1.999 seconds / 15,683,584 bytes peak compiler RSS to 2.418
seconds / 8,720,384 bytes. The lower body retention trades additional temporary-file I/O for memory. These single
observations establish the result for this workload rather than a general throughput claim.

Source compilation changed from 1.516 seconds / 66,994,176 bytes to 1.723 seconds / 67,006,464 bytes. Independent module
preparation changed from 1.191 seconds / 66,998,272 bytes to 1.217 seconds / 66,732,032 bytes. Every phase succeeded.
Complete module and linked-program bytes matched across compilers; both module/program inventories contained the stated
function and instruction counts. Both linked programs passed actual runtime validation and execution.

The private report is `typing-streamed-input-2026-10-27.json`. Paired workload projects, the comparison script, and the
obsolete comparison executable were removed after inspection. This remains a bounded workload; disk-backed declaration
interfaces, lazy runtime code loading, broader platform accounting, and the requested large-project capacity remain
unfinished.

## Streamed cached dependency input

Disposable module-cache envelopes now use the same private spooling reader as published artifacts. Decoded bodies do not
accumulate in memory during cache input. Malformed input is discarded before shared pending code changes; private-spool
and deadline failures stop compilation. The previous RawValue body wrapper and its dependency feature were removed.
Published and cached module formats remain unchanged.

[The cached-dependency workload](tools/benchmark-cache-import.py) contains two modules, 4,096 functions, and 98,293
instructions. A small scene imports one function from a 4,095-function dependency. Editing only the scene must rebuild
that scene and reuse the dependency; the unchanged phase reuses both modules. Each observation used the native GNU Time
protocol above, without concurrent builds, tests, or conformance runs.

In an isolated pair, the scene edit changed from 2.486 seconds / 16,719,872 bytes peak compiler RSS to 2.806 seconds /
10,272,768 bytes. Unchanged reuse measured 0.403 seconds / 3,981,312 bytes before and 0.393 seconds / 3,846,144 bytes
afterward. These are single observations for this workload, including the additional temporary-file I/O, rather than a
general throughput claim.

Clean compilation measured 1.632 seconds / 67,006,464 bytes before and 1.568 seconds / 67,002,368 bytes afterward.
Population measured 1.990 seconds / 67,014,656 bytes before and 1.953 seconds / 67,014,656 bytes afterward. All phases
succeeded within the stated checks. Source and program hashes matched across compilers in every phase. Both final
programs had the stated inventories and executed to `10|FIN` with the actual runtime.

The private report is `typing-streamed-cache-2026-10-27.json`. Paired projects, the comparison script, and its
comparison executable were removed after inspection. Disk-backed declaration interfaces, active-cycle retention, lazy
runtime loading, cross-platform accounting, and the full large-project capacity remain unfinished.

## Shared nominal declaration metadata

Completed modules, linked programs, and analysis catalogs now share immutable nominal records. Inferred method updates
use copy on write; a nested-analysis regression checks that published and enclosing declarations remain unchanged. The
artifact representation and canonical field order are unchanged.

The nominal-import workload contains two modules and 490,679 source bytes. A data class declares 1,024 methods with 32
List<Int> parameters each, plus its generated constructor and shallow-copy method. The linked program has 1,027
functions. A small importing scene constructs the class and reports one field. Its edit changes only the final prose.

```sh
uv run --no-project tools/benchmark-cache-import.py target/release/bifoilc \
  .bifoil/development/nominal-import --workload nominal-import --keep-original
```

An isolated sequential pair used the native GNU Time observer on the reference host. Clean compilation measured 1.504
seconds / 59,531,264 bytes peak RSS before sharing and 0.968 seconds / 55,046,144 bytes afterward. A scene edit reusing
the unchanged dependency measured 1.796 seconds / 36,675,584 bytes before and 1.733 seconds / 22,130,688 bytes
afterward. Unchanged reuse took 0.375 and 0.376 seconds, with peak RSS below 4 MB in both runs. These are individual
observations with warm filesystem caches, not a general throughput or maximum-capacity guarantee.

Corresponding source hashes and program bytes matched for all eight measured phases. Published module bytes also
matched; linking after removing the dependency source preserved the edited program bytes. All six retained original,
edited, and source-free programs independently loaded 1,027 functions, checked the declared/generated method inventory,
and produced the expected output. Both generated projects and the comparison executable were removed after verification.
The optional --keep-original flag retains the clean artifact for this independent execution check.

This reduces resident copies of declaration records. It does not replace the remaining metadata with a disk-backed index
or satisfy the requested scale of tens to hundreds of millions of lines.

## Shared alias graph acceptance

The alias workload repeats Map arguments through 28 private alias levels while its scene uses only the scalar leaf.
Successful resolution and validation retain the graph rather than expanding it into a complete tree. A separate
2,048-alias chain checks shared lexical name scopes. Nullable alias application and invalid mutable map keys retain
semantic regressions, including source-free interfaces.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/alias-graphs --workload aliases --keep-original
```

An isolated sequential comparison used the native GNU Time observer on the reference host. The identical 16-level source
took 2.182 seconds / 30,486,528 bytes peak compiler RSS before the change and 0.228 seconds / 4,620,288 bytes afterward.
At 12 levels, it took 0.241 seconds / 6,369,280 bytes before and 0.195 seconds / 4,661,248 bytes afterward. Program
bytes matched across both pairs, and all four programs executed correctly in the actual runtime.

The corrected 28-level workload populated its cache in 0.279 seconds / 4,882,432 bytes, reused unchanged output in 0.155
seconds / 3,448,832 bytes, and rebuilt a leaf edit in 0.322 seconds / 4,812,800 bytes. Independently executed original
and edited programs produced true and false respectively; saved source and program hashes matched the reports. The
earlier 614-byte, 20-level timeout input compiled in 0.179 seconds / 4,558,848 bytes after the change. An oversized
export failed with a canonical-size diagnostic while preserving and successfully executing the previous output.

These are individual observations for bounded private graphs, rather than general throughput or capacity guarantees.
Public type representations still serialize as ordinary arrays and must fit existing guards. Generated projects and the
comparison executable were removed after verification; the small private comparison report is
`alias-graphs-comparison-2026-10-27.json`.

## Shared type comparison and diagnostic acceptance

Two independent repeated Map alias graphs exercise generic-bound comparison without publishing their expanded types. The
valid input has 28 levels and 1,746 source bytes; an invalid Float/Int bound has 24 levels and 1,508 bytes. Both
previously exceeded a strictly bounded four-second probe. Structural equality now reuses completed node pairs, and type
rendering stops after 4,096 UTF-8 bytes plus one ellipsis per type.

Using the native GNU Time observer, the valid input compiled in 0.185 seconds / 4,636,672 bytes peak compiler RSS. The
invalid input failed in 0.139 seconds / 4,177,920 bytes with an 8,239-byte source diagnostic identifying the unsatisfied
bound. Smaller paired valid inputs preserved complete program bytes and executed correctly. The private reports are
`type-comparison-before-2026-10-27.json` and `type-comparison-after-2026-10-27.json`; temporary projects and the
comparison executable were removed. These isolated observations do not certify all type relationships or large-project
capacity.

## Generic alias and covariant query acceptance

A 28-level private generic Entry alias graph uses its parameter at every leaf; a final application selects Int. Its
1,190 source bytes compiled in 0.178 seconds / 4,972,544 bytes peak compiler RSS. The earlier 951-byte, 24-level Map
input, which previously exceeded a four-second probe, now reports its invalid unconstrained key type in 0.152 seconds /
4,210,688 bytes. It remains invalid: sharing work does not grant its parameter StableKey eligibility.

A separate 1,858-byte input compares two 28-level Entry graphs whose leaves have a valid Int-to-Any subtype relation. It
previously exceeded a four-second probe and now compiles in 0.179 seconds / 4,825,088 bytes. Completed subtype pairs are
retained only during that relationship query, including ownership of temporarily substituted arguments.

Smaller sequential pairs used identical sources before and after the change. Generic alias compilation measured 0.183
seconds / 7,135,232 bytes before and 0.209 seconds / 4,550,656 bytes afterward. The 16-level covariance pair measured
0.194 seconds in both runs, with peak RSS below 4.7 MB. Corresponding program bytes matched and all six valid
paired/larger programs executed correctly. These measurements use the native GNU Time observer and do not establish a
general speedup or maximum capacity. Temporary projects and the comparison executable were removed; the private report
is `type-queries-comparison-2026-10-27.json`.

## Repeated generic call constraints

A 1,325-byte source applies a generic function to an empty Map typed through 24 repeated generic alias levels. Before
constraint sharing, an isolated direct compiler probe exceeded four seconds. Completed structural inference pairs now
prevent repeated constraints from unfolding the same graph. The resulting expanded interface still exceeds the existing
artifact budget, so this input correctly fails compilation rather than producing a successful oversized program.

The native GNU Time observer measured that rejection at 0.963 seconds / 4,902,912 bytes peak compiler RSS. Its previous
completed output remained byte-identical and independently executed correctly. A valid 12-level pair measured 0.297
seconds / 11,948,032 bytes before and 0.344 seconds / 11,968,512 bytes afterward; program bytes matched and both
programs produced 0. These are isolated observations, including a small-input overhead, rather than a general speedup
claim. Temporary projects and the comparison executable were removed; the private report is
`inference-graphs-comparison-2026-10-27.json`.

A separate source-free checkpoint regression retains Int type arguments in an empty repeated Map and distinguishes a
Float specialization at runtime. Native regressions also preserve widening across independent arguments, incompatible
constraint rejection, and distinct parameter bindings.

## Shared generic alias name scopes

Generic aliases retain one shared lexical name map plus their own parameter bindings. The reproducible alias-scopes
workload declares 2,049 generic aliases in a simple chain and uses its final specialization in one scene. Editing the
specialization changes an exact-type comparison from true to false; all templates remain independently checked.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/generic-alias-scopes --workload alias-scopes --keep-original
```

An isolated native GNU Time comparison at 512 aliases measured 0.303 seconds / 46,497,792 bytes peak compiler RSS before
sharing and 0.218 seconds / 6,168,576 bytes afterward. At 256 aliases, it measured 0.197 seconds / 15,380,480 bytes
before and 0.214 seconds / 5,427,200 bytes afterward. Program bytes matched for both pairs, and all four programs
executed correctly. These individual observations include a small-case time overhead and do not imply a general speedup.

The earlier 59,318-byte, 2,048-level timeout source compiled in 0.299 seconds / 10,371,072 bytes after the change and
executed to 3. The incremental workload populated in 0.337 seconds / 10,588,160 bytes, reused unchanged output in 0.161
seconds / 3,665,920 bytes, and rebuilt its specialization edit in 0.354 seconds / 11,034,624 bytes. Original and edited
artifacts independently executed to true and false; their hashes matched the per-phase report.

The private comparison report is `generic-scopes-comparison-2026-10-27.json`. Generated projects and the comparison
executable were removed after verification. Source-free returned callbacks retain distinct enclosing generic arguments
through a checkpoint, and shadowing/duplicate-parameter diagnostics remain covered. This removes quadratic name-map
retention for aliases; disk-backed interfaces and the full requested large-project capacity remain unfinished.

## Captured primitive initialization proofs

A known local helper can establish initialization of captured primitive vars when every normal return follows its
writes. Proofs cover structured branches, loops, early returns, defaults, and suspended choices. The original
measurements below covered primitive cells; the later callback workload extends the same analysis to ready structured
values. Captured vals, unknown function values, and incomplete summaries retain conservative checks. Straight-line
helpers keep two bitsets; other helpers keep facts per basic block, with a bounded optional analysis and incremental
deadline checks.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/capture-initializers --workload capture-initializers --keep-original
```

The workload contains 159,708 source bytes and 5,706 lines: 1,900 captured Int vars, one helper assigning eight-term
expressions, and a caller that reads every initialized value. Its two functions contain 39,919 instructions and emit
3,295,663 bytes. Each source block stays within the existing statement guard. A separate native test checks 3,000-cell
helpers with long straight-line and branching assignments near the instruction limit.

An isolated GNU Time pair compared per-instruction fact storage with the compact implementation. Population measured
1.190 seconds / 31,178,752 bytes peak compiler RSS before and 0.931 seconds / 24,891,392 bytes afterward. Unchanged
reuse took 0.310 seconds / 3,305,472 bytes before and 0.246 seconds / 3,641,344 bytes afterward. An edited store rebuilt
in 1.082 seconds / 31,899,648 bytes before and 1.327 seconds / 27,074,560 bytes afterward. These are individual
observations, including an edited-build time increase, rather than a general speedup or capacity guarantee.

All three corresponding source/program hashes matched across compilers. Four independently executed original and edited
artifacts produced 15200 and 15201 respectively. Generated projects and the comparison executable were removed; the
private report is `capture-initializers-comparison-2026-10-27.json`. Source-free choice regressions save before
assigning the captured cells and resume with their values intact.

## Mutable key invalidation overhead

A paired 1,900-cell assignment workload checks that lookup-key invalidation does not scan unrelated facts on ordinary
stores. Population measured 0.871 seconds / 25,141,248 bytes peak compiler RSS before and 0.836 seconds / 25,325,568
bytes after. Unchanged reuse measured 0.259 seconds / 3,710,976 bytes and 0.261 seconds / 3,731,456 bytes; edited builds
measured 2.802 seconds / 27,525,120 bytes and 1.021 seconds / 27,525,120 bytes. These are individual observations, not a
general speedup claim.

All corresponding artifacts were byte-identical, and four executed programs produced the expected 15200/15201 results.
An unchanged mutable-key guard that previously failed compilation now produced 3. The private report is
`mutable-keys-comparison-2026-10-27.json`; temporary projects and the comparison executable were removed. This verifies
bounded workload overhead and artifact stability without establishing the large-project capacity target.

## Indexed shared-alias assignment facts

The `shared-writes` workload updates 4,090 independent shared Int cells, then reads every updated value into a sum. Its
289,367 source bytes and 12,276 lines produce 4,092 functions, 57,289 instructions, and 5,520,408 output bytes. The two
large scene blocks remain below the existing statement guard.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/shared-writes --workload shared-writes --keep-original
```

An isolated native-observer pair measured population at 1.395 seconds / 31,731,712 bytes peak compiler RSS before and
1.391 seconds / 32,002,048 bytes after indexing shared aliases. Unchanged reuse measured 0.365 seconds / 3,420,160 bytes
and 0.417 seconds / 3,747,840 bytes; editing the last store measured 1.714 seconds / 32,190,464 bytes and 1.780 seconds
/ 32,493,568 bytes. These observations establish bounded workload overhead rather than a speedup or a larger capacity
guarantee.

Every corresponding artifact matched, and four executions checked all writes through their 8366095/8366096 sums. The
original unsound shared-alias program now failed compilation without replacing existing output. The private report is
`global-aliases-comparison-2026-10-27.json`; owned projects and the comparison compiler were removed.

## Typed enum paths and safe receiver proofs

A 551-byte original program traverses fourteen nested immutable Maps using expected-type enum keys and checked
unwrapping. It previously exceeded a four-second bounded probe. Resolving enum shorthand while walking the typed path
removed repeated receiver inference; the same program compiled in 0.350 seconds / 5,193,728 bytes peak compiler RSS and
executed to its expected result. This observation concerns that input, not every possible path or a capacity claim.

A smaller six-level pair preserved complete artifact bytes and produced the same result. Independent safe-field and
shared-alias read examples that previously failed compilation now compiled in 0.379 seconds / 4,980,736 bytes and 0.429
seconds / 4,657,152 bytes respectively and executed correctly. The private report is
`safe-paths-comparison-2026-10-27.json`; owned projects and the comparison executable were removed.

## Captured callback initialization workload

A known helper initializes 4,090 captured callback vars, and its caller invokes every callback and sums the results.
This workload has 326,203 source bytes, 12,277 lines, 4,092 compiled functions, and a 5,290,345-byte program. Its source
blocks fit the existing statement guard. Publication checks reject captured stores that could expose uninitialized
callback dependencies; successful overwrites retire the prior value's deferred requirements.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/callback-initializers --workload callback-initializers --keep-original
```

Isolated GNU Time measurements recorded population at 1.642 seconds / 39,395,328 bytes peak compiler RSS, unchanged
verified reuse at 0.310 seconds / 3,592,192 bytes, and an edited callback rebuild at 1.968 seconds / 40,009,728 bytes.
Unchanged output was identical; original and edited programs independently executed every callback and produced 8362005
and 8362006. The private report is `callback-initializers-2026-10-27.json`; the generated project was removed afterward.
These are observations for this bounded workload and do not establish the requested large-project capacity.

A separate captured-writer regression previously compiled and failed on an uninitialized binding at runtime. It now
receives a source-level diagnostic and preserves an existing completed output. Source-free module cases also restore
suspended callback initialization, scene references, and structured values without replay. Earlier snapshots and skipped
initializer calls retain their original dependencies and remain rejected when unsafe.

## Narrowed shared result graphs

A 1,870-byte source defines two 28-level covariant Pair alias graphs and narrows one in an inferred function result. The
earlier compiler exceeded a four-second isolated timeout; the corrected pending-result and generic-identity queries
reached the existing canonical size diagnostic in 0.645 seconds / 4,837,376 bytes peak compiler RSS. The previous
completed output was preserved. This is bounded rejection evidence, not support for an expanded interface that exceeds
the publication budget.

A five-level valid pair retained identical program hashes before and after in population, unchanged, and edited phases.
Population measured 0.284 seconds / 4,775,936 bytes before and 0.341 seconds / 5,021,696 bytes after; the edited build
measured 0.319 seconds / 4,919,296 bytes before and 0.363 seconds / 5,079,040 bytes after. All four original/edited
artifacts executed correctly. These observations include small-case overhead and imply no general speedup. Source-free
checkpoint coverage also preserves inferred covariant results. The private report is
`result-graphs-comparison-2026-10-27.json`; owned projects and the comparison compiler were removed.

## Captured write-before-read workload

The `capture-readers` workload contains 167,355 source bytes, 5,707 lines, two functions, 39,923 instructions, and a
3,296,018-byte program. A helper writes every one of 1,900 captured Int vars, reads every value into a sum, and returns
it. The caller also reads the final captured cell. The previous compiler rejected its entry preconditions even though
every read followed initialization.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/capture-readers --workload capture-readers --keep-original
```

Isolated measurements recorded population at 0.904 seconds / 27,353,088 bytes peak compiler RSS, verified unchanged
reuse at 0.269 seconds / 3,792,896 bytes, and an edited build at 1.150 seconds / 28,471,296 bytes. Original and edited
programs executed all reads and produced 15200|8 and 15201|9.

The existing 1,900-cell caller-read workload retained identical program hashes across compilers in every phase.
Population measured 0.836 seconds / 25,374,720 bytes before and 0.899 seconds / 25,464,832 bytes after; edited builds
measured 1.285 seconds / 27,779,072 bytes before and 1.080 seconds / 27,906,048 bytes after. All four original/edited
artifacts executed correctly. These are individual bounded observations, including a population-time increase.

The private report is `entry-reads-comparison-2026-10-27.json`. Owned projects and the comparison executable were
removed. Read-before-write rejection also preserved existing output. Source-free regressions restore a helper after its
first write and retain an escaping initializing callback before its first call. This does not establish complete
interprocedural mutation analysis or the requested large-project capacity.

## Mutable field guard workload

The `mutable-fields` workload checks 32 reassignable nullable fields in each of 64 helpers, for 2,048 guards. It has
183,730 source bytes, 6,373 lines, 67 functions, 37,500 instructions, and a 2,975,600-byte original program. The two
executed helpers inspect every field; their combined result is 64, or 65 after editing the first helper's return.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/mutable-fields --workload mutable-fields --keep-original
```

Isolated GNU Time measurements recorded population at 0.947 seconds / 21,172,224 bytes peak compiler RSS, verified
unchanged reuse at 0.265 seconds / 3,776,512 bytes, and an edited build at 1.149 seconds / 21,553,152 bytes. Original
and edited programs executed to their expected results. A simple guarded mutable field previously rejected by the
compiler now compiled in 0.333 seconds / 4,788,224 bytes and executed to 3. An alias write after the guard remained a
compile error and preserved completed output.

The immutable-field workload retained identical program hashes across old and new compilers in every phase. Population
measured 1.098 seconds / 21,344,256 bytes before and 0.923 seconds / 21,352,448 bytes after; edited builds measured
1.171 seconds / 21,286,912 bytes before and 1.235 seconds / 21,069,824 bytes after. All four original/edited programs
executed correctly. These observations include an edited-build time increase and imply no general throughput or capacity
guarantee. The private report is `mutable-fields-comparison-2026-10-27.json`; owned projects and the comparison compiler
were removed. Source-free checkpoints also preserve nested field guards and an argument captured before a later
suspended argument clears the source field.

## Mutable lookup guards with readonly queries

The `mutable-indexes` workload checks 32 mutable-map keys in each of 64 helpers. Every one of its 2,048 guards is
followed by a readonly containsKey query before using the non-null value. It has 243,665 source bytes, 8,388 lines, 65
functions, 51,731 instructions, and a 4,256,486-byte original program. Two executed helpers inspect every key; their
combined result is 64, or 65 after editing the first helper's return.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/mutable-indexes --workload mutable-indexes --keep-original
```

Isolated measurements recorded population at 1.183 seconds / 28,372,992 bytes peak compiler RSS, unchanged reuse at
0.280 seconds / 3,608,576 bytes, and an edited build at 1.439 seconds / 28,127,232 bytes. Both artifacts executed to
their expected results. A simple unchanged mutable-list guard that previously failed compiled in 0.331 seconds /
4,911,104 bytes and executed to 3. A subsequent aliased put of null remained a compile error and preserved completed
output.

The corresponding immutable `indexes` workload retained identical program hashes in every phase. Population measured
1.201 seconds / 28,078,080 bytes before and 1.191 seconds / 28,409,856 bytes after; edited builds measured 1.410 seconds
/ 28,385,280 bytes before and 1.515 seconds / 28,434,432 bytes after. All four original/edited artifacts executed
correctly. These observations include an edited-build increase and establish no general capacity guarantee. The private
report is `mutable-indexes-comparison-2026-10-27.json`; owned projects and the comparison compiler were removed.
Source-free checkpoint cases preserve nested safe indexes and an argument read before a later argument clears its source
container.

## Compound reads across RHS effects

The `compound-indexes` workload checks 2,048 compound updates whose RHS clears the addressed map entry before returning
a String suffix. Its edited source has 239,034 bytes and 8,391 lines. The original program has 66 functions, 64,028
instructions, and 4,800,231 bytes. Two executed helpers visit every key, producing 160 originally and 161 after an edit.

```sh
uv run --no-project tools/benchmark-typing.py target/release/bifoilc \
  .bifoil/development/compound-indexes --workload compound-indexes --keep-original
```

Isolated measurements recorded population at 1.268 seconds / 31,301,632 bytes peak compiler RSS, verified unchanged
reuse at 0.333 seconds / 3,391,488 bytes, and an edited build at 1.595 seconds / 31,477,760 bytes. Both programs
executed to their expected results. A previously rejected field update compiled in 0.345 seconds / 5,136,384 bytes and
executed to Ada!. Effects during target evaluation still rejected an unsafe update and preserved completed output.

The existing mutable lookup workload retained identical program hashes in every phase. Population measured 1.237 seconds
/ 28,356,608 bytes before and 1.197 seconds / 28,348,416 bytes after; edited builds measured 1.374 seconds / 28,450,816
bytes before and 1.649 seconds / 28,495,872 bytes after. All four original/edited programs executed correctly. These
observations include an edited-build increase and imply no general speedup or capacity guarantee. The private report is
`compound-reads-comparison-2026-10-27.json`; owned projects and the comparison compiler were removed. Source-free
checkpoints also retain the original target after root replacement and a callable captured before a later argument
clears its container.

## Shared generic cast proofs

A 1,312-byte source casts between invariant Box types containing 28-level shared Pair alias graphs with independent
generic parameters. The earlier compiler exceeded a four-second isolated timeout. Retained proof memoization reached the
existing canonical size diagnostic in 0.595 seconds / 4,931,584 bytes peak compiler RSS, preserving completed output.
This is bounded rejection evidence; the expanded interface still exceeds its publication budget.

A valid five-level pair retained identical program hashes before and after in population, unchanged, and edited phases.
Population measured 0.318 seconds / 5,021,696 bytes before and 0.356 seconds / 5,079,040 bytes after; edited builds
measured 0.329 seconds / 5,238,784 bytes before and 0.364 seconds / 4,972,544 bytes after. All four original/edited
artifacts executed correctly. These observations include small-case overhead. Source-free tests save within both
matching and mismatching generic casts and resume without repeating their counter updates. The private report is
`overlap-queries-comparison-2026-10-27.json`; owned projects and the comparison compiler were removed.

## Shared function and scene type graphs

An 18-level private function-alias graph exceeded a four-second isolated timeout before sharing callable children. The
updated compiler accepted a 28-level graph from 881 bytes of source in 0.311 seconds / 4,911,104 bytes peak compiler RSS
and a 31-level graph from 968 bytes in 0.305 seconds / 5,083,136 bytes. Corresponding scene-input graphs compiled in
0.324 seconds / 4,743,168 bytes at 28 levels and 0.335 seconds / 5,009,408 bytes at 31 levels. Both 34-level inputs
reported the existing depth diagnostic and preserved previous completed output. These are compact analysis graphs;
publishing their fully expanded interfaces remains subject to the existing serialized size budgets.

The valid 14-level function comparison reduced population RSS from 21,770,240 to 4,939,776 bytes. Population time
measured 0.315 seconds before and 0.328 seconds after; edited builds measured 0.347 seconds / 21,917,696 bytes before
and 0.381 seconds / 4,755,456 bytes after. The scene comparison reduced population RSS from 12,152,832 to 5,033,984
bytes, with population times of 0.304 and 0.309 seconds. These observations include small-case timing overhead. Programs
matched byte-for-byte in population, unchanged, and edited phases, and all eight original/edited programs executed
correctly. Two separately precompiled higher-order function/scene projects also matched before and after, removed
dependency sources, and restored input checkpoints before continuing. The private report is
`callable-graphs-comparison-2026-10-27.json`; its temporary projects were removed after verification.

## Shared collection conversions and common types

A 1,295-byte source infers a common immutable map type across two 20-level shared alias graphs. The earlier compiler
exceeded a four-second isolated timeout; completed join queries reached the existing canonical-size diagnostic in 0.717
seconds / 4,689,920 bytes peak compiler RSS. A 1,245-byte conversion source reached that diagnostic in 0.645 seconds /
4,734,976 bytes, compared with 3.409 seconds in the earlier direct probe. Both rejected builds preserved previous
completed output. This demonstrates bounded analysis, rather than acceptance of an oversized expanded module.

Valid five-level conversion and join programs retained identical output hashes in population, unchanged, and edited
phases. Conversion population measured 0.287 seconds / 4,935,680 bytes before and 0.381 seconds / 5,046,272 bytes after;
edited builds measured 0.351 seconds / 4,866,048 bytes before and 0.344 seconds / 5,087,232 bytes after. Join population
measured 0.283 seconds / 5,042,176 bytes before and 0.321 seconds / 5,025,792 bytes after. These small-case observations
include query bookkeeping overhead. All eight original/edited programs executed correctly. A separately compiled
nested-map project matched before and after without dependency source and restored its input checkpoint with widened
types, a stable copied snapshot, and an independently emptied mutable list. The private report is
`collection-queries-comparison-2026-10-27.json`; owned temporary projects were removed after validation.

## Shared projected generic reads

A 972-byte source reads a projected field through a 20-level shared covariant Pair graph. Its earlier compiler exceeded
a four-second isolated timeout. The receiver-scoped read query reached the existing canonical-size diagnostic in 0.550
seconds / 4,694,016 bytes peak compiler RSS. A 1,292-byte, 28-level source also exceeded four seconds before and reached
that diagnostic in 0.562 seconds / 4,825,088 bytes after. Both failures preserved previous completed output. Expanded
published type records still exceed the current format's size budget.

A valid five-level comparison retained identical program hashes in population, unchanged, and edited phases. Population
measured 0.300 seconds / 4,882,432 bytes before and 0.326 seconds / 4,984,832 bytes after; edited builds measured 0.366
seconds / 5,083,136 bytes before and 0.349 seconds / 4,972,544 bytes after. All four original/edited programs executed
correctly. Independently compiled projected nested values and function results also matched before and after, removed
dependency source, and restored two input checkpoints before continuing. Direct native tests retain separate
receiver-bound contexts, hidden invariant slots, stable-key fallbacks, and callable effects. The private report is
`projected-graphs-comparison-2026-10-27.json`; owned temporary projects were removed after verification.

## Borrowed normalization scopes

The public `normalization-scopes` workload contains 1,901 aliases and 1,900 ordinary functions with two conditional
guards each. Its edited source is 273,359 bytes / 13,304 lines; the linked program contains 1,901 functions and 49,417
instructions in 3,859,824 bytes. Borrowing unchanged lexical type-name maps reduced population from 3.288 seconds /
34,316,288 bytes peak compiler RSS to 1.223 seconds / 34,385,920 bytes. Edited builds fell from 3.392 seconds /
34,287,616 bytes to 1.409 seconds / 33,873,920 bytes. Unchanged builds measured 0.255 seconds / 3,702,784 bytes before
and 0.295 seconds / 3,522,560 bytes after; this small-case overhead is retained in the report.

The original and edited programs matched byte-for-byte before and after, and all four executed correctly. Independently
compiled module bytes also matched. Module publication fell from 2.960 seconds / 34,099,200 bytes to 0.663 seconds /
33,992,704 bytes. After source removal, linking measured 1.168 seconds / 6,348,800 bytes in the updated compiler and
reproduced the edited source program exactly; both source-free outputs executed correctly. A separate precompiled
checkpoint case retained forward local aliases and independent sibling function/branch aliases. The private report is
`normalization-scopes-comparison-2026-10-27.json`; its owned temporary projects were removed after verification. These
bounded process observations do not establish the large-project capacity target.

## Shared generic name frames

The public `generic-name-scopes` workload retains 1,900 local aliases inside 1,900 generic functions, alongside 1,901
visible aliases. Its edited source is 448,165 bytes / 17,104 lines; the program has 1,901 functions, 53,221
instructions, and 4,645,755 bytes. The earlier guarded probe timed out after six seconds under a 240,000,000-byte
virtual-address space cap while preserving completed output. The updated compiler accepted that same guarded input. This
probe guard is distinct from peak-RSS measurement.

Shared name frames compiled the large workload in 1.341 seconds / 44,826,624 bytes peak compiler RSS; its edited build
measured 1.627 seconds / 45,113,344 bytes and its unchanged build 0.332 seconds / 4,001,792 bytes. Module publication
measured 0.887 seconds / 44,834,816 bytes. After source removal, linking measured 1.454 seconds / 7,053,312 bytes and
reproduced the edited source program exactly. Original, edited, guarded, and source-free outputs executed correctly.

The bounded 512-function comparison reduced population RSS from 55,873,536 to 15,667,200 bytes, with population time
falling from 0.866 to 0.545 seconds. Edited builds fell from 0.836 seconds / 55,820,288 bytes to 0.635 seconds /
15,888,384 bytes. All program hashes matched before and after. The existing nongeneric 1,900-function control also
retained identical hashes: population measured 1.043 seconds before and 1.096 seconds after; edited builds measured
1.312 and 1.319 seconds, respectively, retaining the small overhead in the record. Independently compiled generic
class/method/local-function checkpoints also matched and resumed correctly. The private report is
`generic-name-scopes-comparison-2026-10-27.json`; owned temporary projects were removed after verification. Current
module/input/output guards and the remaining large-project capacity work still apply.


## Content-addressed compiled code scenes

`tools/benchmark-packages.py` generates 64 modules, 3,781 functions, 91,121 instructions, and 320,975 source bytes
across 3,912 lines. Its inline output is 8,173,539 bytes. The package root is 22,468 bytes and names one code scene per
module. The initial package implementation measured population at 8.230 seconds / 11,870,208 bytes peak compiler RSS,
unchanged builds at 4.353 seconds / 9,564,160 bytes, and a leaf edit at 4.204 seconds / 11,485,184 bytes. The edit
rebuilt two modules and reused 62, publishing one changed code scene plus its root: 151,756 bytes. Other scenes retained
their bytes and modification times; the unchanged build rewrote no output.

The paired current inline build measured 5.883 seconds for population, 0.602 seconds unchanged, and 4.642 seconds for a
leaf edit, with peaks at or below 11,812,864 bytes. Its edit replaced the entire 8,173,539-byte output. The retained
pre-package executable produced identical inline bytes for original, unchanged, and edited builds. Every measured output
executed through checkpoint restoration and produced the expected original or edited result. Runtime verification ran
outside compiler measurements. A real Chrome check separately exercised the unchanged engine's scene-loading prelude,
checkpoint reload, and completion.

These are compiler-process Linux RSS and bounded workload observations, not aggregate cross-platform or full-capacity
compliance. The initial paired report is `code-packages-comparison-2026-10-27.json`. Package layout reduces changed
output; code remains eagerly loaded and current metadata/input guards remain in force.


### Verified completed-package reuse

Completed-package receipts now retain mode, root checksum, and the bounded inventory of code-scene checksums. Reuse
checks source bytes, compiled-module cache bytes, native source inventory, the root, and every referenced scene. Missing
or damaged data falls back to ordinary validated compilation; same-size/timestamp corruption is covered. Receipt
publication failure preserves the previous root. Stale unreferenced code scenes are irrelevant to the selected package.

On the same generated workload, the paired prior package executable measured an unchanged build at 4.671 seconds /
9,486,336 bytes peak compiler RSS. Verified completed reuse measured 1.057 seconds / 4,210,688 bytes, rewriting no
output. Population measured 8.131 seconds / 11,640,832 bytes; the leaf edit measured 5.080 seconds / 11,988,992 bytes
and still wrote only 151,756 bytes. The prior package edit measured 4.436 seconds / 11,616,256 bytes; the extra
publication checks retain that observed overhead. The corresponding inline unchanged build measured 0.583 seconds /
4,059,136 bytes, and its leaf edit measured 4.263 seconds / 11,747,328 bytes.

Both layouts retained byte-identical output relative to their respective comparison executables in all three phases. All
twelve outputs executed through restored checkpoints. Independently assembled package instructions and metadata matched
inline output after excluding the intentionally different build identities. The private report is
`package-reuse-comparison-2026-10-27.json`. This removes unchanged-body decoding; it does not remove input hashing,
resident declaration metadata, changed-build linking, eager runtime loading, or current capacity guards.


## Borrowed shared-initialization metadata

`benchmark-cache-import.py --workload shared-import` generates a 4,090-binding dependency and a reader module with no
shared declarations of its own. The program contains 4,093 functions. Dependency summaries now borrow names and records;
empty initialization passes still produce summaries for later clients but avoid building resolution tables. Linking
retains initializer IDs instead of cloning all shared definitions.

On the isolated paired workload, an edited consumer measured 1.217 seconds / 14,258,176 bytes peak compiler RSS before
and 1.108 seconds / 13,877,248 bytes after. Clean builds measured 0.749 seconds / 21,680,128 bytes before and 0.787
seconds / 20,889,600 bytes after. Cache population measured 1.061 seconds / 21,741,568 bytes before and 1.127 seconds /
21,041,152 bytes after; unchanged builds measured 0.253 seconds / 4,218,880 bytes and 0.280 seconds / 3,985,408 bytes.
These observations retain the small time regressions rather than claiming a general speedup.

Independent module publication measured 0.537 seconds / 21,594,112 bytes before and 0.662 seconds / 20,828,160 bytes
after. Source-free linking measured 0.943 seconds / 13,979,648 bytes before and 0.974 seconds / 13,635,584 bytes after.
Every original, unchanged, edited, and source-free program matched its comparison bytes; independently published module
bytes also matched. Execution separately checked all 4,090 shared values in both original and edited programs.

The existing 4,094-binding forward-chain control reduced observed peak RSS from 27,492,352 to 25,825,280 bytes, with
clean time changing from 0.735 to 0.788 seconds. Output matched, and execution checked the full initialized chain. The
private report is `initialization-views-comparison-2026-10-27.json`. Generated projects and the comparison executable
were removed. These are bounded compiler-process observations; the larger capacity and cross-platform caveats remain.


## Completed shared-dependency queries

`benchmark-typing.py --workload shared-calls` generates 252,706 source bytes across 9,500 lines, with 1,900 helpers and
1,900 shared initializers. Every initializer calls the same helper chain. Dependency analysis conservatively follows
both branches; execution takes the short branch, keeping runtime call depth separate from call-graph depth. The edited
program has 3,802 functions, 43,702 instructions, and 4,230,667 output bytes.

Completed, context-independent dependency sets are reused for multiply referenced targets. Recursive walks retain their
own visited sets. The conservative indirect target group also has one completed result. Helper cache retention is
bounded at 32,768 borrowed binding entries plus that one group; failed queries are never cached. Source-module and
linked program queries retain separate contexts.

Population changed from 3.570 seconds / 34,983,936 bytes peak compiler RSS to 1.453 seconds / 35,069,952 bytes. The
edited build changed from 4.354 seconds / 35,614,720 bytes to 1.591 seconds / 35,753,984 bytes. Unchanged builds
measured 0.286 seconds / 3,702,784 bytes before and 0.259 seconds / 3,710,976 bytes after. Output matched in every
phase.

Independent module publication changed from 1.977 seconds / 34,938,880 bytes to 0.853 seconds / 34,795,520 bytes.
Source-free linking changed from 2.628 seconds / 10,559,488 bytes to 1.129 seconds / 10,403,840 bytes and reproduced the
edited program exactly. Runtime verification separately checked every one of the 1,900 initialized values in both
original and edited programs from both executables.

The ordinary 4,094-binding shared-chain control measured 0.807 seconds / 25,899,008 bytes before and 0.840 seconds /
25,960,448 bytes after. Those small regressions remain part of the record. Control bytes matched and all chain values
executed correctly. The private report is `dependency-queries-comparison-2026-10-27.json`. Generated projects and the
comparison executable were removed; these observations retain the documented workload and capacity limitations.


## Shared immutable dependency lists

`benchmark-typing.py --workload shared-dependencies` generates 59,288 source bytes across 1,762 lines. It contains 128
seed bindings and 1,500 consumers of one helper that reads every seed. The edited program has 1,631 functions, 11,803
instructions, and 4,432,144 output bytes. Equal immutable dependency lists now share their storage during source
resolution and streamed module/cache input. The canonical JSON arrays and digest procedure remain unchanged; interning
retains at most 32,768 names across 4,096 distinct lists within one scope.

Population changed from 1.078 seconds / 38,359,040 bytes peak compiler RSS to 1.015 seconds / 16,154,624 bytes. The
edited build changed from 1.267 seconds / 38,563,840 bytes to 1.144 seconds / 16,736,256 bytes. Unchanged builds
measured 0.283 seconds / 3,739,648 bytes before and 0.262 seconds / 3,760,128 bytes after. All program bytes matched.

Independent module publication changed from 0.556 seconds / 38,244,352 bytes to 0.506 seconds / 16,142,336 bytes, with
identical module bytes. Source-free linking changed from 0.850 seconds / 21,860,352 bytes to 0.882 seconds / 11,063,296
bytes and reproduced the edited program. Runtime verification separately checked all 128 seeds and 1,500 consumers in
both original and edited programs from both executables.

The ordinary shared-chain control measured 0.806 seconds / 25,817,088 bytes before and 0.809 seconds / 25,554,944 bytes
after. Its output matched and every initialized chain value was checked. The private report is
`shared-dependency-lists-comparison-2026-10-27.json`. Generated projects and the comparison executable were removed.
Sharing metadata in memory does not reduce its expanded wire size, remove the output/input guards, or complete the
larger capacity contract.


## Releasing stale consumer interfaces

`benchmark-cache-import.py --workload nominal-consumer` generates a small shared-state dependency and a large local
consumer class with 1,024 methods, each taking 32 typed parameters. Its 491,085 source bytes span 1,035 lines; the
edited program contains 1,029 functions, 4,180 instructions, and 7,679,493 output bytes. Only the small dependency's
constant changes. Both modules rebuild while the consumer's source hash remains unchanged. Rejected cached bodies and
interfaces are now released before replacement parsing/lowering begins.

The dependent edit changed from 2.596 seconds / 64,307,200 bytes peak compiler RSS to 2.349 seconds / 49,795,072 bytes.
Clean builds measured 1.105 seconds / 49,356,800 bytes before and 1.026 seconds / 49,283,072 bytes after. Population
measured 1.646 seconds / 49,532,928 bytes before and 1.481 seconds / 49,590,272 bytes after; unchanged builds measured
0.363 seconds / 4,050,944 bytes and 0.369 seconds / 4,194,304 bytes. All corresponding programs matched exactly.

Independent consumer-module publication measured 0.771 seconds / 49,274,880 bytes before and 0.795 seconds / 49,287,168
bytes after. Source-free linking measured 1.442 seconds / 19,722,240 bytes before and 1.551 seconds / 19,755,008 bytes
after. Both published modules matched their comparison bytes, and removing both sources reproduced the edited program.
These unchanged paths retain their small timing regressions in the record.

The standard nominal-import control measured its scene edit at 1.997 seconds / 20,328,448 bytes before and 1.784 seconds
/ 20,463,616 bytes after; unchanged builds measured 0.417 and 0.358 seconds. All control programs matched. Runtime
verification separately exercised the original and edited consumer method call, executed both control programs, and
checked all 1,024 method interfaces/body signatures in each retained output.

The private report is `stale-metadata-comparison-2026-10-27.json`. Generated projects and the comparison executable were
removed. This limits overlap of old/replacement interfaces; it does not provide disk-backed metadata or establish the
larger capacity target.


## Shared parsed parameter headers

Parsed function and scene parameter lists now share immutable storage across normalized method copies. Rewriting an
annotation or default expression detaches the affected list. A regression compiles one parsed module into two namespaces
and checks that nominal types and implicit-receiver defaults do not leak back into the original AST.

The nominal-consumer workload above measured clean compilation at 1.158 seconds / 49,545,216 bytes peak compiler RSS
before and 0.988 seconds / 41,881,600 bytes after. Its dependent edit changed from 2.481 seconds / 49,754,112 bytes to
2.320 seconds / 42,360,832 bytes. Population measured 1.506 seconds / 49,422,336 bytes and 1.452 seconds / 42,102,784
bytes; unchanged reuse measured 0.383 seconds / 4,112,384 bytes and 0.360 seconds / 4,214,784 bytes.

Independent consumer-module publication changed from 0.915 seconds / 49,602,560 bytes to 0.902 seconds / 41,971,712
bytes. Source-free linking measured 1.657 seconds / 19,447,808 bytes and 1.503 seconds / 19,599,360 bytes; its metadata
input path is unchanged by this AST optimization. Both published modules matched the comparison bytes, and source-free
linking reproduced the edited program.

The nominal-import control measured its scene edit at 1.906 seconds / 20,439,040 bytes before and 1.941 seconds /
20,267,008 bytes after. Unchanged reuse measured 0.421 and 0.428 seconds. These small timing regressions remain in the
record. All compared programs matched exactly; eight separate runtime executions checked the original/edited outputs and
all 1,024 method interfaces and function signatures. The private report is `ast-parameters-comparison-2026-10-27.json`.
Generated projects and the comparison executable were removed. These bounded observations do not establish the larger
capacity target.


## Shared checked parameter headers

Checked signatures now share immutable parameter lists across symbol, interface, and overload views. Generic
specialization detaches a list only when substitution rewrites one of its types. One substitution query covers the whole
header, including its result; nullable application still normalizes when the argument map is empty. Regressions check
independent Int/String specializations, preserved defaults/mutability/variadic metadata, and unchanged canonical
declarations.

On the same nominal-consumer workload, clean compilation changed from 1.140 seconds / 42,172,416 bytes peak compiler RSS
to 0.994 seconds / 37,498,880 bytes. The dependent edit changed from 2.470 seconds / 42,446,848 bytes to 2.354 seconds /
37,953,536 bytes. Population measured 1.476 seconds / 42,151,936 bytes before and 1.588 seconds / 37,597,184 bytes
after; unchanged reuse measured 0.383 seconds / 4,227,072 bytes and 0.377 seconds / 4,067,328 bytes.

Independent consumer-module publication changed from 0.897 seconds / 42,106,880 bytes to 0.780 seconds / 37,548,032
bytes. Source-free linking measured 1.494 seconds / 19,795,968 bytes and 1.538 seconds / 19,525,632 bytes. Its expanded
metadata still requires separate decoded storage. All program and module bytes matched, and source-free linking
reproduced the edited program.

The nominal-import control measured its scene edit at 1.928 seconds / 20,512,768 bytes before and 2.227 seconds /
20,422,656 bytes after; unchanged reuse measured 0.360 and 0.380 seconds. These and the population/source-free timing
regressions remain recorded. Eight runtime executions checked original/edited results and all 1,024 method interfaces
against their function signatures. The private report is `signature-parameters-comparison-2026-10-27.json`. Generated
projects and the comparison executable were removed. These observations retain the wider capacity limitations.


## Moving prepared method bodies

Declaration preparation now moves normalized method bodies into the lowering list after recording their signatures in
the type catalog. The prepared class header no longer retains another copy. The new source-free acceptance case combines
generic receivers, private/defaulted helpers, overloaded methods, recursive calls, shallow copies, and saved bound
callbacks across a restored choice.

`benchmark-typing.py --workload method-bodies` generates 170,740 source bytes across 1,542 lines. Its 512 methods each
contain a local closure reading 16 parameters and an implicit receiver field. The program has 1,027 functions, 22,085
instructions, and 3,041,880 output bytes. Changing the field default exercises rebuilding these method/local bodies.

Population changed from 0.975 seconds / 27,377,664 bytes peak compiler RSS to 0.885 seconds / 24,793,088 bytes. The
field edit changed from 1.254 seconds / 27,561,984 bytes to 1.051 seconds / 24,698,880 bytes. Unchanged reuse measured
0.263 seconds / 3,932,160 bytes before and 0.220 seconds / 3,719,168 bytes after. Independent module publication
measured 0.560 seconds / 27,275,264 bytes and 0.561 seconds / 24,428,544 bytes. Source-free linking measured 0.746
seconds / 8,380,416 bytes and 0.763 seconds / 8,130,560 bytes; the small timing regressions remain recorded.

A separate 4,096-function clean-build control measured 1.548 seconds / 59,785,216 bytes before and 1.533 seconds /
59,715,584 bytes after. All compared program and module bytes matched. Runtime checks verified the original/edited
method results, 512 local closures, and the ordinary-function control. Source-free linking reproduced the edited
program. The private report is `method-move-comparison-2026-10-27.json`. Generated projects and the comparison
executable were removed; the larger capacity limitations remain.


## Consuming the active source AST

Final source lowering now takes ownership of its parsed AST and normalizes it in place. The loader retains source paths
and hashes for cache publication. Cyclic declaration/interface passes explicitly copy their borrowed source views until
convergence, then transfer each original AST into final lowering. This removes overlap between the active source AST and
a complete normalized copy without changing output formats or hash-checked rereading.

On the method-body workload, population changed from 0.992 seconds / 24,567,808 bytes peak compiler RSS to 0.938 seconds
/ 20,340,736 bytes. The field edit changed from 1.201 seconds / 24,731,648 bytes to 1.147 seconds / 20,398,080 bytes.
Unchanged reuse measured 0.273 seconds / 3,567,616 bytes before and 0.248 seconds / 3,555,328 bytes after. Independent
module publication measured 0.592 seconds / 24,399,872 bytes and 0.546 seconds / 20,152,320 bytes. Source-free linking
measured 0.885 seconds / 8,253,440 bytes and 0.786 seconds / 8,183,808 bytes; its input path remains unchanged.

The separate 4,096-function clean-build control changed from 1.668 seconds / 59,785,216 bytes to 1.642 seconds /
47,480,832 bytes. All compared program and module bytes matched, and removing source reproduced the edited program.
Runtime checks verified the original/edited method results, 512 local closures, and the function control. The private
report is `owned-source-ast-comparison-2026-10-27.json`. Generated projects and the comparison executable were removed.
The compiler still needs disk-backed interfaces, larger artifact indexing, and capacity-scale acceptance.


## Sparse callable-origin analysis

Shared-dependency flow analysis stores only known callable origins. Missing bindings represent unknown origins; joining
paths removes facts absent from either predecessor, while a call discards local facts that captured writes could
invalidate. Operand snapshots retain their earlier origins. This avoids retaining growing maps filled with explicit
unknown entries at every default branch. An exhaustive three-binding regression compares 4,096 joins with pointwise
origin unions, including known non-callable values.

The initial 2,048-field constructor/copy workload failed even without a constructor call: a 47,147-byte source reached
236,429,312 bytes peak compiler RSS and exhausted the native memory guard. The first noncalling-body control reduced
that case to 12,005,376 bytes / 0.397 seconds. Calling defaults still failed: 71,986 source bytes across 2,061 lines
reached 235,929,600 bytes / 6.608 seconds. The sparse representation handles both paths without omitting analysis.

`benchmark-typing.py --workload calling-defaults` generates 2,048 defaults calling a shared-state helper, with a choice
retaining the constructed record. After the sparse fix, population measured 0.668 seconds / 13,987,840 bytes peak
compiler RSS, unchanged reuse 0.215 seconds / 3,866,624 bytes, and a helper edit 0.863 seconds / 14,393,344 bytes.
Independent module publication measured 0.429 seconds / 13,770,752 bytes; source-free linking reproduced the edited
program at 0.668 seconds / 9,580,544 bytes.

A 512-field before/after control changed population from 0.690 seconds / 32,620,544 bytes to 0.390 seconds / 7,036,928
bytes and its helper edit from 0.731 seconds / 32,935,936 bytes to 0.440 seconds / 7,479,296 bytes. Unchanged reuse
measured 0.186 seconds / 3,690,496 bytes before and 0.184 seconds / 3,850,240 bytes after. All compared program bytes
matched. Runtime checks examined every field and the exact helper-call count before and after fresh checkpoint
restoration; no initializer replay occurred. A source-free shared-record case separately checks dependency-first
ordering when shared dependencies appear after their dependent declaration.

The private reports are `noncalling-origins-comparison-2026-10-27.json` and `sparse-origins-comparison-2026-10-27.json`.
The initial allocation failures are retained as failures, with narrower isolation controls. These workloads expose and
fix a small-input memory defect; they do not establish capacity for millions of source lines.


## Indexed named arguments

Compiler argument-shape matching and runtime frame binding build a name-to-position index only when a call supplies
named arguments. Positional calls need no index. Omitted-default discovery marks supplied positions once, preserving
declaration order without repeated membership scans. These changes retain argument evaluation, conversion, duplicate
binding, variadic-group, and default semantics; they introduce no new instruction or checkpoint format.

`benchmark-typing.py --workload named-constructors` generates 882,345 source bytes across 34,855 lines: a 2,048-field
record and 16 calls supplying all 32,768 named arguments in reverse field order. All records remain live at a choice.
Population measured 1.778 seconds / 24,621,056 bytes peak compiler RSS before and 1.328 seconds / 24,473,600 bytes
after. A supplied-value edit measured 1.970 seconds / 26,095,616 bytes and 1.781 seconds / 25,354,240 bytes. Unchanged
reuse measured 0.317 seconds / 3,944,448 bytes and 0.313 seconds / 3,747,840 bytes.

Independent module publication changed from 1.130 seconds / 24,354,816 bytes to 0.895 seconds / 24,219,648 bytes.
Source-free linking measured 1.361 seconds / 17,453,056 bytes and 1.346 seconds / 17,301,504 bytes. Program and module
bytes matched exactly. Runtime checks verified every field before and after checkpoint restoration for original and
edited programs, repeating each execution three times. Median construction/execution to the choice measured
479.73/510.54 milliseconds before and 322.53/314.38 milliseconds after for the original/edited programs. These runtime
observations include VM construction and validation; they are separate from compiler memory measurements.

The private report is `indexed-arguments-comparison-2026-10-27.json`. The ordinary parameter guard and larger
source/module limits remain unchanged; the capacity target remains unfinished.


Completed origin/argument comparison projects, baseline executables, runtime snapshots, and drivers were removed after
verification. Raw measurement and execution reports remain under the ignored development-report directory.


## Source indentation formatting

`bifoilc format` validates syntax before expanding leading code tabs. Its shared layout scan protects prose and
continued literal/interpolation rows. Formatting retains line endings and conditional spellings, checks output growth
before appending both code and protected content, and publishes requested files atomically.

A mixed-tab version of the named-constructor workload contains 679,470 source bytes across 34,855 lines. Formatting
measured 0.180 seconds / 11,718,656 bytes peak compiler RSS and produced 882,339 bytes. The six-byte difference from the
all-space workload is the deliberately untouched prose prefix. A second formatting check reported no changes. Clean
compilation measured 1.081 seconds / 24,526,848 bytes before formatting and 1.131 seconds / 24,514,560 bytes afterward;
the small timing difference remains recorded. Both outputs matched the exact program bytes from the previously verified
wide named-constructor workload. No runtime execution was included in these compiler measurements.

The private report is `source-format-wide-2026-10-27.json`. Syntax, expansion-budget, semantic-digest, clean-stdout,
check-mode, and failed-output-preservation regressions accompany the command. These observations retain the existing
source and capacity limits.


## Ordering forward alias declarations

Transparent aliases now schedule their current lexical group's dependencies before normalizing bodies and bounds. The
index graph stores source positions rather than cloned declarations; independent ready entries retain source order. Type
parameters remain local, imported/enclosing aliases retain their normalized definitions, and nominal declarations remain
leaves. Alias-expansion cycles still produce diagnostics; recursive data fields remain permitted.

The previous compiler overflowed its native stack on 16,384 forward aliases in 426,050 source bytes at 1.276 seconds /
26,546,176 bytes peak RSS. The same input now compiles at 0.320 seconds / 19,034,112 bytes. A regression also replaces
the terminal scalar with a cycle and verifies that failure preserves the previous completed output.

`benchmark-typing.py --workload forward-aliases` generates 32,768 transitively used aliases, a typed value, and a saved
choice: 852,097 source bytes across 32,775 lines. Population measured 0.621 seconds / 33,890,304 bytes peak compiler
RSS; unchanged reuse measured 0.164 seconds / 3,817,472 bytes. Changing the terminal type from Int to Float measured
0.590 seconds / 34,426,880 bytes. Independent module publication measured 0.392 seconds / 33,779,712 bytes, and
source-free linking reproduced the edited program at 0.167 seconds / 4,608,000 bytes. Runtime checks retained the exact
Int/Float value tags before and after checkpoint restoration.

A 4,096-alias control changed from 0.248 seconds / 13,869,056 bytes to 0.197 seconds / 8,093,696 bytes; its program
bytes matched exactly. A separate generic-body/bound control combines a source-free function alias, generated nominal
values, a cyclic field, and a restored callback. Source/module/source-free phases measured 0.181/0.108/0.200 seconds
before and 0.188/0.104/0.177 seconds after; the small source-build regression remains recorded. Program and module bytes
matched exactly, and both runtime executions retained the cyclic nominal value and returned the expected result.

The private report is `forward-aliases-comparison-2026-10-27.json`. These bounded declarations eliminate another
small-input stack failure; the source/module envelope and the larger capacity target remain unchanged.


## Flat horizontal conditional chains

The AST stores aligned if/elif/else-if arms in a vector. Parsing, normalization, capture discovery, value rewriting,
flow inference, and lowering traverse that chain without native recursion. Failed tests retain their effects and
narrowing before later tests; normally completing arm bodies join afterward. Value-result inference preserves the prior
right-associated joins. Each arm retains its header line for diagnostics, and real source blocks keep their scope rules.

The previous compiler overflowed its native stack on 8,192 aligned branches in 399,303 source bytes. The same input now
compiles at 1.231 seconds / 54,812,672 bytes peak RSS. Its 73,748 instructions fit the unchanged instruction guard. A
2,048-arm control changed from 1.115 seconds / 76,337,152 bytes to 0.408 seconds / 16,760,832 bytes; instruction count
changed from 22,546 to 18,452 by removing artificial false-path scopes. Smaller 128/512-arm controls measured
0.193/0.277 seconds before and 0.214/0.349 seconds after; those timing regressions remain recorded.

`benchmark-typing.py --workload conditional-chains` generates 456,844 source bytes across 16,398 lines, with 8,192 tests
calling a shared-counter helper. All tests must run once before selecting the last arm or fallback. The linked program
contains 81,974 instructions and 6,549,417 output bytes. Population measured 2.401 seconds / 60,579,840 bytes peak
compiler RSS, unchanged reuse 0.397 seconds / 3,796,992 bytes, and a selector edit 1.847 seconds / 63,213,568 bytes.
Independent module publication measured 0.866 seconds / 60,645,376 bytes; source-free linking reproduced the edited
program at 1.572 seconds / 19,181,568 bytes. Runtime checks retained the result and exact test count across restoration.

Conditional code bytes change because horizontal false paths no longer allocate nested environments. Old modules linked
through the new compiler reproduce the original program; new modules linked through the old compiler reproduce the flat
program. Function signatures remain identical. A separate source-free expression control restored seven input waits,
preserving numeric widening and six condition calls without replay. Its source/module/source-free phases measured
0.199/0.113/0.217 seconds before and 0.203/0.113/0.176 seconds after.

The private report is `conditional-chains-comparison-2026-10-27.json`. Focused cases also check delayed val joins, null
proofs after early returns, invalidation by a captured write, and original branch-header diagnostics. The native memory,
source, function, instruction, and output guards remain unchanged; the larger capacity target remains unfinished.


## Assignment facts without redundant frame copies

Ordinary writes still emit their runtime stores. Analysis now checks initialization, narrowing, and callable-read
requirements before copying a shared binding frame. Equal facts retain their existing frame; writes to tracked index/key
bindings still invalidate lookup proofs independently. Shared source aliases continue to receive the same facts.

A 172,898-byte control with 4,000 live named values, a sum using every value, and 128 conditional writes previously
exhausted the native memory guard at 235,016,192 bytes peak RSS. The same input now compiled in 0.417 seconds at
11,763,712 bytes. The 1,024-value control changed from 0.592 seconds / 84,611,072 bytes to 0.257 seconds / 6,696,960
bytes; the 256-value control changed from 0.243 seconds / 24,829,952 bytes to 0.204 seconds / 5,771,264 bytes.
Successful comparison controls retained identical program bytes.

`benchmark-typing.py --workload assignment-facts` keeps all 4,000 values and their list live at a player choice. Its
172,973 source bytes span 8,271 lines. Population measured 0.693 seconds / 12,972,032 bytes; the selector edit measured
0.684 seconds / 13,082,624 bytes. Unchanged reuse measured 0.214 seconds / 3,895,296 bytes. Independent module
publication measured 0.274 seconds / 11,747,328 bytes, and source-free linking reproduced the edited program in 0.609
seconds at 7,753,728 bytes.

A separate 1,024-value checkpoint comparison retained exact original/edited bytes. Peak population/edit RSS changed from
85,106,688/85,237,760 bytes to 7,110,656/7,188,480 bytes. Twelve executions checked the smaller controls and the
original, edited, and source-free wide programs. Every checkpointed binding, list element, sum, and branch result was
checked before and after restoration. A native negative regression ensures a ready key assignment still invalidates its
earlier nullable-element proof.

The private report is `assignment-facts-comparison-2026-10-27.json`. Changed facts can still require frame copies; this
optimization does not complete the larger source/module capacity target.


## Joining completed branch facts incrementally

`FlowJoin` merges each completing path as it becomes available instead of retaining every path's binding maps. The first
path retains declaration metadata from the original scope; later paths intersect definite/narrowed facts and union
possible initialization, escape, index/key, and callable requirements. Lowering and return inference share this
operation for conditional, `when`, and choice successors. Unreachable paths remain excluded.

With a delayed `val` result, the 4,000-value conditional control previously failed at 5.250 seconds / 235,134,976 bytes
peak RSS. It now completes in 0.855 seconds at 16,330,752 bytes. Equivalent `when` and choice controls also previously
exhausted the guard near 235 MB; they now complete in 0.871 seconds / 18,665,472 bytes and 0.720 seconds / 16,285,696
bytes. Every value contributes to the computed sum and remains live at a checkpoint.

The 1,024-value conditional control changed from 0.449 seconds / 85,037,056 bytes to 0.314 seconds / 7,790,592 bytes.
Its `when` control changed from 0.514 seconds / 85,225,472 bytes to 0.378 seconds / 8,282,112 bytes, and its choice
control changed from 0.368 seconds / 84,598,784 bytes to 0.305 seconds / 7,913,472 bytes. All successful smaller
controls retained exact program bytes.

`benchmark-typing.py --workload path-joins` uses 172,974 source bytes across 8,271 lines. Population measured 0.867
seconds / 16,453,632 bytes, unchanged reuse 0.199 seconds / 3,616,768 bytes, and the selector edit 1.023 seconds /
19,161,088 bytes. Independent module publication measured 0.670 seconds / 16,334,848 bytes; source-free linking
reproduced the edited program in 0.589 seconds at 7,872,512 bytes. Fourteen executions verified every retained binding,
list element, sum, selected result, and restored checkpoint. Native tests compare 4,096 three-path combinations with the
all-path join and verify that completed frame snapshots are released. Source-free acceptance covers inferred results,
delayed values, callback requirements, early returns, and repeated input waits.

The private report is `path-joins-comparison-2026-10-27.json`. Loop-transfer snapshots and broader capacity work remain
separate. No source, function, or output guard was raised.


## Joining completed loop breaks

Loop state now anchors completed break facts to its entry scopes and merges each break immediately. Normal exits then
join that accumulated result using the same flow rules. Lowering and return inference share this behavior for while,
for, do/while, and labeled outer-loop exits. Continue snapshots retain their individual facts for correlation-sensitive
backedge capture checks.

The 4,000-value while-break workload contains 176,344 source bytes across 8,401 lines. Its previous compiler failed at
4.911 seconds / 235,130,880 bytes peak RSS; the updated compiler completed in 0.813 seconds at 16,175,104 bytes.
Equivalent for, do/while, and labeled-break workloads also exhausted the previous guard near 235 MB. Their updated
results were 0.753 seconds / 16,568,320 bytes, 0.758 seconds / 16,408,576 bytes, and 0.791 seconds / 16,551,936 bytes.

Successful 1,024-value controls retained exact program bytes across all four loop forms. The while control changed from
0.460 seconds / 85,184,512 bytes to 0.317 seconds / 7,761,920 bytes. For, do/while, and labeled controls changed from
0.446/0.375/0.377 seconds at about 85 MB to 0.365/0.299/0.313 seconds at about 8 MB.

`benchmark-typing.py --workload loop-breaks` measured population at 0.895 seconds / 16,560,128 bytes, unchanged reuse at
0.197 seconds / 3,702,784 bytes, and the selector edit at 1.027 seconds / 19,021,824 bytes. Independent module
publication measured 0.712 seconds / 16,318,464 bytes; source-free linking reproduced the edited program in 0.474
seconds at 7,593,984 bytes. Seventeen executions checked every binding, list element, sum, selected result, and restored
checkpoint. Separate source-free acceptance restores ten waits inside or after loop calls and verifies shared
preparation counts without replay. Negative cases retain both definite- and possible-initialization checks.

The private report is `loop-breaks-comparison-2026-10-27.json`. These fixes retain the existing guards and leave the
larger capacity target and continue-path storage work unfinished.


## Sharing binding entries across retained paths

Scope maps now clone entry handles instead of every binding record. Mutating an entry detaches its binding; unchanged
records, callable requirements, and declaration metadata remain shared. Bulk proof invalidation first selects affected
entries. Continue paths retain separate snapshots so their initialization/capture correlations remain intact.

A 176,739-byte, 4,000-value do/while control with 128 continue arms previously exhausted the guard at 4.026 seconds /
235,356,160 bytes peak RSS. The updated compiler completed in 0.700 seconds at 62,525,440 bytes. The 256-value control
changed from 0.313 seconds / 25,362,432 bytes to 0.252 seconds / 8,753,152 bytes; the 1,024-value control changed from
0.532 seconds / 85,819,392 bytes to 0.293 seconds / 19,775,488 bytes. Successful programs and independently compiled
1,024-value module artifacts retained identical bytes.

`benchmark-typing.py --workload continue-facts` spans 8,402 source lines. Population measured 0.776 seconds / 62,582,784
bytes, unchanged reuse 0.199 seconds / 3,665,920 bytes, and the selector edit 0.974 seconds / 63,410,176 bytes.
Independent module publication measured 0.506 seconds / 62,697,472 bytes; source-free linking reproduced the edited
program in 0.510 seconds at 7,577,600 bytes. Twelve executions checked every binding, list element, sum, selected
result, and restored checkpoint. A separate correlation control retained exact output bytes, preserved source-free input
waits, and kept the unsafe-backedge diagnostic.

The ready-assignment control retained exact original/edited programs. Population changed from 0.524 seconds / 11,948,032
bytes to 0.496 seconds / 11,071,488 bytes. Unchanged reuse changed from 0.208 seconds / 3,776,512 bytes to 0.216 seconds
/ 3,633,152 bytes; the edit changed from 0.613 seconds / 13,074,432 bytes to 0.697 seconds / 12,451,840 bytes. These
timing regressions remain part of the recorded comparison.

The private report is `shared-bindings-comparison-2026-10-27.json`. Retained map nodes and key strings still scale with
frame width and path count. The existing guards and larger capacity limitations remain unchanged.


## Persistent ordered scope trees

Lexical binding scopes now use the locked `rpds` ordered persistent map. Unchanged tree nodes and entries remain shared;
updates detach the search path and changed binding. The library also provides entry sharing, so the temporary custom
binding-entry wrapper was removed. Bulk proof invalidation collects affected names before mutation. Continue paths keep
their individual facts.

The 4,000-value workload with 1,024 continue arms contains 244,884 source bytes across 11,090 lines. The earlier
entry-sharing compiler exhausted its guard at 4.421 seconds / 235,917,312 bytes peak RSS. Persistent scopes completed
the same input in 1.100 seconds at 16,277,504 bytes. The 256-value control changed from 0.464 seconds / 34,914,304 bytes
to 0.355 seconds / 10,452,992 bytes; the 1,024-value control changed from 1.226 seconds / 114,016,256 bytes to 0.463
seconds / 11,653,120 bytes. The 4,000-value, 128-arm control changed from 0.724 seconds / 62,681,088 bytes to 0.448
seconds / 11,337,728 bytes. Successful programs and independently compiled control modules retained identical bytes.

`benchmark-typing.py --workload continue-branches` measured population at 1.159 seconds / 16,236,544 bytes, unchanged
reuse at 0.212 seconds / 3,870,720 bytes, and the selector edit at 1.250 seconds / 16,785,408 bytes. Independent module
publication measured 0.852 seconds / 16,211,968 bytes. Source-free linking reproduced the edited program in 0.653
seconds at 9,527,296 bytes. Fourteen executions checked every binding, list element, sum, selected result, and restored
checkpoint.

The ready-assignment control retained exact original/edited programs. Population changed from 0.708 seconds / 11,190,272
bytes to 0.631 seconds / 11,145,216 bytes, unchanged reuse from 0.234 seconds / 3,710,976 bytes to 0.195 seconds /
3,829,760 bytes, and the edit from 0.815 seconds / 12,161,024 bytes to 0.621 seconds / 12,427,264 bytes. The small RSS
regressions remain recorded.

The private report is `persistent-scopes-comparison-2026-10-27.json`. The maximum-path CLI regression now checks both
128 and 1,024 arms under the native guard. These observations retain all source/function/output guards and do not
establish the larger project capacity target.


## Shared module-function environments and a larger code table

The `function-tables` workload in [the workload generator](tools/workload_source.py) declares 16,383 ordinary routines
and one scene. It contains 743,330 source bytes and 32,811 lines. The scene stores every routine reference in batches,
calls every routine once, and retains all references, the call count, and their sum across a prepared input checkpoint.
The edited build changes the final routine's result and must change the retained sum.

The prior 4,096-function guard rejected this source. A staged 16,384-function build exposed repeated narrowing resets:
each module function scanned all the same declarations. An isolated edited incremental build exhausted its 9.5-second
internal deadline at 9.564 seconds and preserved the prior output. A temporary phase probe attributed about 8.3 seconds
to those scans. That instrumentation was removed after diagnosis.

Module siblings and generated shared initializers now clone one prepared capture environment. Local functions and
anonymous functions still reset their own definition-site captures. The tested compiler and runtime function-table guard
is now 16,384; the independent instruction, source, artifact, output, and runtime heap guards remain in force.

Isolated cache population measured 2.566 seconds / 65,208,320 bytes peak compiler RSS. Completed-output reuse measured
0.447 seconds / 3,682,304 bytes. The edited incremental build measured 2.996 seconds / 65,462,272 bytes. Independent
module publication measured 1.547 seconds / 65,171,456 bytes; linking after source removal measured 2.780 seconds /
28,848,128 bytes. Source-free package output measured 3.015 seconds / 28,663,808 bytes. The module artifact occupied
9,952,412 bytes.

The 4,096-function control retained identical program and independent-module bytes. Its clean build changed from 0.993
seconds / 19,595,264 bytes to 0.525 seconds / 19,607,552 bytes; the small RSS increase is retained here rather than
reported as a reduction. Twelve original/restored executions checked every retained routine reference, call count, sum,
and continuation result across original, edited, source-free, package, and control builds. Native and runtime boundary
regressions reject the next function and preserve completed output where applicable.

These are bounded development-profile measurements. They do not establish the requested capacity for tens to hundreds of
millions of source lines or compliance for every program inside the current guards.


## Inferred returns across a full function table

The `inferred-function-tables` workload removes the ordinary return annotations from the preceding 16,384-function
program. It contains 628,649 source bytes and 32,811 lines. Before the inference change, cache population measured
15.739 seconds / 65,298,432 bytes peak compiler RSS. Its edited incremental build failed the internal deadline after
14.142 seconds while preserving the prior output; a whole inference round had delayed the budget check.

Return inference now shares one prepared capture environment within each module round. Newly inferred signatures update
that snapshot before later siblings are analyzed. Shared-type changes enter the next prepared round. Local functions
continue to prepare their own capture environment. Module inference also checks its deadline between functions, rather
than waiting for the complete round.

Cache population measured 3.335 seconds / 68,603,904 bytes after the change. Unchanged reuse measured 0.544 seconds /
3,514,368 bytes; the edited incremental build measured 4.222 seconds / 68,837,376 bytes. Independent module publication
measured 1.533 seconds / 68,403,200 bytes. Source-free inline and package builds measured 2.790 seconds / 28,647,424
bytes and 3.051 seconds / 28,774,400 bytes, respectively.

The full inferred program retained exactly the annotated program's bytes. Smaller inferred controls also retained
identical program and independent-module bytes. Twelve original/restored executions checked every function reference,
call count, sum, and continuation result. Shared forward inference and overloaded returns have a focused native
regression. The broader capacity target and guard limitations remain open.


## Ordinary function parameter width

The `function-parameters` workload declares a suspending function with 2,048 typed default parameters. It contains
191,832 source bytes and 8,218 lines. Each invocation consumes every parameter. Three calls exercise supplied arguments
before omitted defaults, a function value with every argument explicit, and reversed named arguments. A shared list
records every argument/default effect; each function call and the final result wait can be restored.

Named declarations previously failed at the special 32-parameter header guard. That guard has been removed; ordinary and
anonymous functions, like generated constructors, remain subject to the source, instruction, artifact, output, and
memory guards. This change does not impose a language-level arity limit or establish success for arbitrary signatures.

Cache population measured 0.794 seconds / 16,113,664 bytes peak compiler RSS. Unchanged output reuse measured 0.243
seconds / 3,670,016 bytes. An edited named argument rebuilt in 0.964 seconds / 17,117,184 bytes. Independent module
publication measured 0.560 seconds / 15,933,440 bytes; source-free inline and package builds measured 0.843 seconds /
9,957,376 bytes and 0.816 seconds / 9,764,864 bytes, respectively. The independent module occupied 2,584,899 bytes.

Twelve original/restored executions checked every parameter, retained value-list element, and argument/default effect at
all four waits, including source-free inline and package builds. The 32-parameter control retained identical program and
module bytes. Native CLI coverage also compiles and formats a 2,048-parameter function. A compact source-free
conformance case covers the same three call interfaces, evaluation order, and checkpoint continuation at 40 parameters.
The broader language and capacity acceptance work remains open.


## Generic declaration width

The `generic-parameters` workload uses 512 bounded type parameters on a data class, transparent alias, sealed sum, and
suspending function. It contains 119,987 source bytes and 2,079 lines. The function constructs every field, creates a
stable-key set, makes a shallow copy, and returns a typed variant. Restored input waits retain the active generic call,
its data/copy/key relationships, and the returned variant. Editing a final argument alias from Int to Float changes the
last physical argument and field type; an erased generic type test must distinguish that result.

The small generic-parameter guard has been removed. A local ordered-name index still rejects duplicates without a
quadratic scan; source order remains in the parameter vector. A first regression placed a complete wide function on one
physical line and hit the existing 4,096-token row guard. Splitting its declaration/body using normal source syntax
retained that guard and let the test isolate generic width. Source, token, type-depth, code, artifact, output, and
memory guards remain independent limits.

Cache population measured 1.116 seconds / 11,579,392 bytes peak compiler RSS. Unchanged reuse measured 0.207 seconds /
3,530,752 bytes. The edited type argument rebuilt in 1.238 seconds / 11,911,168 bytes. Independent module publication
measured 0.814 seconds / 11,554,816 bytes. Source-free inline and package builds measured 0.563 seconds / 8,376,320
bytes and 0.646 seconds / 8,622,080 bytes, respectively. The independent module occupied 1,850,380 bytes.

The 32-argument control retained identical program and module bytes. Its independent-module observation changed from
0.129 seconds / 5,623,808 bytes to 0.134 seconds / 5,685,248 bytes; those small regressions are retained. Twelve
original/restored executions checked every generic argument, parameter, field, copied object, key relationship, and
variant payload. Native regressions cover all four generic declaration families and duplicate names beyond the former
guard. Compact source-free and negative conformance cases exercise the same supported paths. This does not establish
arbitrary-signature success or the much larger overall capacity target.


## Reachable scene-module capacity

`tools/benchmark-modules.py` builds 512 modules: shared counters, one entry scene, and 510 separately declared scenes.
Every scene is visited through a retained typed scene reference. The original source has 77,217 bytes and 3,584 lines.
The module guard is now 512, aligned with the runtime package inventory guard. Native CLI coverage accepts the full
graph, then rejects the next reachable module without replacing the completed output.

Discovery overlaps bounded byte reads on at most four threads, admitting at most 4 MiB of prepared input bytes across a
build. Parsing, semantic checks, digest verification, and code-spool ownership remain on the loader thread. Unavailable
or changing optional reads use the ordinary bounded path. Existing code-scene comparison now computes its whole-file
checksum during the byte comparison, avoiding a second file read. Prefixes, suffixes, buffer boundaries, and extra
trailing bytes retain regression coverage. No timestamp shortcut or deadline increase was introduced.

Before input prefetch, an edited 512-module graph reached the 9.5-second internal deadline. Input prefetch let the
source edit complete, but package cache population still reached that deadline. After removing the duplicate checksum
read, isolated source cache population measured 26.530 seconds / 9,515,008 bytes peak compiler RSS. Unchanged reuse
measured 2.169 seconds / 4,378,624 bytes; editing the final scene measured 7.913 seconds / 10,084,352 bytes. The edit
preserved the file's size and modification time.

All 512 modules were independently published and their source files removed. Source-free inline linking reproduced the
edited output in 5.102 seconds / 9,543,680 bytes. Package publication measured 20.968 seconds / 9,154,560 bytes, package
cache population 8.335 seconds / 9,248,768 bytes, and unchanged package reuse 4.250 seconds / 4,526,080 bytes. Cold
compilation scales with input and is reported separately from the incremental requirement. The retained native report is
`module-capacity-2026-10-28.json`.

The 64-module before/after controls retained identical program and reusable-module bytes. Twenty-four original/restored
executions checked every scene entry, retained scene reference, shared counter, and final transcript across the large
graph and both controls, including source-free inline/package output. Restoring the prepared input wait did not replay
any scene. These observations qualify this workload on the reference host; the much larger overall capacity target
remains unmet.


## Multi-million-line narrative capacity

`tools/benchmark-narrative.py --scenes 32` generates 3,200,264 source lines and 32,005,321 source bytes across 34
modules. Each of 32 separately visited scenes has 100,000 prose rows between unique boundary markers. Every row
contributes to the rendered paragraph; source-free output and the prepared checkpoint retain all 6.4 million rendered
characters. This is a narrative-heavy workload with few declarations and instructions, rather than a measurement of
millions of independently checked code statements.

The previous compiler rejected this project at its 16 MiB aggregate input guard. Aggregate source/artifact input now
admits 64 MiB; the 1 MiB individual-source and 16 MiB artifact/output limits remain unchanged. The layout scanner
borrows unchanged code and prose rows directly from the active source instead of allocating a String for every physical
row. It owns text only when joining a continued literal, retaining blank placeholders and original line positions.
Parsing, import discovery, and formatting share that scanner.

Isolated native measurements recorded cache population at 4.555 seconds / 12,390,400 bytes peak compiler RSS, unchanged
byte-validated reuse at 0.679 seconds / 4,235,264 bytes, and a final-paragraph edit at 2.520 seconds / 10,244,096 bytes.
The edit preserved the source file's size and modification time. All 34 modules were independently compiled and their
source removed. Source-free inline linkage reproduced the edited program exactly in 1.104 seconds / 9,097,216 bytes.
Package publication measured 2.133 seconds / 9,396,224 bytes, cache population 1.339 seconds / 9,486,336 bytes, and
unchanged package reuse 0.584 seconds / 4,362,240 bytes.

The 800,072-line before/after control retained identical program and reusable-module bytes. Its population changed from
1.704 seconds / 15,638,528 bytes to 1.263 seconds / 11,972,608 bytes; its edit changed from 0.916 seconds / 12,894,208
bytes to 0.861 seconds / 9,867,264 bytes. Source-free control linkage changed from 0.356 seconds / 6,791,168 bytes to
0.402 seconds / 6,934,528 bytes; these small regressions are retained alongside the improvements. The combined report is
`narrative-capacity-2026-10-28.json`.

Twenty-four original/restored executions compared every rendered paragraph byte, scene-entry order, visit counter, and
final continuation across the large workload and both controls. The large prepared checkpoint occupied 6,402,543 bytes
and restored without replaying narrative or visits. Formatting the largest source file retained its bytes. These
measurements qualify the enlarged envelope for this workload on the reference host. They do not establish the requested
tens-to-hundreds-of-millions-of-lines capacity or full compiler compliance.


The default narrative workload now uses 67 scenes: 6,700,544 source lines and 67,010,956 bytes, close to the
67,108,864-byte aggregate guard. Population measured 8.808 seconds / 12,161,024 bytes peak compiler RSS, unchanged reuse
1.204 seconds / 4,100,096 bytes, and an edit 4.940 seconds / 10,682,368 bytes. Source-free linking measured 2.247
seconds / 9,093,120 bytes. Package publication measured 4.609 seconds / 9,347,072 bytes, package cache population 2.867
seconds / 9,568,256 bytes, and unchanged package reuse 0.984 seconds / 4,280,320 bytes.

Eight additional original/restored executions checked every rendered byte and visit at that boundary. Adding a 68th
narrative scene produced 68,011,117 source bytes and correctly reached the aggregate diagnostic in 5.742 seconds /
12,615,680 bytes while preserving the previous completed output. Native accounting tests also reject integer overflow
without charging a failed input. The retained reports are `narrative-capacity-boundary-2026-10-28.json` and
`narrative-capacity-boundary-rejection-2026-10-28.json`.


## Pure computed index and map-key proofs

`benchmark-typing.py --workload computed-indexes --keep-original` generates 302,637 source bytes, 8,460 lines, and 65
functions. All 64 helpers execute 32 nullable-map guards using `prefix + number` keys. Their mutable parameter bindings
and mutable map receivers retain facts across read-only membership queries. Every helper result is retained in a list
and summed before an input wait, so every declared guard contributes to the observed result.

A scoped expression identity now recognizes pure primitive arithmetic, comparisons, Boolean operations, and
concatenation over lexical bindings. It retains operation structure, normalized scalar constants, and each binding's
identity; grouping does not change the identity. Reassigning any mutable dependency invalidates its facts, and existing
heap/call invalidation still applies. In this initial phase, calls and heap-dependent expressions do not establish
computed-key proofs. Each optional identity is limited to 128 nodes and 4 KiB of owned names/literal bytes; larger
expressions can use an explicit immutable key binding. The representation is analysis-only and does not enter module or
runtime formats.

The previous source compiler rejected the positive fixture's nullable member read. The new workload measured population
at 1.500 seconds / 27,361,280 bytes peak compiler RSS, unchanged reuse at 0.399 seconds / 3,678,208 bytes, and an edit
at 1.855 seconds / 27,602,944 bytes. Independent module publication measured 0.937 seconds / 26,951,680 bytes;
source-free inline linkage measured 1.551 seconds / 5,120,000 bytes and reproduced the edited program exactly. Package
publication measured 1.683 seconds / 5,373,952 bytes. The retained report is `computed-key-capacity-2026-10-28.json`.

The existing mutable-index control retained identical program bytes. Its population changed from 1.260 seconds /
23,597,056 bytes to 1.269 seconds / 23,543,808 bytes. Its edit changed from 1.580 seconds / 23,404,544 bytes to 1.418
seconds / 23,523,328 bytes; both the small time and RSS regressions are retained here. Fourteen runtime executions
checked every new workload helper entry and retained result, original/restored continuations, and the smaller controls.
The former compiler also linked the new source-free module to identical program bytes and executed its restored
continuation. No runtime protocol change was required.

Native and source-free cases cover arithmetic indexes, formatted keys, null/type guards, assertions, `when` subjects,
escaping readers, and unchanged local cells across choices. Reassignments, aliased heap writes, effectful key
expressions, stale subject snapshots, changed choice-body cells, and later same-named bindings remain guarded. That
phase leaves generic/composite key expressions and broader alias/effect proofs unfinished.


## Pure text and generic computed keys

The analysis now recognizes String scalar indexing, String length, interpolation, primitive generic upper views, and
immutable selected field chains. Every selector in a captured field chain must be immutable, while unrelated mutable
fields may change. The original node/text budgets and dependency invalidation remain in force; mutable field reads and
effectful calls remain conservative. Named local functions retain block-entry narrowing with definition-site name
visibility; guards established later in that block cannot justify a forward call. Anonymous functions use stable proofs
available at construction.

`benchmark-typing.py --workload text-computed-indexes --keep-original` generates 436,765 source bytes, 6,733 lines, 65
functions, and 1,536 guards. All 64 generic helpers execute; every result is retained and summed before a checkpoint.
The edited program contains 94,735 instructions. Population measured 2.492 seconds / 37,158,912 bytes peak compiler RSS,
unchanged reuse 0.449 seconds / 3,866,624 bytes, and an edit 2.570 seconds / 37,216,256 bytes. Standalone module
publication measured 1.688 seconds / 37,040,128 bytes. Source-free inline linkage measured 2.694 seconds / 5,103,616
bytes; package publication measured 2.401 seconds / 5,353,472 bytes.

The preceding computed-index control retained exact original and edited program bytes. Population changed from 1.435
seconds / 27,078,656 bytes to 1.465 seconds / 27,504,640 bytes; reuse changed from 0.347 seconds / 3,543,040 bytes to
0.348 seconds / 3,764,224 bytes; edits changed from 1.738 seconds / 27,422,720 bytes to 1.839 seconds / 27,398,144
bytes. These time and memory regressions are retained with the positive results in `text-key-capacity-2026-10-28.json`.

Eighteen original/restored executions verified all helper entries, generic signatures, retained results, and final sums
without replay. The preceding compiler also linked the new module to identical source-free program bytes. Unicode and
nullable generic field fixtures exercised escaping readers, unrelated field mutation, and two restored waits. The full
formatted corpus passed all 753 cases in both output modes. Runtime instruction and checkpoint formats remain unchanged;
broader mutable/composite key proofs and the full capacity target remain unfinished.


## Canonical private-code serialization

Completed functions now reuse their canonical spool bytes during hashing and serialization. This avoids repeated
instruction-object construction and encoding while retaining private byte checksums, header fingerprints, syntax/count
validation, bounded allocation, and fatal spool-error propagation. Typed semantic/dependency validation remains
unchanged. Native fault tests include truncated storage, valid-JSON corruption, changed headers, incorrect counts, and
the streamed input failure channel. Unicode, large escaped text, and finite binary64 spellings retain exact canonical
bytes.

The same text/generic workload's population changed from 2.305 seconds / 37,408,768 bytes peak compiler RSS to 2.303
seconds / 37,167,104 bytes. Its edit changed from 2.622 seconds / 37,474,304 bytes to 2.432 seconds / 37,486,592 bytes.
Unchanged reuse changed from 0.458 seconds / 3,915,776 bytes to 0.451 seconds / 4,042,752 bytes. These small memory
regressions remain part of the report.

For 16,384 functions, population changed from 2.488 seconds / 65,236,992 bytes to 1.906 seconds / 65,355,776 bytes; an
edit changed from 3.179 seconds / 65,945,600 bytes to 2.875 seconds / 65,884,160 bytes. Unchanged reuse changed from
0.536 seconds / 3,833,856 bytes to 0.500 seconds / 4,100,096 bytes. Both workloads retained exact original and edited
program bytes.

Standalone publication of the text workload changed from 1.355 seconds / 37,302,272 bytes to 1.153 seconds / 37,158,912
bytes; source-free inline linkage changed from 2.134 seconds / 5,324,800 bytes to 1.756 seconds / 5,656,576 bytes.
Package publication changed from 2.208 seconds / 5,332,992 bytes to 1.892 seconds / 5,459,968 bytes. Module bytes and
package-root bytes remained identical. Twenty-four original/restored executions checked every helper/routine entry,
retained result or function reference, counters, sums, and no-replay behavior. Raw reports are
`raw-code-capacity-2026-10-28.json` and `raw-code-runtime-2026-10-28.json`. These results qualify this serialization
change rather than the full language or its capacity target.


## Aggregate executable-code capacity

`tools/benchmark-code.py` and `tools/code_workload.py` generate six modules, 15,000 called routines, 2,119,750 source
bytes, and 45,044 source lines. Every helper contains 13 explicit runtime arithmetic operations. Every helper is
referenced and called; all results and references remain live at an input wait. Reusable artifacts occupy 46,819,275
bytes. The package contains 15,001 functions and 480,129 instructions, close to the new 500,000-program limit. Each
module remains below 100,000 instructions, and every artifact/code scene retains its 16 MiB guard. This is
executable-code capacity rather than a large prose-only input.

The first larger edit exhausted the 9.5-second deadline and retained its completed package. Removing a redundant
pending-body decode improved the smaller control but still reached that cutoff on the large edit.
`VerifiedModule::store` now copies checksum-verified canonical bodies through one buffered writer. The later dependency
pass still decodes every body for analysis, then retains its existing canonical bytes rather than encoding its
instructions again. Structural validation, dependency summaries, header/count/checksum checks, and legacy-target
collection remain mandatory.

After those changes, population measured 8.344 seconds / 80,801,792 bytes peak compiler RSS; unchanged reuse measured
1.436 seconds / 4,370,432 bytes; a final-routine edit measured 9.320 seconds / 64,741,376 bytes. Independent library
publication measured 0.794–0.854 seconds per library at under 39.4 MB RSS, and entry-module publication measured 6.434
seconds / 62,390,272 bytes. Source-free package linkage measured 8.677 seconds / 40,370,176 bytes, source-free cache
population 8.961 seconds / 40,493,056 bytes, and unchanged reuse 1.167 seconds / 4,026,368 bytes. Edited and source-free
package roots retained exact bytes.

The 2,500-routine control retains exact original/edited package and reusable-module bytes against the former compiler.
Population changed from 1.797 seconds / 15,667,200 bytes to 1.948 seconds / 15,228,928 bytes; edits changed from 2.308
seconds / 15,646,720 bytes to 1.914 seconds / 15,560,704 bytes. Source-free linking changed from 1.822 seconds /
10,235,904 bytes to 1.475 seconds / 10,731,520 bytes. Source-free cache population changed from 1.789 seconds /
10,338,304 bytes to 1.585 seconds / 10,813,440 bytes. Unchanged source/package reuse changed from 0.367/0.335 seconds to
0.350/0.338 seconds; the corresponding RSS readings changed from 4,128,768/4,304,896 bytes to 4,333,568/4,063,232 bytes.
These regressions remain in the report alongside improvements.

Fourteen original/restored executions checked every routine entry, every retained result and function reference, final
sums, and no-replay behavior. The former compiler rejects the larger workload at its aggregate code guard. An additional
arithmetic operation in each routine exceeds the expanded aggregate guard while retaining independently valid module
sizes; rejection preserves the prior root and publishes no code scenes. Runtime tests accept the exact aggregate
boundary, reject its next instruction, and independently reject oversized code modules. Raw results and failed
intermediate attempts are retained under `code-capacity-*-2026-10-28.json`. This qualifies the expanded workload on the
reference host; the requested tens-to-hundreds-of-millions-of-lines target remains unfinished.

The same source-free program also reached the inline output-size diagnostic in 8.260 seconds at 39,628,800 bytes peak
compiler RSS. Its previous output remained intact and staging files were removed; larger code requires packages. The
retained report is `code-capacity-inline-rejection-2026-10-28.json`.


## Early rejection of stale cache identities

Canonical disposable-cache records put compiler and source identities before their module. The decoder now compares each
identity with its request as soon as it arrives. A known mismatch becomes a cache miss without reading or spooling the
unused body. Alternate field order remains supported; matching entries still undergo full digest, structure, and
dependency validation. Tests exercise both file-backed and preloaded input, metadata after a module, malformed
irrelevant bodies, and retained deadline/private-storage failure handling.

A fresh predecessor comparison populated the 15,000-routine workload in 8.290 seconds / 82,726,912 bytes peak compiler
RSS and reused it in 1.224 seconds / 4,120,576 bytes. Its edit reached the deadline at 9.560 seconds / 64,806,912 bytes
and retained the completed package. The updated compiler populated in 7.651 seconds / 82,337,792 bytes, reused in 1.137
seconds / 4,362,240 bytes, and edited in 8.893 seconds / 64,794,624 bytes. Original package bytes matched directly;
edited bytes matched the previously accepted successful producer.

The 2,500-routine control retained identical original/edited package and module bytes. Population changed from 2.009
seconds / 15,785,984 bytes to 1.891 seconds / 15,785,984 bytes; edits changed from 2.040 seconds / 15,495,168 bytes to
1.873 seconds / 15,732,736 bytes. Unchanged reuse changed from 0.362 seconds / 4,128,768 bytes to 0.369 seconds /
4,259,840 bytes. Source-free linking/cache/reuse changed from 1.522/1.562/0.376 seconds to 1.448/1.516/0.325 seconds.
Their RSS readings changed from 10,715,136/10,489,856/4,227,072 bytes to 10,600,448/10,559,488/4,280,320 bytes. The time
and memory regressions are retained with the other measurements.

The large source-free path uses unchanged artifact validation and measured 8.048 seconds / 40,652,800 bytes; cache
population measured 8.044 seconds / 40,349,696 bytes and unchanged reuse 1.142 seconds / 3,977,216 bytes. Fourteen
original/restored executions checked all entries, retained references/results, sums, and no-replay behavior. The
formatted corpus passed all 753 cases in both modes. Reports are `cache-filter-capacity-2026-10-28.json` and
`cache-filter-runtime-2026-10-28.json`. These results qualify the measured workload on the reference host, with broader
capacity still unfinished.


## Wide function graphs and native allocator policy

`benchmark-code.py --workload function-graph` generates 131,310 source lines and 3,467,248 bytes across ten modules.
Eight visits call all 65,520 user routines; shared collections retain every function reference and result until an input
wait. Counting helpers and initializers bring the total to exactly 65,536 functions and 393,838 instructions. Every
declared function participates in execution. Independently published artifacts occupy 53,344,044 bytes, within the
existing aggregate input guard. Each source/module remains under its separate declaration, code, and file-size limits.

The initial graph hit the address-space guard at 97,075,200 bytes peak RSS. A controlled `MALLOC_ARENA_MAX=1` probe
completed the same graph at 114,597,888 bytes without changing that guard. `src/resources.rs` now sets glibc's arena
maximum to one before starting reader threads, then retains the 240,000,000-byte virtual-address-space ceiling. This
avoids reserving additional large arenas for short-lived I/O workers. Other platforms retain their existing
qualification status.

The larger graph also exposed a missing final source-module check for generated functions. Constructors, copies, and
initializers now count against the independent 16,384-function module limit after lowering. A production CLI regression
verifies rejection and preservation of a previous valid artifact. Runtime tests separately accept the exact aggregate
function boundary, reject its next entry, and reject an oversized individual code module.

Private header fingerprints now retain 32 digest bytes instead of allocating hexadecimal strings. Public digests keep
their exact lowercase hexadecimal format. A diagnostic build records elapsed phases with `BIFOIL_TRACE_TIMINGS=1`; its
failing preliminary edit spent 7.878 seconds loading modules and reached the deadline while serializing the last code
chunk. Bounded prefetch now permits 32 MiB of raw input over an entire build, enabling more large artifact/cache records
to use the existing byte-slice decoder. Metadata reservation still bounds actual reads, including racing file growth,
and all validation stays on the loader thread.

The final graph populated its cache in 9.051 seconds / 110,862,336 bytes peak compiler RSS, reused unchanged output in
1.280 seconds / 4,239,360 bytes, and edited its final routine in 9.320 seconds / 112,447,488 bytes. Library publication
measured 0.661–0.724 seconds per library below 38.7 MB RSS; entry publication measured 6.102 seconds / 93,069,312 bytes.
Source-free linkage measured 8.456 seconds / 92,291,072 bytes; source-free cache population measured 8.565 seconds /
94,572,544 bytes and unchanged reuse 1.529 seconds / 4,186,112 bytes. Source-free roots retained the edited program's
exact bytes.

The 16,016-function control retained identical original/edited package and module bytes against the former compiler.
Population changed from 3.080 seconds / 41,127,936 bytes to 2.968 seconds / 30,756,864 bytes. Edits changed from 3.008
seconds / 40,456,192 bytes to 2.452 seconds / 31,612,928 bytes. Unchanged reuse changed from 0.460 seconds / 4,497,408
bytes to 0.477 seconds / 4,427,776 bytes. Source-free linkage changed from 2.538 seconds / 27,774,976 bytes to 2.325
seconds / 26,886,144 bytes; cache population changed from 2.667 seconds / 27,811,840 bytes to 2.336 seconds / 26,750,976
bytes; unchanged reuse changed from 0.438 seconds / 4,321,280 bytes to 0.474 seconds / 4,206,592 bytes. The reuse
regressions remain with the positive results.

Fourteen original/restored executions verified every routine entry, retained reference/result, visit order, and
counting-helper calls without replaying prepared labels or completed work. The large checkpoint occupied 5,496,184
bytes. The former compiler rejects the larger graph; adding one function to the expanded graph also rejects before
publishing code scenes and preserves prior output. All 753 formatted language cases passed in both output modes,
alongside 388 native/CLI and 112 runtime/engine tests. Intermediate allocation/deadline failures and the
allocator/timing probes remain in the raw `function-graph-*-2026-10-28.json` reports. These are bounded reference-host
qualifications; the tens-to-hundreds-of-millions-of-lines target remains unfinished.


## Shared empty initialization summaries

Pure bodies share one immutable empty initialization summary after their ordinary origin analysis. Other summaries
retain their own bindings, calls, indirect-call flags, and errors. Module/input/output formats remain unchanged; the
standalone source regression retains a genuine shared dependency alongside the pooled pure bodies.

The 65,536-function graph's population changed from 8.928 seconds / 110,792,704 bytes peak compiler RSS to 9.112 seconds
/ 102,096,896 bytes. Its edit changed from 8.854 seconds / 112,414,720 bytes to 8.922 seconds / 104,284,160 bytes.
Unchanged reuse changed from 1.318 seconds / 4,112,384 bytes to 1.214 seconds / 4,497,408 bytes. The time regressions
and reuse-memory increase remain with the memory improvement. Source-free linkage measured 8.120 seconds / 86,933,504
bytes, cache population 8.156 seconds / 86,978,560 bytes, and unchanged reuse 1.188 seconds / 4,259,840 bytes.

The 16,016-function control retained exact original/edited package and reusable-module bytes. Population changed from
3.118 seconds / 30,965,760 bytes to 2.624 seconds / 28,704,768 bytes; edits changed from 2.493 seconds / 31,518,720
bytes to 2.469 seconds / 29,724,672 bytes. Unchanged reuse changed from 0.475 seconds / 4,325,376 bytes to 0.448 seconds
/ 4,173,824 bytes. Source-free linkage/cache/reuse changed from 2.193/2.110/0.431 seconds to 2.034/2.169/0.569 seconds,
with RSS changing from 26,775,552/27,025,408/4,263,936 bytes to 24,887,296/25,034,752/4,268,032 bytes. Source-free reuse
became slower and slightly larger in this sample.

Sixteen original/restored executions checked all routine entries, retained references/results, visits, helper calls, and
prepared-label continuity. The full formatted corpus passed all 753 cases in both output modes. Raw reports are
`shared-summary-capacity-2026-10-28.json` and `shared-summary-runtime-2026-10-28.json`. These qualify this metadata
change rather than the full capacity target.


## Removal of unreachable lowered code

The native compiler now removes instructions that cannot be reached after source checking. All local targets, including
deferred choice destinations, are remapped; retained source spans and opaque runtime identities keep their values.
Module instruction accounting sees the compacted code, while the existing intermediate per-function guard remains
active. This changes newly compiled module and program bytes without changing the instruction schema. Old artifacts
retain their producer code and remain readable.

The 16,016-function matched control changed from 13,007,716 to 10,304,336 reusable-artifact bytes. Population changed
from 2.967 seconds / 28,524,544 bytes peak compiler RSS to 2.426 seconds / 28,758,016 bytes. Its edit changed from 2.416
seconds / 29,605,888 bytes to 2.044 seconds / 29,388,800 bytes. Unchanged reuse changed from 0.501 seconds / 4,247,552
bytes to 0.398 seconds / 4,173,824 bytes. Source-free linkage changed from 2.056 seconds / 24,850,432 bytes to 1.724
seconds / 24,457,216 bytes; cache population changed from 2.162 seconds / 24,915,968 bytes to 1.795 seconds / 24,309,760
bytes; unchanged source-free reuse changed from 0.449 seconds / 4,292,608 bytes to 0.387 seconds / 4,255,744 bytes. The
population RSS increase remains with the timing improvements.

The 65,536-function graph changed from 393,838 to 262,786 instructions. The optimized source-free artifacts occupy
42,222,264 bytes; the previously qualified unpruned artifacts occupied 53,344,044 bytes. In this matched run,
predecessor population measured 8.742 seconds / 102,273,024 bytes and unchanged reuse 1.389 seconds / 4,247,552 bytes.
Its edit reached the native deadline after 9.641 observed seconds / 103,993,344 bytes and retained the original
completed root; it did not proceed to module publication. That failure remains in the report.

The optimized population measured 7.698 seconds / 102,178,816 bytes, unchanged reuse 1.017 seconds / 4,415,488 bytes,
and the edit 7.637 seconds / 103,608,320 bytes. Source-free linkage measured 6.476 seconds / 85,557,248 bytes, cache
population 6.769 seconds / 85,807,104 bytes, and unchanged reuse 1.044 seconds / 4,128,768 bytes. Source-free output
retained the edited root bytes. The unchanged RSS increase is retained.

Independent observers compared every original interface and simple routine body and checked all routine entries,
results, references, visits, helpers, and prepared-label continuity through 22 original/restored executions. Both
producer/consumer directions retained exact source-free roots on the smaller control; the previous compiler also linked
the large optimized artifacts with identical root bytes. All 753 formatted cases passed in both output modes alongside
394 native/CLI tests. Runtime implementation and its earlier 112-test qualification are unchanged. Raw measurements,
code comparisons, and execution results remain in the `code-pruning-*-2026-10-28.json` reports. This remains a bounded
reference-host qualification, not acceptance of the larger language capacity target.


## Shared source filenames

Newly lowered source locations share the module filename through immutable handles. The JSON path remains a String, so
all original/edited package and precompiled-module bytes match the predecessor. A native regression covers generated
constructors/copies, nested closures, source lines, escaped/Unicode filenames, and canonical module round trips. This
does not intern every imported filename or remove artifact decoding.

The 16,016-function graph retained byte-identical artifacts. Population changed from 2.756 seconds / 28,540,928 bytes
peak compiler RSS to 2.438 seconds / 28,155,904 bytes. Unchanged reuse changed from 0.418 seconds / 4,493,312 bytes peak
compiler RSS to 0.431 seconds / 4,329,472 bytes. The edit changed from 2.122 seconds / 29,659,136 bytes peak compiler
RSS to 2.163 seconds / 29,073,408 bytes.

Source-free linkage changed from 1.693 seconds / 24,731,648 bytes peak compiler RSS to 1.719 seconds / 24,854,528 bytes.
Source-free cache population changed from 1.737 seconds / 24,731,648 bytes peak compiler RSS to 1.799 seconds /
24,621,056 bytes. Unchanged source-free reuse changed from 0.363 seconds / 4,186,112 bytes peak compiler RSS to 0.376
seconds / 4,288,512 bytes.

The 15,001-function arithmetic workload retained byte-identical artifacts. Population changed from 7.311 seconds /
64,962,560 bytes peak compiler RSS to 6.917 seconds / 61,472,768 bytes. Unchanged reuse changed from 1.161 seconds /
4,333,568 bytes peak compiler RSS to 1.082 seconds / 4,145,152 bytes. The edit changed from 7.685 seconds / 56,705,024
bytes peak compiler RSS to 8.138 seconds / 56,745,984 bytes.

Source-free linkage changed from 6.415 seconds / 35,266,560 bytes peak compiler RSS to 6.862 seconds / 35,762,176 bytes.
Source-free cache population changed from 6.754 seconds / 35,434,496 bytes peak compiler RSS to 6.760 seconds /
35,614,720 bytes. Unchanged source-free reuse changed from 1.139 seconds / 4,313,088 bytes peak compiler RSS to 1.014
seconds / 4,268,032 bytes.

The code workload reduced population RSS while its edit and source-free timings regressed; the smaller control also
retained timing and memory regressions. All results remain in `shared-spans-capacity-2026-10-28.json`. Sixteen
original/restored executions checked every routine, reference, result, visit, and prepared-label continuation, with raw
results in `shared-spans-*-runtime-2026-10-28.json`. All 753 formatted cases passed in both output modes alongside 395
native/CLI tests. The runtime and its prior 112-test qualification are unchanged. These are bounded observations on the
reference host, not full language or large-project resource acceptance.


## Retained artifact validation facts

Streamed artifacts now retain structural facts while their instruction objects are first available. Later verification
checks those facts against the completed module after canonical digest verification, avoiding one redundant typed body
decode. Empty facts are shared, named capture sets remain shared during comparison, and private checksums and header
checks remain active. Original and edited packages and all reusable-module bytes match the predecessor.

The 16,016-function control passed every measured phase. Population changed from 2.616 seconds / 28,311,552 bytes peak
compiler RSS to 2.624 seconds / 28,237,824 bytes. The edit changed from 2.117 seconds / 28,909,568 bytes peak compiler
RSS to 1.942 seconds / 28,827,648 bytes. Source-free linkage changed from 1.718 seconds / 24,539,136 bytes peak compiler
RSS to 1.532 seconds / 24,621,056 bytes.

The 15,001-function arithmetic workload passed every measured phase. Population changed from 6.968 seconds / 61,263,872
bytes peak compiler RSS to 7.242 seconds / 61,403,136 bytes. The edit changed from 7.896 seconds / 56,569,856 bytes peak
compiler RSS to 7.087 seconds / 56,344,576 bytes. Source-free linkage changed from 6.923 seconds / 35,643,392 bytes peak
compiler RSS to 5.414 seconds / 35,278,848 bytes.

The 65,536-function graph passed every measured phase. Population changed from 7.133 seconds / 100,294,656 bytes peak
compiler RSS to 7.291 seconds / 100,421,632 bytes. The edit changed from 7.367 seconds / 101,933,056 bytes peak compiler
RSS to 6.781 seconds / 101,777,408 bytes. Source-free linkage changed from 6.511 seconds / 86,515,712 bytes peak
compiler RSS to 5.654 seconds / 85,635,072 bytes.

The raw `structural-facts-capacity-2026-10-28.json` report retains all unchanged/cache/publication observations and
their timing or RSS regressions alongside the improvements above. Twelve original/restored executions checked every
routine, reference, result, visit, helper, and prepared-label continuation across the current three workload outputs.
Raw results remain in `structural-facts-*-runtime-2026-10-28.json`. All 753 formatted cases passed in both modes
alongside 398 native/CLI tests. Runtime code and its prior 112-test qualification are unchanged. These qualify this
validation change on the reference host rather than full language or large-project acceptance.


## Scoped canonical-value reuse

Package publication now reserves at most 16 MiB for canonical function values during one module's hash/compare/write
sequence, including owned keys and an allocation allowance. Scope exit releases all entries. Every use still
rereads/checksums the spool and checks its requested header; only repeated syntax/count scans are avoided. Oversized
entries use the uncached path. All measured original/edited package and reusable-module bytes match the predecessor.

The 16,016-function control passed every measured phase. Population changed from 2.488 seconds / 28,184,576 bytes peak
compiler RSS to 2.360 seconds / 28,573,696 bytes. The edit changed from 1.868 seconds / 28,991,488 bytes peak compiler
RSS to 1.868 seconds / 29,106,176 bytes. Source-free linkage changed from 1.530 seconds / 24,698,880 bytes peak compiler
RSS to 1.606 seconds / 24,903,680 bytes.

The 15,001-function arithmetic workload passed every measured phase. Population changed from 7.078 seconds / 61,222,912
bytes peak compiler RSS to 6.926 seconds / 61,190,144 bytes. The edit changed from 6.881 seconds / 56,365,056 bytes peak
compiler RSS to 6.698 seconds / 56,815,616 bytes. Source-free linkage changed from 5.962 seconds / 34,996,224 bytes peak
compiler RSS to 5.355 seconds / 35,102,720 bytes.

The 65,536-function graph passed every measured phase. Population changed from 7.112 seconds / 100,634,624 bytes peak
compiler RSS to 7.859 seconds / 101,269,504 bytes. The edit changed from 7.349 seconds / 101,457,920 bytes peak compiler
RSS to 6.773 seconds / 101,625,856 bytes. Source-free linkage changed from 5.841 seconds / 85,700,608 bytes peak
compiler RSS to 5.618 seconds / 85,356,544 bytes.

The smaller source-free control regressed, as did large-graph population and some cache/RSS observations. All
unchanged/cache/publication samples remain in `encoded-cache-capacity-2026-10-28.json`. Twelve original/restored
executions checked every routine, result, reference, visit, helper, and prepared-label continuation; raw results remain
in `encoded-cache-*-runtime-2026-10-28.json`. All 753 formatted cases passed in both modes alongside 401 native/CLI
tests. Runtime code and its earlier 112-test qualification are unchanged. These remain bounded reference-host
observations rather than full language or large-project acceptance.


## Expanded program function ceiling

The native and runtime aggregate function ceiling now admits 81,920; independently compiled modules retain 16,384 and
the existing instruction, file, input, and memory guards. The current function-graph benchmark defaults to ten scene
modules with 8,190 user routines each. Together with stats and main it has twelve modules, 81,918 functions, 328,465
instructions, 4,336,672 source bytes, 164,132 source lines, and 52,778,406 reusable-artifact bytes. Its Float
accumulator sums integer results exactly without overflowing Int32; individual results remain Int and are checked
individually. Earlier sections retain their historical eight-scene/Int-accumulator measurements.

The larger graph populated its cache in 9.621 seconds / 119,357,440 bytes peak compiler RSS. Unchanged reuse measured
1.174 seconds / 4,255,744 bytes; its edit measured 8.641 seconds / 120,946,688 bytes. Source-free linkage measured 7.544
seconds / 104,333,312 bytes, cache population 7.571 seconds / 104,529,920 bytes, and unchanged reuse 1.167 seconds /
4,091,904 bytes. Library publication measured 0.105–0.670 seconds per library; entry publication measured 5.705 seconds
/ 104,243,200 bytes. Source-free output retained the edited root bytes.

The previous compiler rejected the larger graph at its program function guard and preserved a real prior completed
package and its code-scene inventory. The first observer expected the wrong diagnostic phrase; its original 5.547-second
/ 109,711,360-byte measurement remains recorded, and preservation was independently verified before continuing. A clean
exact-boundary compile accepted 81,920 functions in 9.045 seconds / 104,407,040 bytes. Its next function rejected in
5.535 seconds / 104,325,120 bytes, preserving the exact-boundary root and code-scene inventory. These two boundary
commands used --clean and are separate from incremental timing qualification.

The 16,016-function control retained identical original/edited package and module bytes. Population changed from 2.661
seconds / 28,532,736 bytes to 2.750 seconds / 28,684,288 bytes. Unchanged reuse changed from 0.401 seconds / 4,374,528
bytes to 0.406 seconds / 4,210,688 bytes. The edit changed from 1.989 seconds / 29,069,312 bytes to 1.858 seconds /
28,983,296 bytes.

Source-free linkage changed from 1.575 seconds / 24,829,952 bytes to 1.469 seconds / 24,952,832 bytes. Source-free cache
population changed from 1.663 seconds / 24,829,952 bytes to 1.523 seconds / 24,719,360 bytes. Unchanged source-free
reuse changed from 0.394 seconds / 4,362,240 bytes to 0.385 seconds / 4,120,576 bytes. Timing and RSS regressions remain
with the improvements.

Ten original/restored executions checked every routine entry, value, retained reference, visit, helper, prepared label,
and both added exact-boundary helpers. The larger checkpoint occupied 6,872,211 bytes. All 753 formatted cases passed in
both output modes alongside 401 native/CLI and 112 runtime/engine tests, including exact runtime function-count
boundaries. Raw observations remain in `program-capacity-measurements-2026-10-28.json` and
`program-capacity-graph-runtime-2026-10-28.json`. These qualify the larger bounded profile on the reference host; the
tens-to-hundreds-of-millions-of-lines target still requires disk-backed interfaces and lazy runtime code loading.


## Buffered module input

Streamed module input now uses one 64 KiB pending writer for the entire function map. Bodies still decode and leave
memory one at a time; only canonical bytes queue in the existing buffer. Function offsets, checksums, counts,
signatures, and all original/edited package/module bytes match the previous compiler. The visitor explicitly finishes
after parse failures to retain fatal I/O/deadline propagation.

The 16,016-function control passed every measured phase. Population changed from 2.865 seconds / 28,680,192 bytes peak
compiler RSS to 2.399 seconds / 28,499,968 bytes. The edit changed from 2.137 seconds / 29,032,448 bytes peak compiler
RSS to 1.834 seconds / 28,794,880 bytes. Source-free linkage changed from 1.625 seconds / 24,428,544 bytes peak compiler
RSS to 1.573 seconds / 24,686,592 bytes.

The 15,001-function arithmetic workload passed every measured phase. Population changed from 6.916 seconds / 61,095,936
bytes peak compiler RSS to 7.124 seconds / 61,349,888 bytes. The edit changed from 6.693 seconds / 56,664,064 bytes peak
compiler RSS to 6.423 seconds / 56,664,064 bytes. Source-free linkage changed from 5.668 seconds / 34,975,744 bytes peak
compiler RSS to 5.402 seconds / 34,807,808 bytes.

The 81,918-function graph passed every measured phase. Population changed from 9.027 seconds / 119,517,184 bytes peak
compiler RSS to 9.333 seconds / 119,468,032 bytes. The edit changed from 8.460 seconds / 120,602,624 bytes peak compiler
RSS to 8.686 seconds / 120,516,608 bytes. Source-free linkage changed from 7.434 seconds / 104,681,472 bytes peak
compiler RSS to 7.252 seconds / 105,582,592 bytes.

The largest edit regressed while smaller edits and source-free linkage improved. All unchanged/cache/publication
observations and their regressions remain in `buffered-input-capacity-2026-10-28.json`. Twelve original/restored
executions checked every routine, reference, result, visit, helper, and prepared-label continuation; raw results remain
in `buffered-input-*-runtime-2026-10-28.json`. All 753 formatted cases passed in both output modes alongside 402
native/CLI tests. Runtime code and its prior 112-test qualification are unchanged. These are bounded reference-host
observations, not full language or source-line capacity acceptance.

## Paged function-location indexes

Function-location metadata now spills after 16,384 entries into private redb 4.4.0 storage. The database cache is 4 MiB.
Staged writes reserve at most 1 MiB or 4,096 rows, flushing an oversized single key immediately. Serialization reserves
at most 4 MiB for location snapshots, separately from the existing 16 MiB canonical-value cache. Declaration/export
metadata remains resident; this does not establish the tens-to-hundreds-of-millions-of-lines target.

The final comparison reuses the accepted compiler before measurements from record-index-batch-capacity-2026-10-28.json,
with its executable identity checked again. Changed-compiler measurements and all original/edited package/module byte
comparisons are recorded in record-index-lookups-capacity-2026-10-28.json. Native compiler-process RSS remains the
reported memory quantity; these observations do not establish every platform aggregate-job accounting contract.

The 16,016-function control passed every measured phase. Its population changed from 2.727 seconds / 28,110,848 bytes
peak compiler RSS to 2.667 seconds / 28,737,536 bytes. Its incremental edit changed from 1.962 seconds / 29,044,736
bytes peak compiler RSS to 2.213 seconds / 29,208,576 bytes. Its source-free linkage changed from 1.436 seconds /
24,858,624 bytes peak compiler RSS to 1.584 seconds / 25,038,848 bytes.

The 15,001-function arithmetic workload passed every measured phase. Its population changed from 7.627 seconds /
61,018,112 bytes peak compiler RSS to 7.173 seconds / 61,472,768 bytes. Its incremental edit changed from 7.622 seconds
/ 56,713,216 bytes peak compiler RSS to 6.855 seconds / 56,934,400 bytes. Its source-free linkage changed from 5.345
seconds / 34,820,096 bytes peak compiler RSS to 5.152 seconds / 35,176,448 bytes.

The 81,918-function graph passed every measured phase. Its population changed from 9.330 seconds / 119,279,616 bytes
peak compiler RSS to 10.195 seconds / 104,325,120 bytes. Its incremental edit changed from 8.188 seconds / 120,897,536
bytes peak compiler RSS to 8.754 seconds / 105,848,832 bytes. Its source-free linkage changed from 7.486 seconds /
105,590,784 bytes peak compiler RSS to 8.166 seconds / 90,619,904 bytes.

Cold population may scale with input. The final large edit and source-free builds traded some time for lower resident
memory; smaller timing and memory regressions remain recorded. All unchanged, publication, cache, and completed-reuse
observations remain in the raw report.

Earlier candidates failed the large edit deadline: the first per-record index failed at 9.657 seconds / 106,274,816
bytes, and bounded read snapshots alone failed at 9.648 seconds / 106,078,208 bytes. Both preserved the original
completed root. Their reports remain in record-index-capacity-2026-10-28.json and
record-index-snapshot-capacity-2026-10-28.json. Bounded write batches subsequently passed at 9.706 seconds / 105,963,520
bytes; removing duplicate location probes provided the final margin. Intermediate observations remain in
record-index-batch-capacity-2026-10-28.json.

All 411 native/CLI tests and all 753 formatted conformance cases in both output modes passed. Twelve independent
original/restored executions checked every routine, reference, result, visit, helper, and prepared-label continuation
across the three workloads. Runtime code and its prior 112-test qualification are unchanged. Raw execution and
conformance results remain in the record-index reports. Public artifact, instruction, and checkpoint schemas are
unchanged. Larger declaration catalogs and lazy runtime code loading remain unfinished.

## Borrowed initializer signature views

Initializer signature lookup now borrows own and imported declaration maps instead of constructing a flattened tree of
every reachable function. The linker passes a function iterator directly. The conservative candidate set is populated
only when an indirect-call summary exists; when needed, all eligible targets remain included. Completed declaration
storage itself remains resident.

The isolated comparison against the previously qualified paged-index compiler is recorded in
signature-views-capacity-2026-10-28.json. Both executable identities stayed unchanged. Every original/edited package and
independently compiled module matched byte for byte. The figures below report native compiler-process RSS on the
existing reference host.

The 16,016-function control passed every measured phase. Its population changed from 2.689 seconds / 28,639,232 bytes
peak compiler RSS to 2.419 seconds / 28,581,888 bytes. Its incremental edit changed from 1.984 seconds / 29,106,176
bytes peak compiler RSS to 1.873 seconds / 28,958,720 bytes. Its source-free linkage changed from 1.418 seconds /
24,854,528 bytes peak compiler RSS to 1.519 seconds / 24,698,880 bytes.

The 15,001-function arithmetic workload passed every measured phase. Its population changed from 6.760 seconds /
61,362,176 bytes peak compiler RSS to 7.046 seconds / 61,181,952 bytes. Its incremental edit changed from 6.596 seconds
/ 56,942,592 bytes peak compiler RSS to 6.836 seconds / 56,512,512 bytes. Its source-free linkage changed from 5.461
seconds / 35,008,512 bytes peak compiler RSS to 5.682 seconds / 34,541,568 bytes.

The 81,918-function graph passed every measured phase. Its population changed from 10.223 seconds / 104,660,992 bytes
peak compiler RSS to 9.941 seconds / 103,976,960 bytes. Its incremental edit changed from 9.453 seconds / 105,971,712
bytes peak compiler RSS to 8.841 seconds / 105,848,832 bytes. Its source-free linkage changed from 7.819 seconds /
90,853,376 bytes peak compiler RSS to 8.109 seconds / 90,435,584 bytes.

Peak-memory changes are modest on these workloads. Arithmetic timing and some source-free timings regressed; the raw
report retains those results alongside the improvements, completed reuse, cache builds, and module publication. All 412
native/CLI tests, twelve independent original/restored executions, and all 753 formatted cases in both output modes
passed. Runtime code and its prior 112-test qualification are unchanged. This removes temporary signature indexes
without changing public artifact or checkpoint schemas; persistent declaration catalogs and full source-line capacity
remain unfinished.

## Shared callable headers

Callable headers now share immutable allocations across symbols, function tables, exports, constructors, and nominal
methods. Inference, generic specialization, and receiver projection detach changes. Validated artifact input reconciles
equal views only after digest and structure checks. Source and restored-input regressions preserve canonical bytes and
reject detached invalid interfaces.

The reusable tools/compare-code.py observer runs comparisons sequentially and retains failures. It compares all
generated roots, original roots, code scenes, and independently compiled modules using bounded byte reads. The
additional private-function workload changes routine visibility while retaining every call, callback, result, visit, and
checkpoint. Its report also records unchanged observer-source identities.

The 16,016-function control passed every measured phase. Its population changed from 2.440 seconds / 28,860,416 bytes
peak compiler RSS to 2.511 seconds / 25,255,936 bytes. Its incremental edit changed from 1.926 seconds / 29,188,096
bytes peak compiler RSS to 2.124 seconds / 25,624,576 bytes. Its source-free linkage changed from 1.715 seconds /
24,936,448 bytes peak compiler RSS to 1.646 seconds / 21,671,936 bytes.

The 15,001-function arithmetic workload passed every measured phase. Its population changed from 6.842 seconds /
60,968,960 bytes peak compiler RSS to 6.662 seconds / 57,978,880 bytes. Its incremental edit changed from 6.886 seconds
/ 56,328,192 bytes peak compiler RSS to 6.631 seconds / 51,941,376 bytes. Its source-free linkage changed from 5.244
seconds / 34,385,920 bytes peak compiler RSS to 5.290 seconds / 34,394,112 bytes.

The 81,918-function exported graph passed every measured phase. Its population changed from 9.697 seconds / 104,263,680
bytes peak compiler RSS to 10.097 seconds / 88,354,816 bytes. Its incremental edit changed from 8.786 seconds /
105,238,528 bytes peak compiler RSS to 8.811 seconds / 86,540,288 bytes. Its source-free linkage changed from 7.855
seconds / 90,460,160 bytes peak compiler RSS to 7.843 seconds / 72,851,456 bytes.

The 81,918-function private graph passed every measured phase. Its population changed from 10.043 seconds / 76,492,800
bytes peak compiler RSS to 9.100 seconds / 71,880,704 bytes. Its incremental edit changed from 7.950 seconds /
74,137,600 bytes peak compiler RSS to 8.307 seconds / 69,566,464 bytes. Its source-free linkage changed from 7.102
seconds / 65,478,656 bytes peak compiler RSS to 6.960 seconds / 61,980,672 bytes.

The larger exported graph substantially reduced resident copies without materially changing edit time. The private graph
also used less memory, while its edit and several smaller/cache timings regressed. Every observation remains in
shared-signatures-capacity-2026-10-28.json and shared-signatures-private-capacity-2026-10-28.json; cold builds may scale
with input. Native compiler-process RSS remains the reported quantity, rather than a new claim about platform-wide
aggregate accounting.

All 414 native/CLI tests passed, including canonical-header sharing, invalid export isolation, specialization, and
nullable normalization. Sixteen independent original/restored executions and all 753 formatted conformance cases in both
output modes passed. Runtime code and its prior 112-test qualification are unchanged. No public artifact, instruction,
or checkpoint schema changed. Full source-line capacity, persistent declaration catalogs, and lazy runtime code loading
remain unfinished.


## Canonical metadata checks and cleanup before publication

Canonical records now retain the instruction count and header fingerprint derived from their actual payload. Cached and
uncached reads compare both against record metadata; false metadata cannot validate a matching false caller header. An
immutable signature may reuse an already-proven header match through a weak witness. Copy-on-write changes and
initialization/local flags invalidate that match without retaining parameter/type graphs. Legacy signature checks decode
callable metadata without constructing instruction objects.

Output preparation now completes serialization and flushing before releasing large compiler inputs. Root replacement
follows cleanup and another deadline check. A deterministic regression expires the deadline after successful
preparation, then verifies that the old output survives and the owned temporary file is removed. Standalone module,
inline, and package output follow this sequence.

The first metadata candidate exposed a late-publication problem: its largest incremental edit took 10.109 seconds
externally, while the native command reported success after 10.074 seconds. That candidate had replaced the output; the
external observer rejected it. A subsequent diagnostic edit reached the native deadline before publication and preserved
the previous root. Both failures remain in canonical-headers-initial-capacity-2026-10-28.json and
canonical-headers-overrun-trace-2026-10-28.json. The accepted implementation moves large cleanup ahead of the last
deadline check and reuses checked immutable header proofs.

The final sequential comparison preserved every artifact byte across four workloads. The 16,016-function control edit
changed from 2.045 seconds / 25,788,416 bytes peak compiler RSS to 2.038 seconds / 25,493,504 bytes; source-free linkage
changed from 1.605 seconds / 21,868,544 bytes to 1.523 seconds / 21,774,336 bytes.

The 15,001-function arithmetic edit changed from 6.655 seconds / 52,015,104 bytes to 6.399 seconds / 52,068,352 bytes.
Its source-free linkage regressed from 5.329 seconds / 34,549,760 bytes to 5.704 seconds / 34,668,544 bytes.

The 81,918-function exported edit changed from 9.444 seconds / 86,593,536 bytes to 9.232 seconds / 86,700,032 bytes. Its
source-free linkage changed from 8.206 seconds / 72,904,704 bytes to 7.906 seconds / 72,949,760 bytes. Cold population
changed from 10.616 to 9.930 seconds; clean/cold work may scale with input.

The equally large private-function edit regressed from 8.362 seconds / 69,447,680 bytes to 8.619 seconds / 69,615,616
bytes. Source-free linkage changed from 7.350 seconds / 61,837,312 bytes to 7.448 seconds / 57,188,352 bytes. Its cold
population regressed from 9.424 to 10.631 seconds. All observations, compiler and observer identities, and exact
artifact comparisons remain in canonical-header-publication-capacity-2026-10-28.json. These are compiler-process RSS
measurements, without a new aggregate-memory compliance claim.

All 419 native/CLI tests, sixteen original/restored executions, and all 753 formatted conformance cases in both output
modes passed. Runtime code and its prior 112-test qualification are unchanged. Public schemas and canonical bytes are
unchanged. Persistent declarations, the full source-line capacity target, and lazy runtime code loading remain
unfinished.


## Grouped linked function headers

The linker now moves each module's ordered function map into a grouped catalog. It no longer rebuilds a global header
table or clones per-module function-ID lists for package output. Inline output merges borrowed ordered iterators with
one cursor per nonempty module. Global duplicate checks precede entry resolution and initializer planning. One
spool-checked serializer serves both ordinary module maps and the grouped program view.

The sequential four-workload comparison preserved every root, code scene, and independently compiled module byte. The
16,016-function control edit changed from 1.921 seconds / 25,460,736 bytes peak compiler RSS to 1.876 seconds /
25,358,336 bytes. Source-free linkage changed from 1.680 seconds / 21,979,136 bytes to 1.606 seconds / 21,696,512 bytes.

The 15,001-function arithmetic edit changed from 6.799 seconds / 52,006,912 bytes to 6.445 seconds / 51,773,440 bytes.
Source-free linkage changed from 5.568 seconds / 34,529,280 bytes to 5.355 seconds / 34,361,344 bytes.

The 81,918-function exported edit regressed slightly from 9.215 seconds / 86,765,568 bytes to 9.271 seconds / 86,745,088
bytes. Source-free linkage changed from 8.327 seconds / 73,015,296 bytes to 7.879 seconds / 72,781,824 bytes. Cold
population regressed from 9.939 to 10.695 seconds; clean/cold work may scale with input.

The equally large private-function edit changed from 8.067 seconds / 69,226,496 bytes to 8.163 seconds / 68,911,104
bytes. Source-free linkage changed from 7.319 seconds / 56,868,864 bytes to 7.063 seconds / 55,062,528 bytes. Cold
population regressed from 8.987 to 9.456 seconds. All measurements, including mixed timings and small memory changes,
remain in grouped-headers-capacity-2026-10-28.json. These remain compiler-process RSS measurements rather than a new
aggregate-memory compliance claim.

All 421 native/CLI tests, sixteen original/restored executions, and all 753 formatted conformance cases in both output
modes passed. New regressions exercise nested module IDs that interleave with their parent's IDs, explicit ownership,
bounded cursor counts, duplicate identities, canonical bytes, and rejection of changed headers. Runtime code and its
prior 112-test qualification are unchanged. Completed declaration maps still remain resident; persistent storage, the
full source-line target, and lazy runtime code loading remain unfinished.


## Safe projected callback and covariant inputs

Projected receiver interfaces now derive both safe read views and safe input views. Callable parameters and
contravariant nominal arguments reverse the direction. Covariant containers can expose lower views such as
List<Nothing>; invariant inputs remain unavailable. Candidate nominal types must retain their bounds and capability
eligibility. Reads preserve non-null generic refinements. Direction belongs to the bounded node memo key, so a shared
graph can occur in input and result positions without reusing the wrong proof. No function conversion adapters are
introduced.

Nine source-free conformance cases cover generic and bound method references, broad callbacks, contravariant consumers,
empty immutable collections, non-null results, and suspending variadic callbacks with retained state. Negative cases
reject narrow callbacks, hidden-value producers, mutable invariant inputs, and numeric callback adapters. Native
regressions also traverse 28 repeated two-child graph layers with distinct input/read proofs and validate negative
fixtures independently of their rejected calls.

The reusable benchmark-typing.py projected-inputs workload contains 1,024 distinct erased-receiver callback call sites
in 96,723 source bytes. Its nine generated functions include three shared-binding initializers. The previous compiler
rejects the first callback reference; this is new accepted functionality rather than a like-for-like performance
comparison. Current population measured 0.626 seconds / 10,870,784 bytes peak compiler RSS, unchanged reuse 0.276
seconds / 4,255,744 bytes, and an incremental result edit 0.642 seconds / 11,038,720 bytes.

Independent module publication measured 0.316 seconds / 10,518,528 bytes. After source removal, inline linkage measured
0.529 seconds / 10,125,312 bytes and package linkage 0.622 seconds / 10,067,968 bytes. Source-free inline bytes exactly
matched the edited source build. Eight original/restored executions checked all 1,024 method calls, callbacks, typed
results, totals, and no replay across the original, edited, source-free inline, and packaged outputs. The reports are
projected-inputs-capacity-2026-10-28.json, projected-inputs-source-free-capacity-2026-10-28.json, and
projected-inputs-runtime-2026-10-28.json.

All 424 native/CLI tests and all 762 formatted conformance cases in both output modes passed. Runtime code and its prior
112-test qualification are unchanged. The measurements remain native compiler-process RSS observations, without a new
aggregate-memory or full source-line capacity claim. Public artifact, instruction, and checkpoint schemas are unchanged.


## Projected field mutability and alias-safe narrowing

Field proofs now consult the declaration's actual mutability through declarations::field_can_change. A projected
interface's write restriction does not freeze the underlying field: another concrete alias can still replace it. Such
paths remain transient across calls and suspension. Declared val fields and immutable entry fields retain stable proofs;
unknown metadata remains conservative. Assignment permissions and runtime metadata retain their existing rules.

A before/after probe confirms the bug: the previous compiler accepted a String narrowing through Box<*> after a concrete
alias assigned null. Its output failed at the original source line with null member access. The updated compiler rejects
that access as a nullable member receiver and preserves the previous output on failure. The source-free checkpoint
regression changes a projected var field through an alias, rechecks it after an input wait, and retains a separate
immutable field guard through a call and another wait. Its mutation counter remains one after restore.

The immutable 32-field workload's population changed from 0.785 seconds / 17,043,456 bytes peak compiler RSS to 0.845
seconds / 16,928,768 bytes. Its edit changed from 0.893 seconds / 16,961,536 bytes to 0.856 seconds / 17,084,416 bytes.
The mutable counterpart's population changed from 0.824 seconds / 16,965,632 bytes to 0.784 seconds / 16,977,920 bytes.
Its edit changed from 0.873 seconds / 17,006,592 bytes to 0.817 seconds / 16,982,016 bytes. These are bounded
compiler-process measurements, with the mixed observations retained in
projected-field-stability-capacity-2026-10-28.json.

Original and edited artifact bytes matched across both workloads; four executions checked their representative scene
transcripts. The before/after diagnostic and old-runtime failure remain in
projected-mutable-field-before-2026-10-28.json, projected-mutable-field-after-2026-10-28.json, and
projected-field-stability-runtime-2026-10-28.json. All 426 native/CLI tests and all 764 formatted conformance cases
passed in both output modes. Runtime code and its prior 112-test qualification are unchanged. Public artifact,
instruction, and checkpoint schemas are unchanged; the broader capacity target remains unfinished.


## Projected field capture acceptance

Three additional source-free cases exercise the same qualified compiler binary. An escaping anonymous function cannot
retain a projected mutable-field guard from its creation site; a declared val field retains its stable guard, and a
mutable field may be rechecked inside the callback before use. The positive callbacks retain their behavior after
checkpoint restore. All three additions passed with formatting in both output modes. Their disjoint ID sets were
combined with the preceding full 764-case logs under the same compiler SHA, and the union exactly matches all 767
current case IDs. This qualification is recorded in closure-field-audit-conformance-2026-10-28.json; the unchanged
native/CLI suite remains 426 tests. No compiler, runtime, or public schema change was needed beyond the preceding
mutability fix.

## Single-use delayed-val initialization

Parameterless local helpers can now initialize delayed vals under a limited single-use proof. The helper must be
declared directly in the complete outer function or scene body, after its protected declarations. Its body contains only
distinct plain assignments with effect-free value expressions, and exactly one later standalone call is its only
reference. The original declaration retains its definition-site checks and name; the call expands through caller flow
checks and the unused runtime helper becomes a capture-free Unit stub. Broader callable initialization remains
conservative.

The 1,024-helper workload checks chained immutable values and shared counters. Population measured 0.921 seconds /
24,612,864 bytes peak compiler RSS, unchanged reuse 0.265 seconds / 4,120,576 bytes, and the edit 1.017 seconds /
24,580,096 bytes. The preceding compiler rejects this new val pattern, so that rejection is not a like-for-like speed
comparison. An existing 1,900-cell mutable-helper control retained identical original and edited artifact bytes:
population changed from 0.757 seconds / 18,206,720 bytes to 0.813 seconds / 18,075,648 bytes, and its edit from 0.772
seconds / 18,427,904 bytes to 0.742 seconds / 18,382,848 bytes.

Independent module publication measured 0.566 seconds / 24,309,760 bytes. Source-free inline linking measured 0.555
seconds / 10,022,912 bytes and package linking 0.612 seconds / 10,092,544 bytes; the edited inline bytes matched the
source build exactly. These bounded observations measure compiler-process RSS, rather than full cross-platform aggregate
memory compliance.

All 389 native tests and 40 CLI tests passed, together with strict all-target Clippy and formatting checks. All 775
formatted conformance cases passed in both output modes. Ten additional workload executions checked four val outputs
before and after checkpoint restore, plus the original and edited mutable control. Every delayed val was immutable,
initialized, and correct across restore; no expanded helper call or completed initialization replayed. The observer
initially counted generated shared initializer functions as source helpers; the corrected observer distinguishes source-
local IDs, and the initial failure logs remain retained.

Raw evidence is retained in single-use-initializers-capacity-2026-10-28.json, single-use-initializers-source-free-
capacity-2026-10-28.json, single-use-initializers-runtime-2026-10-28.json, and single-use-initializers-
conformance-2026-10-28.json. Runtime source and its prior 112-test qualification are unchanged. Public artifact and
checkpoint schemas are unchanged, and the larger source-line capacity target remains unfinished.

## Specialized method-reference effects

An explicit generic method reference previously hid effects in its receiver from expression-purity analysis. A short-
circuit condition or when case could restore a non-null fact after that receiver cleared the variable. Three
before/after probes cover positive AND, negated OR, and an interpolated when case: the previous compiler accepted each
program and its output failed with null member access. The corrected compiler rejects all three at the original Bifoil
access and preserves the previous completed output.

Effect traversal now visits type-application callees and generated construction fields and explicitly handles every
expression variant. Single-use initializer eligibility reuses that traversal. Pure references preserve valid guards,
immutable lexical values remain stable through effects, and a new current-value check can restore narrowing. The source-
free positive case retains receiver evaluation counts and a saved callback across an input checkpoint.

All 391 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 779 formatted
conformance cases passed in both output modes. The existing 1,024-callback workload retained identical original and
edited artifact bytes; four original/restored executions checked every callback/result and no replay. Population changed
from 0.630 seconds / 10,797,056 bytes peak compiler RSS to 0.598 seconds / 10,784,768 bytes. The edit changed from 0.686
seconds / 10,964,992 bytes to 0.624 seconds / 11,124,736 bytes; this bounded observation includes a small RSS increase.

Raw evidence is retained in generic-reference-effects-diagnostics-2026-10-28.json, generic-reference-effects-
capacity-2026-10-28.json, generic-reference-effects-runtime-2026-10-28.json, and generic-reference-effects-
conformance-2026-10-28.json. An initial private Node invocation used a WSL path with the Windows executable; its path-
error log remains retained, and the corrected invocation passed without repeating the successful resource measurements.
Runtime code and its prior 112-test qualification are unchanged. Public formats, resource guards, and the wider capacity
target remain unchanged.

## Scoped delayed-val initialization

Single-use initializer discovery now follows every complete lexical statement body. Conditionals, for/while loops,
choice options, prose directives, and value blocks use the same proof as outer function bodies. A do/while condition
joins the body's reference inventory before function declaration checking, preventing a hidden second invocation or
escaping reference through that shared scope. The protected delayed val must belong to that same body, so repeated loop
helpers cannot initialize an outer val repeatedly. Each loop iteration receives fresh body bindings.

Five added cases cover source-free initialization after a player-input wait, all ordinary nested bodies, readers
retaining separate loop bindings, hidden condition references, outer-loop writes, and competing earlier initialization.
All 393 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 784 formatted
conformance cases passed in both output modes. A generated native fixture initially indented only the first row of two
nested blocks; the fixture was repaired without a production change, and its failed log remains retained.

The existing 1,024-helper outer-body control retains identical original and edited output bytes. Population changed from
0.871 seconds / 24,530,944 bytes peak compiler RSS to 0.893 seconds / 24,539,136 bytes; its edit changed from 0.929
seconds / 24,928,256 bytes to 0.838 seconds / 24,817,664 bytes. The new 1,024-block workload retains every initialized
immutable cell through a reader. Population measured 0.812 seconds / 17,858,560 bytes, unchanged reuse 0.278 seconds /
4,120,576 bytes, and its edit 0.836 seconds / 18,485,248 bytes. The previous compiler rejects this new form, so the
rejection is not a speed comparison.

Independent publication of the 2,053-function module measured 0.389 seconds / 17,985,536 bytes. Source-free inline
linking measured 0.813 seconds / 14,757,888 bytes, and package linking 0.955 seconds / 14,721,024 bytes. The source-free
edited inline bytes matched the source build exactly. Twelve additional executions checked all immutable cells before
and after restore: eight scoped-workload executions and four outer-body control executions. The scoped checks also
verified 1,024 distinct retained cells and zero initializer calls or replay. Its observer initially assumed builtin
callables had function IDs; the guarded observer passed, with the initial observer-error log retained.

Raw evidence is retained in scoped-val-initializers-capacity-2026-10-28.json, scoped-val-initializers-scoped-
runtime-2026-10-28.json, scoped-val-initializers-control-runtime-2026-10-28.json, and scoped-val-initializers-
conformance-2026-10-28.json. Runtime source and its prior 112-test qualification are unchanged. Public formats, resource
guards, broader callable proofs, and the larger capacity target remain unchanged.

## Parameterized single-use initializers

Single-use helpers with typed parameters now retain their real call frames and captures. Normal argument lowering
handles named/default/variadic inputs, local var parameters, conversions, and suspended arguments. This proof still
requires a non-suspending nongeneric Unit helper with distinct plain assignments and effect-free values/defaults. It
inventories supplied arguments as well as bodies and tails, rejecting hidden extra invocations or escapes. Complete
immutable-write evidence is mandatory, and the compiler checks that arguments have not initialized a protected val
first. The private permission retains lexical slot, owner, runtime name, and binding identity; public formats are
unchanged.

Seven new cases cover source-free generic payloads, current per-call defaults, var-parameter isolation, variadics,
suspended arguments, retained loop readers, hidden argument references, competing writes, effectful defaults, and
overload rejection. All 397 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 791
formatted conformance cases passed in both output modes. The initial native default-transfer fixture had a misplaced
closing delimiter; the corrected fixture passed without a production change, and the initial failure log remains
retained.

The previous 1,024-block control retains identical original and edited artifact bytes. Population changed from 0.867
seconds / 17,940,480 bytes peak compiler RSS to 0.854 seconds / 18,157,568 bytes; unchanged reuse from 0.288 seconds /
4,141,056 bytes to 0.370 seconds / 4,317,184 bytes; and its edit from 0.925 seconds / 18,534,400 bytes to 1.055 seconds
/ 18,411,520 bytes. The mixed observations include slower control reuse/edit timing. The new parameterized workload
measured 1.029 seconds / 21,356,544 bytes for population, 0.321 seconds / 4,194,304 bytes for unchanged reuse, and 0.994
seconds / 21,835,776 bytes for its edit. The previous compiler rejects that new form; this is not a like-for-like speed
comparison.

Publication of its 2,053-function module measured 0.567 seconds / 21,245,952 bytes. Source-free inline linking measured
1.002 seconds / 15,241,216 bytes, and package linking 1.124 seconds / 15,261,696 bytes. Edited inline bytes matched the
source build exactly. Twelve additional executions checked all 1,024 immutable captures before and after restore: eight
parameterized-workload executions and four expansion-control executions. Each original parameterized run invoked 1,024
real helpers; restoration replayed none. The observer also verified distinct retained cells and every value.

Raw evidence is retained in parameter-initializers-capacity-2026-10-28.json, parameter-initializers-parameters-
runtime-2026-10-28.json, parameter-initializers-control-runtime-2026-10-28.json, and parameter-initializers-
conformance-2026-10-28.json. Runtime source and its prior 112-test qualification are unchanged. These bounded compiler-
process RSS observations do not establish aggregate cross-platform compliance or the much larger source-line capacity
target.

## Structured and suspending single-use helpers

Single-use Unit helpers now support structured bodies and explicit suspension. Captured-write discovery includes nested
statements; ordinary immutable-flow checks enforce at-most-once writes, while completed instruction proofs require every
protected val on every normal return. Resolved non-Unit helpers lose this permission before body checking. A Boolean
constant fixes a CFG edge only when the constant and branch share a basic block; a branch entered by short-circuit jumps
retains its alternatives. Pure defaults, nongeneric headers, same-body delayed declarations, and one standalone call
remain required.

Nine added cases cover source-free branches, chained writes, constant loop entry, suspension before a write and between
two writes, partial branches/choices, early returns, internal repetition, short-circuit entries, and reads before
initialization. All 400 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 800
formatted conformance cases passed in both output modes. The runtime restores staged immutable initialization without
repeating completed writes; its code and prior 112-test qualification are unchanged.

The previous parameterized control retains identical original and edited artifact bytes. Population changed from 1.126
seconds / 21,250,048 bytes peak compiler RSS to 1.214 seconds / 21,311,488 bytes, and its edit from 1.104 seconds /
21,704,704 bytes to 1.058 seconds / 21,962,752 bytes. These mixed observations include higher RSS and slower population.
The new 1,024-branch workload measured 1.059 seconds / 26,853,376 bytes for population, 0.336 seconds / 4,100,096 bytes
for unchanged reuse, and 1.091 seconds / 27,291,648 bytes for its edit. The previous compiler rejects that new form, so
the rejection is not a speed comparison.

Publication of its 2,053-function module measured 0.648 seconds / 26,779,648 bytes. Source-free inline linking measured
1.118 seconds / 15,183,872 bytes, and package linking 1.239 seconds / 15,495,168 bytes. Edited inline bytes matched the
source build exactly. Twelve additional executions checked every one of the 1,024 distinct immutable captures before and
after restore, with 1,024 real helper calls per original run and none replayed after restoration.

Raw evidence is retained in structured-initializers-capacity-2026-10-28.json, structured-initializers-structured-
runtime-2026-10-28.json, structured-initializers-control-runtime-2026-10-28.json, and structured-initializers-
conformance-2026-10-28.json. Public formats and resource guards are unchanged. The bounded compiler-process observations
do not establish aggregate cross-platform compliance or the much larger source-line capacity target.


## Publication header lifetime

The linked function catalog now consumes complete signatures into existing header checksums after semantic planning.
Output preparation retains grouped IDs and digests while actual spool bytes, instruction counts, and header fingerprints
remain checked. Native ownership tests verify that complete signatures are released before serialization. This does not
move declaration interfaces to disk or establish an overall peak-memory reduction.

All 403 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 800 formatted
conformance cases passed in both output modes. Six graph outputs and two arithmetic outputs also passed
original/restored execution checks, checking every routine, retained reference, result, and final total across sixteen
executions without replay. Runtime source and its prior 112-test qualification are unchanged.

The initial comparison retained identical artifacts for the control, arithmetic, and private-header workloads. The old
compiler's 81,918-function edit failed its time guard at 9.631 seconds; its previous completed root was preserved. The
candidate completed that edit in 8.967 seconds / 86,953,984 bytes peak compiler RSS. That failed baseline run is
retained rather than treated as a successful byte comparison.

A fresh large-workload comparison completed for both compilers and verified identical bytes across all 39 artifacts. The
edit changed from 9.111 seconds / 86,462,464 bytes before to 9.226 seconds / 86,908,928 bytes after. Source-free linkage
changed from 8.231 seconds / 72,826,880 bytes to 7.999 seconds / 72,929,280 bytes. Cold population changed from 11.351
to 10.022 seconds; clean/cold builds scale with input and do not use the fixed incremental deadline.

The initial arithmetic edit changed from 6.585 seconds / 51,789,824 bytes to 6.752 seconds / 51,941,376 bytes; its
source-free link changed from 5.396 to 5.636 seconds. The private-header edit changed from 7.694 seconds / 68,841,472
bytes to 8.080 seconds / 68,837,376 bytes; its source-free link changed from 7.117 to 6.834 seconds. These mixed
observations include regressions. Earlier loading and compilation still dominate observed peaks, so releasing signatures
late does not imply a general peak-memory win.

Raw evidence is retained in header-publication-initial-capacity-2026-10-28.json,
header-publication-large-followup-capacity-2026-10-28.json, header-publication-runtime-graph-2026-10-28.json,
header-publication-runtime-code-2026-10-28.json, and header-publication-conformance-2026-10-28.json. Public formats and
resource guards are unchanged. These compiler-process samples do not prove aggregate cross-platform compliance or the
much larger source-line capacity target.


## Generic helpers and separate choice proofs

Generic single-use Unit helpers now retain normal parameter/type arguments and real call frames. Captured-write analysis
separates sequential and nested choice destinations, while entry-read analysis distinguishes waiting from actual reads
or callbacks. Explicit type applications preserve normal specialization checks even where ordinary calls would qualify
for compact expansion.

All 409 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Thirteen added conformance
cases cover generic results, nullable inputs, definition checking, bounds, escaped/repeated specializations, argument
writes, generic waits before and after initialization, nested fallbacks, and real preparation reads. All 813 formatted
cases passed in both output modes. Runtime source and its prior 112-test qualification remain unchanged.

The existing 1,024-branch control retained identical original and edited artifact bytes. Population changed from 1.108
seconds / 26,681,344 bytes peak compiler RSS to 1.230 seconds / 26,857,472 bytes, and its edit from 1.224 seconds /
27,381,760 bytes to 1.120 seconds / 27,025,408 bytes. These are mixed observations. The new 1,024-generic-helper
workload measured 1.182 seconds / 28,995,584 bytes for population, 0.344 seconds / 4,280,320 bytes for unchanged reuse,
and 1.263 seconds / 29,347,840 bytes for its edit. The previous compiler rejects the new form; that is not a speed
comparison.

Publication of its 2,053-function module measured 0.734 seconds / 28,762,112 bytes. Source-free inline linkage measured
0.923 seconds / 17,473,536 bytes and package linkage 1.113 seconds / 17,453,056 bytes. Edited inline bytes matched the
source build exactly. Twelve additional original/restored executions checked all 1,024 distinct immutable cells and
every value; each original run invoked 1,024 real helpers and restoration replayed none. The generic observer verifies
every resolved Int argument, combining specialized-callable and call-instruction metadata.

Initial diagnostic-assertion, source-free proof, and observer failures remain recorded. The observer initially inspected
only call-instruction type arguments; specialization also stores them on the callable. Correcting the observer required
no compiler change or repeated resource measurement. Raw evidence is retained in
generic-initializers-capacity-2026-10-28.json, generic-initializers-runtime-2026-10-28.json, and
generic-initializers-conformance-2026-10-28.json. Public formats and guards are unchanged; larger source-line capacity
and aggregate cross-platform resource acceptance remain unfinished.


## Return values from single-use helpers

Single-use block helpers now retain ordinary returned values and can supply the complete initializer of a val or var
declaration. A call consumed by a binding keeps its real frame even when it returns Unit. The proof still requires every
protected immutable binding at every normal return; partial returns, escapes, repeated calls, argument-side writes, and
incompatible result annotations remain errors.

All 412 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Eight added cases cover
inferred/explicit results, Unit-valued bindings, generic returned readers, suspended return values, and the rejected
paths above. All 821 formatted conformance cases passed in both output modes. Runtime source and its prior 112-test
qualification are unchanged.

The existing 1,024-generic-helper control retained identical original and edited artifact bytes. Population changed from
1.220 seconds / 29,036,544 bytes peak compiler RSS to 1.167 seconds / 29,097,984 bytes, and its edit from 1.226 seconds
/ 29,388,800 bytes to 1.365 seconds / 29,499,392 bytes. These mixed observations include slower edited builds and higher
RSS. The new returning workload measured 1.183 seconds / 29,368,320 bytes for population, 0.360 seconds / 4,173,824
bytes for unchanged reuse, and 1.199 seconds / 29,712,384 bytes for its edit. Its source retains 1,024 immutable readers
and 1,024 returned Strings. The previous compiler rejects this new form; the rejection is not a speed comparison.

Publication of the 2,054-function module measured 0.701 seconds / 29,032,448 bytes. Source-free inline linkage measured
0.997 seconds / 21,016,576 bytes and package linkage 1.084 seconds / 20,992,000 bytes. Edited inline bytes matched the
source build exactly. Twelve additional original/restored executions verified all distinct immutable cells and returned
values across original, edited, and source-free outputs plus the control. Each original run invoked 1,024 helpers;
restored runs replayed none.

The source generator now selects named scoped-val modes instead of combining independent flags. Four existing workload
sources retain identical SHA-256 values. Raw evidence is retained in returning-initializers-capacity-2026-10-28.json,
returning-initializers-runtime-2026-10-28.json, and returning-initializers-conformance-2026-10-28.json. Public formats
and resource guards are unchanged. These bounded compiler-process observations do not establish the full source-line
target or aggregate cross-platform resource compliance.


## Expression-bodied single-use helpers

Expression-bodied named helpers now use the same captured-val inventory and completed-write proof as block helpers.
Final branch expressions retain normal result checking. Partial branches, early returns without writes, repeated calls,
and incompatible result annotations remain rejected. No public format or runtime change is required.

All 415 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Seven new cases cover
source-free if/when results, Unit calls, generic returned readers, staged suspension, and invalid paths/results. All 828
formatted conformance cases passed in both output modes. The initial diagnostic assertion expected a return-check
message; it was aligned with the existing branch-check message without a production change, and the failed log remains
retained. Runtime source and its prior 112-test qualification are unchanged.

The previous 1,024-return control retained identical original and edited artifact bytes. Population changed from 1.358
seconds / 29,310,976 bytes peak compiler RSS to 1.185 seconds / 29,220,864 bytes, and its edit from 1.259 seconds /
29,405,184 bytes to 1.158 seconds / 29,609,984 bytes. The edited peak increased despite faster timing. The expression
workload measured 1.404 seconds / 31,801,344 bytes for population, 0.429 seconds / 4,190,208 bytes for unchanged reuse,
and 1.358 seconds / 32,239,616 bytes for its edit. The previous compiler rejects this form, so the rejection is not a
speed comparison.

Publication of its 2,054-function module measured 0.844 seconds / 31,297,536 bytes. Source-free inline linkage measured
1.134 seconds / 20,987,904 bytes and package linkage 1.207 seconds / 20,877,312 bytes. Edited inline bytes matched the
source build exactly. Twelve additional original/restored executions checked every distinct immutable cell and all 1,024
returned Strings; restoration replayed no initializing calls.

The reusable tools/compare-typing.py observer replaces disposable comparison scripts. It requires a new output
directory, records compiler/observer identities, retains failures, validates expected older-compiler rejection, and
checks control/source-free bytes. Five earlier workload sources retain identical SHA-256 values. Raw evidence is
retained in expression-initializers-capacity-2026-10-28.json, expression-initializers-runtime-2026-10-28.json, and
expression-initializers-conformance-2026-10-28.json. These bounded compiler-process samples do not establish aggregate
cross-platform compliance or the larger source-line capacity target.


## Callable defaults in initialization helpers

Single-use helpers now admit callable defaults through ordinary lowering. Selected defaults preserve effects and
suspension; supplied arguments skip omitted-default work and reads. Normal-return proofs and immutable checks still
reject skipped writes, early returns, real uninitialized reads, and double writes. A former implementation-limit
rejection was migrated to a positive case, with its old source and metadata retained in
default-initializers-case-migration-2026-10-28.json.

All 418 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Seven new
source-free/diagnostic cases plus the migrated case cover evaluation order, saved default calls, skipped uninitialized
defaults, callback reads, and invalid writes/returns. All 835 formatted cases passed in both output modes. The semantic
tests were divided by proof responsibility without changing the existing 34 test bodies. Runtime source and its prior
112-test qualification are unchanged.

The 1,024-branch control retains identical original and edited output bytes. Population changed from 1.070 seconds /
26,775,552 bytes peak compiler RSS to 1.153 seconds / 27,074,560 bytes; its edit changed from 1.166 seconds / 27,254,784
bytes to 1.157 seconds / 27,430,912 bytes. These mixed samples include higher RSS and slower population. The default
workload measured 1.240 seconds / 27,451,392 bytes for population, 0.326 seconds / 3,981,312 bytes for unchanged reuse,
and 1.136 seconds / 27,725,824 bytes for its edit. The previous compiler rejects this form; its rejection is not a speed
comparison.

Publication of the 2,055-function module measured 0.653 seconds / 27,336,704 bytes. Source-free inline linkage measured
0.955 seconds / 15,343,616 bytes and package linkage 1.091 seconds / 15,421,440 bytes. Edited inline bytes matched the
source build exactly. Twelve original/restored executions checked every distinct immutable cell and value. The original
workload called all 1,024 defaults once; the edited build supplied the final argument and called 1,023 defaults.
Restored runs replayed neither defaults nor initializers.

Six earlier generated workload sources retain identical SHA-256 values. Raw evidence is retained in
default-initializers-capacity-2026-10-28.json, default-initializers-runtime-2026-10-28.json, and
default-initializers-conformance-2026-10-28.json. Public formats and resource guards remain unchanged. Larger
source-line capacity and aggregate cross-platform resource acceptance remain unfinished.


## Assignment and return initialization calls

Single-use call results now support plain named assignments and direct returns. Grouping uses normal callable
resolution, while one shared predicate limits compact expansion to bare calls. Grouped calls retain real frames,
captured writes, and runtime checks even when their result is unused. Assignment rechecking still rejects a helper that
writes the surrounding assignment's own immutable target.

All 422 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Nine added cases cover
source-free assignments/returned generic readers, suspended RHS results, competing writes, extra uses, result
mismatches, and grouped type-argument checks. An unused parenthesized call retains its expected division-by-zero runtime
failure. All 844 formatted conformance cases passed in both output modes. One fixture initially used an unsupported
grouped generic-callee spelling; grouping the complete typed call exercised the intended checker without a production
change. Its failed log remains retained. Runtime source and its prior 112-test qualification are unchanged.

The 1,024-default control retains identical original and edited output bytes. Population changed from 1.373 seconds /
27,467,776 bytes peak compiler RSS to 1.240 seconds / 27,422,720 bytes, and its edit from 1.298 seconds / 27,766,784
bytes to 1.265 seconds / 27,844,608 bytes. Unchanged reuse changed from 0.395 to 0.413 seconds; observations remain
mixed. The separately assigned-result workload measured 1.302 seconds / 31,371,264 bytes for population, 0.409 seconds /
4,190,208 bytes for unchanged reuse, and 1.383 seconds / 31,617,024 bytes for its edit. The older compiler rejects the
new form, so its rejection is not a speed comparison.

Publication of its 2,054-function module measured 0.721 seconds / 31,281,152 bytes. Source-free inline linkage measured
1.202 seconds / 21,192,704 bytes and package linkage 1.520 seconds / 21,291,008 bytes. Edited inline bytes matched the
source build exactly. Twelve additional original/restored executions checked every distinct immutable cell and assigned
String; restoration replayed no initialization. The control also retained the expected default counts. Seven earlier
generated workload sources keep identical SHA-256 values.

Raw evidence is retained in assignment-initializers-capacity-2026-10-28.json,
assignment-initializers-runtime-2026-10-28.json, and assignment-initializers-conformance-2026-10-28.json. Public formats
and resource guards are unchanged. These bounded compiler-process samples do not establish aggregate cross-platform
compliance or the larger source-line capacity target.


## Addressed initialization calls

Plain member/list/map assignments now consume proven single-use initializing calls through normal address lowering.
Receiver and key/index expressions join the use inventory; hidden calls/references, target-side writes, incompatible
results, and immutable containers remain rejected. Source-free execution covers five active saved waits, target/argument
order, receiver replacement during a suspended RHS, and an old object retained only by its pending address.

All 426 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Six additions bring the
formatted corpus to 850 cases, all passing in both output modes. Runtime source and its prior 112-test qualification are
unchanged.

The separately assigned-result control retains identical original and edited artifact bytes. Population changed from
1.430 seconds / 31,367,168 bytes peak compiler RSS to 1.259 seconds / 31,444,992 bytes, and its edit from 1.392 seconds
/ 31,645,696 bytes to 1.378 seconds / 31,825,920 bytes. These mixed samples include higher RSS. The indexed workload
measured 1.236 seconds / 28,880,896 bytes for population, 0.353 seconds / 4,030,464 bytes for unchanged reuse, and 1.305
seconds / 28,733,440 bytes for its edit. It starts with 1,024 list slots and stores each helper's returned String by
index. The older compiler rejects the form; this is not a speed comparison against a successful old build.

Publication of its 2,054-function module measured 0.742 seconds / 28,418,048 bytes. Source-free inline linkage measured
1.075 seconds / 16,019,456 bytes and package linkage 1.065 seconds / 16,076,800 bytes. Edited inline bytes matched the
source build exactly. Twelve original/restored executions checked all distinct immutable cells and returned values.
Independent address observation also verified each of the 1,024 receiver IDs, numeric indexes, and written Strings;
restoration replayed no writes or initialization. Eight earlier workload sources retain identical SHA-256 values.

Raw evidence is retained in address-initializers-capacity-2026-10-28.json, address-initializers-runtime-2026-10-28.json,
and address-initializers-conformance-2026-10-28.json. Public formats and resource guards are unchanged. Larger
source-line capacity and aggregate cross-platform resource acceptance remain unfinished.


## Compound initialization calls

Single-use initializing calls now supply compound RHS results through ordinary target-first lowering. Five new cases
cover guarded nullable fairmath targets, saved operands across six input waits, String mutation during the RHS, and
rejected uninitialized reads, repeated/hidden calls, and incompatible results. All 430 native tests, 40 CLI tests,
strict all-target Clippy, and formatting checks passed. All 855 formatted cases passed in both output modes. Runtime
source and its prior 112-test qualification are unchanged.

The indexed plain-write control retains identical original and edited bytes. Population changed from 1.179 seconds /
28,893,184 bytes peak compiler RSS to 1.134 seconds / 28,966,912 bytes; its edit changed from 1.275 seconds / 29,110,272
bytes to 1.248 seconds / 29,093,888 bytes. These samples include higher population RSS. The compound workload measured
1.197 seconds / 33,705,984 bytes for population, 0.406 seconds / 4,222,976 bytes for unchanged reuse, and 1.321 seconds
/ 34,025,472 bytes for its edit. Each helper changes the target before returning, and the caller must concatenate the
original captured value. The old compiler rejects this form rather than completing a comparable build.

Publication of its 2,054-function module measured 0.749 seconds / 33,558,528 bytes. Source-free inline linkage measured
1.208 seconds / 20,385,792 bytes and package linkage 1.226 seconds / 20,492,288 bytes. Source-free inline bytes match
the edited source build. Twelve original/restored executions checked all immutable cells and returned Strings, with
independent observation of all 2,048 compound writes per new execution and no replay after restoration. Nine earlier
workload sources retain identical SHA-256 values.

Raw evidence is retained in compound-initializers-capacity-2026-10-28.json,
compound-initializers-runtime-2026-10-28.json, and compound-initializers-conformance-2026-10-28.json. Public formats and
resource guards are unchanged. These bounded compiler-process samples do not establish the much larger capacity target
or aggregate cross-platform resource compliance.


## Initialization within expression operands

Named single-use initializing calls now participate in ordinary operands, arguments, collections, String interpolation,
and assignment targets. Nested Unit calls preserve their real writes instead of entering standalone expansion.
Short-circuit and nullable safe-access paths keep ordinary definite-assignment joins. Eight added source-free/diagnostic
cases bring the formatted corpus to 863 cases, all passing in both output modes, alongside 437 native tests and 40 CLI
tests. Strict all-target Clippy and formatting checks passed. Runtime source and its prior 112-test qualification remain
unchanged. The first focused run used an incorrect diagnostic substring in two assertions; those fixture-only failures
remain recorded, followed by successful corrected checks.

The compound control retains identical original and edited bytes. Population changed from 1.238 seconds / 33,484,800
bytes peak compiler RSS to 1.231 seconds / 33,529,856 bytes; its edit changed from 1.350 seconds / 33,820,672 bytes to
1.296 seconds / 34,111,488 bytes. These samples include higher RSS. The operand workload measured 1.312 seconds /
29,253,632 bytes for population, 0.410 seconds / 4,153,344 bytes for unchanged reuse, and 1.364 seconds / 29,544,448
bytes for its edit. It stores the concatenation of a prefix and each of 1,024 helper results. The old compiler rejects
this form rather than completing a comparable build.

Publication of the 2,054-function module measured 0.733 seconds / 29,196,288 bytes. Source-free inline linkage measured
1.145 seconds / 20,344,832 bytes and package linkage 1.205 seconds / 20,459,520 bytes. Edited inline bytes matched the
source build. Twelve original/restored executions checked every distinct immutable cell and retained String, actual
receiver/index/value writes, and no replay. Ten prior workload sources retain identical SHA-256 values.

Raw evidence is retained in expression-initializers-capacity-2026-10-28.json,
expression-initializers-runtime-2026-10-28.json, and expression-initializers-conformance-2026-10-28.json. Public formats
and resource guards are unchanged. Larger capacity and aggregate cross-platform resource acceptance remain unfinished.


## Structured initialization sites

Single-use initializing calls now participate in prose and structured expression/control sites, including if/when tests,
case guards, choice preparation and selected bodies, and once-evaluated for iterables. Repeated loop regions and
deferred callable bodies retain separate restrictions. Selected options keep only exact-cell initialization facts from
their own preparation; later effects do not regain stale type or value facts.

All 450 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Thirteen additions bring the
formatted corpus to 876 cases, all passing in both output modes. Runtime source and its prior 112-test qualification are
unchanged. Initial source-free tests exposed the lost selected-preparation facts, which were corrected. A negative
sole-once fixture was corrected to include a fallback: without one, consumption fails fast instead of reaching the
uninitialized continuation. Original failed checks and the fixture correction remain in private reports.

The operand control retains identical original and edited bytes. Population changed from 1.146 seconds / 29,364,224
bytes peak compiler RSS to 1.210 seconds / 29,294,592 bytes; its edit changed from 1.244 seconds / 29,327,360 bytes to
1.221 seconds / 29,650,944 bytes. These mixed samples include slower population and higher edit RSS. The prose workload
measured 1.320 seconds / 35,614,720 bytes for population, 0.399 seconds / 4,161,536 bytes for unchanged reuse, and 1.460
seconds / 35,885,056 bytes for its edit. The caller emits each of 1,024 helper results through prose while retaining
separately checked immutable cells and stored Strings. The old compiler rejects this form rather than completing a
comparable build.

Publication of the 2,054-function module measured 0.903 seconds / 35,446,784 bytes. Source-free inline linkage measured
1.285 seconds / 17,465,344 bytes and package linkage 1.382 seconds / 17,514,496 bytes. Edited inline bytes match the
source build. Twelve original/restored executions checked every distinct immutable cell, returned String, prose
paragraph, and control address write, without replaying initialization or output. Eleven earlier workload sources retain
identical SHA-256 values.

Raw evidence is retained in structured-sites-capacity-2026-10-28.json, structured-sites-runtime-2026-10-28.json, and
structured-sites-conformance-2026-10-28.json. Public formats and resource guards are unchanged. These bounded
compiler-process samples do not establish the larger source-line target or aggregate cross-platform resource compliance.


## Forward compact initialization calls

Proven compact helpers can run before their source declaration after their protected cells exist. Caller flow remains
initialized while the original declaration is checked against fresh protected-cell flags in its own environment. Broader
forward captured writes remain conservative. All 454 native tests, 40 CLI tests, strict all-target Clippy, and
formatting checks passed. Seven additions bring the formatted corpus to 883 cases, all passing in both output modes.
Runtime source and its prior 112-test qualification are unchanged. An initial assertion used "unknown binding" where the
compiler reports "unknown name"; the failed fixture check and corrected passing checks remain recorded.

The scoped control retains identical original and edited bytes. Population changed from 1.087 seconds / 18,366,464 bytes
peak compiler RSS to 0.780 seconds / 18,337,792 bytes; its edit changed from 0.844 seconds / 18,526,208 bytes to 0.833
seconds / 18,657,280 bytes. These mixed samples include higher edit RSS. The forward workload measured 0.973 seconds /
18,182,144 bytes for population, 0.394 seconds / 3,768,320 bytes for unchanged reuse, and 0.844 seconds / 18,595,840
bytes for its edit. The 1,024 helper calls precede their definitions, with distinct retained immutable cells. The old
compiler rejects this form instead of completing a comparable build.

Publication of its 2,053-function module measured 0.476 seconds / 18,100,224 bytes. Source-free inline linkage measured
0.711 seconds / 14,778,368 bytes and package linkage 0.783 seconds / 14,688,256 bytes. Edited inline bytes match the
source build. Twelve original/restored executions checked every immutable cell and reader value, compact stubs, and no
replay. Twelve prior workload sources retain identical SHA-256 values.

Raw evidence is retained in forward-initializers-capacity-2026-10-28.json, forward-initializers-runtime-2026-10-28.json,
and forward-initializers-conformance-2026-10-28.json. Public formats and resource guards are unchanged. These bounded
compiler-process samples do not establish the larger source-line capacity target or aggregate cross-platform compliance.


## Shared empty header and type storage

The compiler shares empty parameter/type lists and common immutable return-type shapes during source construction and
metadata decoding. It retains typed decoding, nonempty parameter flags, current-header derivation, and copy-on-write
isolation. Public formats, fingerprints, and resource guards are unchanged. All 456 native tests, 40 CLI tests, strict
all-target Clippy, and formatting checks passed. All 883 formatted cases passed in both output modes; sixteen
original/restored workload executions independently checked every routine reference and result through 81,918-function
programs, without replay. Runtime source and its prior 112-test qualification are unchanged.

Four workload comparisons establish identical canonical bytes for 132 module, original, and edited artifacts. The
16,016-function control edit changed from 2.012 seconds / 25,489,408 bytes peak compiler RSS to 1.824 seconds /
24,948,736 bytes. The 15,001-function arithmetic edit changed from 6.937 seconds / 51,781,632 bytes to 6.343 seconds /
51,367,936 bytes; its source-free link changed from 5.357 seconds / 34,467,840 bytes to 5.588 seconds / 34,648,064
bytes, including higher RSS and a slower source-free sample.

The private 81,918-function edit changed from 7.902 seconds / 68,616,192 bytes to 8.250 seconds / 65,347,584 bytes.
Source-free linkage changed from 7.393 seconds / 53,997,568 bytes to 6.963 seconds / 51,482,624 bytes. The lower RSS
does not imply an improvement for every elapsed-time sample.

The initial exported-graph old-compiler edit exhausted its time guard at 9.628 seconds; its previous root and all
thirteen original package files were independently preserved. The candidate completed that edit at 9.072 seconds, but
this is not a comparison of two successful edits. A fresh paired run completed on both compilers and matched all 39
artifacts. There, the edit changed from 9.288 seconds / 86,941,696 bytes to 9.360 seconds / 83,361,792 bytes;
source-free linkage changed from 8.114 seconds / 72,994,816 bytes to 8.174 seconds / 70,025,216 bytes. The repeated
candidate's 39 artifacts also matched its initial run. These are mixed timing samples with lower observed peak RSS.

The qualified aggregate is shared-empty-types-capacity-2026-10-28.json. The initial failed measurement, preserved-output
audit, and successful paired follow-up remain separately recorded. Raw runtime and both-mode conformance reports use the
same shared-empty-types prefix. These bounded compiler-process observations do not establish aggregate cross-platform
compliance, disk-backed interfaces, lazy runtime loading, or the much larger source-line target.


## Buffered canonical fingerprint writes

Canonical serialization now batches small SHA-256 updates in a fixed 1 KiB buffer while checking the logical-byte budget
before buffering. Larger fragments feed the hash directly after flushing pending bytes. Independent digest regressions
cover partition boundaries, empty writes, explicit flushes, escaped Unicode, and JSON serialization. All 458 native
tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. All 883 formatted cases passed in both
output modes; sixteen original/restored executions checked every routine/result without replay. Runtime source and its
prior 112-test qualification are unchanged.

Four isolated comparisons matched all 132 module, original, and edited artifacts. The 16,016-function control edit
changed from 1.967 seconds / 25,022,464 bytes peak compiler RSS to 1.833 seconds / 25,067,520 bytes; source-free linkage
changed from 1.849 seconds / 21,250,048 bytes to 1.664 seconds / 21,094,400 bytes. The 15,001-function arithmetic edit
changed from 7.142 seconds / 51,511,296 bytes to 6.742 seconds / 51,683,328 bytes; source-free linkage changed from
5.771 seconds / 34,525,184 bytes to 5.180 seconds / 34,594,816 bytes.

The exported 81,918-function edit changed from 9.083 seconds / 83,140,608 bytes to 9.325 seconds / 83,595,264 bytes;
source-free linkage changed from 8.076 seconds / 70,017,024 bytes to 8.061 seconds / 70,238,208 bytes. The equally large
private-function edit changed from 8.025 seconds / 65,380,352 bytes to 7.797 seconds / 65,019,904 bytes; source-free
linkage changed from 7.518 seconds / 51,843,072 bytes to 7.286 seconds / 51,642,368 bytes. These mixed timing and RSS
samples include regressions and do not establish a universal speed or memory improvement.

Raw comparisons remain in buffered-hashes-capacity-2026-10-28.json; runtime and conformance reports retain the same
prefix. Public formats and resource guards are unchanged. These bounded compiler-process samples do not establish
aggregate cross-platform compliance, disk-backed interfaces, lazy runtime loading, or the much larger source-line
target.


## Selectively decoded validated bodies

Pending dependency analysis now avoids another full typed decode for scene, startup, and suspending bodies when they
contain no legacy calls. Real spool checksums, counts, expected headers, initial typed validation, legacy collection,
public formats, and resource guards are unchanged. All 462 native tests, 40 CLI tests, strict all-target Clippy, and
formatting checks passed. All 883 formatted cases passed in both modes; twenty original/restored executions checked
every routine, result, and suspending signature without replay. Runtime source and its prior 112-test qualification are
unchanged. An initial fixture attempted direct shared-signature mutation; its corrected copy-on-write fixture and
successful checks remain recorded.

Five workload comparisons matched 153 module, original, and edited artifacts. An optional --suspend-routines flag
supplies 15,000 suspending arithmetic routines plus the entry scene. All four earlier source generators retain identical
SHA-256 values.

The 16,016-function control edit changed from 1.975 seconds / 24,969,216 bytes peak compiler RSS to 1.949 seconds /
24,907,776 bytes. Source-free linkage changed from 1.573 seconds / 21,250,048 bytes to 1.602 seconds / 21,184,512 bytes.

The 15,001-function ordinary arithmetic edit changed from 6.582 seconds / 51,613,696 bytes peak compiler RSS to 6.745
seconds / 51,826,688 bytes. Source-free linkage changed from 5.560 seconds / 34,586,624 bytes to 6.132 seconds /
34,635,776 bytes.

The 15,001-function suspending arithmetic edit changed from 7.369 seconds / 50,130,944 bytes peak compiler RSS to 6.366
seconds / 50,200,576 bytes. Source-free linkage changed from 5.387 seconds / 34,664,448 bytes to 4.928 seconds /
34,562,048 bytes.

The 81,918-function exported graph edit changed from 8.873 seconds / 83,148,800 bytes peak compiler RSS to 9.472 seconds
/ 83,267,584 bytes. Source-free linkage changed from 8.800 seconds / 69,971,968 bytes to 9.984 seconds / 69,865,472
bytes.

The 81,918-function private graph edit changed from 8.447 seconds / 65,273,856 bytes peak compiler RSS to 8.355 seconds
/ 65,314,816 bytes. Source-free linkage changed from 7.415 seconds / 51,802,112 bytes to 7.486 seconds / 49,627,136
bytes.

The initial old-compiler exported-graph edit exhausted its time guard at 9.643 seconds / 83,419,136 bytes. Its previous
root and all thirteen original package files were independently preserved. One fresh paired run completed on both
compilers and matched all 39 artifacts; its candidate artifacts also matched the initial successful candidate. The
initial failure and preservation audit remain separate from the qualified aggregate. Source-free measurements are clean
links, whose time scales with total input.

The qualified aggregate is selected-bodies-capacity-2026-10-28.json. Initial, follow-up, output-audit, runtime,
generator, and both-mode conformance reports retain the same prefix. These are mixed timing/RSS samples, including
slower ordinary-code and exported-graph observations. They do not establish a universal improvement, aggregate
cross-platform compliance, disk-backed interfaces, lazy runtime loading, or the much larger source-line target.


## Grouped discarded initialization calls

Parentheses around a discarded single-use call or its plain-name callee now preserve compact forward initialization.
Parameters, generics, effectful bodies, consumed results, repeats, references, cell creation, and ordinary
type/assignment checks retain their existing boundaries. All 465 native tests, 40 CLI tests, strict all-target Clippy,
and formatting checks passed. Seven source-free/diagnostic additions bring the formatted corpus to 890 cases, all
passing in both modes. Runtime code and its prior 112-test qualification are unchanged. An initial diagnostic assertion
expected "captured val" where a missing cell reports "captured binding"; the failed assertion and corrected passing run
remain recorded.

The plain forward control retains identical original and edited bytes. Population changed from 0.870 seconds /
18,317,312 bytes peak compiler RSS to 0.745 seconds / 18,046,976 bytes; its edit changed from 0.831 seconds / 18,477,056
bytes to 0.820 seconds / 18,448,384 bytes. Unchanged reuse changed from 0.313 seconds / 4,182,016 bytes to 0.302 seconds
/ 4,251,648 bytes, including higher reuse RSS. These samples do not establish a universal improvement.

The 1,024-grouped-call workload measured 0.801 seconds / 18,358,272 bytes for population, 0.286 seconds / 4,145,152
bytes unchanged, and 0.828 seconds / 18,444,288 bytes for its edit. The prior compiler rejects this form. Its
2,053-function module published in 0.476 seconds / 18,198,528 bytes; source-free inline linkage measured 0.862 seconds /
10,960,896 bytes and package linkage 0.859 seconds / 12,730,368 bytes. Original and edited grouped program bytes also
match the corresponding plain control exactly.

Twelve original/restored executions checked all 1,024 distinct immutable cells, escaped reader values, compact stubs,
and no replay. Thirteen earlier workload source hashes remain unchanged. Raw capacity, output-audit, runtime, generator,
and both-mode conformance evidence retain the grouped-initializers prefix. Public formats and resource guards are
unchanged; these bounded compiler-process samples do not establish aggregate cross-platform compliance or the much
larger capacity target.


## Consumed compact Unit call results

Proven pure, parameterless, nongeneric helpers may initialize captured vals while their Unit result is consumed by an
expression, including before their source declaration. Statement output retains its existing bytes; value lowering
preserves surrounding operands and leaves one Unit value. Every original declaration still undergoes type checking. All
469 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Ten source-free/diagnostic
additions bring the formatted corpus to 900 cases, all passing in both modes. Runtime code and its prior 112-test
qualification are unchanged.

The grouped forward control keeps identical original and edited bytes. Population changed from 0.872 seconds /
18,448,384 bytes peak compiler RSS to 0.763 seconds / 18,354,176 bytes. Its edit changed from 0.827 seconds / 18,690,048
bytes to 0.803 seconds / 18,767,872 bytes, including higher edit RSS. Unchanged reuse changed from 0.285 seconds /
4,194,304 bytes to 0.278 seconds / 4,210,688 bytes. These mixed observations do not establish a universal improvement.

The new workload retains all 1,024 Unit results in a shared MutableList<Unit>, alongside 1,024 distinct immutable cells
and saved readers. Its 2,054-function program measured 0.930 seconds / 20,148,224 bytes for population, 0.308 seconds /
4,354,048 bytes unchanged, and 0.876 seconds / 20,484,096 bytes for its edit. The old compiler rejects the consumed
forward-call form. Module publication measured 0.497 seconds / 19,894,272 bytes; source-free inline linkage measured
0.788 seconds / 12,210,176 bytes and package linkage 0.791 seconds / 14,127,104 bytes. Edited inline bytes match the
source build.

Independent observation found two fixture errors: the expected count omitted the added shared-list initializer, and a
completion probe tried to use an expired call frame. Corrected observation uses the retained collection reference and
checks every Unit value before and after restoration and completion. Twelve executions passed, checking all cells,
readers, compact stubs, and no replay. Fourteen prior workload sources retain identical hashes. The initial logs and
measurement remain recorded separately.

One fresh isolated comparison uses the corrected 2,054-function metadata. All ten original, edited, code-scene, and
module artifacts match the initial observation exactly. The qualified unit-initializers-capacity-2026-10-28.json links
the initial measurement and metadata correction; follow-up byte audits, runtime, generator, and both-mode conformance
reports retain the same prefix. Public formats and resource guards are unchanged. These bounded compiler-process
observations do not establish aggregate cross-platform compliance or the much larger source-line target.


## Forward initializer frames

All 482 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Eighteen additions bring the
formatted corpus to 918 cases, all passing in inline and package modes. The initial returned-reader regression exposed
an overly conservative entry-read barrier: block-entry named closure creation retains references without reading them.
Actual function-value creation, publication, and invocation retain their checks. The initial failure is preserved
alongside its correction. Runtime code and its prior 112-test qualification are unchanged.

The unit-forward-vals control retains identical original and edited bytes. Its edit changed from 1.000 seconds /
20,439,040 bytes peak compiler RSS to 0.866 seconds / 20,619,264 bytes, including higher memory. The new frame-forward-
vals workload executes 1,024 two-parameter initializer calls, including a default parameter. Its 2,053-function program
measured 1.073 seconds / 22,454,272 bytes for population, 0.302 seconds / 4,120,576 bytes unchanged, and 1.114 seconds /
22,892,544 bytes for its edit. Module publication measured 0.583 seconds / 22,274,048 bytes; source-free inline linkage
measured 0.947 seconds / 11,337,728 bytes and package linkage 0.877 seconds / 13,721,600 bytes. Source-free inline bytes
match the edited source build.

Twelve independent executions checked every distinct immutable cell and retained reader across save/restore. The frame
workloads execute all 1,024 initializer calls before saving and none after restoring; the control also retains and
checks all 1,024 Unit values. Fifteen prior workload sources keep identical hashes. Raw resource, generator, runtime,
focused, and both-mode conformance reports retain the forward-frames prefix. Public formats and resource guards are
unchanged. These bounded compiler-process RSS observations do not establish aggregate cross-platform compliance or the
much larger source-line target.


## Lexical initializer correctness and mutable forward calls

Compact expansion now rejects caller-specific name resolution and type inference. Regression executions retain
declaration-time List, Set, and Map arguments through source-free linkage and checkpoints. An initial 928-case run
caught a valid named-reader regression caused by the first conservative fallback. Compact references now require
established lexical cells; that original case and the new negative scope cases pass. Focused execution then exposed
skipped default reads being treated as body reads. Their metadata remains separate while default control flow still
participates in mandatory-write analysis. Initial failures and corrected fixtures remain recorded.

All 500 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Twenty-seven additions bring
the formatted corpus to 945 cases, all passing in both modes. Twenty-nine focused cases additionally exercise the
repaired reader and lexical-body paths. Runtime code and its prior 112-test qualification are unchanged. An intermediate
lexical comparison preserves exact old/new program hashes but is not a successful full-conformance qualification.

The frame-forward-vals control retains identical original and edited bytes. Its edit changed from 1.075 seconds /
22,749,184 bytes peak compiler RSS to 1.101 seconds / 22,872,064 bytes, including higher time and memory. The new
mutable-forward-vals workload retains 1,024 distinct mutable cells and executes 1,024 typed/defaulted forward
initializer calls. Its 2,053-function program measured 1.043 seconds / 22,335,488 bytes for population, 0.304 seconds /
4,042,752 bytes unchanged, and 1.002 seconds / 22,384,640 bytes for its edit. The older compiler rejects the new form.
Module publication measured 0.710 seconds / 22,011,904 bytes; source-free inline linkage measured 0.793 seconds /
11,321,344 bytes and package linkage 0.855 seconds / 13,545,472 bytes. Source-free inline bytes match the edited source
build.

Twelve independent executions checked every cell, value, retained reader, and actual initializer-call count, including
four mutable programs and two immutable controls, each before and after restoration. No initializer replayed after
restoring. Sixteen earlier workload sources keep identical hashes. Raw resource, runtime, generator, focused, native,
and both-mode conformance evidence retains the mutable-forward prefix; intermediate lexical evidence retains lexical-
initializers. Public formats and resource guards are unchanged. These bounded compiler-process RSS measurements do not
establish aggregate cross-platform compliance or the much larger source-line target.


## Repeated mutable initializer calls and overloads

All 507 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Fourteen additions bring the
formatted corpus to 959 cases, all passing in inline and package modes. Initial native regressions show the earlier
repeated-call and alternative-branch failures; their corrected runs retain the same prefix. Runtime code and its prior
112-test qualification are unchanged.

The mutable-forward-vals control retains identical original and edited bytes. Its edit changed from 1.124 seconds /
22,736,896 bytes peak compiler RSS to 1.036 seconds / 22,380,544 bytes. Population changed from 1.348 seconds /
22,261,760 bytes to 1.190 seconds / 21,995,520 bytes. Unchanged reuse changed from 0.317 seconds / 3,960,832 bytes to
0.315 seconds / 4,149,248 bytes, including higher RSS. These observations are mixed rather than a universal improvement.

The repeated-forward-vals workload has 3,072 source call sites. Its branches select 2,048 actual helper calls over 1,024
distinct mutable cells, and every helper returns a String after writing its target. The 2,053-function program measured
1.402 seconds / 30,900,224 bytes for population, 0.345 seconds / 3,919,872 bytes unchanged, and 1.454 seconds /
31,182,848 bytes for its edit. The prior compiler rejects this form. Module publication measured 1.444 seconds /
30,666,752 bytes; source-free inline linkage measured 1.116 seconds / 15,802,368 bytes and package linkage 1.208 seconds
/ 18,931,712 bytes. Edited inline bytes match the source build.

Twelve independent executions check every cell, value, reader, helper result type, and actual call count across
restoration. Each repeated program performs 2,048 initializer calls before saving and none after restoring. Seventeen
earlier workload sources retain identical hashes. Resource, generator, runtime, focused, native, and both-mode
conformance evidence uses the repeated-initializers prefix. Public formats and resource guards are unchanged. These
bounded compiler-process observations do not establish aggregate cross-platform compliance or the much larger source-
line target.


## Loop-contained mutable forward calls

All 516 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Twelve additions bring the
formatted corpus to 971 cases, all passing in both output modes. Initial native regressions show the unsupported for-
body, while-condition, and post-test-argument paths; the corrected run also checks fresh iteration vals and rejects
repeated outer-val stores. Runtime code and its prior 112-test qualification are unchanged.

The repeated-forward-vals control retains identical original and edited bytes. Population changed from 1.382 seconds /
30,523,392 bytes peak compiler RSS to 1.698 seconds / 30,797,824 bytes. Unchanged reuse changed from 0.368 seconds /
3,985,408 bytes to 0.405 seconds / 4,128,768 bytes. Its edit changed from 1.582 seconds / 31,236,096 bytes to 1.361
seconds / 31,256,576 bytes. The measurements include regressions and do not establish a universal improvement.

The loop-forward-vals workload executes one real forward helper call inside each of 1,024 range loops, then breaks with
its distinct mutable cell initialized. Each helper returns a String after writing. The 2,053-function program measured
1.194 seconds / 26,771,456 bytes for population, 0.370 seconds / 4,190,208 bytes unchanged, and 1.284 seconds /
26,996,736 bytes for its edit. The prior compiler rejects this form. Module publication measured 0.790 seconds /
26,734,592 bytes; source-free inline linkage measured 0.997 seconds / 13,840,384 bytes and package linkage 1.055 seconds
/ 16,764,928 bytes. Edited inline bytes match the source build.

Twelve independent executions check every mutable cell, value, retained reader, helper result type, and actual call
count across restoration. The loop programs execute 1,024 initializer calls and the controls 2,048 before saving;
neither replays them after restoring. Eighteen earlier workload sources retain identical hashes. Resource, generator,
runtime, focused, native, and both-mode conformance evidence uses the loop-initializers prefix. Public formats and
resource guards are unchanged. These bounded compiler-process observations do not establish aggregate cross-platform
compliance or the much larger source-line target.


## Nonempty traversal exits

All 525 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Fourteen additions bring the
formatted corpus to 985 cases, all passing in both modes. Initial regressions show lost body/continue writes and a
returned iteration capture. An initial focused run exposed the separate IntProgression type for stepped traversal; final
checks prove only known nonempty forms and keep unknown inputs conservative. The right-operand replacement regression
prevents using later binding state for an earlier captured progression. Runtime code and its prior 112-test
qualification are unchanged.

The loop-forward-vals control retains identical original and edited bytes. Population changed from 1.666 seconds /
26,886,144 bytes peak compiler RSS to 1.245 seconds / 26,873,856 bytes. Unchanged reuse changed from 0.375 seconds /
4,284,416 bytes to 0.378 seconds / 4,235,264 bytes. Its edit changed from 1.449 seconds / 27,123,712 bytes to 1.342
seconds / 27,172,864 bytes, including higher RSS. These observations do not establish a universal improvement.

The range-forward-vals workload completes 1,024 nonempty loops normally, retaining each distinct mutable cell after its
real initializer call. The 2,053-function program measured 1.144 seconds / 26,255,360 bytes for population, 0.345
seconds / 4,050,944 bytes unchanged, and 1.198 seconds / 26,517,504 bytes for its edit. The prior compiler rejects this
form. Module publication measured 0.668 seconds / 26,185,728 bytes; source-free inline linkage measured 0.933 seconds /
13,660,160 bytes and package linkage 1.113 seconds / 16,285,696 bytes. Edited inline bytes match the source build.

Twelve independent executions checked every cell, value, retained reader, helper result type, and actual call count
across restoration. All programs execute 1,024 initializer calls before saving and none after restoring. Nineteen
earlier workload sources retain identical hashes. Resource, generator, runtime, focused, native, and both-mode
conformance evidence uses the range-initializers prefix. Public formats and resource guards are unchanged. These bounded
compiler-process observations do not establish aggregate cross-platform compliance or the much larger source-line
target.


## Immutable stored progressions

All 530 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Ten additions bring the
formatted corpus to 995 cases, all passing in both modes. The initial native failures establish missing stored-alias and
captured-value evidence. Runtime code and its prior 112-test qualification are unchanged.

The range-forward-vals control retains identical original and edited bytes. Population changed from 1.696 seconds /
26,148,864 bytes peak compiler RSS to 1.318 seconds / 26,144,768 bytes. Unchanged reuse changed from 0.405 seconds /
4,198,400 bytes to 0.438 seconds / 4,263,936 bytes. Its edit changed from 1.357 seconds / 26,492,928 bytes to 1.583
seconds / 26,492,928 bytes. These mixed observations do not establish a universal improvement.

The stored-range-vals workload constructs and retains 1,024 immutable nonempty progressions, then traverses each before
retaining its distinct initialized mutable cell. Its 2,053 functions measured 1.394 seconds / 27,127,808 bytes for
population, 0.457 seconds / 4,141,056 bytes unchanged, and 1.449 seconds / 27,414,528 bytes for the edit. The prior
compiler rejects this form. Module publication measured 0.872 seconds / 27,004,928 bytes; source-free inline linkage
measured 1.202 seconds / 14,540,800 bytes and package linkage 1.137 seconds / 17,084,416 bytes. Edited inline bytes
match the source build.

Twelve independent executions checked every cell, value, retained reader, helper result type, and call count across
restoration. All programs execute 1,024 initializer calls before saving and none after restoring. Twenty earlier
workload sources retain identical hashes. Resource, generator, runtime, focused, native, and both-mode conformance
evidence uses the stored-progressions prefix. Public formats and resource guards are unchanged. These bounded
compiler-process observations do not establish aggregate cross-platform compliance or the much larger source-line
target.


## Nonempty text and collection values

All 538 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Twenty additions bring the
formatted corpus to 1,015 cases, all passing in both modes. Initial native regressions establish missing collection,
text, copy, and escaping-reader proofs. Mutable aliases, contextual mutable construction, copies into mutable families,
erased mutable values, and empty or unknown inputs retain diagnostic coverage. Runtime code and its prior 112-test
qualification are unchanged.

The stored-range-vals control retains identical original and edited bytes. Population changed from 1.468 seconds /
27,197,440 bytes peak compiler RSS to 1.258 seconds / 27,312,128 bytes. Unchanged reuse changed from 0.506 seconds /
3,784,704 bytes to 0.347 seconds / 4,165,632 bytes. Its edit changed from 1.432 seconds / 27,496,448 bytes to 1.200
seconds / 27,488,256 bytes. These mixed memory observations do not establish a universal improvement.

The immutable-list-vals workload stores 1,024 immutable singleton lists and traverses each before retaining its distinct
initialized mutable cell. Its 2,053 functions measured 1.176 seconds / 27,074,560 bytes for population, 0.343 seconds /
4,222,976 bytes unchanged, and 1.278 seconds / 27,488,256 bytes for the edit. The prior compiler rejects this form.
Module publication measured 0.803 seconds / 27,029,504 bytes; source-free inline linkage measured 1.010 seconds /
14,090,240 bytes and package linkage 1.010 seconds / 17,166,336 bytes. Edited inline bytes match the source build.

Twelve independent executions checked every cell, value, retained reader, helper result type, and actual call count
across restoration. All programs execute 1,024 initializer calls before saving and none after restoring. Twenty-one
earlier workload sources retain identical hashes. Resource, generator, runtime, focused, native, and both-mode
conformance evidence uses the nonempty-values prefix. Public formats and resource guards are unchanged. These bounded
compiler-process observations do not establish aggregate cross-platform compliance or the much larger source-line
target.


## Exclusive captured-val call sites

All 547 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Twenty additions bring the
formatted corpus to 1,035 cases. The first full run passed 1,034 cases and exposed an obsolete negative expectation: a
selected initializing overload was rejected merely because an unused overload shared its name. That source is now a
checkpoint/output regression; its earlier expectation and failed run remain in private evidence. The corrected corpus
passes in both modes, and all 21 focused cases pass in both modes. Runtime code and its prior 112-test qualification are
unchanged.

The frame-forward-vals control retains identical original and edited bytes. Population changed from 0.952 seconds /
22,540,288 bytes peak compiler RSS to 1.219 seconds / 22,626,304 bytes. Unchanged reuse changed from 0.303 seconds /
4,210,688 bytes to 0.433 seconds / 4,259,840 bytes. Its edit changed from 1.137 seconds / 23,060,480 bytes to 1.331
seconds / 22,945,792 bytes. These mixed observations do not establish a universal improvement.

The exclusive-forward-vals workload has 2,048 source call sites, selecting one real initializing call for each of 1,024
readonly cells. Its 2,053 functions measured 2.058 seconds / 28,487,680 bytes for population, 0.445 seconds / 3,846,144
bytes unchanged, and 1.904 seconds / 28,798,976 bytes for the edit. The prior compiler rejects this form. Module
publication measured 0.840 seconds / 28,196,864 bytes; source-free inline linkage measured 1.068 seconds / 14,426,112
bytes and package linkage 1.452 seconds / 17,145,856 bytes. Edited inline bytes match the source build.

Twelve independent executions checked every cell, value, readonly status, retained reader, and actual call count across
restoration. All programs execute 1,024 initializer calls before saving and none after restoring. Twenty-two earlier
workload sources retain identical hashes. Resource, generator, runtime, focused, native, expectation-migration, and
both-mode conformance evidence uses the exclusive-initializers prefix. Public formats and resource guards are unchanged.
These bounded compiler-process observations do not establish aggregate cross-platform compliance or the much larger
source-line target.


## Initializers on terminal loop paths

All 554 native tests, 40 CLI tests, strict all-target Clippy, and formatting checks passed. Sixteen additions bring the
formatted corpus to 1,051 cases, all passing in both modes. The observer verifies exact case IDs and frozen compiler,
case-source, runner, engine, and runtime fingerprints. All 19 focused cases pass in both modes. Initial native failures
establish missing terminal-loop proofs; follow-up checks preserve deferred-call rejection and diagnose backedge writes
at the loop. Three existing negative inputs retain their sources and now expect that more precise diagnostic. Runtime
code and its prior 112-test qualification are unchanged.

The exclusive-forward-vals control retains identical original and edited bytes. Population changed from 1.220 seconds /
28,803,072 bytes peak compiler RSS to 1.139 seconds / 28,803,072 bytes. Unchanged reuse changed from 0.349 seconds /
4,059,136 bytes to 0.352 seconds / 4,124,672 bytes. Its edit changed from 1.257 seconds / 29,106,176 bytes to 1.226
seconds / 29,143,040 bytes. These mixed observations do not establish a universal improvement.

The loop-exit-vals workload places one initializing call inside each of 1,024 two-value traversals, then exits that loop
before the readonly cell can be written again. Its 2,053 functions measured 1.188 seconds / 26,701,824 bytes for
population, 0.346 seconds / 4,202,496 bytes unchanged, and 1.450 seconds / 26,611,712 bytes for the edit. The prior
compiler rejects this form. Module publication measured 0.762 seconds / 26,472,448 bytes; source-free inline linkage
measured 1.158 seconds / 14,315,520 bytes and package linkage 1.173 seconds / 17,342,464 bytes. Edited inline bytes
match the source build.

Twelve independent executions checked every cell, value, readonly status, retained reader, and actual call count across
restoration. All programs execute 1,024 initializer calls before saving and none after restoring. Twenty-three earlier
workload sources retain identical hashes. Resource, generator, runtime, focused, native, diagnostic-migration, and
both-mode conformance evidence uses the loop-exit-initializers prefix. Public formats and resource guards are unchanged.
These bounded compiler-process observations do not establish aggregate cross-platform compliance or the much larger
source-line target.


## Historical larger-project qualification (2026-10-29)

Compiler versions with 587 and 594 native/CLI checks were compared on five original workload families. The final
comparison verifies 153 exact generated artifacts, including roots, code scenes, and independently compiled modules.
Every measured incremental operation in the selected current-compiler observations finishes below ten seconds. The
largest workload contains 81,918 functions; its edit takes 9.382 seconds, close to the internal 9.5-second guard.

The table's maximum RSS includes all observed current-compiler operations for that workload, including population and
module publication. Source-free timings use clean linkage. Clean compilation scales with input; the larger exported
workload's population takes 10.257 seconds.

| Workload | Functions | Edit seconds | Maximum RSS bytes | Source-free seconds |
| --- | ---: | ---: | ---: | ---: |
| Control | 16,016 | 1.968 | 24,977,408 | 1.587 |
| Ordinary code | 15,001 | 7.002 | 57,905,152 | 5.282 |
| Suspending code | 15,001 | 6.336 | 55,943,168 | 5.145 |
| Exported routines | 81,918 | 9.382 | 86,372,352 | 7.932 |
| Private routines | 81,918 | 8.861 | 66,330,624 | 7.342 |

The initial older-compiler exported workload exhausted its guard during the edit at 9.626 seconds. Every previously
committed output remained byte-identical. One valid immutable code chunk remained reusable under the documented
concurrent-publication policy; the old root did not reference it. The failure and its audit remain in private evidence.
A fresh paired observation completes the older edit in 9.496 seconds and the current edit in 9.382 seconds. These
observations show limited timing margin and do not establish a universal improvement. The initial small control timing
overlapped source validation; the table uses its fresh paired observation. Phase timing output is enabled for both
compilers in that follow-up.

Twenty independent original/restored executions check every routine reference, invocation, result, retained container,
shared total, choice label, and final transcript. Restoring does not replay completed routines. The graph observer's
initial control configuration incorrectly requested an Int total, although the generator declares Float at every size.
The corrected configuration passes all twelve graph executions; the initial diagnostic is retained. The ordinary and
suspending code observers pass eight executions.

Private current-capacity reports retain raw measurements, compiler and observer identities, the selected comparison pair
for each workload, all 153 artifact hashes, runtime assertions, the failed deadline audit, and observer-configuration
correction. Peak compiler-process RSS across these observations is 86,372,352 bytes. The aggregate memory protocol,
other platforms, and the much larger source-line target remain unverified. Public formats and resource guards are
unchanged.


## Historical comparison conformance (2026-10-29)

The formatted language corpus now passes all 1,064 cases in both output modes, with frozen compiler and execution-source
fingerprints. Thirteen dedicated additions exercise typed structural exact comparison, cyclic and nullable values,
independent generic parameters, empty covariant views, shared pattern-binding conversions, and static equality
eligibility. Four pending ledger entries now have targeted partial evidence; none is marked fully conformant. Native
compiler/runtime code is unchanged for this acceptance batch, retaining 554 native tests, 40 CLI tests, strict
all-target Clippy, and formatting qualification. Runtime code remains unchanged from its earlier 112-test qualification.
