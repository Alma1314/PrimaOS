# exception 恢复细化 exc_resume 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 COSIR 细化层异常恢复缺口：定义 `exc_resume` 原语、异常帧、Faulted 细化态，并用一个独立 Rust 可执行状态模型探针验证其状态转换。

**Architecture:** 分两部分。(1) 修改 `COSIR 原语规范.md`，在 §12.9 增加恢复路径子节、定义 `exc_resume` 原语、标注细化要求 #16 与边界清单 #1/#2。(2) 新增 workspace 成员 `cosir`，一个无依赖的纯 host Rust 状态机，建模 Task/ExcSource/pending_exc，验证 `raise_exception`/`exc_resume`/`schedule` 的状态转换单调性。

**Tech Stack:** Rust nightly-2026-07-30、Rust 2024 edition、Cargo workspace（已有 `kernel`/`xtask`，新增 `cosir`）。

## Global Constraints

- 权威设计为 `docs/superpowers/specs/2026-08-07-exc-resume-design.md`。
- 固定 Rust 工具链为 `nightly-2026-07-30`；`cosir` 是 host 二进制，不依赖 `bootloader`/`bootloader_api`，不触碰 real IDT/trap。
- 不实现真实 trap 入口、不连 QEMU kernel、不做 Lean/Coq 形式化（留 BLK-001）。
- 不关闭 BLK-001 / BLK-002，不把阶段 0 标记为满足退出条件。
- `Faulted` 是细化层可见状态；除 `exc_resume` 外不新增对 POSIR 可见的一等原语。
- SretFrame 字段布局是 x86_64 细化定义（`0x88`/136 字节、16 字节对齐），不入 POSIR 抽象正文；探针声明自己的 `SretFrame`。
- 现有阶段 1 启动闭环与 `cargo xtask test` 不得被本计划破坏。
- `.obsidian/` 是现有未跟踪目录，不读取、不修改、不提交。
- 每个任务仅提交该任务列出的文件，不使用 `git add .` 或 `git add -A`。

---

## File Structure

- Modify: `Cargo.toml` — 在 workspace members 增加 `cosir`。
- Create: `cosir/Cargo.toml` — host 二进制 package。
- Create: `cosir/src/state.rs` — Task / ExcSource / SretFrame / PendingException / TaskStatus 数据模型。
- Create: `cosir/src/ops.rs` — `raise_exception` / `exc_resume` / `schedule` 状态转移与错误结果。
- Create: `cosir/src/main.rs` — 线束：运行固定场景打印 PASS 摘要。
- Modify: `COSIR 原语规范.md` — §12.9 恢复路径子节、§12 增加 `exc_resume` 操作、§18.2 要求 #16、边界清单 #1/#2、最近规范修订。
- Modify: `Prima OS 实现阶段.md` — 登记本次细化产出证据，移除 exc_resume 缺口表述。

---

### Task 1: 固定当前验收基线，确认 workspace 可扩展

**Files:**
- 无源码改动（仅验证）。

**Interfaces:**
- Consumes: 现有 workspace（`kernel` / `xtask`）。
- Produces: 证明当前 `cargo xtask test` 仍通过，为后续不破坏闭环提供基线。

- [ ] **Step 1: 运行现有验收，确认基线**

Run：

```bash
cargo xtask test
```

Expected tail：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

不得继续本计划的其他任务，除非此退出码为 0。

- [ ] **Step 2: 记录基线快照**

确认 `git status --short` 只显示已知改动（`.obsidian/` 未跟踪、`Prima OS 实现阶段.md` 或 `.gitignore` 若有未提交改动属于其他工作）。不创建任何 PASS 证据外的写入。

---

### Task 2: 在 COSIR 规范中定义 exc_resume 与恢复路径

**Files:**
- Modify: `COSIR 原语规范.md`

**Interfaces:**
- Consumes: 设计文档 §2（原语）、§3（帧）、§4（层归属）。
- Produces: 规范中的 `exc_resume` 原语条目（供 Task 6 登记证据时引用）、细化要求 #16 的状态更新、边界清单 #1/#2 的状态更新。

