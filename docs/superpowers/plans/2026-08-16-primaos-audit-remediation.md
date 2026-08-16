# PrimaOS Audit Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复规范治理、COW 前态、单核调度、TLB R/W/X 与生命周期、跨模型手工同步和 MemorySegment 裸授权入口，同时保持证据等级不被放大。

**Architecture:** 保留 `RefineState`、`ops::State`、`TlbState` 三个局部机制模型，以新增 `CowSliceState` 作为唯一组合状态和 COW 迁移入口。Capability adapter 在组合边界完成 Authority 验证，低层 MemorySegment 函数只接受已经验证的对象 ID；所有组合迁移采用“完整验证—clone/pending state—单次提交—组合不变量复核”。

**Tech Stack:** Rust nightly-2026-07-30、Cargo workspace、无额外依赖、QEMU 11.x、Markdown 权威规范与 tracker。

## Global Constraints

- 本计划只产生 host `MODEL-TEST` 与既有 `BOOT-PROTOCOL-TEST`；不得宣称 `KERNEL-TRACE`、`REFINEMENT-PROOF` 或 `COMPAT-TEST`。
- 不关闭 BLK-001，不实现真实 CR3/PTE、trap、`invlpg`、`iretq`、SMP shootdown 或帧字节复制。
- 允许破坏性调整内部 Rust API；不得保留长期兼容 shim 或第二套调度/授权入口。
- COW host 私有化只保证相同页数和物理帧集合相离，不声称内容字节已复制。
- 所有失败路径必须保持完整 `CowSliceState` 不变。
- 正常业务测试不得直接修改子状态；只有 mutation/invariant 测试可故意破坏状态。
- 当前两处 Markdown 表格格式化变更必须保留；三份权威文档统一为 LF。
- 不新增第三方 crate。
- 本计划不执行 git commit；只有用户明确授权后才能提交。

---

## File Map

**核心状态与权限**

- Modify: `cosir/src/state.rs` — Capability object/rights、Task/Segment/Exception 共享类型。
- Modify: `cosir/src/ops.rs` — 保留异常局部迁移，移除无 `current` 语义的公开调度入口。
- Modify: `cosir/src/memseg_refine.rs` — 仅保留无裸授权参数的 verified 低层映射操作。

**TLB**

- Modify: `cosir/src/tlb_refine.rs` — `AccessKind`、Task 生命周期权威、地址验证、幂等失效和 Task 清理。
- Modify: `cosir/src/tlb_refine_tests.rs` — R/W/X 矩阵与生命周期测试。

**COW 组合层**

- Modify: `cosir/src/cow_refine.rs` — `CowSliceState`、`CowError`、Authority adapter、调度、COW 建立/访问/私有化/恢复/销毁。
- Modify: `cosir/src/cow_refine_tests.rs` — 组合状态 TDD、失败原子性和完整切片测试。
- Modify: `cosir/src/main.rs` — 冒烟迁移到组合 API，移除手工同步和夸大测试名。
- Modify: `cosir/src/memseg_refine_tests.rs` — 删除裸权限参数测试，保留低层地址/allocator/双视图测试。

**规范与治理**

- Modify: `Prima OS.md` — 原语统计口径、COW 候选映射、版本日期。
- Modify: `COSIR 原语规范.md` — W1–W16、`SegMapW`、COW 证据边界、版本日期。
- Modify: `Prima OS 实现阶段.md` — issue、证据、最新规范修订和最终验收。
- Modify: `docs/superpowers/specs/2026-08-15-tlb-refine-design.md` — 去除证明性措辞。
- Modify: `docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md` — 状态改为已批准。
- Modify: `docs/superpowers/plans/2026-08-14-cow-refinement-slice.md` — Completed/Archived、历史错误断言。
- Modify: `docs/superpowers/plans/2026-08-15-tlb-refine.md` — Completed/Archived、历史 API/措辞。

---

### Task 1: 冻结规范口径并登记修复项

**Files:**
- Modify: `Prima OS.md:3-11,203-243,411-427,913`
- Modify: `COSIR 原语规范.md:2-12,114-142,1358-1376,1411-1418,1871-1881,2052-2067`
- Modify: `Prima OS 实现阶段.md:31-47,51-86,101-114`
- Modify: `docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md:1-8`

**Interfaces:**
- Consumes: 已批准设计中的证据等级和完成定义。
- Produces: 后续代码任务必须遵守的唯一规范口径；tracker 中状态为 `In Progress` 的 `ISSUE-DOC-001`、`ISSUE-COW-003`、`ISSUE-TLB-001`、`ISSUE-DOC-002`。

- [ ] **Step 1: 将批准设计状态改为已批准**

将：

```markdown
> 状态：待用户书面批准
```

改为：

```markdown
> 状态：已批准
```

- [ ] **Step 2: 统一原语类别统计口径**

在 `Prima OS.md` 当前机制基线处写入唯一口径：

```markdown
本文按 **11 类 Bootstrap 后常规机制 + 1 个 Bootstrap 初始化迁移** 统计接口类别。§19 总览因此列出 12 项；其中 Bootstrap 是初始 Authority 的唯一来源，不是系统引导完成后的常规可调用操作。H1 中的“11 类”仅指 Bootstrap 后常规机制，不包含该唯一初始化迁移。
```

