# PF error-code capture + write-protect semantics (slice C continuation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the vector-14 #PF capture from "CR2 + skip" to a structured fault classifier — capture and decode the hardware #PF error code into a disjoint semantics (`NotPresent` vs `WriteProtection` vs `Other`), add a `read_cr0_wp()` gate, and generalize the `page_fault_ready` event to carry `err` + `class`.

**Architecture:** Pure decoder lives in new `kernel/src/fault.rs` (no asm → fully host-unit-testable). `page_fault_entry` in `idt.rs` saves the hardware error code (currently popped-and-dropped) into a new static read by `last_fault_err()`. `cpu.rs` gains a one-instruction `read_cr0_wp()` (the real total-switch for whether a supervisor write to a read-only page faults). Event `data` changes from `not_mapped=<0|1>` to `addr=<hex>;err=<hex>;class=<name>`; xtask re-parses and asserts.

**Tech Stack:** Rust `no_std` kernel (nightly 2026-07-30 toolchain), `bootloader_api`, inline `asm!`/`naked_asm!`, `cargo xtask test` driving QEMU (BIOS, run=1/2, six PASS). Kernel lib crate modules declared in `kernel/src/lib.rs`; each `.rs` there inherits `no_std` from the lib root (no per-file `#![no_std]` needed).

## Global Constraints

Copy verbatim from the spec `docs/superpowers/specs/2026-09-05-pf-error-code-semantics-design.md`:
- Not building a leaf-PTE enumeration. `translate` still only walks 4KiB (huge→`PageNotMapped`). Not a deliverable.
- The supervisor-write-protection gate is **CR0.WP** (bit 16), not "finding a RO leaf". `mov <reg>, cr0` does not read memory/flags.
- Write-protection real field is **best-effort**, gated on WP. Only claim/semantics-conclude on real `present∧write` when WP=1 AND a translate-present-but-readonly VA is triggerable; otherwise record the boundary (no self-built PT, no PTE mutation).
- Do NOT claim: self-built page tables, changing PTEs, feeding the fault to `exc_resume`, ring3/user fault delivery, IRQ/PIC full dispatch, `KERNEL-TRACE`, `REFINEMENT-PROOF`.
- The #PF error code must be saved **before** the entry pops it and **before** the `FAULT_SKIP_LEN` return-address fix. Entry stack contract: after the entry's existing six 8-byte callee-saved `push`es, the CPU-pushed error code sits at `[rsp+48]`, return rip at `[rsp+56]` (unchanged from prior slice).
- `#![deny(unsafe_op_in_unsafe_fn)]` is active on the kernel lib crate.
- Wire `class` names (exact strings): `not_present`, `write_protection`, `other`. Anti-confusion unit rule to pin: a **write to an absent page** yields `P=0, W/R=1` → `NotPresent`, never `WriteProtection`.

---

### Task 1: `FaultClass` decoder + wire name — pure module + unit table — `kernel/src/fault.rs`

**Files:**
- Create: `kernel/src/fault.rs` (impl + inline `#[cfg(test)] mod`)
- Modify: `kernel/src/lib.rs` (register `pub mod fault;`)

**Interfaces:**
- Produces (later tasks consume):
  - `pub enum FaultClass { NotPresent, WriteProtection, Other }` (derive `Debug, Clone, Copy, PartialEq, Eq`)
  - `pub const fn as_str(self) -> &'static str` on `FaultClass` (`not_present`/`write_protection`/`other`)
  - `pub fn classify(err: u64) -> FaultClass`  (u64 because CPU pushes a zero-extended 64-bit error code; low bits meaningful)

- [ ] **Step 1: Write the module with a deliberately-stubbed `classify` (compiles, turns red)**

Create `kernel/src/fault.rs`:

