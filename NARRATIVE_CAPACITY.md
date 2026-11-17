# Large narrative capacity measurements

These are bounded synthetic narrative experiments, not complete language-capacity or aggregate-memory acceptance. The
selected requirements remain below 250,000,000 bytes for the compiler and helpers and below ten seconds for incremental
end-to-end builds. Clean builds scale with input. [Resource history](RESOURCES.md) retains other workload families and
earlier observations; the [language checklist](../bifoil-language/language-implementation-checklist.md) keeps their
remaining acceptance obligations separate.

## Workload and actual compiled dimensions

The generator creates one shared counter, one routing scene, and separately declared room scenes. Each room increments
the counter, renders its begin/end markers and every generated prose row, then waits at a return choice. All rooms are
reachable. Routing uses groups of at most 128 options above 128 rooms; it does not relax the compiler's 256-option
limit. A playthrough selects one room rather than accumulating the entire story's output.

| Dimension | Ten-million-row input | Hundred-million-row input |
| --- | ---: | ---: |
| Prose rows | 10,000,000 | 100,000,000 |
| Total physical source lines | 10,001,206 | 100,012,022 |
| UTF-8 source bytes | 100,026,219 | 1,000,277,511 |
| Modules | 102 | 1,002 |
| Import edges | 201 | 2,001 |
| Scene declarations | 101 | 1,001 |
| Function headers in the actual package | 103 | 1,003 |
| Instructions in the actual package | 3,819 | 38,099 |
| Nominal type declarations / generic parameters | 0 / 0 | 0 / 0 |
| Shared declarations | 1 | 1 |
| Maximum function parameter count | 0 | 0 |

The largest source is 1,000,167 bytes, below the unchanged 1 MiB per-source guard. The graph is acyclic, with maximum
dependency depth three and a wide routing fan-out. Room imports converge on the shared-counter module. The repeated rows
contain x; this stresses reachable text volume, discovery, cached decoding, packaging, and suspended execution. It does
not represent heavy generics, large cyclic interfaces, broad type graphs, extensive diagnostics, or equivalent volumes
of executable statements. Successful inputs expect zero compiler diagnostics. Flat-choice and deadline failures remain
separate failed controls.

The verified hundred-million-row package contains 1,002 code scenes totaling 203,437,429 bytes; its largest scene is
887,540 bytes. Its root is 584,345 bytes. These counts come from actual package metadata and artifact sizes, not a
source-line multiplier. Package v2 keeps instruction bodies on demand; declarations remain resident. Per-String,
active-runtime allocation, collector, call-depth, and checkpoint limits need their own [runtime
checks](../bifoil-runtime/CAPACITY_TESTS.md).

## Reference conditions and retained observations

The reference machine is an AMD Ryzen 7 6800H, eight physical cores and sixteen logical CPUs, running x86-64 WSL2 Linux
6.18.54.1-microsoft-standard-WSL2. Mounted-Windows trials use 9p with a 65,536-byte message size. Linux-filesystem
trials use ext4. The compiler executable remains on the Windows mount. No competing compiler or runtime test suite runs
during measured compiler processes. The completed trial records its overlapping documentation inventory separately
below. Ordinary checking/lowering is sequential; input prefetch uses at most four threads and completed-build
verification uses at most sixteen. Earlier trials explicitly retain their four-thread verification condition.

A fresh population starts with absent project cache and outputs. The repeated sixteen-thread Windows trial retained
previous output chunks but had an empty disposable cache, so its population is listed separately. OS filesystem caches
are not flushed. Incremental trials reuse previously populated files. Independent module publication runs as separate
measured native processes. Artifact-only linking retains the inactive source cache under another name before its first
link, making its new project cache genuinely cold. Earlier reports that reused an existing project cache retain that
qualification.

