---
id: spec-windows-blocker-decisions
kind: specification
authority: windows-blocker-decisions
status: frozen-with-partial-coverage
lifecycle: current
updated: 2026-11-27
---

# Windows 计划审计阻断处置：M0 通道、D3b 用户 GS、D4 共享 root

## 1. 身份与效力

作者在 2026-11-27 要求“解决阻断”，针对
[Windows 计划审计](../reports/2026-11-27-windows-plan-audit.md)的 B1–B3。本文冻结三项
处置：M0 前原型通道（B1）、WIN-D3b 用户 GS 的联合决定（B2）、D4 共享 root 多线程的
v1 契约（B3）。冻结的是**方向、门禁与不变量**；实现、参考 Windows 与逐 API 语义仍为
未完成，不构成 M0–M10 任何签署。

覆盖边界：B1 的参考 Windows/程序白名单/逐 API 行无法在本仓库内提供，本文只把漂移
改为受控门；B2 决定目标与分期，不实现 `swapgs`；B3 冻结模型与反例，不实现线程。

## 2. B1：M0 前原型通道与 M4 门

### 2.1 决定

- **M0 保持未通过**，不因本文或既有 M1–M3 证据改变；`status` 继续为
  `bounded-baseline-m0-open`。
- 在 M0 签署前，只允许**原型通道**工作；M4（文件/句柄）及之后的语义实现被硬门禁止。
- 原型报告必须标注 `prototype` 与“无 Windows 参考差分”，不得使用“兼容”“原样支持”
  等验收措辞。

### 2.2 原型通道允许/禁止

| 允许 | 禁止 |
|---|---|
| 宿主 PE/ABI 解析、映射计划、重定位/导入/TLS 目录模型 | 冻结未由参考 Windows 证实的错误值、路径/大小写、共享打开、TEB/PEB 字段布局 |
| 自建 PE 夹具与 ring3 机制原型（装载、停车/发布、发布失败清理） | 以自建探针或宿主测试宣称真实 Windows 程序兼容 |
| 通用机制的独立设计与反例（GS 上下文、共享 root、等待/A） | 开始 M4+ 的用户可见语义实现或 profile/白名单承诺 |
| 工具链、夹具生成与哈希记录的改进 | 用 Wine 代替参考 Windows 签署差分 |

### 2.3 M0 冻结清单与签署

本地可先冻结：目标 PE32+ AMD64 控制台、无 GUI；仓库 nightly；PE 夹具生成工具链与
哈希记录策略；不得提交 Windows SDK/参考镜像。

仍须外部提供：参考 Windows build/修订、语言/区域/时区、文件系统与权限、CPU、服务
配置；首批独立 CLI 与服务样例及其完整导入闭包、SHA-256；逐 API 期望与限制行；同一
EXE/DLL 字节在参考环境与 Prima 的运行结果。

M0 签署 = 上述外部项齐备 + 独立评审；签署前不得声明 M0 通过，不得启动 M4+。

### 2.4 残差

B1 的治理漂移已关闭（门禁明确）；**B1 的参考环境部分仍是外部依赖**，在取得参考
Windows 前无法关闭。任何“已解决兼容性”的主张仍不成立。

## 3. B2：WIN-D3b 用户 GS 的联合决定

### 3.1 决定

WIN-D3 拆为三部分：

| 子项 | 现状 | 处置 |
|---|---|---|
| D3a 停车/发布 | `task_create_held` 与发布失败清理原型已有 | 保留；按节权限/发布前 W^X 仍为 M2/M3 退出缺口 |
| D3b 用户 GS | 无任何证据；内核只有 FS TLS，无 `swapgs` | **采用内核 GS + `swapgs` 作为 Windows x64 目标**，按 §3.3 分期 |
| D3c 映射权限 | 原型整映像 R+X | M2/M3 退出前落实按节权限 |

选择理由：Windows x64 的 TEB/TLS 访问依赖用户 GS，无法用 FS 重映射或用户态模拟满足；
推迟即永久停留宿主模型。代价是陷入路径、每 CPU GS 基址与安全评审，必须与 SMP 联合。

### 3.2 与 P5 的解耦

- P5-3（每 CPU GDT/TSS/IDT、idle/调度边界）**不等待 GS**；CPU 索引在 GS 安全落地前
  继续用 CPUID/APIC ID 映射。
- GS 不是 P5-3 的前置；GS-3 之后才允许把热路径 CPU 索引迁到每 CPU GS 区。
- 任何一条轨道不得在对方未评审时单方面引入 `swapgs` 或用户 GS 语义。

### 3.3 分期与验收

| 片 | 内容 | 验收（机器证据） |
|---|---|---|
| GS-0 | 本文决定与安全不变量 | 文档冻结；无代码 |
| GS-1 | `IA32_GS_BASE`/`IA32_KERNEL_GS_BASE` 原语、per-task 用户 GS 保存/恢复、`syscall` 入口/出口交换 | ring3 用户 GS 往返；两任务 GS 不串扰；错误 GS 值被拒绝 |
| GS-2 | 异常/中断/IRQ 入口出口交换；嵌套与 NMI/IST 策略 | 嵌套 #PF/IRQ 下 GS 仍指向内核区；NMI 不观察半交换状态；负例矩阵 |
| GS-3 | 每 CPU GS 区替换 CPUID 热路径索引 | AP 并发下 `current_index` 正确；与 P5-4 IPI/TLB 协同 |
| GS-4 | Windows 运行期 TEB/PEB/TLS 接入 | TEB 指针、静态/动态 TLS、跨线程隔离；M3 运行期退出 |
| GS-5 | APC/SEH 的 GS 上下文 | 与 D4/异常上下文联合验收 |