```rust
//! 纯 #PF 语义分类模块：无 asm、无真实现场依赖，唯一分类语义宿主，供 host 单测钉住。
//! 与 `idt.rs`（error-code 捕获）分离：本模块只负责把已捕获的 error code 译成互斥类别。

/// 一次 #PF 的语义分类（互斥）。由 x86_64 #PF error code 译码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultClass {
    /// P=0：页不存在 → 真缺页（demand-paging 候选，非 COW 写保护）。
    NotPresent,
    /// P=1 且写意图：写保护 / 保护违规 → COW 处置候选。
    WriteProtection,
    /// P=1 且非写访问、或含 RSVD/PK/取指等其它置位；本 slice 不展开处置语义。
    Other,
}

impl FaultClass {
    /// wire 上的小写类别名。
    pub const fn as_str(self) -> &'static str {
        match self {
            FaultClass::NotPresent => "not_present",
            FaultClass::WriteProtection => "write_protection",
            FaultClass::Other => "other",
        }
    }
}

/// #PF error code 的关切位（Intel SDM §4.7 / 表 4-27）。
pub const PF_P: u64 = 1 << 0; // present; 0 => page not present; 1 => protection/permission
pub const PF_W: u64 = 1 << 1; // write access
pub const PF_US: u64 = 1 << 2; // user/supervisor (CPL3 source)
pub const PF_RSVD: u64 = 1 << 3; // reserved-bit violation in a paging-structure entry
pub const PF_ID: u64 = 1 << 4; // instruction fetch
pub const PF_PK: u64 = 1 << 5; // protection-key violation

/// 把 #PF error code（CPU 零扩展压栈 64 位）译成互斥分类。
///
/// 优先级：RSVD/PK/取指 等"页表/机器格式"异常 → `Other`；
/// 之后 `P=0` → `NotPresent`（即便 W/R=1，写 absent 页仍是缺页，不是写保护）；
/// 再 `P=1 && W/R=1` → `WriteProtection`；余下（present 读保护等）→ `Other`。
pub fn classify(err: u64) -> FaultClass {
    // 【stub——本步先返回 Other 使单测转红，下一步填入真判定】
    let _ = err;
    let _ = (PF_P, PF_W, PF_RSVD, PF_PK, PF_ID, PF_US);
    FaultClass::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_read_is_not_present() {
        // ring0 supervisor read of an absent page: P=0, W/R=0, U/S=0 -> 0x0
        assert_eq!(classify(0x0), FaultClass::NotPresent);
    }

    #[test]
    fn write_to_absent_page_is_not_present_not_write_protection() {
        // 反混淆关键：写一个 absent 页 -> P=0, W/R=1 -> 依然是缺页，不是写保护。
        assert_eq!(classify(PF_W), FaultClass::NotPresent);
    }

    #[test]
    fn write_protection_is_present_plus_write() {
        // a present(read-only) page written: P=1, W/R=1
        let err = PF_P | PF_W;
        assert_eq!(classify(err), FaultClass::WriteProtection);
    }

    #[test]
    fn present_read_protection_is_other() {
        // present but read fault (not write): P=1, W/R=0 -> not write-protection, Other.
        assert_eq!(classify(PF_P), FaultClass::Other);
    }

    #[test]
    fn rsvd_wins_over_present_write() {
        assert_eq!(classify(PF_P | PF_W | PF_RSVD), FaultClass::Other);
    }

    #[test]
    fn pk_wins_over_present_write() {
        assert_eq!(classify(PF_P | PF_W | PF_PK), FaultClass::Other);
    }

    #[test]
    fn user_write_protection_still_write_protection() {
        // U/S=1 does not change the P+W test.
        assert_eq!(classify(PF_P | PF_W | PF_US), FaultClass::WriteProtection);
    }

    #[test]
    fn instruction_fetch_format_error_is_other() {
        assert_eq!(classify(PF_P | PF_ID), FaultClass::Other);
    }

    #[test]
    fn as_str_names_wire_lowercase() {
        assert_eq!(FaultClass::NotPresent.as_str(), "not_present");
        assert_eq!(FaultClass::WriteProtection.as_str(), "write_protection");
        assert_eq!(FaultClass::Other.as_str(), "other");
    }
}
```

