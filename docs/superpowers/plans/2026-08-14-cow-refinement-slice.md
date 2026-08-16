# COW fork 组合协议细化实现计划

> Status: Completed / Archived
>
> 本文件是 2026-08-14 的历史实施计划，不再是当前可执行指令。当前治理与后继要求以 `docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md` 和 `Prima OS 实现阶段.md` 为权威。
>
> Evidence: SPEC-015
>
> Implementation commits: `518670e`, `43d940b`, `18aac5f`, `1357a4d`, `a65fce6`, `5827d75`, `54f932f`
>
> Deprecated assumptions: “证明完整 COW fork”、无条件“无需新增内核机制”、副本复用原物理帧、测试手工同步双状态。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**历史 Goal（已弃用，非当前结论）：** 本计划曾拟新增 `cow_refine` 验证层，以测试 §12.8 声明的原语组合；“精确表达 COW fork”与“无需新增内核机制”均为已弃用的强主张，不能由本计划或 host `MODEL-TEST` 支持。成功路径与反例边界的当前证据范围以权威设计和 tracker 为准。

**Architecture:** 新增 `cosir/src/cow_refine.rs`，同时持用既有 `memseg_refine::RefineState`（四级索引树/PTE/segment/帧池）与 `ops::State`（`Faulted`/`pending_exc`/`exc_resume`/Audit），以 `TaskId` 对齐；不修改两个既有模型。公开 `probe_write`（纯查询）、`write_fault`（组合动作，`Wrote|Faulted`）、`cow_state_consistent`（跨模型一致性断言）。TDD：先写失败测试，再最小实现，独立提交。

**Tech Stack:** Rust workspace（cosir crate）、`BTreeMap`/`HashMap` 集合、Cargo tests、Clippy、COSIR Markdown 规范与阶段追踪器。

## Global Constraints

- 本切片只提供 host `MODEL-TEST`，不得声称 `KERNEL-TRACE` 或 `REFINEMENT-PROOF`，形式化义务留给 BLK-001。
- **不修改** `cosir/src/ops.rs`、`cosir/src/memseg_refine.rs`、`cosir/src/pagewalk.rs`、`cosir/src/state.rs` 的既有语义；验证层只 `use` 其公开项。
- 不新增依赖；不接入真实 kernel/IDT/trap/`iretq`。
- `cargo xtask test` 只证明启动闭环无回归。
- 每项失败/原子性测试比较完整组合状态二元组 `(pf: RefineState, exc: ops::State)`。
- 复用既有常量：`state::ACCESS_R/W/X`、`SRET_FRAME_LEN`、`USER_CS/USER_SS`；`pagewalk::PAGE_SIZE`。
- 新增源码文件顶部使用 `#![allow(dead_code)]`（与 `memseg_refine.rs`/`pagewalk.rs` 一致）。

## 文件结构

- Create: `cosir/src/cow_refine.rs` — 验证层：`probe_write`/`write_fault`/`cow_state_consistent` + fixture helper。
- Create: `cosir/src/cow_refine_tests.rs` — 单元测试（成功路径 + R1–R4）。
- Modify: `cosir/src/main.rs` — 声明模块 + smoke 场景。
- Modify: `Prima OS 实现阶段.md` — 新增 `SPEC-0xx` 证据与已知问题表行。
- Modify: `COSIR 原语规范.md` — §12.8 增补证据边界注释。

任务分解锁定的接口常量与函数签名（供各任务按编号引用）：

```rust
// cow_refine.rs 公开项：
pub enum CowProbe { ReadOnly, Writable, NotMapped }
pub enum WriteOutcome { Wrote, Faulted(ExceptionKind) }
pub fn probe_write(pf: &RefineState, task: TaskId, va: VirtAddr) -> CowProbe;
pub fn write_fault(exc: &mut ops::State, pf: &mut RefineState, task: TaskId, va: VirtAddr) -> WriteOutcome;
pub fn cow_state_consistent(pf: &RefineState, exc: &ops::State, task: TaskId) -> bool;
pub fn page_fault_source_id() -> ExcSourceId;      // = 1
pub const PARENT: TaskId = 1;
pub const CHILD: TaskId = 2;
pub const COW_VA: VirtAddr = 0x4000_0000;           // 父子共享映射的虚拟地址
pub const CODE_VA: VirtAddr = 0x5000_0000;          // 可执行代码映射（frame RIP 落点）
pub fn new_refine() -> RefineState;                 // 帧池 4 帧，P(1) 与 C(2) 已 spawn，含 CODE 可执行段
pub fn base_exc() -> ops::State;                    // P/C task、source、cap、subscription 就绪，status=Running
```

COW 主场景使用的段与映射常量（fixture helper 内部固定）：

```rust
// new_refine(): universe = [0x1000,0x2000,0x3000,0x4000]
//   共享数据段 seg=1：memseg_create(1, ACCESS_R|ACCESS_W)，占 0x1000
//   可执行代码段 seg=2：memseg_create(1, ACCESS_X)，占 0x2000
//   数据：P 映射 seg1 @COW_VA RW；C 映射 seg1 @COW_VA R
//   代码：P 映射 seg2 @CODE_VA X；C 映射 seg2 @CODE_VA X（供 exc_resume 的 RIP 校验）
// base_exc(): tasks P(Running, vspace[COW_VA..+0x1000] user RW, [CODE_VA..+0x1000] user X),
//             tasks C(Running, vspace[COW_VA..+0x1000] user R, [CODE_VA..+0x1000] user X),
//             exc_sources[1]=PageFault{page_present=false,writable_fault=true} active,
//             capabilities[10]=ExceptionSource(1)/CAP_EXC_SUBSCRIBE owner C, active,
//             capabilities[11]=Segment(1)/CAP_SEG_WRITE owner C, active,
//             capabilities[12]=Segment(2)/CAP_SEG_WRITE owner C, active,（frame Segment 用 seg 2）
//             frame Segment 2 bytes filled with a valid user SretFrame（RIP@CODE_VA+0x80, RSP@COW_VA+0x80）,
//             subscriptions={(C,1),(P,1)}
```

