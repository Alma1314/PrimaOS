---
id: reports-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-31
---

# Reports 索引

本次统一验收：[阶段0–5缺口后继](2026-10-25-stage0-5-gap-followup.md)，workspace1016、lint、Lean、Rust/musl、compat及完整单次BIOS68轮通过；保留失败修复和时序断言修订，最终范围/阶段签署待确认。

本目录保存**实现报告与审查/复核证据**：记录实际命令、结果、计数、反例和证据边界。历史报告保留其写作时的结论，后续复核可能改变适用范围。

> 本索引按阶段/主题分组，文件日期用于历史定位。新增文档同步更新本页和相关主题路由，再运行 `python3 scripts/docs.py build` / `check`。统一入口见[文档中心](../../README.md)。

## OSTD / 跨项目代码复用

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-asterinas-once-publication-report.md](2026-10-24-asterinas-once-publication-report.md) | Asterinas 参考续篇：一次初始化与启动发布验证 |
| 2026-10-24 | [2026-10-24-asterinas-pod-buffer-report.md](2026-10-24-asterinas-pod-buffer-report.md) | Asterinas 参考续篇：POD 与恢复帧缓冲边界 |
| 2026-10-24 | [2026-10-24-asterinas-reuse-report.md](2026-10-24-asterinas-reuse-report.md) | Asterinas 复用与 OSTD 临界区验证 |

## 跨阶段 / 文档体系

源码细化：[生产 Rust 句柄方法→Lean](2026-10-31-rust-handle-source-refinement.md)，Charon/Aeneas 自动提取、生成物新鲜度、5 条一般定理与真实 Rust mutation；范围限构造/后继值，翻译器为可信边界。

形式化后继：[有限能力槽表组合证明](2026-10-31-kernel-handle-table-proof.md)，10 条一般 Lean 定理、整表任意序列无复活、真实 Rust 三槽六步回归；不宣称 Rust 源码细化证明。

阶段 1～5 的当前后继整改：[VFS 内容别名与 musl 检查护栏](2026-10-31-stage1-5-regression-remediation.md)，保留历史签署及其限制。

独立审查：[实现质量独立审查](2026-10-27-implementation-quality-review.md)，host 全门禁实测（1018 测试、lint、TCB 6.42%、docs check）、R1 双轮启动通过，并复现 musl 审计的 locale 依赖缺陷；不改阶段签署。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-27 | [2026-10-27-implementation-quality-review.md](2026-10-27-implementation-quality-review.md) | 实现质量独立审查：门禁可复现性与结构负债（公开缺陷：musl 审计 locale 依赖） |
| 2026-10-24 | [2026-10-24-documentation-lifecycle-report.md](2026-10-24-documentation-lifecycle-report.md) | 文档生命周期治理与过时材料清理 |
| 2026-10-23 | [2026-10-23-documentation-restructure-report.md](2026-10-23-documentation-restructure-report.md) | 文档体系重构与验收报告 |

## 阶段 5

**出口签署（2026-10-27）**：[阶段 5 出口签署 SPEC-152](2026-10-27-stage5-exit-signoff.md)，冻结有界 profile v2、确认 D1–D10 处置、按阶段 5 范围关闭 BLK-003；签署复跑与两处夹具修复记录在该报告，历史失败保留。

POSIX std最新续做：[文件I/O与接收归账](2026-10-27-posix-std-file-io-report.md)，单次36组72/72轮、workspace1020、lint与三模式Clippy通过；初始69/70失败保留，底层支持仍有界。

本批增量：[启动、栈保护、退出与 Rust 工作目标](2026-10-25-stage5-gap-closure-report.md)。

