# Compiler architecture

The compiler is a native Rust program. Parsing and analysis operate on owned source data; only lowered instructions and
explicit type interfaces enter module artifacts. Execution belongs to the separately versioned runtime.

Parsed parameter headers retain immutable shared lists. Checked callable headers use `Signature = Arc<SignatureData>`,
sharing parameter contracts, generics, identities, results, and effects across symbols and interfaces. Inference and
specialization detach changed headers and parameter lists, preserving other views. Signature substitution uses one
query-local memo; empty argument maps still normalize nullable application. Declaration preparation moves normalized
method bodies into lowering after retaining their checked signatures in the type catalog.

Final source compilation takes ownership of the parsed AST and normalizes it in place. The loader keeps source paths and
content hashes for validation/publication, while transferring the active AST into lowering. Cyclic declaration and
interface passes explicitly clone their borrowed source views until convergence; final member lowering consumes each
source AST after import-scope construction. This separates reusable interface analysis from the one-time body pass.

## Ordered checking and artifact compatibility

Expression lowering retains the type of each evaluated operand and checks the operator against those captured types.
A later effect cannot change the value already on the stack, while a subsequent read must use the current proof state.
Non-emitting applicability, generic inference, and binary inference reuse return analysis's normal-completion walker;
only facts surviving a normally completed earlier expression reach the next one. Function return inference shares
nonempty iterable evidence and actual loop exit joins with lowering.

Generic bounds are semantically validated after nominal interfaces are installed. Equality-based collection methods
use the same element/key eligibility query as operators. Loaded module graphs check callable/storage collisions before
publication. Dependency discovery sorts and deduplicates module identities independently of cache/source representation.

Compiler artifacts use bifoil.module.v3, requiring regeneration of v2 typed bodies. The VM profile remains v2 because
these repairs retain the existing instructions. Template parts retain physical expression starts; choice conditions
retain their own starts and once sites use opening headers. This metadata survives serialization without extra runtime
scope instructions.

## Pipeline

1. The CLI reads the manifest and discovers source or artifact dependencies without constructing every source AST.
2. The lexer and parser construct the active component's AST, defined in `src/syntax.rs`.
3. Declaration preparation resolves names, aliases, and generic scopes, then generates constructors and copies.
4. Semantic analysis checks types, captures, initialization, reachability, and suspension permissions.
5. Lowering emits instructions with original source spans into independently compiled modules.
6. Shared-initializer analysis computes conservative dependencies and rejects cycles.
7. Completed components retain signatures and summaries while function bodies move to temporary storage.
8. The linker resolves summaries against the complete program and emits the deterministic startup plan.
9. Canonical serialization reads one stored function at a time without rebuilding the whole instruction graph.

`src/resources.rs` owns native allocation policy and platform memory enforcement. Linux/glibc uses one allocator arena
before reader threads start, then preserves the stricter existing address-space guard. `src/trace.rs` emits optional
elapsed phase timings when `BIFOIL_TRACE_TIMINGS=1`; ordinary output is unchanged. Loader/component boundaries and
package publication report their own work through that shared diagnostic helper.

`src/manifest.rs` owns bounded project-configuration input and its strict typed schema. The locked TOML parser handles
string, key, and comment syntax; CLI overrides and dependency discovery use the decoded values. Equivalent spellings
retain identical build semantics and output.

`src/source_format.rs` owns the source-indentation command. The parser's layout scanner optionally records protected
physical rows while combining multiline literals; ordinary parsing requests no protection bitmap. Formatting reuses that
scanner, preserves narrative/literal bytes, and expands leading code tabs without changing structural levels or
alignment. Syntax and output-budget checks precede publication through the existing atomic writer.

Reusable-module verification rejects empty/duplicate parameter names and invalid variadic placement/defaults after
checking the canonical digest. Runtime envelope validation enforces the same parameter rules before creating execution
state, including well-formed optional variadic flags. Valid source, instructions, and checkpoint formats are unchanged.

Expression tokenization scans borrowed UTF-8 source by byte offset. Whitespace decoding advances by scalar width; ASCII
names, numbers, operators, comments, and the literal balancer return valid boundaries. `src/lexer/numbers.rs` owns
numeric-token extent, while literal parsing retains validation of radix, separators, and range. Tokens own only their
resulting text; no complete character or byte-offset arrays are allocated. Located and unlocated scans share these
rules, with regressions for multibyte text, multiline starts, numeric ranges, and keyword boundaries.

Dependency discovery shares the full parser's import grammar and ignores import-looking triple-string contents. It
retains file paths and source hashes instead of buffers or ASTs along unfinished import paths. Compilation rereads the
active source and verifies its hash before parsing; edits during discovery require a retry. Multiline literal layout
scans each physical line once while preserving source line numbers. `src/lexer/literals.rs` shares a bounded, iterative
quote/interpolation stack with expression tokenization and template decoding. Nested strings, triple literals, quoted
braces, and code comments follow the same balancing rules. Template decoding borrows expression slices instead of
copying character arrays. Prose joins narrative lines while retaining code and literal newlines inside interpolation;
embedded directive markers there remain literal. Discovery and full parsing report the same lexical source locations.
Conditional statements and block expressions share one branch parser. Branch lookahead inspects only the next aligned
header; it cannot consume a following value block or anonymous function. Prose branch headers require their executable
prefix, so literal `else:` narrative remains text. Whole cyclic components still require active ASTs.
`src/parser/operators.rs` shares precedence and continuation recognition. A terminal `>` is checked against complete
type/reference grammars before joining another source line. Lexical `!in` and `!is` require identifier boundaries; ASCII
operator matching borrows source bytes without allocating candidate character arrays.

Flow scopes track definite and possible initialization separately. Reachable branch joins intersect the former and union
the latter: reads require definite initialization, while delayed `val` writes require no possible prior write. Loop
backedges and right-hand-side block expressions retain those checks. `SkippedPath` merges safe arguments and
short-circuit operands without treating skipped writes or returns as executed. Unchanged shared scopes are retained
without copying their binding/signature maps; changed joins borrow existing bindings and replace only different facts.
Constant Boolean branches and unreachable loop jumps do not contribute impossible successor states.

Choice preparation merges hidden and once-consumed paths under the selected evaluation order. Visibility guards narrow
the live preparation branch; enabling does not skip visible label evaluation. Selected bodies retain guard proofs only
for immutable, nonvolatile bindings, since later preparation can change mutable values. Provably unavailable options and
unreachable fallbacks do not force normal-return paths.

Null checks and assertions retain generic parameter identity with an internal non-null refinement. Substitution removes
null from the resolved argument; the source syntax still uses ordinary `T` and `T?`. Nullable block headers distinguish
a terminal type question mark and body colon from a continued Elvis expression. Generic upper views expose checked
nominal fields/methods, built-in members, collection traversal/mutation, primitive operations, and function/scene calls.
Nullable subtype checks admit null separately and compare the source's non-null refinement with the target's present
type. This preserves nullable-capable generic bounds through function and nominal variance. Compatible joins retain an
already-proven subtype relation before decomposing nullable syntax. Neither rule creates a function adapter, converts a
payload, or introduces an arbitrary `Any` fallback. Sibling variants may join through their declared sealed parent only
when both original types are subtypes of that result. Invariant arguments and unrelated nominal declarations retain
their existing restrictions.

`src/generics/inference.rs` owns structural call constraints. Scene input shapes, declared sealed parents, and known
upper-bound interfaces can supply type arguments. Only the callee's declared parameters are inferable; enclosing
parameters retain their caller-selected identities. Final applicability still checks nullability, invariant arguments,
function compatibility, and approved conversions. Inference does not invent a family copy or signature adapter.

Supplied arguments establish raw compatible joins before the expected result is applied. If those constraints conflict,
`src/compiler/overloads/specialization.rs` supports a retry seeded by the explicit result context. Arguments are still
checked under the completed signature, so an unchanged narrowed binding may retain its original generic identity while a
computed value cannot acquire it. Successful nullable-input inference and literal joins remain the preferred path.

Bounds participate in that retry. When applying the result context would invalidate otherwise valid supplied arguments,
keep the supplied instantiation if its completed result can satisfy the context. Contextual arguments may retain the
non-null refinement of their original identity when a bound requires presence; ordinary argument checking still demands
a current proof. Non-null bound refinement is monotonic across dependent parameters. It grants no equality/key
capability and no computed-value identity.

`src/generics/constraints.rs` applies declared bounds after the supplied arguments have established their joins. It
selects existing Int-to-Float or Char-to-String widenings, including recursive immutable containers, until dependent
bounds stop changing. Each candidate must be a subtype of its bound and use only an approved widening conversion.
Explicit type arguments and function-reference signatures bypass this pass. It cannot invent an Any fallback, a
container-family copy, mutable argument widening, nullable unwrapping, or an invariant nominal conversion.

Generic function references use the same specialization module. If whole-signature inference violates a bound, retain
bound-valid input arguments and infer result-only arguments from the expected function type. Non-null refinement and
return covariance still require a complete function-subtype proof. This creates no numeric or copying adapter and does
not discard a named function's result for Unit. Overloaded references still require a unique compatible candidate.

Projected nominal arguments retain explicit capability bounds and proven closed upper bounds when deriving the
containing type's equality/key eligibility. Built-in `Set<*>` elements and `Map<*, V>` keys retain their required stable
key guarantee; List elements and Map values do not gain such a guarantee. Dependency tracking includes types reached
through projection bounds, so later mutable definitions cannot leave an optimistic capability cached. Primitive-only
container queries avoid rebuilding unrelated nominal formulas. Mutable fields and unknown hidden contents remain
ineligible for stable keys; parameter-dependent existential bound proofs remain conservative.

Approved container-family copies can use a non-null generic value's proven collection bound. Projected targets retain
concrete runtime arguments in newly constructed containers; copying plus projection keeps its existing overload rank.
Nullable source parameters still require an explicit check or assertion before a non-null copy.

`src/compiler/enum_aliases.rs` registers qualified cases of plain enum aliases at file and lexical block entry.
Inference and lowering share the same registration, including value-block prefixes and inferred local functions. Visible
cases are indexed once per registration; aliases preserve the original constructor identities and add no runtime type.
Repeated inference is idempotent. Local scope, bare-name conflicts, source-free callbacks, and saved suspension have
regressions.

