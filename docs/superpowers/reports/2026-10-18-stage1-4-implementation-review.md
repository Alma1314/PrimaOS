# 阶段 1–4 实现复核（阶段 5 输入）

**日期**：2026-10-18
**基线**：本地 `master` `eba03c4`（SPEC-151）；本轮开始前仅有阶段 5 准入分析的文档改动
**范围**：逐项回读 S1–S4 的退出报告、对应机制入口、ring3 服务和 xtask 断言；运行当前基线的 host、lint 与 BIOS QEMU 验收。此报告是针对阶段 5 前置的实现复核，不是新的组织级签署或完整形式化细化证明。

## 1. 复核结果

| 阶段 | 代码与可重复见证 | 本轮判断及对阶段 5 的影响 |
|---|---|---|
| 1 启动/Authority/页帧 | `bootstrap.rs::create_state/validate_authority` 建三条初始根并校验 W5；SPEC-150/151 补齐初始审计、非 init 结构根、追加式 Cap 历史与单调 CapId；`frames.rs::check_conservation/check_disjoint` 逐帧核对；xtask 检查 `authority_ready` 和 `frames_ready`。 | 已签 S1 条件在当前代码和本轮回归中仍有对应证据。`ISSUE-AUTH-001` 的 kernel image 与规范 init image、同段 RWX 细化分叉仍被接受；阶段 5 的用户 ELF 装载和 W^X 不能沿用这一分叉当作已解决。 |
| 2 Capability/Segment/Task/IPC | `handle.rs` 与 `bootstrap.rs` 有 generation、权限收窄、Segment 清零与真实映射、Task create/destroy、FIFO/阻塞收发；xtask 的 `handle_ready`、`memseg_ready`、`task_ready`、`ipc_ready` 和负例覆盖既定路径。 | 已签条件的既定路径保留。`Task` 是当前内核任务，不等于 POSIX 进程/线程；无退出状态/等待接口。`send_call` 对阻塞接收者的带 Cap 投递另有下述未覆盖差距。 |
| 3 Namespace/Timer/Audit | `namespace.rs` 与 `bootstrap.rs` 提供绑定/挂载/解析、DFS 环检测、绝对单调 Timer、Capability 派生/位置历史；xtask 核对 `namespace_ready`、`timer_ready`、`audit_ready`。 | 已签条件的既定路径保留。Namespace 存的是 Capability 绑定，不是文件树；Runtime 路径限 64 字节。Timer 的 PIT tick 和现有 Audit 查询不能直接宣称 `clock_gettime`、目录遍历或 POSIX 文件语义。 |
| 4 原生服务/驱动/应用 | 独立 ring3 `servicemgr`、log/ns/store、驱动与客户端；`service.rs` 的 log/store 在一页 Segment 中保存 `u64` 状态，ns 代理固定 `svc` 路径解析；SPEC-130 的 8 KiB 共享数据、执行前取消、驱动 `#PF` 与其余应用继续运行由 xtask 验收。 | S4-D/X 的原生示例证据在当前基线仍可复现。现有 log/store 是专用状态服务，没有字节流 tty、fd、VFS 或网络；共享段演示也不是通用 libc I/O 协议。 |

**判决**：本轮未发现足以推翻阶段 1–4 已声明的*有界*退出见证的回归；本报告不把这些见证扩大成完整 COSIR 细化或 POSIX 支持。下列两项新增的阶段 5 前置需明确处置。

## 2. 新登记的前置与差距

### ISSUE-IPC-001：阻塞接收者的带 Cap 投递

`COSIR 原语规范.md` §9.1/9.2 要求 `send` 对已阻塞接收者也可完成消息与 Capability 的原子交付。当前 `kernel/src/bootstrap.rs::send_message_with_capabilities` 的模型路径能将 Cap 直投到接收者；但 `kernel/src/syscall.rs::send_call` 在 `was_blocked && cap_count > 0` 时提前返回 `EINVAL`。`recv_call` 阻塞时又先把接收方 24 字节 out 缓冲清零，现有直投只写恢复帧的 `rax`，没有给被唤醒的接收方写 Cap 句柄。SPEC-104/105 的 ring3 协议因此先发无 Cap 唤醒，再发带 Cap 消息。

这是**用户可达的 syscall 细化差距**，并非阶段 4 服务不存在。S2-D5 先前验证的 FIFO、无 Cap 阻塞直投和排队带 Cap 路径仍成立，但不可据此声称完整 §9.1/9.2。阶段 5 若建立通用 libc IPC，需先修此路径，或先在冻结 profile 中明示限制且不把通用带 Cap 收发标 supported。修复验收：阻塞接收方得到 payload、Cap 数和句柄；发送失败时不转移权限；排队与直投的权限/审计结果等价；QEMU 加一条 ring3 负例和成功见证。

### BLK-004：新增用户程序的容量门槛

`kernel/src/bootstrap.rs` 的 `MAX_TASKS=16`；`TaskTable::next_free_id` 只考察 ID `<16`，正常销毁保留 `Dead` 记录。阶段 4 驱动在 QEMU 中是 Task 15，现有 boot 已占满可分配 ID 区间 1–15。当前验收只启动固定原生程序，不能再据此推断可启动额外的 C 程序。`MAX_AUDIT_EVENTS=768`，SPEC-151 见证 `state_audit_ready.used=604`；更多创建/IPC 会消耗余量。扩 Task 数还牵涉调度表、内核栈、Capability/Segment/消息/Audit 容量与启动镜像大小，不宜只改一个常量。

首个 C 程序之前，应以“现有服务全活时再建一个独立用户 Task、正常退出并回收”为验收，记录峰值容量和失败时的确定错误；保留现有 43 条事件的语义校验。此项是阶段 5 扩展门槛，并非阶段 4 已完成的固定拓扑见证反例。

## 3. 当前验证

| 命令 | 结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage5-review-target cargo test --workspace --quiet` | PASS：799 个 host 测试，覆盖模型、内核、runtime、xtask；不执行 ring3 |
| `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage5-review-target cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机内核构建、`kernel/src` 零 `unsafe` |
| `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage5-review-target cargo xtask test` | PASS：kernel 构建、BIOS/UEFI 镜像、BIOS QEMU run=1/2、stage1 boot slice；验证固定启动协议及 xtask 断言 |

`xtask test` 首次在受限工作区因无法写入 `target/prima-os/prima-os-ramdisk.img` 而中止；获授权后在 `master` 工作区重跑并全部 PASS。测试通过只证明已有场景，不能覆盖上面的新增 Task 与带 Cap 阻塞直投。
