---
id: report-stage6-user-copy-transactions
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M1 接线前置：真实页表输出预检与 COW 事务

## 结论与范围

在 `9dd0512` 基线上推进 [M1a 后继](2026-10-29-stage6-m1-blocker-closure.md)：
把 [M1 协议 §3–4](../specs/2026-10-29-stage6-m1-protocol.md) 所需的全范围输出预检、
显式 root 读入和 COW 分配回滚落实到真实内核页表原语，并接入原生输出、IPC 接收缓冲和
wait 状态写回。失败的第二页不再留下前页 COW 映射改变或未归还的预留帧。

这是 **M1b wait 输出事务 / M1c 代理复制的内存基础**，不完成 Gate 能力事务或跨任务代理调用。
未新增 Gate/Request 对象、权利或 syscall；73–78 仍未发布。Gate trap、能力最后授权丢失、
审计全局 fail-stop、G1–G3 与完整 LibOS 仍待实现，阶段 6 退出条件不变。

## 基线与输入

基线 `9dd0512`，起始工作区干净。契约链：
[阶段 6–7 细化计划](../plans/2026-10-27-stage6-7-refinement.md) →
[递送提案](../specs/2026-10-29-stage6-m1-delivery-design.md) →
[R1–R6 发现](2026-10-29-stage6-m1-decision-review.md) →
[M1a 协议](../specs/2026-10-29-stage6-m1-protocol.md)与
[整改及未覆盖项](2026-10-29-stage6-m1-blocker-closure.md)。
既有 COW/四字节 wait 输出的边界见
[fork 续执行契约](../specs/2026-10-24-fork-continuation-design.md)与
[报告](2026-10-24-fork-continuation-report.md)；本次明确收紧其允许部分 PTE 私有化的准备失败语义。
工具链仍为 `nightly-2026-07-30`；QEMU 沿用 TCG 单线程、1 vCPU、128 MiB 的仓库配置，
没有 KVM/实机或性能主张。全部构建离线使用已有依赖；未改依赖、容量或栈守卫。
完整命令获准在沙箱外运行，原因是既有 Linux N1 / R3 参考需要主机套接字；
未降低权限检查、跳过参考或修改超时。前序版本的沙箱失败仍保留在 M1a 报告。

## 发现与修复

| 基线行为 / 反例 | 本次修复 |
|---|---|
| `copy_user_bytes` 在源第二页缺失前已改写第一部分内核 scratch | `read_user_bytes(root, ...)` 先解析两页，全部成功才写 scratch；同 VA 不同 root 独立实测 |
| 原生 copy-out / IPC 输出只在叶不可写时调用 COW helper；叶 RW、上层页表 RW 清除仍可能经物理直映射写入 | 完整输出统一验证四级 present/user，写方向另验证中间层 writable、拒绝大页及真正只读/隐藏叶 |
| 原四字节 COW 准备允许先改第一页 PTE，再因第二个帧分配失败而返回；用户字节虽未变，映射和帧账已变 | 所有页权限先过检，所有 COW 帧先预留；失败按原页 Segment 归还，PTE、字节及帧归属保持；成功才复制旧页、提交 PTE、刷新 |
| IPC 输出预检与 wait 状态写回各自维护页检查逻辑 | `prepare_task_output` 共用单一路径；IPC 在消费消息/移动 token 前获得物理写计划，提交时无可失败页表遍历 |

`vspace::USER_COPY_LIMIT=4096`，任意不对齐范围至多覆盖两页，无动态分配。
现有 `copy_user_bytes` / `copy_out_user` 调用点均在此上限内；不是新增用户 ABI 限额。
检查加法溢出、低半区及完整长度。底层零长度不解引用 root/指针；原生调用入口仍保留各自
指针/Task 检查，未来 Gate 必须先检查活动请求和两端身份，不能用零长度绕过授权。

`PreparedUserWrite` 只保存已验证的物理片段；它不是能力，不允许跨调度或映射修改保存。
调用者须保证 root/映射/帧从准备到提交均独占。本次调用点在单核不可抢占内核路径内消费计划，
不会在阻塞服务时保留物理地址；IPC 阻塞登记仍只保存用户地址，唤醒时重新预检。
COW 通过普通帧分配入口取得带 Segment 归属的帧，失败经 `release_frame` 撤销归属；
物理帧 0 用 `Option` 表示，不能误作“没有分配”的哨兵。
活动 root 在提交后逐页 `invlpg`；非活动 root 下次写 CR3 激活时刷新，不对服务当前根误刷同 VA。

