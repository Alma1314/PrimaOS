# 切片 S 设计：IPC 用户化与两 ring3 任务共享 Segment

> 文档类型：真实内核实现设计（阶段 2 交付 S2-D5，产出退出条件 S2-X2 的主见证）
>
> 日期：2026-09-16
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-16，两项关键决策逐项确认）

## 0. 定位与前置

`BootstrapState` 里的 IPC 模型（`IpcMessage`/`IpcQueue`/`send_message`/`receive_message`/

`send_message_with_capabilities`/`block_recv`/`wake_recv`）在 SPEC-024~031 的 host 切片里就已存在，
但**没有任何用户可调路径**——syscall 号只到 16，唯一的 in-kernel 生产者是 `exc_raise`（且它只
构造无 cap 的通知）。因此 S2-D5 未交付，S2-X2 也没有见证。

本切片把 IPC 暴露为真实 syscall，并用两个**已经存在**的 ring3 任务（task 1 = init、task 2，
各自独立 PML4 根）产出 S2-X2 主见证：两个不可信 Task 的交互只能经「授权消息」与「共享 Segment」。

### 0.1 已确认的关键决策（用户逐项确认）

1. **`SYS_SEND` 授权绑定接收者**：调用者必须持有指向 `Task(receiver)` 且含 `TaskSend` 的
   Capability（用既有 `authorize` 比对 `target_of` 与 receiver）。这**替换**原先接收者无关的
   `has_task_send_authority`（它只问「是否持有任一 `TaskSend` cap」）。
2. **跨 Task 映射只做自身 + 负例**：task 2 把收到的段映射进**自己的**根（caller == target）；
   跨任务只以负例覆盖（未持目标的 `TaskManageVSpace` → 拒绝）。**不给任何任务对他方地址空间的
   管理权。**

### 0.2 动手前核对到的实现缺口

| 项 | 规范要求 | 改前现状 |
|---|---|---|
| 发送/接收用户化 | §8.1~§8.4 用户可调 send/recv | 无 syscall；模型函数仅 host 测试可达 |
| 发送授权 | 授权应对准**接收者** | `has_task_send_authority(sender)` 接收者无关 |
| 阻塞接收 | 空队列阻塞、被投递唤醒 | `receive_message` 只置 `BlockedRecv` 并返回 `QueueEmpty`；无 syscall 做切换 |
| task 2 的可用性 | 任务需持有 cap 才能调用 cap/段原语 | task 2 **零 cap**，连 `SYS_AUTHORIZE` 都只能得 `EACCES` |
| `memseg_map` 的目标根 | 目标根应与 task cap 一致 | syscall 层恒用 `sched::current_root()`，与 task cap 目标无一致性检查 |
| 消息携带 cap 的路径 | §8.3 位置 `InMessage` | `move_to_message`/`PositionMismatch` 从未被任何测试或运行路径走到 |

## 1. 设计

### 1.1 接收者绑定的发送授权（`bootstrap.rs`）

```rust
send_message(sender, receiver, send_cap: Handle, payload)
send_message_with_capabilities(sender, receiver, send_cap: Handle, payload, capability_ids)
```

检查序：两任务存活 → `authorize(sender, send_cap, TaskSend)` 且
`target_of(send_cap) == Some(ObjectRef::Task(receiver))` → cap 条数/重复/live/持有者 →
容量与审计预检 → 直投或排队。`has_task_send_authority` 整个删除。

### 1.2 播种用户 IPC Capability

槽 47/48（`USER_IPC_CAP_SLOT_BASE`，避开 1..5、6/7/8、32..46 与 `UNSET_CAP`=60），均为
`mint_root_capability_at` 的**新增铸造**，不动内核持有的平台 cap：

| 槽 | 目标 | 权限 | 持有者 | 作用 |
|---|---|---|---|---|
| 47 | `Task(2)` | `TaskSend` | `HeldByTask(INIT_TASK_ID)` | task 1 发给 task 2 的授权凭证 |
| 48 | `Task(2)` | `TaskManageVSpace` | `HeldByTask(2)` | task 2 把收到的段映射进自己的根 |

铸造不追加审计（`mint_root_capability_at` 只预检容量），故 bring-up 不产生新审计事件。

### 1.3 两个新 syscall（`syscall.rs`）

- `SYS_SEND=17`：`rdi=receiver`、`rsi=send_cap`、`rdx=payload`、`r10=cap_count`、`r8=cap0`、`r9=cap1`。
  授权与目标检查**先于**任何策略判断（未授权者不应从错误码差异推断接收者状态）；携带 cap 的消息
  若接收者正 `BlockedRecv` → `EINVAL`（见 §2 边界）；直投成功后 `take_pending_global(receiver)`
  吃掉 pending 并 `sched::set_frame_rax(receiver, payload)`。
- `SYS_RECV=18`：`rdi=out_ptr`，指向 3 个 qword（`cap0/cap1/cap_count`）。out 缓冲在**消费消息之前**
  校验（24B、8 对齐、不跨页、当前根下 present|user|writable），坏指针 → `EFAULT`；空队列 → 预置
  out 缓冲为 0、保存帧、`pick_next` + `block_to` 切走；被直投唤醒时从保存帧 `rax` 取得 payload。
