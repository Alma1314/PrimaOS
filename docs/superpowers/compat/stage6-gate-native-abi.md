---
id: compatibility-stage6-gate-native-abi
kind: compatibility
authority: scoped-native-abi-ledger
status: experimental
lifecycle: current
updated: 2026-11-03
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
| 75 | `SYS_GATE_COPY_IN` | request_cap, client_va, len, service_dst, 0, 0 | len；≤4096字节，全范围预检后提交 |
| 76 | `SYS_GATE_COPY_OUT` | request_cap, client_va, len, service_src, 0, 0 | len；≤4096字节，全范围预检后提交 |
| 77 | `SYS_GATE_REPLY` | request_cap, value, 0, 0, 0, 0 | 0；只改客户端 rax |
| 78 | `SYS_GATE_ABORT` | request_cap, value, 0, 0, 0, 0 | 0；与 reply 同提交，审计原因不同 |

75/76已由[代理复制与G2/G3后继](../reports/2026-11-01-stage6-gate-copy-lifecycle.md)接入。后续增加入口、共享 root 或整帧答复
必须同步更新本表、源码号表、协议与证据，不能修改既有号。

控制调用的普通错误为 EINVAL（编码/保留参数/输出形状/复制长度超过4096）、ENOENT（失效）、EACCES（授权/身份）、
EAGAIN（准入状态/未消费原生返回或恢复续体/共享 root）、ENOSPC（能力/等待/绑定容量）和 EFAULT（输出页权限/范围）。
COW 帧不足为 ENOSPC；审计耗尽先封存诊断并全局 fail-stop，绝不向用户返回 ENOSPC。
[失败尝试后继](../reports/2026-10-31-stage6-gate-control-rejections.md)在这四个入口补齐逐次拒绝事件：
尝试者 id/cookie、原始控制号/六参数和返回错误编码。非法句柄只作为原始参数，不断言其对象身份；
入口先预检一条失败日志容量，耗尽先停止；成功事务仍预检全成本。
[复制后继](../reports/2026-11-01-stage6-gate-copy-lifecycle.md)将同一拒绝审计接入75/76，范围不扩至其它原生调用；
成功复制另记`GateCopied`（Gate/Request、双端cookie、地址、长度、方向）。
这些是原生控制错误；客户端收到的 value/orphan_value 是不透明 u64，不按 errno 解释。

[Ready 准入后继](../reports/2026-10-31-stage6-gate-admission.md)补齐旧 IPC、等待、异常／信号恢复、
进程状态写回及原生重启检查；排队信号保留，旧 wait4 地址自身不构成未决返回。

wait 输出要求 8 字节对齐、总长恰为 96；字段为
`[1, 96, request_cap, task_cookie, number, arg0, arg1, arg2, arg3, arg4, arg5, flags]`。
request_cap 使用服务者本地句柄视图，cookie 含 Task 代次，flags bit0 为 int80，其他位零。
空队列登记不可警报的私有等待，唤醒后重新执行原生 wait、重新验证当前输出映射；
不跨阻塞保留物理写计划。客户端的全部 syscall/int80 号（包括原生号和控制号）先递送 Gate，
孤儿绑定继续使用配置值，不退回原生分派；信号保持排队，Linux 信号语义留 M4。

copy的服务缓冲使用服务root，客户端使用attach保存的root及代次；同物理地址重装也拒绝。
前序实现禁止全部管理者修改绑定映射；[受管理装载后继](../specs/2026-11-01-stage6-managed-launch.md)
将权限收敛为仅固定服务者、仅Delivered期间、持TaskManageVSpace的逐调用串行事务，其它管理者仍拒绝。
复制事务不跨阻塞/调度保存。两端逐页检查Segment普通RAM归属、
逻辑读写权限和硬件用户权限，先完整读入scratch，再全目标COW预留并提交。
零长不解引用指针，但仍验证请求、root及两端地址加法；失败保持目标字节，COW预留失败也保持PTE及帧归属。

孤儿绑定的后续trap保持fail-closed，并原子追加request=0的原始`GateTrapped`与结果`GateTerminated`；
request=0不是可交付请求。两条审计空间不足先停止，不能用只有结果的事件丢失客户端身份。

## 受管理的初始装载（79–82，实验性）

实现契约见[停车与装载](../specs/2026-11-01-stage6-managed-launch.md)，1–78不重编号、不改73–78布局。

