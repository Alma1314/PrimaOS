# 阶段 5 / M17：fork/execve/wait4 设计（D3 提案）

**日期**：2026-10-23
**状态**：设计提案（未实现）。D3 未组织级关闭；本文件给出 M17 的实现契约与
工作决定，按路线图“先 spec 后实现”推进。
**范围**：单线程 `fork`（COW 地址空间、fd 表继承、cap 继承）、静态 ELF
`execve`（argv/envp、`CLOEXEC`）、`wait4`/`waitpid`（状态、阻塞/非阻塞、
`ECHILD`）。不做进程组/session、`ptrace`、`vfork`、`posix_spawn`、动态链接。

## 1. 决策与工作假设

| 项 | 工作决定 |
|---|---|
| 进程模型 | `fork` 的子 Task 拥有**新 Process**（用于 wait/退出状态），地址空间为新 COW root；子 Task 的 **Capability 空间沿用父进程的 leader**（`space_of` 不变），即父子的 cap 句柄共享 |
| fd 表 | 子进程的适配层 fd 表是父表的 **COW 副本**（地址空间克隆）；表内的 cap 句柄因能力空间共享而可用；`CLOEXEC` 位随 fd 表复制 |
| fork 限制 | 调用进程有多线程（`process_thread_count > 1`）→ `EAGAIN` |
| exec | 静态 ELF64：解析 `PT_LOAD`，用 `memseg_*` 在**当前**地址空间替换映射（先拆旧、后建新），重建初始栈（argc/argv/envp/auxv），入口 = `e_entry`；保留未设 `CLOEXEC` 的 fd |
| 新 syscall（D2） | `SYS_FORK=51`、`SYS_EXECVE=52`、`SYS_WAIT4=53`；适配层把 Linux 57/59/61 映射到它们 |
| 退出状态 | Process 记录 `exit_status`；子进程退出唤醒等待的父进程；`wait4` 返回 pid/status，已回收 → `ECHILD` |

**已知偏差（必须登记）**：共享 cap 空间意味着“子进程 close 一个 fd”会关闭父子
共享的服务句柄（POSIX 下 fds 各自独立、open file description 共享）。M17 的
fd 继承探针只验证“子进程可用继承的 fd”，close 隔离标为已知偏差，待 cap
继承方案（多 holder 句柄或 per-Process cap 空间）替代。

## 2. 内核对象增量

```text
Process {
    parent: Option<ProcessId>,
    exit_status: Option<i32>,
    waiters: [TaskId],
}

Task {
    ...,
    process: ProcessId,      // fork 子进程的新 Process
    cap_space: ProcessId,    // 能力空间 leader（fork 子沿用父 leader）
}
```

- `space_of(task)` 改为读 `cap_space`（线程继承同一值；fork 子进程显式设为
  父的 cap_space）；`task_is_thread` 改为 `process != cap_space` 的 leader 判定
  或显式 `kind` 字段，避免 fork 子被误判为线程。
- 进程表新增 `parent`/`exit_status`；`wait4` 在子进程未退出时把父 Task 置
  `BlockedWait` 并登记到子 Process 的 waiters；子进程退出时设置状态并唤醒
  waiters。

## 3. fork 的执行路径（内核）

1. 校验：调用者单线程（同 Process 的 thread_count == 1）；任务/进程/审计容量。
2. 新 Task id + 新 Process（parent=调用者的 Process）。
3. 新 root = `vspace::clone_root(current_root, new_root, map)`（COW：共享帧、
   写保护位与既有 COW 机制一致），新 Task 的 vspace 条目为调用者条目的副本。
4. 复制调用者的用户帧：子进程从 `fork` 返回 **0**，父进程返回子 pid；
   `SretFrame` 的 RIP 为 syscall 之后，RSP/CS/SS/FS base 复制。
5. 子进程的调度登记：root/kstack/frame；状态 Ready。
6. 审计 `TaskCreated`/`ProcessCreated`。

COW 复用：`clone_root` 已用于 `task_create`；本切片需补“父子同时写同一页”
的反例（各自私有帧、互不可见）与“只读共享帧不复制”的正例。

## 4. execve（用户态优先）

- 适配层读取 ELF 头/程序头（VFS 读），校验 `ET_EXEC`/`EM_X86_64`/静态；
- 用 `memseg_unmap` 拆旧映射（保留栈/参数区），`memseg_create`+`memseg_map`
  建新段（R/X、R/W），清零 `.bss`；
- 在用户栈顶构造 `argc/argv/envp/auxv`（AT_PHDR/AT_PAGESZ/AT_ENTRY 最小集）；
- 设置 FS base/TLS？静态 musl 需要 TLS 初始化：exec 后由 libc 的 `_start`
  自行初始化（`__init_tls` 使用 PT_TLS）；本阶段先支持“无 TLS 的静态探针”，
  再扩展 musl 程序；
- fd 表保留，`CLOEXEC` 的 fd 关闭；
- 若内核无法从用户态安全替换 root，则新增 `SYS_EXEC_REPLACE` 辅助（内核拆
  旧 vspace、映射新段）——设计上优先用户态 `memseg_*`，减少内核新面。

## 5. wait4

- `wait4(pid, status_ptr, options, rusage)`：pid=-1 或指定子进程；`WNOHANG`
  支持；无子进程 → `ECHILD`；
- 子进程已退出 → 立即返回并写 status（`WIFEXITED`/`WEXITSTATUS` 编码）；
- 未退出 → 父 Task `BlockedWait` 登记到子 Process waiters；子退出时唤醒；
- 子进程资源（root/页表/caps）在 wait 后回收；未 wait 的子进程保留为 zombie
  （记录状态），父退出时由内核回收。

## 6. 证据计划

- **Linux 同源**：`stage5_proc.c`（fork/wait/COW/退出码/ECHILD/CLOEXEC）在
  Linux 与 Prima 上逐字节比较；`stage5_exec.c`（argv/envp）在 Linux 与 Prima。
- **QEMU**：`W6` fork/wait/COW 双轮；`W7` exec/argv 双轮；多线程 fork 的
  `EAGAIN` 负例；fd 继承/CLOEXEC。
- **host**：bootstrap 状态机覆盖 Process 的 parent/exit_status/waiters、
  `wait4` 的阻塞/唤醒/ECHILD、fork 的容量/多线程负例。
- 报告登记“共享 cap 空间导致的 close 语义偏差”与未实现项。

## 7. 本设计不代表

- 不表示 fork/exec/wait 已实现；
- 不表示 D3 已关闭或 COSIR 已修订；
- 不表示阶段 5 退出条件满足。