- [ ] **Step 2: Register module in `kernel/src/lib.rs`**

```rust
pub mod cpu;
pub mod fault;
pub mod gdt;
```

- [ ] **Step 3: Run test — expect RED**

Run: `cargo test -p kernel fault`
Expected: compiles; the assertion tests FAIL where a real classification is expected (`absent_read_is_not_present` and friends — stub returns `Other`). `as_str_names_wire_lowercase` passes.

- [ ] **Step 4: Fill the real `classify` body (turn green)**

Replace the stub `classify` body (the `let _ = ...` lines + `FaultClass::Other`) with the real decode:

```rust
pub fn classify(err: u64) -> FaultClass {
    let present = err & PF_P != 0;
    let write = err & PF_W != 0;
    let format_err = err & (PF_RSVD | PF_PK | PF_ID) != 0;
    if format_err {
        FaultClass::Other
    } else if !present {
        FaultClass::NotPresent
    } else if write {
        FaultClass::WriteProtection
    } else {
        FaultClass::Other
    }
}
```

- [ ] **Step 5: Run tests — expect GREEN**

Run: `cargo test -p kernel fault`
Expected: all 9 `fault::tests` PASS.

- [ ] **Step 6: clippy + fmt gate**

Run: `cargo clippy -p kernel --all --no-deps -- -D warnings && cargo fmt -p kernel --check`
Expected: no warnings; no fmt diff.

- [ ] **Step 7: Commit**

```bash
git add kernel/src/fault.rs kernel/src/lib.rs
git commit -m "feat(kernel): pure #PF error-code classifier (FaultClass) with unit table"
```

---

### Task 2: capture the #PF error code in `page_fault_entry` — `kernel/src/idt.rs`

**Files:**
- Modify: `kernel/src/idt.rs` (new `static FAULT_ERROR`, `last_fault_err()`, entry asm saves err at `[rsp+48]`)

**Interfaces:**
- Consumes: nothing from Task 1. Preserves existing entry stack layout.
- Produces: `pub fn last_fault_err() -> u64` (mirror of `fault_addr()`). After `trigger_page_fault`, reads the CPU-pushed #PF error code (zero-extended u64; `0` for the absent-read probe). Does not disturb `fault_addr()`, `trigger_page_fault`, `FAULT_SKIP_LEN==3`.

- [ ] **Step 1: Write the failing test** — append to the existing `#[cfg(test)] mod tests` in `kernel/src/idt.rs`:

```rust
#[test]
fn last_fault_err_defaults_to_zero() {
    assert_eq!(FAULT_ERROR.load(Ordering::Relaxed), 0);
    assert_eq!(last_fault_err(), 0);
}
```

