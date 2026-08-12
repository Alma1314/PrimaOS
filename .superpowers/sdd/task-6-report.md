# Task 6 Report

## Status

Completed and committed.

- Base HEAD: `509fdd9`
- Commit: `b485d57b91e74d686e5ff2e51d4dd53ed5635051`
- Commit subject: `docs: calibrate SPEC-014 and portability evidence claims`
- Tracked working tree after commit: clean

## Changes

- Calibrated the harness output to `PASS model-test memseg page-walk dual view` and `PASS model-test memseg W6/W7 checks`.
- Classified Bootstrap projection and the MemorySegment host four-level index tree as `REFINEMENT-DESIGN`; classified Rust executable-model tests as `MODEL-TEST`.
- Limited W8 evidence to the unmap entry-page guard and recorded uncovered non-leaf permission composition, page-table-page physical identity, TLB behavior, multicore behavior, real CR3/PTE behavior, and atomic update windows.
- Downgraded cross-architecture applicability to a candidate hypothesis and GPU/NPU/distributed decompositions to mappings requiring frozen observable semantics and `COMPAT-TEST`; removed unconditional H1 support.
- Added research stop conditions and fixed the next vertical slice as `COW/page fault → handler → exc_resume → TLB → Linux trace`.
- Marked `ISSUE-MEM-006` Fixed.

## Verification

- `git diff --check`: passed; only the existing Git LF-to-CRLF advisory for `cosir/src/main.rs` was printed.
- `cargo test -p cosir`: passed, 69 tests; 0 failed, 0 ignored.
- `cargo run -p cosir`: passed. Final calibrated lines:
  - `PASS model-test memseg page-walk dual view`
  - `PASS model-test memseg W6/W7 checks`
- Output inspection: no unqualified `refinement PASS` and no complete W8 claim.
- Scope inspection: commit contains exactly the four requested files.

## Self-review

No code behavior or implementation scope was expanded. The changes only calibrate evidence labels, research claims, uncovered boundaries, stop conditions, and the next slice. Cross-document terminology is consistent with `MODEL-TEST`, `REFINEMENT-DESIGN`, and `COMPAT-TEST`.

## Important findings follow-up

- Recast the Prima OS claims about primitive-category exhaustion, second-architecture stability, SMP invariance, and distributed/multi-medium completeness as falsifiable candidate hypotheses. Decisions now explicitly wait for a second architecture or SMP/distributed implementation, frozen observable semantics, `COMPAT-TEST`, and relevant fault injection; counterexamples require revising the mapping or POSIR.
- Retitled COSIR §20.5 as a host four-level index-tree ↔ vspace refinement design. The tree and `Pte` fields are explicitly an x86_64-inspired candidate encoding, not real CR3/page-table/PTE state; all previously listed uncovered boundaries remain recorded.
- Residual strong-claim search covered `穷举`, `不会增加`, `无新增原语`, `语义不变`, `逻辑不变`, `不引入新的 OS API 语义维度`, and `不需要新增 OS API`; no unqualified target claims remain.
- Follow-up verification on 2026-08-13: `git diff --check` passed with no output; `cargo test -p cosir` passed 69 tests with 0 failures/ignored; `cargo run -p cosir` exited successfully and retained the two calibrated `PASS model-test` lines.
