---
id: spec-stage6-m1-protocol
kind: specification
authority: scoped-mechanism-design
status: model-reviewed
lifecycle: current
updated: 2026-10-31
---

# S6-M1 协议收口：候选 A、单服务者与不可警报等待

## 适用范围与效力

本切片落实[六项决定复审 R1–R6](../reports/2026-10-29-stage6-m1-decision-review.md)，
作为[递送提案](2026-10-29-stage6-m1-delivery-design.md)在 M1a 的具体协议。
上位语义仍是 [COSIR W9/W17/§21](../../../COSIR%20原语规范.md)。
[整改与技术评审](../reports/2026-10-29-stage6-m1-blocker-closure.md)给出可执行模型证据及范围。
这是机制设计评审，不是组织级签署、Lean 定理、机器 ABI 发布或 S6-D2 完成。
M1b 必须按本文落实真实 trap/能力/页表/审计事务，再取得 G1–G3；不能把 host 模型接线成内核实现。

## 1. 拓扑、授权与准入

- 一个 Gate 有一个固定服务 Task；多个客户端共享它。服务者必须保持原生分派且不能被门控，
  客户端不能同时是另一个活动 Gate 的服务者，排除自调用及服务环。
- `attach(task_cap, gate_or_zero, orphan_value, flags=0)`：调用者必须实际持有目标 Task 的
  `TaskManageVSpace` 能力；第一次挂接铸造 Gate 根能力，后续挂接要求同一服务者持有 GateAttach。
  `gate_or_zero=0` 新建 Gate；否则向已有 Gate 增加绑定，返回调用者已有的 gate 句柄。
- Gate 根权利为 `{Derive, GateAttach, GateServe}`，请求根为 `{Derive, GateServe}`；
  GateAttach 只用于 Gate，GateServe 用于 Gate/Request。权利追加位 31/32，保留 0–30；
  **只在相应对象接线时加入枚举**。本次提前将内核/Runtime 位集拓宽到 u64，不提前铸造或登记新权利。
- 服务能力、请求能力可有别名，但所有使用必须同时验证 token 活动、当前位置、对象代次、
  固定服务 Task 身份和请求 Delivered 状态。移到其它 Task/邮箱/Namespace 不能转移服务身份。
  最后一个由原服务 Task 持有的 GateServe 消失，或某 Delivered 请求最后一个原服务者持有的
  请求能力消失，均孤儿化整个 Gate；不得遗留无法答复的请求。移动与丢弃共用同一检查路径。
- 目标须为本地 CPU Task、Ready、无其它未决内核等待或返回；v1 只准入独占地址空间的
  单线程任务。初次 Linux 装载须先用可信停车入口保证挂接前不会执行 Linux trap（M2 装载义务）。
  挂接后禁止直接 exec/clone/更换 root；未来开放这些行为须在 M4 扩展生命周期。
- 每个绑定在 attach 时实际预留一个请求槽及一个私有 WaitObject；先预检全部资源再提交。
  Gate≤16、绑定/未决槽≤64、每 Gate 可递送队列≤8；这些是 M1a 模型常量，不是 ABI 保证。
  活动同步请求至多一个/Task，槽在空闲时仍属于该绑定；其它 Gate 不能借用。
  新 Gate 另预留一个私有服务等待对象，因此等待对象预算≤80（64 客户端＋16 服务队列）；
  服务空队列等待也不得在阻塞入口临时分配。
- 客户端销毁才解除绑定并归还预留；孤儿化只退役服务授权，保留 fail-closed 绑定。
  Gate 空间可在无绑定后回收，但句柄代次不得回绕；模型保守地不复用 Gate 索引。

## 2. 请求、答复与信号

状态为 `Idle → Queued | WaitingCapacity → Queued → Delivered → Terminal → Idle`。
WaitingCapacity、Queued、Delivered 全都使用 attach 已预留的槽；容量等待 FIFO 最多 64 项。
服务 dequeue 后立即按 FIFO 晋级一个容量等待者；任一阶段任务销毁均删除登记并重新晋级。
同一 request id 不复用，所有能力别名引用相同的终结状态，不靠释放某一个句柄实现一次性。

