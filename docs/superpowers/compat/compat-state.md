# 阶段 5 兼容状态台账（M0）

**日期**：2026-10-18。**性质**：M0 依赖盘点的状态记录，不是新的 ABI 承诺；M3 与 M3b
完成后补充了 musl 基座与 `malloc/free` 证据，但不改变任何 profile 行的支持状态。冻结
profile 仍是 [profile v0](../specs/2026-10-18-stage5-profile-v0.md)；**没有**把任何 API 从
`planned` 升为 `supported`/`partial`，也没有勾选 S5-D*/S5-X*。`malloc/free` 的通过只代表
mallocng 在 64 页用户态 arena 上工作，不代表通用 `mmap` 支持。

符号级 `UNTEST` 沿用候选规格的术语：musl 会把它链接进来，但本阶段不承诺其行为。`UNTEST`
不进入 profile 支持表。

## 1. 冻结 profile v0 行（未改状态）

| ID | 当前状态 | M0 结论 |
|---|---|---|
| P5-ABI-01 | supported | 不变；`SYS_ABI_CHECK=37` 已见证 |
| P5-BOOT-01 | partial | 固定探针形态仍成立；任意 argv/envp 与完整 auxv 未实现。M0 发现 echo 的 E1–E6 需要按用例注入 argv，且完整 auxv 是通用 libc 的前置 |
| P5-PROC-01 | partial | 固定探针的返回码已见证；musl 的 `exit_group` 需要新的退出观察接线 |
| P5-IO-01 | partial | `write(1/2)` 仍受“非法 fd 静默”边界限制；musl stdio 实际走 `writev`，适配层必须实现 |
| P5-PROC-02 | planned | `getpid` 未进入本片 |
| P5-IO-02 | planned | `read`/`readv` 不在 echo 闭包；未实现 |
| P5-FD-01 | planned | `close` 只在适配层对 fd 1/2 返回成功；不是通用 fd 生命周期 |
| P5-FS-01/02 | planned | 不在 echo 闭包 |
| P5-MM-01 | planned | echo 的 TLS 预计走 `builtin_tls`，不需要 `mmap`；mallocng 仅链接、不执行 |
| P5-TIME-01 | planned | 不在 echo 闭包 |
| P5-THREAD-01 | unsupported | 不变；单线程锁在 `libc.need_locks=0` 时为空操作 |
| P5-NET-01 | unsupported | 不变 |
| P5-LIB-01 | planned | v0 行的符号清单不完整；见第 2 节与 v1 提案。M3/M3b 已用静态 musl 跑通 `snprintf`/`getenv`/errno、`malloc/free/realloc` 与 `exit`，但 stdio 输出路径未验收，不升级状态 |

## 2. P5-LIB-01 符号级状态（M0 盘点）

sbase `echo` 在锁定 commit 上的对象级未解析符号（`nm -u`，`-O2 -std=c99 -fno-stack-protector`）：

- 正常路径（E1–E6）：`strcmp`、`fputs`、`fputc`、`putc`、`fflush`、`fclose`、`ferror`；
  `putchar` 在 GCC/Clang `-O2` 下会降为 `putc`，两者都必须可用。
- 错误路径（`fshut` 的 `weprintf`，正常用例不触发）：`fprintf`、`vfprintf`、`perror`、`strlen`、`strncmp`。
- 终止：`exit`（`__libc_start_main` 正常路径总会调用）。
- 应用自身：`putword`、`fshut`、`weprintf`；`argv0` 与 `putword` 的 `static first` 位于可写段。

| 符号 | 状态 | 说明 |
|---|---|---|
| `strcmp` | planned | 纯函数，无 syscall；仍未有 Prima QEMU 证据 |
| `fputs` | planned | `fwrite` → `__fwritex` → `stdout` 的 `write` |
| `fputc` / `putc` | planned | `putc.h` → `__overflow`（换行/满缓冲时）/ 缓冲写入 |
| `putchar` | planned | 编译器可降为 `putc`；两符号都要提供 |
| `fflush` | planned | 缓冲非空时调用 `f->write` |
| `fclose` | planned | `fflush` + `__stdio_close`（fd 1/2 必须返回成功）；`F_PERM` 提前返回 |
| `ferror` | planned | 单线程下锁为空操作 |
| `fprintf` | UNTEST | 仅错误路径；v0 行未列，需 v1 登记 |
| `vfprintf` | UNTEST | 同上；链接闭包远大于 v0 行 |
| `perror` | UNTEST | 仅错误路径 |
| `strlen` / `strncmp` | UNTEST | 纯函数；`fputs`/`xvprintf` 引用 |
| `exit` | UNTEST | 正常终止路径；`exit_group` 适配是 P5-PROC-01 的前置 |