**关键约束（exc_resume 校验）：** `ops::exc_resume` 要求帧中 `RIP` 位于调用者可执行 user 映射（需 `ACCESS_X`）、`RSP` 位于可写 user 映射（需 `ACCESS_W`）。因此恢复帧必须 `RIP = CODE_VA + 0x80`、`RSP = COW_VA + 0x80`；frame Segment 必须是 C 可写的独立段（用代码段 `seg=2` 作 frame 载体）。所有测试与 smoke 中的 `exc_resume` 调用都必须传入这个 RIP/RSP 与 frame cap，否则返回 `InvalidFrame`。

`cow_state_consistent(pf, exc, task)` 的具体语义：对给定 `task`，若其 `ops::State` 的 `vspace` 有覆盖 `COW_VA` 的 `PageMapping`，则其 `access` 必须等于 `RefineState.tasks[task].vspace[COW_VA].access`；若 `RefineState` 无该 va 而 ops 有（或反之），返回 false。两模型 Task 都必须存在。为保持聚焦，仅检查 `COW_VA` 这一页。

---

### Task 1: 建立 cross-model fixture 与状态可比性

**Files:**
- Create: `cosir/src/cow_refine.rs`
- Create: `cosir/src/cow_refine_tests.rs`
- Modify: `cosir/src/main.rs`

**Interfaces:**
- Consumes: `memseg_refine::{RefineState, memseg_create, memseg_map, memseg_alpha, spawn_task}`；`ops::{raise_exception, exc_resume, schedule}`；`state::{ACCESS_R, ACCESS_W, ACCESS_X, ExceptionKind, SretFrame, State, Task, TaskStatus, Capability, CapObject, CAP_EXC_SUBSCRIBE, CAP_SEG_WRITE, PageMapping, USER_CS, USER_SS, SRET_FRAME_LEN}`；`pagewalk::PAGE_SIZE`。
- Produces: `new_refine()`、`base_exc()`、常量 `PARENT/CHILD/COW_VA`。

- [ ] **Step 1: 在 `main.rs` 声明新模块（红灯编译失败）**

在 `cosir/src/main.rs` 顶部模块声明区加入 `mod cow_refine;` 与 `#[cfg(test)] mod cow_refine_tests;`：

```rust
mod abstract_ops;
mod cow_refine;
mod machine;
mod memseg_refine;
mod ops;
mod pagewalk;
mod state;

#[cfg(test)]
mod cow_refine_tests;
#[cfg(test)]
mod memseg_refine_tests;
#[cfg(test)]
mod pagewalk_tests;
```

Run: `cargo test -p cosir`
Expected: FAIL（找不到 `cow_refine` 模块）。

- [ ] **Step 2: 写 fixture helper（二者互为类型不匹配，需同时落地）**

创建 `cosir/src/cow_refine.rs`，给出常量与两个 fixture helper（此处只建立数据，验明类型可用）：