候选 A 使用真实的私有 WaitObject 和现有 `WaitRequest`：entries 只有该对象，mode=Any，
`alertable=false`、`deadline=None`；Task=BlockedWait，pending_return/pending_wait_result 为空。
对象不向用户铸造 WaitWait/WaitSignal/WaitDestroy 能力；普通 signal/destroy 不能先于 reply 唤醒客户端。
reply 的受控提交直接清理 wait_request/waiters、写保存帧 rax、置 Ready，并记录请求终结和状态迁移。
这新增的是私有完成迁移，不修改公开 `signal_object` 的 §21 语义。

**v1 所有门控等待均不可警报**：到达的 upcall/信号只排队，不取消、不选择 EINTR、不退回 rip、
不调用 `mark_blocked_call`/`prepare_signal_resume` 的原生号重启分支。
这显式选择比原提案更小的 M1 范围，无须另造取消请求/确认 ABI。服务可通过 `abort(req, value)`
确认终结；abort 与 reply 同样搬运不透明 u64，但审计原因不同。Linux 信号语义及可中断等待在 M4 定义。
客户端被销毁时不恢复帧、不重放；服务死亡/最后服务授权消失按 orphan_value 终结。
已提交日志/复制等副作用不回滚，只有每请求终结与帧写入至多一次的保证。

服务活着但不答复时允许无限等待；监督者可显式终止服务以触发孤儿清理。
未来若要超时/租约，须另立协议及反例，不从“有退出路径”推导时间上界。

## 3. 输出与候选调用布局

