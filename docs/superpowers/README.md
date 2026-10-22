# Superpowers 文档目录

本目录保存 PrimaOS 的设计、执行计划和实现证据。**它不是平铺的知识库，而是按工件类型和生命周期组织的档案。**

## 先读哪个

| 目标 | 阅读顺序 |
|---|---|
| 了解项目架构和研究目标 | [`Prima OS.md`](../../Prima%20OS.md) → [`COSIR 原语规范.md`](../../COSIR%20原语规范.md) → [`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md) |
| 判断当前做到哪一步 | [`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md) → 本目录 [`reports/`](reports/README.md) 中最新阶段报告 |
| 实现某个阶段切片 | 实现阶段跟踪 → [`plans/`](plans/README.md) 中对应计划 → [`specs/`](specs/README.md) 中对应设计 → 实现报告 |
| 审查某项技术结论 | [`reports/`](reports/README.md) 中的报告/复核记录 → 对应 spec/plan → 原始测试与命令 |
| 查看 POSIX/Linux 兼容范围 | [`specs/2026-10-18-stage5-profile-v0.md`](specs/2026-10-18-stage5-profile-v0.md) → [`compat/`](compat/README.md) → 阶段 5 报告 |

## 四类工件

| 目录 | 作用 | 是否代表“已完成” |
|---|---|---|
| [`specs/`](specs/README.md) | 设计、规格、接口和语义提案 | **否**。设计完成不等于实现完成。 |
| [`plans/`](plans/README.md) | 执行步骤、任务拆分和计划中的验收方式 | **否**。计划是当时的执行记录。 |
| [`reports/`](reports/README.md) | 实现报告、审查、复核、反例和 QEMU/host 证据 | 只代表报告明确声明的范围。 |
| [`compat/`](compat/README.md) | 阶段 5 API/符号兼容状态与追溯表 | 只代表已登记的状态，不等同于 POSIX/Linux 全兼容。 |

