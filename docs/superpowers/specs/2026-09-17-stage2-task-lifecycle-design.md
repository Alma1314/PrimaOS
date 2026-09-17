# 切片 T 设计：Task create/yield/destroy 用户原语与单核调度（S2-D4）

> 文档类型：真实内核实现设计（阶段 2 交付 S2-D4，收口退出条件 S2-X1 的实现侧）
>
> 日期：2026-09-17
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-17，三项关键决策逐项确认）

## 0. 定位与前置

阶段 2 的交付项里，S2-D1/D2/D3/D5 与退出条件 S2-X2/S2-X3/S2-X4 已在切片 Q/R/S 交付；
**S2-D4（Task create/yield/destroy 与单核调度）是最后一个未交付项**。当前状态：

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| `task_yield` | §8.3 用户可调、`Running→Ready` | 已有 `SYS_YIELD=8`（切片 M），但迁移**不追加** `TaskStateChanged` |
| `task_create` | §8.1 完整签名与成功动作 1–9 | 只有 `TaskTable::create(task_id)`（建一条 `Ready` 记录），**无 syscall、无 vspace、无 cap、无审计** |
| `task_destroy` | §8.2 成功动作 1–9 | 只有内核侧 `destroy_task`（cap/消息/记录清理），**无 syscall、无 vspace 拆除、无订阅者移除、无 `TaskDestroy` 授权门控** |
| `TaskCreated` 事件 | §8.1 步骤 8 | `AuditEvent` **没有该变体** |
| 单核调度 | §8.4 `schedule` | `sched::pick_next`/`switch_to` 已有；新任务无法登记 root/kstack/frame |

### 0.1 已确认的关键决策（用户逐项确认，2026-09-17）

1. **`task_create` 做完整 §8.1**：入参含 `code_seg_cap`、`load_base`、`entry`、`code_offset`、
   `code_pages`、`parent_ns_cap`、`root_rights`、`initial_caps`；成功动作含「铸造 manager cap 给
   调用者」「派生 self cap 给新 Task」「从 parent_ns_cap 派生新 Task 的 `root_ns_cap`」
   「把 `initial_caps` 从调用者移动到新 Task」。
2. **自我销毁不在本切片**：只销毁非 `Running` 目标（父销毁子）；自毁登记为
   `ISSUE-TASK-002` 并以确定性 errno 拒绝。理由：自毁要在**正被使用的 CR3 与内核栈**上回收
   自身根页表帧，依赖「释放与 `write_cr3` 之间不得分配帧」的脆弱时序，而本切片的见证不需要它。
3. **W13 只补 create/destroy 的迁移事件**：调度路径（`yield`/抢占/`block`/`wake`）缺
   `TaskStateChanged` 的缺口登记为 `ISSUE-TASK-003`，留给 S3-D5（「对象生命周期 Audit」）。
   理由：`BootstrapState` 实测 15240 B、guard 上限 16384 B，余量仅 1144 B（约 47 条 24 B 事件），
   为调度路径补齐事件必须做审计存储重构；该重构与 S3-D5 的交付面重合，不在本切片顺手做。

### 0.2 动手前核对到的实现缺口