The write side lives in the naked entry and is not host-drivable (calling the naked entry as a function is UB). Its real semantics are asserted through the QEMU wire path in Task 5 (`page_fault_ready ... err=0x0` on the absent-read probe) and through the decoder unit table in Task 1. The default-0 read is the host-double-bar safety that the getter exposes the right storage.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kernel sh last_fault_err_defaults_to_zero`
Expected: FAIL — `last_fault_err` undefined (and `FAULT_ERROR` unresolved) — compile error.

- [ ] **Step 3: Add static + getter**

Next to the existing `FAULT_ADDR` static in `kernel/src/idt.rs`:

```rust
/// 记录最近一次 #PF 的 error code（初始 0）。由裸入口在弹出前写入、`last_fault_err` 读取。
static FAULT_ERROR: AtomicU64 = AtomicU64::new(0);
```

And near the existing `fault_addr()` getter:

```rust
/// 读取最近一次 #PF 的 error code（初始 0；CPU 压入为 64 位零扩展，低 32 位有效）。
pub fn last_fault_err() -> u64 {
    FAULT_ERROR.load(Ordering::Relaxed)
}
```

- [ ] **Step 4: Save err in the entry before pops + return-addr fix**

The existing `page_fault_entry` naked body pushes six callee-saved regs, records CR2 into `FAULT_ADDR`, bumps the return address `[rsp+56]` by `FAULT_SKIP_LEN`, pops the six regs, `add rsp, 8` (drop hardware error code), `iretq`. Insert the error-code save **between** the CR2 store and the return-address bump. Scratch regs `rdi`/`rsi` are caller-volatile (exceptions need not preserve them) and are not among the pushed set, so no extra save/restore. The modified assembly region (do not disturb ordering or labels):

```rust
        "push r15",
        // 读 CR2 并写入 FAULT_ADDR（既有）。
        "mov rax, cr2",
        "lea rbx, [rip + {fault}]",
        "mov qword ptr [rbx], rax",
        // 保存硬件压栈的 error code：6 个 push 后位于 [rsp+48]，pop/弹栈前读取。
        "lea rdi, [rip + {err}]",
        "mov rsi, qword ptr [rsp + 48]",
        "mov qword ptr [rdi], rsi",
        // 返回地址前移 FAULT_SKIP_LEN，跳过故障指令（既有逻辑不变）。
        "mov rax, qword ptr [rsp + 56]",
        "add rax, {skip}",
        "mov qword ptr [rsp + 56], rax",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "add rsp, 8",
        "iretq",
        fault = sym FAULT_ADDR,
        err = sym FAULT_ERROR,
        skip = const FAULT_SKIP_LEN,
```

Concretely: in the existing `naked_asm!`, after the existing `"mov qword ptr [rbx], rax",` line insert the three new `err` lines (`lea rdi`, `mov rsi`, `mov [rdi]`), and add `err = sym FAULT_ERROR,` to the symbol bindings after `fault = sym FAULT_ADDR,`.

- [ ] **Step 5: Full kernel test — no regression**

Run: `cargo test -p kernel`
Expected: all PASS (existing traps + the new default test). The naked-asm must still compile (label slots verified).

- [ ] **Step 6: clippy/fmt**

Run: `cargo clippy -p kernel --all --no-deps -- -D warnings && cargo fmt -p kernel --check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add kernel/src/idt.rs
git commit -m "feat(kernel): capture #PF error code in page_fault_entry (last_fault_err)"
```

---

### Task 3: `read_cr0_wp()` — the supervisor-write gate — `kernel/src/cpu.rs`

**Files:**
- Modify: `kernel/src/cpu.rs` (new `pub fn` after `cr3()`)

**Interfaces:**
- Produces: `pub fn read_cr0_wp() -> bool` (true ⇒ WP set ⇒ supervisor write to a read-only page faults).

- [ ] **Step 1: Write the failing test** — append to `#[cfg(test)] mod tests` in `kernel/src/cpu.rs`:

```rust
#[test]
fn read_cr0_wp_returns_stable_bool() {
    // Reads current CR0.WP. Host value depends on platform (may be 0 or 1), so we
    // only pin stability/legal-bool, not a constant truth value.
    let a = read_cr0_wp();
    let b = read_cr0_wp();
    assert_eq!(a, b);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p kernel read_cr0_wp`
Expected: FAIL — function undefined (compile error).

- [ ] **Step 3: Implement**

```rust
/// 读 CR0 的 WP 位（bit 16）：1 = supervisor 写只读页产生 #PF；0 = 允许通过。
/// 这是"本次是否可能造出 supervisor 写保护现场"的总闸门，不依赖页表形态。
pub fn read_cr0_wp() -> bool {
    let mut cr0: u64;
    unsafe {
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, preserves_flags));
    }
    cr0 & (1 << 16) != 0
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p kernel read_cr0_wp`
Expected: PASS.

