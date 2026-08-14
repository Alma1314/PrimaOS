# TLB 细化（tlb_refine）设计

> 文档类型：细化层设计（REFINEMENT-DESIGN）
>
> 日期：2026-08-15
>
> 项目：Prima OS（cosir host 模型）
>
> 状态：已获用户确认（2026-08-15）

## 1. 背景与动机

垂直切片固定为 **COW/page fault → handler → `exc_resume` → TLB → Linux trace**。前三节点已在 host 层实现并合并回 master：

- `ops` 抽象层：`raise_exception` / `exc_resume` / `schedule`（exc_resume 细化，`cosir/src/ops.rs`）；
- `pagewalk`：x86_64 四级页表 host 模型（`translate` / `map_vaddr` / `unmap_vaddr` / `visit_leaves` / `FrameAllocator`）；
- `memseg_refine`：MemorySegment 细化（每 Task 的 `PageTable` CR3 树 + `vspace` 双视图，W6/W7 不变式）；
- `cow_refine`：以既有 memseg/ops 原语组合表达用户态 COW fork（共享只读 → 写 fault → 换映射 → `exc_resume`）。

本切片补全垂直路径中缺失的 **TLB** 节点。当前 host 模型直接走页表（`translate`），**没有任何 TLB 语义**——没有命中/失效，也没有"页表被改后 TLB 仍缓存旧映射"的非强制刷新语义。真实 x86 中，TLB 不会因页表被写而自动失效；软件必须显式 `invlpg` / CR3 reload / `INVPCID`。遗漏 flush 会导致后续访问命中**陈旧 TLB 条目**，返回旧物理帧或旧权限。

该节点同时承担研究主张 H1（有限原语完备性）在"真实 TLB behavior"维度的可证伪验证，并支撑后续 Linux trace / `COMPAT-TEST`（地址翻译一致性）。

## 2. 目标

- 为每 Task（每 CR3）建模独立 TLB：命中时访问走 TLB 缓存的翻译（**不用户表**），未命中时才走 `pagewalk::translate` 并把结果填充进 TLB。
- 建模**非强制刷新**语义：`memseg_unmap` / `memseg_map` 只改 PTE，**绝不自动清除对应 TLB**。
- 提供**显式失效原语**：`tlb_invalidate_va`（单地址）与 `tlb_flush_all`（全量），作为让访问重新观察新 PTE 的手段。
- 核心可证命题 **T1**：COW 换映射（PTE 变为可写）后、未 flush 时，对同一 va 的写访问命中陈旧只读 TLB → 权限被驳回（可检测的不一致）；显式失效后访问走新 PTE → 正常。
- 遵循既有切片独立验证模式，不并入 `cow_refine`，不修改已定案的 `ops` / `memseg_refine` 语义。

## 3. 非目标

- 不建模 TLB 容量、关联度或替换策略（FIFO/LRU）。
- 不建模多核 shootdown / TLBI IPI / ASID 复用（留给多核阶段）。
- 不建模 TLB 因上下文切换（CR3 写）而整体失效的特殊路径；本切片只有 `tlb_flush_all` 表达全维度失效。
- 不接入真实 QEMU/hardware，不宣称 `KERNEL-TRACE` 或 `REFINEMENT-PROOF`；证据边界仅 `MODEL-TEST`。

## 4. 架构

在既有 `memseg_refine::RefineState`（含每 Task `PageTable` + `vspace` 双视图）之上，新增一个**平行的 TLB 视图**。两者通过一组可证伪的转移共同描述一次地址访问：

- **页表视图（已有）**：`PageTable` 四级树 + `vspace` 双视图；是"权威"映射，但真实 x86 中访问不一定经过它。
- **TLB 视图（新增）**：每 Task 一个 TLB，缓存最近 walk 的翻译结果；命中时**遮蔽**页表视图。

### 4.1 数据结构

```rust
struct CachedXlat {
    frame_phys: PhysAddr, // 命中时返回的物理帧；可对不应期 PTE 的陈旧帧
    access: u8,           // 命中时返回的权限位（R/W/X）；可对不应期 PTE 的旧权限
}

struct Tlb {
    entries: BTreeMap<VirtAddr, CachedXlat>,
}
```

TLB 状态容器：

```rust
struct TlbState {
    per_task: BTreeMap<TaskId, Tlb>,
}
```

错误枚举（细化层可见，非对 POSIR 新增原语）：

```rust
enum TlbError {
    NoSuchTask,          // per_task 中无该 Task
    UnalignedAddress,    // va 未按页对齐
    NonCanonicalAddress, // va 非 canonical 用户地址
    NotMapped,           // 未命中 TLB 且页表也无映射
}
```

### 4.2 访问流程：`tlb_translate`

```
tlb_translate(state, task_id, va) -> Result<CachedXlat, TlbError>
```

1. `validate_page_vaddr(va)`（对齐 + canonical，非 canonical / 未对齐 → `NonCanonicalAddress` / `UnalignedAddress`）。
2. Task 不存在 → `NoSuchTask`。
3. 若 `per_task[task_id]` 命中 `va` → **命中**：直接返回缓存 `CachedXlat`（含权限、物理帧），不 consult 页表。
4. 否则 → **未命中**：走 `pagewalk::translate`：
   - 页表无映射（`None`）→ `NotMapped`；不填充 TLB。
   - 有 PTE → 把 PTE 的 `frame_phys` + `mapping.access` 填充进 TLB，返回之。

命中语义要点：**权限决定取自 TLB 缓存，而不是当前 PTE**。这正是不允许 `unmap/map` 静默清 TLB 的原因——若未失效，命中的是陈旧权限/陈旧物理帧。