| 项 | 规范要求 | 改前现状 | 本切片处置 |
|---|---|---|---|
| `TaskCreated` | §8.1 步骤 8 追加 | 变体不存在 | 新增变体并在 create 成功时追加 |
| 新 Task 的 self cap | §8.1 步骤 4 | 无 | `mint_root_capability` 给新 Task |
| 新 Task 的 `root_ns_cap` | §8.1 步骤 6 从 `parent_ns_cap` 派生 | `TaskRecord::root_ns_cap` 存在但非 init 任务恒为 `Handle::ZERO` | 新增「派生到指定持有者」路径 |
| `initial_caps` | §8.1 步骤 7 从调用者移入新 Task | `positions.move_to_task` 存在，无操作暴露 | create 中逐条移动 + `CapabilityMoved` 审计 |
| vspace 拆除 | §8.2 步骤 1 删除目标全部页映射 | `destroy_task` 不触碰 vspace | 拆除目标的 `TaskVspaceTable` 条目 + 归还页表帧 + 归还根帧 |
| 异常订阅移除 | §8.2 步骤 7 | 无 `ExcSourceTable` 移除路径 | 新增按 task 移除订阅者的内部路径 |
| `TaskDestroy` 门控 | §8.2 前置 | `destroy_task` 无授权检查 | syscall 层 `authorize(caller, task_cap, TaskDestroy)` |
| 新 Task 的内核栈 | 细化层义务 | 只有静态 `KSTACK_TASK1/2` | 新增按 task id 索引的静态内核栈数组 |
| 新 Task 的初始 rsp | §5.2 无 SP，细化层义务 | 无 | 参数块显式携带 `stack_top`（细化层字段，见 §2） |

## 1. 设计

### 1.1 两个新 syscall（`syscall.rs`）

- `SYS_TASK_CREATE = 19`：`rdi = req_ptr`，指向用户内存中 10 个 qword 的请求块
  （80 B，8 对齐、不跨页、当前根下 present|user|**可读**）。返回 manager cap 句柄，失败返回 errno。
- `SYS_TASK_DESTROY = 20`：`rdi = task_cap`。返回 `0`，失败返回 errno。

`SYS_YIELD = 8` 保持不变（§8.3 用户原语在切片 M 已交付，本切片只把它纳入 S2-D4 的证据面）。

请求块布局：

| 偏移 | 字段 | 说明 |
|---|---|---|
| 0 | `code_seg_cap` | 调用者持有、授权 `SegMapR`+`SegMapX` 的 Segment cap |
| 8 | `code_offset` | 代码在 Segment 内的起始帧下标 |
| 16 | `code_pages` | 代码页数（>0） |
| 24 | `load_base` | 代码首页的目标 VA（4 KiB 对齐） |
| 32 | `entry` | 入口 VA，须落在 `[load_base, load_base + code_pages*4 KiB)` |
| 40 | `stack_top` | **细化层字段**：新 Task 初始 `rsp`（抽象模型无 SP） |
| 48 | `parent_ns_cap` | 调用者持有、授权 `NsResolve`+`Derive` 的 Namespace cap |
| 56 | `root_rights` | 权限位集，须满足 `NsResolve ∈ root_rights ⊆ parent_ns_cap.rights` |
| 64 | `initial_cap0` | 可为零（缺省） |
| 72 | `initial_cap1` | 可为零（缺省） |

> **为什么栈不随 create 映射**：§8.1 步骤 2 只把**代码**装进新 Task 的 vspace；§20.3 又把
> 「新 Task 的初始代码映射」列为唯一免于 `TaskManageVSpace` 检查的映射。若 create 顺手再映射
> 一个栈段，那条映射没有授权记账，会绕过 §20.3。故 `task_create` 只登记 `stack_top` 到初始帧，
> **映射由调用者用 create 返回的 manager cap（含 `TaskManageVSpace`）经 `SYS_MEMSEG_MAP` 完成**——
> 记账完整，且新 Task 首次被调度前调用者必须完成该映射（首条指令的 `push` 就会用到）。

### 1.2 `BootstrapState::task_create`（`bootstrap.rs`）

前置（§8.1 1–8，全部检查在任何状态修改之前）：

1. `code_seg_cap` live、目标为活动 Segment、持有者为调用者、权限 ⊇ `{SegMapR, SegMapX}`；
2. `code_pages > 0`、`code_offset + code_pages ≤ segment.frame_count()`；
3. `load_base` 4 KiB 对齐且 `load_base ≤ page_of(entry) < load_base + code_pages*4 KiB`；
4. `parent_ns_cap` live、目标为 Namespace、持有者为调用者、权限 ⊇ `{NsResolve, Derive}`；
5. `root_rights ⊆ parent_ns_cap.rights` 且含 `NsResolve`、`well_typed(Namespace)`；
6. `initial_caps` 至多 2 条、两两不同、逐条（非零时）live + 目标存活 + 持有者为调用者，且不含
   `code_seg_cap`/`parent_ns_cap`、不含任何活动 Task 的结构性根 Namespace cap；
