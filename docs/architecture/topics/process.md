---
id: topic-process
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-31
---

# 进程、线程、fork、exec 与信号

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

Process 与 Task/Thread 如何关联？pthread join、fork/wait、exec_replace 与信号的最小路径在哪里？为什么 fork 子进程直接 execve 仍受限？

检索别名：Process, Thread, ShareGroup, pthread, clone, fork, execve, wait4, signal, sigaction, COW。

## 权威章节与阅读顺序

原语 Task/Capability 基线；M15/M17 工作设计；M17c-2 exec_replace 契约；阶段5退出条件。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

M16b-1/2 的 clone/join 阻塞 → M16b-3 clear_child_tid 与 W4/W5。M17b-2/3 的 COW/wait 不完整 → M17b-4 W6。M17c-1 解析 → c-2 契约 → c-3 内存 exec → c-4 父预读 VFS 镜像后子 exec。M18a 默认终止 → M18b handler/sigreturn；后者只补强 handler 范围。

S6-M2 的[按需 ELF 输入契约](../../superpowers/specs/2026-10-31-stage6-streamed-elf-input.md)
与[修复/失败证据](../../superpowers/reports/2026-10-31-stage6-streamed-elf-input.md)关闭
原生 exec 的 1 MiB 输入及首个 4 KiB 程序头限制；稀疏输入仅载入两页段数据，
不能替代 busybox 的 526 页装载、Gate 或 Linux LibOS。

## 契约、计划与证据入口

- [2026-10-23-stage5-m15-process-thread-wait-design.md](../../superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)
- [2026-10-23-stage5-m17-fork-exec-design.md](../../superpowers/specs/2026-10-23-stage5-m17-fork-exec-design.md)
- [2026-10-23-stage5-m17c2-exec-syscall.md](../../superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md)
- [2026-10-22-stage5-completion-roadmap.md](../../superpowers/plans/2026-10-22-stage5-completion-roadmap.md)
- [2026-10-23-stage5-m16b1-clone-basis.md](../../superpowers/reports/2026-10-23-stage5-m16b1-clone-basis.md)
- [2026-10-23-stage5-m16b2-musl-clone.md](../../superpowers/reports/2026-10-23-stage5-m16b2-musl-clone.md)
- [2026-10-23-stage5-m16b3-pthread-join.md](../../superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)
- [2026-10-23-stage5-m17b2-cow-fork.md](../../superpowers/reports/2026-10-23-stage5-m17b2-cow-fork.md)
- [2026-10-23-stage5-m17b3-zombie-wait.md](../../superpowers/reports/2026-10-23-stage5-m17b3-zombie-wait.md)
- [2026-10-23-stage5-m17b4-fork-wait.md](../../superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)
- [2026-10-23-stage5-m17c3-exec.md](../../superpowers/reports/2026-10-23-stage5-m17c3-exec.md)
- [2026-10-23-stage5-m17c4-execve-vfs.md](../../superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md)
- [2026-10-23-stage5-m18a-signal-kill.md](../../superpowers/reports/2026-10-23-stage5-m18a-signal-kill.md)
- [2026-10-23-stage5-m18b-sigaction.md](../../superpowers/reports/2026-10-23-stage5-m18b-sigaction.md)
- [2026-10-23-stage5-m21-exit-review.md](../../superpowers/reports/2026-10-23-stage5-m21-exit-review.md)

## 源码定位

- [bootstrap.rs](../../../kernel/src/bootstrap.rs)
- [sched.rs](../../../kernel/src/sched.rs)
- [syscall.rs](../../../kernel/src/syscall.rs)
- [exec.rs](../../../kernel/src/exec.rs)
- [signal.rs](../../../kernel/src/signal.rs)
- [cow.rs](../../../kernel/src/cow.rs)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)
- [clone.s](../../../prima-user/musl/patches/clone.s)

## 测试与观察入口

- [stage5_pthread.c](../../../prima-user/c/stage5_pthread.c)
- [stage5_share.c](../../../prima-user/c/stage5_share.c)
- [stage5_proc.c](../../../prima-user/c/stage5_proc.c)
- [stage5_exec.c](../../../prima-user/c/stage5_exec.c)
- [stage5_execve.c](../../../prima-user/c/stage5_execve.c)
- [stage5_signal.c](../../../prima-user/c/stage5_signal.c)
- [stage5_sigaction.c](../../../prima-user/c/stage5_sigaction.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

原生 fork 绕行及续执行缺陷的发现链见 M17b-3/4；当前整改与实际通过范围见下方后继报告。W7b 不是子任务直接 execve(path) 完整闭环。共享 cap 空间导致 close 隔离偏差；信号掩码/EINTR/SA_RESTART/SIGCHLD 自动通知、pthread 取消/分离/属性未覆盖。不要把工作决定视为组织级确认。

## fork / 等待写回整改后继

[工作契约](../../superpowers/specs/2026-10-24-fork-continuation-design.md)与
[实际验证](../../superpowers/reports/2026-10-24-fork-continuation-report.md)接续 M17b 的续执行失败链：
clone_root_cow 的构造/写保护/同步刷新顺序，分配失败回收、NX 保留、wait_status_bytes 的四字节边界、
跨页/COW 输出以及 musl 子进程退出。`cargo xtask test-process [W6|W7|W7b|W8]` 使用原双轮验收。
没有新增抽象原语、SMP、fd close 隔离或子任务完整 VFS 授权；历史失败不删除。