```rust
#![allow(dead_code)]

use crate::memseg_refine::{
    RefineState, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, spawn_task,
};
use crate::ops;
use crate::pagewalk::PAGE_SIZE;
use crate::state::{
    ACCESS_R, ACCESS_W, ACCESS_X, CAP_EXC_SUBSCRIBE, CAP_SEG_WRITE, CapObject, Capability,
    ExceptionKind, ExcSourceId, PageMapping, SRET_FRAME_LEN, SretFrame, State, Task, TaskId,
    TaskStatus, USER_CS, USER_SS,
};

pub const PARENT: TaskId = 1;
pub const CHILD: TaskId = 2;
pub const COW_VA: u64 = 0x4000_0000;
pub const CODE_VA: u64 = 0x5000_0000;
pub const CODE_SEG: u32 = 2; // frame 载体段：C 可写、内核可读
pub const FRAME_CAP: u32 = 12; // C 对 CODE_SEG 的 SegWrite capability

pub fn page_fault_source_id() -> ExcSourceId {
    1
}

/// 帧池 4 帧；P(1) 与 C(2) 已 spawn；数据段 seg=1（RW）、可执行段 seg=2（X）。
pub fn new_refine() -> RefineState {
    let mut pf = RefineState::new(&[0x1000, 0x2000, 0x3000, 0x4000]).expect("valid frame universe");
    spawn_task(&mut pf, PARENT, CODE_VA).expect("spawn parent");
    spawn_task(&mut pf, CHILD, CODE_VA).expect("spawn child");
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
    // 数据段 seg=1（RW）；代码段 seg=2（X，兼作 frame 载体）
    let data_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("create data seg");
    assert_eq!(data_seg, 1);
    let code_seg = memseg_create(&mut pf, 1, ACCESS_X).expect("create code seg");
    assert_eq!(code_seg, 2);
    // 数据：父私写、子只读
    memseg_map(&mut pf, rights, true, data_seg, PARENT, 0, COW_VA, 1, ACCESS_R | ACCESS_W)
        .expect("map parent data RW");
    memseg_map(&mut pf, rights, true, data_seg, CHILD, 0, COW_VA, 1, ACCESS_R)
        .expect("map child data R");
    // 代码：父子都可执行（frame RIP 落点）
    memseg_map(&mut pf, rights, true, code_seg, PARENT, 0, CODE_VA, 1, ACCESS_X)
        .expect("map parent code X");
    memseg_map(&mut pf, rights, true, code_seg, CHILD, 0, CODE_VA, 1, ACCESS_X)
        .expect("map child code X");
    pf
}

/// 构造一个合法恢复帧：RIP 在可执行 CODE_VA、RSP 在可写 COW_VA（user canonical）。
pub fn resume_frame() -> SretFrame {
    SretFrame {
        rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
        rsp: COW_VA + 0x80, r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
        rflags: 0x202, rip: CODE_VA + 0x80, cs: USER_CS, ss: USER_SS,
    }
}

/// 异常源、capability、订阅、P/C task（Running）与 frame Segment(seg=2) 就绪的 ops 态。
pub fn base_exc() -> State {
    let mut exc = State {
        booted: true,
        ..State::default()
    };
    exc.exc_sources.insert(
        page_fault_source_id(),
        crate::state::ExceptionSource {
            supported_kind: ExceptionKind::PageFault { page_present: false, writable_fault: true },
            active: true,
        },
    );
    exc.capabilities.insert(
        10,
        Capability {
            owner: CHILD,
            object: CapObject::ExceptionSource(page_fault_source_id()),
            rights: CAP_EXC_SUBSCRIBE,
            active: true,
        },
    );
    // frame 载体段 = 代码段 CODE_SEG；C 有写权
    exc.capabilities.insert(
        FRAME_CAP,
        Capability {
            owner: CHILD,
            object: CapObject::Segment(CODE_SEG),
            rights: CAP_SEG_WRITE,
            active: true,
        },
    );
    // 同时给 P 一个指向 / 无 frame 权（P 不参与 exc_resume，仅保持订阅）
    exc.segments.insert(
        CODE_SEG,
        crate::state::Segment {
            bytes: {
                let frame = resume_frame();
                let mut bytes = vec![0; SRET_FRAME_LEN];
                bytes.copy_from_slice(&frame.encode());
                bytes
            },
            kernel_readable: true,
        },
    );
    exc.subscriptions.insert((PARENT, page_fault_source_id()));
    exc.subscriptions.insert((CHILD, page_fault_source_id()));
    exc.tasks.insert(
        PARENT,
        Task {
            id: PARENT,
            status: TaskStatus::Running,
            vspace: vec![
                PageMapping { base: COW_VA, len: PAGE_SIZE, access: ACCESS_R | ACCESS_W, user: true },
                PageMapping { base: CODE_VA, len: PAGE_SIZE, access: ACCESS_X, user: true },
            ],
            kernel_resume_frame: None,
        },
    );
    exc.tasks.insert(
        CHILD,
        Task {
            id: CHILD,
            status: TaskStatus::Running,
            vspace: vec![
                PageMapping { base: COW_VA, len: PAGE_SIZE, access: ACCESS_R, user: true },
                PageMapping { base: CODE_VA, len: PAGE_SIZE, access: ACCESS_X, user: true },
            ],
            kernel_resume_frame: None,
        },
    );
    exc
}
```

- [ ] **Step 3: 写状态可比性测试（`cow_refine_tests.rs`）**

创建 `cosir/src/cow_refine_tests.rs`：

```rust
use crate::cow_refine::{PARENT, base_exc, new_refine};

#[test]
fn cross_model_state_is_deeply_comparable() {
    let pf = new_refine();
    let exc = base_exc();
    assert_eq!(pf, pf.clone());
    assert_eq!(exc, exc.clone());
    assert!(crate::memseg_refine::memseg_alpha(&pf), "fixture alpha");
}

#[test]
fn parent_and_child_share_read_only_at_child() {
    let pf = new_refine();
    let child_ref = pf.tasks[&crate::cow_refine::CHILD].vspace[&crate::cow_refine::COW_VA];
    assert_eq!(child_ref.access, crate::state::ACCESS_R);
}
```

Run: `cargo test -p cosir`
Expected: FAIL（`cow_refine` 因 `crate::state::ExceptionSource` 路径等未导入会先报多项编译错；逐个补 `use` 直至通过）。

- [ ] **Step 4: 编译通过后提交**

Fix 所有编译错误（补 import / 修字段路径），直到：

Run:
```bash
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS（`cross_model_state_is_deeply_comparable`、`parent_and_child_share_read_only_at_child` 均过）。

```bash
git add cosir/src/cow_refine.rs cosir/src/cow_refine_tests.rs cosir/src/main.rs
git commit -m "feat: establish cross-model COW fixture"
```

---

### Task 2: `probe_write` 三态纯查询

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `pagewalk::translate`、`pagewalk::Pte`（`user`/`writable`/`present` 字段）。
- Produces: `pub enum CowProbe { ReadOnly, Writable, NotMapped }`；`pub fn probe_write(pf: &RefineState, task: TaskId, va: VirtAddr) -> CowProbe`（无副作用）。

- [ ] **Step 1: 写失败测试**

在 `cow_refine_tests.rs` 增：

```rust
use crate::cow_refine::{CHILD, COW_VA, CowProbe, PARENT, new_refine, probe_write};