这只保证本内存准备操作的失败回滚：更大的控制调用若在成功准备之后因授权、消息或审计检查
失败，已经私有化的 COW 页仍可能保留；没有宣称把既有 IPC 的全部状态变更合为一个事务。
未来 Gate wait 须在准备前先完成其余可失败预检，并把 token、输出、出队和审计一起提交。

## 本次测试与观察

新增 4 个页表测试组，并增强既有四字节输出负例：

- 显式两个 root 的同 VA，0/64/4096 字节，不对齐跨页；源内容与目标相邻字节保持。
- 4097、溢出、零地址、高半区、空操作；第二页未映射/只读/隐藏时不分配、不改变第一页。
  源权限失败不改 scratch；既有逐级权限测试新增读方向检查。
- 第二个 COW 帧分配失败时，完整页表快照不变、无 TLB 刷新，所有已预留帧归还。
  另以真实 `FrameRegistry` / `FreeFrameAllocator` 的仅一帧池验证 Segment 归属、守恒和可再次分配。
- 物理帧 0 的成功 COW，通过非恒等直映射及实际 PTE 检查，避免零值哨兵丢失预留。

W9 同源 C 探针新增 `copy=1`：先 mmap 两页并填 `0xa5`，fork 后子进程以 raw
`rt_sigprocmask` 查询把 8 字节旧 mask 写到页尾前 4 字节，两页在内核写入前仍为 COW。
子进程检查完整输出及所有相邻字节，父进程 wait 后检查两页完整原值。
Linux 参考与 Prima ring3 都实际运行这条检查；未把 Linux 的 EFAULT 部分输出行为当作原子性契约。
`xtask` 严格 stdout 预期新增该字段，不能缺失或为 0。
完整套件 W9 最后一轮串口保存在[归档](artifacts/stage6-user-copy/w9-last-run.txt)，
只将 CRLF 归一为 LF：原始 21092 字节，SHA-256
`e07dcf8066f6d94754d0c2fe7705e2a73c6a4286f788b63bf1797aa0da87c5eb`；
归档 21006 字节，SHA-256
`f448824216c5fc08a2ca7a7cdd5b4a3cbfb29644a66e33018adcbe248bedecc7`。
其余轮次不从这一个串口文件倒推，由完整命令的逐轮校验结果记录。
host 命令日志仅移除文件末尾多余的空行，以通过 Git 空白检查。

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib vspace::tests` | PASS：32 个真实页表逻辑测试（host 物理直映射），包含上述反例；[日志](artifacts/stage6-user-copy/vspace-tests.txt)，不是 Gate 请求授权测试 |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W9` | PASS：Linux 参考及 ring3 QEMU 双轮，`copy=1`；[命令日志](artifacts/stage6-user-copy/w9-command.txt) |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1039 passed、1 个既有外部 ELF 样本 ignored；[日志](artifacts/stage6-user-copy/workspace-tests.txt)，未运行 Lean |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机内核编译、kernel/src 32 文件零 unsafe；[日志](artifacts/stage6-user-copy/lint.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：沙箱外获准的单次完整命令 exit=0，36 程序 / 72 轮，全部主机参考通过；[日志](artifacts/stage6-user-copy/full-suite.txt) |
| 文档四项校验、`git diff --check` | PASS：`self-test`、`build`、`check`、`check --base 9dd0512`；332 文档，变更及链接一致 |

## 限制、后继与阶段影响

本次补强 M1a 报告明确未覆盖的真实 PTE / 可失败帧分配，不删除旧报告或追认旧版本已有这些性质。
真实页表 host 测试与 ring3 成功 COW 路径是不同证据：耗尽注入只在 host 运行；未声称 QEMU
已跑 Gate G3，或真实服务已获得代理读取另一 Task 的能力。

Gate 接线仍必须验证请求 Delivered、固定服务者、token 当前位置/代次、Task cookie/root 身份、
Segment 用户映射范围与设备页排除，拒绝共享 root，并把 VM map/unmap/protect/exec 串行化到
同一事务。该底层 helper 接受可信 root，不承担能力授权、pin 协议或 SMP 锁。
审计耗尽全局停止控制器、最后 GateServe 丢失钩子、私有等待/信号隔离与 rax-only 回复仍为 M1b 工作。
没有勾选 S6-D2 或任何退出条件；当前状态只在[阶段跟踪](../../../Prima%20OS%20实现阶段.md)维护。
