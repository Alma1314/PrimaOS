# 阶段 5 符号 ↔ 系统调用追溯（musl 1.2.5 / echo）

> 阅读范围：§1–4 是最初 echo/musl 链接闭包及当时状态；§5 是 M20 后继调用路径汇总。早期 planned/UNTEST 不自动覆盖后继报告的验收，也不由本文整理升级为完整兼容。当前范围见[兼容台账](compat-state.md)与[M21](../reports/2026-10-23-stage5-m21-exit-review.md)。

**日期**：2026-10-18。**快照**：[musl-snapshot.lock.toml](musl-snapshot.lock.toml)
（commit `0784374d561435f7c787a555aeab8ede699ed298`）。**目标程序**：sbase `echo`
（profile v0 锁定的 `c546c3a…`）。

本表覆盖锁定 sbase `echo` 的首版链接闭包。`执行` 列区分 `E1–E6` 正常路径、仅错误路径与
仅链接。`状态` 是 M0 的**未承诺**标记，不是 profile 状态升级；`UNTEST` 不进入支持表。

## 1. 应用与 stdio 公开符号

| 符号 | musl 源 | 内部路径 | Linux syscall/后端 | Prima 机制 | 执行 | 状态 | 测试 ID |
|---|---|---|---|---|---|---|---|
| `strcmp` | `src/string/strcmp.c` | 纯循环 | 无 | 无 | E1–E6 | planned | T-MUSL-04 |
| `fputs` | `src/stdio/fputs.c` | `strlen` → `fwrite` → `__fwritex` → `f->write` | `writev` | 适配层逐 iovec 走 tty IPC | E3–E6 | planned | T-MUSL-04 |
| `fputc` | `src/stdio/fputc.c` | `do_putc` → `putc_unlocked`/`__overflow` | `writev`（换行/满缓冲） | 同上 | E3–E6 | planned | T-MUSL-04 |
| `putc` | `src/stdio/putc.h`（内联） | 同上；`-O2` 可把 `putchar` 降为 `putc` | `writev` | 同上 | E1–E6 | planned | T-MUSL-04 |
| `putchar` | `src/stdio/putchar.c` | `do_putc(stdout)` | `writev` | 同上 | E1–E6 | planned | T-MUSL-04 |
| `fflush` | `src/stdio/fflush.c` | 缓冲非空 → `f->write`；`NULL` 走 `__ofl_lock` | `writev` | 同上；单线程锁为空操作 | E1–E6（flush） | planned | T-MUSL-04 |
| `fclose` | `src/stdio/fclose.c` | `fflush` → `__stdio_close`；`F_PERM` 提前返回 | `close` | 适配层 fd 1/2 返回 0；其余 `-EBADF` | E1–E6 | planned | T-MUSL-04 |
| `ferror` | `src/stdio/ferror.c` | `FLOCK` + `F_ERR` | 无 | 单线程锁空操作 | E1–E6 | planned | T-MUSL-04 |
| `fprintf` | `src/stdio/fprintf.c` | `vfprintf` | `writev` | 同 stdio；仅错误路径 | 错误路径 | UNTEST | T-MUSL-08 |
| `vfprintf` | `src/stdio/vfprintf.c` | 格式化 → `__fwritex`/`f->write` | `writev` | 同 stdio；未承诺格式集 | 错误路径 | UNTEST | T-MUSL-08 |
| `perror` | `src/stdio/perror.c` | `strerror` → `fprintf(stderr)` | `writev` | 同 stdio；`ENOSYS` 等未冻结路径可能被打印 | 错误路径 | UNTEST | T-MUSL-08 |
| `strlen` | `src/string/strlen.c` | 纯函数 | 无 | 无 | E1–E6 | UNTEST | T-MUSL-04 |
| `strncmp` | `src/string/strncmp.c` | 纯函数 | 无 | 无 | 错误路径 | UNTEST | T-MUSL-08 |
| `exit` | `src/exit/exit.c` | `__funcs_on_exit` → `__libc_exit_fini` → `__stdio_exit` → `_Exit` | `exit_group` | 适配层上报状态 + `SYS_TASK_DESTROY` | E1–E6 | UNTEST | T-MUSL-07 |

## 2. 启动、TLS 与退出