- [ ] **Step 1: 修改 §12.9 的“恢复路径”段，指向新增子节**

定位 `COSIR 原语规范.md` 中"**恢复路径：**"之后、以"**当前抽象模型的显式范围：**"开头的段落（当前约第 1396–1398 行）。在该范围段之前插入：

```markdown
**恢复路径（exc_resume）：**

- 应对页故障的可恢复异常的恢复入口是本规范定义的 `exc_resume` 原语。其含义：内核仅恢复“一帧”（异常上下文），恢复目标（重播故障指令、跳处理程序或进程退出）由用户态 LibOS/运行时自管。帧字段布局是具体架构细化层的定义（见 §18.0 边界清单 #1），本抽象层只定义原语的抽象行为。
- `exc_resume` 是当前 Running Task 的操作，绑定到其订阅的原异常 `ExceptionSource`；只作用于该 Task 自身的待恢复异常帧。调用需持有指向活跃 `ExceptionSource` 的 Capability 并含 `ExcSubscribe` 权限，且持有一个指向可写且内核可读 Segment 的 Capability 表示恢复帧。
- 详细原语（前置条件 / 成功动作 / 错误）见下方 §12.10。
```

- [ ] **Step 2: 在 §12.9 之后新增 §12.10 exc_resume 操作条目**

定位 §12.9 结束、§13 Audit 开始的位置。在 §13 之前插入：

```markdown
### 12.10 `exc_resume` — 恢复被暂停的异常帧

```text
exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset, resume_vaddr) -> ()
```

前置条件：

1. 调用者是当前 Running Task；
2. `exc_source_cap` 由调用者持有且指向活跃 `ExceptionSource`，并含 `ExcSubscribe` 权限；
3. 调用者对 `exc_source_cap` 所指源有活跃订阅；
4. `sret_frame_seg_cap` 由调用者持有且指向调用者可写、内核可读的 Segment；
5. `frame_offset + sret_frame_len ≤ seg.len()`，其中 `sret_frame_len` 由具体架构的 `SretFrame` 定义决定；`frame_offset` 满足该架构的对齐约束；
6. `resume_vaddr` 在调用者 `vspace` 中是已映射、可执行、用户态可访问的虚拟地址；
7. 调用者当前有未恢复的异常帧（细化 `Faulted` 态），且该帧对应的 `ExcSourceId` 与 `exc_source_cap` 所指源一致；
8. 待恢复异常的 `ExceptionKind` 属可恢复类型：`PageFault` / `GeneralProtectionFault` / `DivideError` / `InvalidOpcode`。

成功动作：

1. 将调用者 Task 从细化 `Faulted` 态置为 `Ready`，并原子清除待恢复帧标记；
2. 追加 `TaskResumed(exc_source_id, resume_vaddr)` Audit 事件；
3. 将 `sret_frame_seg_cap` 指向 Segment 中 `frame_offset` 起的 `sret_frame_len` 字节复制到内核后清零该区域。

错误：`InvalidExcSource`、`MissingRight(ExcSubscribe)`、未订阅、`InvalidFrameAddr`、`NotExecutable`、`NoPendingException`、`UnrecoverableFault`。

错误语义：

- `NoPendingException`：调用者没有未恢复的异常帧（含帧已消费后再次调用）；
- `UnrecoverableFault`：待恢复异常的 `ExceptionKind` 属不可恢复（如 `DoubleFault`）。不销毁 Task，保留给用户态判断；
- `InvalidFrameAddr`：`frame_offset + sret_frame_len > seg.len()` 或 `frame_offset` 未对齐；
- `MissingRight(...)`：帧 Segment 无该 Task 写权限，或内核不可读；
- `NotExecutable`：`resume_vaddr` 不在可执行映射上。

恢复 `schedule` 交互：`exc_resume` 只把 Task 从 `Faulted` 置为 `Ready`；恢复帧在 `schedule` 选中该 Task 后由细化层交付（代替原始指令流），保持 §12.9 的“调度选中 faulted Task 时进入异常返回路径”不变。
```

