# 切片 U 设计：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项）

> 文档类型：真实内核实现设计（阶段 2 收尾，补 `ISSUE-AUTH-002` 的「用户可达」面）
>
> 日期：2026-09-17
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-17，三项关键决策逐项确认）

## 0. 定位与前置

`ISSUE-AUTH-002` 的登记原文要求「由 `memseg_create` 接管这些区域并铸造可授权 Capability」。
切片 R（SPEC-063）已把 13 个平台区域迁为真实 `SegmentObject`（id 2..14）并铸了根
Capability（槽 32..44），但**这 13 条段 cap 与 2 条 task cap 全部由 `KERNEL_ACTOR` 以
`HeldByKernel` 持有**，用户 Task 拿不到；用户可见区域的映射至今由内核以
`caller = KERNEL_ACTOR` 代劳（`user.rs::map_region`，`kernel/src/user.rs:978`）。

ISSUE 表把「用户可见」是否等于「用户可达」的解读分歧留给复核者裁定。本切片按「可达」
补齐，并把解读分歧与全部偏差照实登记，**闭合判断交回复核者**。

| 面 | 规范/登记要求 | 改前现状 |
|---|---|---|
| 平台段根 cap 位置 | 可授权给实际使用者 | `HeldByKernel`（`bootstrap.rs:3207-3217`），`authorize` 的 `holder_of(cap) == caller`（`bootstrap.rs:1849`）在用户侧必然失败 |
| 用户映射路径 | 由持 cap 者自行 `memseg_map` | 内核以 `KERNEL_ACTOR` 代映射（`user.rs:978-994`） |
| 平台 Task cap | 目标地址空间的管理权 | 槽 45/46 `HeldByKernel`，用户无法经 `TaskManageVSpace` 中介映射 |
| `platform_cap_ok` | 平台根 cap 校验 | 硬要求位置恰为 `HeldByKernel`（`bootstrap.rs:3339-3346`） |

### 0.1 已确认的关键决策（用户逐项确认，2026-09-17）

1. **独立切片**：本项与 `ISSUE-TASK-002`（自毁）各为一个切片（SPEC-066 / SPEC-067），
   不合并。两者都要重测同一条 QEMU 事件基线，各自独立成报告，职责单一、回归面可分别阅读。
2. **按属主移交 11 条**：`SEG_VSPACE`(id 2) / `SEG_COW`(id 3) **保留内核持有**；其余 11 条
   用户可见区域按属主移交给 task 1 / task 2（见 §1.1）。
3. **粒度：完整交付一个领域**——不做「先只移交 task 1」的半成品，按属主授权语义完整建立。

VSPACE/COW 不移交的理由（登记，非遗漏）：两者 `allowed_access` 为空
（`platform_create_segment` 允许空访问集），`memseg_map` 对其必然 `RightsExceeded`；且其根
cap 带 `SegDestroy`，交给不可信用户等于授权销毁内核自建映射与 COW 私有帧区（`frames.rs`
的 `SEG_VSPACE`/`SEG_COW`）。这是有意的范围收窄。

## 1. 设计

### 1.1 移交范围（按属主）

槽位公式 `platform_segment_cap_slot(id) = 32 + (id - 2)`（`bootstrap.rs:1317`）。

| 属主 | 段 id / 槽位 | task cap |
|---|---|---|
| task 1（init） | 4→34、5→35、6→36、7→37（TASK1 CODE/STACK/X/Y）；11→41、12→42（SPIN1 CODE/DATA） | 槽 45 `Task(1)` + `TaskManageVSpace` |
| task 2 | 8→38、9→39、10→40（TASK2 CODE/STACK/Y）；13→43、14→44（SPIN2 CODE/DATA） | 槽 46 `Task(2)` + `TaskManageVSpace` |
| 保留内核 | 2→32（VSPACE）、3→33（COW） | — |

### 1.2 `BootstrapState::handover_platform_caps()`

复用现成的 `move_capability(cap, KERNEL_ACTOR, owner)`（`bootstrap.rs:2423`）：其内部
`positions.move_to_task` 已支持 `HeldByKernel.is_held_by(KERNEL_ACTOR)`
（`holder_id()` 返回 `KERNEL_ACTOR`，`bootstrap.rs:666`），且审计记录**实际**源位置
（`from = HeldByKernel`）。

- 逐条移交上表 11 条段 cap（按属主表）与 2 条 task cap；
- `SEG_VSPACE`/`SEG_COW` 跳过；
- 新增全局包装 `handover_platform_caps()`（模式同 `platform_prepare_task_caps`，
  `bootstrap.rs:4107`）。

**调用时机**：`main.rs` 在 `user::prepare_task_create(...)`（`:229`）之后、
`configure_fast_path`/`activate_init`/`user::enter()`（`:236-243`）之前。必须在 bring-up
的全部 `map_region` 之后——那些映射以 `KERNEL_ACTOR` 为 caller，移交后就无法再代用户映射。

### 1.3 校验与见证口径

- `platform_cap_ok`（`bootstrap.rs:3335`）：位置判定由「恰为 `HeldByKernel`」放宽为
  「`HeldByKernel` **或** `HeldByTask(存活)`」；槽 Live、目标匹配、权限非空且 well-typed 三条
  校验保持不变。