当前阶段、退出条件、勾选状态和最新证据索引**只以** [`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md) 为准。

## 分阶段导航

| 阶段 | 主题 | 主要入口 |
|---|---|---|
| 0 | 语义、形式模型、Capability 生命周期 | 阶段 0 签署报告、[`cosir/`](../../cosir/) 模型、[`lean/`](../../lean/) |
| 1 | 启动、物理内存、页表、trap | 阶段 1 设计与 SPEC-146 复核 |
| 2 | Capability、Segment、Task、IPC | 阶段 2 设计与 SPEC-147 复核 |
| 3 | Namespace、Timer、Audit | 阶段 3 设计与 SPEC-148 复核 |
| 4 | 原生运行时、服务管理器、用户态驱动/应用 | SPEC-129/130 与 SPEC-149 复核 |
| 5 | POSIX 源码兼容、musl、C 入口、tty、文件、时间 | [profile v0](specs/2026-10-18-stage5-profile-v0.md)、[阶段 5 收尾路线图](plans/2026-10-22-stage5-completion-roadmap.md)、[compat 台账](compat/README.md) |
| 6 | Linux x86_64 静态 ELF | [阶段 6 入口](plans/2026-10-22-stage5-completion-roadmap.md)（路线图 §7）；尚未开始 |
| 7 | SMP、动态链接、配额、恢复、第二架构 | 候选范围，未冻结 |

## 当前阶段（阶段 5）快速入口

- [POSIX profile v0（冻结）](specs/2026-10-18-stage5-profile-v0.md)
- [POSIX profile v1 delta（M8/M9 提案）](specs/2026-10-20-stage5-profile-v1.md)
- [M8 用户态 VFS 设计](specs/2026-10-20-stage5-user-vfs-design.md)
- [M10 匿名内存/保护设计](specs/2026-10-21-stage5-m10-anon-memory-design.md)
- [M11 文件树变更设计](specs/2026-10-22-stage5-m11-file-mutation-design.md)
- [M12 C 启动/退出设计](specs/2026-10-22-stage5-m12-c-startup-exit-design.md)
- [M13a 符号链接/rename 设计](specs/2026-10-22-stage5-m13a-vfs-links-design.md)
- [M13b 权限/stdio 设计](specs/2026-10-22-stage5-m13b-permissions-stdio-design.md)
- [M19 loopback UDP 设计](specs/2026-10-23-stage5-m19-loopback-udp-design.md)
- [M14 tty 输入设计](specs/2026-10-23-stage5-m14-tty-input-design.md)
- [M15 Process/Thread/WaitObject 设计](specs/2026-10-23-stage5-m15-process-thread-wait-design.md)
- [M17 fork/execve/wait4 设计](specs/2026-10-23-stage5-m17-fork-exec-design.md)
- [阶段 5 收尾与阶段 6 入口路线图](plans/2026-10-22-stage5-completion-roadmap.md)
- [阶段 5 修订计划](plans/2026-10-18-stage5-revised-plan.md)
- [musl 移植计划](plans/2026-10-18-musl-port-plan.md)
- [兼容状态台账](compat/compat-state.md)
- [符号 ↔ syscall 追溯](compat/symbol-trace.md)
- [M5 sbase echo](reports/2026-10-19-stage5-m5-sbase-echo.md)
- [M6 POSIX 时间](reports/2026-10-19-stage5-m6-time.md)
- [M7 fd 表 / dup / getpid](reports/2026-10-19-stage5-m7-fd-table.md)
- [M8 用户态 VFS / rootfs / 多级路径](reports/2026-10-20-stage5-m8-user-vfs.md)
- [M9 statx / chdir / getcwd + sbase cat/ls](reports/2026-10-21-stage5-m9-metadata-cwd.md)
- [M10 匿名内存 / 保护](reports/2026-10-21-stage5-m10-anon-memory.md)
- [M11 文件树变更](reports/2026-10-22-stage5-m11-file-mutation.md)
- [M12 C 启动栈与退出清理](reports/2026-10-22-stage5-m12-c-startup.md)
- [M13a 符号链接/rename/fchdir](reports/2026-10-22-stage5-m13a-vfs-links.md)
- [M13b 权限位/umask/stdio](reports/2026-10-22-stage5-m13b-permissions-stdio.md)
- [M19 用户态 loopback UDP](reports/2026-10-23-stage5-m19-loopback-udp.md)
- [M14 tty 输入](reports/2026-10-23-stage5-m14-tty-input.md)
- [M15a-1 WaitObject](reports/2026-10-23-stage5-m15a-waitobject.md)
- [M15a-2 WaitObject 阻塞](reports/2026-10-23-stage5-m15a2-waitobject-blocking.md)
- [M15b-1 Process/Thread 元数据](reports/2026-10-23-stage5-m15b1-process-metadata.md)
- [M15b-2a 共享能力空间线程](reports/2026-10-23-stage5-m15b2a-thread-create.md)
- [线程容量调查（ISSUE-THREAD-CAPACITY-001）](reports/2026-10-23-stage5-thread-capacity-investigation.md)
- [M15b-2b 线程槽位回收与 W2](reports/2026-10-23-stage5-m15b2b-thread-slots.md)
- [M16a futex WAIT/WAKE](reports/2026-10-23-stage5-m16a-futex.md)
- [M16b-1 clone 基础与 musl 阻塞](reports/2026-10-23-stage5-m16b1-clone-basis.md)
- [M16b-2 musl clone 路由与 pthread 进展](reports/2026-10-23-stage5-m16b2-musl-clone.md)
- [M16b-3 pthread 创建/join 打通](reports/2026-10-23-stage5-m16b3-pthread-join.md)
- [M17b-1 wait4 状态机模型](reports/2026-10-23-stage5-m17b1-wait4-model.md)
- [M17b-2a COW 页表原语](reports/2026-10-23-stage5-m17b2-cow-primitives.md)

## 文档约定

### 命名

新文档使用：

```text
YYYY-MM-DD-<topic>-design.md     # specs/
YYYY-MM-DD-<topic>-plan.md       # plans/
YYYY-MM-DD-<topic>-report.md     # reports/
```

历史文件若没有 `-design` / `-plan` / `-report` 后缀，保留原名，避免破坏已有链接。**不要为了改名而搬动历史文档**；如确需移动，必须同时更新所有引用。

### 新增文档

1. 先确定它属于 `specs/`、`plans/`、`reports/` 还是 `compat/`；
2. 使用日期前缀和主题名；
3. 在对应目录的 `README.md` 索引中加入一行；
4. 如果改变阶段状态或退出证据，同时更新 [`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md)；
5. 报告必须写清证据等级（host / QEMU / Lean / 兼容性差分）和未覆盖边界。

### 历史工件

- 旧报告不因后续复核而改写结论；后续复核应新增报告，并在旧报告或新报告中明确“取代/补强/推翻”关系。
- `plans/` 中的复选框是计划执行痕迹，不等于当前状态。
- 任何“当前是否支持”的判断，以实现阶段跟踪和最新 `compat/` 台账为准。

## 全目录索引

- [设计规格索引](specs/README.md)
- [执行计划索引](plans/README.md)
- [实现报告索引](reports/README.md)
- [兼容状态索引](compat/README.md)
