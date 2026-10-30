---
id: report-stage6-private-completion-waits
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M1 内核私有等待：登记、信号隔离与代次完成

## 结论与范围

在 `01d44a6` 上落实 [M1 协议 §1–2](../specs/2026-10-29-stage6-m1-protocol.md)的
私有等待原语：使用真实 `BootstrapState`、`WaitObject`、`WaitRequest` 和 Task 状态表，
预留等待对象、不可警报阻塞、按代次完成、销毁清理并记录审计。原生信号入口也识别
不可警报等待，避免默认终止分支在排队前绕过保护。

这是候选 A 的**内核等待基础**，尚无 Gate attach/trap/reply 调用这些原语，也没有
ring3 私有等待见证。本次不新增 Gate/Request 能力、syscall 或机器 ABI，不把 host
状态机测试当成 G1/G2；S6-D2 和阶段退出条件仍未完成。

## 基线与契约链

起始工作区干净；基线 `01d44a6`。输入为
[阶段计划](../plans/2026-10-27-stage6-7-refinement.md)、
[递送设计](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[R1–R6 发现](2026-10-29-stage6-m1-decision-review.md)、
[M1a 整改](2026-10-29-stage6-m1-blocker-closure.md)及
[内存事务前置](2026-10-30-stage6-user-copy-transactions.md)。
既有公开等待与信号边界见 [M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)、
[阻塞等待报告](2026-10-23-stage5-m15a2-waitobject-blocking.md)和
[信号报告](2026-10-23-stage5-m18b-sigaction.md)；抽象字段与一致性仍以
[COSIR §5.2/§21/W9/W17](../../../COSIR%20原语规范.md)为准。

使用 pinned `nightly-2026-07-30`、已有离线依赖；完整 BIOS 回归沿用单核 QEMU TCG 配置。
完整命令获准在沙箱外运行，既有 Linux N1/R3 参考需要主机套接字；无依赖更新、超时放宽、
参考跳过、重试通过或 KVM/实机主张。

## 实现与检查边界

- `kernel/src/completion.rs` 是 bootstrap 子模块，直接操作真实对象表。预留不铸造任何用户
  token；opaque `CompletionWait` 包含完整 Task cookie。对象 id 单调、不复用；每次阻塞
  generation 检查递增，耗尽拒绝。`CompletionTicket` 绑定该次登记，迟到或重复完成均 stale。
- 阻塞必须从 Running 且没有其它未决返回/等待进入。真实请求为单项 Any、
  `alertable=false`、无 deadline；没有原生 blocked-call 号、通用 WaitResult 或帧写回。
  受控完成清理双向登记并置 Ready，不经过 `inject_wait_wakes`。
- 生命周期与状态迁移审计一起预检：阻塞/完成各需两条日志。不足时对象、Task、登记和
  generation 保持原值；登记不能为了报告 AuditFull 被提前清掉。
- 普通 signal/destroy/observe/wait 路径拒绝私有对象；两个根铸造入口拒绝导出私有对象，
  公开派生入口也加防御检查。原生 generic wake、两种时钟超时、upcall 中断不完成私有等待；
  原生物理期限转换不能给它添加超时。
- `SYS_UPCALL_CONTROL` 的默认终止捷径增加不可警报检查；此时通知只进入 pending。
  公开等待仍可警报，原生 wait4/recv/信号挂起路径保持其现有细化范围。
- 公共对象预算仍为 32，另有 80 个私有预留位置（协议的 64 客户端 + 16 服务上限）。
  arena 合计 112；两个预算互不借用。没有提高 Task、能力、审计、内核栈容量或结构体守卫。
  80 是等待存储预算，不声称现有 18 槽 Task 表能同时运行 64 客户端。
- 公共 Task 销毁体在 Task cookie 退役前清除其全部私有预留；审计总预检计入每个对象的
  清理事件。不恢复被销毁任务，不生成等待结果；等待完成本身不消费 pending upcall。故障本身不释放预留，服务死亡/
  Gate 孤儿化策略仍由后继 Gate 层承担。

`completion_waits_well_formed` 检查私有登记双向一致、所有者 cookie、无原生 pending 结果、
BlockedWait 及精确请求形态。这是有界 host 检查器，未证明所有可达状态或整个内核 W9/W17；
既有 wait4/信号挂起可无 WaitRequest 的细化分支明确不在其投影内。

## 测试、反例与结果

7 组 host 测试覆盖：真实表中的预留/完成及不铸 token、默认/自定义通知保留 pending、
普通三种 wake 和双时钟不能越权、重复/迟到完成、公开能力误装的防御、全部审计失败边界、
generation 耗尽、等待者销毁/Task 槽复用、双向登记损坏及 32/80 独立容量。
快照读取实际 arena 内容，避免仅比较 arena 句柄而漏掉对象变更。归档命令日志仅去掉末尾多余空行。

首次销毁/复用测试出现 `DuplicateTask`：测试在保留 Dead 记录时直接创建同编号任务；
按真实分配路径补上先移除 Dead 记录、再用单调 Task generation 创建的步骤后通过。
未放宽生产任务创建检查，也未把该 fixture 错误描述为 OS 已有缺陷。

| 命令 | 本次结果与证据范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib bootstrap::completion` | PASS：7 组；[日志](artifacts/stage6-completion/focused.txt)，真实内核状态表的 host 测试 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1046 passed，0 failed，1 个既有外部 ELF 样本 ignored；[日志](artifacts/stage6-completion/workspace.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机编译、kernel/src 33 文件零 unsafe；[日志](artifacts/stage6-completion/lint.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：单次命令 exit=0，36 程序/72 轮，全部主机参考通过；[日志](artifacts/stage6-completion/full-suite.txt)。既有回归，非 Gate G1–G3 |
| 文档四项及 `git diff --check` | PASS：`self-test`、`build`、`check`、`check --base 01d44a6`，333 文档；无空白错误 |

## 后继、未覆盖项与阶段影响

此层是可信内核接口，不能单独提供 Gate/Request 授权，也不更新保存帧。Gate reply 须先预检
全部审计与身份，再在同一不可调度事务内消费 ticket、终结请求并只改 rax；不能把本层完成
后 Ready 当作提前调度许可。attach 同样须预检 Gate、请求槽、全部等待预留、能力和审计，
不能把多个独立 reserve 的成功拼成已原子化的 attach。

审计错误目前是内核 `Error::AuditFull`：后继 Gate 全局 fail-stop 控制器必须先停止门控调度，
不得把它变成客户端返回值或回退原生分派。尚无活跃 Gate，故本次没有启用部分 fail-stop 协议。
固定服务者/最后服务授权丢失、请求能力当前位置/代次、服务空队列重试、trap 入口、rax-only
帧提交、代理复制授权及 G1–G3 仍待完成。通知在完成后如何递送给门控任务也需 Gate 适配层
排除原生号重启分支；本次只保证私有阻塞期间不被中断。

本报告在私有等待主题上补强 M1a host 设计，不覆盖前轮内存证据、不修订冻结抽象对象表、
不运行 Lean 或复制性能计量。阶段 6 仍进行中，当前结论只在
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)维护。