- `ipc_errno`：`NoSuchTask|InvalidCapability|QueueEmpty → ENOENT`；
  `NotAuthorized|CapabilityHolderMismatch → EACCES`；`TooManyCapabilities → EINVAL`；
  `AuditFull|QueueFull → ENOSPC`。

### 1.4 目标根由 task cap 派生

`memseg_map_call`/`memseg_unmap_call` 改用 `bootstrap::task_of_cap(task_cap)` + `sched::root(task_id)`，
不再假定等于当前根。这消除「逻辑视图记在 Task X 名下、页表却改当前任务」的错配（此前无任何检查
拦截）。今天所有用户可达的 `TaskManageVSpace` cap 都指向持有者自身，故行为不变。

### 1.5 ring3 序列与见证

两个新块**追加**在既有块之后（既有 cap/段槽位分配顺序不变）：

- task 1（1477..1794）：`memseg_create`(1 页, R|W) → 映射进自己的根（`USER_SEG_SHARED_VA`）→ 写
  `IPC_SHARED_MAGIC` → `SYS_SEND`(2, 槽 47, magic, cap=[seg]) —— 此时 task 2 为 Ready，走**排队路径**，
  cap 位置变 `InMessage` → `SYS_YIELD` → `SYS_SEND`(2, 槽 47, `IPC_DIRECT_PAYLOAD`, cap=[]) ——
  此时 task 2 已 `BlockedRecv`，走**直投路径** → `SYS_YIELD` → `int3`。
- task 2（137..471）：`SYS_RECV`（取回 payload + cap0 + cap_count）→ 用**自己的** `Task(2)` 管理权
  映射进自己的根（`USER2_SHARED_VA`）→ 读回魔数 → **负例 1**：用不属于自己的 `Task(1)` cap 做
  跨任务映射（应 `EACCES`）→ **负例 2**：未授权发送给 task 1（应 `EACCES`）→ `SYS_RECV` 阻塞 →
  被直投唤醒（rax = payload）→ `SYS_YIELD`。

新事件 `ipc_ready`（seq 23）字段：`create/map1/send1/send2/recv1/cap0/magic/map2/cross/send_denied/
recv2/cap_count/queued/direct/blocks`。`queued`/`direct`/`blocks` 是**内核侧**的路径计数，使
「两条投递路径都真实走到」成为可断言的事实而非推断。

## 2. 显式边界（不夸大）

- **携带 cap 的消息只走排队路径**：接收者正 `BlockedRecv` 时 syscall 层拒绝（`EINVAL`），而模型层的
  直投路径本身支持搬 cap。原因是跨 CR3 写接收者用户缓冲（把句柄写进它的 out 缓冲）未实现；
  静默把 cap 挂到一个未被告知句柄的接收者名下，只会制造它无法寻址却仍然生效的 Authority。
  本切片的见证只用「排队路径送 cap + 直投路径送无 cap 消息」，故该限制未被触发。
- **两个负例代替跨任务正例**：不给任何任务对他方地址空间的管理权（用户决策 2）。报告里
  「跨 Task `TaskManageVSpace` 未覆盖」的边界因此是**以拒绝路径闭合**，不是以成功路径闭合。
- **共享 Segment 存活不销毁**：`frames_ready` 的 `segment` 因之 +1（17），`free` 相应 −1；守恒与
  不交性仍成立。保留存活是为了让「跨任务共享」在收尾时依然成立，而不是演示完即拆除。
- **task 2 不再零 cap**：它持有槽 48 的自管理 cap。这是刻意的，且该 cap 不授予它对他方地址空间或
  消息通道的任何权限。
- **`memseg_map` 仍无「root 与 task 一致」的构造性保证**：root 现在由 task cap 目标派生，但
  `BootstrapState::memseg_map` 本身仍把 `root_phys` 当普通参数（内核直映射路径也依赖这一点）。
- 未实现：S2-D4（Task create/yield/destroy 用户原语）、S2-D1/D2 之外的 task 生命周期、`dev_vspace`/W16.4、
  `REFINEMENT-PROOF`。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 3. 测试面

- host：接收者绑定授权的三类拒绝（非持有者 / 目标不匹配 / 缺 `TaskSend`）；`prepare_user_ipc_caps`
  不触碰平台 cap；排队 `InMessage` 下 `validate_authority` 通过、承载消息消失后 `PositionMismatch`
  （此前无测试）；`ipc_errno` 全变体映射；`recv_out_bounds` 越界与跨页；`IpcWitness::evidence_ok`
  正例 + 五类负例；两个 ring3 块的逐字节断言与长度。
- QEMU：29 条事件按新序列断言；`validate_ipc_data` 钉住 create/cap0/cap_count、魔数一致性、
  两个 `EACCES`、直投 payload 与三个路径计数；5 条反例注入。
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --all --check`、`cargo xtask test`、`cargo xtask compat`。
