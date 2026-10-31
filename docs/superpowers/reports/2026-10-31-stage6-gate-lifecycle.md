---
id: report-stage6-gate-lifecycle
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M1/M1b 内核门控对象与请求生命周期

## 结论与范围

在 `477a768`（工作区干净）上落实[S6-M1 协议 §1–5](../specs/2026-10-29-stage6-m1-protocol.md)的
内核对象层：Gate 固定服务者、绑定与请求槽预留、有界 FIFO 队列与容量等待、请求出队与
请求能力安装、一次性答复、客户端销毁取消、服务者死亡孤儿化，以及这些事务的审计预检与
`AuditFull` 全局停止。新增 `kernel/src/gate.rs`（bootstrap 子模块）、
`Right::GateAttach`/`GateServe`（按 D17 追加到位 31/32）、`ObjectKind::Gate`/`GateRequest`
与 `ObjectRef::Gate`/`GateRequest`，并把门控清理预算接入 `destroy_audit_events`。

**未完成**：73–78 未在 `prima-abi` 分配；trap 入口未接管；`gate_copy_in/out`（M1c）未实现；
没有 ring3 QEMU `G1`/`G2`/`G3`，没有可信停车入口或 Linux LibOS。因此 S6-D2 与阶段 6
退出条件仍全部不勾选；本报告只把 host 状态机与事务边界记为证据，不把模型测试当成
候选 A 的裸机验收。

## 基线与契约链