| Input and filesystem | Phase / condition | Wall seconds | Compiler peak RSS bytes | Result |
| --- | --- | ---: | ---: | --- |
| 10M, Windows mount, four threads | Fresh population | 21.173 | 43,229,184 | Compiled |
| 10M, Windows mount, four threads | Same-size/same-mtime leaf edit | 8.177 | 40,407,040 | Compiled |
| 100M, Windows mount, four threads | Fresh population | 214.817 | 43,294,720 | Compiled |
| 100M, Windows mount, four threads | First unchanged build | 9.532 | 5,316,608 | Deadline failure |
| 100M, Windows mount, sixteen threads | Cold cache, previous output chunks | 162.728 | 43,327,488 | Compiled |
| 100M, Windows mount, sixteen threads | First unchanged build | 9.541 | 6,082,560 | Deadline failure |
| 100M, ext4, sixteen threads | Fresh population | 37.439 | 43,569,152 | Compiled |
| 100M, ext4, sixteen threads | Same-size/same-mtime leaf edit | 9.557 | 43,769,856 | Deadline failure |
| 100M, ext4, checked raw function parts | Fresh population | 36.237 | 43,229,184 | Compiled |
| 100M, ext4, checked raw function parts | Same-size/same-mtime leaf edit | 9.564 | 42,565,632 | Deadline failure |
| 100M, ext4, bounded small-record decoder | Fresh population | 36.410 | 43,409,408 | Compiled |
| 100M, ext4, bounded small-record decoder | Same-size/same-mtime leaf edit | 7.552 | 42,610,688 | Compiled |
| 100M, ext4, bounded small-record decoder | Cold source-free link | 5.777 | 39,477,248 | Compiled |

The ext4 unchanged builds took 0.332, 0.316, and 0.317 seconds in the first trial, and 0.365, 0.327, and 0.348 seconds
with checked raw function parts. These results isolate an important filesystem cost without qualifying Windows-mount
incremental acceptance. The first hundred-million-row generator used one oversized choice and failed at the existing
option limit; its failed output/report and exact input identities are retained. Grouped routing repairs the workload,
not that language guard.

Fresh population throughput was about 4.72 MB/s for the 10M Windows input, 4.66 MB/s for the first valid 100M Windows
input, and 26.72 MB/s for the first ext4 population. These are observations for their stated conditions. They neither
impose ten seconds on clean builds nor establish worst-case throughput across all source families. The subsequent
bounded-decoder trial below retains those failed observations unchanged.

The bounded-decoder trial completed every phase: all 1,002 modules were independently published, their owned source
files were removed only after checked publication, and the cold artifact-only link reproduced the edited source root.
All original, edited, and source-free first/middle/last routes passed fresh-program checkpoint checks without replay.
Source-free unchanged builds took 0.224, 0.226, and 0.225 seconds. The highest observed compiler RSS across compilation
and publication was 43,409,408 bytes. The original root remained byte-identical to the earlier unbuffered producer.

This establishes the measured narrative probe on ext4. It does not qualify the complete language envelope, the failed
Windows-mount cases, or the unresolved aggregate-memory quantity. A documentation/ledger integrity inventory overlapped
part of module publication; no competing compiler or runtime test suite ran. The raw observation retains that activity
rather than silently calling every job fully isolated.

The strengthened [large-package verifier](../bifoil-runtime/tools/verify-narrative-capacity.mjs) checks code residency
on Linux and Windows Node. Across eighteen first/middle/last route executions in original, edited, and source-free
builds, every fresh package began with zero fetched bodies. At the checked route endpoints, at most 2 modules and
1,089,969 encoded scene bytes were resident, with at most 2 total module loads per fresh reader. This is encoded-code
accounting, not a measurement of the whole JavaScript heap. The Windows checks retain the same 1,002-module manifests
but provide only the selected exact companion scenes, so eager fetching of unvisited bodies would fail. These are
Node-runtime checks, separate from compiler-host and production-browser acceptance.

## Measurement, validation, and implementation boundary

[The native observer](tools/process_measurement.py) starts a monotonic wall clock before GNU Time launches the compiler
and stops after that process exits. It includes native startup, discovery, input validation, required compilation and
linking, artifact writes, and compiler-owned cleanup. Temporary code/index stores are released before successful root
replacement. Workload generation, compiler construction, benchmark-controller bookkeeping, optional runtime checks, and
retaining recovery archives are outside the measured compiler job. An independently compiled module is measured when it
is prepared; its previous existence is stated for later linking trials.

