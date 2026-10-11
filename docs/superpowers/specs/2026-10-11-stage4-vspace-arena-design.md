# SPEC-091 设计：逻辑 vspace 存储移出 `BootstrapState`（arena 化）

> 状态：实现；属于支撑性重构，不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-090 服务管理器](2026-10-11-stage4-service-manager-design.md)、
> [SPEC-081 Namespace arena](../reports/2026-09-25-stage3-namespace-report.md)、
> [SPEC-074 审计存储重构](2026-09-24-stage2-audit-storage-refactor-design.md)

## 1. 本片要回答的问题

SPEC-090 报告 §6.1 如实登记：两个注册服务由**同一宿主 ring3 任务**承载，**独立多端点任务**
待后续切片。侦察（2026-10-11）量化了该简化的**真实硬约束**：

| 约束 | 实测 | 上限/守卫 | 余量 |
|---|---|---|---|
| `BootstrapState` 尺寸 | **13240 B** | 软守卫 13312 B（13 KiB） | **72 B** |
| `TaskVspaceTable` 内联数组 | `[Option<VspaceEntry>; 44]` = **1408 B** | — | — |
| **vspace 实际占用** | **41 / 44** | 44 | **3 槽** |
| `VspaceEntry` | 32 B | — | — |

新增一个**独立 ring3 Rust 服务任务**需把同一 ELF 的 10 代码页 + 1 栈页映射进其独立根——
`task_create` 逐页记入 `TaskVspaceTable`（`kernel/src/bootstrap.rs` 成功动作 2），即 **+11 槽**。
`41 + 11 = 52 > 44`，必须扩容 `MAX_VSPACE_ENTRIES`。但每 +1 槽 = +32 B 结构体，现有**仅 72 B 余量**：
扩容必然越过 13 KiB 软守卫。项目既有约束明写「**守卫未放宽**」「进一步存储增长必须先做存储重构」，
故正路是把 `TaskVspaceTable` 的**存储**移出结构体，而不是放宽守卫。

本片落地这一支撑性重构；它不新增任何机制能力，只**为后续多端点切片解除存储墙**。

## 2. 机制

### 2.1 复用既有 arena

`kernel/src/arena.rs`（SPEC-081）已提供通用 `Arena<T: Copy, SLOTS, REGIONS>`——静态存储 + 原子
游标分段 + `Slice` 句柄，`NsEntryTable` 已用它。本片**不新增抽象**，直接复用：

```rust
static VSPACE_ARENA: crate::arena::Arena<VspaceEntry, MAX_VSPACE_ENTRIES, VSPACE_REGIONS> =
    crate::arena::Arena::new();

pub struct TaskVspaceTable {
    slice: crate::arena::ArenaSlice,  // 8 B
    count: usize,
}
```

`TaskVspaceTable` 由「内联数组 + count」变为「区段句柄 + count」（1416 B → 16 B）。
`VSPACE_REGIONS` 与审计/Namespace 同口径：内核 1 段，host 单测 512 段（并行构造多个 state 互不干扰）。

### 2.2 容量

`MAX_VSPACE_ENTRIES` 44 → **64**（与 `MAX_CAPABILITY_SLOTS` 对齐）。存储已移出，故该值**不占结构体
空间**——后续切片可自由扩容。boot 实测占用 41，多端点切片预计用 ~52，64 留有安全余量。

### 2.3 接口保持

`find`（返回值由 `Option<&VspaceEntry>` 改为 `Option<VspaceEntry>` 副本）、`insert`、`remove`、
`references_segment`、`count` 语义不变；新增 `for_each_entry`（值拷贝遍历，等价旧的内联迭代）。
四个直接访问 `entries[..count]` 的调用点（任务销毁解映射 ×2、平台段见证 ×2）改为 `for_each_entry`
/`references_segment`。

## 3. 判据

1. `BootstrapState` 尺寸实测 **13240 → 11840 B**（−1400 B），对 13 KiB 软守卫余量 72 B → **1472 B**；
2. **事件序列与 seq 不变**（42 条、无增删），**仅布局/计数类字段变化**（结构体变小使按值搬移代码减少、
   镜像缩小 22 帧），逐条登记见报告 §3；
3. `cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`、`cargo xtask compat` 全绿。

> 注：`cargo xtask test` 的 `validate_*_data` 对 `memory_ready`/`bootstrap_ready`/`frames_ready`/
> `self_root_ready`/`preempt_ready` 只断言**结构不变式**（如 `free==usable`、页对齐、`kstart<kend`），
> **不钉死**具体值——故这类布局字段的变化只有**手工事件流 diff** 才可见，需按维护规则如实登记
> （与 SPEC-074 同法）。

## 4. 未决与登记

1. **本片无新交付**：只是把「进一步存储增长」的正确路径（先移存储）落到实处，供多端点切片使用；
   不勾选 S4-D2 或任何阶段 4 交付项/退出条件。
2. `TaskVspaceTable::PartialEq` 比较 `slice + count`（同 `NsEntryTable` 口径）：两个**独立构造**的等
   内容表因区段不同而不相等——与既有 arena 型表一致，无调用方依赖整表相等。
3. `USER_TASK_KSTACK_COUNT`、cap 槽等其它容量约束**本片未动**，多端点切片再处理。