`src/compiler/overloads/ranking.rs` owns conversion dominance and the variadic/default/generic tie-breakers. Ordinary
calls and closed capture call profiles share it. `captures/call_profile.rs` retains compact primitive argument type
facts without storing expression trees or argument values. When every argument is known and every candidate has closed
nongeneric parameter types, it can select one declaration's body/default reads. Unknown arguments, generic candidates,
and ambiguity retain conservative dependencies. Literal type facts participate in bounded memo keys, preventing a
successful Int call from masking a later Float overload's deferred read.

`src/compiler/overloads/arguments.rs` shares argument positions and declaration-order omitted defaults between overload
selection and capture call profiles. Named calls lazily build a borrowed name-to-index map; supplied-position marks
avoid quadratic default-membership scans. Source evaluation remains in the lowering layer.

Named callable capture requirements retain a declaration-scope limit. Proofs and escaped-cell marking resolve names
through that prefix, preventing a caller's inner binding from satisfying a different, later capture. References,
containers, overload selection, and omitted defaults retain the same limit. When a scope retires, surviving requirements
are tightened to their resolved outer binding; retiring local reads still require initialization. These anchors are
compile-time metadata and do not alter instruction encoding or runtime closure identity.

Physical capture discovery retains every declaration ID denoted by a local overload name. Static selection of deferred
reads does not remove those function cells from a saved environment. Named and anonymous callbacks have source-free,
post-return checkpoint regressions for the collector's retention of the selected target.

The built-in Unit value is registered in the root environment for lexical collision and expected-enum checks. Lowering
still emits its constant directly; registration introduces no runtime cell or instruction-format change.

Bindings distinguish compiler-installed method receivers from ordinary source names. Private fields and method overload
filtering require that trusted receiver marker and compare nominal declaration identities. A parameter or local named
`this` grants no private permission; another generic instantiation of the same class shares its declaration's private
scope. Return inference and lowering install the marker consistently, and nested functions retain the lexical permission
without forcing an otherwise unused receiver into runtime captures.

A narrowed binding retains its original declared type as well as its usable refined view. Contextual name typing can
consume that original identity, including `T` after an `is String` or variant test. Argument ranking uses that same
contextual type when the narrowed view alone cannot satisfy the parameter. This does not adapt computed values or
perform a runtime conversion; a newly computed String still cannot satisfy an unrelated caller-selected `T`. General
intersection-type inference remains unfinished.

`src/initialization/flow.rs` retains only known local callable facts at basic-block entries. Missing facts mean unknown,
including after calls that could change captured bindings. Joins intersect the known-binding domains and union their
possible targets; a known empty target set remains distinct from unknown. Operand snapshots survive local invalidation.
The sparse representation avoids copying redundant unknown entries at every default-argument branch.

`src/types/overlap.rs` performs bounded, conservative possibility checks for casts. Nullable types can share null;
covariant containers and compatible function shapes can share a runtime subtype. Invariant slots require a possible
identical reified argument. Generic bounds use the proven non-null member view after shared null paths are handled.
Variance checks and overlap proofs share `argument_variance` from declaration metadata. Distinct primitive types,
container families, and closed invariant arguments retain disjointness diagnostics. Runtime casts inspect the actual
retained type and preserve the value and its aliases. Complex bound and inhabitation proofs remain conservative.

Capability bounds never invent a receiver shape. Nullability and private-member checks remain at access boundaries;
function-value calls retain positional interfaces, variadic shape, and checked suspension effects.

Inference does not emit instructions. Value-producing blocks share callable-result flow analysis for their statement
prefixes, conditions, guarded cases, loop exits, and early transfers. Ordinary calls invalidate mutable proofs, while
built-in calls preserve them after their arguments have been analyzed. `src/compiler/local_functions.rs` shares local
result convergence between inference and lowering. Concrete returns can anchor a recursive group; wholly unresolved
cycles still require annotations. The same provisional join handles ordinary returns and expression-bodied `if`/`when`
recursion. Variables remain visible in lexical declaration order. Bodies without inferred local functions skip the
callable-convergence pass.

`src/compiler/result_types.rs` retains the original generic type alongside a narrowed result view. Normal compatible
joins remain preferred; an unchanged generic binding can supply its original identity when needed, or retain a more
precise subtype. Nested blocks carry that alternative until contextual typing fixes the result. New values receive no
such identity, and nongeneric Any bindings do not add an arbitrary Any fallback. Only the selected type enters the
instruction/interface format. Non-null assertions pass a nullable expected type into their operand and retain the
unchanged operand's generic identity after removing null. Newly computed values receive no original-type alternative.
Lowering reuses the already inferred value-block type instead of analyzing it twice.

Named local functions retain only immutable narrowing already established when their enclosing block begins. Their
hoisted closures may be called before a later source declaration or guard. Body/default checking must therefore reject
facts established only later in that block. Anonymous functions retain immutable facts from their actual creation point.
Both forms still discard mutable captured proofs at a callable boundary.

Lowering preserves source evaluation order, including conversions, omitted defaults, short-circuit paths, and partially
evaluated expressions that may suspend. `src/types/display.rs` renders source-language type names in diagnostics;
`src/diagnostics.rs` preserves an existing precise source location instead of repeatedly prefixing outer locations.
Parser continuation scans retain progress across physical lines and check expression depth during construction,
preventing oversized left-associated or postfix trees from reaching recursive destruction.

`src/declarations/normalization.rs` checks binding names before converting implicit members to explicit `this`
receivers. Parameters, locals, local functions, loop variables, `when` subjects, and payload bindings cannot hide an
instance member under the selected no-shadowing rule. This prevents a lexical read or assignment from silently becoming
a field operation. Generated `copy()` is included in implicit method lookup; generated constructor/copy parameters
remain internal and are prepared separately.

Explicit :: references and bound receiver::method expressions resolve their callable before a subsequent value call.
That call exposes parameter/return types and variadic shape, without names or defaults. Its argument list cannot select
an unresolved overload or infer an otherwise unknown generic reference. Declared name/member calls retain their named
and default interfaces. Scene references follow the same explicit positional input rule. Receivers are captured before
arguments, including saved suspension; method emission uses the same trusted receiver permission as overload selection.

`result_types::infer_value` keeps the present part of an original generic type when a stable read has a proven non-null
view. Later branch joins can retain T without adding an unnecessary nullable wrapper from T?. This applies to lexical
bindings, immutable fields, and stable container lookups; real null-return paths remain nullable and computed values do
not acquire a caller-selected generic identity. Block, if, and when inference, source-free references, and suspended
returns share these checks.

`src/compiler/field_proofs.rs` owns sparse shared facts for stable value paths. Its `path.rs` resolves a nonvolatile
lexical root and typed field/index selectors, preserving qualified static lookup. Mutable fields use transient
selectors; index/key steps require a constant or lexical binding into a List, Map, or String. Mutable containers use
transient selectors, independently of whether their key bindings are reassignable. Binding selectors use analysis-only
lexical identities that survive scope copies and distinguish later declarations with the same spelling. These identities
never enter instructions, artifacts, runtime values, or checkpoints. `key.rs` normalizes constant map keys through the
declared key type, including numeric/character widening, nullable keys, equal zero signs, and nominal enum case
identities. Qualified and expected-type enum spellings identify the same key. Map duplicate diagnostics use this same
typed representation. Known prefix types guide nested lookup; ordinary member, privacy, and index checks still run
before consuming a fact. Reassignable roots lose facts on assignment and ordinary effect invalidation. Built-in `Entry`
keys and values share the field catalog with nominal fields, preserving the map traversal's immutable snapshots and
projected read types. Contextual reads and result inference retain the declared generic identity of an unchanged field
alongside its refined view. Null checks and assertions refine the current view instead of discarding an earlier type
test.

`flow::SubjectSource` connects a `when` capture to its source only across genuine subtyping. Numeric/character
conversions and container copies do not narrow the unchanged original value. Paths with immutable roots and selectors
retain their connection across effectful or suspending case tests; reassignable roots, mutable fields/keys, and ordinary
binding sources require unchanged case-test provenance. Case matching and null remainders share that provenance between
return inference and lowering.

Field-path stability uses declarations::field_can_change to read the declaration's actual mutability. A projected
receiver may expose a restricted write interface while concrete aliases can still change a var field. Those paths remain
transient across calls and waits; declared val and entry fields retain permanent facts. Missing metadata is
conservative. The projection's assignment permission remains a separate check.

Branch joins retain facts proved on every reaching path. Named closures use block-entry field proofs. Anonymous
functions start from creation-time facts; FunctionEnvironment removes mutable, volatile, and transient facts before
checking the body. Immutable roots and selectors retain stable proofs across calls and player-input waits without
runtime or checkpoint metadata. Mutable-root field facts are cleared with ordinary mutable narrowing. Paths depending on
mutable fields or index/key bindings are transient even when the root is immutable. Calls discard both forms; heap
writes discard field-dependent facts across all visible aliases, while lexical key writes discard key-dependent facts.
Independent field/key facts survive unrelated writes. A per-binding key-use marker avoids scanning unrelated facts on
ordinary stores; visible aliases of a shared cell receive that marker together. Branch joins preserve possible key use,
and writes recheck usage after evaluating their right-hand side. Prepared choice guards cannot reestablish those
transient facts. Computed receivers and computed or composite map-key expressions remain conservative. Constant-index
proofs preserve generic element identity through result inference, and subject provenance uses typed paths rather than
encoded names. Ordinary bindings allocate no field-fact map, and unchanged scope/maps stay shared.

`bindings::aliases` indexes multiple imported names for the same shared global once during declaration preparation.
Assignment updates are shared by lowering and return inference: a write clears every alias's null/type and lookup facts,
updates retained value requirements, and invalidates dependent mutable keys. The shared name list is absent from
ordinary bindings and never enters artifacts. Local copies and parameter bindings retain their value snapshots.
Unaliased writes update one binding directly; aliased writes visit only the indexed names instead of scanning the whole
environment.

Shared-alias guards narrow every source name for that cell. Value paths and lexical index keys select one canonical
alias so equivalent reads share facts; readonly aliases remain stable across prepared choice guards. Safe fields and
indexes retain the receiver-prefix positions checked by a non-null result. Successful guards/assertions narrow those
stable prefixes before refining the final value. Nullable type matches do not establish presence, and effectful indexes
or invalidated when provenance cannot supply these facts. String indexes share this path machinery without changing
Unicode scalar bounds checks. Unqualified enum keys remain unresolved selectors until their typed path supplies the
expected key type, avoiding recursive reinference of nested receivers. All this metadata remains compile-time only.