本轮进程后继：[fork 续执行与进程边界整改记录](2026-10-24-fork-continuation-report.md)。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-musl-port-boundary-report.md](2026-10-24-musl-port-boundary-report.md) | musl 移植补强：调用边界与 fd/stdio 验证 |
| 2026-10-23 | [2026-10-23-vfs-libos-foundation-report.md](2026-10-23-vfs-libos-foundation-report.md) | VFS / LibOS 基础重构验证报告 |
| 2026-10-22 | [2026-10-22-stage5-m11-file-mutation.md](2026-10-22-stage5-m11-file-mutation.md) | 阶段 5 M11：文件树变更（`mkdir`/`rmdir`/`unlink`） |
| 2026-10-22 | [2026-10-22-stage5-m12-c-startup.md](2026-10-22-stage5-m12-c-startup.md) | 阶段 5 M12：C 启动栈与退出清理（envp/auxv、自动关闭 fd） |
| 2026-10-22 | [2026-10-22-stage5-m13a-vfs-links.md](2026-10-22-stage5-m13a-vfs-links.md) | 阶段 5 M13a：符号链接/readlink/rename/fchdir、内容区压实 |
| 2026-10-22 | [2026-10-22-stage5-m13b-permissions-stdio.md](2026-10-22-stage5-m13b-permissions-stdio.md) | 阶段 5 M13b：节点 mode、chmod/fchmod/umask、readv 与 stdio 文件 API |
| 2026-10-23 | [2026-10-23-stage5-m19-loopback-udp.md](2026-10-23-stage5-m19-loopback-udp.md) | 阶段 5 M19：用户态 loopback IPv4/UDP、socket/端口/数据报 |
| 2026-10-23 | [2026-10-23-stage5-m14-tty-input.md](2026-10-23-stage5-m14-tty-input.md) | 阶段 5 M14：fd 0 tty 输入、canonical/raw、termios 子集 |
| 2026-10-23 | [2026-10-23-stage5-m15a-waitobject.md](2026-10-23-stage5-m15a-waitobject.md) | 阶段 5 M15a-1：WaitObject 创建/观察/信号/立即等待/销毁 |
| 2026-10-23 | [2026-10-23-stage5-m15a2-waitobject-blocking.md](2026-10-23-stage5-m15a2-waitobject-blocking.md) | 阶段 5 M15a-2：BlockedWait/唤醒/超时/结果注入 |
| 2026-10-23 | [2026-10-23-stage5-m15b1-process-metadata.md](2026-10-23-stage5-m15b1-process-metadata.md) | 阶段 5 M15b-1：Process/Thread 元数据 |
| 2026-10-23 | [2026-10-23-stage5-m15b2a-thread-create.md](2026-10-23-stage5-m15b2a-thread-create.md) | 阶段 5 M15b-2a：共享能力空间的 thread_create（QEMU 容量阻塞） |
| 2026-10-23 | [2026-10-23-stage5-thread-capacity-investigation.md](2026-10-23-stage5-thread-capacity-investigation.md) | ISSUE-THREAD-CAPACITY-001 调查：扩容破坏 task_ready 见证 |
| 2026-10-23 | [2026-10-23-stage5-m15b2b-thread-slots.md](2026-10-23-stage5-m15b2b-thread-slots.md) | 阶段 5 M15b-2b：线程槽位回收、W2 端到端唤醒 |
| 2026-10-23 | [2026-10-23-stage5-m16a-futex.md](2026-10-23-stage5-m16a-futex.md) | 阶段 5 M16a：futex WAIT/WAKE（用户态表 + WaitObject） |
| 2026-10-23 | [2026-10-23-stage5-m16b1-clone-basis.md](2026-10-23-stage5-m16b1-clone-basis.md) | 阶段 5 M16b-1：clone/线程退出内核基础与 musl clone.s 阻塞 |
| 2026-10-23 | [2026-10-23-stage5-m16b2-musl-clone.md](2026-10-23-stage5-m16b2-musl-clone.md) | 阶段 5 M16b-2：musl clone.s 补丁、pthread_create 成功、join 唤醒待解 |
| 2026-10-23 | [2026-10-23-stage5-m16b3-pthread-join.md](2026-10-23-stage5-m16b3-pthread-join.md) | 阶段 5 M16b-3：clear_child_tid、pthread create/join、共享内存可见性 |
| 2026-10-23 | [2026-10-23-stage5-m17b1-wait4-model.md](2026-10-23-stage5-m17b1-wait4-model.md) | 阶段 5 M17b-1：wait4 状态机模型（阻塞/唤醒/回收/ECHILD） |
| 2026-10-23 | [2026-10-23-stage5-m17b2-cow-primitives.md](2026-10-23-stage5-m17b2-cow-primitives.md) | 阶段 5 M17b-2a：COW 页表克隆/写故障原语 |
| 2026-10-23 | [2026-10-23-stage5-m17b2-cow-fork.md](2026-10-23-stage5-m17b2-cow-fork.md) | 阶段 5 M17b-2b：general COW、fork syscall、W6 部分通过 |
| 2026-10-23 | [2026-10-23-stage5-m17b3-zombie-wait.md](2026-10-23-stage5-m17b3-zombie-wait.md) | 阶段 5 M17b-3：fork 僵尸保留、唤醒回收（W6 未接入） |
| 2026-10-23 | [2026-10-23-stage5-m17b4-fork-wait.md](2026-10-23-stage5-m17b4-fork-wait.md) | 阶段 5 M17b-4：fork/wait 打通（W6 双轮） |
| 2026-10-23 | [2026-10-23-stage5-m17c1-elf-plan.md](2026-10-23-stage5-m17c1-elf-plan.md) | 阶段 5 M17c-1：静态 ELF 解析与初始栈（host） |
| 2026-10-23 | [2026-10-23-stage5-m18a-signal-kill.md](2026-10-23-stage5-m18a-signal-kill.md) | 阶段 5 M18a：kill 默认终止与 wait4 信号状态（W8） |
| 2026-10-23 | [2026-10-23-stage5-m18b-sigaction.md](2026-10-23-stage5-m18b-sigaction.md) | 阶段 5 M18b：sigaction/handler/sigreturn（W9） |
| 2026-10-23 | [2026-10-23-stage5-m17c3-exec.md](2026-10-23-stage5-m17c3-exec.md) | 阶段 5 M17c-3：exec_replace（W7） |
| 2026-10-23 | [2026-10-23-stage5-m17c4-execve-vfs.md](2026-10-23-stage5-m17c4-execve-vfs.md) | 阶段 5 M17c-4：VFS 镜像 exec（W7b） |
| 2026-10-23 | [2026-10-23-stage5-m20-compat-final.md](2026-10-23-stage5-m20-compat-final.md) | 阶段 5 M20：兼容矩阵定稿 |
| 2026-10-23 | [2026-10-23-stage5-m21-exit-review.md](2026-10-23-stage5-m21-exit-review.md) | 阶段 5 M21：出口评审（未签署） |
| 2026-10-21 | [2026-10-21-stage5-m10-anon-memory.md](2026-10-21-stage5-m10-anon-memory.md) | 阶段 5 M10：匿名内存与保护（`mmap`/`munmap`/`mprotect` + 内核 `memseg_protect`） |
| 2026-10-21 | [2026-10-21-stage5-m9-metadata-cwd.md](2026-10-21-stage5-m9-metadata-cwd.md) | 阶段 5 M9：`statx` 元数据、`chdir`/`getcwd` 与 sbase `cat`/`ls` |
| 2026-10-20 | [2026-10-20-stage5-m8-user-vfs.md](2026-10-20-stage5-m8-user-vfs.md) | 阶段 5 M8：用户态 VFS/rootfs、通用 fd 与多级路径 |
| 2026-10-19 | [2026-10-19-stage5-m7-fd-table.md](2026-10-19-stage5-m7-fd-table.md) | 阶段 5 M7：完整本地 fd 表、`dup` 与 `getpid` |
| 2026-10-19 | [2026-10-19-stage5-m6-time.md](2026-10-19-stage5-m6-time.md) | 阶段 5 M6：POSIX 时间首切片（clock_gettime / nanosleep） |
| 2026-10-19 | [2026-10-19-stage5-m5-sbase-echo.md](2026-10-19-stage5-m5-sbase-echo.md) | 阶段 5 · M5 首切片：sbase `echo` E1–E6 |
| 2026-10-18 | [2026-10-18-stage5-task16-qemu.md](2026-10-18-stage5-task16-qemu.md) | 阶段 5：服务后 Task 16 的 QEMU 见证 |
| 2026-10-18 | [2026-10-18-stage5-ptload-wx.md](2026-10-18-stage5-ptload-wx.md) | 阶段 5：`PT_LOAD` 写段/BSS 私有装载与 W^X |
| 2026-10-18 | [2026-10-18-stage5-native-abi-check.md](2026-10-18-stage5-native-abi-check.md) | 阶段 5：原生 ABI 版本握手见证 |
| 2026-10-18 | [2026-10-18-stage5-musl-m4-file.md](2026-10-18-stage5-musl-m4-file.md) | 阶段 5 / M4：文件与 fd 生命周期（首切片） |
| 2026-10-18 | [2026-10-18-stage5-musl-m3-base.md](2026-10-18-stage5-musl-m3-base.md) | 阶段 5 · musl M3：SSE/FPU 契约与静态 musl 基座 |
| 2026-10-18 | [2026-10-18-stage5-musl-m2-boundary.md](2026-10-18-stage5-musl-m2-boundary.md) | 阶段 5 · musl M2：C 系统边界与 per-task FS base |
| 2026-10-18 | [2026-10-18-stage5-musl-m0-inventory.md](2026-10-18-stage5-musl-m0-inventory.md) | 阶段 5：musl M0 依赖盘点 |
| 2026-10-18 | [2026-10-18-stage5-main-return-status.md](2026-10-18-stage5-main-return-status.md) | 阶段 5：同源 C `main` 返回码见证 |
| 2026-10-18 | [2026-10-18-stage5-kickoff.md](2026-10-18-stage5-kickoff.md) | 阶段 5 启动：首个新 Task 容量与 profile 草案 |
| 2026-10-18 | [2026-10-18-stage5-entry-analysis.md](2026-10-18-stage5-entry-analysis.md) | 阶段 5 准入分析：POSIX 源码兼容 |
| 2026-10-18 | [2026-10-18-stage5-capacity-watermarks.md](2026-10-18-stage5-capacity-watermarks.md) | 阶段 5：服务后静态容量水位 |
| 2026-10-18 | [2026-10-18-stage5-c-tty-write.md](2026-10-18-stage5-c-tty-write.md) | 阶段 5 片 C：tty 字节流、`write(1/2)` 与同源 `_exit` 见证 |
| 2026-10-18 | [2026-10-18-stage5-c-start-stack.md](2026-10-18-stage5-c-start-stack.md) | 阶段 5：C Task 初始栈见证 |
| 2026-10-18 | [2026-10-18-stage5-c-elf-qemu.md](2026-10-18-stage5-c-elf-qemu.md) | 阶段 5：首个 C ELF 的服务后 QEMU 见证 |