#[test]
fn probe_parent_readwrite_is_writable() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
}

#[test]
fn probe_child_readonly_is_readonly() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::ReadOnly);
}

#[test]
fn probe_unmapped_is_not_mapped() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, 0x0600_0000), CowProbe::NotMapped);
}

#[test]
fn probe_has_no_side_effects() {
    let pf = new_refine();
    let before = pf.clone();
    let _ = probe_write(&pf, CHILD, COW_VA);
    assert_eq!(pf, before);
}
```

Run: `cargo test -p cosir probe_ -- --nocapture`
Expected: FAIL（`probe_write`/`CowProbe` 未定义）。

- [ ] **Step 2: 实现 `CowProbe` 与 `probe_write`**

在 `cow_refine.rs` 增：

```rust
use crate::pagewalk::translate;

pub enum CowProbe { ReadOnly, Writable, NotMapped }

/// 纯查询该 vaddr 在 task 四级树中的可写观测（无副作用）。
pub fn probe_write(pf: &RefineState, task: TaskId, va: u64) -> CowProbe {
    let Some(tc) = pf.tasks.get(&task) else {
        return CowProbe::NotMapped;
    };
    let Ok(Some(pte)) = translate(&tc.table, va) else {
        return CowProbe::NotMapped;
    };
    if !pte.present || !pte.user {
        return CowProbe::NotMapped;
    }
    if pte.writable {
        CowProbe::Writable
    } else {
        CowProbe::ReadOnly
    }
}
```

注意 `RefineState.tasks` 已存在于 `new_refine()`；`translate` 返回 `Result<Option<&Pte>, PageWalkError>`。

- [ ] **Step 3: 验证并提交**

Run:
```bash
cargo test -p cosir probe_ -- --nocapture
cargo test -p cosir
```
Expected: PASS。

```bash
git add cosir/src/cow_refine.rs cosir/src/cow_refine_tests.rs
git commit -m "feat: add readonly probe of COW target page"
```

---

### Task 3: `write_fault` 组合动作

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `probe_write`、`ops::raise_exception`、`ops::schedule`、`ExceptionKind::PageFault`、`TaskStatus`。
- Produces: `pub enum WriteOutcome { Wrote, Faulted(ExceptionKind) }`；`pub fn write_fault(exc: &mut ops::State, pf: &mut RefineState, task: TaskId, va: u64) -> WriteOutcome`。

语义（先 `raise_exception` 前用 `schedule` 确保 task 为 `Running`，因 `raise_exception` 断言 `Running`）：

```rust
pub enum WriteOutcome { Wrote, Faulted(ExceptionKind) }

/// 模拟写指令：可写→Wrote；只读或未映射→暂停 task 并递送 page fault。
pub fn write_fault(exc: &mut ops::State, pf: &mut RefineState, task: TaskId, va: u64) -> WriteOutcome {
    let kind = match probe_write(pf, task, va) {
        CowProbe::Writable => return WriteOutcome::Wrote,
        CowProbe::ReadOnly => ExceptionKind::PageFault { page_present: true, writable_fault: true },
        CowProbe::NotMapped => ExceptionKind::PageFault { page_present: false, writable_fault: true },
    };
    ops::schedule(exc, task); // 确保 task 处于可执行态（raise_exception 断言 Running）
    let save = resume_frame();
    ops::raise_exception(exc, task, page_fault_source_id(), kind, save);
    WriteOutcome::Faulted(kind)
}
```

> **语义说明：** 对 `NotMapped` 也递送 `page_present=false` fault 并暂停 task，保持"写无法访问的地址 = 可观察 fault"的自洽语义；COW 成功路径不出现 `NotMapped`，它属于应按需映射处理的边界 case。

- [ ] **Step 1: 写失败测试**

在 `cow_refine_tests.rs` 增：

```rust
use crate::cow_refine::{CHILD, COW_VA, PARENT, WriteOutcome, new_refine, base_exc, write_fault};
use crate::state::{ExceptionKind, TaskStatus};

#[test]
fn write_fault_on_readwrite_parent_writes_without_fault() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    assert_eq!(write_fault(&mut exc, &mut pf, PARENT, COW_VA), WriteOutcome::Wrote);
    // 父仍 Running，未产生 pending
    assert_eq!(exc.tasks[&PARENT].status, TaskStatus::Running);
    assert!(!exc.pending_exc.contains_key(&PARENT));
}