7. 任务表、cap 槽位、页帧（根表帧 + 代码页路径上的表帧）与审计容量均有余量。

成功动作是一个整体（对齐 §8.1 1–9）：

1. 取新 task id（最小空闲、≥2），建 `TaskRecord { status: Ready, entry, root_ns_cap }`；
2. 分配新根表帧 → `clone_root(boot_root)` → 清空两个用户 PML4 槽 → 逐页
   `vspace::map_4k(child_root, load_base + i*4 KiB, code_frame_i, writable=false, user=true)`，
   并把每页记入 `TaskVspaceTable { task_id, va, seg_id, seg_offset, access: {R,X} }`；
3. 铸造 manager cap：`mint_root_capability(caller, {Derive,TaskSend,TaskManageVSpace,TaskDestroy},
   Task(new_id))`，位置 `HeldByTask(caller)`；
4. 铸造 self cap：同权限、同目标、位置 `HeldByTask(new_id)`；
5. 从 `parent_ns_cap` 派生 `root_rights` 子 cap，**位置为 `HeldByTask(new_id)`**，写入
   `TaskRecord::root_ns_cap`（新增 `derive_capability_to_holder` 路径）；
6. 逐条把 `initial_caps` 从 `HeldByTask(caller)` 移到 `HeldByTask(new_id)`，各追加一条
   `CapabilityMoved`；
7. `sched::set_root/set_kstack_top/set_frame`（rip = `entry`、rsp = `stack_top`、CS/SS 用户选择子、
   RFLAGS 置 FIXED+IF）；
8. 追加 `TaskCreated { task_id }`；
9. 返回 manager cap 句柄。

失败保持在成功动作 1 之前：全部前置校验先做；步骤 2 中途失败（表帧/代码映射）时归还已取表帧与
根帧、回滚已建 vspace 条目；步骤 3–6 的 cap 铸造失败时归还 vspace 并退役已铸 cap。

### 1.3 `BootstrapState::task_destroy` 的 §8.2 补齐（`bootstrap.rs`）

在既有 `destroy_task` 基础上补齐：

1. **vspace 拆除（步骤 1）**：对目标的每条 `TaskVspaceTable` 条目调
   `vspace::unmap_4k(child_root, va, map, release_table_frame)`，删条目，最后归还目标根表帧并
   清 `sched` 的 root/kstack/frame 登记；
2. **异常订阅移除（步骤 7）**：新增 `ExcSourceTable::remove_subscriber(task)`，从所有源的
   `subscribers` 移除目标；
3. 既有步骤保持：消息删除（2/3）、cap 位置删除（4）、`TaskDestroyed`（9）。

**不适用步骤的显式登记**：步骤 5（Timer）与步骤 8 的 `pending_exc`/`kernel_resume_frame` 清理
依赖尚未实现的对象（Timer、`Faulted`），分别登记为既有缺口，不在本切片假装完成。

自毁（`target == caller`）在 syscall 层以 `EACCES` 拒绝并登记 `ISSUE-TASK-002`；
销毁 init 仍按既有 `CannotDestroyInit` → `EACCES`。

### 1.4 调度登记（`sched.rs`）

`resume()` 已按 task id 读 root/kstack/frame。新增 `clear_task(task_id)`（root=0、kstack=0、
frame=`SretFrame::ZERO`），供 destroy 拆除后调用，使被销毁的 id 不再可被恢复。

新 Task 的内核栈：`user.rs` 新增按 **task id 索引**的静态内核栈数组（`MAX_TASKS` 项 × 16 KiB），
仅 id ≥ 3 使用（task 1/2 仍用既有 `KSTACK_TASK1/2`，故 `preempt_ready` 的 `rsp*`/`kstack*_top`
逐字段不变）。