**v1 提案**：把 `putc`、`fprintf`、`vfprintf`、`perror`、`strlen`、`strncmp`、`exit` 增列为
`planned`/`UNTEST`，并把 P5-IO-01 的 Prima 后端注记为“stdio 使用 `writev`”。在本文件发布
v1 前，上述符号不得对外宣称可用。

## 3. 已确认可复用 / 必须新建 / 不支持（M0）

| 类别 | 结论 |
|---|---|
| 可复用（用户态现有机制） | fd 1/2 的逐字节 `SYS_SEND` 到服务管理器、管理器转发 tty；`SYS_TASK_ID`；`SYS_TASK_DESTROY`；私有可写 `PT_LOAD` |
| 用户态适配（无新内核机制） | `writev`/`write`/`close(1/2)`/`ioctl` 的 Linux 号映射；`set_tid_address` → `SYS_TASK_ID`；`exit(60)`/`exit_group(231)` → 退出消息 + 自毁；`__syscall4..6` 全参数透传（取消点适配层路径就绪）；`brk`/匿名 `mmap`/`mprotect`/`munmap` → 预映射 arena 的 bump 分配（partial，见 M3b）；`__stdio_*`、`fputs`/`fputc`/`fflush` 等 musl 路径 |
| 需新增内核机制（已落地 M2/M3） | `arch_prctl(ARCH_SET_FS)` → `SYS_TASK_SET_FS_BASE=38`，per-task FS base；M3 另启用 SSE（`CR4.OSFXSR/OSXMMEXCPT`）与 per-task FXSAVE/FXRSTOR |
| musl 静态基座（M3/M3b 已落地） | 锁定快照 + 两个可重放补丁（`syscall_arch.h`、`__set_thread_area.s`）；Prima crt 接 `__libc_start_main`；静态 `libc.a`/sysroot 由 `prima-user/musl/build.sh` 构建；`snprintf`/`getenv`/errno 已验；C 任务由服务管理器预映射 64 页匿名 arena（与栈同段），`malloc/free/realloc` 已验 |
| 明确不支持（本片） | 线程/`clone`、futex 等待、信号、`fork/execve/wait4`、网络、动态链接、通用 `mmap`/文件映射 |
| 仅链接、不执行（E1–E6） | mallocng `free`/`calloc`（`brk`/`mmap`/`munmap`/`madvise`）、futex/`__wait`（`libc.need_locks=0`）、`lseek`（无读缓冲）、`poll`/`ppoll`/`open`（非 secure auxv 早退） |

## 4. 非承诺

- 本文件不表示 musl 已完整移植、stdio 已可用或 echo 已通过 E1–E6。M3/M3b 只覆盖静态
  启动、`exit`、`snprintf`/`getenv`/errno、`malloc/free/realloc` 与既有适配层调用集；
  stdio 输出路径、线程/信号/网络未验收。
- 匿名 `mmap` 是 64 页用户态 arena 的 partial 适配：无逐映射回收、无权限变更、
  无文件/共享映射，单次上限 48 页；不得据此宣称通用 `mmap`。
- M3/M3b 有新的 QEMU 运行证据（双轮 `cargo xtask test`），但既有 h/i/xyz 字节流与
  容量事件逐字节未变；这不是 profile 升级证据。`frames_ready` 的 image/free 数值随
  内核重构变化，属真实帧数变化，不是事件语义变化。
- SSP/fortify 在 Prima 构建中预计关闭（避免 `AT_RANDOM` 与 glibc 专有 `*_chk` 符号）；
  这不是安全承诺，需在 M3 中作为已知边界登记。