#[test]
fn write_fault_on_child_readonly_triggers_page_fault() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let before_pf = pf.clone();
    let out = write_fault(&mut exc, &mut pf, CHILD, COW_VA);
    assert!(matches!(out, WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
    assert!(exc.pending_exc.contains_key(&CHILD));
    assert_eq!(pf, before_pf, "probe/写不修改页表");
}

#[test]
fn write_fault_on_unmapped_is_faulted() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let out = write_fault(&mut exc, &mut pf, CHILD, 0x0600_0000);
    assert!(matches!(out, WriteOutcome::Faulted(ExceptionKind::PageFault { page_present: false, .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
}
```

`base_exc()` 初始把 C 置 `Running`，故 `write_fault` 内 `schedule`（仅 Ready→Running）不改变它，`raise_exception` 断言成立。

- [ ] **Step 2: 确认红灯**

Run: `cargo test -p cosir write_fault_on_ -- --nocapture`
Expected: FAIL（`write_fault`/`WriteOutcome` 未定义）。

- [ ] **Step 3: 实现**

按上方 `WriteOutcome`/`write_fault` 代码写入 `cow_refine.rs`；把 `resume_frame` 改为 `pub(crate)` 或内部复用（见 Task 1 已定义私有函数，此处保持私有即可，`write_fault` 在 `cow_refine.rs` 内可直接调用）。

- [ ] **Step 4: 验证并提交**

Run:
```bash
cargo test -p cosir write_fault_on_ -- --nocapture
cargo test -p cosir
```
Expected: PASS。

```bash
git add cosir/src/cow_refine.rs cosir/src/cow_refine_tests.rs
git commit -m "feat: model write-access page fault outcome"
```

---

### Task 4: `cow_state_consistent` 跨模型一致性断言

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `RefineState.tasks[task].vspace[COW_VA].access`；`ops::State.tasks[task].vspace`（`Vec<PageMapping>`）。
- Produces: `pub fn cow_state_consistent(pf: &RefineState, exc: &ops::State, task: TaskId) -> bool`。

语义：对 `task`，两模型都必须存在该 Task；`RefineState.tasks[task].vspace` 中 `COW_VA` 对应的 `SegmentRef.access` 必须等于 `ops` 中覆盖 `COW_VA` 的 `PageMapping.access`（且该 mapping `user` 为真）。仅检查 `COW_VA` 页。

```rust
pub fn cow_state_consistent(pf: &RefineState, exc: &ops::State, task: TaskId) -> bool {
    let (Some(tc), Some(tk)) = (pf.tasks.get(&task), exc.tasks.get(&task)) else {
        return false;
    };
    let Some(pf_access) = tc.vspace.get(&COW_VA).map(|r| r.access) else {
        return false;
    };
    match tk.vspace.iter().find(|m| m.contains(COW_VA)) {
        Some(mapping) => mapping.user && mapping.access == pf_access,
        None => false,
    }
}
```

- [ ] **Step 1: 写失败测试**

```rust
use crate::cow_refine::{CHILD, COW_VA, PARENT, cow_state_consistent, new_refine, base_exc};

#[test]
fn shared_readonly_state_is_consistent() {
    let pf = new_refine();
    let exc = base_exc();
    assert!(cow_state_consistent(&pf, &exc, PARENT));
    assert!(cow_state_consistent(&pf, &exc, CHILD));
}

#[test]
fn inconsistent_when_child_upgraded_in_one_model_only() {
    let mut exc = base_exc();
    // 只改 ops 侧为 RW，RefineState 仍 R → 不一致
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = crate::state::ACCESS_R | crate::state::ACCESS_W;
    let pf = new_refine();
    assert!(!cow_state_consistent(&pf, &exc, CHILD));
}

#[test]
fn missing_task_is_inconsistent() {
    let mut pf = new_refine();
    // 删掉 RefineState 的 child task → 不一致
    let gone = pf.tasks.remove(&CHILD).unwrap();
    let exc = base_exc();
    assert!(!cow_state_consistent(&pf, &exc, CHILD));
    pf.tasks.insert(CHILD, gone);
}
```

Run: `cargo test -p cosir state_is_consistent -- --nocapture`
Expected: FAIL（函数未定义）。

- [ ] **Step 2: 实现并验证**

写入上述实现；Run `cargo test -p cosir state_is_consistent -- --nocapture` → PASS；再 `cargo test -p cosir` → PASS。

- [ ] **Step 3: 提交**

```bash
git add cosir/src/cow_refine.rs cosir/src/cow_refine_tests.rs
git commit -m "feat: assert cross-model COW state consistency"
```

---

### Task 5: COW 成功路径端到端（主用例）

**Files:**
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `new_refine`、`base_exc`、`write_fault`、`cow_state_consistent`、`memseg_refine::{memseg_create, memseg_map, memseg_unmap, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha}`、`ops::{exc_resume, schedule}`、`state::{ACCESS_R, ACCESS_W, SRET_FRAME_LEN, USER_CS, USER_SS, ExceptionKind, TaskStatus, SretFrame}`。
- Produces: 无新公开项；在测试内完成 §12.8 8 步。

COW 8 步到 host 的映射（全部在测试函数内逐步执行并逐断言）：

1. P 私有 RW（fixture 已建）→ `write_fault(P)` = `Wrote`。
2. C 只读共享（fixture 已建）→ `alpha` 与 `cow_state_consistent(C)` 成立。
3. C 写 → `write_fault(C)` = `Faulted`；`cow_state_consistent(C)` 仍成立（仅 status 变）。
4. 历史错误（已更正）：曾将 `memseg_create` 后的新段误写为“复制原段帧地址集合”。现行 host 语义是分配页数相同且物理帧相离的新 Segment。
5. C 换私写：历史计划曾使用 `memseg_unmap`/`memseg_map` 并手工同步 ops；现由 `CowSliceState::privatize_cow` 统一执行。
6. `exc_resume`：现行组合路径在受保护恢复帧验证后恢复为 Ready，且不隐式调度。
7. 显式 `CowSliceState::schedule(CHILD)` → C Running。
8. 终态检查：组合不变量成立；子新段与父段页数相同、物理帧相离；不对帧字节内容作结论。

**历史“复制帧”纠错。** 旧计划错误地把副本表述为复用原物理帧。现行 host 模型不模拟字节内容复制；私有化只检查页数相同和物理帧相离：

```rust
assert_eq!(parent_frames.len(), copy_frames.len());
assert!(parent_frames.iter().all(|frame| !copy_frames.contains(frame)));
```

这不表示字节内容复制。

- [ ] **Step 1: 写成功路径测试**

测试顶部集中导入（放在文件顶部既有 import 之后）：

```rust
use crate::cow_refine::{CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, base_exc, new_refine, probe_write, resume_frame, write_fault, WriteOutcome};
use crate::memseg_refine::{SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha, memseg_create, memseg_map, memseg_unmap};
use crate::ops;
use crate::state::{ACCESS_R, ACCESS_W, ExceptionKind, TaskStatus};
```

```rust
#[test]
fn historical_cow_roundtrip_archived() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;

    // 1. 父可写，直接写成功
    assert_eq!(write_fault(&mut exc, &mut pf, PARENT, COW_VA), WriteOutcome::Wrote);

    // 2. 子只读共享，一致
    assert!(memseg_alpha(&pf));
    assert!(crate::cow_refine::cow_state_consistent(&pf, &exc, CHILD));

    // 3. 子写只读 → page fault → Faulted
    assert!(matches!(write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
    assert!(crate::cow_refine::cow_state_consistent(&pf, &exc, CHILD));

    // 历史错误（已更正）：副本不复用原物理帧；host 模型仅表示等页数、相离分配。
    let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).unwrap();
    let parent_frames = pf.segments[&1].frames.clone();
    let copy_frames = pf.segments[&copy_seg].frames.clone();
    assert_eq!(parent_frames.len(), copy_frames.len());
    assert!(parent_frames.iter().all(|frame| !copy_frames.contains(frame)));
    // 这不表示字节内容复制。

    // 5. 子换为私写副本
    memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).unwrap();
    memseg_map(&mut pf, rights, true, copy_seg, CHILD, 0, COW_VA, 1,
               ACCESS_R | ACCESS_W).unwrap();
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;
    assert!(crate::cow_refine::cow_state_consistent(&pf, &exc, CHILD));

    // 6. exc_resume：frame Segment(CODE_SEG) 写合法用户帧，帧 cap=FRAME_CAP
    let frame = resume_frame();
    exc.segments.get_mut(&CODE_SEG).unwrap().bytes.copy_from_slice(&frame.encode());
    ops::exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).expect("resume child");
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Ready);

    // 7. schedule 恢复执行
    ops::schedule(&mut exc, CHILD);
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Running);

    // 8. 终态：子私写可写，父仍私写；α 全程成立
    assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::Writable);
    assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
    assert!(crate::cow_refine::cow_state_consistent(&pf, &exc, CHILD));
    assert!(memseg_alpha(&pf));
    // 父子对原共享页的写隔离：父段帧集合未因子复制而丢失
    assert_eq!(pf.segments[&1].frames, parent_frames);
    assert_eq!(pf.segments[&copy_seg].frames, copy_frames);
}
```

- [ ] **Step 2: 确认红灯**

Run: `cargo test -p cosir cow_roundtrip -- --nocapture`
历史预期：旧函数 `cow_roundtrip_proves_no_new_kernel_mechanism` 曾应当在旧计划步骤中缺失/未编译；该名称为历史夸大措辞，不是当前测试或结论。 `resume_frame` 等符号当时由 Task 1 提供。

- [ ] **Step 3: 实现使测试通过**

`resume_frame()` 已在 Task 1 定义为 `pub fn resume_frame() -> SretFrame`（RIP=CODE_VA+0x80、RSP=COW_VA+0x80）。如需调整帧内容，直接编辑该函数；本任务不新增实现代码。

- [ ] **Step 4: 验证并提交**

Run:
```bash
cargo test -p cosir cow_roundtrip -- --nocapture
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS，无 warning。