- [ ] **Step 5: clippy/fmt**

Run: `cargo clippy -p kernel --all --no-deps -- -D warnings && cargo fmt -p kernel --check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add kernel/src/cpu.rs
git commit -m "feat(kernel): read CR0.WP as supervisor write-protection gate"
```

---

### Task 4: generalize event wire + classify in boot — `kernel/src/protocol.rs`, `kernel/src/main.rs`

**Files:**
- Modify: `kernel/src/protocol.rs` (`page_fault_ready` shape), `kernel/src/main.rs` (C stage classification; drop pre-`translate` bool)

**Interfaces:**
- Consumes: `kernel::fault::{classify, FaultClass}`; `kernel::idt::{trigger_page_fault, last_fault_err, fault_addr}`.
- Produces wire: `page_fault_ready` with `data=addr=<hex>;err=<hex>;class=<name>`; `class` ∈ `not_present|write_protection|other`. The absent-read probe yields `addr=0x40000;err=0x0;class=not_present`.

- [ ] **Step 1: Rewrite `protocol::page_fault_ready`**

Replace the whole existing fn (currently writes `not_mapped=<0|1>` via `write_u32`) with:

```rust
pub(crate) fn page_fault_ready(fault_addr: u64, fault_err: u64, class: &str) {
    write_prefix("page_fault_ready", "ok");
    serial::write_bytes(b"|data=addr=");
    write_hex_u64(fault_addr);
    serial::write_bytes(b";err=");
    write_hex_u64(fault_err);
    serial::write_bytes(b";class=");
    serial::write_bytes(class.as_bytes());
    serial::write_bytes(b"\r\n");
}
```

(`write_hex_u64` already exists; it prints `0x0` for the zero error code. All keys/pair-splitting remain `;`-separated as before.)

- [ ] **Step 2: Update the C stage in `kernel/src/main.rs`**

Replace the C-stage block (the `matches!(kernel::cpu::translate(...)... )` → `not_mapped` binding and the `protocol::page_fault_ready(...)` call) with a version that classifies from the hardware error code:

```rust
                                    // C：触发一次真实 #PF。handler 已把 CR2 记入 fault_addr、
                                    // error code 记入 last_fault_err。分类由 CPU 给的 error code
                                    // 决定（读一个 absent 页 → P=0,W/R=0 → err=0x0 → not_present），
                                    // 不再需要软件先 translate 自证。
                                    kernel::idt::trigger_page_fault(PROBE_FAULT_VA);
                                    let fault_err = kernel::idt::last_fault_err();
                                    let class = kernel::fault::classify(fault_err);
                                    protocol::page_fault_ready(
                                        kernel::idt::fault_addr(),
                                        fault_err,
                                        class.as_str(),
                                    );
```

Remove the now-unused `not_mapped` local and its `matches!(...)` call. Keep the surrounding stages (`pml4_va`, `map`, `audit_reachable_page_tables`, GDT/IDT init) intact — they are needed earlier/later and still consume `map`/`phys_to_virt`. After the edit, `kernel_image_offset`/`kernel_len` remain used by `create_state`, so the top-of-fn `mem` imports stay live; only prune if `-D warnings` flags a specific now-unused import (verify by build, adjust narrowly).

- [ ] **Step 3: Compile kernel**

Run: `cargo build -p kernel`
Expected: clean compile; no `unused`/`deny` breakage. Fix only genuinely-dead refs the compiler names.

- [ ] **Step 4: Commit**

```bash
git add kernel/src/protocol.rs kernel/src/main.rs
git commit -m "feat(kernel): page_fault_ready carries err+class; boot classifies from hardware"
```

---

### Task 5: xtask — reparse `err` + `class`, update fixtures/assertions — `xtask/src/protocol.rs`

**Files:**
- Modify: `xtask/src/protocol.rs` (`validate_page_fault_data`, `VALID` fixture, `rejects_page_fault_*` tests, any `not_mapped` string refs)

