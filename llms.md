---
id: ai-entry
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-24
---

# PrimaOS

> Research OS: object-capability mechanisms, native runtime, user-space OS personalities.
> Read the same source documents used by humans. Navigation is not implementation evidence.

## Start here
- [Repository instructions](AGENTS.md): constraints and validation commands.
- [Documentation center](docs/README.md): task-based routing and authority rules.
- [AI retrieval protocol](docs/architecture/ai-retrieval.md): bounded reading and claim provenance.
- [Current status](Prima%20OS%20实现阶段.md): sole current stage/exit/blocker summary.

## Contracts and evidence
- [Architecture](Prima%20OS.md): research goals, layers, boundaries, stage definitions.
- [Abstract semantics](COSIR%20原语规范.md): states, operations, failures, invariants.
- [Subsystem topics](docs/architecture/topics/README.md): contract → report chain → code → tests → limits.
- [Evidence rules](docs/architecture/evidence.md): host, Lean, QEMU, compatibility, sign-off scopes.
- [Lean mapping](lean/README.md): exact model mapping and unmodeled fields.
- [Compatibility ledger](docs/superpowers/compat/compat-state.md): scoped API status.
- [Symbol trace](docs/superpowers/compat/symbol-trace.md): public symbols to service/syscall routes.

## Machine-readable retrieval
- [Topic routes](docs/architecture/topics.json): curated sources/code/tests by topic id.
- [Full catalog](docs/catalog.json): paths, roles, hashes, headings, line ranges, links, mentioned ids.
- Run: python3 scripts/docs.py topic process
- Run: python3 scripts/docs.py search 'fork' --kind report --limit 5 --json
- [Document lifecycle](docs/lifecycle.json): retirement reasons, replacements, reserved IDs and Git recovery.
- Run: python3 scripts/docs.py inspect <repository-relative-path>
- Run: python3 scripts/docs.py check --base <pre-change-commit>

Dates and search scores do not establish approval. Preserve failures and unresolved limits.
Do not equate source compatibility with unmodified Linux binary compatibility.
