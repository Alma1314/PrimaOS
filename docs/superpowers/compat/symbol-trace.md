# 阶段 5 符号 ↔ 系统调用追溯（musl 1.2.5 / echo）

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

| Linux x86_64 | musl 名称 | Prima 动作 | E1–E6 是否执行 | 错误 |
|---:|---|---|---|---|
| 0 | `read` | 未实现 | 否（`stdin` 未链接） | `-ENOSYS` |
| 1 | `write` | fd 1/2 逐字节 `SYS_SEND` 到管理器 | stdio 实际走 20；直接调用时执行 | 非法 fd `-EBADF=9` |
| 3 | `close` | fd 1/2 成功；其余失败 | `fclose(stdout)` 会执行 | `-EBADF=9` |
| 8 | `lseek` | 未实现 | 否 | `-ENOSYS` |
| 9 | `mmap` | 未实现（echo 的 TLS 用 `builtin_tls`） | 否；mallocng 仅链接 | `-ENOSYS` |
| 11 | `munmap` | 未实现 | 否 | `-ENOSYS` |
| 12 | `brk` | 未实现 | 否；mallocng 仅链接 | `-ENOSYS` |
| 16 | `ioctl` | 恒失败，触发 `stdout` 全缓冲 | 首次 stdout 写 | `-ENOSYS` |
| 20 | `writev` | 逐 iovec 走 fd 1/2 字节流 | 是 | 非法 fd `-EBADF=9` |
| 28 | `madvise` | 未实现 | 否 | `-ENOSYS` |
| 60 | `exit` | 上报状态 + 自毁 | 正常终止 | 不返回 |
| 158 | `arch_prctl` | `SET_FS` → `SYS_TASK_SET_FS_BASE=38`；`GET_FS` 读回 | 是 | `-EINVAL=22` |
| 202 | `futex` | 未实现 | 否（单线程无竞争） | `-ENOSYS` |
| 218 | `set_tid_address` | 返回 `SYS_TASK_ID` | 是 | `-ENOSYS` 不适用 |
| 231 | `exit_group` | 上报状态 + 自毁 | 正常终止 | 不返回 |
| 7 / 271 / 2 | `poll`/`ppoll`/`open` | 未实现 | 否（非 secure auxv 早退） | `-ENOSYS` |

## 5. 未决依赖与归属

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