**Interfaces:**
- Consumes: wire `addr` (hex→u64 via existing `parse_addr`), `err` (hex→u64, reuse `parse_addr`), `class` (string). `PROBE_FAULT_VA == 0x40000` already defined here.
- Constraint kept: 10-event sequence, event name `page_fault_ready`, addr==`PROBE_FAULT_VA`, current ordering. New: `err==0x0` and `class=="not_present"` asserted for the absent-read probe.

- [ ] **Step 1: Replace `validate_page_fault_data`**

Replace the existing fn body that parsed `addr` + `not_mapped`:

```rust
fn validate_page_fault_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "page_fault_ready event is missing its data field".to_owned())?;
    let mut addr = None;
    let mut err = None;
    let mut class = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "addr" => addr = Some(parse_addr(value)?),
            "err" => err = Some(parse_addr(value)?),
            "class" => class = Some(value.to_owned()),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let addr = addr.ok_or_else(|| "missing addr in page_fault_ready data".to_owned())?;
    let err = err.ok_or_else(|| "missing err in page_fault_ready data".to_owned())?;
    let class = class.ok_or_else(|| "missing class in page_fault_ready data".to_owned())?;
    if addr != PROBE_FAULT_VA {
        return Err(format!(
            "page_fault_ready addr (0x{addr:x}) must equal probe address 0x{PROBE_FAULT_VA:x}"
        ));
    }
    if err != 0 {
        return Err(format!(
            "page_fault_ready err must be 0x0 for absent-read probe, got 0x{err:x}"
        ));
    }
    if class != "not_present" {
        return Err(format!(
            "page_fault_ready class must be not_present for absent-read probe, got {class}"
        ));
    }
    Ok(())
}
```

Verify by reading the file: the existing helper `parse_addr` parses hex including a leading `0x` (used for `addr=0x40000`, `translated=0x...`), so `err=0x0` parses to `0`. Reuse it for `err`; if `parse_count` (formerly for `not_mapped`) becomes unused elsewhere it will be flagged—leave it only if other code uses it, else this removal is out of Task-5 scope and clippy will tell you (handle narrowly).

- [ ] **Step 2: Update the `VALID` fixture line**

The full-sequence fixture's page-fault line becomes:

```rust
        "PRIMA|v=1|seq=8|event=page_fault_ready|status=ok|data=addr=0x40000;err=0x0;class=not_present\n",
```

(Replace the old `data=addr=0x40000;not_mapped=1` string exactly.)

- [ ] **Step 3: Repoint the rejection tests** — replace these three existing tests and delete the obsolete one:

```rust
    #[test]
    fn rejects_page_fault_wrong_addr() {
        // addr wrong but err/class consistent
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x50000;err=0x0;class=not_present",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal probe address"), "{error}");
    }

    #[test]
    fn rejects_page_fault_wrong_class() {
        // class claim inconsistent with hardware absent-read evidence (replaces old "mapped" fiction)
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x40000;err=0x0;class=write_protection",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("class must be not_present"), "{error}");
    }

    #[test]
    fn rejects_page_fault_bad_err() {
        // err nonzero contradicts the absent-read supervisor fault signature (err stays 0x0 here)
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x40000;err=0x2;class=not_present",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("err must be 0x0"), "{error}");
    }
```

Delete the old `rejects_page_fault_mapped` test entirely (its premise — a pre-computed `not_mapped=0` — no longer maps to a hardware-measured class; `rejects_page_fault_wrong_class` replaces it). Search the whole file for `not_mapped` and update every valid-event occurrence to the new shape (there may be a couple more fixture strings).

- [ ] **Step 4: Run xtask + workspace tests**

Run: `cargo test -p xtask`
Expected: updated `VALID` passes `validate_output`; three rejection tests pass on their messages.

Then `cargo test --workspace` — expected all green (cosir + kernel + xtask).

