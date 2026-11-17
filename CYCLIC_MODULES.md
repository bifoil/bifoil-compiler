# Cyclic module storage

The loader retains discovery paths and hashes, then reads and checks the source identity for each declaration,
interface, and lowering pass. Each pass consumes its active module's AST. Sibling ASTs no longer remain resident across
interface convergence, and the interface helpers do not clone whole borrowed ASTs.

The convergence bound still counts the same declarations. Component fingerprints and final dependency stamping visit
actual dependency edges using a member-name set. Fingerprints retain the canonical content of every member and external
dependency while excluding internal dependency digests. Public module, program, package, and checkpoint formats are
unchanged.

Each checked cyclic member's bodies move to private storage before its successor is lowered. Cached component
fingerprints use those encoded functions directly. Summary analysis reads one body at a time, followed by metadata-only
dependency resolution. Cache publication uses the same canonical stored view and the atomic writer's whole-file hash;
it preserves the previous record on failure. Publication rejects a module that still carries resident instructions,
preventing stale stored bodies from concealing changed code.

The component's interfaces and declaration graphs remain resident. Member modules share one immutable group identity.
Validated cache and artifact inputs reuse an identity only when both its fingerprint and full membership agree.
Conflicting identities remain available to normal rejection; weak index entries do not keep discarded member vectors
alive. The actual component member set is compared once before checking shared identities and internal edge digests.
Serialized artifacts retain their existing fields and still repeat membership. Source reparsing adds work; interface
paging, selective changed-build linking, and target-scale capacity remain separate work.

## Reproducible comparison

The shared [workload generator](tools/cyclic_workload.py) emits a ring of explicit-signature functions. The existing
[cyclic observer](tools/benchmark-cyclic.py) supports module/function counts, both output modes, and retained original
roots. Its default two-module source bytes are preserved. The sequential [comparison tool](tools/compare-cyclic.py)
records clean, population, unchanged, and member-edit results, verifies original and edited outputs byte for byte,
and retains failed observations. Its control tests reject failed children and interrupted observers.

```sh
uv run --no-project tools/compare-cyclic.py --before /path/to/previous/bifoilc \
  --after target/release/bifoilc --output .bifoil/development/cyclic-comparison
```

The larger 8,192-function body workload uses packages because it exceeds the existing single-file inline output guard.
Both modes cover the 4,096-function body workload, the control, and the sparse 64-module ring. No admission guard is
raised for these measurements.

## Source-AST ownership comparison (2026-11-04)

This first comparison predates streamed cyclic bodies and measures the source-AST ownership change alone.

The host is a Ryzen 7 6800H with eight cores and sixteen logical CPUs, running x86-64 Linux under WSL2. Sources and
outputs are on the Windows-mounted v9fs filesystem. Compiler jobs run sequentially without other goal-owned builds or
tests. OS caches are not flushed; clean compilation bypasses the project cache rather than simulating a cold machine.
GNU Time observes compiler-process peak RSS. Wall time includes observer startup, compiler execution, publication,
and process cleanup. It does not measure strict aggregate compiler-plus-observer memory.

| Profile and mode | Modules | Functions | Peak RSS before, bytes | Peak RSS after, bytes | Edit before, seconds | Edit after, seconds |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| control-inline | 2 | 1,024 | 14,127,104 | 13,393,920 | 0.701 | 0.925 |
| control-package | 2 | 1,024 | 14,127,104 | 13,209,600 | 0.760 | 1.048 |
| bodies-inline | 8 | 4,096 | 29,814,784 | 29,593,600 | 3.172 | 3.719 |
| bodies-package | 8 | 4,096 | 29,749,248 | 29,356,032 | 3.325 | 3.926 |
| package-bodies-package | 8 | 8,192 | 53,227,520 | 52,604,928 | 6.163 | 7.475 |
| sparse-inline | 64 | 1,024 | 14,135,296 | 12,066,816 | 4.638 | 6.160 |
| sparse-package | 64 | 1,024 | 14,168,064 | 11,698,176 | 4.557 | 6.447 |