73/74/77/78 已按[接线检查点](#12-trap-与机器入口实现检查点2026-10-31)在 `prima-abi`
追加并接入分派，仍为实验性 ABI；75/76 尚未分配。下表中 copy 为 M1c 适配目标，不更改 1–72。

| 候选号 | 调用 | 寄存器参数与结果 |
|---|---|---|
| 73 | attach | `(task_cap, gate_or_zero, orphan_value, 0, 0, 0) → gate_cap` |
| 74 | wait | `(gate_cap, output_ptr, output_bytes=96, 0, 0, 0) → 0`；空队列阻塞服务者 |
| 75 | copy_in | `(request_cap, client_va, len, service_dst, 0, 0) → len` |
| 76 | copy_out | `(request_cap, client_va, len, service_src, 0, 0) → len` |
| 77 | reply | `(request_cap, value, 0, 0, 0, 0) → 0` |
| 78 | abort | `(request_cap, value, 0, 0, 0, 0) → 0` |

wait 输出为 12 个小端 u64，8 字节对齐、总长恰为 96：
`[version=1, bytes=96, request_cap, task_cookie, number, args0, args1, args2, args3, args4, args5, flags]`。
flags 的 bit0 记录 int80（0=syscall）；其它位必须为零；记录来源不承诺 i386 personality。
这里的 task_cookie 必须含 Task 代次，不能只用可复用的 id。

服务在空队列时登记到该 Gate 的私有服务等待对象，使用单项、不可警报、无 deadline 的
BlockedWait；入队只唤醒这项服务等待并重试原生 wait，不完成任何客户端请求。孤儿化清理
服务登记；若服务仍存活，以原生 stale 错误结束控制调用，输出保持不变。服务对象也不铸普通
WaitSignal 能力。host 模型的空队列 wait 返回 Busy 表示适配器检查点，未模拟这段服务调度。

wait 在出队前验证整个输出范围、写权限、COW 准备、句柄安装位置与审计容量；失败不出队、
不安装半个 token、不写半个输出。成功把能力安装、96 字节输出、Delivered、容量晋级与审计
作为单一提交。服务唤醒后重新验证其 root/输出地址；不把阻塞前的用户指针当作固定物理页。

## 4. 复制权限、地址空间与原子性

**明确选择整个目标用户地址空间授权**。可信管理者把 TaskManageVSpace 交给服务者即信任它
读写该 Task 的用户内存。参数快照并非内存快照，4 KiB 也不是权限边界。
copy 改用显式 client_va，支持 iovec 等嵌套地址；这局部取代 D19 的 `args[index]+offset` 寻址限制，
没有引入映射授权，也不声称参数级最小授权。需要参数级隔离时须有来自可信端的范围授权新协议。

每次 copy 必须验证当前服务者与 Delivered 请求，选择请求绑定的 Task 代次、root 身份与映射
代次，独占内存事务直到结束。禁止用 `sched::current_root()` 访问客户端；服务本地缓冲使用服务 root。
v1 不接收有其它线程共享 root 的目标；管理者的 map/unmap/protect/exec 在绑定期间须拒绝或串行化
到同一事务，不能在预检与提交之间改映射。页 pin/COW 准备不经 fault 路径递归进入服务。

`0≤len≤4096`，两端 address+len 检查溢出，逐页检查完整用户范围、读/写权限与 Segment 边界；
内核/设备/隐藏页拒绝。先读完整输入到 scratch、完成全部目标验证与 COW 准备，再写用户可见字节。
失败不改目标字节；PTE/分配回滚是另一个主张，M1c 须单独说明，不能由 host Arc-COW 例子推出。
零长度不解引用用户指针，但仍验证请求、两端 root 身份与长度运算。

## 5. 审计耗尽与失败关闭

reply/abort/销毁/孤儿化的 frame、wait、token、队列、槽位与审计须事务提交。
审计对象至少有 gate/request/task_cookie/service_cookie、原始 number/args、迁移原因与原始返回值；
它不解释 Linux 调用。copy 的审计另含方向/地址/长度；普通错误保留机制错误与尝试者身份。

本切片不引入日志回收或无界动态存储。审计池耗尽采用明确的**全局 fail-stop**：
在变更前预检，保持保存帧和已有登记，封存一个预留的首次耗尽诊断单元，停止调度/处理门控任务，
不得把内部 AuditFull 返回为客户端 syscall 结果、不得原生分派、不得继续旧请求 copy/reply。
诊断单元在 attach 准入前预留；全局控制器必须处理它，不能只在某个 syscall helper 返回 ENOSPC。
此时不承诺服务死亡仍可完成清理；这是资源耗尽停止语义，恢复/日志 drain/域配额留 7-C/7-G。
这一选择牺牲可用性，保持有界与可审计性，不以“故障都返回 orphan_value”掩盖日志耗尽。

普通准入/句柄/输出失败则返回原生控制调用的确定性错误且状态不变；不是未修改客户端的 Linux errno。
孤儿绑定后续 trap 使用配置值且另记原因；资源/审计不足先停止，绝不因释放 Gate 恢复原生分派。

## 6. COSIR 技术评审与接线义务

模型复用 State 的真实 WaitObject/WaitRequest，并在每个成功事务检查 `waits_well_formed`；
私有完成没有泄露信号权，使用既有 Ready/BlockedWait，候选 A 无需新增 BlockedCall。
原始 number/args/result 不参与语义分支；因此本切片的 A 通过零 Linux 语义与 W17 的技术设计评审。
这是有限 host 反例及源码评审，不是所有可达状态的形式证明；不变更冻结 COSIR 对象表。

M1b 的必做验收：真实 Gate/Request 对象与别名当前位置检查、服务能力丢失钩子、私有等待与调度
串行化、audit fail-stop 控制器、wait 输出事务、root/Task 代次校验、rax 外寄存器保持及 G1/G2。
M1c 再落实硬件页表/COW 事务与 G3。上述属于已定义的实现/细化工作，不能从本设计直接勾选 S6-D2。
重审触发：7-A 给出反例、候选 A 无法保持 W17、开放共享 root/信号中断/整帧答复、改审计耗尽策略。

## 7. 内核前置实现检查点（2026-10-30）

[后继页表事务报告](../reports/2026-10-30-stage6-user-copy-transactions.md)实现了显式 root 读入、
全范围输出预检、COW 帧全部先预留与分配失败回滚，接入已有原生输出、IPC 接收与 wait 写回。
底层写计划只在独占 root 的非调度区间内有效；它不是授权对象，不能跨服务阻塞保存。
这补强 §3–4 的内存前置，不改变本文协议或发布 ABI；Gate 的句柄/审计/出队提交与身份检查
仍须在 M1b/M1c 实现，不能从这些原语或 W9 成功路径直接认定 G3 已通过。

## 8. 私有等待实现检查点（2026-10-30）

[后继私有等待报告](../reports/2026-10-30-stage6-private-completion-waits.md)实现真实内核
WaitObject/WaitRequest 的预留、不可警报登记、代次 ticket 完成及所有者销毁清理；普通
等待权限与原生信号/超时出口不能完成它。公共 32 与私有 80 个对象的存储预算分离。
这是候选 A 的内核等待基础；未接入 Gate attach/trap/reply，也未改保存帧或实现 Gate
全局 fail-stop。Gate 须将这些可信接口与能力/请求/帧/审计合为不可调度事务后取得 G1–G3，
不能把单个等待操作的失败原子性提升为整个 Gate 事务已完成。

## 9. 审计停止实现检查点（2026-10-30）

[后继控制器证据](../reports/2026-10-30-stage6-audit-fail-stop.md)实现独立首次耗尽诊断、
持久停止、私有等待/销毁预检，以及分派、调度和 IRQ 的全局停止检查。单核实现采用既有
terminal panic 停止整个系统。Gate 能力/请求/复制事务尚未接入；其审计字段与原子提交
仍是后继义务，不能将 host 耗尽测试当作 G1–G3。

## 10. 裸机停止验证检查点（2026-10-30）

[QEMU 后继报告](../reports/2026-10-30-stage6-audit-stop-qemu.md)针对七个既有执行边界
提供独立编译启用的审计压力注入及严格终止记录校验；移除停止调用的变异必须被拒绝。
证据使用真实私有等待与 AuditLog 容量预检，仍无 Gate/Request 能力或门控 trap，
不能替代 Gate 全事务的审计接线与 G1–G3。普通镜像不包含探针。

## 11. 服务授权丢失实现检查点（2026-10-31）

[内核后继报告](../reports/2026-10-31-stage6-gate-authority-loss.md)将 drop、批量 drop、move、
revoke、Namespace 绑定和 IPC 转移接入总审计预检与孤儿化提交。根句柄消失后依然按
实际服务别名判断授权，请求终结按对象身份回收别名。已退役请求句柄在容器中只保留原
身份供 recv/unbind 返回，不重建位置或权限；这不是 COSIR §4.3 中仍活动的 stale token。
服务空队列等待被清理后，未来 wait 适配器仍须重新验证并返回 stale。本检查点只记录
对象层 host 证据，不发布 ABI 或覆盖 trap、输出事务和 G1–G3。


## 12. trap 与机器入口实现检查点（2026-10-31）

[接线与 G1 报告](../reports/2026-10-31-stage6-gate-dispatch.md)落实 syscall/int80 截获、
原生 attach/wait/reply/abort、wait 全范围输出与本地句柄视图预检、不可警报服务等待重试、
门控信号保留和原生控制错误映射。已实现入口登记于[实验性机器台账](../compat/stage6-gate-native-abi.md)。
G1 逐寄存器核对及部分重复/孤儿/输出失败路径不代表完整 G2/G3；代理复制仍属 M1c，
也没有可信停车入口、Linux LibOS 或阶段退出证据。普通控制失败尚无独立的尝试审计事件，
§5的这一义务仍未闭合；成功链与耗尽诊断的证据不代替它。§1的Ready目标还须穷尽待领取返回/恢复续体检查，
现有准入尚未覆盖，见报告的明确偏差；G1干净停车目标不替代该反例。前面各检查点的未完成描述属于其当时范围。
