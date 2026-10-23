# 文档导航

本页是 PrimaOS 文档的统一入口。当前实现状态、退出条件和证据链接以[实现阶段跟踪](../Prima%20OS%20实现阶段.md)为准；历史计划、设计和报告保留其写作时的结论，后续复核可能改变适用范围。

## 快速入口

| 需要了解 | 入口 |
|---|---|
| 项目架构、研究假设与边界 | [Prima OS 白皮书](../Prima%20OS.md) |
| 平台无关操作语义 | [COSIR 原语规范](../COSIR%20原语规范.md) |
| 当前阶段、阻塞、退出条件与证据索引 | [Prima OS 实现阶段](../Prima%20OS%20实现阶段.md) |
| 文档目录说明与阅读顺序 | [Superpowers 文档说明](superpowers/README.md) |
| 对话整理：架构问答 | [PrimaOS 架构问答整理](architecture/primaos-architecture-qa.md) |
| 设计规格索引 | [specs/README.md](superpowers/specs/README.md) |
| 执行计划索引 | [plans/README.md](superpowers/plans/README.md) |
| 实现报告与复核索引 | [reports/README.md](superpowers/reports/README.md) |
| 阶段 5 兼容性台账 | [compat/README.md](superpowers/compat/README.md) |

## 当前阶段（阶段 5）文档

- [POSIX profile v0（冻结）](superpowers/specs/2026-10-18-stage5-profile-v0.md)
- [POSIX profile v1 delta（M8/M9 提案）](superpowers/specs/2026-10-20-stage5-profile-v1.md)
- [M8 用户态 VFS 设计](superpowers/specs/2026-10-20-stage5-user-vfs-design.md)
- [M10 匿名内存/保护设计](superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md)
- [M11 文件树变更设计](superpowers/specs/2026-10-22-stage5-m11-file-mutation-design.md)
- [M12 C 启动/退出设计](superpowers/specs/2026-10-22-stage5-m12-c-startup-exit-design.md)
- [M13a 符号链接/rename 设计](superpowers/specs/2026-10-22-stage5-m13a-vfs-links-design.md)
- [M13b 权限/stdio 设计](superpowers/specs/2026-10-22-stage5-m13b-permissions-stdio-design.md)
- [M19 loopback UDP 设计](superpowers/specs/2026-10-23-stage5-m19-loopback-udp-design.md)
- [M14 tty 输入设计](superpowers/specs/2026-10-23-stage5-m14-tty-input-design.md)
- [M15 Process/Thread/WaitObject 设计](superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)
- [M17 fork/execve/wait4 设计](superpowers/specs/2026-10-23-stage5-m17-fork-exec-design.md)
- [M17c-2 execve syscall 契约](superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md)
- [阶段 5 收尾与阶段 6 入口路线图](superpowers/plans/2026-10-22-stage5-completion-roadmap.md)
- [阶段 5 修订计划](superpowers/plans/2026-10-18-stage5-revised-plan.md)
- [musl 移植计划](superpowers/plans/2026-10-18-musl-port-plan.md)
- [兼容状态台账](superpowers/compat/compat-state.md)
- [符号与系统调用追溯](superpowers/compat/symbol-trace.md)
- 最新实现证据：
  - [M5 sbase echo](superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)
  - [M6 POSIX 时间](superpowers/reports/2026-10-19-stage5-m6-time.md)
  - [M7 fd 表 / dup / getpid](superpowers/reports/2026-10-19-stage5-m7-fd-table.md)
  - [M8 用户态 VFS / rootfs / 多级路径](superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)
  - [M9 statx / chdir / getcwd + sbase cat/ls](superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)
  - [M10 匿名内存 / 保护](superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)
  - [M11 文件树变更](superpowers/reports/2026-10-22-stage5-m11-file-mutation.md)
  - [M12 C 启动栈与退出清理](superpowers/reports/2026-10-22-stage5-m12-c-startup.md)
  - [M13a 符号链接/rename/fchdir](superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)
  - [M13b 权限位/umask/stdio](superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md)
  - [M19 用户态 loopback UDP](superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md)
  - [M14 tty 输入](superpowers/reports/2026-10-23-stage5-m14-tty-input.md)
  - [M15a-1 WaitObject](superpowers/reports/2026-10-23-stage5-m15a-waitobject.md)
  - [M15a-2 WaitObject 阻塞](superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md)
  - [M15b-1 Process/Thread 元数据](superpowers/reports/2026-10-23-stage5-m15b1-process-metadata.md)
  - [M15b-2a 共享能力空间线程](superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md)
  - [线程容量调查（ISSUE-THREAD-CAPACITY-001）](superpowers/reports/2026-10-23-stage5-thread-capacity-investigation.md)
  - [M15b-2b 线程槽位回收与 W2](superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)
  - [M16a futex WAIT/WAKE](superpowers/reports/2026-10-23-stage5-m16a-futex.md)
  - [M16b-1 clone 基础与 musl 阻塞](superpowers/reports/2026-10-23-stage5-m16b1-clone-basis.md)
  - [M16b-2 musl clone 路由与 pthread 进展](superpowers/reports/2026-10-23-stage5-m16b2-musl-clone.md)
  - [M16b-3 pthread 创建/join 打通](superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)
  - [M17b-1 wait4 状态机模型](superpowers/reports/2026-10-23-stage5-m17b1-wait4-model.md)
  - [M17b-2a COW 页表原语](superpowers/reports/2026-10-23-stage5-m17b2-cow-primitives.md)
  - [M17b-2b general COW 与 fork](superpowers/reports/2026-10-23-stage5-m17b2-cow-fork.md)
  - [M17b-3 僵尸保留与唤醒回收](superpowers/reports/2026-10-23-stage5-m17b3-zombie-wait.md)
  - [M17b-4 fork/wait 打通（W6）](superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)
  - [M17c-1 静态 ELF 解析与初始栈](superpowers/reports/2026-10-23-stage5-m17c1-elf-plan.md)
  - [M18a 最小信号 kill](superpowers/reports/2026-10-23-stage5-m18a-signal-kill.md)
  - [M18b sigaction/handler/sigreturn](superpowers/reports/2026-10-23-stage5-m18b-sigaction.md)
  - [M17c-3 exec_replace（W7）](superpowers/reports/2026-10-23-stage5-m17c3-exec.md)
  - [M17c-4 VFS 镜像 exec（W7b）](superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md)