## 阶段 6（进行中，2026-10-29 启动）

阶段 6 于 2026-10-29 由作者指令启动，状态为**进行中**；退出条件与交付检查仍全部未勾选。
本节收录阶段 6 的实现证据与准备性证据。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-30 | [2026-10-30-stage6-audit-stop-qemu.md](2026-10-30-stage6-audit-stop-qemu.md) | 审计停止裸机验证：七个执行边界的专用 QEMU 注入与停止调用变异拒绝；非 G1–G3 |
| 2026-10-30 | [2026-10-30-stage6-audit-fail-stop.md](2026-10-30-stage6-audit-fail-stop.md) | M1b 审计停止控制器：首次诊断、不可恢复标记、分派/调度/IRQ 接线；非 G1–G3 |
| 2026-10-30 | [2026-10-30-stage6-private-completion-waits.md](2026-10-30-stage6-private-completion-waits.md) | M1b 私有等待基础：真实对象登记、不可警报、代次完成、销毁与独立预算；Gate 尚未接入，非 G1/G2 |
| 2026-10-30 | [2026-10-30-stage6-user-copy-transactions.md](2026-10-30-stage6-user-copy-transactions.md) | M1b/M1c 内存前置：真实页表全范围输出预检、COW 预留回滚；原生输出/IPC/wait 接入，W9 跨页 COW，非 Gate G3 |
| 2026-10-29 | [2026-10-29-stage6-m1-blocker-closure.md](2026-10-29-stage6-m1-blocker-closure.md) | R1–R6 协议收口、候选 A 技术设计评审、u64 权限及 8 组 host 反例；D14 ring3 人格桩 QEMU 双轮，非 Gate 接线/阶段退出 |
| 2026-10-29 | [2026-10-29-stage6-m1-decision-review.md](2026-10-29-stage6-m1-decision-review.md) | S6-M1 六项决定复审：保留路线，R1–R6 需修订；静态预审，非架构通过 |
| 2026-10-29 | [2026-10-29-stage6-start.md](2026-10-29-stage6-start.md) | 阶段 6 启动与首个 S6-M2 增量：确定性 `AT_RANDOM` |
| 2026-10-28 | [2026-10-28-stage6-loader-slice.md](2026-10-28-stage6-loader-slice.md) | 阶段 6 装载一致性切片（启动前）：静态 ELF 装载与初始 auxv，含未完成清单 |