All seven comparisons pass, including 100 exact artifact pairs. Twenty-eight independent executions of
original and edited roots produce the expected transcripts. Peak values include every measured phase. These samples
show lower observed peak RSS and longer edits after the ownership change; they do not establish a general throughput
improvement. Every measured incremental operation remains below ten seconds for these bounded workloads.

The largest body profile has 8,202 source lines and
631,097 source bytes. This is far below the intended tens-to-hundreds-of-millions
of source lines. These compiler-process observations do not close the target-scale, full-job memory, or cross-platform
resource requirements.

## Stored-body comparison (2026-11-04)

This second comparison starts from the source-AST revision above and adds streamed cyclic bodies, read-only summary
analysis, canonical stored-module views, and single-pass cache publication. It uses the same generator, workload
shapes, output modes, host, and measurement protocol in a fresh sequential observation.

| Profile and mode | Peak RSS before, bytes | Peak RSS after, bytes | Edit before, seconds | Edit after, seconds |
| --- | ---: | ---: | ---: | ---: |
| control-inline | 13,819,904 | 11,128,832 | 0.848 | 0.998 |
| control-package | 13,811,712 | 11,194,368 | 0.903 | 1.238 |
| bodies-inline | 29,749,248 | 15,028,224 | 3.536 | 4.028 |
| bodies-package | 29,790,208 | 14,942,208 | 3.824 | 4.055 |
| package-bodies-package | 52,940,800 | 24,150,016 | 6.723 | 7.423 |
| sparse-inline | 12,828,672 | 7,729,152 | 6.013 | 6.101 |
| sparse-package | 12,566,528 | 7,602,176 | 5.999 | 6.297 |

All seven pairs pass with 100 identical artifact pairs and 28 successful original/edited executions. The
8,192-function package profile drops from 52,940,800 to 24,150,016 bytes observed peak RSS; its edit changes from
6.723 to 7.423 seconds. Samples retain a time/memory tradeoff, and every measured incremental operation remains below
ten seconds. These are bounded compiler-process observations, with target-scale and aggregate accounting still open.

Eight additional native controls cover canonical views, rejected resident instructions, read-only inspection,
failed-body ownership, cache reloads, preservation of previous records, failed fingerprint rollback, and cross-member
shared dependencies with bodies still spooled. Production code no longer exposes whole-module body restoration;
that operation remains available only to compatibility/fault tests.

## Cached and independently compiled component checks

The same 8,192-function package graph also runs through two isolated paths. A copied project keeps valid per-module
cache records but has a different completed-build context, forcing validation and linking without the completed-root
shortcut. Both compilers report eight reused modules and no recompilation.

Each compiler then independently publishes all eight modules through the native module command. Only after every member
is published are its source files removed. Source-free builds load the public artifacts, verify the whole component,
and reproduce the edited source root exactly. All eight module artifact pairs and both final roots match byte for byte.

| Operation | Peak RSS before, bytes | Peak RSS after, bytes | Time before, seconds | Time after, seconds |
| --- | ---: | ---: | ---: | ---: |
| cached_component_relink | 52,936,704 | 14,004,224 | 4.168 | 3.921 |
| source_free_component_link | 53,329,920 | 14,237,696 | 3.789 | 3.766 |

Every forced relink and source-free link remains below ten seconds in these observations. Independent module
publication ranges from 2.249 to 2.463 seconds before the change and 3.195 to 3.449 seconds afterward; each publication
recompiles the source component. Its observed peak stays below 24 MiB afterward. These clean publication costs are
retained separately from incremental timing. All raw command results, failures, compiler/observer identities, and
artifact hashes remain in the local qualification evidence.

## Shared component identities (2026-11-04)

This comparison starts from the compiler with physical operation locations and changes cyclic identity ownership.
Fresh components share one immutable member list and fingerprint. Checked cache/artifact inputs reuse that storage only
when complete identities match. Weak group handles release discarded vectors; conflicting lists retain their own data
for rejection. Module, program, package, and checkpoint formats remain unchanged.