```bash
git add cosir/src/cow_refine.rs cosir/src/cow_refine_tests.rs
git commit -m "feat: prove COW fork from shared read-only map without new primitives"
```

---

### Task 6: 反例与边界（R1–R4）

**Files:**
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `cow_refine::{CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, base_exc, new_refine, probe_write, resume_frame, write_fault, WriteOutcome}`；`memseg_refine::{SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, memseg_unmap}`；`ops::{exc_resume, schedule}`；`state::{ACCESS_R, ACCESS_W, AuditEvent, ExceptionKind, TaskStatus}`。
- Produces: 无新公开项；各反例测试在文件内实现。测试顶部统一 import：

```rust
use crate::cow_refine::{CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, base_exc, new_refine, probe_write, resume_frame, write_fault, WriteOutcome};
use crate::memseg_refine::{SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, memseg_unmap};
use crate::ops;
use crate::state::{ACCESS_R, ACCESS_W, AuditEvent, ExceptionKind, TaskStatus};
```

- [ ] **Step 1: R2 — exc_resume 的 Task 级原子性隔离**

`ops::PendingException` 按 Task 各自存在；对一个 Task 的 `exc_resume` 只消费其自身 pending，不串扰另一 Task。用父页可写的前提，验证子 fault+恢复不影响父的任何状态。

