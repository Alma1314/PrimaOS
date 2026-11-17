---
id: report-p2b-closeout
kind: report
authority: scoped-implementation-and-validation
status: evidence-complete-review-pending
lifecycle: current
updated: 2026-11-18
---

# P2b-6：P2b 收口

## 结论与范围

本报告汇总 P2b-0 至 P2b-5 的交付与证据，并把 P2b 作为**证据包**收口：Linux
personality 的数据面已经全部经管理器签发的 Store 会话；两套真实 ABI 程序在同一
镜像中运行；代理 fork/exec、显式 `/shared` 授权、私有根隔离、根逃逸与伪造/重放
负例均有机器证据；既有 36 组 72 轮 BIOS 套件与 11 项 Linux 差分保持通过。

**本报告不签署 P2，不关闭 7-A，也不构成组织级复核。** 组织级独立审查仍是
P2b-6 的未满足验收项；`Direct` 传输的去留已在本片给出留用裁决。P3（异步批量与
背压）、P4（Gate/IPC 收窄）、P5（多核）、P6（撤销/恢复与总验收）未开始。

## 基线与输入

- 基线：P2b-0 起于 `5a163e9`；本片在 `f6341f8`（P2b-5）之后执行。
- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)；
  上位计划：[LibOS 数据通路完整改造计划](../plans/2026-11-10-libos-data-path.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 的 P2b-0 至
  P2b-5 节。
- QEMU 输入：pinned static busybox v1.36.1，sha256
  `dbac288c…4e62d6ac14`。

## 分片证据汇总

| 切片 | 交付 | 关键证据 | 报告 |
|---|---|---|---|
| P2b-0 | op 矩阵、fd/游标/errno、根策略与 fixture 冻结 | 契约文本 + `bulk` host 编解码测试 | [store 批量](2026-11-17-p2b-store-batch-ops.md) |
| P2b-1 | Store 侧 OPEN/STAT/CLOSE/GETDENTS/LSEEK/STAT_PATH 与代理 EMFILE | host 负例、owner 隔离、重放/跳号、游标回退 | 同上 |
| P2b-2 | Linux 前端改走 `Session`，不再持有 `Ram`/`OpenFiles` | workspace 1166、既有 Linux 差分 22 轮 | [Linux 前端](2026-11-17-p2b-linux-frontend.md) |
| P2b-3 | fork 预留/发布/回滚、exec CLOEXEC、exit owner 关闭 | `PROXY_RESERVE/PUBLISH/RELEASE`、host 生命周期测试 | [fork/exit](2026-11-17-p2b-fork-exit.md) |
| P2b-4 | 场景镜像、管理器签发会话、`Remote`/原生客户端 | `test-linux-proxy` 首次两 ABI 通过 | [镜像接线](2026-11-18-p2b-proxy-wiring.md) |
| P2b-5 | 正反例矩阵、原生策略视图、fork 限额 | 两次 QEMU 启动；workspace 1170 | [代理矩阵](2026-11-18-p2b-proxy-matrix.md) |
| P2b-6 | 本汇总、`Direct` 裁决、完整 BIOS 回归 | 本报告 | — |

## 本片命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `CARGO_NET_OFFLINE=true cargo xtask test` | 本次 | PASS；`full BIOS suite: 36 programs, 72 runs`，日志无 FAIL/ERROR/panic | QEMU（既有套件） |
| `cargo test --workspace` | 本次（P2b-5 末次） | PASS；1170 passed、0 failed、1 ignored | host |
| `cargo xtask lint` | 本次（P2b-5 末次） | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `test-linux-proxy`（linux-probe） | 本次（P2b-5 末次） | PASS；两次启动，`S7ABI\|…;status=0`，QEMU 退出码 33 | QEMU（新矩阵） |
| `test-linux`（linux-probe） | 本次（P2b-5 末次） | PASS；两轮各 11 项，含 `M4-fork` | QEMU（既有差分） |
| `cargo xtask build` | 本次（P2b-5 末次） | PASS（kernel/bios/uefi） | host/镜像 |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS；407 documents、3870 sections | 文档 |
| `python3 scripts/docs.py check --base bd5a2ec` | 本次（P2b-5） | PASS | 文档 |

## 跨切片发现

- **能力绑定取代内核父系**：受管 gate fork 的 Task 在内核中无 `Process.parent`，
  原“父 Process”校验在真实路径恒假。`PROXY_PUBLISH` 改由子 Task 的 `TaskSend`
  能力认证；`PROXY_REGISTER/ADD` 由管理器转交主体能力、Store 校验目标。P2b-4 的
  单 `cat` 场景从未触发该路径。
- **代理页槽位复用**：关闭时客户端仍映射派生页，Store 的 `memseg_destroy` 延迟
  失败且未重试；`PROXY_OPEN` 现在先重试销毁，仍被映射时保持 `EAGAIN`。
- **Task id 容量**：`MAX_TASKS=18` 且 `create_held` 不复用已销毁 id；每次启动
  只剩两个 guest 根名额。矩阵分两次启动；未提高上限以免平移冻结 id。
- **fixture 契约修订**：新增 `/native/README` 与两条根外逃逸符号链接；P2b-0
  表中已显式登记，原有四个 fixture 字节与哈希不变。

## `Direct` 裁决

**保留。** 依据：

- Store 的直接会话路径（`OPEN/SUBMIT/CLOSE`）是 P2a/P2b 冻结协议的一部分，
  删除 `Direct` 会让该分支失去唯一客户端与编译期使用点。
- 原生场景改走策略视图是隔离需要，不表示直接会话无用途；后续单 personality
  镜像或不需私有根的场景仍可用它。
- 保留不扩大攻击面：直接客户端不参与代理授权，其视图固定为服务根，行为未变。

若 P3/P5 引入新的客户端形态并确认直接路径无人使用，可在对应切片删除，并同步
更新契约与测试。

## 失败、反例与限制

- **组织级复核未完成**：本报告由实现侧汇总，不构成独立审查；P2b-6 的“复核记录”
  验收项仍待组织级复核。不得据此宣称双 personality 完成或 7-A 退出。
- **旧 incarnation 仅 host 覆盖**：QEMU 无法强制任务 id 重生；该负例只有
  `reused_task_ids_cannot_select_old_sessions` host 证据。
- **单侧退出见证的范围**：`S7HALF` 只记录本次启动中谁先报告，不证明任意调度顺序。
- **P2b-4 的 M3 超时波动**：该次 30 秒阈值超时已在其报告中保留；本次完整 72 轮
  无失败，但不改写历史记录。
- **未覆盖**：SMP、配额、完整撤销/恢复、性能曲线、Windows personality、
  真实双白名单全集；P3–P6 未开始。既有 72 轮套件不含代理场景，`test-linux-proxy`
  是独立命令。
- `Direct` 保留但场景未选用；旧协议路径的 QEMU 覆盖只来自历史镜像与 host 测试。

## 阶段/profile 影响

- P2b 作为证据包收口；**P2 本身不签署**，P3–P6 与组织级复核仍使“双 personality
  共同服务”处于进行中。
- 不改变阶段 7 九轨勾选，不关闭 7-A，不改变 profile v1/v2 或 D12 白名单。
- 既有 Linux 差分事件、回执与 72 轮 BIOS 套件行为未修改。

## 后继关系

- 下一批实现：LibOS 计划 P3（有界异步提交/完成与背压）、P4（Gate/IPC 收窄）、
  P5（2/4 vCPU）、P6（撤销/恢复与总验收）。
- holder 模型：计划中的 H0–H7，H5 依赖本收口后的 P2b 绑定。
- 组织级复核：以本报告与 P2b-0…P2b-5 报告为输入；复核记录应单独落盘并回链。
