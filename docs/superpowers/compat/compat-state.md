# 阶段 5 兼容状态台账（M0）

**日期**：2026-10-18。**性质**：M0 依赖盘点的状态记录，不是新的 ABI 承诺；M3/M3b 补充了
musl 基座与 `malloc/free` 证据，M4 首切片补充了单文件 fd 生命周期与 `printf` 输出证据。
冻结 profile 仍是 [profile v0](../specs/2026-10-18-stage5-profile-v0.md)；M4 把
`P5-IO-02`/`P5-FD-01`/`P5-FS-01`/`P5-FS-02`/`P5-LIB-01` 推进到 `partial`；M5 的
sbase `echo` E1–E6 把 `P5-LIB-01` 的冻结符号集推进到 `supported`（见
[M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)），但**没有**勾选 S5-D*/S5-X*。`malloc/free` 仍只代表 mallocng 在 64 页 arena 上工作，
不代表通用 `mmap`；文件支持只覆盖固定的单文件 `f`，不代表 VFS/目录。

符号级 `UNTEST` 沿用候选规格的术语：musl 会把它链接进来，但本阶段不承诺其行为。`UNTEST`
不进入 profile 支持表。

## 1. 冻结 profile v0 行（M5 更新 P5-LIB-01）

| ID | 当前状态 | M0 结论 |
|---|---|---|
| P5-ABI-01 | supported | 不变；`SYS_ABI_CHECK=37` 已见证 |
| P5-BOOT-01 | partial | 固定探针形态仍成立；任意 argv/envp 与完整 auxv 未实现。M0 发现 echo 的 E1–E6 需要按用例注入 argv，且完整 auxv 是通用 libc 的前置 |
| P5-PROC-01 | partial | 固定探针的返回码已见证；musl 的 `exit_group` 需要新的退出观察接线 |
| P5-IO-01 | partial | fd 1/2 仍逐字节 tty 流；M4 另支持 fd 3..6 的 `write`（经存储服务，3 字节分片）。无短写/EPIPE/非阻塞 |
| P5-PROC-02 | planned | `getpid` 未进入本片 |
| P5-IO-02 | partial | **M4**：fd 3..6 的 `read` 可用（EOF/短读由服务端容量决定）；tty `read` 与 `readv` 未实现 |
| P5-FD-01 | partial | **M4**：fd 3..6 的 open/close 生命周期、句柄失效与复用；无 `dup`、无 `CLOEXEC` 语义、无进程退出自动关闭 |
| P5-FS-01 | partial | **M4**：`openat(AT_FDCWD, "f", ...)` 单文件、单层 ≤3 字节路径、Namespace `f` 授权；无目录/多级路径/其他 `O_*` |
| P5-FS-02 | partial | **M4**：`lseek` 的 SET/CUR/END、服务端 offset、每次 open 独立偏移；无跨句柄共享 offset |
| P5-MM-01 | planned | echo 的 TLS 预计走 `builtin_tls`，不需要 `mmap`；mallocng 仅链接、不执行 |
| P5-TIME-01 | planned | 不在 echo 闭包 |
| P5-THREAD-01 | unsupported | 不变；单线程锁在 `libc.need_locks=0` 时为空操作 |
| P5-NET-01 | unsupported | 不变 |
| P5-LIB-01 | supported | **M5**：sbase `echo` E1–E6 双轮验收了 `strcmp`、`fputs`、`fputc`/`putc`、`putchar`、`fflush`、`fclose(stdout)`、`ferror` 与 `exit`；M4 的 `printf`+`fflush`、`snprintf`/`getenv`/errno、`malloc/free/realloc` 仍成立。stdio **文件** API（`fopen`/`fread`/`fwrite`）未验收 |

## 2. P5-LIB-01 符号级状态（M0 盘点）

sbase `echo` 在锁定 commit 上的对象级未解析符号（`nm -u`，`-O2 -std=c99 -fno-stack-protector`）：

- 正常路径（E1–E6）：`strcmp`、`fputs`、`fputc`、`putc`、`fflush`、`fclose`、`ferror`；
  `putchar` 在 GCC/Clang `-O2` 下会降为 `putc`，两者都必须可用。
- 错误路径（`fshut` 的 `weprintf`，正常用例不触发）：`fprintf`、`vfprintf`、`perror`、`strlen`、`strncmp`。
- 终止：`exit`（`__libc_start_main` 正常路径总会调用）。
- 应用自身：`putword`、`fshut`、`weprintf`；`argv0` 与 `putword` 的 `static first` 位于可写段。

