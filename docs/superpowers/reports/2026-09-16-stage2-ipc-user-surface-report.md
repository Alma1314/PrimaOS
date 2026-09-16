# 切片 S 报告：IPC 用户化与两 ring3 任务共享 Segment

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-16
>
> 对象：`kernel/src/{bootstrap,syscall,user,resume,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 S2-D5（send/recv 用户原语），产出退出条件 S2-X2 的主见证——两个不可信 Task 只能经
> 授权消息或共享 Segment 交互

## 0. 结论速览

- **S2-D5 用户化**：新增 `SYS_SEND=17`/`SYS_RECV=18`，把既有 IPC 模型暴露为真实用户路径。
  `SYS_SEND` 的授权**绑定接收者**（调用者须持有指向 `Task(receiver)` 且含 `TaskSend` 的 Capability），
  替换原先接收者无关的 `has_task_send_authority`。`SYS_RECV` 空队列时阻塞切走，被直投唤醒后
  从保存帧 `rax` 取得 payload。
- **S2-X2 主见证**：QEMU 实测 `ipc_ready`——
  `create=0x100000008;cap0=0x100000008;cap_count=1`（段 cap 经**授权消息**转交）、
  `recv1=magic=0x51a5000000000001`（task 2 映射进**自己的**根后读回 task 1 写的同一物理帧）、
  `cross=EACCES`（未持目标 `TaskManageVSpace` 的跨任务映射被拒）、
  `send_denied=EACCES`（未授权发送被拒）、`recv2=0xd1ec700000000002`（直投唤醒阻塞接收者）、
  `queued=1;direct=1;blocks=1`（排队与直投两条路径各走一次）。
- **路径计数是内核侧事实**：`queued/direct/blocks` 由内核在 `send_call`/`recv_call` 中递增，
  故「两条投递路径都被走到」不是从回执推断的。
- **cap 移交**：新增两条铸给用户 Task 的根 cap（槽 47 = task 1 的 `TaskSend` on `Task(2)`；
  槽 48 = task 2 的自管理 `TaskManageVSpace`）。**task 2 不再零 cap**，且不对任何他方地址空间
  或消息通道获得权限。
- `authority_ready`（`caps=3;segments=1`）与 `platform_segments_ready`（`task_caps=2`、13 段、
  `unified=1`）**逐字段不变**；init 的初始 Authority 仍严格为 3 条。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛，见 §6）。

## 1. 方法与独立性声明

- 全部为真实内核实现 + QEMU 实测 + host 单测；无机器证明。
- 本报告复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，也不构成
  `REFINEMENT-PROOF`；不改变阶段 0/1/2 的独立性门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 发送授权 | `send_message`/`send_message_with_capabilities` 增 `send_cap` 参数；`authorize(sender, send_cap, TaskSend)` + `target_of == Task(receiver)`；删除 `has_task_send_authority` |
| 播种 | `USER_IPC_CAP_SLOT_BASE=47`、`TASK2_SEND_CAP_HANDLE`/`TASK2_SELF_CAP_HANDLE`、`prepare_user_ipc_caps`（`main.rs` 在 `platform_prepare_task_caps` 之后调用） |
| syscall | `SYS_SEND`/`SYS_RECV`、`ipc_errno`、`send_call`、`recv_call`、`recv_out_bounds`、`write_recv_out`、`IpcWitness` + 内核侧 `IpcPathState` |
| 目标根 | `memseg_map_call`/`memseg_unmap_call` 的 root 改由 `task_of_cap` + `sched::root(task_id)` 派生 |
| ring3 | task 1 `1478→1795` 字节、task 2 `137→472` 字节；新 VA `USER_SEG_SHARED_VA`(+0x6000)、`USER2_SHARED_VA`(+0x3000) |
| 见证 | 新事件 `ipc_ready`（seq 23）；`PreemptReady`..`BootComplete` 23..28 → 24..29 |

## 3. 实测证据（2026-09-16）

```
PRIMA|v=1|seq=23|event=ipc_ready|status=ok|data=create=0x100000008;map1=0x0;send1=0x0;send2=0x0;recv1=0x51a5000000000001;cap0=0x100000008;magic=0x51a5000000000001;map2=0x0;cross=0xfffffffffffffff3;send_denied=0xfffffffffffffff3;recv2=0xd1ec700000000002;cap_count=1;queued=1;direct=1;blocks=1
PRIMA|v=1|seq=26|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;task_caps=2;mapped=11;direct=2;direct_ids=2,3;vspace=11;holder=0;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,...,0x10000002c;frames=2,3,1,1,1,1,1,1,1,1,1,1,1;unified=1
PRIMA|v=1|seq=27|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=0x100000002;init_running=1
PRIMA|v=1|seq=28|event=frames_ready|status=ok|data=universe=31500;free=30745;segment=17;table=24;image=714;conserved=1;disjoint=1
PRIMA|v=1|seq=29|event=boot_complete|status=ok
```

解读：

- `create=cap0=0x100000008`：task 1 铸出的段 cap（槽 8 gen1）**经消息**到了 task 2 手里，
  且 `cap_count=1` 说明它确实是随消息转交的（不是共享同一进程的巧合）。
- `recv1=magic=0x51a5000000000001`：task 2 在自己的根下、用**自己**的管理权把段映射进来后，
  读回的值与 task 1 写入的魔数逐位相同——这是「共享 Segment」的判据，且 `map2=0` 证明映射成功。
- `cross=EACCES`：task 2 用不属于自己的 `Task(1)` cap 做跨任务映射被拒；`send_denied=EACCES`：
  未授权的发送被拒。两者一起给出「只能经授权消息或共享 Segment 交互」的**拒绝侧**证据。
- `recv2=0xd1ec700000000002`：阻塞中的 task 2 被直投唤醒，返回值正是发送方注入的 payload。
- `queued=1;direct=1;blocks=1`：一条 cap 走的排队路径（cap 位置经历 `InMessage`）、一条无 cap
  消息走的直投路径、一次真实阻塞各发生**恰好一次**——这三个计数由内核记录，不是从回执推断。
- `segment=17` = 平台 16 帧 + 存活的共享段 1 帧；`table=24` 与切片 R 相同（新 VA 落在既有
  2 MiB 页表内，未新增表帧）；守恒 `30745+17+24+714 = 31500 = universe`，`conserved=1`、
  `disjoint=1`。
- 未变项：`authority_ready`（`caps=3;segments=1`）、`platform_segments_ready`（`task_caps=2`、
  13 段、`frames=2,3,1×11`、`unified=1`）、`multitask_ready`（`switches=0x2`）、
  `exc_wait_ready`（`blocks=1`）、`memseg_ready`、`cap_lifecycle_ready`、`handle_ready`。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel 257、xtask 139 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**29 条**事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| 事件序列 | 新增 `ipc_ready`(23)；`preempt_ready` 23→24、`exc_resume_ready` 24→25、`platform_segments_ready` 25→26、`authority_ready` 26→27、`frames_ready` 27→28、`boot_complete` 28→29；`EXPECTED_EVENTS` 28 → 29 条 |
| `memory_ready` | `usable` 30798→**30786**、`image` 703→**714**、`kend` 0x12bf000→0x12ca000：内核镜像再增 11 帧（新增 syscall/IPC 代码与 ring3 代码 317+335 字节） |
| `bootstrap_ready` | `image_len` 2921504、`image_frames`/`free_frames` 随上；三个初始 cap 句柄与三组权限**不变** |
| `frames_ready` | `universe` 31501→**31500**（镜像增长吃掉一个可用帧）、`free` 30758→**30745**、`segment` 16→**17**（存活的共享段）、`image` 703→**714**；`table=24`、`conserved=1`、`disjoint=1` 不变 |
| `preempt_ready` | `rsp*`/`kstack*_top` 随内核镜像移动（`root1=0xc000`、`root2=0x15000`、`data1=0x1e000`、`data2=0x22000` 不变）；`ticks=4`、`pre1=pre2=2`、`mark1/mark2` 不变 |
| `vspace_ready`/`cow_ready`/`user_cow_ready` | 帧地址随镜像分布（`shared=0x6000`、`private=0xb000`、`frame_a=0x0`、`frame_b=0x1000` 与切片 R 相同）；`err=0x7`、魔数与模式语义不变 |
| `handle_ready`/`cap_lifecycle_ready`/`memseg_ready`/`syscall_*`/`exc_*`/`multitask_ready`/`exc_wait_ready` | **逐字段不变**（槽 1..8 与全部回执保持原值）——新 cap 落在预留槽 47/48 |
| `user_code` | 1478 → **1795** 字节；`task2_code` 137 → **472** 字节 |

以上均按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. 边界与反例保留

- **携带 cap 的消息不能投给已阻塞的接收者（登记）**：`SYS_SEND` 在 `BlockedRecv` + `cap_count>0`
  时返回 `EINVAL`，而模型层的直投路径本身支持搬 cap。原因是**跨 CR3 写接收者用户缓冲未实现**；
  静默把 cap 挂到一个未被告知句柄的接收者名下，只会制造它无法寻址却仍然生效的 Authority。
  本切片的见证只用「排队路径送 cap + 直投路径送无 cap 消息」，故该限制未被触发。
- **跨任务只以负例闭合（登记）**：按用户决策，不给任何任务对他方地址空间的管理权。因此切片 R
  报告里「跨 Task `TaskManageVSpace` 未覆盖」这条边界是**以拒绝路径**覆盖的，不是以成功路径。
- **发送授权是「接收者绑定」，但不是「每接收者一 cap」**：`TaskSend` 权限本身仍可从任意
  Task 目标的 cap 派生；绑定的是**目标对象必须等于 receiver**，不是「cap 与该 receiver 一一对应」。
- **`memseg_map` 仍把 `root_phys` 当普通参数**：syscall 层现在由 task cap 目标派生根，
  消除了调用侧的错配；但 `BootstrapState::memseg_map` 内部仍不做「root 与 task_id 一致」的校验
  （内核直映射路径依赖这一自由度）。
- **共享 Segment 存活不销毁**：`frames_ready.segment=17`；这是刻意保留（演示结束后共享关系依然
  成立），代价是守恒式中 segment 类多 1 帧、free 少 1 帧。
- **task 2 不再零 cap（登记）**：它持有槽 48 的自管理 cap。该 cap 只含 `TaskManageVSpace` 且目标为
  `Task(2)` 自身，不授予它对他方地址空间或消息通道的任何权限。
- **`InMessage` 位置首次被真实走到**：此前 `move_to_message`/`AuthorityError::PositionMismatch`
  既无运行路径也无测试；本切片新增 host 测试覆盖「queued cap 下 `validate_authority` 通过」与
  「承载消息消失 → `PositionMismatch`」。
- 未实现：S2-D4（Task create/yield/destroy 用户原语）、`dev_vspace`/W16.4、`REFINEMENT-PROOF`。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 6. 关闭口径与下一步

- **不勾选 S2-D5/S2-X2 任何退出条件、阶段 2 维持「进行中」**：证据虽为真实内核 + QEMU，
  独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛，不为「收尾」放宽。
- 下一步候选：
  1. S2-D4（Task create/yield/destroy 用户原语）——阶段 2 最后一个未交付项；
  2. S2-X1（每项成功和失败路径的状态转换测试）在 D4 落地后一并收口；
  3. 阶段 2 各退出条件的范畴外复核；
  4. `ISSUE-FRAME-001`（image Segment 转 `Vector<FrameId>`）。