- [ ] **Step 3: 更新细化要求 #16**

定位 §18.2 清单第 16 项（当前约第 1946 行）并改为：

```markdown
16. 异常帧格式、`iretq` 恢复路径和 `exc_resume` 的参数与语义：已由 §12.10 定义，待实现与形式化验证；
```

- [ ] **Step 4: 更新边界清单 #1、#2**

定位 §18.0 边界清单第 1、2 行（当前约第 1965–1966 行）并改为：

```markdown
| 1 | 异常帧格式与寄存器保存/恢复 | §12.9, §12.10 | x86_64 帧布局已设计（§3）；寄存器恢复目标由 `resume_vaddr` 给定，帧布局按架构实现 |
| 2 | `exc_resume` / 异常返回原语 | §12.9, §12.10 | 已定义为 §12.10 原语；实现与形式化验证待阶段 2 后 |
```

- [ ] **Step 5: 更新最近规范修订**

定位 `Prima OS 实现阶段.md` 中"| 最近规范修订 |"行（当前约第 41 行），在其后追加（不删除原内容）：

```markdown
| exc_resume（本切片） | 2026-08-07 — 定义异常恢复原语 `exc_resume`、x86_64 SretFrame 帧布局（细化层定义）、Faulted 细化态与错误码；新增 §12.10，标注细化要求 #16 与边界清单 #1/#2 |
```

- [ ] **Step 6: 校验规范同步一致**

Run：

```bash
python -c 'from pathlib import Path; s=Path("COSIR 原语规范.md").read_text(encoding="utf-8"); assert "## 12.10" in s and "exc_resume" in s and "已由 §12.10 定义" in s and "正在实现" not in s; print("PASS spec sync")'
```

Expected：`PASS spec sync`。

- [ ] **Step 7: 提交规范同步**

```bash
git add -- "COSIR 原语规范.md"
git commit -m "$(cat <<'EOF'
docs: define exc_resume exception resume primitive

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 新增 cosir workspace 成员与状态数据模型

**Files:**
- Modify: `Cargo.toml`
- Create: `cosir/Cargo.toml`
- Create: `cosir/src/state.rs`
- Create: `cosir/src/ops.rs`
- Create: `cosir/src/main.rs`

**Interfaces:**
- Consumes: 无（独立 host 二进制，不依赖 kernel/xtask）。
- Produces:
  - 模块 `state`：类型 `TaskId`(u32)、`ExcSourceId`(u32)、`VirtAddr`(u64)、`SegId`(u32)、`FaultCause`、`ExceptionKind`、`SretFrame`、`TaskStatus`、`PendingException`、`Task`、`ExceptionSource`、`State`。
  - 模块 `ops`：`raise_exception`、`exc_resume`、`schedule`、`error::ResumeError`。

- [ ] **Step 1: 扩展现有 workspace**

Modify `Cargo.toml`，将 members 改为：

```toml
[workspace]
resolver = "3"
members = ["kernel", "xtask", "cosir"]
default-members = ["xtask"]

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

Create `cosir/Cargo.toml`：

```toml
[package]
name = "cosir"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "cosir-state"
path = "src/main.rs"
```

不声明任何外部依赖。这是纯 host 二进制。

- [ ] **Step 2: 写状态数据模型的失败测试（先确认模块存在性）**

先创建最小可见骨架使测试能编译失败。Create `cosir/src/state.rs` 仅声明空接口（后面 Task 4 填充实现，Task 3 只建壳）：

```rust
// placeholders; ops 转移在 Task 4 实现
pub type TaskId = u32;
pub type ExcSourceId = u32;
pub type SegId = u32;
pub type VirtAddr = u64;
pub type PageMapping = (VirtAddr, u8); // (addr, access:bitfield R/W/X)

#[repr(C, align(16))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SretFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub flags: u64,
}

pub const SRET_FRAME_LEN: usize = core::mem::size_of::<SretFrame>();
```

