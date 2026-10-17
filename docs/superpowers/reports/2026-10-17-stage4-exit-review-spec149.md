# SPEC-149：阶段 4 退出条件复核与签署

> 文档类型：阶段 4 退出条件复核/签署记录
>
> 日期：2026-10-17
>
> 基线：`6de2cb8`（阶段 3 签署后的 HEAD）
>
> 范围：S4-D1–D5、S4-X1–X4；依据 SPEC-130 技术退出报告与当前 QEMU 验收。
>
> 结论：**S4-D1–D5、S4-X1–X4 通过（实现 + host 测试 + QEMU）。** 阶段 4 正式晋级条件已满足，状态改为“已完成”；当前阶段推进到阶段 5（未启动）。

## 1. 复核方法

- 基线 `6de2cb8`；回读 SPEC-130 阶段 4 技术退出报告、`Prima OS 实现阶段.md` 的 S4 行与 `prima-user`/`kernel/src/user.rs`/`protocol.rs` 相关交付。
- 运行 `CARGO_NET_OFFLINE=true cargo xtask test`：`PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1`、`PASS qemu bios run=2`、`PASS stage1 boot slice`，退出 0。
- 既有 `cargo test --workspace` 786 passed 与 `cargo xtask lint` 通过；本复核不新增代码。

## 2. 交付项与退出条件

| ID | 证据 | 结果 |
|---|---|---|
| S4-D1 | `prima-runtime` 原生 API/版本协商；SPEC-084/085 + SPEC-130 | 通过 |
| S4-D2 | init / 服务管理器；SPEC-103–105、109 + SPEC-130 | 通过 |
| S4-D3 | log / Namespace / store 三个有状态基础服务；SPEC-106、110–112 + SPEC-130 | 通过 |
| S4-D4 | 第一条 ring3 用户态驱动（COM1 受控 I/O）；SPEC-113/114 + SPEC-130 | 通过 |
| S4-D5 | 使用消息与共享 Segment 的原生示例应用；SPEC-129 + SPEC-130 的 8 KiB 跨页路径 | 通过 |
| S4-X1 | 不依赖 POSIX LibOS 自举并运行 ring3 Rust 原生客户端；QEMU BIOS 双次启动 | 通过 |
| S4-X2 | 服务按 §8.1 最小权限拆分；启动表列出初始 Capability、空隔离 Namespace 与运行期授予 | 通过 |
| S4-X3 | 共享 Segment 传 8 KiB 跨页数据；执行前取消、长度拒绝与重试由双侧 trace 校验 | 通过 |
| S4-X4 | 驱动从 ring3 写无权客户端 VA 触发真实 `#PF`（err=0x6），驱动 `Faulted`，无关客户端继续完成两次校验并到 `boot_complete` | 通过 |

## 3. 已知边界（不改变技术退出结论）

- S4-X3 的取消发生在执行前；退出条件只要求取消与错误恢复，不要求执行中的并发取消。
- S4-X4 覆盖单核 ring3 驱动的未映射页写故障与其他应用继续运行；未扩展到其他 CPU 异常、多核、异常订阅者通知或故障服务自动重启。
- EXO-3 的完整 ring/任意长度消息、EXO-4 其余设备路径、DOC-007 新机制的实现与证明属于**后续硬化**，不在本阶段退出定义内；不得把它们记成已闭合。
- 阶段 5/6（POSIX 源码兼容、Linux x86_64 静态 ELF）尚未启动；其上游前提与 BLK-003/DOC-002 需在阶段 5 启动前评审。

## 4. 结论与签署

- **S4-D1–D5、S4-X1–X4 全部通过。** 阶段 4 技术退出条件与跨阶段正式晋级条件均满足；状态改为“已完成（2026-10-17，SPEC-149）”。
- 当前阶段推进到阶段 5（未启动）；阶段 6 未启动。
- 证据等级：真实内核/ring3 + host 测试 + QEMU 集成断言；**不构成** `REFINEMENT-PROOF`、所有 §17 安全结论或后续硬化项的完成证明。