同步将 `COSIR 原语规范.md` §19 和 `Prima OS 实现阶段.md` 的 `SPEC-001` 改为同一口径。保留历史决策日志，但注明旧数字是当时的常规机制计数。

- [ ] **Step 3: 修正规范义务和恢复帧权限术语**

将：

```markdown
每个成功迁移保持 W1–W14
```

改为：

```markdown
每个成功迁移保持 W1–W16。对不涉及 ExceptionSource 或 Device 的迁移，可以通过未修改投影保持与受影响投影保持分解证明，但成功结果仍须满足完整 WellFormed。
```

将 `exc_resume` 前置条件中的未定义 `SegWrite` 改为：

```markdown
`sret_frame_seg_cap` 必须由调用者持有、处于 active、指向调用者拥有 `SegMapW` Authority 且内核可读取 bytes 的 Segment；内核读取仅用于受保护的 `exc_resume` 实现，不新增用户可见 `SegWrite` 权限。
```

- [ ] **Step 4: 收窄 COW/TLB 证据措辞**

COW 统一采用：

```markdown
在明确限定的 host 组合场景中，MODEL-TEST 未发现为“共享只读映射 → 写保护 fault → 用户态显式分配相离新 Segment 并换映射 → exc_resume → 显式调度”新增 POSIR 一等原语的需求。该结论不覆盖帧字节复制、真实 trap/PTE/TLB、多核协议、KERNEL-TRACE 或 REFINEMENT-PROOF。
```

TLB 统一采用：

```markdown
host MODEL-TEST 仅观察到该 per-task、无替换策略缓存模型中的非自动刷新与显式失效行为；不外推为真实硬件 TLB、CR3/PCID、invlpg/INVPCID、多核 shootdown 或内存顺序结论。
```

- [ ] **Step 5: 在 tracker 登记问题但暂不标 Fixed**

新增以下行，状态均为 `In Progress`：

```markdown
| ISSUE-DOC-001 | 原语类别口径、W1–W16 义务与 SegWrite 术语不一致 | 本轮规范审计 | 本轮 | 三份权威文档口径一致；规范无 W1–W14/SegWrite 残留 | In Progress |
| ISSUE-COW-003 | COW 默认父可写、双 Running、双模型手工同步且证据措辞过强 | 源码审查 + MODEL-TEST 边界审计 | 本轮 | 组合状态与新测试通过；无 prove/完整 COW 外推 | In Progress |
| ISSUE-TLB-001 | 非写访问未区分 R/X，失效 API 的 Task/地址语义不一致 | 源码审查 | 本轮 | AccessKind 矩阵、幂等失效与地址测试通过 | In Progress |
| ISSUE-DOC-002 | 已完成计划未归档且含过期帧复用断言 | 历史计划审计 | 本轮 | 两计划 Archived；错误断言不可执行 | In Progress |
```

- [ ] **Step 6: 验证规范文本**

Run:

```bash
git diff --check
git diff -- "Prima OS.md" "COSIR 原语规范.md" "Prima OS 实现阶段.md" "docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md"
```

Expected: 无 whitespace error；只包含批准范围内的规范、日期、issue 和既有表格格式化变更。

Run with the dedicated content-search tool equivalent to these patterns:

```text
W1–W14
SegWrite
证明可表达 COW
无需新增内核原语
```

Expected: 权威文档无未解释残留。

- [ ] **Step 7: Review checkpoint**

确认本任务只冻结规范和登记 issue；不得提前将任何实现 issue 标为 `Fixed`。不提交。

---

### Task 2: 建立不重叠的 Capability rights 与 verified MemorySegment 边界

**Files:**
- Modify: `cosir/src/state.rs:30-50`
- Modify: `cosir/src/memseg_refine.rs:1-20,75-120,254-411`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `cosir/src/cow_refine.rs:1-30`
- Modify: `cosir/src/cow_refine_tests.rs`

**Interfaces:**
- Consumes: `TaskId`、`SegId`、`CapId`、`Capability`、`RefineState`、`MemSegError`。
- Produces:

```rust
pub const CAP_SEG_MAP_R: u8 = 0b0000_0010;
pub const CAP_SEG_MAP_W: u8 = 0b0000_0100;
pub const CAP_SEG_MAP_X: u8 = 0b0000_1000;
pub const CAP_SEG_DESTROY: u8 = 0b0001_0000;
pub const CAP_TASK_MANAGE_VSPACE: u8 = 0b0010_0000;

pub enum CapObject {
    ExceptionSource(ExcSourceId),
    Segment(SegId),
    Task(TaskId),
}

pub(crate) fn map_verified(
    st: &mut RefineState,
    seg_id: SegId,
    task_id: TaskId,
    seg_offset: u64,
    target_base: VirtAddr,
    page_count: u64,
    access: u8,
) -> Result<(), MemSegError>;
```

`unmap_verified` 与 `destroy_verified` 同样删除调用者自报 rights/bool。

- [ ] **Step 1: 写 Capability 常量与 object 类型的编译失败测试**

在 `cow_refine_tests.rs` 添加：