| 号 | 名称 | 参数/结果 |
|---|---|---|
| 79 | TASK_CREATE_HELD | `(request_ptr,0,0,0,0,0) → manager_cap`；11字请求且initial_caps为空 |
| 80 | TASK_LOAD_IMAGE | `(task_cap,descriptor_ptr,0,0,0,0) → 0`；6字描述符，源属于服务 |
| 81 | TASK_START | `(task_cap,gate_or_zero,orphan,flags,0,0) → gate_cap或0`；flags=1原子挂接再发布，0原生发布 |
| 82 | TASK_SET_TLS | `(task_cap,fs_base,0,0,0,0) → 0`；Ready/初始停车/固定服务者Delivered |

79–82复用普通控制拒绝审计及全局fail-stop；成功另记停车/发布迁移、TaskImageLoaded和TaskTlsSet。
80保留ENOEXEC与EFAULT的区别；82拒绝非用户规范基址。TaskInfo selector8返回当前结构性Namespace的既有句柄。
本轮[统一验收](../reports/2026-11-02-stage6-process-and-differential.md)实际双轮执行D12白名单及M3/M4；
范围为只读原始ELF载体和固定服务者，不扩为任意VFS执行或稳定外部ABI。

## 独立上下文与共享线程（83–86，实验性）

[上下文契约](../specs/2026-11-01-stage6-gate-context.md)与
[共享线程契约](../specs/2026-11-01-stage6-gate-threads.md)局部扩展D18/独占root；73–78布局及rax-only保持。
TaskContext为Task权利bit33；普通Task管理根不自动获得。新根有独立撤销寿命，不是管理根的派生子集。

| 号 | 名称 | 参数/结果 |
|---|---|---|
| 83 | TASK_CONTEXT_AUTHORITY | `(manager,0,0,0,0,0) → context_cap`，实际TaskManageVSpace显式授权 |
| 84 | GATE_READ_FRAME | `(request,context,out,bytes,0,0) → 0`；8字节对齐，bytes=160或672 |
| 85 | GATE_REPLY_FRAME | `(request,context,in,bytes,0,0) → 0`；完整用户帧校验后一次终结；672另含FXSAVE |
| 86 | GATE_SPAWN_THREAD | `(request,context,in,bytes=176,fs,gate_cap) → manager`；160字节帧后接两个可选id输出地址，返回管理根显式含TaskContext |

20字帧沿用SretFrame线序；672字节格式在160后追加512字节FXSAVE，不支持XSAVE/AVX。
86还需实际GateAttach；新Task共享当前Process/root，继承请求者保存FPU，子绑定安装及可选id写出先于调度。
只有显式共享模式允许同Process/同Gate的受管root共享者；所有copy/context/VM逐次重验完整共享域。
成功访问/授予/创建与普通拒绝均审计；耗尽仍全局fail-stop。[构建记录](../reports/2026-11-01-stage6-context-signals-threads.md)
的未运行范围由[统一验收](../reports/2026-11-02-stage6-process-and-differential.md)显式后继：host反例及P1/P2/pthread双轮运行通过，
只覆盖报告登记的拒绝与往返组合，不声称完整Linux信号或全部硬件故障组合。


## 受管进程与审计（87–89，实验性）

契约：[受管进程与审计读取](../specs/2026-11-02-stage6-gate-process.md)。
87/88同时要求Delivered、固定Gate服务者、TaskContext和实际TaskManageVSpace，且仅单线程Process。

| 号 | 名称 | 参数/结果 |
|---|---|---|
| 87 | GATE_FORK | `(request,context,manager,gate,0,0) → child_manager`，独立COW root、保存GPR/FS/FXSAVE，绑定先于可调度 |
| 88 | GATE_EXEC_IMAGE | `(request,context,manager,descriptor,0,0) → 0`，新root/入口/栈、FS/FXSAVE重置、刷新绑定代次并一次终结请求 |
| 89 | GATE_AUDIT_READ | `(gate,cursor,out,128,0,0) → 0/1`，固定服务者GateServe只读查询本Gate的下一条原始trap/delivered/terminal记录 |

87不登记原生wait4亲缘或僵尸；Linux亲缘/退出/wait由用户态维护。
88完整构建成功前保留旧映像，成功后不从旧PC返回。89的16字布局及无记录不写输出语义见契约；
读取不消耗审计，不在内核识别Linux号/errno。[统一验收](../reports/2026-11-02-stage6-process-and-differential.md)
记录host反例、M4 fork/exec/wait及11项原始审计双轮核对；不外推多代进程、任意VFS执行或全部上下文组合。