The current native compiler starts no helper processes. Its threads share process memory. GNU Time records the kernel's
process peak resident-set accounting; this avoids relying on periodic sampling. A representative untimed process trace
confirms one executable, thread creation with shared address space, no forked helpers, and a Linux address-space limit
of 240,000,000 bytes. This does not settle the pending choice between process memory and broader job accounting that
includes unmapped OS filesystem caches. If helpers are introduced, the largest individual child peak must not be
substituted for a simultaneous aggregate. [Linux resource
accounting](https://man7.org/linux/man-pages/man2/getrusage.2.html) and [child
accounting](https://man7.org/linux/man-pages/man2/wait4.2.html) describe that distinction.

The address-space guard covers native allocation and mappings and preserves a stricter inherited limit. It differs from
resident-memory measurement and from cgroup cache accounting. Warmed work still stops at the existing 9.5-second
internal deadline. Rejected or killed jobs remain failed builds. Cross-platform compiler enforcement and full strict
aggregate acceptance remain unfinished. [Linux limit semantics](https://man7.org/linux/man-pages/man2/getrlimit.2.html)
explain the address-space mechanism; a configured guard alone is not a successful capacity demonstration.

The current development admission guards are 2 GiB of aggregate source/artifact inputs and 1,024 modules. Per-source,
per-artifact, declaration, instruction, nesting, and hashing limits remain separately bounded. New guards admit the
measured narrative shape; they do not promise that every admitted combination completes within the incremental bound.
Older runtime consumers with smaller inventory guards can reject these larger packages before fetching code even when
the format/profile strings match. Current runtime tests pair the 1,024-module boundary with rejection of its successor.

Checked raw function parts retain canonical signature/code JSON and independently verify counts, header identities, and
spool checksums. Small cache and public-module records use the same bounded one-MiB slice-decoding policy; larger
records continue streaming. This bounds the extra active byte buffer instead of accumulating all decoded inputs.
Integrity failures and deadlines must retain their original fatal or disposable-cache behavior as appropriate.

The harness anchors whole-second source timestamps before population, then changes one leaf without changing its size or
timestamp. A successful edited build must change the root hash. Untouched roots remain byte-identical. The runtime
verifier checks first, middle, and last routes, exact paragraph hashes, full option snapshots, shared-counter results,
fresh-program checkpoint restoration, and absence of replay. Grouped routes cross three restored waits. Publication
requires a checked module artifact before removing its generator-owned source, and source-free output must match the
corresponding edited source output. A failed earlier phase cannot be relabeled a successful complete experiment.

## Reproducing and retaining evidence

Use a Linux native release compiler, Python through uv, and GNU Time. Choose an empty project directory and enough
physical disk reserve for sources, caches, modules, output scenes, and active compiler spools. The generator writes at
most one room source at a time and publishes exact file hashes/counts; describing a 100M-row workload does not allocate
100M rows. The benchmark refuses changed/aliased source inputs and existing report names.

```sh
cargo build --release --locked
uv run --no-project tools/benchmark-capacity.py \
    target/release/bifoilc /tmp/bifoil-capacity-100m \
    --scenes 1000 --rows 100000 --publish \
    --runtime /path/to/bifoil-runtime --node /path/to/node
```

The explicit runtime path is optional verification tooling, not a hidden build dependency. The runtime and compiler
remain independently usable projects. [Generator/tool tests](tools/test_capacity_workload.py) exercise input identity,
reachable grouped destinations, disk reserve, report preservation, timestamp controls, module publication, and
source-free identity.

Machine-specific raw reports, failures, tool snapshots, source identities, and recovery maps stay in ignored development
storage. Large generated failed-run files are removed only after per-file assessment, ownership/process checks, and
verified compressed recovery; reports/manifests retain their original observation identities. Cleanup is outside timed
jobs and does not turn a failed job into passing resource evidence.