| 阶段 | musl 源 | 行为 | Linux syscall | Prima 机制 | 测试 ID |
|---|---|---|---|---|---|
| 入口 | `crt/x86_64/crt_arch.h` / `crt/crt1.c` | `_start` 取 `rsp` → `_start_c` → `__libc_start_main` | 无 | **Prima crt**：先保存入口五寄存器到 `.bss` 的 `prima_ctx`，再调用 `__libc_start_main`；不执行 `rcrt1` 的自重定位 | T-MUSL-03 |
| libc 初始化 | `src/env/__libc_start_main.c` | 解析 `envp`/auxv；`__init_tls`；`__init_ssp`；非 secure 早退 | `poll`/`ppoll`/`open`（仅 secure 分支） | auxv 由初始栈提供；缺省 0 走非 secure 早退 | T-MUSL-06 |
| TLS | `src/env/__init_tls.c` | `main_tls` 来自 `AT_PHDR` 的 `PT_TLS`；无 PT_TLS 时用 `builtin_tls`；`__init_tp` 设置 FS | `mmap`（仅当 `tls_size > builtin_tls`）、`set_tid_address`、`arch_prctl(ARCH_SET_FS)` | `builtin_tls`（echo 无 TLS）；`set_tid_address` → `SYS_TASK_ID`；`arch_prctl` → 新 `SYS_TASK_SET_FS_BASE=38` | T-MUSL-02 |
| errno | `src/errno/__errno_location.c` | `&__pthread_self()->errno_val` | 无 | 依赖 FS base；无独立 syscall | T-MUSL-02 |
| 退出 | `src/exit/_Exit.c` | `exit_group(status)`，失败再 `exit(status)` | `exit_group` / `exit` | 适配层 `SYS_SEND` 上报状态，再 `SYS_TASK_DESTROY(self)`；失败则阻塞 | T-MUSL-07 |

## 3. stdio 内部与适配点

| 内部符号 | musl 源 | 关键行为 | 适配要求 |
|---|---|---|---|
| `__stdio_write` | `src/stdio/__stdio_write.c` | 两个 iovec，`syscall(SYS_writev, fd, iov, iovcnt)`；短写会循环 | 适配层 `writev`：逐 iovec 发送，返回总字节；失败返回原始负 errno |
| `__stdout_write` | `src/stdio/__stdout_write.c` | 首次写调用 `ioctl(fd, TIOCGWINSZ)`；失败置 `lbf=-1` | `ioctl` 返回 `-ENOSYS`（`stdout` 转全缓冲，flush/exit 时写出） |
| `__stdio_close` | `src/stdio/__stdio_close.c` | `syscall(SYS_close, fd)` | fd 1/2 返回 0；其余 `-EBADF` |
| `__stdio_seek` | `src/stdio/__stdio_seek.c` | `__lseek(fd, …)` | E1–E6 不执行（`stdout` 无读缓冲）；返回 `-ENOSYS` 即可 |
| `__stdio_exit` | `src/stdio/__stdio_exit.c` | 遍历 open-file list，刷新缓冲 | 依赖 `fflush`/`writev`；`__ofl_lock` 单线程空操作 |
| `__lockfile` / `__lock` | `src/stdio/__lockfile.c` / `src/thread/__lock.c` | `libc.need_locks==0` 时 `__lock` 立即返回；`stdout`/`stderr` 的 `.lock=-1` 根本不调用 | 单线程下无 futex；不承诺多线程 |
| `free` / `calloc` | `src/malloc/free.c`、`src/malloc/libc_calloc.c` → mallocng | `fclose` 对非 `F_PERM` 文件、`atexit` 扩容会引用；`echo` 正常路径不调用 | 仅链接；mallocng 用 `brk`/`mmap`/`munmap`/`madvise`，适配层返回 `-ENOSYS`；不得宣称 malloc 可用 |
| `__wait` / `__wake` / `futex` | `src/thread/__wait.c`、`pthread_impl.h` | 仅在锁竞争时进入 | 仅链接；E1–E6 不执行 |

## 4. Linux 系统调用号 → Prima 动作（适配层内部）

Linux 号只存在于 musl/适配层；内核仍只认 Prima `SYS_*`。未列出者统一 `-ENOSYS`（38）。