```rust
#[test]
fn capability_right_domains_do_not_overlap() {
    assert_eq!(CAP_EXC_SUBSCRIBE & CAP_SEG_MAP_R, 0);
    assert_eq!(CAP_SEG_MAP_R | CAP_SEG_MAP_W | CAP_SEG_MAP_X, 0b0000_1110);
    assert_eq!(CAP_SEG_DESTROY & CAP_TASK_MANAGE_VSPACE, 0);
    let object = CapObject::Task(CHILD);
    assert_eq!(object, CapObject::Task(CHILD));
}
```

- [ ] **Step 2: 运行测试确认红灯**

Run:

```bash
cargo test -p cosir capability_right_domains_do_not_overlap -- --nocapture
```

Expected: FAIL to compile because `CAP_SEG_MAP_R/W/X` and `CapObject::Task` do not exist.

- [ ] **Step 3: 实现不重叠 rights 与 Task object**

在 `state.rs` 保留现有 `CAP_EXC_SUBSCRIBE`，新增上述常量和 `CapObject::Task(TaskId)`；将现有 `CAP_SEG_WRITE` 的所有调用迁移为规范已定义的 `CAP_SEG_MAP_W` 后删除 `CAP_SEG_WRITE`，避免继续保留未定义的平行 Authority。

- [ ] **Step 4: 将低层映射 API 改为 verified primitive**

将 `memseg_map` 改名为 `map_verified` 并删除：

```rust
seg_cap_rights: u8,
task_manage_rights: bool,
```

将 `memseg_unmap` 改名为 `unmap_verified` 并删除裸权限形参；将 `memseg_destroy` 改名为 `destroy_verified` 并删除 `seg_destroy_right: bool`。

保留这些函数现有的地址、范围、overlap、allocator 和双视图检查；不要在低层重新添加 Capability 查询。

- [ ] **Step 5: 迁移低层模型测试**

删除或迁移只验证 `false`/`0 rights` 的测试，例如 `map_missing_seg_map_right_rejected`。保留：

```rust
#[test]
fn verified_map_rejects_overlap_atomically() {
    let mut st = fixture();
    map_verified(&mut st, SEG, TASK, 0, VA, 1, ACCESS_R).unwrap();
    let before = st.clone();
    assert_eq!(
        map_verified(&mut st, SEG, TASK, 0, VA, 1, ACCESS_R),
        Err(MemSegError::Overlap)
    );
    assert_eq!(st, before);
}
```

- [ ] **Step 6: 运行局部测试并修复所有签名连锁**

Run:

```bash
cargo test -p cosir memseg_refine_tests -- --nocapture
cargo test -p cosir capability_right_domains_do_not_overlap -- --nocapture
```

Expected: PASS。允许其他模块暂时出现编译错误的时间不得跨越本步骤；在步骤结束前迁移 COW/TLB fixture 到新 low-level 名称。

- [ ] **Step 7: Review checkpoint**

搜索并确认生产接口中不存在 `seg_cap_rights`、`task_manage_rights`、`seg_destroy_right`。不提交。

---

### Task 3: 修复 TLB AccessKind、地址验证和 Task 生命周期

**Files:**
- Modify: `cosir/src/tlb_refine.rs:1-95`
- Modify: `cosir/src/tlb_refine_tests.rs:1-170`
- Modify: `cosir/src/main.rs:286-320`

**Interfaces:**
- Consumes: `RefineState`、`State.tasks`、`CachedXlat`、`validate_page_vaddr`。
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    Read,
    Write,
    Execute,
}

pub fn access_allowed(xl: &CachedXlat, kind: AccessKind) -> bool;

pub fn translate(
    memory: &RefineState,
    tasks: &std::collections::HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
    va: VirtAddr,
) -> Result<CachedXlat, TlbError>;

pub fn invalidate_va(
    tasks: &std::collections::HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
    va: VirtAddr,
) -> Result<(), TlbError>;

pub fn flush_all(
    tasks: &std::collections::HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
) -> Result<(), TlbError>;