### 1.5 bring-up 新增（`user.rs` / `bootstrap.rs`）

| 槽 | 目标 | 权限 | 持有者 | 作用 |
|---|---|---|---|---|
| 49 | `Segment(15)` | `Derive,SegMapR,SegMapX,SegDestroy` | `HeldByTask(1)` | 新 Task 的代码段（内核写入 `task3_code()`） |
| 50 | `Task(1)` | `TaskSend` | `HeldByTask(1)` | 作为 `initial_cap0` 移入新 Task，使子任务能向 task 1 发送 |

两条均为 `mint_root_capability_at` 的**新增铸造**（不动内核持有的平台 cap，故
`platform_segments_ready.task_caps` 仍为 2）。代码段用**用户区 id**（`USER_SEGMENT_ID_FIRST = 15`），
不改 `PLATFORM_SEGMENT_ID_LAST = 14` / `PLATFORM_SEGMENT_COUNT = 13` / 平台 cap 槽 32..44，
避免扰动切片 R/S 的既有证据。代价：ring3 里既有的 `memseg_create` 的 Segment **id** 顺移一位
（无任何金标断言 id），cap 句柄与帧数不变。

### 1.6 ring3 序列与见证

> **实现期修订（2026-09-17，随报告 §2.2/§6 登记）**：原计划「一次让出后子任务即运行」不成立
> ——`pick_next` 取最小 Ready id，而 task 2（id 2）在切片 S 序列结束时仍 `Ready`，会一直抢先于
> 新任务（id 3）。实际实现改为：task 2 代码尾部加**阻塞接收循环**，task 1 在 create 块里
> **让出两次**（第一次排空 task 2、第二次轮到子任务），并在块末**直投唤醒 task 2** 以保证收尾的
> 时钟抢占阶段仍有 `Ready` 任务。末段的 `SYS_YIELD`（原第 13 步）因此取消。另有两条负例
> （自毁、销毁 init）**只有 host 覆盖**：用户态没有指向 init 且含 `TaskDestroy` 的 cap，而自毁
> 需子任务持有自己的 manager cap（其槽位运行期才确定），故 QEMU 见证不含这两条。

task 1 追加一个块（既有块之后）：

1. `SYS_MEMSEG_CREATE(1, R|W)` 建栈段 → 栈段 cap（实测槽 9 gen1）；
2. `SYS_TASK_CREATE(req)`：`code_seg_cap` = 槽 49、`load_base`=`entry`=`USER3_CODE_VA`、
   `stack_top`=`USER3_STACK_VA + 4 KiB`、`parent_ns_cap` = 槽 2、`root_rights` = `NsResolve`、
   `initial_cap0` = 槽 50 → manager cap（实测槽 10 gen1）；
3. **负例 1**：同一请求但 `entry` 落在代码段外 → `EINVAL`；
4. **负例 2**：`code_seg_cap` 换成不含 `SegMapX` 的 cap（栈段 cap）→ `EACCES`；
5. `SYS_MEMSEG_MAP(栈段 cap, manager cap, 0, USER3_STACK_VA, 1, R|W)` → 栈映射进**子任务**根；
6. 让出两次 → 子任务运行。

task 3（新代码 `task3_code()`，由内核在 bring-up 写入其代码段）：

1. `SYS_TASK_ID` → 3，写入自己的栈页；
2. `SYS_SEND(1, 槽 50 句柄, payload = 上一步的 id, 0, 0, 0)`；
3. `SYS_YIELD`（让回父任务；此后不应再被调度，若被调度则写哨兵并继续让出）。

task 1 恢复后：

7. `SYS_RECV` → 取回子任务自报的 id（实测 `0x3`）；
8. **负例 3**：用**不含 `TaskDestroy`** 的 cap（栈段 cap）销毁 → `EACCES`；
9. `SYS_TASK_DESTROY(manager cap)` → `0`；
10. **负例 4**：再次 `SYS_TASK_DESTROY(manager cap)` → `EACCES`（目标已销毁 → `target_live` 假，
    manager cap 成为 stale token）；
