---
id: compatibility-stage6-gate-native-abi
kind: compatibility
authority: scoped-native-abi-ledger
status: experimental
lifecycle: current
updated: 2026-10-31
---

# 阶段 6 Gate 原生机器入口台账

本台账登记 [D17](../specs/2026-10-29-stage6-m1-mechanism-decisions.md) 允许追加、且已接入分派的
入口，参数契约为 [M1 协议 §1–5](../specs/2026-10-29-stage6-m1-protocol.md)。号码的唯一源码
为 `prima-abi::syscall::NUMBERS`；既有 1–72（52 保留）逐号冻结测试保持。此处不发布稳定
外部 ABI、不改变阶段 5 profile，也不提供 Linux syscall 语义或 Runtime 高层包装。
入口初次接线与运行范围见[接线报告](../reports/2026-10-31-stage6-gate-dispatch.md)：G1两轮通过，
该轮普通BIOS全套有W7一轮30秒超时；本表登记入口，不作整套验收或阶段签署。

| 原生号 | 入口 | 六个参数 | 成功结果 |
|---|---|---|---|
| 73 | `SYS_GATE_ATTACH` | task_cap, gate_or_zero, orphan_value, 0, 0, 0 | 本地 Gate 句柄 |
| 74 | `SYS_GATE_WAIT` | gate_cap, output_ptr, 96, 0, 0, 0 | 0；输出 12 个小端 u64 |
| 77 | `SYS_GATE_REPLY` | request_cap, value, 0, 0, 0, 0 | 0；只改客户端 rax |
| 78 | `SYS_GATE_ABORT` | request_cap, value, 0, 0, 0, 0 | 0；与 reply 同提交，审计原因不同 |

75/76 尚未分配，调用仍为原生 ENOSYS；代理复制留 M1c。后续增加入口、共享 root 或整帧答复
必须同步更新本表、源码号表、协议与证据，不能修改既有号。

控制调用的普通错误为 EINVAL（编码/保留参数/输出形状）、ENOENT（失效）、EACCES（授权/身份）、
EAGAIN（准入状态/未消费原生返回或恢复续体/共享 root）、ENOSPC（能力/等待/绑定容量）和 EFAULT（输出页权限/范围）。
COW 帧不足为 ENOSPC；审计耗尽先封存诊断并全局 fail-stop，绝不向用户返回 ENOSPC。
失败尝试的独立审计事件尚待补齐；当前有成功请求链和耗尽诊断，不声称失败均已逐次审计。
这些是原生控制错误；客户端收到的 value/orphan_value 是不透明 u64，不按 errno 解释。

[Ready 准入后继](../reports/2026-10-31-stage6-gate-admission.md)补齐旧 IPC、等待、异常／信号恢复、
进程状态写回及原生重启检查；排队信号保留，旧 wait4 地址自身不构成未决返回。

wait 输出要求 8 字节对齐、总长恰为 96；字段为
`[1, 96, request_cap, task_cookie, number, arg0, arg1, arg2, arg3, arg4, arg5, flags]`。
request_cap 使用服务者本地句柄视图，cookie 含 Task 代次，flags bit0 为 int80，其他位零。
空队列登记不可警报的私有等待，唤醒后重新执行原生 wait、重新验证当前输出映射；
不跨阻塞保留物理写计划。客户端的全部 syscall/int80 号（包括原生号和控制号）先递送 Gate，
孤儿绑定继续使用配置值，不退回原生分派；信号保持排队，Linux 信号语义留 M4。