### 4.3 显式失效原语

```
tlb_invalidate_va(state, task_id, va) -> Result<(), TlbError>
```

删除 `per_task[task_id]` 中键 `va` 的条目；缺失或多个与否都成功（幂等），Task 不存在则 `NoSuchTask`。对应单地址 `invlpg`。

```
tlb_flush_all(state, task_id) -> Result<(), TlbError>
```

清空该 Task 全部 TLB 条目；Task 不存在则 `NoSuchTask`。对应整块失效（CR3 reload / `INVPCID`）。

`memseg_unmap` / `memseg_map` **不同步**调用失效原语——它们只改 PTE，绝不静默清 TLB。这是非强制刷新语义（§2）的直接体现，也是 §6 中证明 T1 的前提：换映射后必须显式失效。

## 5. 与既有代码的对接

- 依赖：`crate::pagewalk`（`PAGE_SIZE`, `PhysAddr`, `translate`, `validate_page_vaddr`）；`crate::state`（`TaskId`, `VirtAddr`, `ACCESS_*`）。
- **不修改**：`ops.rs`、`memseg_refine.rs`、`state.rs`、`pagewalk.rs` 的既有语义。
- 新增文件：`cosir/src/tlb_refine.rs`（实现）、`cosir/src/tlb_refine_tests.rs`（测试）、`cosir/src/main.rs` 冒烟入口。
- 订阅 `main.rs` 的冒烟开关，与 `cow_refine`/`memseg_refine`/`pagewalk` 一致。

## 6. 可证命题与反例

> 记 `R(st)` 为任务 P 在 `RefineState` 中的页表视图（含每 Task `PageTable`），`G` 为新增的 TLB 视图。一次"模拟访问"`access(st, va, kind)` 的**可观察权限**取自命中 TLB，否则取自页表。具体地：`access` 谓词对 `va` 调用 `tlb_translate`，得到 `CachedXlat`（命中走 TLB 缓存、未命中走页表），再检查 `cached.access` 是否覆盖 `kind` 所需的 R/W/X 位；被驳回表示 `cached.access` 不含该位。

- **T1（核心，非强制刷新）**：`memseg_map` 把 `va` 映射为**只读**；`tlb_translate` 访问一次（未命中 → 填充）。随后 `memseg_map`（经 unmap+map 换映射）把同一 `va` 改为**可写**。此时：
  - 未调用失效原语 → `access(st, va, W)` 命中陈旧 TLB（只读权限）→ **权限被驳回**（可检测不一致）。
  - 调用 `tlb_invalidate_va(st, tid, va)` / `tlb_flush_all` 后 → `access(st, va, W)` 走新 PTE → **成功**。
  - 该对照证明"换映射必须显式失效"是让新权限生效的必要步骤（真实 x86 语义）。
- **T2（失效作用域）**：`tlb_invalidate_va` 只清除指定 `va`；同 Task 其他地址的 TLB 条目仍命中。验证单地址失效不至于过度失效。
- **T3（Task 隔离）**：每 Task TLB 独立。Task A 失效不影响 Task B 的缓存；即使两者映射同一 `va` 同一物理帧，各自 TLB 独立维护。
- **R1（未命中不填充空映射）**：`tlb_translate` 对未映射 `va` 返回 `NotMapped`，且**不填充** TLB。随后补建映射后仍需先 walk 才命中（首次访问不走缓存）。
- **R2（陈旧物理帧承接）**：换映射导致物理帧改变（COW 复制换帧）时，未 flush 的 TLB 仍返回旧 `frame_phys`。断言"命中命中的是旧帧、而非新帧"，验证 TLB 遮蔽页表。

边界澄清（记录，非缺陷）：多核 shootdown、替换策略、CR3 切换自动失效不属本切片，留给后续阶段。

## 7. 测试与验收

- `cargo test -p cosir`：新增 `tlb_refine_tests`，覆盖 T1 对照（未 flush 驳回 / 显式失效后成功）、T2 单地址作用域、T3 Task 隔离、R1 不填充未映射、R2 陈旧物理帧承接。
- `cow_refine` 既有 117 测试不回归；`memseg_refine`、`pagewalk` 不回归。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零告警。
- `cargo test --workspace`（cosir + xtask 全绿）。
- `cargo xtask test`（QEMU 启动闭环 no regression）。
- `cargo run -p cosir` 冒烟新增 TLB 条目 PASS。

## 8. 证据边界

本切片证据仅 `MODEL-TEST`。**不宣称**：

- 真实 QEMU 页表 / TLB 行为；
- `KERNEL-TRACE`；
- `REFINEMENT-PROOF`；
- 机器证明（BLK-001 仍 Open）。

本切片证明的是：在既有 `pagewalk` + `memseg_refine` 之上，不加新内核原语，即可刻画"非强制刷新 + 显式失效"的 TLB 语义（TLB 一侧的 H1 边界验证），并让"COW 换映射后必须 flush"成为可检测命题。

## 9. 增量提交划分（供计划阶段参考）

1. `tlb_refine.rs`：`CachedXlat` / `Tlb` / `TlbState` + `tlb_translate` / `tlb_invalidate_va` / `tlb_flush_all`（未填充、命中/未命中分叉）。
2. `tlb_refine_tests.rs`：T1-T3 / R1 / R2 全量测试 + mutation 非空。
3. `main.rs` 冒烟开关并接入。
4. 全量验收命令与证据记录更新到实现阶段文档 tracker。