```rust
#[test]
fn r2_resume_of_child_keeps_parent_state_intact() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let parent_status_before = exc.tasks[&PARENT].status;
    let parent_pending_before = exc.pending_exc.contains_key(&PARENT);
    let parent_vspace_before = exc.tasks[&PARENT].vspace.clone();
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;

    // 子写只读 → fault；父不 fault（父页可写）
    assert!(matches!(write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert!(!exc.pending_exc.contains_key(&PARENT));

    // 子换私写
    let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).unwrap();
    memseg_map(&mut pf, rights, true, copy_seg, CHILD, 0, COW_VA, 1, ACCESS_R | ACCESS_W).unwrap();
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;

    // 子 exc_resume → Ready
    let frame = resume_frame();
    exc.segments.get_mut(&CODE_SEG).unwrap().bytes.copy_from_slice(&frame.encode());
    ops::exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).unwrap();
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Ready);
    assert!(!exc.pending_exc.contains_key(&CHILD));

    // 父的状态、pending、vspace 与 Audit 中父相关均不受影响
    assert_eq!(exc.tasks[&PARENT].status, parent_status_before);
    assert_eq!(exc.pending_exc.contains_key(&PARENT), parent_pending_before);
    assert_eq!(exc.tasks[&PARENT].vspace, parent_vspace_before);
    assert_eq!(exc.tasks[&PARENT].kernel_resume_frame, None);
    assert!(exc.audit.iter().all(|ev| !matches!(ev, AuditEvent::TaskResumed { actor: PARENT, .. })));
}
```

- [ ] **Step 2: R3 — 父子隔离正向**

子已拿私写副本后，父再写自己的私有页不误触 fault。

```rust
#[test]
fn r3_parent_writes_own_page_after_child_copy_does_not_fault() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
    let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).unwrap();
    memseg_map(&mut pf, rights, true, copy_seg, CHILD, 0, COW_VA, 1, ACCESS_R | ACCESS_W).unwrap();
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;

    // 子可写后，父仍写自己的私有页 → Wrote（COW 后正常写不误触）
    assert_eq!(crate::cow_refine::write_fault(&mut exc, &mut pf, PARENT, COW_VA),
               WriteOutcome::Wrote);
    assert_eq!(exc.tasks[&PARENT].status, TaskStatus::Running);
    assert!(!exc.pending_exc.contains_key(&PARENT));
}
```

- [ ] **Step 3: R4 — 内核式 COW 不可表达（诚实边界）**

现有 `ops` 没有"复制页"内核原语；`raise_exception` 只暂停并保存现场。测试断言：**即便在 fault 后，host 也无法在不调用用户态 `memseg_create`+`memseg_map` 序列的情况下让任务继续到写私有页**——即恢复前必须显式换映射，否则 `exc_resume` 后该页仍只读，再写仍 fault。

```rust
#[test]
fn r4_kernel_only_cow_is_not_derivable_from_primitives() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    // 子fault
    assert!(matches!(crate::cow_refine::write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    // 不换映射，直接把只读共享恢复 → 该页仍只读
    let frame = resume_frame();
    exc.segments.get_mut(&CODE_SEG).unwrap().bytes.copy_from_slice(&frame.encode());
    crate::ops::exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).unwrap();
    crate::ops::schedule(&mut exc, CHILD);
    // 再写：仍只读共享，触发第二次 fault —— 说明内核式自动复制不存在
    assert_eq!(crate::cow_refine::probe_write(&pf, CHILD, COW_VA), CowProbe::ReadOnly);
    assert!(matches!(crate::cow_refine::write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
}
```

- [ ] **Step 4: R1 — 并发写需用户态协调（边界记录）**

`memseg`/`ops` 均无互斥或仲裁原语；两个 Task 对同一只读页并发写各有 pending，且无合并语义。断言：host 中两个 Task 对同一共享页的写都会各自 fault（而非合并/原子化），从而**并发 COW 的用户态协调责任被显式暴露**。

```rust
#[test]
fn r1_two_tasks_writing_shared_readonly_page_each_faults() {
    // fixture 里 PARENT 页是 RW，无法对同一 va 同时让两个都不 fault。
    // 用两个任务对同一 COW_VA 都只读的场景不可得（PARENT 是 RW）。
    // 该用例表达：若两个 Task 都只读共享同一页（需第三个只读 Task），并发写各 fault，无合并原语。
    let mut pf = new_refine();
    let mut exc = base_exc();
    // 把 PARENT 也降为只读（模拟两个只读共享者）
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
    memseg_unmap(&mut pf, true, PARENT, COW_VA, 1).unwrap();
    memseg_map(&mut pf, rights, true, 1, PARENT, 0, COW_VA, 1, ACCESS_R).unwrap();
    exc.tasks.get_mut(&PARENT).unwrap().vspace[0].access = ACCESS_R;

    // 两者各自 fault，均无合并：无互斥原语 → 用户态协调是显式责任
    assert!(matches!(crate::cow_refine::write_fault(&mut exc, &mut pf, PARENT, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert!(matches!(crate::cow_refine::write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert!(exc.pending_exc.contains_key(&PARENT));
    assert!(exc.pending_exc.contains_key(&CHILD));
}
```

- [ ] **Step 5: 验证并提交**