## 阶段 4

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-native-runtime-service-report.md](2026-10-24-native-runtime-service-report.md) | 原生运行时与服务重构验证 |
| 2026-10-17 | [2026-10-17-stage4-manifest-index-report.md](2026-10-17-stage4-manifest-index-report.md) | SPEC-132：从 manifest 顺序确定 init 服务索引 |
| 2026-10-17 | [2026-10-17-stage4-init-dynamic-handles-report.md](2026-10-17-stage4-init-dynamic-handles-report.md) | SPEC-131：init 自检读取动态 Capability 句柄 |
| 2026-10-17 | [2026-10-17-stage4-exit-review-spec149.md](2026-10-17-stage4-exit-review-spec149.md) | SPEC-149：阶段 4 退出条件复核与签署 |
| 2026-10-17 | [2026-10-17-exo3m-kernel-service-witness-retirement-report.md](2026-10-17-exo3m-kernel-service-witness-retirement-report.md) | SPEC-138 / EXO-3M：内核服务 witness 组装退役 |
| 2026-10-17 | [2026-10-17-exo3l-shared-trace-only-report.md](2026-10-17-exo3l-shared-trace-only-report.md) | SPEC-136 / EXO-3L：共享 trace 成为唯一记录通道 |
| 2026-10-17 | [2026-10-17-exo3k-shared-trace-bytes-report.md](2026-10-17-exo3k-shared-trace-bytes-report.md) | SPEC-135 / EXO-3K：共享页原始 trace 字节流 |
| 2026-10-17 | [2026-10-17-exo3j-raw-trace-validation-report.md](2026-10-17-exo3j-raw-trace-validation-report.md) | SPEC-133 / EXO-3J：原始 trace 进入 QEMU 验收 |
| 2026-10-16 | [2026-10-16-stage4-native-shared-segment-app-report.md](2026-10-16-stage4-native-shared-segment-app-report.md) | SPEC-129：原生应用的消息控制与共享 Segment 数据路径 |
| 2026-10-16 | [2026-10-16-stage4-closure-report.md](2026-10-16-stage4-closure-report.md) | SPEC-130：阶段 4 原生应用、最小权限与故障隔离收口 |
| 2026-10-16 | [2026-10-16-exo3i-raw-trace-debug-channel-report.md](2026-10-16-exo3i-raw-trace-debug-channel-report.md) | SPEC-128 / EXO-3I：原始 trace 调试通道 |
| 2026-10-16 | [2026-10-16-exo3h-servicemgr-trace-report.md](2026-10-16-exo3h-servicemgr-trace-report.md) | SPEC-127 / EXO-3H：`servicemgr_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3g-service-selected-trace-report.md](2026-10-16-exo3g-service-selected-trace-report.md) | SPEC-126 / EXO-3G：`service_selected_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3f-service-negotiated-trace-report.md](2026-10-16-exo3f-service-negotiated-trace-report.md) | SPEC-125 / EXO-3F：`service_negotiated_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3e-service-reply-trace-report.md](2026-10-16-exo3e-service-reply-trace-report.md) | SPEC-124 / EXO-3E：`service_reply_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3d-generic-trace-runtime-hello-report.md](2026-10-16-exo3d-generic-trace-runtime-hello-report.md) | SPEC-123 / EXO-3D：通用 trace 通道 + `runtime_hello_ready` 迁移 |
| 2026-10-16 | [2026-10-16-exo3c-protocol-tag-decoupling-report.md](2026-10-16-exo3c-protocol-tag-decoupling-report.md) | SPEC-122 / EXO-3C：协议 tag 不再进入内核 witness |
| 2026-10-16 | [2026-10-16-exo2d-manifest-boot-abi-report.md](2026-10-16-exo2d-manifest-boot-abi-report.md) | SPEC-121 / EXO-2D：通用 entry ABI + 用户态 manifest |
| 2026-10-16 | [2026-10-16-exo2c-init-loader-report.md](2026-10-16-exo2c-init-loader-report.md) | SPEC-120 / EXO-2C：init 机器码迁移到 ramdisk 的 ring3 opaque ELF |
| 2026-10-15 | [2026-10-15-stage4-user-driver-ioport-report.md](2026-10-15-stage4-user-driver-ioport-report.md) | SPEC-114：第一个 ring3 用户态驱动（`prima-driver`，S4-D4 第二步） |
| 2026-10-15 | [2026-10-15-stage4-service-storage-segment-report.md](2026-10-15-stage4-service-storage-segment-report.md) | SPEC-112：日志/存储服务持内核 MemorySegment 落数据（S4-D3 收口） |
| 2026-10-15 | [2026-10-15-stage4-device-ioport-capability-report.md](2026-10-15-stage4-device-ioport-capability-report.md) | SPEC-113：设备 `IOPortRange` 对象与受控 I/O 端口路径（S4-D4 内核侧） |
| 2026-10-15 | [2026-10-15-exo5-mechanism-baseline-review.md](2026-10-15-exo5-mechanism-baseline-review.md) | DOC-008 / EXO-5：机制基线再评估（审查记录） |
| 2026-10-15 | [2026-10-15-exo3b-witness-expectations-report.md](2026-10-15-exo3b-witness-expectations-report.md) | SPEC-119 / EXO-3B：witness 期望值移出内核 |
| 2026-10-15 | [2026-10-15-exo3a-kernel-name-decoupling-report.md](2026-10-15-exo3a-kernel-name-decoupling-report.md) | SPEC-118 / EXO-3A：内核不再解释服务名 |
| 2026-10-15 | [2026-10-15-exo2b-exo4a-report.md](2026-10-15-exo2b-exo4a-report.md) | SPEC-117 / EXO-2B + EXO-4A：共享映像格式与设备可见撤销 |
| 2026-10-15 | [2026-10-15-exo2a-name-free-service-directory-report.md](2026-10-15-exo2a-name-free-service-directory-report.md) | SPEC-116 / EXO-2A：无服务名的映像目录 |
| 2026-10-15 | [2026-10-15-exo1-boot-image-report.md](2026-10-15-exo1-boot-image-report.md) | SPEC-115 / EXO-1：通用 boot image（ramdisk）装载 |
| 2026-10-14 | [2026-10-14-stage4-three-base-services-report.md](2026-10-14-stage4-three-base-services-report.md) | SPEC-106 报告：S4-D3 三个基础服务（日志 / Namespace / 基础存储）接入 ring3 服务表 |
| 2026-10-14 | [2026-10-14-stage4-stateful-service-protocol-report.md](2026-10-14-stage4-stateful-service-protocol-report.md) | SPEC-110：有状态服务请求协议（三服务语义下沉） |
| 2026-10-14 | [2026-10-14-stage4-stack-segment-creation-sink-report.md](2026-10-14-stage4-stack-segment-creation-sink-report.md) | SPEC-108 报告：服务**栈段创建**下沉到 ring3 服务管理器 |
| 2026-10-14 | [2026-10-14-stage4-servicemgr-migration-body-report.md](2026-10-14-stage4-servicemgr-migration-body-report.md) | SPEC-104（主体）报告：注册表/按名解析/路由转发迁入 ring3 servicemgr |
| 2026-10-14 | [2026-10-14-stage4-servicemgr-endpoint-creation-report.md](2026-10-14-stage4-servicemgr-endpoint-creation-report.md) | SPEC-105 报告：ring3 服务管理器**表驱动创建端点** |
| 2026-10-14 | [2026-10-14-stage4-service-image-segment-report.md](2026-10-14-stage4-service-image-segment-report.md) | SPEC-109：单一服务映像段（代码段移交下沉到 ring3 服务管理器） |
| 2026-10-14 | [2026-10-14-stage4-ring3-servicemgr-task-report.md](2026-10-14-stage4-ring3-servicemgr-task-report.md) | SPEC-103 报告：ring3 服务管理器任务的引入（迁移第一片） |
| 2026-10-14 | [2026-10-14-stage4-peer-id-entry-report.md](2026-10-14-stage4-peer-id-entry-report.md) | SPEC-104（前置片）报告：对端 task id 的入口 ABI |
| 2026-10-14 | [2026-10-14-stage4-namespace-service-proxy-report.md](2026-10-14-stage4-namespace-service-proxy-report.md) | SPEC-111：Namespace 服务代理内核解析（S4-D3 收口） |
| 2026-10-14 | [2026-10-14-stage4-destroy-stack-snapshot-report.md](2026-10-14-stage4-destroy-stack-snapshot-report.md) | SPEC-107 报告：销毁路径 vspace 栈快照治本（滚动拆除取代栈上 VA 快照） |
| 2026-10-13 | [2026-10-13-stage4-string-service-names-report.md](2026-10-13-stage4-string-service-names-report.md) | SPEC-101 报告：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1） |
| 2026-10-12 | [2026-10-12-stage4-runtime-service-registry-report.md](2026-10-12-stage4-runtime-service-registry-report.md) | SPEC-095 报告：运行期服务注册表（闭合 SPEC-093 §5.2） |
| 2026-10-12 | [2026-10-12-stage4-cap-slot-separation-report.md](2026-10-12-stage4-cap-slot-separation-report.md) | Capability 槽位区间分离（`ISSUE-CAP-002`） |
| 2026-10-12 | [2026-10-12-stage4-alpha-independent-stack-report.md](2026-10-12-stage4-alpha-independent-stack-report.md) | SPEC-094 报告：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1） |
| 2026-10-11 | [2026-10-11-stage4-vspace-arena-report.md](2026-10-11-stage4-vspace-arena-report.md) | SPEC-091 报告：逻辑 vspace 存储移出 `BootstrapState`（arena 化） |
| 2026-10-11 | [2026-10-11-stage4-service-manager-report.md](2026-10-11-stage4-service-manager-report.md) | SPEC-090 报告：服务管理器（init 按名解析并路由，阶段 4 S4-D2 前置） |
| 2026-10-11 | [2026-10-11-stage4-multi-endpoint-service-report.md](2026-10-11-stage4-multi-endpoint-service-report.md) | SPEC-092 报告：独立第二服务端点（init 按名路由到不同任务） |
| 2026-10-11 | [2026-10-11-stage4-malformed-handle-bounds-report.md](2026-10-11-stage4-malformed-handle-bounds-report.md) | SPEC-089 报告：畸形句柄越界（阶段 4 缺陷修复，`ISSUE-CAP-001`） |
| 2026-10-11 | [2026-10-11-stage4-independent-service-images-report.md](2026-10-11-stage4-independent-service-images-report.md) | SPEC-093 报告：独立第二服务端点（独立二进制映像 + 独立物理栈段） |
| 2026-10-10 | [2026-10-10-stage4-service-negotiation-report.md](2026-10-10-stage4-service-negotiation-report.md) | SPEC-088 报告：服务端解析与协商（阶段 4 S4-D2 前置） |
| 2026-10-10 | [2026-10-10-stage4-ring3-runtime-hello-report.md](2026-10-10-stage4-ring3-runtime-hello-report.md) | SPEC-085 报告：独立 ring3 原生客户端与 IPC Hello 握手 |
| 2026-10-10 | [2026-10-10-stage4-mm-vspace-fixes-report.md](2026-10-10-stage4-mm-vspace-fixes-report.md) | SPEC-086 报告：阶段 4 期内的 mm/vspace 正确性与性能修复批次 |
| 2026-10-10 | [2026-10-10-stage4-init-client-handshake-report.md](2026-10-10-stage4-init-client-handshake-report.md) | SPEC-087 报告：init ↔ 存活 ring3 客户端双向握手 |
| 2026-09-25 | [2026-09-25-stage4-runtime-api-prototype-report.md](2026-09-25-stage4-runtime-api-prototype-report.md) | SPEC-084 报告：阶段 4 原生运行时 API 初始实现 |