> 本表是 M0 盘点时的快照；M4–M10 已把 fd、路径、时间与内存行推进到实现。内存行按 M10 更新，其余以 [profile v1](../specs/2026-10-20-stage5-profile-v1.md) 与各 M 报告为准。

| Linux x86_64 | musl 名称 | Prima 动作 | E1–E6 是否执行 | 错误 |
|---:|---|---|---|---|
| 0 | `read` | 未实现 | 否（`stdin` 未链接） | `-ENOSYS` |
| 1 | `write` | fd 1/2 逐字节 `SYS_SEND` 到管理器 | stdio 实际走 20；直接调用时执行 | 非法 fd `-EBADF=9` |
| 3 | `close` | fd 1/2 成功；其余失败 | `fclose(stdout)` 会执行 | `-EBADF=9` |
| 8 | `lseek` | 未实现 | 否 | `-ENOSYS` |
| 9 | `mmap` | M10：56 页匿名 arena 内按页 first-fit 分配/清零（`MAP_PRIVATE\|MAP_ANONYMOUS`） | 否；A1 与 mallocng 执行 | `EINVAL`/`ENOMEM`/`EACCES`（`PROT_EXEC`） |
| 10 | `mprotect` | M10：`SYS_MEMSEG_PROTECT=41` 改逻辑访问集与叶 PTE 可写位；`PROT_NONE` 近似只读 | 否；A1 执行 | `EINVAL`/`ENOMEM`/`EACCES`（`PROT_EXEC`） |
| 11 | `munmap` | M10：真正 `SYS_MEMSEG_UNMAP` 拆 PTE，VA 可复用 | 否；mallocng `free` 执行 | `EINVAL` |
| 12 | `brk` | M3b：40 页 brk 区指针推进（不拆映射） | 否；mallocng 元数据执行 | 超界返回当前 brk |
| 83 | `mkdir` | M11：`mkdirat(AT_FDCWD, ...)` → MUTATE/PATH/COMMIT | 否；D1 执行 | `EEXIST`/`ENOENT`/`ENOTDIR`/`EACCES`/`ENOSPC` |
| 84 | `rmdir` | M11：`unlinkat(AT_FDCWD, ..., AT_REMOVEDIR)` | 否；D1 执行 | `ENOENT`/`ENOTDIR`/`EACCES` |
| 87 | `unlink` | M11：`unlinkat(AT_FDCWD, ..., 0)` | 否；D1 执行 | `ENOENT`/`EACCES` |
| 258 | `mkdirat` | M11：MUTATE/PATH/COMMIT，`dirfd` 起始目录 | 否；D1 执行 | 同 `mkdir` |
| 263 | `unlinkat` | M11：`AT_REMOVEDIR` 选择 rmdir；其它 flags `EINVAL` | 否；D1 执行 | 同 `unlink`/`rmdir` |
| 16 | `ioctl` | 恒失败，触发 `stdout` 全缓冲 | 首次 stdout 写 | `-ENOSYS` |
| 20 | `writev` | 逐 iovec 走 fd 1/2 字节流 | 是 | 非法 fd `-EBADF=9` |
| 28 | `madvise` | 未实现 | 否 | `-ENOSYS` |
| 60 | `exit` | 上报状态 + 自毁 | 正常终止 | 不返回 |
| 158 | `arch_prctl` | `SET_FS` → `SYS_TASK_SET_FS_BASE=38`；`GET_FS` 读回 | 是 | `-EINVAL=22` |
| 202 | `futex` | 未实现 | 否（单线程无竞争） | `-ENOSYS` |
| 218 | `set_tid_address` | 返回 `SYS_TASK_ID` | 是 | `-ENOSYS` 不适用 |
| 231 | `exit_group` | 上报状态 + 自毁 | 正常终止 | 不返回 |
| 7 / 271 / 2 | `poll`/`ppoll`/`open` | 未实现 | 否（非 secure auxv 早退） | `-ENOSYS` |

## 历史基线：未决依赖与归属（M0）