Semantic regression bodies live under `src/tests/`, grouped by syntax, types, casts, captures, members, assertions,
immutable/mutable-root field proofs, generic inference/bounds, flow, expressions, and module validation. `src/tests.rs`
supplies the shared independent compile helper and module declarations.

## Borrowed initialization views

Initialization analysis borrows dependency symbol names, signatures, shared definitions, and completed summaries.
Modules without their own shared declarations still produce ordinary-function summaries for later clients; they skip
only the unneeded dependency-resolution pass. The linker collects initializer IDs before updating dependencies, rather
than cloning every shared definition. It builds borrowed function/summary indexes only when shared bindings exist.

`src/initialization/dependencies.rs` memoizes completed dependency sets for helper targets with multiple incoming calls.
Each query uses the current scope's immutable summaries and possible-target set. Recursive walks retain an explicit
visited set; computing a new memo cannot recursively start another new memo. An additional completed set covers all
conservative indirect targets. Failed queries remain failures and are never cached. Borrowed binding retention is capped
at 32,768 entries for helper memos, plus one set for the indirect group.

The conservative possible-call target set and dependency-first execution order remain unchanged. Native and source-free
checkpoint regressions retain summaries through a helper module with no shared declarations, including its imported
reads and effects. `benchmark-cache-import.py --workload shared-import` isolates a 4,090-binding dependency from its
small reader module.

## Shared dependency-list storage

Shared definitions hold immutable reference-counted dependency slices. `src/shared_dependencies.rs` reuses equal lists
within one resolution or artifact-input scope, bounded at 32,768 retained names and 4,096 distinct lists. Its keys
retain the list's exact order and contents; serialization remains the same JSON array with the same canonical digest.

`src/module_input/streamed/shared.rs` interns each definition before retaining the next. It also rejects duplicate
shared-binding keys and enforces the existing function-derived shared-definition budget. Metadata errors discard private
function spools like other invalid input. Source and streamed-input regressions check storage reuse and unchanged bytes;
a source-free checkpoint case retains independent initialized values and one-time side effects.

`benchmark-typing.py --workload shared-dependencies` supplies 128 seed bindings and 1,500 consumers of one broad helper.
Wire-size guards, eager runtime metadata, and the broader capacity target remain separate work.

## Packaged output

`src/package.rs` borrows each defining module's function map from `FunctionCatalog`. The shared `StoredFunctions`
serializer restores and verifies one stored body at a time. The chunk hash covers code and signatures, excluding
dependency digests; the root retains those module identities separately. This lets a recompiled client keep its existing
code scene when only a dependency's identity changed.

`artifact::matches_json` compares a bounded stream against an existing file and stops at the first mismatch. Missing or
changed scenes receive the ordinary atomic write. All immutable chunks publish before the root manifest, whose own build
hash includes its chunk inventory. Chunks survive unsuccessful builds: deleting one could invalidate a root published
concurrently. Cache pruning therefore needs an explicit retained-root policy. Changed package builds still traverse code
bodies and keep declaration metadata resident. Completed-build receipts include the output mode and exact root/chunk
byte checksums. Their bounded scene-name inventory excludes arbitrary paths; corruption falls back to normal validated
compilation. A failed receipt update prevents root replacement. Only an unchanged, fully verified receipt can skip body
decoding and linking.

`tools/benchmark-packages.py` measures isolated inline/package population, unchanged builds, and leaf edits. It records
file hashes, modification times, and written bytes; optionally executes each output through restored checkpoints using
explicit runtime/Node paths. An optional baseline executable checks that inline bytes remain unchanged.

## Source map

| Location                                                                | Responsibility                                                                           |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `src/main.rs`, `src/project.rs`                                         | CLI, project builds, linking, and process resource limits.                               |
| `src/module_validation.rs`                                              | Reusable artifact identity, nominal ownership, member interfaces, and body checks.       |
| `src/input.rs`                                                          | Bounded source and streamed artifact reads with exact-byte checksums.                    |
| `src/code_store.rs`                                                     | Owned temporary function-body storage and canonical per-function serialization.          |
| `src/scope.rs`                                                          | Borrowed imported-module views and module isolation.                                     |
| `src/loader.rs`, `src/loader/groups.rs`                                 | Dependency components, cyclic interface resolution, reuse, and group validation.         |
| `src/artifact.rs`                                                       | Streamed canonical hashing and bounded atomic artifact output.                           |
| `src/lexer.rs`                                                          | Tokens, literal scanning, and indentation measurement.                                   |
| `src/syntax.rs`                                                         | Source expressions, statements, declarations, and patterns.                              |
| `src/parser/`                                                           | Separate expression, type, declaration, statement, layout, prose, and pattern grammars.  |
| `src/declarations/`                                                     | Name resolution, annotation normalization, generated declarations, and nominal metadata. |
| `src/types.rs`                                                          | Subtyping, approved conversions, compatible joins, and built-in capability queries.      |
| `src/generics.rs`                                                       | Type substitution, argument inference, and bound checking.                               |
| `src/compiler/inference.rs`, `src/compiler/return_inference.rs`           | Expression types and flow-aware callable result inference.                               |
| `src/compiler/flow.rs`                                                  | Branch joins, narrowing, and loop initialization checks.                                 |
| `src/compiler/captures.rs`                                              | Lexical capture discovery and initialization requirements.                               |
| `src/compiler/context.rs`                                               | Per-function instructions, source spans, jump patching, and resource checks.             |
| `src/compiler/expressions.rs`                                           | Expression and assignment-address lowering.                                              |
| `src/compiler/calls.rs`                                                 | Callable specialization and ordered argument evaluation.                                 |
| `src/compiler/assignments.rs`                                           | Binding declarations and checked assignment updates.                                     |
| `src/compiler/conditionals.rs`, `loops.rs`, `choices.rs`, `patterns.rs` | Control-flow lowering.                                                                   |
| `src/compiler/functions.rs`                                             | Function signatures, parameter defaults, and return handling.                            |
| `src/compiler/modules.rs`, `src/compiler/modules/interfaces.rs`         | Shared declaration preparation, module lowering, and provisional interfaces.             |
| `src/model.rs`                                                          | Serialized types, instructions, functions, modules, and programs.                        |
| `src/tests.rs`                                                          | Semantic regressions expressed as readable Bifoil programs.                              |
| `tests/cli.rs`                                                          | Real CLI builds, artifact reuse, corruption checks, and resource failures.               |

Overload applicability and selection live in `src/compiler/overloads.rs`. Lexical bindings retain an explicit set of
signatures and per-callable capture requirements. Selection compares conversion ranks before applying tie breakers;
lowering captures the selected callable or receiver before evaluating arguments. Nominal method metadata and exported
interfaces preserve the complete set, with distinct runtime IDs for each function.

`src/compiler/captures/requirements.rs` distinguishes binding reads from named calls with supplied argument shapes.
Transitive expansion retains that shape in visited keys, so a body-only reference cannot mask an omitted-default read of
the same function. Binding reads expose function bodies; named calls add only omitted defaults from shape-compatible
overloads. `src/compiler/overloads/arguments.rs` shares the actual position/name/spread validation. Unknown targets and
transitive overload types remain conservative. Initialization and scope-exit expansion use explicit worklists; runtime
closure names remain flattened, preserving instruction and checkpoint formats. Nested callable capture unions retain
their existing conservative dependencies.

`src/compiler/captures/empty.rs` memoizes only local, immutable declared-callable graphs with no variable leaves. Keys
retain every declaration ID in an overload group and the complete binding-read or call profile. Mutable values, ordinary
captured bindings, and top-level/imported declarations stay on the current-state check path. A shared visited node
proves no emptiness unless it belongs to the current closed proof; empty recursive components can be retained. Candidate
and retained key storage are bounded to 4,096 entries and 1,000,000 estimated bytes. The compiler owns the cache for one
module compilation; no environment snapshots or runtime/checkpoint state enter it. Different defaults, changing function
variables, and sibling declarations have negative regressions.

Plain captured assignments have address-only write requirements. They retain runtime cells without demanding an
initialized old value or expanding callbacks stored in that value. Compound assignments and explicit reads keep their
read requirements; captured compound checks run at invocation, while current-function reads retain direct checks. Scope
retirement permits write-only cells to remain uninitialized. Opaque writers do not mark those cells as readable aliases.
This does not establish definite assignment in the caller after a call, and repeatable captured `val` writes remain
conservatively rejected.

Capture discovery in `src/compiler/captures/names.rs` separates body names from each parameter's default expression.
Per-overload records are shared immutably across analysis scopes. Named calls retain the selected record before argument
evaluation, then require captures only from the body and omitted defaults. Checks run after supplied arguments and
conversions. Function values expose explicit fixed arguments, so their unused defaults add no read requirements. Runtime
environments still retain the complete capture set, including future bindings. Transitive callable requirements remain
conservative so indirect default reads cannot disappear from initialization proofs. Default expression blocks exclude
their own local declarations from free names. Named-function requirements retain only names visible at the function's
declaration, including preceding variables that may not yet exist at an earlier call. Hoisted function names remain
visible throughout the block. Enum shorthand and other nonbinding identifiers are resolved by ordinary semantic checking
rather than treated as uninitialized captures. The scan inspects referenced names without copying the module's complete
symbol table.

`src/declarations/variance.rs` checks field and method positions after callable result inference. It composes nested
type-constructor variance and reverses function-input positions. Generated copies construct a fresh instance using the
statically requested result type.

Definition-time annotation checks live in src/declarations/well_formed.rs. Recursive equality and stable-key
qualification use finite formulas in src/declarations/capabilities.rs: each formula records required type parameters or
impossible qualification. A dependency worklist propagates requirements without expanding recursive generic types.
Catalog replacement invalidates the formula cache. Projected receiver interfaces live in
src/declarations/projections.rs. Read views widen hidden values from their bounds; input views must fit every possible
hidden argument. Callable parameters and contravariant nominal arguments reverse the direction. Covariant containers
preserve safe lower views, including empty immutable collections. Invariant input positions remain unavailable;
invariant reads retain stars. Candidate types must still satisfy their bounds and capabilities. Explicit nullable inputs
admit null; bare hidden parameters expose Nothing. Defaults and empty variadics remain usable. Effects, variadic shape,
generic owner identity, and the original calling contract are retained without function conversion adapters. Non-null
result refinements remain present in projected upper views. Expected stars do not select concrete inferred arguments.

