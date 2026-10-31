# Working on Bifoil Compiler

Read [README.md](README.md) for this repository's intended role and current status. This is a standalone repository
root,
not a dependency on an adjacent checkout. Parent foundation instructions apply when present; this file preserves the
essential context for a future independent clone. The user's latest instructions take precedence.

## Project direction

The native Rust compiler and reusable language-analysis components for Bifoil.

Use bifoil-language as the specification. Runtime behavior belongs in bifoil-runtime; editor presentation belongs in
bifoil-editors. The current development profile targets extended ChoiceScript through bifoil-vm.v2.

The foundation's broader goal is a FOSS ecosystem comprising a ChoiceScript-compatible superset language, shared
language services, editor integrations, a toolchain, libraries, and potentially a standalone studio. The first compiler
slice is implemented and full-spec implementation is authorized. README.md and RESOURCES.md describe tested
paths and measured limits. Human readability and maintainability are explicit requirements; use the module
ownership described in ARCHITECTURE.md. Do not treat partial coverage as full steering conformance or release
authorization. Existing Bifoil
ChoiceScript code remains in its own repository.

## Shared preferences

- Use Bifoil Foundation branding and **chinay**, lowercase, as the public maintainer identity. Keep local account names
  and personal filesystem paths out of public documentation and identifiers.
- Preserve independence from Choice of Games and upstream ChoiceScript, with separate licenses and provenance for
  third-party material. Do not invent legal status or transfer copyright to an assumed entity.
- Use pnpm for TypeScript work and the Gradle wrapper for Kotlin when those stacks are selected. Use system-installed
  **uv** for Python. Prefer idiomatic code, including multiline Kotlin statements.
- Where Prettier is adopted, preserve the founder's intentional `printWidth: 120` and use defaults otherwise. Add
  plugins
  only for formats the repository actually contains.
- Keep builds independent of sibling checkouts and chat workspaces. Reuse components through explicit, versioned
  interfaces or dependencies rather than silently copying or duplicating semantics.
- Use `git mv` for tracked moves, preserve unrelated edits, and follow the user's commit/signing workflow.
- Keep public guides separate from regression inputs and private reports. Use original minimal examples; do not copy
  externally supplied game sources or private corpus identities into the repository.
- Keep local agent records under ignored `.codex/` and development reports under ignored `.bifoil/development/`.
- Limit generated artifacts and large downloads to the task's needs. Validate relevant behavior and report actual
  results without claiming that a scaffold, build, or one-platform check proves release readiness.

The foundation's existing release and acceptance deferrals still apply unless the user changes their scope. Creating
this root does not publish it, select a license, authorize a release, or begin implementation of unspecified features.
