# 阶段 2 实现复核与 IPC 整改

**日期**：2026-10-18
**基线**：`master` `d9a5c78`（SPEC-147 签署之后）
**范围**：对照 SPEC-147 的 S2-D1–D5/X1–X4 退出证据，重点复核 Capability 位置、`SYS_SEND`/`SYS_RECV`、任务保存帧和 QEMU `ipc_ready` 的现行代码。本轮是签署后的实现复核，不重签 SPEC-147，也不宣称形式化细化证明。

## 发现与修复

| 项 | 复核发现 | 修复及可重复见证 |
|---|---|---|
| ISSUE-IPC-001 | `send_call` 在接收者 `BlockedRecv` 且消息带 Cap 时直接返回 `EINVAL`；模型却已经把 Cap 从发送者迁到接收者并记录 `CapabilityMoved`。旧 syscall 直投只写被唤醒帧的 `rax`，接收者的 24 字节 out 缓冲仍为零。 | `recv_call` 阻塞时保存 out 指针；`send_call` 在转移 Cap 前，按接收者页表重新验证 user/writable 映射并求物理直映射地址，成功后把 pending 的句柄和数量写到原 out 缓冲，再写 payload 到保存帧并消费 pending。失败路径不动 Capability 位置。ring3 的 Task 1 把 page-fault-source Cap 随直投交给阻塞的 Task 2；`ipc_ready` 同时核验 payload、`direct_cap0=0x100000004`、`direct_cap_count=1`。 |
| 接收缓冲地址 | `recv_out_bounds` 只查对齐和页内长度；非用户 canonical 地址可按低位走页表，却在直接写用户虚址时出错。 | 拒绝 `ptr >= 2^47`；host 测试覆盖跨页、未对齐、非用户 canonical 地址、不可写/非用户 PTE。 |
| 接收者 ID | `send_call` 将 ABI 的 `u64` 接收者参数直接截断为 `u32`，高位畸形 ID 可别名到真实 Task。 | 使用 `u32::try_from`，越界返回 `EINVAL`，不进入授权与投递。 |

模型负例在接收方阻塞时以无效 Cap 发送，断言失败后发送方仍持有 Cap、接收方仍阻塞；随后的有效直投成功。原有排队 Cap 传递、无 Cap 直投、FIFO、Task 与 Segment 见证仍由现有测试覆盖。stage 4 服务仍保留先唤醒再发 Cap 的既有协议顺序；该协议精简另行评审，不是本轮正确性前提。

## 验证与边界

- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage2-review-target cargo test --workspace --quiet`：PASS，含新增模型与缓冲边界测试。
- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage2-review-target cargo xtask lint`：PASS（fmt、Clippy、裸机内核构建、零 `unsafe`）。
- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/tmp/prima-stage2-review-target cargo xtask test`：PASS（BIOS QEMU 双跑，`ipc_ready` 新字段必须匹配）。

本轮抽查未推翻 S2 既有声明的其余路径；验证仍是有限 host/QEMU 见证，不能替代所有并发、设备或形式化安全结论。阶段 5 的 BLK-004 Task 容量门槛仍独立存在。