Shared binding registration and initializer lowering live in src/compiler/shared.rs. The separate src/initialization/
pass owns callable-origin flow, transitive read/write dependencies, and stable topological scheduling. Unknown indirect
calls include every available ordinary target. Module artifacts retain initializer identities, declaration order, and
dependencies; the linker recomputes against the complete program. Runtime loads never trigger initialization.

JSON artifact and spool decoding enables serde_json's `float_roundtrip` feature. Serialized finite binary64 values
retain their exact bits when read back, including subnormals and adjacent values used as stable collection keys.
Canonical digest verification uses those decoded values; a second rounding step could otherwise reject a valid
source-free artifact or merge distinct keys. Focused regressions also preserve defaults and shared initializers.

`src/cache.rs` owns disposable source-module cache records. It hashes the executable and source contents, validates
module digests, and leaves dependency validation to the loader. Cached code is reused before parsing; any changed import
digest rebuilds its client. Source-free distribution artifacts remain distinct from this disposable cache.

`src/cache/completed.rs` permits unchanged completed builds to skip decoding and linking. It verifies the compiler,
manifest-derived context, source precedence, every source/cache/artifact checksum, the native source inventory, and the
existing output bytes. Timestamps never establish validity. Up to four native threads verify independent module files;
all retain the incremental deadline and aggregate input accounting. A failed check takes the ordinary build path.

The loader discovers strongly connected import components with Tarjan's algorithm. `src/loader/traversal.rs` owns an
explicit work stack and one map of discovery indices, lowlinks, and active membership. Traversal yields dependency reads
and completed components to the loader; it does not parse or compile them. Acyclic components retain the direct
dependency-first compilation path. Cyclic components first normalize nominal declarations and signatures, then infer
interfaces to a fixed point, and finally lower every body against the complete interfaces. Shared dependency discovery
runs against the real bodies after lowering. Signature identities and type exports share builders between interface and
body passes; provisional Pending types cannot be serialized as artifacts.

When an acyclic cached consumer cannot be reused, discard its pending bodies and release its old interfaces before
parsing/lowering replacement source. Dependency validation has already finished, and the new source owns its complete
local declaration set. This prevents old and replacement nominal metadata from overlapping unnecessarily. An incremental
CLI regression changes an imported result type, checks fresh local method inference, preserves output after an invalid
edit, and verifies recovered cache reuse.

When a cyclic cache group cannot be reused and every member has source, discard its old bodies and interfaces before
seeding replacement declarations. Validation still checks the complete cached group first. Source-free members retain
their explicit recompile diagnostic; stale records cannot supply inference anchors for replacement source.

Traversal tests cover cycles, cross edges, completed imports, and a 100,000-node chain on a 64 KiB thread stack. That is
an isolated graph test; source discovery and linking retain their documented input, metadata, and workload limits.

A cyclic group's fingerprint hashes each member with its own digest, group descriptor, and internal dependency hashes
cleared, then hashes the ordered map of member hashes. Final artifacts record that group descriptor, the group hash for
internal imports, and ordinary body hashes for external imports. Reuse verifies every member and the entire normalized
group. This avoids circular hashes and rejects independently valid artifacts from different group revisions. Validation
moves temporary ownership rather than cloning all instruction bodies.

Artifact hashing serializes compact canonical JSON directly into SHA-256. Output writing uses a bounded buffered file
and removes incomplete temporary files on failure. Linking moves owned definitions into the program. These changes
remove whole-serialization and body-copy overhead. Completed components spool their bodies to an exclusive temporary
file; Unix unlinks it immediately, while other platforms remove it on normal cleanup. Headers and compact shared-call
summaries remain resident. Pending cached/artifact ancestors use a separate spool. Reusable acyclic modules summarize
and transfer one function at a time into the final store; stale cached bodies are discarded before recompilation. Cyclic
group fingerprinting restores its active component. An emptied pending spool is truncated. Whole cyclic components and
declaration metadata can still remain resident. Published modules and disposable cache records use private input spools.
Canonical digest bytes remain unchanged by the storage layer.

`src/module_input/streamed.rs` and its function-map visitor decode published and cached JSON modules one function at a
time into a private code spool. Function headers and compact spool handles remain resident. The bounded reader accepts
arbitrary field order, rejects duplicates and trailing data, and hashes the exact input bytes. A failed parse drops all
private handles. Canonical hashing and structural validation read one stored body at a time; only a verified module may
copy bodies into shared pending storage. Spool errors remain build failures.

`src/module_input.rs` shares canonical verification and pending transfer for stored handles. Its code wrapper serializes
typed instructions rather than raw formatting, and retained instruction counts avoid decoding solely to count code.
`src/cache/input.rs` parses disposable cache envelopes with the same module seed. A per-input failure channel preserves
private-spool I/O and deadline errors across Serde and canonical verification; these stop the build, while malformed
cache contents remain misses. The obsolete encoded-body wrapper and unused RawValue feature were removed.

Completed module, linked-program, and analysis views share the `TypeDefinitions` map's reference-counted nominal
records. Installing a catalog or publishing owned declarations clones those references rather than all fields and method
signatures. Inferred method changes use copy on write, so nested analysis cannot alter a published dependency or its
enclosing session. Serialization remains transparent; this does not add a protocol wrapper or remove resident metadata.
A regression checks sharing, method-update isolation, and restoration of the outer catalog.

The generic Function and Module containers retain their existing wire format and field order. Pretty-printed artifacts,
exact Float constants, captures, invalid instructions, cache recovery, fatal deadlines, and private-spool cleanup have
regressions. Cyclic fingerprinting still restores its active component, and declaration metadata remains resident.

`src/code_store/writer.rs` owns buffered append, offsets, legacy-call references, and instruction accounting. Source
emission and pending transfers share it. `src/code_store/tests.rs` checks bytes, summaries, discards, restoration, and
failure. `initialization::SummaryContext` borrows complete signature and shared-binding metadata while analyzing one
borrowed or restored body. It reads no other instruction bodies.