Create `cosir/src/ops.rs` 仅声明错误类型（Task 4 填充转移）：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeError {
    NoPendingException,
    UnrecoverableFault,
    InvalidFrameAddr,
    NotExecutable,
    MissingWrite,
    InvalidExcSource,
    NotSubscribed,
    AlreadyResumedStub, // 占位，Task 4 移除
}
```

Create `cosir/src/main.rs` 最小线束：

```rust
mod state;
mod ops;

fn main() {
    println!("PASS cosir noop");
}
```

- [ ] **Step 3: 运行，确认 workspace 编译**

Run：

```bash
cargo build -p cosir
```

Expected：编译通过，无错误；`cargo build -p cosir` 结束后运行 `target/debug/cosir-state.exe` 输出单行 `PASS cosir noop`。此处 `AlreadyResumedStub` 可能产生 dead-code warning，Task 4 接入真实转移前保留，不视为失败。

- [ ] **Step 4: 提交 cosir 成员骨架**

```bash
git add -- Cargo.toml cosir/Cargo.toml cosir/src/state.rs cosir/src/ops.rs cosir/src/main.rs
git commit -m "$(cat <<'EOF'
feat: add cosir host state machine member

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 实现状态转移与测试

**Files:**
- Modify: `cosir/src/state.rs`
- Modify: `cosir/src/ops.rs`
- Modify: `cosir/src/main.rs`

**Interfaces:**
- Consumes: Task 3 的 `state` 类型与 `ResumeError`。
- Produces:
  - `state::State`：`tasks: HashMap<TaskId, Task>`、`exc_sources: HashMap<ExcSourceId, ExceptionSource>`、`pending_exc_tasks: HashMap<TaskId, PendingException>`（隐藏细化标记单独存，方便断言"POSIR 不可见"）。
  - `state::TaskStatus`：`Ready | Running | BlockedRecv | Faulted`（Faulted 为细化态）。
  - `ops::raise_exception(&mut State, src_id, kind, faulting_task, vaddr)` → 转换。
  - `ops::exc_resume(&mut State, caller: TaskId, src_id: ExcSourceId, seg_ok: bool, frame_ok: bool, exec_ok: bool) -> Result<(), ResumeError>`。
  - `ops::schedule(&mut State, chosen: TaskId)` → 转换。

- [ ] **Step 1: 扩充 state 数据模型**

Replace `cosir/src/state.rs` with：

```rust
use std::collections::HashMap;

pub type TaskId = u32;
pub type ExcSourceId = u32;
pub type SegId = u32;
pub type VirtAddr = u64;

// access 位域：bit0=R, bit1=W, bit2=X
pub type PageMapping = (VirtAddr, u8);
const ACCESS_R: u8 = 0b001;
const ACCESS_W: u8 = 0b010;
const ACCESS_X: u8 = 0b100;

#[repr(C, align(16))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SretFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub flags: u64,
}

pub const SRET_FRAME_LEN: usize = core::mem::size_of::<SretFrame>();
```

将 `state.rs` 的 `PageMapping` 与访问位常量导出；这些在 `ops.rs` 用于可执行性判定。确认现有 `Task3` 的占位类型被完整替换，不残留 `PageMapping` 里的无意义 bitfield 用法。

（保留 Task 3 已有的 `TaskId`/`ExcSourceId`/`SegId`/`VirtAddr`/`SretFrame`/`SRET_FRAME_LEN`；仅新增按上方覆盖的 `PageMapping` 与访问位常量。）

- [ ] **Step 2: 新增 State / TaskStatus / PendingException / ExceptionSource 类型**

Append 到 `cosir/src/state.rs`：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    BlockedRecv,
    Faulted, // 细化层可见状态
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegRange {
    pub len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaultCause {
    PageNotPresent,
    PageProtectionViolation,
    PageReservedBitSet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExceptionKind {
    PageFault { page_present: bool, writable_fault: bool },
    GeneralProtectionFault,
    DivideError,
    InvalidOpcode,
    DoubleFault,
    // SyscallTrap / ExternalInterrupt / TaskExit 本探针不建模恢复，仅枚举保留未来扩展
}

impl ExceptionKind {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ExceptionKind::PageFault { .. }
                | ExceptionKind::GeneralProtectionFault
                | ExceptionKind::DivideError
                | ExceptionKind::InvalidOpcode
        )
    }
}