起始工作区干净；基线 `477a768`（主分支整合后的当前 HEAD）。输入依次为
[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)、
[D11–D16 前置裁决](../specs/2026-10-28-stage6-7-release-decisions.md)、
[D17–D22 机制裁决](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[递送提案](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[R1–R6 复审](2026-10-29-stage6-m1-decision-review.md)、
[协议收口](../specs/2026-10-29-stage6-m1-protocol.md)与
[冻结台账](2026-10-30-stage6-freeze-ledgers.md)。内核前置原语见
[内存事务](2026-10-30-stage6-user-copy-transactions.md)、
[私有等待](2026-10-30-stage6-private-completion-waits.md)与
[审计停止](2026-10-30-stage6-audit-fail-stop.md)；`S6AUDIT` 的裸机边界一致性由
[审计停止 QEMU](2026-10-30-stage6-audit-stop-qemu.md)承担，本切片没有改动它。

使用 pinned `nightly-2026-07-30`、离线依赖与既有单核配置；没有超时放宽、参考跳过或重试通过。

## 实现与检查边界

- **对象表不入结构体**：Gate/Binding/Request 三张表存放在静态 arena，实例身份复用审计区段号
  （与 `gate_audit` 的首次诊断单元同一模式）。`BootstrapState` 不新增字段，13 KiB 结构体守卫
  保持不变并通过；容量为 Gate≤16、绑定/请求槽≤64、每 Gate 可递送队列≤8。
- **attach 是预留事务**：调用者必须是 `Running`、持有目标 Task 的 `TaskManageVSpace`、自身未被
  门控；目标必须是 `Ready`、无绑定/无服务身份、独占地址空间（单线程 Process）。新门控一次
  预留服务等待与客户端等待各一个私有 `WaitObject`，并铸出 `{Derive, GateAttach, GateServe}`
  根能力；已有门控只预留客户端等待。审计按 5/2 条预检，能力、位置、历史、等待配额与槽位
  全部先于第一次变更检查；失败不铸 token、不留半条绑定。
- **trap 保持零 Linux 语义**：绑定任务命中时，原始 `number`、6 个 `u64` 参数与来源 flags 原样
  记录（flags bit0 = `int 0x80`，其它位不解释）；队列未满进入 `Queued`，满则进入
  `WaitingCapacity`，两者都占用 attach 已预留的槽。客户端登记到自己的私有等待（不可警报、
  无期限）；若服务者正阻塞在该门控的空队列等待上，则只唤醒这一项服务等待并清空其 ticket。
  服务授权已消失时直接返回 attach 配置值，**不查**原生分派表。
- **wait 出队即提交**：服务者必须持有 `GateServe` 且是固定服务者与代次。出队前验证页表
  输出范围由适配层负责（本层给出 12 字布局与 `request_cap`）；本层预检能力容量与审计后
  一次提交：安装请求根能力 `{Derive, GateServe}`、把请求置 `Delivered`、按 FIFO 晋级一个
  容量等待者并记录事件。空队列则登记到该门控的私有服务等待，返回 `Blocked`；孤儿化且
  队列为空返回 `Stale`，输出保持不变。
- **答复至多一次**：`reply`/`abort` 先拒绝非活动句柄与已终结请求，再校验固定服务者身份；
  提交在同一个不可调度区间内完成——完成客户端私有等待（`BlockedWait → Ready`）、**只写**
  客户端保存帧的 `rax`、记录 `GateTerminated`、撤销请求根及其派生子树（每个别名的
  `CapabilityRevoked`）。请求 id 单调且不复用；终结后旧句柄与所有别名确定性失败。
- **销毁清理有预算**：`destroy_audit_events` 增加 `gate_cleanup_audit_events`。客户端销毁
  取消请求且**不恢复帧**（不写答复值），释放空闲预留、解绑并按 FIFO 晋级；服务者销毁把
  门控置为孤儿、以每个绑定自己的配置值终结全部未决请求（写 `rax` 后完成等待），撤销请求
  能力。绑定保留为 fail-closed：孤儿化后再次 trap 得到配置值。
- **失败关闭**：所有门控事务先走 `preflight_gate_audit`；不足即记录首次诊断并返回
  `AuditFull`，既有 `enforce_stop` 在分派/调度/IRQ 边界终止系统，不把内部错误返回给客户端。
  普通准入失败返回 `Denied/Capacity/Busy/Stale` 且状态不变。
- **权限与类型**：只追加 `GateAttach`(31)/`GateServe`(32)，D17 的 31 项冻结逐项测试扩展到 33 项；
  `Gate` 允许 `{GateAttach, GateServe}`，`GateRequest` 只允许 `{GateServe}`；`target_live` 对
  请求额外要求所属门控仍活动。
- **明确不做**：`gate_copy_in/out`（M1c）与跨任务映射；真实 trap/`rax` 适配；`SYS_GATE_*`
  号段；QEMU `G1–G3`；共享 root、多线程客户端、可警报等待与 `EINTR`（M4）；超时/租约。
  host 测试不能替代页表/硬件页权限事务，也不能证明所有可达状态满足 W17。

## 测试与反例

10 组 host 测试操作真实 `BootstrapState`、能力表、`WaitObject` 表与调度保存帧：

| 用例 | 断言 |
|---|---|
| attach 预留 | 5 条审计、1 个新能力、服务/客户端各 1 个私有等待、请求槽 `Idle`、根权利、`gate_well_formed` |
| 非 Ready 目标 | 返回 `Busy`，审计/槽/表/等待对象逐项不变，不留下 Gate |
| trap→wait→reply | 原始 number/args/flags 与 12 字输出；只改 `rax`；重复答复 stale；下一次 trap 用新请求 id |
| 空队列服务等待 | `Blocked`→入队只唤醒服务等待一次→再次 wait 交付；双向登记一致 |
| 服务者死亡 | 门控孤儿化、Delivered/Queued 两个客户端都以各自配置值返回、迟到答复 stale、孤儿 trap 仍返回配置值 |
| 客户端销毁 | 不恢复帧（保留 sentinel）、请求能力失效、绑定计数归零 |
| 队列容量 | 第 9 个 trap 进入 `WaitingCapacity`，出队后按 FIFO 晋级到 `Queued` |
| 别名与身份 | 移动后的别名不能冒充服务者；答复后根与别名一起失效 |
| 审计耗尽 | `AuditFull` 且诊断已记录；审计计数、能力槽与门控表完全不变、等待对象为零 |
| 投影损坏 | 篡改绑定代次令 `gate_well_formed` 失败 |

开发过程中被测试抓到的两处真实缺陷：孤儿化路径最初只完成客户端等待而没有写配置值，
`service_death` 用例读到旧 `rax` 后暴露，已改为在同一提交内先写 `rax` 再完成等待；
`full_queue` 用例最初用平台 `create_task`（8 个预留结构根槽）创建 9 个客户端而失败，
按真实分配路径改用 `create_task_record`。两次都只修实现/夹具，没有放宽容量、审计或
身份检查。另有一处测试夹具把句柄编码写成 `index<<32|generation`，按
`Handle::encode`（`generation<<32|index`）修正。

## 全量 BIOS 套件的环境性超时（本轮未通过）

本轮完整 `cargo xtask test` 返回 **13 个失败组、18 次运行超时**，全部是
`QEMU timed out after 30s`；失败日志中没有断言不匹配、panic 或协议错误。同一时间宿主 8 核负载
3.8–6.3、沙箱外不可见进程约占 44% CPU，交换区已用约 6 GB。抽查结果：

- `W7`（exec）在该镜像上手工运行 3 次为 28.3/28.7/29.0 秒，串口恰好包含
  `EXEC|exec=1;wait=1;bad=1;stream=1;large=1|ok=1;err=0` 且以 33 退出；harness 复核仍超时，
  说明它就在 30 秒门限边缘。
- 在临时 stash 的工作区把基线 `477a768` 重新构建后，`W7` harness 两次通过，但其手工运行耗时
  **30.8 秒**——基线同样贴着门限，因此本组超时不构成"门控切片引入变慢"的证据。
- `V1`（VFS）harness 通过（手工 19.8 秒），同一镜像族在低负载时留有明显余量。

**没有放宽 30 秒门限、没有重试到通过、也没有把该套件记为 PASS**；其余 11 组只记录为超时，
未逐组重新取证。套件日志只保留 PASS/ERROR 汇总行，失败运行的完整串口另存
[失败日志](artifacts/stage6-gate/full-suite-errors.txt)。上述三次手工运行、基线 30.8 秒与
V1 19.8 秒的原始计时输出合并保留在[计时日志](artifacts/stage6-gate/W7-timing.txt)。前序[主分支整合](2026-10-31-stage6-main-merge.md)的整套 PASS 仍只属于其基线，
本报告不把它外推到本次改动。

## 命令与结果

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib gate::tests` | PASS：10 组；[日志](artifacts/stage6-gate/focused.txt)，host 状态机，无 QEMU |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1076 passed、0 failed、1 个既有外部 ELF 样本 ignored；[日志](artifacts/stage6-gate/workspace.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机 `deny(unsafe_code)` 编译、kernel/src 37 文件零 `unsafe`；[日志](artifacts/stage6-gate/lint.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask test` | **未通过（环境性）**：13 组/18 次运行 30 秒超时，无断言失败或 panic；[失败日志](artifacts/stage6-gate/full-suite-errors.txt)、[W7 harness 复核](artifacts/stage6-gate/W7-rerun.txt)、[W7 手工串口](artifacts/stage6-gate/W7-manual-serial.txt)、[V1 harness](artifacts/stage6-gate/V1-harness.txt)、[基线 harness 对照](artifacts/stage6-gate/baseline-W7-harness.txt)；门限未放宽、未重试到通过 |
| 文档四项与 `git diff --check` | PASS：`self-test`、`build`、`check`、`check --base 477a768`，347 文档/3393 章节；`git diff --check` 无空白错误 |

## 阶段影响与后续

本切片关闭的是「Gate/Request 对象与生命周期尚未存在」这一半，不是交付机制。后继必须：

1. 在 `serve` 入口接入 `gate_trap`（冻结返回帧、`BlockedWait`、调度切走），并把
   `gate_reply` 返回的提交与真实 `int 0x80` 帧对齐；
2. 分配并登记 73/74/77/78（`attach`/`wait`/`reply`/`abort`），`wait` 输出用
   `prepare_task_output` 的 96 字节全文预检；`copy_in/out`（75/76）留 M1c；
3. 用 ring3 客户端/服务者取得 `G1`（截获并改写 `rax`）、`G2`（服务死亡/重复答复/请求者销毁）
   与 `G3`（越界/跨页/未映射/只读目标）；
4. 可信停车入口与 Linux LibOS 仍属 S6-M2/S6-M3；`Gate` 能力丢失钩子（drop/move/revoke
   时的最后服务授权检测）仍待接线，当前只在 Task 销毁路径清理。

阶段 6 仍进行中；S6-D1–D5 交付检查与 S6-X1–X4 退出条件全部未勾选。全量 BIOS 套件本轮
未取得通过，阶段跟踪只记录 host 证据与本轮超时事实；当前状态只见
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