## 阶段 0–4 签署入口

| 阶段 | 复核报告 |
|---|---|
| 0 — 语义与形式模型 | [SPEC-145](superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md) |
| 1 — 启动、内存与 trap | [SPEC-146](superpowers/reports/2026-10-17-stage1-exit-review-spec146.md) |
| 2 — Capability、Segment、Task、IPC | [SPEC-147](superpowers/reports/2026-10-17-stage2-exit-review-spec147.md) |
| 3 — Namespace、Timer、Audit | [SPEC-148](superpowers/reports/2026-10-17-stage3-exit-review-spec148.md) |
| 4 — 原生运行时、服务与应用 | [SPEC-149](superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) |

签署后整改和复核：

- [阶段 1 遗留问题整改 SPEC-151](superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)
- [阶段 2 IPC 整改](superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)
- [阶段 3 复核整改](superpowers/reports/2026-10-18-stage3-review-remediation.md)
- [阶段 1–4 实现复核](superpowers/reports/2026-10-18-stage1-4-implementation-review.md)

## 文档目录结构

```text
docs/
├── README.md                 # 本页：统一入口
├── architecture/             # 架构说明与对话整理
│   └── primaos-architecture-qa.md
└── superpowers/
    ├── README.md             # 文档约定、阅读顺序、分阶段导航
    ├── specs/                # 设计/规格：预期语义，不代表已完成
    ├── plans/                # 执行计划：任务拆分和历史执行记录
    ├── reports/              # 实现/审查/复核报告：实际证据与边界
    └── compat/               # 阶段 5 兼容状态与符号追溯
```

## 证据边界

- **specs**：设计与预期，不等于实现完成；
- **plans**：执行步骤，不等于当前状态；
- **reports**：报告明确声明的 host / QEMU / Lean / 差分证据范围；
- **compat**：已登记的兼容状态，不自动等于 POSIX/Linux 全兼容；
- **实现阶段跟踪**：当前阶段、退出条件和证据索引的权威入口。

host 测试、QEMU 执行、Lean 模型和兼容性差分不能互相替代。任何安全、兼容性或性能结论都必须按相应证据范围解释。

## 维护规则

1. 新文档先归入 `specs/`、`plans/`、`reports/` 或 `compat/`，不要平铺在 `docs/` 根下；
2. 使用日期前缀和主题名，具体命名规则见 [superpowers/README.md](superpowers/README.md)；
3. 新文档必须同步加入对应目录的 `README.md` 索引；
4. 改变阶段状态或退出证据时，同步更新[实现阶段跟踪](../Prima%20OS%20实现阶段.md)；
5. 历史报告不改写结论；后续复核通过新增报告建立“补强/取代”关系。