`src/code_store/counting_writer.rs` tracks logical offsets from bytes accepted by the buffered writer. It avoids
per-function position queries that [seek and flush the
buffer](https://doc.rust-lang.org/std/io/struct.BufWriter.html#impl-Seek-for-BufWriter%3CW%3E). Writes retain
partial-write and error handling, and each module finishes with an explicit checked flush. Append, pending restoration,
damaged storage, and canonical serialization have focused regressions.

Dependency analysis records deferred errors per ordinary function, rejecting only initializers that reach an invalid
summary. Artifact validation rejects foreign nominal owners and mismatched generated/member signatures; linking rejects
duplicate identities instead of allowing an order-dependent replacement. Proven exhaustive branches end their impossible
path with `unreachable`. Loop operand marks let break and continue discard partial inner expressions while preserving
operands owned by enclosing expressions.

`src/legacy.rs` validates primitive, plain-enum, and nullable bridge signatures and literal native targets, and collects
original sources without parsing them as Bifoil. `src/parser/legacy.rs` owns state-interface grammar;
`src/compiler/legacy.rs` registers live, volatile bindings. Function lowering emits a typed legacy-call instruction
after ordinary parameter/default setup. Enum case constructors stay with their owning nominal declaration, including
imports through transparent aliases in transitive dependencies.

Preliminary artifact validation can defer qualified external enum identities until the complete dependency graph is
available. `src/loader/legacy.rs` then checks every bridge against actual loaded enum definitions before either project
output or an independent module is published. Stored legacy-call references share one function-ID allocation per body.
Duplicate or empty nominal variant names are rejected during artifact validation. A resigned source-free interface that
substitutes a data type for an enum is a regression case; failing builds preserve earlier completed output.

Constant map-key checks share scalar constant proofs, including Unit and folded Boolean expressions. A sorted set avoids
quadratic duplicate scans. Unit and null have separate canonical keys; nullable-key checkpoints preserve that
distinction.

Expression-effect classification traverses explicit type applications and generated construction fields, in addition to
receivers, indexes, collection contents, and interpolation. Taking receiver::method<T> still evaluates the receiver;
specializing its type arguments cannot hide a call from short-circuit or when-subject stability checks. The traversal is
exhaustive over expression variants and is shared by single-use initializer eligibility, avoiding a separate divergent
purity check.

## State and ownership

Compound address updates capture their storage type during address evaluation, then retain the current read type before
evaluating the right-hand side. Later mutations cannot change the operator's already-read operand type. Effects during
address evaluation still invalidate earlier facts before that read is checked. Plain-binding compounds retain their
binding snapshot through the same evaluation order.

A `FunctionContext` owns the instruction sequence, lexical scopes, loop targets, return type, and suspension permissions
for one callable. Lexical scope snapshots use `rpds::RedBlackTreeMapSync` with shared tree nodes and entries. Writes
detach the search path and changed binding rather than copying the frame. An outer Arc retains fast unchanged-snapshot
checks. Continue paths keep their individual facts and correlations; storage sharing never aliases those analysis
writes. Bulk narrowing/key/heap invalidation collects affected names before taking mutable binding references. The
library provides entry sharing, so the temporary custom binding-entry wrapper has been removed. Other metadata catalogs
retain their existing map representations. Branch joins consider reachable exits and retain the union of deferred
capture requirements. `when` tracks effects through failed alternatives and guards, joins successful alternatives before
their shared body, and excludes known skipped paths. Effect-free scalar comparisons and short-circuit facts improve
proofs without replacing evaluation. Evaluated values carry those requirements through collection construction,
indexing, casts, anonymous functions, `when` bindings, and block results. Calls retain the callable first, then each
argument's requirements, and check initialization after supplied arguments finish. Returns, heap writes, and assignments
to captured or shared/global storage check currently retained reads before publication. A captured store applies this
check even when the writer was compiled before another function published a reader for that cell. Collection copies and
extraction methods carry receiver requirements into returned values; container-only built-ins do not invoke stored
callbacks. Bound method references retain receiver requirements alongside callable requirements. Scope exits retire
initialized local dependencies without losing their outer dependencies; loop backedges check carried callable state.
`single_use_initializers.rs` proves at-most-once executed initialization by later direct local helper calls, including
mutually exclusive source sites. The helper follows protected declarations in the same complete lexical body.
`statements::body_with_tail` inventories supplied arguments,
nested bodies, defaults, interpolation, and references before declaration checking; do/while also supplies its trailing
condition. Other uses forbid the proof. Captured-write discovery includes structured and expression bodies, while
ordinary immutable-flow checks prevent a second write within a call or loop. Generic headers and ordinary results retain
their normal checking.

Simple effect-free nongeneric parameterless non-suspending calls can expand through caller checks and retain a
capture-free Unit stub. Other supported calls keep their real body, captures, and lowering, including suspension and
returned values. Private targets retain entry reads, lexical slots, owner, runtime name, and binding identity. Every
protected val requires complete evidence on every normal return; proof exhaustion rejects the helper. Supplied arguments
run before the uninitialized-state check, and normal completion applies matching facts. Defaults retain their ordinary
selected evaluation, readiness, suspension, and immutable-write checks. Same-body declarations remain required; a sole
source call is required only by compact expansion. The detailed initializer ownership section below describes framed
calls and exclusive sites. No permission flag enters public artifacts.

The mandatory-write CFG can select an edge after an immediately preceding Boolean constant only when both instructions
belong to one basic block. A short-circuit jump can otherwise enter the branch directly with another stack value.
Partial branches, early returns, missing choice writes, and reads preceding initialization retain rejection; staged
input waits preserve already completed immutable writes.

`captures::escapes` marks mutable lexical cells retained by values crossing an argument, return, heap-write, or global
storage boundary. Later writes to those cells check their new requirements and propagate obligations into newly
referenced cells. The flag lives in copy-on-write binding state and merges across reachable exits; it does not leak
between exclusive branches or mark a direct call merely for reading a captured cell.

`captures::publication_names` summarizes retained free values, nested closure captures, and invoked helpers separately
from ordinary read preconditions. `captures::publication` applies those effects at reachable calls, including forward
calls, transitive wrappers, and only omitted defaults. Retaining a closure marks its live captured cells; invoking an
existing function value retains its payload without automatically retaining the variable from which it was loaded.
Literal overload profiles exclude unselected declarations. Empty named-call publication graphs use a separate bounded
cache; mutable payloads are always checked against current scope state. Summaries are conservative across unresolved
branches and aliases. They do not establish full interprocedural mutation or alias conformance.

Defaults execute inside their invocation frame, after supplied arguments. They inherit the declared function or scene
suspension permission, retain earlier parameters, and preserve progress across input checkpoints. Supplied-argument
paths join with default-evaluation paths, so a skipped default cannot establish initialization, reachability, or
narrowing facts for the body. Callable checking and return inference discard narrowing inherited from mutable or
volatile captures; immutable capture proofs remain usable. `src/compiler/return_inference.rs` joins only normally
completing `if` branches, preserving null/type guards after early returns. Calls and assignments conservatively
invalidate mutable facts before later result inference. `src/compiler/return_inference/expressions.rs` collects returns
from operands and nested value blocks in evaluation order while preserving lazy/skipped paths. Anonymous functions form
separate return boundaries. Successful assertions narrow stable bindings through the same helper used by lowering.
Located token batches retain physical lines through continuations, nested templates, prose paragraph flow, and option
labels. Anonymous function bodies use their expression line. Continued block headers retain their opening structural
level; the artifact instruction format stays unchanged. Artifact validation rejects missing instruction locations and
inconsistent named-capture contracts. Value-returning paths that evaluate to `Nothing` retain that bottom result during
inference; bodies with no value return still default to `Unit`. Transfer-valued unary operands remain valid because
their operator never executes. `return_inference/branches.rs` retains failed-test and guard effects, joins successful
alternatives, and applies only stable selected-option proofs. Its normally completing exits determine which facts reach
following statements. `return_inference/loops.rs` records normal condition exits and labeled break/continue
environments. It merges only paths that reach the successor, retaining direct do-body declarations at the closing
condition. Callable boundaries start fresh loop analysis, and constant-unreachable branches do not contribute transfer
or result paths.

Anonymous functions and named local declarations emit optional capture names resolved to their actual lexical runtime
bindings. Named closures are installed at block entry; their metadata is filled from definition-site visibility when the
declaration is checked. The runtime can trim complete capture graphs without resolving future cells prematurely. Module
validation checks those lists and local targets. Qualified capture paths follow the same lookup order as expressions.
Enum coverage uses resolved declaration identities, including imports and grouped cases. Constructorless type imports
install their variant namespaces without creating fake values or unapplied generic instances. Final exhaustive anonymous
`when` blocks return their branch values; a contextual `Unit` result still discards values under the selected rules.

Named type arguments use immutable shared slices. Cloning annotations, signatures, and bindings retains their argument
graphs; normalization uses copy on write when resolving names. Serialization retains ordinary JSON arrays and the same
canonical module/program bytes, so this ownership change introduces no wire-format version change.

The current `Type` helpers query thread-local nominal metadata. Catalog entries are shared immutable definitions;
lookups clone a reference rather than an entire class. Inferred method changes use copy on write only for the matching
definition. Artifact publication still produces owned type metadata. `AnalysisSession` gives that metadata a bounded
lifetime and restores any enclosing catalog, alias state, and generic bounds when compilation succeeds or fails.
Serialized artifacts contain owned definitions; they do not retain references to analysis state.

The runtime ABI is defined by `model.rs` and the matching runtime protocol. Changing an instruction or type encoding
requires updating its producer, consumer, validation, and relevant conformance coverage together.

Alias registration publishes all lexical names before resolving definitions. On demand, one resolver normalizes an
alias's generic bounds and body, then caches the complete canonical definition. The expansion guard covers both bounds
and bodies; arity is checked before instantiation. Plain and generic aliases share their enclosing lexical name frames.
Generic aliases retain only a small overlay of their own parameter bindings; the lookup view checks it before the shared
map. Ordinary callable scopes use the same parameter binder, preserving canonical identities and shadowing diagnostics.
Later aliases therefore work in bounds as well as bodies, including local scopes and exported source-free interfaces.
Substitution preserves unchanged shared children and memoizes nominal, function, and scene nodes within one query.
Nullable alias application flattens before signatures and exports are checked.

`src/types/graph.rs` owns borrowed, query-local identities for shared nominal arguments, function parameters/results,
and scene inputs. Depth checks revisit a node when a longer path could exceed the limit. Declaration validation,
capability formulas, dependency discovery, and variance checks avoid repeating identical subgraphs while preserving
different nominal names, node kinds, callable flags, and variance positions. These identities are never serialized or
used as language-level type identity.

Structural type equality checks metadata and shared argument pairs without unfolding repeated descendants. Pointer
identity is only a shortcut for an immutable argument allocation; independent graphs still compare structurally. Scalar,
nullable, parameter, scene, and callable shapes retain their existing equality rules. Diagnostic rendering has one
4,096-byte budget per displayed type, plus an ellipsis, and preserves UTF-8 boundaries. Ordinary short type names keep
their original spelling; diagnostic truncation never changes serialized type identities.

`src/types/subtyping.rs` owns relationship queries and their completed shared-node pairs. Retained argument allocations
keep identities valid for temporary bound refinements and substituted parents. Each query retains at most 4,096 pairs;
results never cross analysis contexts. Projection detection and closed-overload checks share the pure node traversal.
Alias resolution reuses at most 1,024 successful generic instantiations per request, keyed by alias and complete
argument types. Bounds and cycle checks still run; independent type arguments retain independent substitutions.

Generic call inference retains at most 4,096 completed structural constraint pairs for each supplied argument. Repeated
constraints contribute once; subsequent supplied arguments start fresh queries against the accumulated compatible joins.
The cache retains temporary argument allocations and cannot conflate distinct generic parameter identities. Projection
dependency checks use the same pure shared-graph traversal, skipping work entirely when no argument is hidden.

Canonical hashing streams at most 64 MiB of internal serialized data per hash operation, covering interface and summary
overhead beyond the 16 MiB published-artifact limit. A repeated private graph may remain compact, but publishing its
expanded representation must fit the existing limits. Hash-budget failure preserves the previous completed output.

`src/compiler/capture_initialization.rs` proves definite writes to mutable cells captured by completed local helpers,
including callback-bearing and other structured values. Captured-store checks prevent publication of unready values. The
control-flow classifier is exhaustive over instructions. Straight-line code uses two bitsets; branching code records
generated writes and entry reads per basic block and intersects incoming facts and normal returns. Choice analysis
includes all possible offered destinations and fallbacks. Runtime failures and transfers do not create normal return
paths.

Summaries retain lexical slot, runtime name, shared owner, and binding identity. Calls apply them only to matching cells
in the owning caller, before merging any skipped safe-call path. A proved overwrite discards dependencies of the old
value; earlier value snapshots retain their own requirements. Captured val writes require a bounded direct-call plan.
Calls through unknown function values remain conservative; eligible forward helpers retain declaration and code proofs.
Some
deferred stores to enclosing function cells also remain conservative because they require a stronger interprocedural
publication proof. The analysis caps block fact storage at one million 64-bit words and propagation at 200,000 steps;
exhausting an optional proof adds no initialization fact. Incremental deadline checks remain active. None of these
summaries enter artifact encoding.

`capture_initialization/entry_reads.rs` records reads before each block's local writes. After the fixed point, incoming
must-write facts discharge reads dominated by an earlier store. A completed helper that overwrites a cell on every
normal return can drop that exact binding's old body precondition when no read or opaque effect precedes its write.
Defaults keep their separate requirements; earlier value snapshots keep their own payloads. Calls, retained closures,
and publication before the write remain conservative barriers. Scope anchors and binding identities prevent a deeper
same-spelled declaration from supplying another cell. Runtime capture lists remain unchanged.

`types/overlap.rs` retains up to 4,096 completed shared-node relationships per cast query. Value overlap and possible
identity of invariant reified arguments use separate proof keys. Depth is also part of each key: a conservative answer
near the depth limit cannot replace a shallower disjointness proof. Retained argument allocations protect identities
derived from temporary parents or bounds. This creates no numeric conversions or container-copy adapters.

Function parameters, function results, and scene inputs use immutable shared allocations alongside nominal arguments.
Cloning an analysis type retains those allocations. Resolution uses copy-on-write when it must change children;
substitution keeps unchanged children and memoizes rebuilt graphs per argument environment. Structural equality, shape
predicates, depth checks, well-formedness, variance, capabilities, subtyping, generic inference, and cast overlap reuse
completed shared nodes within their existing analysis contexts. Callable identities include parameter and result
allocations plus suspension and variadic flags. Scene and nominal nodes have distinct identity variants. Serialized
module and VM type records retain their existing JSON shape; this ownership change introduces no runtime identity.


`types/conversions.rs` owns lossless scalar/container conversion ranks. `types/joins.rs` owns compatible common-type
inference. Each query retains at most 4,096 completed shared-node pairs, including failed relationships; common-type
results retain shared child allocations rather than rebuilding repeated branches. Query identities remain scoped to the
current declaration/generic context. Conversion ranks, nullable lifting, mutable invariance, and outer-family copy rules
retain their existing meaning. Inferred joins never introduce container-family copies or an arbitrary Any fallback.

`declarations/projections.rs` keeps projected reads and inputs in one `ProjectionView`. Its receiver definition, known
arguments, hidden identities, and at most 4,096 completed source-node/direction pairs belong to that query. A shared
type node may need distinct upper and lower views; direction is part of the memo key. Repeated results retain shared
children. Upper bounds, invariant restrictions, capability fallbacks, and callable effects/variadic flags retain their
checks. The cache never crosses receiver contexts or changes a projected value's runtime nominal identity.

`declarations/type_names.rs` owns persistent lexical name frames. A root contains module names; a generic declaration
adds only its parameter bindings, and a block declaring aliases adds only those aliases. Declaration-free scopes reuse
their parent's frame. Aliases retain the immutable declaration snapshot, including generic ancestors, without copying
the full visible map. Insertions report any visible previous binding so existing shadowing checks still apply. Alias
registration still precedes annotation normalization within a block, preserving forward visibility. Parameter overlays
for generic alias definitions remain local to that alias. No frame identity is serialized or exposed as language type
identity.

## Maintaining a change

Keep grammar, static rules, and instruction lowering separate. Add a syntax form to the AST and its parser first;
implement its static rules in analysis and its execution order in lowering. Runtime behavior belongs in the runtime
repository. A new module should own a coherent responsibility rather than becoming another miscellaneous helper file.

Use descriptive state and argument names. Prefer named options when several flags control behavior. Keep checks and
error paths visible; comments should explain semantic constraints or ownership decisions.

Format Rust with the checked-in 120-column configuration, and keep compiler warnings actionable:

```sh
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
```

For end-to-end checks, `tools/run-conformance.py` accepts explicit compiler, runtime, Node, and case-file paths. It does
not silently turn an adjacent checkout into a build dependency. The steering conformance ledger remains the place to
distinguish implemented, partial, and unverified decisions.

Result inference uses the same pure graph traversal for pending results and retained generic parameter identities. A
narrowed closed alias graph cannot trigger repeated tree expansion merely to prove that it has no parameters. The final
canonical interface still must fit the existing publication budget.


`src/declarations/resolution/order.rs` schedules each lexical alias group through body and bound dependencies before
normalization. A shared-node traversal collects only references to that group; type parameters and nominal definitions
create no alias edges. An index graph and source-ordered ready set remove recursive forward-declaration scheduling.
Definition checking, generic instantiation, and the existing expansion guard remain in the resolver. Source-free
acceptance retains generic bounds, nominal recursion, and checkpointed function values.


`ConditionalBranch` in `src/syntax.rs` represents horizontal conditional arms, each with a condition, scoped body, and
header line. The parser keeps the fallback separately instead of creating nested synthetic else blocks. Normalization,
capture walkers, value/return rewriting, inference, and emission iterate the arm vector. Failed-path facts and effects
flow into the next test; only completing bodies contribute successor facts. Value joins retain the previous association.
Lowering emits a shared end target and omits artificial false-path environments. Existing v2 module instructions remain
compatible in both compiler directions, while changed generated bodies receive different build identities.


`src/compiler/bindings/aliases.rs` applies assignment facts only after RHS evaluation and conversion. Its read-only
comparison avoids cloning a shared frame when initialization, narrowing, and callable requirements are unchanged.
Runtime stores and index/key proof invalidation remain independent of that optimization. Shared aliases are inspected
and updated together; ordinary local copies retain their own facts.


`FlowJoin` in `src/compiler/flow.rs` accumulates completed conditional, `when`, and choice paths. It keeps one joined
set of facts instead of a full scope snapshot for every arm. The initial join anchors non-flow metadata to the original
declarations; subsequent joins preserve the existing fact intersection/union rules. Lowering and return inference use
the same helper. Native exhaustive comparisons cover initialization, narrowing, field paths, escape/index flags, and
callable requirements; CLI flow-memory regressions live in `tests/cli/flow_memory.rs`.


Loop state retains its entry scopes and a `FlowJoin` for completed breaks. Each reachable transfer retires exited scopes
before contributing its facts, including labeled outer transfers. Final normal exits use the same accumulator. Continue
paths retain separate snapshots: backedge capture checks require each path's correlation between callback requirements
and initialized bindings. The return-inference loop state follows the same break-join rules.


`FunctionEnvironment` in `src/compiler/functions.rs` removes unstable capture narrowing when an environment is prepared.
Module siblings and generated shared initializers clone that prepared snapshot; local declarations and anonymous
functions prepare a snapshot from their own definition site. Function bodies still own their parameter and local scopes.
This prevents a quadratic scan of unchanged module bindings without retaining mutable capture proofs.


Module return inference reuses a prepared `FunctionEnvironment` for each fixed-point round. Signature-only updates
refresh the snapshot before the next sibling; shared-type updates happen before the following snapshot is prepared. The
ordinary local-function entry point still removes unstable capture facts independently. Deadline checks occur between
module functions as well as between rounds.


Ordinary named function headers use the same source/complexity envelope as anonymous functions and generated
constructors. `src/parser/declarations.rs` no longer applies an extra 32-parameter ceiling. Native CLI coverage compiles
and formats a wide header, while source-free runtime cases exercise defaults, named calls, and function-value calls.


`Cursor::generics` uses a local ordered-name index to reject duplicates while preserving declaration order in its result
vector. Generic declaration width is governed by the existing source/token/type/code/output guards rather than a
separate small parameter ceiling. Native tests and source-free checkpoints cover wide functions, data, aliases, and
sealed variants together.


`src/loader/inputs.rs` owns optional bounded input prefetch. Four scoped reader threads may prepare at most 32 MiB of
raw source/cache/artifact bytes across a build. A file-size reservation also limits its read, so racing growth cannot
allocate past that reservation. The loader consumes those bytes through the existing checksum, aggregate accounting,
seeded decoding, and structural validation paths. Failed optional reads fall back to ordinary input; private spool and
deadline failures remain fatal. Source recompilation still verifies its discovery hash before parsing the active AST.

`artifact::matching_json_checksum` compares existing output against streaming serialization and returns the checksum of
those same verified bytes. Package publication reuses that checksum without reopening the unchanged scene. The
comparison includes generated headers, suffixes, and end-of-file; no complete serialized artifact is retained in memory.


The layout scanner stores unchanged physical lines as borrowed `Cow<str>` views into the active source. Joined multiline
literals own their combined text; consumed continuation rows become empty borrowed placeholders. This keeps physical
spans stable without allocating each ordinary prose/code row. The parser owns the resulting AST independently of those
temporary views. Import discovery and the indentation formatter use the same scanner and protection bitmap.


`field_proofs/computed.rs` owns bounded identities for pure primitive key expressions. An immutable shared expression
tree retains lexical dependency identities separately, so copying a flow snapshot does not clone the original source
AST. Existing typed index/member checks still run first. `field_proofs.rs` marks every mutable dependency for assignment
invalidation, includes computed selectors in transient heap/call checks, and checks dependency identities when reading a
fact. The original constant/simple-binding selector paths remain separate fast paths. No expression is evaluated or
cached at runtime by this analysis.

String length, scalar indexing, interpolation, and primitive generic upper views reuse the bounded computed-key tree.
Immutable selected field paths retain their root dependency identity; all selected fields must be fixed, while unrelated
mutable fields do not invalidate the path. Calls and mutable/composite heap reads remain outside that optional proof.
Named function environments retain block-entry narrowing so forward calls cannot use guards established later in a
block; anonymous functions retain safe construction-point proofs.


`code_store/encoded.rs` serializes a completed function's canonical JSON without rebuilding its typed instruction
objects. The spool writer records a checksum over exactly the bytes accepted by its buffered output and a separate
signature/init/local fingerprint. Before reuse, the store verifies recorded length, checksum, expected header, JSON
syntax, and instruction count. Only one bounded canonical function body is retained. Module input serialization borrows
that body's raw code array; semantic validation still decodes typed instructions through the existing path. Private
spool failures continue through the fatal input channel, and atomic output cannot replace an earlier successful artifact
after such a failure.


`VerifiedModule<streamed::Code>` transfers only bodies from one validated private parse. `streamed::store_verified`
retains declaration headers and verifies storage ownership before `CodeStore::copy_pending_from` appends canonical
records through one buffered writer. Consumed records leave pending storage; unfinished ancestors remain available, and
an empty spool is truncated. After dependency interfaces resolve, `transfer_pending` still decodes a typed body for
initialization analysis and legacy-target discovery, then writes its unchanged canonical representation. Source/module
instruction limits remain separate from the aggregate package/program limit.


`cache::input::RecordSeed` carries the requested compiler and source identities into envelope decoding. A known mismatch
returns through the disposable-cache miss path before constructing an irrelevant module. Canonical records put these
fields first; alternate field order remains valid and may decode a body before discovering the mismatch. Matching
records still verify their digest, structure, and dependencies. Deadline and private-spool errors remain on the fatal
side channel.


`Module::summaries` retains immutable shared records. `FunctionSummary::shared` interns only the entirely empty effect
record; bindings, calls, indirect-call flags, and analysis errors remain independent. Origin analysis still checks every
body, and dependency queries borrow the same summary contents. This storage optimization does not enter module or
runtime JSON. A regression combines distinct pure bodies with a real shared read and verifies that initialization still
orders its dependency first.


## Lowered-code reachability

`src/compiler/reachability.rs` compacts each completed function once, after named/local/generated bodies have been
checked and before module instruction counting, initialization summaries, and hashing. Straight-line bodies truncate
after the first return or trap without allocating a graph. Other bodies retain the entry and conservative successor
edges, including deferred option destinations, then remap local addresses while compacting the existing vector. Calls
and scene transfers retain fallthrough conservatively. The bounded bitmap, worklist, and address map belong to one
function; deadline checks remain active.

`Op::instruction_target` and its mutable counterpart centralize local addresses for validation, patching, and pruning.
Function/scene identities, iterator IDs, stack-mark IDs, and option-history sites are separate and remain unchanged.
Source checking still rejects invalid unreachable statements. Regressions cover backward loops, all current target
forms, deferred choices, malformed addresses, idempotence, hoisted local functions, generated bodies, and canonical
source locations. Existing artifacts remain accepted without rewriting their code; producer/consumer checks preserve
their exact source-free roots.


## Shared source filenames

`Compiler::file`, `FunctionContext::source_file`, and `Span::file` share an immutable `Arc<str>`. Newly lowered
instructions, including generated and nested bodies, clone the module path handle instead of allocating another path
String. Serde retains the existing plain String representation; imported spans do not depend on a global intern table.
Validation, legacy references, source-mapped diagnostics, option-history sites, canonical hashes, and runtime values
retain their existing contents. The original source regression checks shared storage and exact serialization through
escaped/Unicode paths and reusable-module verification.


## Retained artifact validation facts

`src/module_validation/body.rs` records structural facts during the first typed function decode. Immediate source/target
failures and ordered reference/capture/legacy events retain the existing diagnostic order. Complete-module verification
checks targets against all function headers, compares named capture contracts, and validates legacy signatures against
the completed nominal interface. Pure bodies share one immutable empty fact record; capture sets share immutable storage
during contract checks.

`FunctionCode::structural_facts` supplies those facts from streamed input or derives them from an owned instruction
vector. Canonical module hashing still reads checksum-checked spool bytes before any facts can validate a module. No
proof flag is accepted from artifact JSON, and malformed source locations, targets, captures, references, digests, or
closed private storage retain failures. Facts leave with the input view after verified transfer. Initialization, legacy
discovery, and other semantic consumers retain their typed body reads. The unused second-decoding helper was removed;
public schemas and canonical bytes remain unchanged.


## Scoped canonical-value reuse

`src/code_store/encoded_cache.rs` owns bounded reuse during one module's package hash/compare/write sequence. Its scope
borrows the code store immutably, preventing record identity/count mutation, and releases entries on every exit.
Reservation includes serialized bytes, keys, and an allocation allowance; an oversized entry remains valid through the
ordinary uncached path. Nested scopes fail without disturbing an active cache.

`CodeStore::encoded` still checks the length bound, rereads actual bytes, and verifies the stored checksum before using
a cached immutable value. Each lookup compares the record's instruction count and header fingerprint with values derived
from the actual canonical payload. `encoded_checked` also verifies the supplied header's identity and contents. A weak
witness reuses that proof only for the same immutable signature allocation and unchanged flags; copy on write
invalidates it. The cache reservation includes its inline allocation allowance without retaining parameter or type
graphs. Only repeated syntax/count scans and already-proven immutable-header hashing are reused.

Package publication creates one scope per independently bounded module; pending input and inline output keep their
ordinary paths. Tests cover canonical bytes, pointer reuse, scope cleanup, nested rejection, oversized fallback, header
changes, false record metadata, valid-JSON corruption, and closed storage. Public schemas, chunk names, and build
identities remain unchanged.


## Expanded aggregate function ceiling

`MAX_PROGRAM_FUNCTIONS` and the runtime program guard now admit 81,920 functions. `MAX_FUNCTIONS` in independently
compiled modules remains 16,384. The public function graph uses ten scene modules, retains every user routine/result
across a prepared wait, and uses a Float accumulator so its checksum exceeds Int32 safely without changing the routine
result type. Exact-boundary helpers execute after restoration; the next-function rejection preserves the completed root
and code scenes. This count change does not introduce an instruction or artifact schema version. The larger source-line
target still requires disk-backed interfaces and lazy code loading.


## Buffered module input

`src/module_input/streamed/functions.rs` holds one `CodeStore::pending_writer` through the module function map. Each
decoded body supplies its header and structural facts, writes one canonical record, and leaves memory before the next
body. `PendingWriter` exposes only pending-record writes and finalization, with legacy discovery retained in verified
transfer. The old per-function iterator/writer interface was removed. The existing 64 KiB buffer and per-record
position/checksum accounting are unchanged.

The visitor retains the parse result, finishes the writer even after a syntax/shape failure, then checks its deadline.
This makes buffered I/O errors fatal rather than losing them in BufWriter destruction and treating the input as an
ordinary cache miss. A Linux regression injects ENOSPC before a later malformed function; existing tests retain
canonical order independence, malformed-code diagnostics, closed-spool propagation, and cleanup. No input, instruction,
module, or checkpoint format changes.


## Paged function-location indexes

`src/code_store/record_index.rs` keeps up to 16,384 function locations in memory, then migrates them into an owned
`redb` database. The replacement is complete before the old map is released. `record_index/disk.rs` owns the private row
encoding, database, transactions, and cleanup. The pinned dependency is redb 4.4.0 with default features disabled; its
upstream provenance and MIT/Apache-2.0 licensing remain separate from Bifoil. No database is a reusable module or public
artifact.

A vacant-entry guard holds the exclusive index borrow from duplicate checking through spool append, avoiding a second
lookup without reserving an identity after a failed write. Checked canonical reads share one location query while
retaining header fingerprints, actual-byte checksums, and instruction counts.

The database cache is explicitly 4 MiB. Writes use an overlay capped at 4,096 rows and a 1 MiB reservation including
keys and a tree-node allowance; an oversized single key flushes immediately. Pending replacements and deletions remain
visible to reads. A batch opens the table once, commits without crash durability, then releases its overlay. Committed
readers are invalidated before each batch so old transaction roots cannot accumulate. Failed commits retain staged rows
until the failed compiler job unwinds. Module finalization checks the final commit, and empty stores release the
database. Unix unlinks owned temporary files immediately; other platforms remove them after handles close.

Serialization can hold a location snapshot with a 4 MiB reservation. Its immutable store borrow prevents index mutation;
uncached rows still use the database. Nested serialization reuses the outer scope and cleanup releases it on every exit.
The existing canonical-value cache retains its separate bound. Location reuse never bypasses body-byte checksums, header
fingerprints, instruction counts, or the fatal private-storage error channel. Tests force spills, batch and byte
boundaries, deletion/reinsertion, reader invalidation, snapshot fallback, corrupt rows, fatal input propagation, and
owned-file cleanup. Public schemas, module bytes, code scenes, and build identities retain their existing contracts.


## Borrowed initializer signature views

Shared-initializer analysis uses the borrowed maps in src/initialization/signatures.rs. Own declarations take priority;
imported lookups try complete module-name prefixes without allocating another map entry for every reachable function.
Member/local suffixes can resemble nested module names, so a missing function in a longer prefix continues through
shorter prefixes. The view borrows declaration lifetimes and retains ordinary source ownership checks.

Dependency resolution receives a function iterator directly. Its conservative candidate set is populated only when an
indirect-call summary exists; when one does, the existing complete target inventory and sorted set remain available.
Summaries, shared bindings, cycle rejection, deferred errors, and dependency ordering retain their existing roles. The
lookup regression covers own overrides, nested modules, member suffixes, missing names, and component boundaries. These
views remove temporary flattened indexes; completed declaration maps still remain resident.


## Shared callable headers

`src/model.rs` defines the shared callable header and its parameter mutation helper. Return inference, receiver
projection, and generic substitution use copy on write; an unchanged substitution retains the entire header. Lowering
clones only the identifiers and result values that must outlive their borrowed header.

`src/signature_sharing.rs` reuses a function's canonical header across equal export, constructor, overload, and
nominal-method views. Source modules reconcile checked headers before hashing. Artifact input reconciles only after its
original digest and all structural/interface checks succeed. Matching uses complete structural metadata, including
parameter names and flags, so canonical module bytes are unchanged.

Nominal records detach only when a method needs another header allocation. A mismatched interface retains its validation
failure. Regressions compare source and verified-input sharing, exact canonical bytes, detached invalid exports,
independent generic specializations, and nullable normalization. Sharing reduces resident copies; it does not provide a
persistent declaration catalog.


## Metadata reads and output preparation

`CodeStore::header` decodes a `FunctionHeader` containing the signature and initialization/local flags, without
allocating instruction objects. It verifies the actual payload's fingerprint and requested function identity. Legacy
bridge checks use this metadata path; code interpretation and semantic analysis keep their typed-body reads.

`artifact::prepare_json` writes and flushes an owned temporary output while calculating its checksum. The returned
`PreparedJson` retains no analysis or serialization borrows. Dropping it removes only that temporary file. `commit`
checks the deadline before and after the completion callback, then replaces the destination. An expired deadline or
failed receipt update preserves the previous output.

Package preparation writes immutable chunks and prepares its root, or retains the checksum of an unchanged root. Project
builds then release function headers, module catalogs, code spools, and temporary indexes before committing the root.
Standalone module and inline output follow the same preparation/cleanup/commit sequence. This places large cleanup costs
before the final deadline check. Existing chunks remain available under the concurrent-root policy. A deterministic
regression expires the deadline after successful serialization and verifies that the old output survives and the owned
temporary file is removed.


## Grouped linked function headers

`src/function_catalog.rs` retains each completed module's ordered function map through linking. It moves those maps
without rebuilding a global header table or cloning a second list of IDs for package ownership. Duplicate function
identities are checked before entry resolution and shared-initializer planning. Module names and member-name prefixes do
not establish ownership.

Package output borrows one module map directly. Inline output uses an ordered merge with one borrowed cursor per
nonempty module, preserving global canonical function-ID order even when nested module IDs interleave with their
parent's IDs. The bounded module inventory bounds cursor storage. Shared-initializer analysis still receives every
function through the catalog's values iterator.

`src/code_store/function_map.rs` provides one checked serializer for ordinary module maps and the grouped program
catalog. Both retain private byte, instruction-count, identity, and header checks. The obsolete selected-ID serializer
is removed. Regressions exercise nested modules, member ownership, empty groups, duplicate identities, bounded cursor
counts, unchanged canonical bytes, and rejection of changed headers. Completed declaration maps remain resident; this
grouping is not persistent catalog storage.


## Publication header digests

After entry checking and shared-initializer planning, FunctionCatalog::freeze consumes full function records into
grouped IDs and 32-byte HeaderDigest values. The digest covers the existing signature/init/local tuple. This releases
private parameter and type graphs before final hashing and encoding; it retains actual defining-module ownership and
canonical ordering. Keys must match signature identities, and bodies must already have moved to the spool. Deadline
checks remain active during the transition.

StoredFunctions uses a private header-proof interface for full module headers and frozen linked digests. A digest does
not authorize unverified payload reuse: CodeStore still checks actual stored bytes, instruction count, location
fingerprint, and the expected header checksum. Public artifact formats remain unchanged. Regressions verify released
signature allocations, empty and interleaved groups, unchanged canonical bytes, forged digests, corrupt payloads, count
mismatches, and closed storage. The transition shortens metadata lifetime; completed declaration metadata is still
resident during semantic analysis.


## Direct-call captured initialization

single_use_initializers.rs discovers named local helpers and the delayed bindings they write within one complete lexical
body. Target declarations must precede the helper, and every reached forward call requires its target cells to exist.
The inventory in single_use_initializers/call_sites.rs recognizes grouped names, explicit generic applications,
arguments, expression blocks, assignments, interpolation, prose directives, and choice preparation or selected bodies.
Deferred callable bodies remain opaque; publishing or retaining a writer prevents this proof. A do/while closing
condition participates in the body's reference inventory.

Mutable targets receive optional must-write facts independently at each direct call. Protected vals require complete
writes on every normal return and proof that each reached call finds their cells unwritten. Several source sites may
therefore execute on exclusive branches. Overloads retain separate declaration identities and ordinary selection rules;
a nonwriting unused overload does not invalidate the selected initializing overload. Actual second calls, competing
assignments, reads before writes, and incomplete returns remain errors. Failing the bounded mandatory write proof
rejects a protected-val plan instead of silently dropping its initialization facts.

calls.rs evaluates the callable or receiver first, then supplied arguments and their conversions in source order.
Protected-cell and capture checks follow those arguments; arguments that transfer control do not invoke the helper.
Default requirements remain separate and apply only when omitted. Default expressions and bodies keep their selected
order, suspension permissions, and publication barriers. Completed argument values retain their original requirements
when a named callable's metadata is refreshed.

Noncompact helpers retain ordinary frames, parameters, generic bindings, defaults, variadics, result types, returned
readers, and suspension. local_functions/definition.rs shares the original and early definition checker, derives
must-write facts, and refines entry reads. capture_initialization/entry_reads.rs separates the leading default regions
from body entry requirements, while stores, branches, and returns remain in the complete must-write graph. Each caller
scope snapshot receives its own refinement, including alternative branches. Empty optional summaries remove stale cache
entries. Captured-write targets retain lexical slot, owner, and binding identities.

single_use_initializers/forward.rs retains the AST and shared entry scopes only for forward frames that require an early
check. That check truncates caller scopes to the declaration depth. Plan::prepare_definition resets protected-cell flags
in a detached definition environment without changing the actual caller. The canonical digest of the whole checked
function, including its signature, must match its final declaration. Caller-local names, later declarations, transient
narrowing, and argument effects cannot retarget the selected helper or supply a different lexical environment.

Compact expansion requires one source site and a unique, pure, parameterless, nongeneric, nonsuspending declaration.
single_use_initializers/compact.rs permits fixed scalar expressions and selected already-existing named references whose
meaning is independent of the call site. General collection, indexed, member, and constructed expressions retain frames
to preserve lexical inference and reified type arguments. Grouping around a discarded call or its callee preserves the
same statement shape. A consumed compact call leaves exactly one Unit value and clears only that result's empty payload
requirements. Surrounding operands and argument values remain intact. The original helper is still checked before its
capture-free runtime stub is retained.

Assignment receivers, indexes, and map keys evaluate once before the RHS. Compound updates also retain the target's
checked read and type before invoking their RHS. Initializing calls may supply address expressions, values, direct
returns, collection entries, interpolation, and other ordinary operands. Their effects remain subject to the existing
short-circuit, safe-access, conversion, overflow, and suspension rules. Source-free checkpoints cover retained
assignment addresses and partially evaluated expressions.

Loop-body calls use ordinary loop flow to reject writes reaching a backedge. A protected outer val can be initialized
before an unconditional or labeled exit, or in a post-test body whose condition is false. Unwritten continue paths do
not invent writes. While-condition calls require a separate repetition proof and remain conservative for protected vals;
the entry snapshot already includes the first condition evaluation. call_sites.rs preserves this condition context
through nested expressions. Fresh declarations inside a loop receive their own discovery pass and runtime binding on
each iteration. Mutable helpers remain available in both loop bodies and conditions.

choices/preparation.rs retains monotonic initialization facts for each prepared option. Selecting an option proves its
preparation ran, even when visibility or once history can otherwise skip it. Facts contain exact lexical identities and
newly initialized cells; applying them preserves current values, dependencies, callables, aliases, and type narrowings
rather than restoring an old scope snapshot. Other options and fallbacks do not inherit facts they do not imply. A
consumed once option without fallback has no normal continuation; an explicit fallback remains a possible unwritten
path. Prose and option-label interpolation retain their non-suspending restriction.

Native and source-free regressions cover value and Unit results, overload selection, generic/default calls, lexical
captures, publication, competing writes, argument transfers, conditional paths, loop exits, and retained readers across
checkpoints. These proofs are compiler analysis metadata; they change no artifact or checkpoint format.


## Shared empty parameter and type storage

src/type_storage.rs provides constant-size sharing for empty parameter/type slices and common Int, Unit, and String
result shapes. Source signature construction and typed metadata decoding use the same helpers; nonempty parameter names,
flags, and type values remain intact. Other result shapes retain ordinary ownership and inner sharing. SignatureData::ty
reads the current header every time, so inference or specialization cannot reuse a stale derived type.

SignatureData::params_mut does not detach an empty list: no elements can be changed through its fixed-size slice API.
Nonempty lists and result-shape edits retain normal copy-on-write isolation. Decoding still delegates every element and
result to the typed deserializer; invalid shapes are rejected. Owned complex decoded results move directly into their
Arc, avoiding an extra clone. Public serialization, header fingerprints, record validation, and code-store lifetimes are
unchanged. Tests cover detached result edits, later header changes, canonical roundtrips, variadic/scene flags, and
malformed inputs. This storage is compiler metadata, not shared mutable story state.


## Buffered canonical fingerprints

artifact/buffered_hash.rs batches small serializer writes into a fixed 1 KiB buffer before feeding SHA-256. Larger
fragments flush pending bytes and feed the hash directly. HashWriter checks the existing logical-byte budget before
accepting each fragment; buffered bytes cannot bypass that guard. Flush and finalization retain all pending bytes
exactly once. No full serialized object is retained. Independent SHA-256 comparisons cover empty input, varied partition
boundaries, escaped Unicode JSON, and repeated flushes. Canonical artifacts, private spool verification, and publication
contracts are unchanged.


## Selective decoding of validated pending bodies

SummaryContext::requires_body owns the ordinary shared-initializer analysis policy. Pending transfer decodes requested
bodies plus every body containing a legacy call; scene, startup, and suspending bodies without legacy calls keep their
already validated canonical representation. A borrowed operation-tag scan handles escaped JSON tags and ignores
unrelated string contents. It is not input validation: typed parsing, complete-module checks, and digest validation
already precede pending transfer.

Every transfer still reads actual private spool bytes and checks their checksum, instruction count, and expected header
before selecting this path. Legacy calls retain their typed target and original span through ordinary collection.
Canonical output and summary results remain identical. Regressions cover selective inspection, real body retention,
legacy fallback, escaped tags, and fatal changed-byte/header failures. No new cache or retained body map is introduced.


## Definite assignment after nonempty traversals

For-loop exits now merge reachable body, break, and continue states. A zero-iteration state is included only when
emptiness remains possible. This retains completed writes and retired capture requirements after guaranteed traversal,
including outer labeled continues. Missing writes on any exit still prevent reads, and the existing at-most-once checks
still reject repeated outer-val assignments.

loops/nonempty.rs uses the actual captured iterable type and conservative structural evidence. Ascending IntRange values
are nonempty; positive steps preserve a proven first value, and constant descending bounds can establish one. Unknown
IntProgression values remain conservative. Declared binding types are invariant under RHS effects; later replacement or
narrowing cannot describe a progression already captured by the step operator. This analysis does not alter progression
membership, nominal type identity, or the open descending-bound policy. Native and source-free cases cover retained
iteration closures, old callback overwrites, stable guards, and retired keys with reused spellings.


## Immutable stored progression evidence

loops/nonempty.rs retains construction evidence under lexical binding identities for initialized immutable declarations.
Aliases, captured values, and safe universal or nullable annotations preserve that evidence. A known stored progression
can therefore establish a first iteration without re-reading later binding state. Reused names, mutable bindings, and
unknown parameters cannot acquire another declaration's evidence. Normal type and null checks remain mandatory before
iteration; value evidence does not turn Any into an iterable type.

Only immutable declaration initializers record these facts. Checking a deferred writer body does not initialize its
captured target or publish a value proof into another call. The evidence remains analysis metadata and changes neither
the artifact format nor progression membership. Source-free regressions cover escaping readers, suspension before
traversal, reuse, narrowing, and rejected mutable or unknown aliases.


## Nonempty text and collection traversal

loops/nonempty.rs also recognizes nonempty String, Char, and collection literals. Formatting or concatenation preserves
known nonempty text. Fresh mutable collection literals have a first element at traversal capture. Their readonly aliases
do not acquire a lasting value proof, because container contents can still change through another reference.

Stored facts require fixed container contents after conversion. An immutable family copy preserves a known first
element; a conversion into a mutable family discards this evidence. Erasing a mutable container to Any cannot freeze it.
Immutable containers may still contain mutable objects, because changing those objects does not alter the container
count. Lexical identities preserve evidence through immutable aliases, captures, and safe narrowing. Ordinary iterable
and null checks remain authoritative. Empty literals, unknown inputs, and reused names retain conservative
initialization checks.
