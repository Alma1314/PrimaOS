---
id: repository-instructions
kind: agent-instructions
authority: operational
status: maintained
lifecycle: current
updated: 2026-10-24
---

# Repository Guidelines

## Project Structure & Module Organization

This is a Rust workspace for a research OS. `cosir/` contains the executable, platform-independent model; `kernel/` implements x86_64 mechanisms; `prima-ostd/` owns privileged hardware support. Shared syscall definitions live in `prima-abi/`, native user APIs in `prima-runtime/`, and user programs and services in `prima-user/`. `prima-initgen/` builds boot inputs, while `xtask/` provides build and validation commands. Formal models are in `lean/`; plans, specifications, and evidence reports are under `docs/superpowers/`. Rust tests sit beside their modules and in crate-level `tests/` directories. Boot images are generated artifacts, not source assets.

## How to Read the Documentation

Start at `docs/README.md`, then use `docs/architecture/ai-retrieval.md` and the topic routes in `docs/architecture/topics.json` to select a bounded context package. `llms.md` is a lightweight routing entry, not another source of project status.

Authority depends on the question:

- `Prima OS.md`: architecture, research hypotheses, stage definitions and design boundaries.
- `COSIR 原语规范.md`: abstract operation semantics, errors and invariants (the historical file name does not make it a machine ABI specification).
- `Prima OS 实现阶段.md`: sole current stage, exit checkboxes, active blockers and current evidence pointers.
- `docs/superpowers/specs/`: expected behavior; verify proposal/frozen status before treating a document as a contract.
- `docs/superpowers/plans/`: historical execution plans, not current implementation status.
- `docs/superpowers/reports/`: evidence within each report's explicit scope. Follow discovery/remediation/review chains; newer dates alone do not establish supersession.
- `docs/superpowers/compat/`: scoped API compatibility and symbol-to-syscall ledgers, not complete POSIX/Linux compatibility.
- `lean/README.md`: authoritative mapping between the modeled Lean and cosir fields, with explicit omissions.
- `docs/architecture/`: explanatory guides and retrieval routes; these do not override contracts or sign stages off.

Before changing behavior, read the relevant spec, plan and report chain, including failures and limitations. If implementation and contract differ, record the deviation rather than silently making implementation normative. Changes to phase status or evidence must update the stage tracker and relevant evidence indexes.

New documents belong under `specs/`, `plans/`, `reports/`, `compat/`, or `architecture/`; keep the docs root for navigation and generated machine catalogs. Follow `docs/superpowers/specs/2026-10-23-documentation-system-design.md` and the templates in `docs/architecture/templates/`. Preserve contracts, decisions, failures and scoped evidence. Remove redundant or obsolete process/explanatory documents only through docs/lifecycle.json with retained replacements, reserved identities and verifiable Git recovery; use portable relative Markdown links, not wiki links. New documents require explicit lifecycle metadata; document lifecycle never implies implementation or approval status.

For documentation changes, run:

```sh
python3 scripts/docs.py self-test
python3 scripts/docs.py build
python3 scripts/docs.py check
python3 scripts/docs.py check --base <pre-change-commit>
```

`docs/catalog.json` and `docs/architecture/catalog.md` are generated; do not hand-edit them. `topics.json` and `docs/lifecycle.json` are curated. The catalog uses schema_version 2; default search excludes deprecated documents but retains historical evidence. `inspect <path>` finds lifecycle, inbound links and retired-path recovery. The base check enforces metadata on new documents and recovery registration for deletions. The catalog includes heading line ranges, hashes and identifier mentions; mentions are not unique document IDs or approval evidence. `python3 scripts/docs.py topic <id>` and `search '<terms>' --limit 5 --json` support bounded offline retrieval.

## Build, Test, and Development Commands

Use the pinned nightly toolchain in `rust-toolchain.toml`. From the repository root:

- `cargo xtask build` builds the boot images.
- `cargo xtask run` starts the BIOS image in interactive QEMU.
- `cargo test --workspace` runs Rust unit and integration tests.
- `cargo xtask test` builds images and runs the BIOS QEMU acceptance checks twice.
- `cargo xtask lint` checks formatting, Clippy warnings, the bare-metal kernel build, and the kernel's zero-`unsafe` rule.
- `cargo xtask tcb` reports trusted-code and kernel coupling metrics.

The QEMU commands require QEMU and the configured Rust targets on the host.

## Coding Style & Naming Conventions

Use `cargo fmt --all` and four-space Rust indentation. Follow Rust naming conventions: `snake_case` for modules and functions, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Keep `kernel/src` free of `unsafe`; place necessary privileged hardware operations in `prima-ostd/` and document their invariants. Treat `prima-abi/` as experimental rather than a stable external interface.

## Testing Guidelines

Add focused `#[test]` cases near changed Rust behavior; name them for the condition and expected result. For boot or protocol changes, also update the relevant `xtask` QEMU trace checks. Run `cargo test --workspace` and `cargo xtask lint` before submission, plus `cargo xtask test` when the boot path changes. There is no stated numeric coverage target. Record evidence at its actual scope: host tests, QEMU execution, and Lean proofs establish different claims.

## Commits & Pull Requests

Older stage commits use subjects such as `feat(stage4): ... (SPEC-114)`; the latest commits are short numbered subjects, so history has no single consistent convention. Prefer a descriptive, scoped subject and include a SPEC identifier when applicable. In pull requests, summarize the behavior, link the issue or specification, list commands and results, and update `Prima OS 实现阶段.md` plus relevant reports when evidence or phase status changes. Include screenshots only for visual changes.