## 阶段 3

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-18 | [2026-10-18-stage3-review-remediation.md](2026-10-18-stage3-review-remediation.md) | 阶段 3 实现复核与整改（2026-10-18） |
| 2026-10-17 | [2026-10-17-stage3-exit-review-spec148.md](2026-10-17-stage3-exit-review-spec148.md) | SPEC-148：阶段 3 退出条件复核与签署 |
| 2026-10-10 | [2026-10-10-stage3-audit-trace-restore-report.md](2026-10-10-stage3-audit-trace-restore-report.md) | SPEC-083 实现恢复报告：`audit(cap)` 在 master 上落地（§13.2） |
| 2026-09-25 | [2026-09-25-stage3-timer-report.md](2026-09-25-stage3-timer-report.md) | 切片 AK 报告：阶段 3 第二片 —— Timer（§10 全量） |
| 2026-09-25 | [2026-09-25-stage3-namespace-report.md](2026-09-25-stage3-namespace-report.md) | 切片 AJ 报告：阶段 3 第一片 —— Namespace 路径操作（§11 全量） |
| 2026-09-25 | [2026-09-25-stage3-audit-trace-report.md](2026-09-25-stage3-audit-trace-report.md) | 切片 AL 报告：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2） |

## 阶段 2 / Framekernel

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-kernel-handle-model-report.md](2026-10-24-kernel-handle-model-report.md) | 内核句柄、权限与 Lean 模型补强 |
| 2026-10-18 | [2026-10-18-stage2-ipc-review-remediation.md](2026-10-18-stage2-ipc-review-remediation.md) | 阶段 2 实现复核与 IPC 整改 |
| 2026-10-17 | [2026-10-17-stage2-exit-review-spec147.md](2026-10-17-stage2-exit-review-spec147.md) | SPEC-147：阶段 2 退出条件复核与签署 |
| 2026-09-25 | [2026-09-25-framekernel-skeleton-report.md](2026-09-25-framekernel-skeleton-report.md) | 切片 AE 报告：framekernel 分层与 unsafe 收敛（SPEC-076） |
| 2026-09-25 | [2026-09-25-framekernel-mm-pod-report.md](2026-09-25-framekernel-mm-pod-report.md) | 切片 AF 报告：物理内存与页表 POD 收敛（SPEC-077） |
| 2026-09-25 | [2026-09-25-framekernel-global-state-report.md](2026-09-25-framekernel-global-state-report.md) | 切片 AG 报告：内核全局状态 safe 化（SPEC-078） |
| 2026-09-25 | [2026-09-25-framekernel-entry-inversion-report.md](2026-09-25-framekernel-entry-inversion-report.md) | 切片 AH 报告：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079） |
| 2026-09-25 | [2026-09-25-framekernel-closure-report.md](2026-09-25-framekernel-closure-report.md) | 切片 AI 报告：收口与 TCB 量化（SPEC-080） |
| 2026-09-24 | [2026-09-24-stage2-self-built-cr3-report.md](2026-09-24-stage2-self-built-cr3-report.md) | 切片 X 报告：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001） |
| 2026-09-24 | [2026-09-24-stage2-scheduler-state-audit-report.md](2026-09-24-stage2-scheduler-state-audit-report.md) | 切片 Z 报告：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003） |
| 2026-09-24 | [2026-09-24-stage2-reclaim-predicate-tightening-report.md](2026-09-24-stage2-reclaim-predicate-tightening-report.md) | 切片 AC 报告：内核回收判据收紧（SPEC-074） |
| 2026-09-24 | [2026-09-24-stage2-kernel-reclaim-report.md](2026-09-24-stage2-kernel-reclaim-report.md) | 切片 AA 报告：§8.2 内核回收路径（`KernelActor`，无 `task_cap`） |
| 2026-09-24 | [2026-09-24-stage2-faulted-state-machine-report.md](2026-09-24-stage2-faulted-state-machine-report.md) | 切片 Y 报告：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001） |
| 2026-09-24 | [2026-09-24-stage2-exc-resume-report.md](2026-09-24-stage2-exc-resume-report.md) | 切片 AB 报告：§12.10 `exc_resume` 用户化（`KernelActor` 代调） |
| 2026-09-24 | [2026-09-24-stage2-audit-storage-refactor-report.md](2026-09-24-stage2-audit-storage-refactor-report.md) | 切片 AD 报告：审计存储重构（SPEC-075） |
| 2026-09-23 | [2026-09-23-stage2-image-frame-vector-report.md](2026-09-23-stage2-image-frame-vector-report.md) | 切片 W 报告：image Segment 的真实逐帧向量（ISSUE-FRAME-001） |
| 2026-09-17 | [2026-09-17-stage2-task-self-destroy-report.md](2026-09-17-stage2-task-self-destroy-report.md) | 切片 V 报告：Task 自毁与延迟帧归还（ISSUE-TASK-002） |
| 2026-09-17 | [2026-09-17-stage2-task-lifecycle-report.md](2026-09-17-stage2-task-lifecycle-report.md) | 切片 T 报告：Task create/yield/destroy 用户原语与单核调度 |
| 2026-09-17 | [2026-09-17-stage2-platform-cap-handover-report.md](2026-09-17-stage2-platform-cap-handover-report.md) | 切片 U 报告：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项） |
| 2026-09-16 | [2026-09-16-stage2-opening-report.md](2026-09-16-stage2-opening-report.md) | 切片 Q 报告：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语 |
| 2026-09-16 | [2026-09-16-stage2-ipc-user-surface-report.md](2026-09-16-stage2-ipc-user-surface-report.md) | 切片 S 报告：IPC 用户化与两 ring3 任务共享 Segment |
| 2026-09-16 | [2026-09-16-stage2-bringup-segment-migration-report.md](2026-09-16-stage2-bringup-segment-migration-report.md) | 切片 R 报告：bring-up 13 区域迁移为真实 Segment 对象 |
| 2026-08-16 | [2026-08-16-framekernel-gap-analysis.md](2026-08-16-framekernel-gap-analysis.md) | Prima OS × Framekernel 差距分析报告 |