The observer adds a metadata workload with 256 modules, 257 functions, 515 source lines, and 160 padding characters per
module name. It uses the same ring generator and retains normal output/source checks. Defaults still emit exactly the
previous source bytes. These small sources stress repeated membership metadata, rather than approximating the full
source-capacity target.

| Profile and mode | Peak RSS before, bytes | Peak RSS after, bytes | Edit before, seconds | Edit after, seconds |
| --- | ---: | ---: | ---: | ---: |
| control-inline | 11,898,880 | 11,444,224 | 0.906 | 0.975 |
| control-package | 11,898,880 | 11,436,032 | 0.957 | 1.223 |
| bodies-inline | 15,437,824 | 14,913,536 | 3.750 | 4.038 |
| bodies-package | 15,478,784 | 15,122,432 | 4.221 | 3.934 |
| package-bodies-package | 24,649,728 | 23,781,376 | 7.221 | 9.012 |
| sparse-inline | 8,007,680 | 7,573,504 | 6.373 | 6.783 |
| sparse-package | 8,101,888 | 7,663,616 | 6.169 | 6.240 |
| membership-inline | 21,389,312 | 8,552,448 | rejected at 9.511 | rejected at 9.533 |
| membership-package | 21,258,240 | 8,466,432 | rejected at 9.511 | rejected at 9.536 |

The seven earlier profiles pass with 100 identical artifact pairs and 28 successful original/edited executions.
Both metadata-heavy member edits are rejected by the existing deadline in both compilers. Their prior roots remain
unchanged and executable; eight separate executions verify original and retained outputs. Those executions do not count
as successful edited builds. The new workload therefore fails incremental acceptance, despite its lower memory use.
No count, memory, or deadline guard is relaxed.

The metadata-heavy inline profile drops from 21,389,312 to 8,552,448 bytes peak compiler RSS; its package profile drops
from 21,258,240 to 8,466,432. Clean and cold-population times remain separate from the incremental requirement. These
observations show lower resident metadata, without establishing faster compilation or target-scale compliance.

### Forced loading paths

A copied two-module control validates normal cache reuse and independently publishes both modules before removing
source. The metadata-heavy projects restore the original sources after the recorded rejected edit and populate fresh,
coherent caches. Removing the completed-root record then forces actual module loading.

For the 256-module source-free setup, canonical module objects are extracted from those native-produced cache records.
The first member is separately produced by the native module command and matches exactly in both revisions. This setup
is distinct from independently publishing every member. All 256 artifact pairs and final roots match byte for byte.
Four final source-free executions verify both profiles.

| Operation | Peak RSS before, bytes | Peak RSS after, bytes | Time before, seconds | Time after, seconds | Outcome |
| --- | ---: | ---: | ---: | ---: | --- |
| control-package: cached_component_relink | 8,159,232 | 6,909,952 | 0.595 | 0.713 | both complete |
| control-package: source_free_component_link | 7,962,624 | 7,168,000 | 0.562 | 0.685 | both complete |
| membership-package: cached_component_relink | 20,934,656 | 7,491,584 | 9.403 | 9.532 | after rejected at deadline |
| membership-package: source_free_component_link | 20,729,856 | 7,233,536 | 7.855 | 7.889 | both complete |

The rejected cached-load result remains a failure. Its independent source-free follow-up does not replace that result.
Source-free metadata loading reduces observed peak RSS from 20,729,856 to 7,233,536 bytes while completing in about
7.9 seconds in both revisions. Strict aggregate accounting, wire-level membership repetition, paged declarations,
selective invalidation, and much larger workloads remain open.

Reproduce the new source workload with:

```sh
uv run --no-project tools/benchmark-cyclic.py target/release/bifoilc \
  .bifoil/development/cyclic-membership --modules 256 --functions 257 --name-padding 160 --package
```