| 符号 | 状态 | 说明 |
|---|---|---|
| `strcmp` | supported | M5 E1–E6 的 `-n`/`--` 判定经 Prima QEMU 双轮验证 |
| `fputs` | supported | `fwrite` → `__fwritex` → `stdout`；M5 E3–E6 输出经 tty |
| `fputc` / `putc` | supported | 词间空格由 `putword` 的 `fputc` 输出；M5 E5 验证 |
| `putchar` | supported | 换行由 `putchar` 输出；M5 E1/E3/E5/E6 验证 |
| `fflush` | supported | `fshut` → `fflush` → `writev`；M5 每个用例经 `fclose`/退出前冲刷 |
| `fclose` | supported | `fshut` → `fflush` + `fclose(stdout)`；M5 六个用例均返回 0 |
| `ferror` | supported | `fshut` 的错误检查；M5 成功路径返回 0 |
| `fprintf` | UNTEST | 仅错误路径；v0 行未列，需 v1 登记 |
| `vfprintf` | UNTEST | 同上；链接闭包远大于 v0 行 |
| `perror` | UNTEST | 仅错误路径 |
| `strlen` / `strncmp` | UNTEST | 纯函数；`fputs`/`xvprintf` 引用 |
| `exit` | supported | M5 六个用例经 musl `exit` 退出码 0，管理器与内核双重观察 |

**v1 提案**：`putc` 与 `exit` 已由 M5 升为 `supported`；`fprintf`、`vfprintf`、`perror`、
`strlen`、`strncmp` 仍为 `UNTEST`（只被 `fshut` 的错误路径链接，成功路径未执行），不得对外
宣称可用。P5-IO-01 的 Prima 后端仍注记“stdio 使用 `writev`”。

## 3. 已确认可复用 / 必须新建 / 不支持（M0）

| 类别 | 结论 |
|---|---|
| 可复用（用户态现有机制） | fd 1/2 的逐字节 `SYS_SEND` 到服务管理器、管理器转发 tty；`SYS_TASK_ID`；`SYS_TASK_DESTROY`；私有可写 `PT_LOAD` |
| 用户态适配（无新内核机制） | `writev`/`write`/`close(1/2)`/`ioctl` 的 Linux 号映射；`set_tid_address` → `SYS_TASK_ID`；`exit(60)`/`exit_group(231)` → 退出消息 + 自毁；`__syscall4..6` 全参数透传；`brk`/匿名 `mmap`/`mprotect`/`munmap` → 预映射 arena 的 bump 分配（partial，见 M3b）；**M4**：fd 3..6 表与 `openat/read/write/lseek/close` 翻译、3 字节分片、Namespace `f` 授权、存储服务单文件后端；`__stdio_*`、`printf`/`fflush` 等 musl 路径 |
| 需新增内核机制（已落地 M2/M3） | `arch_prctl(ARCH_SET_FS)` → `SYS_TASK_SET_FS_BASE=38`，per-task FS base；M3 另启用 SSE（`CR4.OSFXSR/OSXMMEXCPT`）与 per-task FXSAVE/FXRSTOR |
| musl 静态基座（M3/M3b/M4 已落地） | 锁定快照 + 两个可重放补丁；Prima crt 接 `__libc_start_main`；静态 `libc.a`/sysroot；`snprintf`/`getenv`/errno、`malloc/free/realloc`、`printf`+`fflush` 已验；C 任务由服务管理器预映射 64 页匿名 arena（与栈同段） |
| 明确不支持（本片） | 线程/`clone`、futex 等待、信号、`fork/execve/wait4`、网络、动态链接、通用 `mmap`/文件映射、目录/多级路径、`dup`、`O_APPEND` |
| 仅链接、不执行（E1–E6） | mallocng `calloc`、futex/`__wait`（`libc.need_locks=0`）、`poll`/`ppoll`（非 secure auxv 早退）；`open`/`lseek` 已由 M4 适配层承接，但不在 echo 闭包 |

## 4. 非承诺

- 本文件不表示 musl 已完整移植、stdio 已可用或 echo 已通过 E1–E6。M4 只覆盖单文件
  `f` 的 fd 生命周期与 `printf`+`fflush` 输出；stdio 文件 API、线程/信号/网络未验收。
- 匿名 `mmap` 是 64 页用户态 arena 的 partial 适配：无逐映射回收、无权限变更、
  无文件/共享映射，单次上限 48 页；不得据此宣称通用 `mmap`。
- 文件服务是**单文件字节流**：路径 `f`、单层 ≤3 字节、4 个句柄、服务端 offset；
  无目录/枚举/多级路径/`dup`/`CLOEXEC`/`O_APPEND`/进程退出自动关闭。管理器只做
  Namespace 授权与路由，不是 VFS。
- M4 有新的 QEMU 运行证据（双轮 `cargo xtask test`）。既有 h/i/xyz 字节流保持；
  容量与调度事件的变化（`ipc_ready` direct/blocks、`task_ready`/`TASKCAP` caps、
  `namespace_ready` entries/binds/resolves、`CAPACITY` 与 `state_audit_ready` 数值）
  逐条登记在 [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) §4，属真实计数
  增长，不是断言放宽。`frames_ready` 的 image/free 数值随内核镜像变化。
- SSP/fortify 在 Prima 构建中预计关闭（避免 `AT_RANDOM` 与 glibc 专有 `*_chk` 符号）；
  这不是安全承诺，需在 M3 中作为已知边界登记。
