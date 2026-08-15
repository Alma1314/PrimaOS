# tlb_refine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-Task TLB model with non-coherent refresh semantics (memseg_unmap/map never auto-invalidate) and explicit invalidation primitives, proving T1 (COW remap without flush hits stale read-only TLB).

**Architecture:** New independent `tlb_refine.rs` layer on top of the existing `memseg_refine::RefineState` (which already holds each Task's `PageTable`). A parallel `TlbState` holds one `Tlb` per Task. `tlb_translate` returns the cached translation on a TLB hit (never consulting the page table), otherwise falls back to `pagewalk::translate` and fills the TLB. `tlb_invalidate_va` / `tlb_flush_all` are explicit invalidation primitives that `memseg_map`/`memseg_unmap` never call. Follows the existing dual-view MODEL-TEST pattern.

**Tech Stack:** Rust (nightly-2026-07-30 via `rust-toolchain.toml`), `cargo` 1.99.0-nightly, `BTreeMap` from `std::collections`.

## Global Constraints

- Toolchain pinned by `rust-toolchain.toml`; do not change it.
- Must not modify the semantics of: `cosir/src/ops.rs`, `cosir/src/memseg_refine.rs`, `cosir/src/state.rs`, `cosir/src/pagewalk.rs`, `cosir/src/cow_refine.rs`.
- Every new file starts with `#![allow(dead_code)]` (matching all existing cosir model files).
- Clippy must pass with `-- -D warnings`; rustfmt via `cargo fmt --all --check`.
- TLB capacity/replacement policy, multi-core shootdown, and CR3-switch auto-invalidation are OUT of scope (spec §3).
- Evidence boundary is `MODEL-TEST` only; never claim `KERNEL-TRACE` / `REFINEMENT-PROOF` (spec §8).
- `memseg_unmap` / `memseg_map` must NEVER call the TLB invalidation primitives (non-coherent refresh, spec §2 / §4.3).
- Access constants: `ACCESS_R=0b001`, `ACCESS_W=0b010`, `ACCESS_X=0b100` (from `crate::state`, spec §4).
- Addresses must be page-aligned and canonical (`validate_page_vaddr` from `crate::pagewalk`, spec §4.2).

---

### Task 1: Core TLB data structures and translate/invalidate primitives

**Files:**
- Create: `cosir/src/tlb_refine.rs`
- Test: `cosir/src/tlb_refine_tests.rs`

**Interfaces:**
- Consumes:
  - `crate::pagewalk::translate(&PageTable, u64) -> Result<Option<&Pte>, PageWalkError>` — the authoritative page-table walk.
  - `crate::pagewalk::validate_page_vaddr(u64) -> Result<(), PageWalkError>` — align + canonical check.
  - `crate::pagewalk::{PAGE_SIZE, PhysAddr}`.
  - `crate::memseg_refine::{RefineState, spawn_task, memseg_create, memseg_map, memseg_unmap, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X}`.
  - `crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, TaskId, VirtAddr}`.
- Produces:
  - `pub fn access_allowed(xl: &CachedXlat, for_write: bool) -> bool` — true iff the cached `access` includes W (when `for_write`) or R (when stale).
  - `pub fn has_tlb(tlb: &TlbState, task: TaskId, va: u64) -> bool` — is `va` currently cached for `task`?

- [ ] **Step 1: Write the failing tests**

Create `cosir/src/tlb_refine_tests.rs` with tests that exercise the full surface. Fall back to `memseg_refine` helpers to build a two-task state with a shared read-only page, then assert TLB hit/miss/fill and invalidation behavior:

```rust
//! tlb_refine host MODEL-TEST：每 Task TLB + 非强制刷新 + 显式失效。spec 2026-08-15-tlb-refine-design.md。

use crate::memseg_refine::{
    RefineState, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, spawn_task,
};
use crate::state::{ACCESS_R, ACCESS_W};
use crate::tlb_refine::{
    CachedXlat, TlbError, TlbState, access_allowed, flush_all, has_tlb, invalidate_va, translate,
};

const T1: u32 = 1;
const T2: u32 = 2;
const VA_A: u64 = 0x4000_0000;
const VA_B: u64 = 0x5000_0000;
const ENTRY: u64 = 0x6000_0000;

// 两个 Task，各自把自己独立的 data 段映射到唯一地址，避免共享（每 Task 独立）。
// universe 含足够帧：最重用例（stale_frame_survives_without_flush）需 5 帧。
fn two_task_state() -> (RefineState, TlbState) {
    let mut pf = RefineState::new(&[0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000])
        .expect("universe");
    spawn_task(&mut pf, T1, ENTRY).expect("spawn");
    spawn_task(&mut pf, T2, ENTRY).expect("spawn");
    let t1seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let t2seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg, T1, 0, VA_A, 1, ACCESS_R).expect("t1 A R");
    memseg_map(&mut pf, rights, true, t1seg, T1, 0, VA_B, 1, ACCESS_R).expect("t1 B R");
    memseg_map(&mut pf, rights, true, t2seg, T2, 0, VA_A, 1, ACCESS_R).expect("t2 A R");
    (pf, TlbState::new())
}

// T1a — 首次访问未命中，走页表并填充
#[test]
fn miss_fills_and_permits() {
    let (pf, mut tlb) = two_task_state();
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("walk fills");
    assert!(access_allowed(&xl, false), "只读访问应被允许");
    assert!(!access_allowed(&xl, true), "只读 map 不应许可写访问");
    assert!(has_tlb(&tlb, T1, VA_A), "未命中后应填充 TLB");
}

// T1b — 换映射（改可写）后未失效 → 命中陈旧只读 TLB，写仍被驳回
#[test]
fn remap_without_flush_hits_stale() {
    let (mut pf, mut tlb) = two_task_state();
    // 填充 TLB：只读
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill");
    // 换映射为可写（unmap + map）
    memseg_unmap(&mut pf, true, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    // 未 flush：仍命中陈旧只读翻译
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("stale hit");
    assert!(!access_allowed(&xl, true), "未 flush 应命中陈旧只读 TLB");
}

// T1c — 换映射后 invalidate_va → 走新可写 PTE；失效作用域只限指定地址（T2）
#[test]
fn invalidation_then_walk_permits_write() {
    let (mut pf, mut tlb) = two_task_state();
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill");
    let _ = translate(&pf, &mut tlb, T1, VA_B).expect("fill B");
    memseg_unmap(&mut pf, true, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    invalidate_va(&mut tlb, T1, VA_A).expect("invalidate");
    assert!(!has_tlb(&tlb, T1, VA_A), "失效后不应命中");
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("walk new PTE");
    assert!(access_allowed(&xl, true), "失效后应走新可写 PTE");
    // 同 Task 其他地址 TLB 保留（T2 作用域）
    assert!(has_tlb(&tlb, T1, VA_B), "invalidate_va 只清指定地址");
}

// T3 — Task 隔离：A 失效不影响 B 的 TLB
#[test]
fn invalidation_is_per_task() {
    let (pf, mut tlb) = two_task_state();
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill t1");
    let _ = translate(&pf, &mut tlb, T2, VA_A).expect("fill t2");
    invalidate_va(&mut tlb, T1, VA_A).expect("invalidate t1");
    assert!(!has_tlb(&tlb, T1, VA_A), "T1 已失效");
    assert!(has_tlb(&tlb, T2, VA_A), "T2 不受影响");
}

// R1 — 未映射地址返回 NotMapped 且不填充
#[test]
fn not_mapped_does_not_fill() {
    let (pf, mut tlb) = two_task_state();
    let err = translate(&pf, &mut tlb, T1, ENTRY).expect_err("ENTRY 未映射");
    assert_eq!(err, TlbError::NotMapped);
    assert!(!has_tlb(&tlb, T1, ENTRY), "未映射不应填充 TLB");
}

// R2 — 换映射改变物理帧后，未失效的 TLB 仍返回旧 frame_phys。
#[test]
fn stale_frame_survives_without_flush() {
    let (mut pf, mut tlb) = two_task_state();
    // 先单独 map 一块可写地址 Y（本测试专用新地址）避免干扰
    let yseg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, yseg, T1, 0, 0x7000_0000, 1, ACCESS_R | ACCESS_W)
        .expect("map Y RW");
    let xl0 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("fill Y");
    let old = xl0.frame_phys;
    // 换映射到另一新段（新物理帧）
    memseg_unmap(&mut pf, true, T1, 0x7000_0000, 1).expect("unmap Y");
    let yseg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg2");
    memseg_map(&mut pf, rights, true, yseg2, T1, 0, 0x7000_0000, 1, ACCESS_R | ACCESS_W)
        .expect("remap Y");
    // 未失效：仍是旧帧
    let xl1 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("stale hit");
    assert_eq!(xl1.frame_phys, old, "未 flush 应命中旧物理帧");
    // 失效后走新帧
    invalidate_va(&mut tlb, T1, 0x7000_0000).expect("invalidate");
    let xl2 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("walk new");
    assert_ne!(xl2.frame_phys, old, "失效后应观察到新物理帧");
}

// 错误面：NoSuchTask（空 TlbState 即触发）
#[test]
fn missing_task_errors() {
    let mut tlb = TlbState::new();
    assert_eq!(
        flush_all(&mut tlb, 99).expect_err("no such task"),
        TlbError::NoSuchTask
    );
    assert_eq!(
        invalidate_va(&mut tlb, 99, VA_A).expect_err("no such task"),
        TlbError::NoSuchTask
    );
    // 空状态上 flush_all 同一不存在的 task 仍为 NoSuchTask（幂等错误，非重复清空）
    assert_eq!(
        flush_all(&mut tlb, 99).expect_err("still no such task"),
        TlbError::NoSuchTask
    );
}

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p cosir tlb_refine_tests`
Expected: compile error "unresolved import `crate::tlb_refine`" (module not yet declared).

- [ ] **Step 3: Declare the module and write the minimal implementation**

Add to `cosir/src/main.rs`. First, next to the other model-module declarations (near line 7, after `mod state;`):

```rust
mod tlb_refine;
```

Second, next to the other test-module declarations (near lines 9-14, alongside `#[cfg(test)] mod pagewalk_tests;`):

```rust
#[cfg(test)]
mod tlb_refine_tests;
```

Then create `cosir/src/tlb_refine.rs`:

Create `cosir/src/tlb_refine.rs`:

```rust
#![allow(dead_code)]

use crate::memseg_refine::RefineState;
use crate::pagewalk::{PageWalkError, PhysAddr, translate as table_translate, validate_page_vaddr};
use crate::state::{TaskId, VirtAddr, ACCESS_W};
use std::collections::BTreeMap;

/// 每条 TLB 缓存翻译：命中时返回的物理帧与权限，可对不应期 PTE（陈旧）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedXlat {
    pub frame_phys: PhysAddr,
    pub access: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Tlb {
    entries: BTreeMap<VirtAddr, CachedXlat>,
}

/// 每 Task 独立 TLB（每 CR3 一 TLB）。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TlbState {
    per_task: BTreeMap<TaskId, Tlb>,
}

impl TlbState {
    pub fn new() -> Self {
        TlbState::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlbError {
    NoSuchTask,
    UnalignedAddress,
    NonCanonicalAddress,
    NotMapped,
}

pub fn access_allowed(xl: &CachedXlat, for_write: bool) -> bool {
    if for_write {
        xl.access & ACCESS_W != 0
    } else {
        xl.access != 0
    }
}

pub fn has_tlb(tlb: &TlbState, task: TaskId, va: u64) -> bool {
    tlb.per_task
        .get(&task)
        .is_some_and(|t| t.entries.contains_key(&va))
}

/// 命中走 TLB 缓存（不用户表）；未命中去页表并填充。
pub fn translate(pf: &RefineState, tlb: &mut TlbState, task: TaskId, va: u64) -> Result<CachedXlat, TlbError> {
    match validate_page_vaddr(va) {
        Err(PageWalkError::UnalignedAddress) => return Err(TlbError::UnalignedAddress),
        Err(PageWalkError::NonCanonicalAddress) => return Err(TlbError::NonCanonicalAddress),
        Ok(()) => {}
    }
    let entry = tlb
        .per_task
        .get_mut(&task)
        .ok_or(TlbError::NoSuchTask)?;
    if let Some(xl) = entry.entries.get(&va) {
        return Ok(*xl); // 命中：陈旧与否都返回缓存
    }
    let tc = pf.tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    let Some(pte) = table_translate(&tc.table, va).map_err(|_| TlbError::NotMapped)? else {
        return Err(TlbError::NotMapped); // 未映射，不填充
    };
    let xl = CachedXlat {
        frame_phys: pte.frame_phys,
        access: pte.mapping.access,
    };
    entry.entries.insert(va, xl);
    Ok(xl)
}

/// 单地址失效（对应 invlpg）：幂等。
pub fn invalidate_va(tlb: &mut TlbState, task: TaskId, va: u64) -> Result<(), TlbError> {
    let entry = tlb.per_task.get_mut(&task).ok_or(TlbError::NoSuchTask)?;
    entry.entries.remove(&va);
    Ok(())
}

/// 全量失效（对应 CR3 reload / INVPCID）。
pub fn flush_all(tlb: &mut TlbState, task: TaskId) -> Result<(), TlbError> {
    let entry = tlb.per_task.get_mut(&task).ok_or(TlbError::NoSuchTask)?;
    entry.entries.clear();
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p cosir tlb_refine_tests`
Expected: all `tlb_refine_tests` pass.

- [ ] **Step 5: Run lint and fmt**

Run: `cargo clippy -p cosir --all-targets --no-deps -- -D warnings` then `cargo fmt --all --check`
Expected: no warnings; fmt clean. (If the awkward `missing_task_errors` scaffolding trips clippy, replace it with the simplified second variant and re-run clippy.)

- [ ] **Step 6: Commit**

```bash
git add cosir/src/main.rs cosir/src/tlb_refine.rs cosir/src/tlb_refine_tests.rs
git commit -m "feat: add per-task TLB model with non-coherent refresh and explicit invalidation"
```

---

### Task 2: Wire TLB smoke into `main.rs` and validate no regression

**Files:**
- Modify: `cosir/src/main.rs:145` (fn main), plus the `#[cfg(test)]` module declaration block near line 9-14.

**Interfaces:**
- Consumes: everything from Task 1 (`translate`, `invalidate_va`, `flush_all`, `access_allowed`, `has_tlb`, `TlbState`, `TlbError`).
- Produces: a `PASS`-printing smoke block in `fn main()` demonstrating the T1 non-coherent-refresh claim end to end.

- [ ] **Step 1: Write the failing test (smoke smoke-step cannot be "tested" by cargo test)**

The smoke block below is a runtime assertion inside `fn main()` that exits non-zero (panics) on failure, which would fail the overall `cargo run -p cosir` and `cargo xtask test`. Write it now; it will `panic!` until the TLB surface exists, giving a red signal.

- [ ] **Step 2: Run smoke to verify current red (module missing)**

Run: `cargo run -p cosir`
Expected: FAIL (cannot compile — `crate::tlb_refine` not yet declared / smoke references missing names).

- [ ] **Step 3: Add the module declaration and the smoke block**

First add the non-test module declaration next to the other model modules (line ~7):

```rust
mod tlb_refine;
```

Then append a smoke block inside `fn main()` (after the COW smoke block, before the closing `}` of `main`):

```rust
    // TLB 细化冒烟：非强制刷新 + 显式失效（spec 2026-08-15-tlb-refine-design.md）。
    // 每 Task 独立 TLB；换映射后未失效命中陈旧只读 TLB，显式失效后走新可写 PTE。
    {
        use memseg_refine::{
            RefineState, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, memseg_unmap,
            spawn_task,
        };
        use tlb_refine::{access_allowed, flush_all, has_tlb, invalidate_va, translate};
        use state::{ACCESS_R, ACCESS_W};

        let mut pf = RefineState::new(&[0x1000, 0x2000, 0x3000, 0x4000]).expect("universe");
        spawn_task(&mut pf, 1, 0x6000_0000).expect("spawn");
        let seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
        let rights = SEG_MAP_R | SEG_MAP_W;
        memseg_map(&mut pf, rights, true, seg, 1, 0, 0x4000_0000, 1, ACCESS_R).expect("map R");
        let mut tlb = tlb_refine::TlbState::new();
        let xl = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("walk fills");
        assert!(!access_allowed(&xl, true), "只读 map 的首访不应许可写");
        assert!(has_tlb(&tlb, 1, 0x4000_0000), "未命中后应填充 TLB");
        // 换映射为可写（unmap+map），未失效
        memseg_unmap(&mut pf, true, 1, 0x4000_0000, 1).expect("unmap");
        let seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg2");
        memseg_map(&mut pf, rights, true, seg2, 1, 0, 0x4000_0000, 1, ACCESS_W).expect("remap W");
        let xl2 = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("stale hit");
        assert!(!access_allowed(&xl2, true), "未 flush 应命中陈旧只读 TLB");
        // 显式失效后走新可写 PTE
        invalidate_va(&mut tlb, 1, 0x4000_0000).expect("invalidate");
        assert!(!has_tlb(&tlb, 1, 0x4000_0000), "失效后不应命中");
        let xl3 = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("walk new");
        assert!(access_allowed(&xl3, true), "失效后应走新可写 PTE");
        // flush_all 全量失效
        let _ = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("refill");
        flush_all(&mut tlb, 1).expect("flush all");
        assert!(!has_tlb(&tlb, 1, 0x4000_0000), "flush_all 后应清空");
        println!("PASS model-test tlb non-coherent refresh + explicit invalidation");
    }
```

- [ ] **Step 4: Run the full test suites to verify everything is green**

Run: `cargo test -p cosir` then `cargo test --workspace` then `cargo clippy --workspace --all-targets --no-deps -- -D warnings`
Expected: cosir tests pass (existing ~117 + new tlb tests), workspace green, clippy no warnings.

- [ ] **Step 5: Run smoke and QEMU no-regression**

Run: `cargo run -p cosir` then `cargo xtask test`
Expected: `cargo run` prints the new `PASS model-test tlb ...` line plus all existing smoke PASS lines; `cargo xtask test` prints six PASS and exits 0 (QEMU BIOS startup unaffected).

- [ ] **Step 6: Commit**

```bash
git add cosir/src/main.rs
git commit -m "test: wire tlb_refine non-coherent refresh smoke and assert no regression"
```

---

### Task 3: Record evidence in the implementation-stage tracker

**Files:**
- Modify: `Prima OS 实现阶段.md` (tracker): the `当前快照` table's "最近细化证据"-adjacent row, and the `证据索引` table (add `SPEC-016`), and the `当前阻塞项`/known-issues area if warranted.

**Interfaces:**
- Consumes: the acceptance evidence produced in Task 2 (test counts, clippy status, smoke PASS, `cargo xtask test` result).

- [ ] **Step 1: Capture exact evidence with pinned versions**

Run: `cargo --version && rustc --version` and record the exact numbers from Task 2 (cosir test count, workspace count, clippy clean, `cargo run` new PASS line, `cargo xtask test` six-PASS + exit 0).

- [ ] **Step 2: Update the tracker**

Add a `SPEC-016` row to the `证据索引` table following the exact style of `SPEC-015`, and update the `当前快照` paragraph under "最近细化证据". State the boundary explicitly: evidence is `MODEL-TEST` only; does NOT claim `KERNEL-TRACE` / `REFINEMENT-PROOF`; no replacement policy / multi-core shootdown / CR3-switch auto-invalidation (spec §3). Do not overstate: this is a TLB host model, not real hardware.

- [ ] **Step 3: Commit**

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: record verified tlb_refine model evidence"
```
