# Repository Guidelines

## Project Structure & Module Organization

This is a Rust workspace for a research OS. `cosir/` contains the executable, platform-independent model; `kernel/` implements x86_64 mechanisms; `prima-ostd/` owns privileged hardware support. Shared syscall definitions live in `prima-abi/`, native user APIs in `prima-runtime/`, and user programs and services in `prima-user/`. `prima-initgen/` builds boot inputs, while `xtask/` provides build and validation commands. Formal models are in `lean/`; plans, specifications, and evidence reports are under `docs/superpowers/`. Rust tests sit beside their modules and in crate-level `tests/` directories. Boot images are generated artifacts, not source assets.

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
