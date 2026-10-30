---
id: spec-fork-continuation
kind: specification
authority: implementation-contract
status: current-contract
lifecycle: current
updated: 2026-10-30
---

# fork 续执行、COW 提交与等待写回

## 问题与范围

收口 ISSUE-FORK-CONTINUATION-001 的实现缺口：让单线程进程经真实 musl `fork()`、
`syscall(57)` 和 `__syscall0(57)` 返回，父得到子任务 id、子得到 0，继承的用户栈不被父退栈污染。
接续 [M17 工作设计](2026-10-23-stage5-m17-fork-exec-design.md)与
[M17b-4 失败/绕行](../reports/2026-10-23-stage5-m17b4-fork-wait.md)，
遵守[原语规范](../../../COSIR%20原语规范.md)§12.8/§18 的隔离与 TLB 责任。
这是已有内核 COW 细化的修复，不把内核 fork syscall 提升为新的抽象原语或完整用户态 COW 协议。

## 工作决定与成功契约

1. `bootstrap::check_fork` 在取页表帧之前只读检查调用者、单线程、审计、Process/Task 和映射容量。
   Busy 映射 EAGAIN；其他资源拒绝保持既有 ENOSPC。模型创建仍重复检查；不依赖预检省略错误清理。
2. `clone_root_cow` 先构造完全私有的子用户页表，不改父页表；成功后才将父可写用户叶变成只读 COW，
   并同步刷新本 CPU 的父用户翻译，随后才能返回父用户代码/发布子任务。
   单核、无 PCID，使用保留当前 CR3 值的重装；用户大页/global 叶拒绝，supervisor 子树原样共享。
   源/目标根须有效且互异、目标须清零，调用方独占页表变化；新表帧须独占、合法且可通过物理直映射访问。
3. 分配或不支持的用户映射失败：逐级释放本次子表，清空子根，源表和用户数据保持不变；根帧仍由调用方归还。
   分配返回的旧帧可以不清零，构造者负责清零。任何回调不得 panic、重入或调度。
   克隆成功后的模型/登记失败也必须释放子表和根；此时父可能已只读 COW，**不承诺整个 fork 的 PTE 原子回滚**。
4. COW 页复制保留 NX 等非地址标志，替换物理地址与 W/COW 位；每一级必须满足用户/可写路径约束。
5. wait4 状态严格为 little-endian 四字节 int；NULL 省略写回，合法跨页缓冲检查两页的全部 present/user/W 权限。
   只读 COW 可先私有化，再通过物理直映射写数据；不盲写用户 VA。缺页/越界/保护错误 EFAULT，缺帧 ENOSPC。
   预检发生在阻塞/回收前，唤醒后切到接收方根并重新校验。复制失败不写任何用户字节；若第二页缺帧，
   已私有化第一页允许保留。等待中其他线程撤图/换权限尚无完整并发契约。
6. pending status 随任务清理丢弃；恢复时先完成 pending 写回再读取最终寄存器帧，错误可正确注入 RAX。
7. 适配层在子返回后记录私有 COW 的 fork-child 标记，musl `_exit`/exit_group 的子路径走已有 SYS_PROCESS_EXIT，
   避免拿继承的 manager/self cap 结束父任务。子退出暂不发送继承 fd 的 CLOSE，保留 ISSUE-FORK-CAPS-001。
   原始用户汇编直发 51 不经过此标记，不承诺其后可直接使用适配层 exit_group；原生进程退出 54 仍可用。
8. W6/W7/W7b/W8 撤去 fork 内联汇编绕行。W6 三种 C 路径依次验证父先写栈/全局、子快照/TLS errno、
   子 `_exit`、父 wait/waitpid、4 字节邻接哨兵、ECHILD；原 trace/容量/输出及双轮 IPC 基线继续执行。

## 参考与实现

本地 Asterinas 锁定版本的 VMAR fork 显式在返回前同步 TLB；这里采用同一责任顺序，独立适配现有四级页表与固定帧注册表，
不移植 VmSpace/VMAR/RCU/远端 shootdown。来源与哈希见[登记](../../architecture/asterinas-provenance.md)。
实现位于 kernel 的 vspace/syscall/bootstrap/sched 与用户态 prima_syscall；没有新依赖/号段/固定容量扩充/Lean 声明。
聚焦命令 `cargo xtask test-process [W6|W7|W7b|W8]` 共用原 validator、30 秒限时与双轮 IPC 比较，无自动重试。

## 失败与验证边界

host 覆盖每个子表分配点失败、源表逐字不变、所有私有表恰好归还、提交后刷新、用户大页/同根拒绝、
NX/supervisor 保留、跨页 COW int 写回、各级权限与地址/缺帧反例、模型容量和取消回收。
QEMU 必须验证真实 C 续执行与 musl 退出；host 不能证明 TLB 硬件生效。实际结果见[报告](../reports/2026-10-24-fork-continuation-report.md)。
SMP/PCID、设备/共享映射策略、并发撤图、完整故障恢复 copy、信号掩码与 atfork 多线程语义、
fd close 隔离/子 VFS 授权/完整 execve、子 COW 数据帧独立生命周期、完整寄存器继承证明及尚未接线的fork子FPU状态继承仍在范围外。
不得以本片关闭阶段5签署、BLK-003 或解释既有 QEMU 启动超时。
修改页表层级/回收/CPU模型/等待输出/退出与能力继承时须重审。

本片明确修订 [M17b-2a](../reports/2026-10-23-stage5-m17b2-cow-primitives.md) 的用户大页原样共享行为：
当前用户映射只支持4KiB，遇到用户大页/global叶按错误返回，不能继续共享可写页。
这不是大页COW支持；supervisor大页共享保持原机制。

## 输出准备的后继收紧（2026-10-30）

[S6-M1 内存事务](../reports/2026-10-30-stage6-user-copy-transactions.md)将上文第 5 项允许的
“第二页缺帧时第一页可保留私有化”收紧：现有四字节 wait 输出和至多 4096 字节的通用输出
共用全范围检查/全量 COW 帧预留，准备失败归还预留并保持所有 PTE、用户字节与帧归属。
这是内存准备操作的更强保证，不承诺整个 wait/fork/IPC 控制事务的 PTE 回滚，
不改变等待中多线程撤图的原有并发边界；历史报告仍按原版本解释。