Run:
```bash
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS，无 warning。

```bash
git add cosir/src/cow_refine_tests.rs
git commit -m "test: record COW concurrency and kernel-COW inexpressibility boundaries"
```

---

### Task 7: main.rs smoke 与文档、tracker 同步

**Files:**
- Modify: `cosir/src/main.rs`
- Modify: `Prima OS 实现阶段.md`
- Modify: `COSIR 原语规范.md`

- [ ] **Step 1: main.rs 加 COW smoke**

在 `main()` 末尾（memseg 冒烟 block 之后）追加：

```rust
    // COW 组合协议冒烟：共享只读 → 写触发 fault → 复制换私写 → exc_resume
    {
        use cow_refine::{CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, base_exc, new_refine, probe_write, resume_frame, write_fault};
        use memseg_refine::{SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha, memseg_create, memseg_map, memseg_unmap};
        use ops::exc_resume;
        use state::{ACCESS_R, ACCESS_W};
        let mut pf = new_refine();
        let mut exc = base_exc();
        let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
        assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
        assert!(matches!(write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                         cow_refine::WriteOutcome::Faulted(state::ExceptionKind::PageFault { .. })));
        let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("copy seg");
        let parent_frames = pf.segments[&1].frames.clone();
        let copy_frames = pf.segments[&copy_seg].frames.clone();
        assert_eq!(parent_frames.len(), copy_frames.len());
        assert!(parent_frames.iter().all(|frame| !copy_frames.contains(frame)));
        // 这不表示字节内容复制。
        memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).expect("unmap read-only");
        memseg_map(&mut pf, rights, true, copy_seg, CHILD, 0, COW_VA, 1,
                   ACCESS_R | ACCESS_W).expect("map private");
        exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;
        assert!(cow_refine::cow_state_consistent(&pf, &exc, CHILD));
        let frame = resume_frame();
        exc.segments.get_mut(&CODE_SEG).unwrap().bytes.copy_from_slice(&frame.encode());
        exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).expect("resume child");
        assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::Writable);
        assert!(memseg_alpha(&pf));
        println!("PASS cow shared read-only probe");
        println!("PASS cow write-page-fault");
        println!("PASS cow copy-and-remap");
        println!("PASS cow exc_resume roundtrip");
        println!("PASS model-test cow fork archived-historical-scenario");
    }
```

Run:
```bash
cargo run -p cosir
```
Expected（历史验收记录，非当前证据）：输出新增四条 `PASS cow ...` + 一条 `PASS model-test cow fork archived-historical-scenario`，且程序退出 0。该输出不证明 COW 可表达性或无需新增内核原语。

- [ ] **Step 2: tracker 加证据条目**

在 `Prima OS 实现阶段.md` 证据索引表新增一行 `SPEC-015`，类型 `MODEL-TEST + REFINEMENT-DESIGN + 反例`，日期 2026-08-14，证据边界明示支持与反例结果。并新增已知问题表行（如有边界发现则记录；若 R4 成立为边界澄清，非反例，写入"已知问题与纠偏计划"表或决策日志）。

- [ ] **Step 3: COSIR §12.8 证据边界注释**

历史错误（已弃用，非当前指令）：本计划曾要求在 `COSIR 原语规范.md` §12.8 追加“host 模型 `cow_refine` 下通过 `MODEL-TEST` 证明可表达 COW 且无需新增内核原语”的表述。该强主张不成立；host `MODEL-TEST` 仅记录受限场景观察，不能证明 COW 可表达性或无需新增内核原语。反例 R1/R4 的边界及当前证据范围以权威设计和 tracker 为准，且不宣称 `KERNEL-TRACE`/`REFINEMENT-PROOF`。

- [ ] **Step 4: 检查并提交**

Run:
```bash
git diff --check
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS。

```bash
git add cosir/src/main.rs "Prima OS 实现阶段.md" "COSIR 原语规范.md"
git commit -m "docs: record COW combination protocol model evidence"
```

---

### Task 8: 本切片总验收

**Files:**
- Modify only if recording evidence: `Prima OS 实现阶段.md`

- [ ] **Step 1: 运行总验收**

```bash
cargo fmt --all --check
cargo test -p cosir
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

Expected: 全部退出码 0；xtask 仅证明启动闭环无回归。记录实际测试数量。

- [ ] **Step 2: 独立验证**

调用 verification agent，传入 COW 切片原始目标、改动文件、接口与全部 ISSUE/R 用例；要求复核：成功路径 8 步、`probe_write` 无副作用、`write_fault` 的 `Wrote/Faulted` 分支、`cow_state_consistent` 正反例、R1–R4 各边界断言是否正确引用既有模型，且未改动 `ops.rs`/`memseg_refine.rs`/`state.rs`/`pagewalk.rs` 语义。Expected verdict: PASS。

- [ ] **Step 3: 记录实际证据**

仅在 PASS 后，把实际测试数量、命令与直接支持命题写入 tracker 的 `SPEC-015`。若 R4 成立，明确标注为对 §12.8"用户态 COW"的边界澄清（非 H1 反例）；若某步需新增原语，记录为 H1 反例并进入研究停止条件。

- [ ] **Step 4: 提交证据记录（如有变化）**

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: record verified COW model evidence"
```

---

## Explicit Non-Goals

- 不修改 `ops.rs`、`memseg_refine.rs`、`pagewalk.rs`、`state.rs` 的既有语义，不新增 POSIR 一等原语。
- 不实现真实 IDT/trap/访问指令/真实内存复制；`write_fault` 是 host 近似。
- 不实现用户态调度策略或并发仲裁原语；R1 只记录"互斥需用户态协调"这一边界。
- 不横跨另一硬件架构；不把 host 结果提升为真实内核或形式化证据。
