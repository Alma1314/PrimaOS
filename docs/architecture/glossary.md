---
id: guide-glossary
kind: reference
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-23
---

# 术语与检索别名

本页用于检索。精确定义以[原语规范](../../COSIR%20原语规范.md)及[白皮书](../../Prima%20OS.md)为准。

| 术语 / 别名 | 含义与边界 | 首查 |
|---|---|---|
| PrimaOS / Prima OS | 元操作系统研究项目及仓库整体 | 白皮书摘要 |
| HexArc | 最小对象能力机制内核的架构名称；主要实现位于 kernel | 白皮书§4 |
| POSIR | 平台无关对象能力抽象状态机 | 原语规范§1 |
| COSIR | POSIR 在特定硬件上的实例化/细化；规范文件保留历史名称 | 原语规范§18/§20 |
| personality / persona / LibOS | 用户态构造的 OS 语义与 ABI 兼容层；源码兼容不等于二进制兼容 | 白皮书§8 |
| Capability / cap / 能力 | 显式授权 token；对象、rights、活动状态、派生与位置有独立约束 | 原语规范§4 |
| Authority / 初始权力 | 自举建立的初始授权来源，不等于任意后续拥有的 cap | 原语规范§14 |
| CDT / 派生树 | Capability Derivation Tree；父关系与无环性质 | 原语规范§4.2；lean/README |
| Handle / 句柄 / stale | 具体实现的 opaque 编码；generation 防止槽复用重新授权 | kernel/handle.rs；阶段2复核 |
| Segment / MemorySegment / memseg | 帧集合及受授权的映射对象；不是 POSIX 文件 | 原语规范§5.1/§7 |
| vspace / root / CR3 / PTE | 虚拟映射及 x86_64 页表细化；抽象页号与字节 VA 不混用 | 原语规范§5.2/§20 |
| COW / 写时复制 | 写保护、共享帧、故障复制的组合；模型与真实路径分别取证 | 内存主题、进程主题 |
| Task / 任务 | 原语规范中的执行对象；不等同于 POSIX process | 原语规范§5.2/§8 |
| Process / Thread / ShareGroup | 阶段5设计与实现的进程/线程增量；工作决定不自动成为冻结 POSIR 语义 | M15/M17 设计 |
| space_of / holder / 能力空间 | 实现中的授权位置解释；线程和 fork 共享空间须保留语义偏差 | 进程主题 |
| IPC / Send / Recv / 邮箱 | 授权消息、FIFO、阻塞与 Capability 移动 | 原语规范§9 |
| Namespace / 命名空间 | 内核能力绑定/挂载/解析；不是直接的 POSIX 文件树 | 原语规范§11 |
| VFS / rootfs / fd / OFD | 用户态文件树、文件描述符及共享打开状态；与 Namespace 分层 | VFS 主题 |
| ExceptionSource / upcall / alert | 原语异常源与通知；不能自动等同于 POSIX signal | 原语规范§12/§21 |
| WaitObject / generation / wait_objects | 代际观察与等待唤醒机制；futex 是其上的适配子集 | 原语规范§21 |
| Timer / PIT / time_now / ticks | 抽象 Timer 时间推进与 POSIX PIT 轮询时钟有不同路径 | 同步主题；prima-abi |
| Audit / 审计 / trace / witness | Audit 是语义事件；trace/witness 是观测与测试机制，两者不能互换 | 原语规范§13；证据规则 |
| framekernel / prima-ostd / TCB | 特权硬件框架与 safe 机制层分离；TCB 指标按 xtask 的限定口径 | 系统地图；开发手册 |
| profile / supported / partial / unsupported | 明确冻结的兼容范围与逐项状态；不是完整 POSIX/Linux 声明 | compat 台账 |
| SPEC / PLAN / REPORT / DOC | 历史编号与工件引用；编号提及不保证唯一身份或批准状态 | 机器目录 mentioned_ids |
| Sx-Dy / Sx-Xy | 阶段 x 的交付项 / 退出条件 | 阶段跟踪 |
| BLK / ISSUE | 阻塞或问题标识；Open/关闭需来源和限定范围 | 阶段跟踪活跃问题 |
| M0–M21 / W1–W9 | 阶段5里程碑与验收探针编号；须带阶段/文档上下文 | 兼容主题 |

## 容易混用的编号

`W1` 在原语规范§16 表示 ID 新鲜不变量，在阶段5 QEMU 中表示 WaitObject 探针；`M7` 在不同候选计划中也可能指不同工作。检索时组合主题词、阶段与路径；引用时写“COSIR §16 W1”或“阶段5 W1 探针”。

ABI 1.0 是原生机器握手版本，`prima-runtime` 的源码 API 版本单独维护，boot manifest 和 boot info 又有各自版本。参见[接口边界](interfaces.md)。