pub fn remove_task(tlb: &mut TlbState, task: TaskId);
```

- [ ] **Step 1: 写 R/W/X 矩阵红灯测试**

```rust
#[test]
fn access_kind_matrix_enforces_each_bit_independently() {
    let cases = [
        (ACCESS_R, [true, false, false]),
        (ACCESS_W, [false, true, false]),
        (ACCESS_X, [false, false, true]),
        (ACCESS_R | ACCESS_X, [true, false, true]),
    ];
    for (access, expected) in cases {
        let xl = CachedXlat { frame_phys: 0x1000, access };
        assert_eq!(access_allowed(&xl, AccessKind::Read), expected[0]);
        assert_eq!(access_allowed(&xl, AccessKind::Write), expected[1]);
        assert_eq!(access_allowed(&xl, AccessKind::Execute), expected[2]);
    }
}
```

- [ ] **Step 2: 运行矩阵测试确认红灯**

Run:

```bash
cargo test -p cosir access_kind_matrix_enforces_each_bit_independently -- --nocapture
```

Expected: FAIL to compile because `AccessKind` does not exist and `access_allowed` still accepts `bool`.

- [ ] **Step 3: 写生命周期与地址红灯测试**

```rust
#[test]
fn invalidate_live_task_without_cache_is_idempotent() {
    let (memory, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    assert_eq!(invalidate_va(&ops.tasks, &mut tlb, TASK, VA), Ok(()));
    assert!(!has_tlb(&tlb, TASK, VA));
    assert!(memory.tasks.contains_key(&TASK));
}

#[test]
fn invalidate_rejects_unaligned_address_atomically() {
    let (_, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    let before = tlb.clone();
    assert_eq!(
        invalidate_va(&ops.tasks, &mut tlb, TASK, VA + 1),
        Err(TlbError::UnalignedAddress)
    );
    assert_eq!(tlb, before);
}
```

另加 `flush_live_task_without_cache_is_idempotent`、noncanonical、missing task、`remove_task` 测试。

- [ ] **Step 4: 运行生命周期测试确认红灯**

Run:

```bash
cargo test -p cosir tlb_refine_tests::invalidate_ -- --nocapture
cargo test -p cosir tlb_refine_tests::flush_live_task_without_cache_is_idempotent -- --nocapture
```

Expected: 旧 API 对无 cache Task 返回 `NoSuchTask`，且不验证地址。

- [ ] **Step 5: 实现 AccessKind 和统一生命周期语义**

实现：

```rust
pub fn access_allowed(xl: &CachedXlat, kind: AccessKind) -> bool {
    let required = match kind {
        AccessKind::Read => ACCESS_R,
        AccessKind::Write => ACCESS_W,
        AccessKind::Execute => ACCESS_X,
    };
    xl.access & required != 0
}
```

`invalidate_va` 先调用 `validate_page_vaddr`，再以 `tasks.contains_key` 判断 Task；合法无 cache 时直接 `Ok(())`。`flush_all` 同理。`remove_task` 幂等删除 cache。

- [ ] **Step 6: 迁移全部 TLB 调用点**

把所有：

```rust
access_allowed(&xl, true)
access_allowed(&xl, false)
```

改为 `AccessKind`。迁移 `translate`、`invalidate_va`、`flush_all` 的 fixture 和 `main.rs` smoke，使其提供权威 Task 表。

- [ ] **Step 7: 运行 TLB 与全 crate 测试**

Run:

```bash
cargo test -p cosir tlb_refine_tests -- --nocapture
cargo test -p cosir
```

Expected: 所有 TLB 测试通过；无 `bool` access 调用。

- [ ] **Step 8: Review checkpoint**

确认 cache existence 不再代表 Task existence；PTE 改动仍不会自动刷新 cache。不提交。

---

### Task 4: 引入 CowSliceState、Authority adapter 和单核 schedule

**Files:**
- Modify: `cosir/src/cow_refine.rs:1-295`
- Modify: `cosir/src/cow_refine_tests.rs`
- Modify: `cosir/src/ops.rs:195-203`
- Modify: `cosir/src/main.rs`

**Interfaces:**
- Consumes: Task 2 rights/verified primitives；Task 3 `AccessKind`/TLB APIs。
- Produces:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CowSliceState {
    pub memory: RefineState,
    pub ops: State,
    pub tlb: TlbState,
    pub current: Option<TaskId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CowError {
    NoSuchTask,
    NoSuchSegment,
    NoSuchCapability,
    InactiveOrStaleCapability,
    CapabilityHolderMismatch,
    CapabilityObjectMismatch,
    CapabilityRightDenied,
    InvalidTaskState,
    CpuBusy,
    InvalidAddress,
    MappingConflict,
    TlbLifecycleInconsistent,
    CorruptCompositeState,
    MemSeg(MemSegError),
    Resume(ResumeError),
    Tlb(TlbError),
}

impl CowSliceState {
    pub fn new_cow_fixture() -> Self;
    pub fn check_invariants(&self) -> Result<(), CowError>;
    pub fn yield_current(&mut self, task: TaskId) -> Result<(), CowError>;
    pub fn schedule(&mut self, task: TaskId) -> Result<(), CowError>;
}
```

- [ ] **Step 1: 写单核 schedule 红灯测试**

```rust
#[test]
fn fresh_slice_has_one_current_running_parent() {
    let slice = CowSliceState::new_cow_fixture();
    assert_eq!(slice.current, Some(PARENT));
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Running);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Ready);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn schedule_rejects_busy_cpu_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(slice.schedule(CHILD), Err(CowError::CpuBusy));
    assert_eq!(slice, before);
}
```

另加 unknown task、non-ready、预存第二个 Running 被识别为 corrupt 的测试。

- [ ] **Step 2: 运行 schedule 测试确认红灯**

Run:

```bash
cargo test -p cosir cow_refine_tests::fresh_slice_has_one_current_running_parent -- --nocapture
cargo test -p cosir cow_refine_tests::schedule_ -- --nocapture
```

Expected: `CowSliceState` 和 `CowError` 未定义；旧 fixture 有两个 Running。

- [ ] **Step 3: 实现组合 fixture 和 schedule**

`new_cow_fixture()` 建立父 `Running/current=Some(PARENT)`、子 `Ready`。初始阶段父拥有私有 RW 数据映射，子尚未映射 `COW_VA`。

`yield_current(task)` 是 `task_yield` 的组合迁移：只接受 `current == Some(task)` 且 Task 为 `Running`，原子执行 `Running → Ready` 与 `current = None`；失败保持完整组合状态不变。

`schedule` 必须：

```rust
pub fn schedule(&mut self, task: TaskId) -> Result<(), CowError> {
    self.check_invariants()?;
    if self.current.is_some() {
        return Err(CowError::CpuBusy);
    }
    let next = self.ops.tasks.get(&task).ok_or(CowError::NoSuchTask)?;
    if next.status != TaskStatus::Ready {
        return Err(CowError::InvalidTaskState);
    }
    let mut pending = self.clone();
    pending.ops.tasks.get_mut(&task).unwrap().status = TaskStatus::Running;
    pending.current = Some(task);
    pending.check_invariants()?;
    *self = pending;
    Ok(())
}
```

- [ ] **Step 4: 写组合不变量 mutation 测试**

新增测试故意破坏：

- 第二个 Running；
- `current` 指向 Ready；
- memory/ops Task 集不一致；
- mapping access/Segment/frame 不一致；
- dead Task TLB；
- Faulted 无 pending 或仍为 current。

每个测试只在 mutation 后调用 `check_invariants()`，正常流程不得复用这些 mutation helper。

- [ ] **Step 5: 实现 check_invariants**

检查批准设计 §4.2 的九项。现有 host 表示若无法直接从 `ops::PageMapping` 取得 Segment/frame，应将组合层维护的映射关系作为唯一可检查 metadata，而不是假装仅比较 access 就是完整关系。任何无法建立完整关系的正常状态返回 `CorruptCompositeState`。

- [ ] **Step 6: 实现私有 Authority adapter**

```rust
struct MappingAuthority {
    seg_id: SegId,
    task_id: TaskId,
}

fn authorize_mapping(
    ops: &State,
    memory: &RefineState,
    actor: TaskId,
    seg_cap: CapId,
    task_cap: CapId,
    requested: u8,
) -> Result<MappingAuthority, CowError>;
```

验证顺序固定为：requested bits → segment cap existence/active/holder/object/live target/rights → task cap existence/active/holder/object/live target/right → `requested ⊆ allowed_access`。

- [ ] **Step 7: 写并通过 Authority 原子性测试**

对 missing、foreign holder、wrong object、inactive/stale、missing map right、missing task-manage、allowed_access 越界分别：

```rust
let before = slice.clone();
assert_eq!(operation(&mut slice, ...), Err(expected));
assert_eq!(slice, before);
```

Run:

```bash
cargo test -p cosir cow_refine_tests::authority_ -- --nocapture
cargo test -p cosir cow_refine_tests::check_invariants_ -- --nocapture
cargo test -p cosir cow_refine_tests::schedule_ -- --nocapture
```

Expected: PASS。

- [ ] **Step 8: 删除旧公开 scheduler**

COW 路径不得再调用 `ops::schedule`。若没有非 COW 的合法调用点，删除它及 `main.rs` 中对应局部测试；所有调度验证转移到 `CowSliceState::schedule`。

- [ ] **Step 9: Review checkpoint**

确认 `current` 只存在于 `CowSliceState`，没有第二权威来源。正常 fixture 只有一个 Running。 不提交。

---

### Task 5: 实现 COW 建立与 fault/current 迁移

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`
- Modify: `cosir/src/ops.rs:17-46`

**Interfaces:**
- Consumes: `CowSliceState`、`authorize_mapping`、`map_verified`、`unmap_verified`、Task 3 TLB APIs。
- Produces:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessOutcome {
    Allowed(CachedXlat),
    Faulted(ExceptionKind),
}

impl CowSliceState {
    pub fn establish_cow(
        &mut self,
        actor: TaskId,
        parent: TaskId,
        child: TaskId,
        seg_cap: CapId,
        child_task_cap: CapId,
        va: VirtAddr,
    ) -> Result<(), CowError>;

    pub fn access(
        &mut self,
        task: TaskId,
        va: VirtAddr,
        kind: AccessKind,
    ) -> Result<AccessOutcome, CowError>;
}
```

- [ ] **Step 1: 写 COW 建立红灯测试**

```rust
#[test]
fn establish_cow_makes_parent_and_child_readonly_on_same_frame() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.establish_cow(PARENT, PARENT, CHILD, DATA_CAP, CHILD_TASK_CAP, COW_VA).unwrap();
    let parent = slice.mapping(PARENT, COW_VA).unwrap();
    let child = slice.mapping(CHILD, COW_VA).unwrap();
    assert_eq!(parent.access, ACCESS_R);
    assert_eq!(child.access, ACCESS_R);
    assert_eq!(parent.seg_id, child.seg_id);
    assert_eq!(parent.frame_phys, child.frame_phys);
    assert_eq!(slice.check_invariants(), Ok(()));
}
```

另加 invalid address、child overlap、parent 非 RW/private、bad authority、失败完整状态不变测试。

- [ ] **Step 2: 运行建立测试确认红灯**

Run:

```bash
cargo test -p cosir cow_refine_tests::establish_cow_ -- --nocapture
```

Expected: `establish_cow`/完整 mapping projection 不存在；旧 fixture 预先映射 child 且 parent 保持 RW。

- [ ] **Step 3: 实现原子 establish_cow**

顺序：

1. `self.check_invariants()?`；
2. 完整验证 actor、parent/child、Capabilities、地址、父私有 RW 映射、子无 overlap；
3. clone 为 `pending`；
4. child 建立原 Segment 的 R 映射；
5. parent unmap+map 为 R；
6. 同步更新 ops mapping projection；
7. 对父子地址执行幂等 invalidate；
8. `pending.check_invariants()?`；
9. `*self = pending`。

- [ ] **Step 4: 写父子首次写 fault 红灯测试**

```rust
#[test]
fn parent_first_write_after_cow_faults_and_releases_cpu() {
    let mut slice = established_cow_fixture(PARENT);
    assert_eq!(
        slice.access(PARENT, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        }))
    );
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Faulted);
    assert!(slice.ops.pending_exc.contains_key(&PARENT));
}
```

对 child：先让 parent yield/结束当前运行态，再显式 `schedule(CHILD)`，之后写入；不得构造双 Running。

- [ ] **Step 5: 运行 fault 测试确认红灯**

Run:

```bash
cargo test -p cosir cow_refine_tests::parent_first_write_after_cow_faults_and_releases_cpu -- --nocapture
cargo test -p cosir cow_refine_tests::child_first_write_after_cow_faults_and_releases_cpu -- --nocapture
```

Expected: 旧 `write_fault` 隐式 schedule，且不会管理 `current`。

- [ ] **Step 6: 实现 access/fault 迁移**

`access` 要求 `current == Some(task)` 且 Task 为 Running。TLB/page walk 允许访问时返回 `Allowed`；权限拒绝时 clone 组合状态、调用 `raise_exception` 保存 frame、设置 Faulted/pending、清 `current`、检查不变量并提交。禁止调用 `schedule`。

- [ ] **Step 7: 运行 COW 建立和 fault 测试**

Run:

```bash
cargo test -p cosir cow_refine_tests::establish_cow_ -- --nocapture
cargo test -p cosir cow_refine_tests::parent_first_write_ -- --nocapture
cargo test -p cosir cow_refine_tests::child_first_write_ -- --nocapture
cargo test -p cosir
```

Expected: PASS；默认 COW 路径不存在父 RW 特权。

- [ ] **Step 8: Review checkpoint**

确认任何 COW 首次写都经过 fault；无隐式 schedule；失败比较完整组合状态。 不提交。

---

### Task 6: 实现私有化、恢复、显式重调度与 Task/TLB 清理

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`
- Modify: `cosir/src/tlb_refine.rs`

**Interfaces:**
- Consumes: Task 5 faulted state；现有 `ops::exc_resume`。
- Produces:

```rust
impl CowSliceState {
    pub fn create_segment(
        &mut self,
        actor: TaskId,
        page_count: u64,
        allowed_access: u8,
    ) -> Result<CapId, CowError>;

    pub fn privatize_cow(
        &mut self,
        actor: TaskId,
        faulted_task: TaskId,
        private_seg_cap: CapId,
        task_cap: CapId,
        va: VirtAddr,
    ) -> Result<(), CowError>;

    pub fn resume(
        &mut self,
        caller: TaskId,
        exc_source_cap: CapId,
        frame_seg_cap: CapId,
        frame_offset: usize,
    ) -> Result<(), CowError>;

    pub fn destroy_task(&mut self, task: TaskId) -> Result<(), CowError>;
}
```

- [ ] **Step 1: 写私有化红灯测试**

```rust
#[test]
fn privatize_cow_uses_equal_page_count_and_disjoint_frames() {
    let mut slice = faulted_child_fixture();
    let old_frames = slice.cow_frames(CHILD, COW_VA).unwrap();
    let private_cap = slice.create_segment(CHILD, old_frames.len() as u64, ACCESS_R | ACCESS_W).unwrap();
    slice.privatize_cow(CHILD, CHILD, private_cap, CHILD_SELF_TASK_CAP, COW_VA).unwrap();
    let new_frames = slice.cow_frames(CHILD, COW_VA).unwrap();
    assert_eq!(old_frames.len(), new_frames.len());
    assert!(old_frames.iter().all(|frame| !new_frames.contains(frame)));
    assert_eq!(slice.mapping(CHILD, COW_VA).unwrap().access, ACCESS_R | ACCESS_W);
}
```

明确不比较 frame bytes。

- [ ] **Step 2: 运行私有化测试确认红灯**

Run:

```bash
cargo test -p cosir cow_refine_tests::privatize_cow_ -- --nocapture
```

Expected: API 不存在；旧 roundtrip 依靠直接 memseg/ops mutation。

- [ ] **Step 3: 实现 create_segment 与 privatize_cow**

`create_segment` 使用 allocator 创建 Segment，并为 actor 铸造 root Segment Capability；rights 由 `allowed_access` 映射到 `CAP_SEG_MAP_R/W/X`，并含 `CAP_SEG_DESTROY`。

`privatize_cow` 要求 Task 为 Faulted/current None、pending fault 地址/原因匹配、private cap 和 task cap 合法。clone 后执行旧映射移除、新私有 RW 映射、ops projection 更新、TLB 地址失效、组合检查和提交。

- [ ] **Step 4: 写 resume/schedule 红灯测试**

```rust
#[test]
fn resume_requires_explicit_schedule_before_running_again() {
    let mut slice = privatized_child_fixture();
    slice.resume(CHILD, EXC_CAP, FRAME_CAP, 0).unwrap();
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Ready);
    assert_eq!(slice.current, None);
    slice.schedule(CHILD).unwrap();
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Running);
    assert_eq!(slice.current, Some(CHILD));
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Allowed(_))
    ));
}
```

- [ ] **Step 5: 实现组合 resume**

clone 组合状态，委派 `ops::exc_resume`；成功后断言 Task 为 Ready 且 `current` 仍为 None；检查组合不变量后提交。不得自动调度。

- [ ] **Step 6: 写并实现 Task 销毁/TLB 清理测试**

```rust
#[test]
fn destroy_ready_task_removes_all_task_state_and_tlb() {
    let mut slice = ready_child_with_tlb_fixture();
    slice.destroy_task(CHILD).unwrap();
    assert!(!slice.memory.tasks.contains_key(&CHILD));
    assert!(!slice.ops.tasks.contains_key(&CHILD));
    assert!(!slice.tlb_has_task(CHILD));
    assert!(slice.task_capabilities(CHILD).all(|cap| !cap.active));
}
```

拒绝 current/Faulted/pending Task，失败完整状态不变。成功时清 memory、ops、TLB，并使 `CapObject::Task(task)` capability inactive/stale。

- [ ] **Step 7: 运行私有化、恢复、销毁与全 crate 测试**

Run:

```bash
cargo test -p cosir cow_refine_tests::privatize_cow_ -- --nocapture
cargo test -p cosir cow_refine_tests::resume_ -- --nocapture
cargo test -p cosir cow_refine_tests::destroy_task_ -- --nocapture
cargo test -p cosir
```

Expected: PASS。

- [ ] **Step 8: Review checkpoint**

确认恢复后必须显式 schedule；销毁不会留下 TLB；无字节复制外推。 不提交。

---

### Task 7: 删除手工同步入口并迁移所有测试与 smoke

**Files:**
- Modify: `cosir/src/cow_refine.rs`
- Modify: `cosir/src/cow_refine_tests.rs`
- Modify: `cosir/src/tlb_refine_tests.rs`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `cosir/src/main.rs:147-320`

**Interfaces:**
- Consumes: Tasks 2–6 的最终组合 API。
- Produces: 没有旧 `cow_state_consistent`、`write_fault`、裸权限参数或 `bool` access 的单一实现路径。

- [ ] **Step 1: 写最终端到端 MODEL-TEST**

```rust
#[test]
fn cow_slice_child_write_fault_private_resume_schedule_succeeds() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.establish_cow(PARENT, PARENT, CHILD, DATA_CAP, CHILD_TASK_CAP, COW_VA).unwrap();
    slice.yield_current(PARENT).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    slice.privatize_cow(CHILD, CHILD, private, CHILD_SELF_TASK_CAP, COW_VA).unwrap();
    slice.write_valid_resume_frame_for_test(CHILD).unwrap();
    slice.resume(CHILD, EXC_CAP, FRAME_CAP, 0).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Allowed(_))
    ));
    assert_eq!(slice.check_invariants(), Ok(()));
}
```

这里必须调用 Task 4 定义的合法 `yield_current` 迁移；不得增加 test-only 的 status/current 直接写入口。

- [ ] **Step 2: 重写 R1/R4 测试**

R1 使用合法顺序：parent schedule/write fault → current None → child schedule/write fault。R4 在不调用 `privatize_cow` 时，resume+schedule 后再次写仍 fault；不得依靠非法双 Running fixture。

- [ ] **Step 3: 删除旧符号与夸大测试名**

删除：

```text
cow_state_consistent
CowProbe
probe_write
WriteOutcome
write_fault
cow_roundtrip_proves_no_new_kernel_mechanism
```

新测试名必须描述场景，不使用 `prove`/`proves`。

- [ ] **Step 4: 清除正常测试和 smoke 中的直接 mutation**

正常流程不得直接执行：

```rust
slice.ops.tasks.get_mut(...).vspace[...] = ...;
map_verified(...);
unmap_verified(...);
slice.ops.segments.get_mut(...).bytes[...] = ...;
```

恢复帧写入使用组合层 test fixture API，该 API 必须验证 owner/Segment/offset 并保持不变量。

- [ ] **Step 5: 迁移 main.rs smoke**

COW smoke 只调用组合 API，并输出边界准确的标记，例如：

```text
PASS model-test cow limited host composition
```

TLB smoke 使用 `AccessKind::{Read, Write, Execute}` 和权威 Task 表。QEMU boot protocol 文案不变。

- [ ] **Step 6: 运行源码搜索验收**

使用 dedicated content-search tool 验证下列模式无生产调用：

```text
cow_state_consistent
probe_write
write_fault
seg_cap_rights
task_manage_rights
seg_destroy_right
access_allowed(..., true)
access_allowed(..., false)
_proves_
```

历史归档文档可出现旧符号，但必须位于 `Deprecated assumptions` 或“历史错误”说明内。

- [ ] **Step 7: 运行全 workspace 测试**

Run:

```bash
cargo fmt --all --check
cargo test -p cosir
cargo test --workspace
cargo clippy --workspace --all-targets --no-deps -- -D warnings
```

Expected: 全部退出码 0；无 warning。

- [ ] **Step 8: Review checkpoint**

逐个检查 COW 正常路径只通过 `CowSliceState`；局部 mutation 只存在于明确命名的 invariant corruption 测试。 不提交。

---

### Task 8: 归档历史计划、同步证据并完成最终验证

**Files:**
- Modify: `docs/superpowers/specs/2026-08-15-tlb-refine-design.md:140-149`
- Modify: `docs/superpowers/plans/2026-08-14-cow-refinement-slice.md:1-20,564-575,608-613,825-858`
- Modify: `docs/superpowers/plans/2026-08-15-tlb-refine.md:1-20,411-431`
- Modify: `Prima OS.md`
- Modify: `COSIR 原语规范.md`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: Tasks 2–7 的 fresh 测试结果和最终符号名称。
- Produces: 已归档计划、状态为 Fixed 的 tracker issue、准确 evidence boundary、LF 权威文档。

- [ ] **Step 1: 给 COW/TLB 计划添加统一归档头**

COW：

```markdown
> Status: Completed / Archived
>
> 本文件是 2026-08-14 的历史实施计划，不再是当前可执行指令。当前治理与后继要求以 `docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md` 和 `Prima OS 实现阶段.md` 为权威。
>
> Evidence: SPEC-015
>
> Implementation commits: `518670e`, `43d940b`, `18aac5f`, `1357a4d`, `a65fce6`, `5827d75`, `54f932f`
>
> Deprecated assumptions: “证明完整 COW fork”、无条件“无需新增内核机制”、副本复用原物理帧、测试手工同步双状态。
```

TLB 使用 `SPEC-016` 和提交 `570fc60`, `9ea01ad`, `23603ea`, `45a90e3`；废弃 `proving T1`、`for_write: bool`、硬件 TLB 外推和独立状态手工同步。

- [ ] **Step 2: 修正 COW 历史错误断言**

将历史：

```rust
assert_eq!(parent_frames, copy_frames);
```

替换为显式历史纠错说明和现行 host 语义：

```rust
assert_eq!(parent_frames.len(), copy_frames.len());
assert!(parent_frames.iter().all(|frame| !copy_frames.contains(frame)));
```

注明这只表示页数相同与物理帧相离，不表示字节内容复制。

- [ ] **Step 3: 清理全部证据措辞**

测试、规范、白皮书、tracker、TLB design/plan 中不再使用 `prove`/“证明”描述普通 `MODEL-TEST`。保留历史 commit message 时必须明确标为历史措辞，不得作为当前结论。

- [ ] **Step 4: 更新 tracker issue 和证据**

只有 Tasks 2–7 的验证通过后，才把：

```text
ISSUE-DOC-001
ISSUE-COW-003
ISSUE-TLB-001
ISSUE-DOC-002
```

改为 `Fixed`，并记录本轮实际测试计数、工具版本、验收命令和明确不支持的证据等级。不要修改阶段 0/1 未满足退出条件的状态。

- [ ] **Step 5: 统一三份权威文档 LF**

确保：

```bash
git ls-files --eol -- "Prima OS.md" "COSIR 原语规范.md" "Prima OS 实现阶段.md"
```

Expected after normalization: worktree 行尾均为 LF；保留已批准的两处表格格式化，不产生无关全文件重排。

- [ ] **Step 6: 运行最终文本审计**

使用 dedicated content-search tool 搜索：

```text
SegWrite
W1–W14
证明完整 COW
证明可表达 COW
无需新增内核原语
proving T1
_proves_
access_allowed(..., true|false)
seg_cap_rights
task_manage_rights
```

Expected: 当前规范和源码无不合格残留；归档计划中的旧词仅存在于明确的 Deprecated/历史错误说明。

- [ ] **Step 7: 运行 fresh 完整验收**

Run exactly:

```bash
cargo fmt --all --check
cargo test -p cosir
cargo test --workspace
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo xtask test
git diff --check
git status --short
```

Expected:

- 所有 Rust tests 通过，0 failed；
- clippy 0 warnings；
- QEMU BIOS run 1/2 和 2/2 通过；
- `git diff --check` 无错误；
- 工作区只包含本计划批准范围内的源码、测试和文档改动。

- [ ] **Step 8: 独立审查**

请求独立 reviewer 逐项核对：

1. 默认 COW 父子均只读；
2. 最多一个 Running 且与 current 一致；
3. fault 清 current，resume 不调度；
4. TLB 严格区分 R/W/X；
5. 存活无 cache Task 的失效幂等；
6. MemorySegment/COW 不接受裸权限 bool；
7. 正常路径无双状态手工同步；
8. 文档没有把 MODEL-TEST 升格。

Reviewer 若发现 Critical/Important 问题，保持对应任务未完成并修复后重新运行 Step 7。

- [ ] **Step 9: Final checkpoint**

汇报实际测试数量、QEMU 结果、残留边界和工作区 diff；不得提交，除非用户随后明确要求。