- [ ] **Step 5: clippy/fmt**

Run: `cargo clippy --workspace --all --no-deps -- -D warnings && cargo fmt --all --check`
Expected: clean, no diff.

- [ ] **Step 6: Commit**

```bash
git add xtask/src/protocol.rs
git commit -m "feat(xtask): parse PF addr/err/class; assert absent-read class=not_present"
```

---

### Task 6: QEMU validation + best-effort WP observation — final gates + docs

**Files:**
- Modify: `kernel/src/main.rs` (add an *informational, non-PRIMA* serial note reading `read_cr0_wp()` after the PF stage). No new PRIMA event, no new sequence item, **no** hard boot assertion of a live `present∧write` capture.
- Modify: `Prima OS 实现阶段.md` (new tracker SPEC row) + whitepaper boundary line if it enumerates #PF capture depth.

**Why observational only:** per spec §3 a real `present∧write` capture is best-effort and depends on CR0.WP plus a present-but-RO VA, neither guaranteed on the bootloader CR3. Asserting it would make the QEMU gate flaky. The deterministic positive evidence lives in Task 1's decoder unit table (incl. the absent-write anti-confusion, P1W1→WriteProtection) plus the now-hardware-driven `err`/`class` asserted in Task 5.

- [ ] **Step 1: Add informational serial note (non-PRIMA, ignored by validator)**

In `kernel/src/main.rs`, right after the C-stage `protocol::page_fault_ready(...)` call in `main`, add:

```rust
                                    // 观测：读 CR0.WP，看 supervisor 写只读页在当前环境是否故障。
                                    // 仅供日志/未来自建页表证实写保护现场；不硬断言、不加 PRIMA 事件，
                                    // 也不在 CR3 缺一个 present-只读页时伪造现场。
                                    let wp = kernel::cpu::read_cr0_wp();
                                    serial::write_bytes(b"[pf] cr0.wp=");
                                    serial::write_bytes(if wp { b"1" } else { b"0" });
                                    serial::write_bytes(b"\r\n");
```

`serial::write_bytes` / `serial::flush` are already used throughout `main.rs`'s `crate::serial`, so matching their call form compiles. This line is NOT prefixed with `PRIMA|`, so `xtask validate_output` treats it as firmware noise and ignores it (verified pattern: SeaBIOS text is similarly ignored).

Per Global Constraints, do **not** add code that fabricates a write-protection capture when CR0.WP is 0 or no present-but-RO VA is triggerable; the informational note plus the recorded boundary in the docs are the honest scope.

- [ ] **Step 2: Full QEMU BIOS run**

Run: `cargo xtask test`
Expected: QEMU BIOS run=1/2 → six PASS, exit 0. The rewritten `page_fault_ready` line must validate every boot (real path now exercising the Task-5 validator). If the gate reports e.g. "class must be not_present", the boot emitted a nonzero `err` or mis-spelled class — inspect the raw QEMU log/`[pf]` line and reconcile with evidence (do not widen the fixture blindly).

- [ ] **Step 3: full workspace + clippy + fmt final gate**

Run: `cargo clippy --workspace --all --no-deps -- -D warnings && cargo fmt --all --check && cargo test --workspace`
Expected: all clean.

- [ ] **Step 4: update tracker + docs, commit**

Add a tracker row to `Prima OS 实现阶段.md` mirroring the SPEC-040 style: error-code capture (`last_fault_err`), pure decoder unit table (count + the absent-write anti-confusion), `read_cr0_wp()` gate, and the best-effort WP boundary (not claimed when not triggerable on the bootloader CR3; registered as future self-built-page-table work). Sync the whitepaper boundary line if it lists #PF capture depth.

```bash
git add kernel/src/main.rs "Prima OS 实现阶段.md"
git commit -m "docs+feat: observe CR0.WP; record SPEC row for PF error-code semantics"
```