## 阶段 1

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-18 | [2026-10-18-stage1-open-issues-spec151.md](2026-10-18-stage1-open-issues-spec151.md) | SPEC-151：阶段 1 遗留 Open ISSUE 整改（AUTH-003/004、AUDIT-007、CAP-004） |
| 2026-10-18 | [2026-10-18-stage1-4-implementation-review.md](2026-10-18-stage1-4-implementation-review.md) | 阶段 1–4 实现复核（阶段 5 输入） |
| 2026-10-17 | [2026-10-17-stage1-review-remediation-spec150.md](2026-10-17-stage1-review-remediation-spec150.md) | SPEC-150：阶段 1 补充复核与整改 |
| 2026-10-17 | [2026-10-17-stage1-exit-review-spec146.md](2026-10-17-stage1-exit-review-spec146.md) | SPEC-146：阶段 1 退出条件复核与签署 |
| 2026-09-16 | [2026-09-16-stage1-closure-report.md](2026-09-16-stage1-closure-report.md) | 切片 P 报告：阶段 1 退出条件——帧唯一归属与初始 Authority 物化 |

## 阶段 0 / 模型

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-17 | [2026-10-17-stage0-thirdparty-organizational-review-verdict.md](2026-10-17-stage0-thirdparty-organizational-review-verdict.md) | 阶段 0 第三方组织级复核结论（未通过） |
| 2026-10-17 | [2026-10-17-stage0-third-party-review-remediation.md](2026-10-17-stage0-third-party-review-remediation.md) | SPEC-137：阶段 0 范畴外复核发现项整改 |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec143.md](2026-10-17-stage0-review-remediation-spec143.md) | SPEC-143：阶段 0 第三方组织级复核发现项整改（29 项） |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec141.md](2026-10-17-stage0-review-remediation-spec141.md) | SPEC-141：阶段 0 第二轮组织级复核发现项整改（11 项） |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec140.md](2026-10-17-stage0-review-remediation-spec140.md) | SPEC-140：阶段 0 对抗式复核发现项整改（12 项） |
| 2026-10-17 | [2026-10-17-stage0-re-review-spec144.md](2026-10-17-stage0-re-review-spec144.md) | SPEC-144：SPEC-143 复核残修（3 项） |
| 2026-10-17 | [2026-10-17-stage0-organizational-review-remediation.md](2026-10-17-stage0-organizational-review-remediation.md) | SPEC-139：组织级阶段 0 审查发现项整改（9 项） |
| 2026-10-17 | [2026-10-17-stage0-frozen-re-review-spec145.md](2026-10-17-stage0-frozen-re-review-spec145.md) | SPEC-145：阶段 0 冻结基线复核与签署 |
| 2026-10-17 | [2026-10-17-stage0-4-gap-audit.md](2026-10-17-stage0-4-gap-audit.md) | 阶段 0–4 缺口复核包 |
| 2026-10-17 | [2026-10-17-s0x2-coverage-increment-doc007.md](2026-10-17-s0x2-coverage-increment-doc007.md) | SPEC-134：S0-X2 逐操作覆盖报告（DOC-007 增量复核） |
| 2026-10-14 | [2026-10-14-cosir-dual-personality-gap-analysis.md](2026-10-14-cosir-dual-personality-gap-analysis.md) | COSIR/POSIR 双人格底座差距分析 |
| 2026-10-14 | [2026-10-14-cosir-dual-personality-adoption-review.md](2026-10-14-cosir-dual-personality-adoption-review.md) | COSIR/POSIR 双人格扩展候选采纳审核 |
| 2026-10-13 | [2026-10-13-stage0-bootstrap-taskcreated-report.md](2026-10-13-stage0-bootstrap-taskcreated-report.md) | 阶段 0 / 内核 Audit 修复：bootstrap init Task 的创建事件（ISSUE-EXC-010） |
| 2026-10-13 | [2026-10-13-lean-cdt-invariant-report.md](2026-10-13-lean-cdt-invariant-report.md) | 形式化推进（参考 seL4）：Lean 模型的能力派生树（CDT）不变量与保持性证明 |
| 2026-10-13 | [2026-10-13-lean-cdt-forest-report.md](2026-10-13-lean-cdt-forest-report.md) | 形式化推进（参考 seL4）：Lean 证成能力派生树的森林性（无环） |
| 2026-10-13 | [2026-10-13-cosir-cdt-parent-report.md](2026-10-13-cosir-cdt-parent-report.md) | cosir 侧补能力派生树 `parent` 并建模 CDT（关 ISSUE-FORMAL-001） |
| 2026-10-12 | [2026-10-12-stage0-code-review.md](2026-10-12-stage0-code-review.md) | 阶段 0（语义基线）代码评审报告 |
| 2026-09-13 | [2026-09-13-spec058-lifecycle-unification.md](2026-09-13-spec058-lifecycle-unification.md) | SPEC-058 统一修订记录：对象生命周期 19 项 Open ISSUE |
| 2026-09-13 | [2026-09-13-s0x2-operation-coverage.md](2026-09-13-s0x2-operation-coverage.md) | S0-X2 逐操作覆盖报告（修订后复核） |
| 2026-09-13 | [2026-09-13-s0x1-independent-lifecycle-review.md](2026-09-13-s0x1-independent-lifecycle-review.md) | S0-X1 独立规格审查报告：对象生命周期 |
| 2026-09-13 | [2026-09-13-s0x1-fix-reverification.md](2026-09-13-s0x1-fix-reverification.md) | S0-X1 修复独立复核报告：SPEC-056 九项生命周期修复的对抗式再验证 |
| 2026-08-17 | [2026-08-17-capability-derivation-model-gap.md](2026-08-17-capability-derivation-model-gap.md) | Capability 派生 / 位置 / 溯源建模缺口备忘 |