11. **负例 5**：向已销毁子任务的地址空间 `SYS_MEMSEG_MAP` → `ENOENT`；
12. `SYS_SEND(2, 槽 47, ...)` 直投唤醒阻塞的 task 2 → `int3`。

新事件 `task_ready`（seq 24）字段：`stack_seg`、`create`（manager cap 句柄）、`bad_entry`、
`no_x`、`stack_map`、`recv`、`deny`、`destroy`、`again`、`map_dead`、`child`（内核选定的子任务 id）、
`tasks`（活动任务数）、`caps`（cap 槽活跃数）、`root_cleared`（子任务调度登记是否已清）。

`child`/`tasks`/`caps`/`root_cleared` 由**内核侧**在事件发射时读取（`task_create` 的返回值、
`live_task_count`、`live_cap_count`、`sched::root(child)`），使「子任务确已被从任务表与调度登记
清除、其 cap 确已撤销」成为可断言事实而非推断。

## 2. 显式边界（不夸大）

- **自毁未实现**（`ISSUE-TASK-002`）：`task_destroy` 对 `target == caller` 返回 `EACCES`；
  §8.2「删除自身是合法的」字面未满足。
- **调度路径的 `TaskStateChanged` 未补**（`ISSUE-TASK-003`）：§13.1/W13 要求的
  `Running↔Ready`、`Running→BlockedRecv`、`BlockedRecv→Ready` 迁移事件仍缺；本切片只补
  `TaskCreated` 与 create/destroy 引发的 `TaskStateChanged`。留 S3-D5。
- **`initial_caps` 容量固定 2 条**：§8.1 是列表，本切片按消息 cap 的同一容量（2）实现。
- **`root_rights` 只校验 `NsResolve` 必含**：§8.1 步骤 6 的「权限衰减」由 `subset_of` 保证，
  但本切片不建 Namespace 层级，故 `parent_ns_cap` 实际只能是 init 根 Namespace cap。
- **Timer 与 `pending_exc`/`kernel_resume_frame` 清理未实现**：§8.2 步骤 5、6 依赖尚未存在的对象。
- **代码段只支持内核写入**：调用者不能自己把代码写进新段（需要自身的 `TaskManageVSpace`），
  故新 Task 的代码段由 bring-up 提供并铸 cap 给 task 1。这是本切片的务实取舍，已登记。
- **`stack_top` 是细化层字段**：抽象模型 §5.2 的 Task 无 SP；此字段是细化层必须补的自由度，
  已在本设计与报告显式登记，不冒充规范字段。
- 未实现：`dev_vspace`/W16.4、`REFINEMENT-PROOF`、`ISSUE-FRAME-001`。

## 3. 测试面

- host：`task_create` 的九类前置拒绝（缺 `SegMapX` / 入口越界 / `code_pages=0` /
  `code_offset+pages` 超段 / `parent_ns_cap` 缺 `NsResolve`+`Derive` / `root_rights ⊄` /
  `root_rights` 缺 `NsResolve` / `initial_caps` 重复或非调用者持有 / 结构性根 cap）各断言
  **状态不变**；成功路径断言 manager/self/`root_ns_cap` 三条 cap 的持有者、权限与 `parent` 链、
  `TaskCreated` + `CapabilityMoved` 事件、`initial_caps` 位置变化；
- host：`task_destroy` 的授权拒绝、init 保护、自毁拒绝、vspace 条目清空、`TaskDestroyed` 事件、
  重复销毁的 stale 判定；
- host：`task_create`/`task_destroy` 的 errno 映射全变体；请求块边界检查（对齐/跨页/长度）；
- QEMU：30 条事件按新序列断言；`task_ready` 钉住 manager cap 句柄、子任务 id、两个创建负例、
  三个销毁负例、`recv` 魔数、`post_yield=0`、`tasks`/`caps` 计数；
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --all --check`、`cargo xtask test`、`cargo xtask compat`。