| 依赖 | 归属 | 测试 ID |
|---|---|---|
| `SYS_TASK_SET_FS_BASE=38` + per-task FS base 保存/恢复 | M2 内核机制（暂称 M2-TLS） | T-MUSL-02 |
| Prima crt（保存入口寄存器、跳过 `rcrt1`）+ 静态 PIE 重定位核验 | M2/M3 crt | T-MUSL-03 |
| C 适配层 `__syscall0..6`（原始负 errno）与 `writev`/`close`/`ioctl` | M2 适配层 | T-MUSL-01/04 |
| `exit_group` 退出观察与管理器回收 | M2 + 服务管理器 | T-MUSL-07 |
| 完整 auxv（`AT_PHDR/PHNUM/PHENT/ENTRY/BASE/EXECFN/...`） | P5-BOOT-01 | T-MUSL-06 |
| E1–E6 的 argv 注入 | P5-BOOT-01（manifest args blob 候选） | T-MUSL-04 |
| profile v1（符号清单、`writev` 注记、机制登记） | 组织级复核 | — |

**负责人**：M2/M3 实现由主 agent 本人执行（不使用实现子 agent）；组织级独立复核尚未安排，
不得由实现者自签。每个测试 ID 在 M2/M3 落地时必须附 Linux/Prima 两端的实际命令与结果。


## 5. 阶段 5 收尾探针的符号/系统调用追溯（W1–W9）

| 探针 | 使用的 libc/符号 | Linux syscall | Prima 机制 | 证据 |
|---|---|---|---|---|
| W1/W2 wait object | `syscall`（Prima 原生直发）/`prima_native` | 无（Prima 42–47） | `SYS_WAIT_*`/`SYS_THREAD_CREATE` | M15 报告 |
| W3 futex | musl `pthread` + 适配层 futex | 202 | 适配层静态 futex 表 + WaitObject | M16a 报告 |
| W4 pthread | musl `pthread_create/join`、`clone` | 56 | `SYS_CLONE(48)`（clone.s 补丁）、`SYS_THREAD_EXIT(49)`、`SYS_THREAD_CLEAR_TID(50)` | M16b 报告 |
| W6 fork/wait | 内联 asm `syscall` 51 / `wait4` 61 | 57/61（Linux 参考） | `SYS_FORK(51)`、`SYS_WAIT4(53)`、`SYS_PROCESS_EXIT(54)` | M17b-4 报告 |
| W7 exec | 内联镜像 + `SYS_EXEC_REPLACE` | 无（Prima 55） | `SYS_EXEC_REPLACE(55)` | M17c-3 报告 |
| W7b VFS exec | `open/read/close` + fork/exec | 2/0/3/57 | 适配层 VFS + `SYS_EXEC_REPLACE(55)` | M17c-4 报告 |
| W8 kill | `kill` 62 | 62 | `SYS_SIGNAL(56)` | M18a 报告 |
| W9 sigaction | `sigaction`/`kill`（Linux 参考） | 13/62 | `SYS_SIGACTION(57)`、`SYS_SIGRETURN(58)` | M18b 报告 |

上表记录 M17 原始路径；`syscall(57)` 包装损坏及原生 51 绕行的发现保留在 M17b-3/4。
当前 COW/TLB 和 musl 子退出整改见[fork 后继报告](../reports/2026-10-24-fork-continuation-report.md)，
实际验收范围以该报告为准，不能把原始绕行证据作为 libc fork 支持。

## 6. 2026-10-24 musl 边界与 fd/stdio 后继

本节补强前述历史表，不把未实现 syscall 或原生机制探针当成 libc 符号支持。工作契约和实际运行范围见[设计](../specs/2026-10-24-musl-port-boundary-design.md)、[报告](../reports/2026-10-24-musl-port-boundary-report.md)。

