# Task 6 Report — PrimaOS slice C terminal task (CR0.WP observation + QEMU gate + docs)

## Status

Completed and committed.

- Base HEAD: `ff66443`
- Commit: `5a6d823`
- Commit subject (exact brief message): `docs+feat: observe CR0.WP; record SPEC row for PF error-code semantics`
- Files committed: `kernel/src/main.rs`, `Prima OS 实现阶段.md`
- Tracked working tree after commit: clean (report file below is the un-committed SDD handoff; like prior slices, it is not part of the commit scope staged by the brief).

## Scope / what was done

1. **Kernel informational note (Task 2 of brief's code step):** added, right after the C-stage `protocol::page_fault_ready(...)` call in `kernel/src/main.rs`, a NON-PRIMA `[pf] cr0.wp=` serial line reading `kernel::cpu::read_cr0_wp()`:

```rust
// 观测：读 CR0.WP，看 supervisor 写只读页在当前环境是否故障。
// 仅供日志/未来自建页表证实写保护现场；不硬断言、不加 PRIMA 事件，
// 也不在 CR3 缺一个 present-只读页时伪造现场。
let wp = kernel::cpu::read_cr0_wp();
serial::write_bytes(b"[pf] cr0.wp=");
serial::write_bytes(if wp { b"1" } else { b"0" });
serial::write_bytes(b"\r\n");
```

No new PRIMA event, no new sequence item (PRIMA seq still ends at 10), no hard write-protection boot assertion. `serial::write_bytes`/`flush` call form matches the existing usage in that file; the note is not `PRIMA|`-prefixed so `xtask validate_output` treats it as firmware noise (same as SeaBIOS), verified by the gate passing.

2. **QEMU BIOS gate:** `cargo xtask test` — run=1/2, **six PASS**, **exit 0** (see Verification).
3. **Docs:** added tracker SPEC row (SPEC-041) + synced the slice-C enumeration/presentline in `Prima OS 实现阶段.md`; whitepaper `Prima OS.md` reviewed — it does not enumerate a #PF-capture-depth/claims line to sync (its #PF content is model-level ExceptionKind / future-vertical-slice statements at the "已定义模型" level, not kernel-validation capture-depth), so the presentline sync correctly lives in the tracker.

## Real QEMU command, exit, pass count

The gate invoked by the brief is `cargo xtask test` (builds kernel + BIOS/UEFI images, then runs the BIOS image twice under QEMU with structured serial capture + exit-code 33 assertion).

- Command run (authoritative, on the final committed code): `cargo xtask test`
- Process exit code: **0**
- `PASS build kernel`, `PASS image bios`, `PASS image uefi`, `PASS qemu bios run=1`, `PASS qemu bios run=2`, `PASS stage1 boot slice` — **six PASS total**
- Evidence: `cargo test --workspace` 303/303 green (cosir 145, cosir_state 40, lean_backtrace 2, kernel 74, xtask 42); `cargo clippy --workspace --all --no-deps -- -D warnings` clean; `cargo fmt --all --check` clean.

To obtain the raw real-serial `cr0.wp` value (closure obligation (a) — the observation must be EXECUTED in QEMU, since a CPL3 host cannot read CR0), I also re-ran QEMU directly with the xtask's exact image+args and captured the raw serial/stdout:

- Direct QEMU: `qemu-system-x86_64.exe` `-machine pc-i440fx-10.0 -accel tcg,thread=single -cpu qemu64 -smp 1 -m 128M -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04 -drive format=raw,file=target/prima-os/prima-os-bios.img -boot order=c,strict=on`
- QEMU exit code: **33** (SUCCESS, matches xtask `SUCCESS_EXIT_CODE`)

Raw real-boot captures from that serial log:

```
[pf] cr0.wp=1
PRIMA|v=1|seq=8|event=page_fault_ready|status=ok|data=addr=0x40000;err=0x0;class=not_present
```

## Closure obligations — evidence

- (a) **Real cr0.wp datum:** the real kernel printed **`[pf] cr0.wp=1`** on the real QEMU boot. `wp=1` ⇒ WP set ⇒ supervisor write to a read-only page would fault in this environment; it is reported as DATA (note says 1, not 0 — either is acceptable), and I did not spin.
- (b) **Wire round-trip on a REAL boot (both merged sides):** the `page_fault_ready` event on the real boot was exactly `addr=0x40000;err=0x0;class=not_present`, produced by the kernel writer (Task 4) and re-parsed+asserted by the xtask validator (Task 5). The six-PASS / exit-0 `cargo xtask test` proves the round-trip.
- (c) **No host live-CR0-read stability test reinstated:** none added — this is documented infeasible at CPL3; the `[pf] cr0.wp=1` observation is the QEMU-side counterpart.

I did NOT fabricate a write-protection live capture: CR0.WP was 1, but no present-but-RO (to supervisor write) VA was triggerable on the bootloader CR3, so no `WriteProtection` class occurred in the boot; that boundary is honestly recorded as best-effort future self-built-page-table work rather than asserted.

## SPEC row / docs text added

Evidence-index row added to `Prima OS 实现阶段.md` (after SPEC-040), entry `SPEC-041` — `真实内核 MODEL-TEST + QEMU 集成断言`, artifacts `kernel/src/idt.rs` (`page_fault_entry` stores error code / `last_fault_err()`), `kernel/src/fault.rs` (`classify` pure decoder), `kernel/src/cpu.rs` (`read_cr0_wp`/`wp_of`), `kernel/src/main.rs` (no `translate` self-assert, PF trigger/report, WP info line), `kernel/src/protocol.rs`, `xtask/src/protocol.rs`, date `2026-09-05`, supported checks `S1-D4`. It documents error-code capture (`last_fault_err`), the pure decoder unit table (`err=0x0→not_present`, `P1W1→WriteProtection`, else `Other`, incl. the absent-write anti-confusion), the `read_cr0_wp()` gate, and the best-effort WP boundary (not claimed when untriggerable on the bootloader CR3), plus the measured 2026-09-05 acceptance (303/303 workspace, clippy/fmt clean, `cargo xtask test` run=1/2 six PASS exit 0), and ends with the standard non-claims list.

The slice-C presentline (line ~57) and the phase-1 summary enumeration (line ~60) were updated to reference SPEC-041 and the error-code/classify/CR0.WP deepening. The SPEC-041 row keeps the "DOC"-style conventions used by SPEC-040 and states exact measured numbers (no fabricated run values).

## Self-review

- Code change is minimal, non-invasive, and matches the brief's canonical snippet; clippy/fmt/workspace/QEMU gates all clean on the final committed tree.
- The informational line does not touch PRIMA event sequence (seq ends at 10) and adds no hard assertion; the WP observation is honest DATA and the best-effort boundary is recorded, satisfying the Global Constraint against fabricating a capture.
- Report pass count / exit / `cr0.wp=1` are the actual measured values from the runs above.
- Tracker/SPEC wording follows the file's existing Chinese evidence-format conventions; I limited edits to the SPEC/record lines and did not fabricate whitepaper capture-depth claims that do not exist there.

## Concerns

- The `.superpowers/sdd/task-6-report.md` path previously held the *memseg*-slice task-6 report (a different, already-committed slice that shares the same numbered report filename). Per the brief's fixed report path I wrote this PrimaOS report to that path; I did NOT stage it, so the previously committed memseg file's historical copy is preserved in git. If the reviewer expects a distinct filename to avoid overwriting that working-tree copy, that is a rename/cleanup decision left to the runner.
- `wp=1` here only indicates write-protection is *enabled*; it does not themselves produce a `WriteProtection` event because the active bootloader CR3 shipped no present-but-RO-and-written VA during this minimal boot. This is expected and is the documented best-effort boundary, not a gap.