### 3.4 安全不变量

1. `swapgs` 只在受控的内核入口/出口执行；用户不能影响内核 GS 的交换时机。
2. 用户 GS 基址在保存/恢复时校验 canonical 与用户范围；内核绝不把用户提供值当内核指针。
3. 入口先交换、再触碰每 CPU 状态；出口最后交换；异常路径与正常路径共用同一不变量。
4. NMI/IST 必须使用不依赖半交换状态的栈与指针；否则记反例并停止 GS-2。
5. 任务切换保存/恢复用户 GS；内核 GS 基址属于 CPU，不属于任务。
6. 所有汇编留在 `prima-ostd`/入口层，`kernel/src` 保持零 `unsafe`。

### 3.5 残差

“GS 方向未决”的阻断已关闭；**运行期 GS/TEB/TLS 未实现**。M3 运行期退出、M5/M6 的
APC/SEH 仍被 GS-1…GS-5 与 D4 阻塞，任何当前报告不得宣称 Win32 TLS 兼容。

## 4. B3：D4 共享 root 多线程 v1 契约

现有内核已有共享 root 的脚手架：`gate_root_access_permitted` 对 `gate.shared_root`
按同进程/绑定 cookie/root 代次逐事务扫描，`gate_spawn_thread` 只允许受管生成。D4 v1
在此基础上冻结如下模型；未实现。

### 4.1 对象模型

- **Process**：身份 + 共享 root + 进程级句柄表 + 地址空间 epoch。
- **Thread**：一个内核 Task，独立保存帧、内核栈、`TSS.rsp0` 槽、FS/GS、阻塞调用、
  wait/APC 队列；共享 Process 的 root 与句柄表。
- root 由 Process 持有，Thread 引用；进程退出等待线程退休后回收 root。
- 每 CPU TSS/内核栈来自 P5-3；线程切换必须同时切换 `rsp0`、FS、GS、FPU 与信号/APC 状态。

### 4.2 Gate 与授权

- 共享 root 客户端仅经受管 `gate_spawn_thread` 绑定准入，绑定记录
  `(process, thread, gate, cookie, root, root_generation)`。
- 逐事务重扫同 root 活跃线程，不缓存 peer；root 代次不符、跨进程或绑定缺失一律拒绝。
- Windows 句柄数值不直接作为 Capability；句柄经进程级表映射到域内对象/访问权/会话。
- 授权不得因线程加入而扩大；进程退出/线程销毁按在途引用结算。

### 4.3 参数快照与失败原子性

- 每个 API 请求在服务侧把完整用户参数图（含数组/字符串/结构版本）复制到稳定快照，
  校验长度/计数/对齐后才分派；分派后不再解引用用户指针。
- 复制或校验失败不产生部分副作用；多指针事务不得逐段提交。
- 句柄/根代次在快照时固定；在途请求遇到 close/exit 按固定完成点返回确定的失败。

### 4.4 等待、APC 与销毁

- 等待登记与唤醒按 Thread 归属；WaitAll 的检查与消费在同一逻辑提交中完成，禁止逐个
  占有后阻塞；不得把 POSIX signal 当作 APC。
- APC 队列按目标 Thread；目标退休时按确定规则丢弃/完成，投递与线程退出竞争必须可审计。
- 线程终止、进程退出、close 与在途 I/O 的完成点分别定义；旧句柄不得因代次复用复活。

### 4.5 未决项（M5 前必须冻结）

线程终止 vs 进程退出顺序；句柄继承标志与伪句柄；APC 投递时机；Gate 绑定身份用
Thread 还是 Process（v1 契约暂定 Thread + Process epoch）；共享 root 下映射变更的
唯一所有者与 TLB 刷新；COW 与多线程写故障的串行化。

### 4.6 验收反例（未来实现）

跨进程线程注入、root 代次过期、wait 中线程退出、I/O 中 close、WaitAll 部分消费、
APC 投递到已退休线程、共享 root 下并发映射/COW、进程退出时在途请求结算。

### 4.7 残差

“D4 无冻结设计”的阻断已关闭为 v1 契约；**多线程/APC 实现未开始**，M5/M6 仍不得
宣称通过。

## 5. 对计划与跟踪的影响

- [Windows 计划书](../plans/2026-10-31-windows-libos-roadmap.md) 的 M4 增加 M0 硬门，
  M3 运行期退出增加 GS-1…GS-4 前置，M5 增加 D4 冻结前置；里程碑表其余内容不改写。
- [M0 规格](2026-11-26-windows-m0-baseline.md) 与 [S7-B.1 设计](2026-11-27-stage7-smp-state-partition.md)
  增加本文链接；P5-3 不等待 GS。
- 不改变阶段 7、7-A/7-B、profile 或任何勾选；不新增 OS 运行证据。

## 6. 后继

- GS 实现切片（GS-1 起）与 D4 实现切片各自独立评审、独立报告。
- M0 外部输入齐备后按原退出条件签署；在此之前保持 prototype 通道与 M4 门。