- `PlatformSegmentsRecord`（`bootstrap.rs:3338`）新增：
  - `user_roots`(11) / `kernel_roots`(2)——按位置分类计数；
  - `holders: [u32; PLATFORM_SEGMENT_COUNT]`——逐槽属主（内核持有记 `KERNEL_ACTOR`=0）；
  - `task_holders: [u32; PLATFORM_TASK_CAP_COUNT]`。
  - `roots` 语义不变（通过全部校验的平台段根数 = 13）。
- `protocol.rs::platform_segments_ready`（`:517`）：删去硬编码 `holder=KERNEL_ACTOR`
  （`:539-540`），改为 `holders=0,0,1,1,1,1,2,2,2,1,1,2,2`（按 id 2..14 顺序）+
  `user_roots=11;kernel_roots=2;task_holders=1,2`；`slot_base`/`caps`/`task1_cap`/
  `task2_cap`/`frames`/`unified` 保持。

### 1.4 用户态见证（「用户真的能自持 cap」的直接证据）

在 `user.rs::user_code()` 末尾（现有 S2-D4 块之后、`int3` 之前）追加一段，`USER_CODE_LEN`
相应扩大：

1. **正例**：`SYS_MEMSEG_MAP(rdi=平台 X 段 cap[槽36], rsi=平台 Task(1) cap[槽45], rdx=0,
   r10=USER_MAP_WITNESS_VA, r8=1, r9=R)` → 期望返回 0。
2. **读回**：读 `USER_MAP_WITNESS_VA` → 期望 `cow::pattern(0)`（`cow.rs:30`），证明映射真的
   指向该 Segment 的帧。
3. **负例**：用 **task 2 所属**的段 cap（槽 38）以 task 1 为 caller 再映射 → `EACCES`
   （`authorize` 的 `holder_of(cap) == caller` 在用户侧对非属主成立拒绝）。
4. 三条回执写入 Y 页空闲偏移（回执区 368..447 已用，取 448/456/464；`TASK_RECV_BUF` 在
   Y+768，不冲突）。

`USER_MAP_WITNESS_VA = USER_BASE_VA + 0x4000`：与既有映射同处一个 2 MiB 区域，复用已存在的
PT 页，故**不新增页表帧**；只新增一条指向 X 段既有帧的 PTE。

这一段经 `memseg_map_call`（`syscall.rs:797`）→ `bootstrap::memseg_map`（`:3424`）→
`authorize`（`:1849`）全链路，是本切片「用户可达」的判决性证据。

## 2. 回归面（逐条登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| `platform_segments_ready` | 删 `holder`，增 `holders`/`user_roots`/`kernel_roots`/`task_holders`；`vspace` 因用户新映射 X 段到第二个 VA 由 11→12；`mapped`(11) 不变（X 段已有条目）；`roots`(13)/`task_caps`(2)/`caps`/`frames`/`unified` 不变 |
| `task_ready` | 新增映射见证三字段（正例回执、读回值、非属主负例） |
| `memory_ready` | `image`/`kend`/`usable`/`universe` 随 `user_code` 增长 |
| `frames_ready` | `image` 随之上移；映射不新增表帧，故 `table` 不变；`free`/`universe` 同步 |
| `preempt_ready` | `rsp*`/`kstack*_top` 随镜像增长整体后移 |
| `TASK_READY_LIVE_CAPS` | cap 数本身不变（移交只改位置），但需按新事件实读复核 |
| host 单测 | `bootstrap_state_stays_within_the_stack_budget`（16 KiB）：移交的 `CapabilityMoved` 是运行时审计追加，不改结构体尺寸；`MAX_AUDIT_EVENTS=128` 余量需实测，不足时按「扩展模型而非放宽守卫」处置（登记为与 `ISSUE-TASK-003` 同批的审计重构，不在本切片放宽） |
| 既有切片 E/F/G/O/Q/R/S/T | 逐条与报告实测值对账；`caps` 句柄本身不变（`move` 保持同槽同 generation） |

## 3. host 单测

- 移交后：属主 `authorize` 成功、非属主拒绝；
- `SEG_VSPACE`/`SEG_COW` 仍在 `HeldByKernel`，其 `authorize` 对任何 Task 失败；
- `platform_segments_record` 的 `user_roots=11`/`kernel_roots=2`/`holders` 逐槽符合预期；
- 移交产生 11 条 `CapabilityMoved`，`from = HeldByKernel`；
- 重复调用 `handover_platform_caps` 的行为（幂等或确定性失败）二选一并钉住。

## 4. 验证

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `cargo xtask test`（QEMU BIOS run=1/2，全部事件按新序列与 data 断言）
- `cargo xtask compat`（`PASS compat cow-write-fault`）

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛，与阶段 0/1 一致）。
- 两项既有偏差照实登记并交复核者裁定：(1) 平台路径经 `platform_create_segment` 而非
  `memseg_create` 入口；(2) VSPACE/COW 保留内核持有，故「平台区域全部用户可达」不成立。
- 移交只建立「用户可自持并提供 cap」的机制，不宣称最小权限已最优（例如 task 1 同时持有
  自己的 6 个区域与 `TaskManageVSpace`，其中 code/stack 的映射权是否该给用户态属于后续复核面）。
