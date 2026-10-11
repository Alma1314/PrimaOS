# SPEC-091 报告：逻辑 vspace 存储移出 `BootstrapState`（arena 化）

> 状态：实现；支撑性重构，不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[逻辑 vspace 存储移出设计](../specs/2026-10-11-stage4-vspace-arena-design.md)

## 1. 本片要回答的问题

SPEC-090 §6.1 登记的「单一宿主任务」简化的**真实硬约束**是 `BootstrapState` 的 13 KiB 软守卫
（余量仅 72 B）：新增独立服务端点需 +11 条 vspace 条目 → 必须扩容 `MAX_VSPACE_ENTRIES` → 每槽
32 B 必然越守卫。项目既有约束「守卫未放宽、进一步存储增长先做存储重构」指明了正路。本片把
`TaskVspaceTable` 的**存储**移入静态 arena，为后续多端点切片解除这道墙。

## 2. 改动

`kernel/src/bootstrap.rs`：

- 新增 `static VSPACE_ARENA: crate::arena::Arena<VspaceEntry, MAX_VSPACE_ENTRIES, VSPACE_REGIONS>`
  （复用 SPEC-081 的通用 arena，**不新增抽象**）；`VSPACE_REGIONS`：内核 1 段、host 单测 512 段。
- `TaskVspaceTable` 由「`[Option<VspaceEntry>; 44]` + count」（1416 B）改为「`ArenaSlice` + count」
  （16 B）；`MAX_VSPACE_ENTRIES` 44 → **64**（与 `MAX_CAPABILITY_SLOTS` 对齐）。
- `find`/`insert`/`remove`/`references_segment`/`count` 语义不变；新增 `for_each_entry`。
- 四个直接访问 `entries[..count]` 的调用点（任务销毁解映射 ×2、平台段见证 ×2）改用
  `for_each_entry` / `references_segment`。

**未经改动的调用方语义**：`platform_segments_record` 的 `vspace` 字段仍是「落在平台区间的映射条目
数」（`references_segment` 口径），boot 实测仍为 13。

## 3. 回归面（手工事件流 diff，逐条登记）

`cargo xtask test` 通过（42 条事件、seq 无增删）；但 `validate_*_data` 对布局/计数类字段**只断言
结构不变式、不钉死数值**，故本片按维护规则 5 手工 diff 逐条登记变化：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（seq 3） | `usable` | 30154 → **30174** |
| | `image` | 1312 → **1290** |
| | `free` | 30154 → **30174** |
| | `kend` | 0x1520000 → **0x150a000**（`kstart` 0x1000000 不变） |
| `bootstrap_ready`（seq 9） | `image_len` | 5371608 → **5282768** |
| | `image_frames` | 1312 → **1290** |
| | `free_frames` | 30154 → **30174** |
| `preempt_ready`（seq 30） | `rsp1`/`rsp2` | 0x…a4678 / 0x…a8678 → **0x…a2748 / 0x…a6748** |
| | `kstack1_top`/`kstack2_top` | 0x…a46a0 / 0x…a86a0 → **0x…a2770 / 0x…a6770** |
| | `root*`/`cr3*`/`data*`/`mark*`/`ticks`/`pre*` | **不变** |
| `frames_ready`（seq 36） | `universe` | 31466 → **31464** |
| | `free` | 30018 → **30038** |
| | `image` | 1312 → **1290**（`segment=31`/`table=105` 不变） |
| `self_root_ready`（seq 37） | `pages` | 459 → **458** |

**根因**：结构体 13240 → 11840 B（−1400 B），按值搬移代码减少、镜像缩小 22 帧（−90112 B），
故 `kend` 前移、镜像/空闲帧计数变化、per-task 内核栈地址（BSS 内）前移。**其余 37 条事件逐字段
不变**（含 `ipc_ready` 6/9/11、`task_ready` 5/54、`service_selected_ready` 等全部判决性见证）。

同镜像连跑两次 42 条事件逐字节相同。

## 4. 容量收益（本片的判决性数字）

| 指标 | 前 | 后 |
|---|---|---|
| `BootstrapState` 尺寸 | 13240 B | **11840 B** |
| 对 13 KiB 软守卫余量 | **72 B** | **1472 B** |
| `MAX_VSPACE_ENTRIES` | 44 | **64**（不再占结构体空间） |
| vspace boot 占用 | 41 / 44 | 41 / 64 |

⇒ 后续多端点切片可把上限提到 64（乃至更高）**而不动 `BootstrapState`**。

## 5. 验证（2026-10-11）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **366**、prima-ostd 21、xtask 198 |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20501 total_lines=21763 ratio=5.80%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，seq 无增删） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

手工事件流 diff（§3）为**方法边界内的唯一回归证据来源**（门禁不钉布局值）。

## 6. 未决与登记

1. **本片无新交付能力**：只是把「进一步存储增长先移存储」的正确路径落实，供多端点切片使用；
   **不勾选** S4-D2 或任何阶段 4 交付项/退出条件。
2. `TaskVspaceTable::PartialEq` 比较 `slice + count`（同 `NsEntryTable` 口径）：两个独立构造的
   等内容表因区段不同而不相等；无调用方依赖整表相等。
3. 测试段数 512 与审计/Namespace 同口径；段耗尽即确定性 panic（不静默复用他人区段）。