| 符号/路径 | musl 实现或本地替换 | Linux 号 | Prima 后端 | 验证 |
|---|---|---|---|---|
| `fcntl` | src/fcntl/fcntl.c → __syscall3 | 72 | C fd 表的 flags/dup；GETFL 读共享打开状态 | F1 + host adapter |
| `fdopen` | 替换 src/stdio/__fdopen.c → GETFL/SETFL/SETFD → stdio 读写 | 72/19/20/3 | fd 表 + VFS；append 切换失败传播并释放 FILE | F1 Linux/Prima；Prima 专有失败分支 |
| `fopen("a")` | src/stdio/fopen.c → open → __fdopen | 2/72/20 | 服务端已有 append 写入语义 | F1 内容 abcZefBC |
| `readv/writev` | src/unistd/readv.c/writev.c → syscall_cp → adapter prima_fd_iov | 19/20 | 相同 fd 的分段读写；先预校验长度 | F1 + host 边界 |
| `__syscall_cp_asm` | 替换 src/thread/x86_64/syscall_cp.s | n+完整六参数 | __syscall6 → prima_translate | host stub 检查每个参数；F1 真实调用 |
| `vfork` | 替换 src/process/x86_64/vfork.s | 不陷入 | __syscall_ret(-ENOSYS) | F1 Prima 负例 |
| `__unmapself` | 替换 src/thread/x86_64/__unmapself.s | 60（只交适配层） | 正常 thread exit / clear_child_tid；保留映射 | 归档检查；不宣称 detach 端到端 |
| `__restore_rt` | 替换 src/signal/x86_64/restore.s | 无 | 原生 58 | 归档检查；不宣称 libc sigaction 端到端 |

全 libc.a 的直接 syscall 只允许 clone.lo 的原生 48/49 与 restore.lo 的原生 58。syscall_arch.h、TLS 与 clone 旧补丁及本次五个新增替换文件共八项，清单由 musl-snapshot.lock.toml 驱动。`build.py --check` 在输入匹配、缓存完整时反汇编检查产物；构建成功只代表静态基座，不代表所有可链接符号已支持。

## 7. fork C 续执行整改后继

| 调用路径 | Linux 号 | Prima 边界 | 验证入口与限制 |
|---|---:|---|---|
| musl `fork()` / `syscall()` / 原始 C `__syscall0` | 57 | SYS_FORK 51 → COW提交/同步TLB → 子返回0/父返回id | W6三路径；[报告](../reports/2026-10-24-fork-continuation-report.md)列实际结果 |
| musl `waitpid` / `syscall(wait4)` | 61 | SYS_WAIT4 53 → 四字节int / 跨页+COW写回 | 只支持pid>0或-1、options=0/WNOHANG；无rusage/完整故障恢复 |
| fork子 musl `_exit` / exit_group | 231/60 | 私有child标记 → SYS_PROCESS_EXIT 54 | 不发送继承fd的CLOSE；共享cap/close隔离仍未解决 |
| W7/W7b/W8 的 fork | 57 | musl fork → 同一内核路径 | 撤去内联51；exec镜像及默认信号仅按原探针范围解释 |

未增加 libc 补丁或放宽 musl 归档原生陷入白名单；信号掩码、完整 pthread_atfork/多线程 fork、
任意原始51调用后的libc退出、子VFS授权、CLOEXEC及完整POSIX生命周期均不能由本节外推。


## 8. 2026-10-25 工作增量调用链

[契约](../specs/2026-10-25-stage5-gap-closure-design.md)与[实跑范围](../reports/2026-10-25-stage5-gap-closure-report.md)分别解释：

| 符号/入口 | 输入边界 | 实际调用链 | 限制 |
|---|---|---|---|
| 托管C main/_exit/exit_group | Linux60/231进入适配层 | CLOSE本地fd → 原生SEND17报告 → RECV18停驻 → manager TASK_DESTROY20 → 段回收 | fork子走Process exit54；不误用父self cap |
| A2 main(argc,argv,envp) | 版本化参数载体 | ring3按实际块大小构栈 → task_create入口 → musl启动 | 固定边界集，无新增syscall |
| fork/clone | 原生51/48 | register_clone_task → 保存当前父FPU → 子槽复制 | MXCSR/x87控制字见证，非完整向量寄存器机器证明 |
| Rust R1/R2 _start | Prima五寄存器入口、16对齐初始RSP | 汇编call建立C ABI对齐 → rust_start → Runtime TASK_ID7/SEND17/YIELD8/RECV18 | 不经过Linux libc号表；托管退出由manager销毁 |
| Rust std::fs::metadata | restricted std | unsupported PAL → ErrorKind::Unsupported | 无VFS调用，不能称为已支持std::fs |
| Rust std::alloc::System | restricted std | Prima PAL → NULL/OOM | 实际Vec/Box由探针自带GlobalAlloc供给；默认System不支持 |