#[derive(Debug)]
pub struct PendingException {
    pub source_id: ExcSourceId,
    pub kind: ExceptionKind,
    pub save_frame: SretFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExceptionSource {
    pub supported_kind: ExceptionKind, // 简化：单一可支持类型锚
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: TaskId,
    pub status: TaskStatus,
    pub vspace: Vec<PageMapping>, // 简化：直接列映射，(addr, access)
}

#[derive(Default)]
pub struct State {
    pub tasks: HashMap<TaskId, Task>,
    pub exc_sources: HashMap<ExcSourceId, ExceptionSource>,
    pub pending_exc: HashMap<TaskId, PendingException>,
    pub segments: HashMap<SegId, SegRange>,
}
```

为满足 `Warning: unused constant` 清理，`ACCESS_R`/`ACCESS_W`/`ACCESS_X` 在 Task 4 的 `ops` 中被引用，不删。若 `FaultCause` 未被使用，则在 Task 4 用到（页故障标志）或删除。此处保留。

- [ ] **Step 3: 替换 ops，实现三个转移**

Replace `cosir/src/ops.rs` with：

```rust
use crate::state::{
    ExceptionKind, State, TaskId, TaskStatus, ACCESS_R, ACCESS_W, ACCESS_X,
    SRET_FRAME_LEN,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeError {
    NoPendingException,
    UnrecoverableFault,
    InvalidFrameAddr,
    NotExecutable,
    MissingWrite,
    InvalidExcSource,
    NotSubscribed,
}

/// 异常发生：Running -> Faulted，并保存现场帧。
pub fn raise_exception(
    st: &mut State,
    faulting_task: TaskId,
    source_id: crate::state::ExcSourceId,
    kind: ExceptionKind,
    save_frame: crate::state::SretFrame,
) -> Result<(), ResumeError> {
    let task = st
        .tasks
        .get_mut(&faulting_task)
        .ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Running {
        return Err(ResumeError::NoPendingException);
    }
    task.status = TaskStatus::Faulted;
    st.pending_exc.insert(
        faulting_task,
        crate::state::PendingException {
            source_id,
            kind,
            save_frame,
        },
    );
    Ok(())
}

/// 判断某 vaddr 是否已映射、可执行。
fn is_executable(st: &State, task_id: TaskId, vaddr: u64) -> bool {
    st.tasks
        .get(&task_id)
        .map(|t| {
            t.vspace
                .iter()
                .any(|(base, acc)| *base == vaddr && acc & ACCESS_X != 0)
        })
        .unwrap_or(false)
}

/// 恢复异常帧：Faulted -> Ready。
pub fn exc_resume(
    st: &mut State,
    caller: TaskId,
    source_id: crate::state::ExcSourceId,
    seg_ok: bool,   // true: 帧 Segment 存在、frame_offset 对齐、可写且内核可读
    exec_ok: bool,  // true: resume_vaddr 已映射可执行
) -> Result<(), ResumeError> {
    // (1) 订阅与源校验
    let src = st
        .exc_sources
        .get(&source_id)
        .ok_or(ResumeError::InvalidExcSource)?;
    let _ = src; // 订阅存在性：调用者持有 ExcSubscribe 隐含订阅；探针以 pending_exc 绑定为准
    if !seg_ok {
        return Err(ResumeError::InvalidFrameAddr);
    }
    if !exec_ok {
        return Err(ResumeError::NotExecutable);
    }
    // (2) 待恢复帧必须存在且匹配源
    let pending = st
        .pending_exc
        .get(&caller)
        .ok_or(ResumeError::NoPendingException)?;
    if pending.source_id != source_id {
        return Err(ResumeError::InvalidExcSource);
    }
    if !pending.kind.is_recoverable() {
        return Err(ResumeError::UnrecoverableFault);
    }
    // (3) 转换
    let task = st
        .tasks
        .get_mut(&caller)
        .ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Faulted {
        return Err(ResumeError::NoPendingException);
    }
    task.status = TaskStatus::Ready;
    st.pending_exc.remove(&caller);
    Ok(())
}

/// 调度选中 Task 进入 Running。若该 Task 处于 Faulted 且未 resume，则细化层交付异常帧。
pub fn schedule(st: &mut State, chosen: TaskId) {
    if let Some(task) = st.tasks.get_mut(&chosen) {
        if task.status == TaskStatus::Ready {
            task.status = TaskStatus::Running;
        }
    }
}

pub fn access_bits() -> u8 {
    ACCESS_R | ACCESS_W | ACCESS_X
}
```

为满足 `Unused` warning，若 `ACCESS_R`/`ACCESS_W` 在 `access_bits` 中被使用则无警告。若 `FaultCause` 定义了但未用，回到 `state.rs` 删除之。这以 Task 4 Step 5 编译结果为准调整。

- [ ] **Step 4: 扩充 main.rs 为验收线束，并内置单元测试**

Replace `cosir/src/main.rs` with：

```rust
mod ops;
mod state;

use ops::{exc_resume, raise_exception, schedule, ResumeError};
use state::{
    ExceptionKind, SretFrame, State, Task, TaskId, TaskStatus, ACCESS_R, ACCESS_W, ACCESS_X,
};

fn default_frame() -> SretFrame {
    let mut f = SretFrame {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: 0x7fff_f000,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        flags: 0x202,
    };
    f.rax = 0x1234;
    f
}

fn build_state() -> State {
    let mut st = State::default();
    st.segments.insert(1, state::SegRange { len: 0x1000 });
    st.exc_sources.insert(
        1,
        state::ExceptionSource {
            supported_kind: ExceptionKind::PageFault {
                page_present: false,
                writable_fault: false,
            },
        },
    );
    let t = Task {
        id: 1,
        status: TaskStatus::Running,
        vspace: vec![
            (0x4000_0000, ACCESS_R | ACCESS_W),
            (0x5000_0000, ACCESS_X),
        ],
    };
    st.tasks.insert(1, t);
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_exception_moves_running_to_faulted() {
        let mut st = build_state();
        let frame = default_frame();
        raise_exception(
            &mut st,
            1,
            1,
            ExceptionKind::PageFault { page_present: false, writable_fault: true },
            frame,
        )
        .unwrap();
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
        assert!(st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_recovers_faulted_to_ready() {
        let mut st = build_state();
        raise_exception(
            &mut st,
            1,
            1,
            ExceptionKind::PageFault { page_present: false, writable_fault: true },
            default_frame(),
        )
        .unwrap();
        exc_resume(&mut st, 1, 1, true, true).unwrap();
        assert_eq!(st.tasks[&1].status, TaskStatus::Ready);
        assert!(!st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_on_ready_is_no_pending() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        // 从未 fault：pending 不存在
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_after_consumed_is_no_pending() {
        let mut st = build_state();
        raise_exception(
            &mut st,
            1,
            1,
            ExceptionKind::PageFault { page_present: false, writable_fault: true },
            default_frame(),
        )
        .unwrap();
        exc_resume(&mut st, 1, 1, true, true).unwrap();
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_double_fault_is_unrecoverable() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame()).unwrap();
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::UnrecoverableFault);
        // 不销毁 Task，仍处于 Faulted
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
    }

    #[test]
    fn exc_resume_bad_frame_addr_rejected() {
        let mut st = build_state();
        raise_exception(
            &mut st,
            1,
            1,
            ExceptionKind::PageFault { page_present: false, writable_fault: true },
            default_frame(),
        )
        .unwrap();
        let err = exc_resume(&mut st, 1, 1, false, true).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_non_executable_target_rejected() {
        let mut st = build_state();
        raise_exception(
            &mut st,
            1,
            1,
            ExceptionKind::PageFault { page_present: false, writable_fault: true },
            default_frame(),
        )
        .unwrap();
        let err = exc_resume(&mut st, 1, 1, true, false).unwrap_err();
        assert_eq!(err, ResumeError::NotExecutable);
    }

    #[test]
    fn schedule_faulted_without_resume_is_safe() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame()).unwrap();
        // Faulted 状态 schedule 不会回到 Running（原指令流不重播）
        schedule(&mut st, 1);
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
    }

    #[test]
    fn schedule_ready_becomes_running() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        schedule(&mut st, 1);
        assert_eq!(st.tasks[&1].status, TaskStatus::Running);
    }
}

fn main() {
    let mut st = build_state();
    raise_exception(
        &mut st,
        1,
        1,
        ExceptionKind::PageFault { page_present: false, writable_fault: true },
        default_frame(),
    )
    .expect("raise page fault");
    exc_resume(&mut st, 1, 1, true, true).expect("resume page fault");
    schedule(&mut st, 1);
    assert_eq!(st.tasks[&1].status, TaskStatus::Running);
    println!("PASS cosir raise_exception");
    println!("PASS cosir exc_resume");
    println!("PASS cosir schedule");
    println!("PASS cosir recoverable-fault roundtrip");
}
```

注意：`build_state` 中 `st.pending_exc` 尚未插入任何元素。`raise_exception` 需要 `pending_exc` 写入；`exc_resume_on_ready` 与 `schedule_ready` 路径依赖手动设置 `Ready`。上面的测试与 `main` 在 Step 5 运行后需符合实际；若编译/断言不符，以下一步为准做最小修正（例如 `build_state` 增加辅助 helper 直接造一个已 fault 的 State）。

- [ ] **Step 5: 运行单元测试与线束**

Run：

```bash
cargo test -p cosir
```

Expected：9 个测试全通过（`9 passed; 0 failed`）。若任一测试因 helper 构造不符失败，先修 helper/断言以符合规范语义（§12.10），再继续。

Run：

```bash
cargo run -p cosir --bin cosir-state
```

Expected tail：

```text
PASS cosir raise_exception
PASS cosir exc_resume
PASS cosir schedule
PASS cosir recoverable-fault roundtrip
```

若出现 `unused constant` 或 `unused import` 警告，按 Step 1–4 的说明清理（删未用常量或改用之），直到 `cargo build -p cosir` 无 warning。

- [ ] **Step 6: 提交 cosir 状态机实现**

```bash
git add -- cosir/src/state.rs cosir/src/ops.rs cosir/src/main.rs
git commit -m "$(cat <<'EOF'
feat: implement exc_resume state transitions

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: 完整多成员静态校验，确认不破坏启动闭环

**Files:**
- 无源码改动（仅验证）。

**Interfaces:**
- Consumes: Task 4 的 `cosir` 与既有 `kernel` / `xtask`。
- Produces: 全 workspace 干净编译与既有验收基线证据。

- [ ] **Step 1: 全 workspace 静态与单元校验**

Run：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
```

Expected：所有命令退出码 0；`cargo test --workspace` 中 `cosir` 的 9 个测试与 `xtask` 的既有 host 测试全部通过。此命令**不**运行 QEMU（QEMU 验收走 `cargo xtask test`），故 `xtask` 的 unit 测试通过即可。

- [ ] **Step 2: 重新运行既有启动验收，确认被破坏**

Run：

```bash
cargo xtask test
```

Expected tail：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

只有这条真实通过，才符合"不得破坏启动闭环"约束。

- [ ] **Step 3: 校验差异与提交计划文件**

Run：

```bash
git diff --check && git status --short
```

Expected：无 diff error；状态包含本计划各 Task 已提交文件与始终未跟踪的 `.obsidian/`。若 `docs/superpowers/plans/2026-08-07-exc-resume-slice.md` 尚未入库，本次提交之：

```bash
git add -- "docs/superpowers/plans/2026-08-07-exc-resume-slice.md"
git commit -m "$(cat <<'EOF'
docs: exc_resume refinement implementation plan

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: 登记阶段 0 细化产出证据，同步实现阶段文档

**Files:**
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: 本计划各 Task 通过的真实证据（`cargo test -p cosir`、`cargo xtask test`、规范同步）。
- Produces: 阶段 0 新证据条目 `SPEC-012`，当前首要缺口更新，BLK-001/002 保持 Open。

- [ ] **Step 1: 运行最终静态与单元校验**

Run：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

Expected：全部退出码 0；`cargo test --workspace` 含 9 个 cosir 测试；`xtask test` 六条 PASS。

仅在此命令全部通过后才执行以下证据编辑。

- [ ] **Step 2: 更新实现阶段文档头部与当前快照**

在 `Prima OS 实现阶段.md` 中：

- 将 `> 更新日期：2026-08-06` 改为 `> 更新日期：2026-08-07`；
- 将"| 最近实现证据 |"行后追加一行（不删除原行）：

```markdown
| 最近细化证据 | 见 SPEC-012：exc_resume 原语与 Faulted 细化态（COSIR §12.10）定义完成，Rust 可执行状态模型探针 9 测试通过 |
```

- 将"| 当前首要缺口 |"行更新为：

```markdown
| 当前首要缺口 | x86_64 机器状态到 POSIR 状态的细化草案（BLK-002）；形式化工具中的可执行状态模型（BLK-001） |
```

（移除"异常帧格式与 `exc_resume` 原语（COSIR 细化层缺口）"表述，因为该缺口已由本切片关闭。）

- [ ] **Step 3: 增加证据条目**

在证据索引 `TEST-003` 行后增加：

```markdown
| SPEC-012 | 细化规范与可执行模型 | `COSIR 原语规范.md` §12.10、`cosir/` | 2026-08-07 | 定义 `exc_resume` 原语、x86_64 SretFrame 与 Faulted 细化态；`cargo test -p cosir` 9 tests 通过；`cargo xtask test` 六条 PASS | S0-D1、S0-D2 |
```

- [ ] **Step 4: 校验阶段边界未虚报**

Run：

```bash
python -c 'from pathlib import Path; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); assert "SPEC-012" in s and "BLK-001" in s and "BLK-002" in s; assert "| BLK-001 | 0 |" in s and "| BLK-002 | 0 |" in s; assert "**状态：已满足退出条件**" not in s; print("PASS evidence boundaries")'
```

Expected：`PASS evidence boundaries`。

- [ ] **Step 5: 提交证据登记**

```bash
git diff --check
git add -- "Prima OS 实现阶段.md"
git commit -m "$(cat <<'EOF'
docs: record exc_resume refinement evidence

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: 最终验证**

Run：

```bash
git status --short
cargo xtask test
```

Expected：Git 只显示既有未跟踪 `.obsidian/`；`cargo xtask test` 六条 PASS。不得把 `.obsidian/` 加入任何提交。

---

## Plan Self-Review

- **Spec coverage:** 设计 §2（原语/状态/错误）→ Task 2（规范 §12.10）与 Task 4（探针 ops）；设计 §3（SretFrame）→ Task 3/4（cosir 帧类型）；设计 §4（跨架构中立，帧入细化层）→ Task 2 Step 4 边界清单；设计 §5（探针）→ Task 3/4；设计 §6（文档同步与证据）→ Task 2 Step 5 + Task 6；设计 §7（范围边界，不关 BLK）→ Task 6 Step 4 校验。
- **Placeholder scan:** 所有代码/命令/期望均给出具体内容；`AlreadyResumedStub` 仅在 Task 3 过渡期存在并于 Task 4 移除；`FaultCause` 若未使用则在 Task 4 Step 5 删除，均有明确处理。
- **Type consistency:** `TaskId`/`ExcSourceId`/`SegId`/`VirtAddr`/`SretFrame`/`SRET_FRAME_LEN`/`TaskStatus`/`PendingException`/`ExceptionSource`/`State`/`ResumeError` 在 Task 3–4 定义且被 Task 4 测试与 Task 5/6 校验一致引用。`exc_resume(st, caller, source_id, seg_ok, exec_ok) -> Result<(), ResumeError>` 在 Task 4 定义并被 `cosir/src/main.rs` 使用。