## 跨阶段 / 双人格

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-14 | [2026-10-14-dual-personality-holder-model-h0-review.md](2026-10-14-dual-personality-holder-model-h0-review.md) | 双人格 Holder 模型 H0 评审报告 |

## 跨阶段 / 架构评估

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-17 | [2026-10-17-kernel-primitives-comparative-review.md](2026-10-17-kernel-primitives-comparative-review.md) | Prima OS 内核原语对照评估 |

## 跨阶段 / 兼容性

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-15 | [2026-09-15-linux-trace-compat-report.md](2026-09-15-linux-trace-compat-report.md) | 切片 P 报告：Linux trace 与有界差分（`cow-write-fault`） |

## 跨阶段 / 文档治理

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-08-13 | [2026-08-13-task-6-historical-artifact-consistency.md](2026-08-13-task-6-historical-artifact-consistency.md) | Task 6 Historical Artifact Consistency Report |

## 跨阶段 / 其他

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-13 | [2026-10-13-task-create-capability-audit-report.md](2026-10-13-task-create-capability-audit-report.md) | Task Capability 审计与派生链（ISSUE-EXC-011）+ 审计存储扩容 |
| 2026-10-13 | [2026-10-13-ipc-capability-move-audit-report.md](2026-10-13-ipc-capability-move-audit-report.md) | IPC 携带 Capability 的位置变化审计（ISSUE-EXC-012） |
| 2026-10-13 | [2026-10-13-exc-source-destroy-report.md](2026-10-13-exc-source-destroy-report.md) | 异常源销毁 `exc_source_destroy`（§12.6 补齐） |

- [POSIX std 后端：目标、ABI、标准入口及验证](2026-10-27-posix-std-backend-report.md)
