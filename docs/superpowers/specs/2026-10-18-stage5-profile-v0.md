# 阶段 5 POSIX profile v0（冻结）

**状态**：**冻结 v0**（2026-10-18，独立技术复核后定稿）。这是阶段 5 的 API 准入契约；S5-D1 的组织级确认仍属阶段出口程序，未经组织级确认前不在跟踪页勾选。权威目标见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)，实施顺序见[修订计划](../plans/2026-10-18-stage5-revised-plan.md)。

**变更控制**：本文件的 API 行、参数、错误边界与状态定义一经冻结，修改必须发布 v1 并重新评审。实现状态可以在**不改变冻结边界**的前提下由 `planned` 推进到 `supported`/`partial`，但必须同时提交 Linux/Prima 差分证据与测试 ID；不得只改状态字。

## 1. 范围与不变量

- 阶段 5 只承诺**静态链接**的 C/POSIX 源码兼容；动态链接、`ld.so`、`PT_INTERP` 与共享库不在本 profile。
- 本 profile **不是** Linux ABI 仿真：POSIX 名称只出现在 libc/适配层，内核仍使用 Prima 原生 `SYS_*` 与 Capability。
- 已有调用号 **1–37 保留、只追加、空洞不复用**。新增内核可见调用从 38 起追加；用户态服务之间使用消息 tag，不占用 syscall 号。
- 不采用 DOC-002 的 `0x0xx` 号表、`NT_RUXOS_ABI`、`x86_64-ruxos-none`；原生 ABI 版本为 `(major<<32)|minor = 0x0000000100000000`，与 `prima-runtime::API_MAJOR=0` 的 Rust 源码版本无关。
- 未登记调用返回 `-ENOSYS`。错误采用 Linux x86_64 数值的二补码负值；libc 适配层负责转换为 `-1`/`errno`。已实现集合见第 2 节；`EBADF=9`、`EINTR=4`、`EPIPE=32` 的**数值已冻结但实现未承诺**，进入对应 API 时才落地。

## 2. BLK-003 裁决（冻结）

| 事项 | 裁决 |
|---|---|
| 调用号 | 保留 1–37；POSIX 首批实现优先走用户态服务，只有确实需要内核机制时才追加 38+ |
| 命名 | 内核/原生运行时用 Prima 命名（`SYS_*`、`Task`、`MemorySegment`）；POSIX 名称仅存在于 libc/适配层 |
| 错误码 | 已实现并冻结：`ENOENT=2`、`EAGAIN=11`、`EACCES=13`、`EFAULT=14`、`EEXIST=17`、`ENOTDIR=20`、`EINVAL=22`、`ENOSPC=28`、`ENOSYS=38`。数值已冻结、实现待落地：`EINTR=4`、`EBADF=9`、`EPIPE=32`。其余错误在进入 profile 前必须逐项登记，禁止用相邻码“近似” |
| C 入口 | 采用第 3 节的 Prima 入口契约；`argc/argv/envp/auxv` 由装载器写入初始栈，不伪造 Linux `_start` |
| 动态链接 | 归入阶段 7 候选；阶段 5 不支持，静态 ELF 中不得有 `PT_INTERP`/`DT_NEEDED`，证据须由 `readelf` 核验 |
| 退出观察 | 退出状态由服务管理器/父 Task 观察；本 profile 不承诺 `wait4` 或进程组 |

## 3. C 入口与初始栈（冻结）

- 内核在进入 CPL3 时只设置 `rsp`；探针的汇编 `_start` 自行把 `rsp` 移入 `r9` 再调用 C 入口。`r9` 是**探针内部约定**，不是内核 ABI。入口寄存器沿用现有五参数原生约定
  `rdi=self_cap, rsi=initial_cap0, rdx=initial_cap1, rcx=name, r8=peer_task_id`。
- 初始栈 16 字节对齐，布局：
  - `[0] = argc`；
  - `argv[0..argc]`，随后一个 NULL；
  - `envp[0..]`，随后一个 NULL；
  - `auxv`：`{AT_PAGESZ=6, 4096}`、`{AT_NULL=0, 0}`。
- 当前只冻结 **argc=2、argv=["stage5-probe","stack-ready"]、envp=["PRIMA_ABI=1"]** 的固定探针形态；
  完整 auxv 与任意 argv 仍为 `planned`；`PT_LOAD` 写段已按权限分别装载（见
  [PT_LOAD/W^X 报告](../reports/2026-10-18-stage5-ptload-wx.md)）。
- 初始 fd/Capability：仅 fd 1/2 的字节流（由服务管理器与 tty 服务提供）；无继承 fd 表，无
  `stdin`。
- 代码段按 W^X 只读映射；可写 `PT_LOAD` 建私有 `R|W` 段并按 vaddr 复制已重定位字节，
  BSS 清零。探针可使用可写全局变量（`.data`/`.bss`），但不得依赖共享映像页可写。

## 4. API 矩阵

状态定义：`supported` = 冻结边界内已实现并有重复 Linux/Prima 证据；`partial` = 仅列出的子集可用；
`planned` = 契约已冻结但未实现；`unsupported` = 阶段 5 明确不承诺。

| ID | API / 契约 | Linux 参考 | Prima 后端/授权 | 状态 | 冻结边界（partial 必读） | 测试/证据 |
|---|---|---|---|---|---|---|
| P5-ABI-01 | 原生 ABI 版本握手 | 不适用（原生行） | `SYS_ABI_CHECK=37` | supported | `major=1, minimum_minor=0, reserved=0`；版本 `0`、其它 major/minor、非零保留位均返回 `-EINVAL`；成功返回 `0x0000000100000000` | [ABI 报告](../reports/2026-10-18-stage5-native-abi-check.md) |
| P5-BOOT-01 | C 入口 `argc/argv/envp/auxv` | `_start` + auxv | 通用 ELF 装载 + 固定初始栈 | partial | 仅第 3 节固定探针形态；无完整 auxv、无任意 argv。`PT_LOAD` 写段/私有 BSS 与 W^X 已实现（每映像最多 4 个写段） | [初始栈](../reports/2026-10-18-stage5-c-start-stack.md)、[PT_LOAD/W^X](../reports/2026-10-18-stage5-ptload-wx.md) |
| P5-PROC-01 | `main` 返回码与 `_exit` 观察 | `exit_group(status)` | Task 自毁 + 管理器观察 | partial | 仅返回码低 8 位；无 Process/zombie/父子等待/信号终止状态 | [同源返回码](../reports/2026-10-18-stage5-main-return-status.md)、[tty/exit](../reports/2026-10-18-stage5-c-tty-write.md) |
| P5-PROC-02 | `getpid` | 稳定 PID | 可映射 `SYS_TASK_ID` | planned | 尚未冻结 PID 稳定性、复用与错误边界 | P5-PROC-02 |
| P5-IO-01 | `write(fd, buf, len)` | 返回写入字节数；`EBADF/EFAULT/EPIPE` | tty 字节流（fd 1/2）；存储服务（fd 3..6） | partial | fd 1/2：逐字节 IPC、不限长度、非法 fd 拒绝。**M4**：fd 3..6 经存储服务写，单条 IPC 3 字节、由适配层分片；无短写/`EPIPE`/非阻塞 | [tty 报告](../reports/2026-10-18-stage5-c-tty-write.md)、[M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) |
| P5-IO-02 | `read(fd, buf, len)` | 阻塞/EOF/中断 | 存储服务（fd 3..6） | partial | **M4**：fd 3..6 读，EOF 与短读由服务端容量决定；tty `read`/`readv`/阻塞/EINTR 未实现 | [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) |
| P5-FD-01 | `close(fd)` | 释放 fd；`EBADF` | 用户态 fd 表 + 存储服务句柄 | partial | **M4**：fd 3..6 的 open/close、失效句柄 `EBADF`、关闭后句柄复用；无 `dup`、无 `CLOEXEC`、无进程退出自动关闭 | [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) |
| P5-FS-01 | `openat(dirfd, path, flags, mode)` | 路径解析与错误码 | 存储服务；Namespace `f` 仅作授权入口 | partial | **M4**：`AT_FDCWD`、唯一文件名 `f`（≤3 字节、无 `/`）、access/CREAT/TRUNC；无目录/枚举/多级路径/`O_APPEND` | [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) |
| P5-FS-02 | `lseek(fd, off, whence)` | 偏移与不可 seek 错误 | 服务端共享偏移 | partial | **M4**：SET/CUR/END、服务端 offset、每次 open 独立；无跨句柄共享 offset | [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) |
| P5-MM-01 | `mmap/munmap/mprotect` | 匿名/文件映射、flags | `MemorySegment` + 页表 | planned | 仅承诺匿名子集；装载器已按 `PT_LOAD` 做 W^X，但 `mmap` 三路径未实现；文件映射不支持 | P5-MM-01 |
| P5-TIME-01 | `clock_gettime/nanosleep` | CLOCK_MONOTONIC 等 | PIT 自由运行计数（`SYS_TIME_ENABLE=39`/`SYS_TIME_TICKS=40`）+ 用户态轮询 | partial | **M6**：仅 `CLOCK_MONOTONIC` 可查询；10 ms 分辨率；`nanosleep`/`clock_nanosleep` 为轮询实现，无 `EINTR`、`rem` 不写回；`CLOCK_REALTIME` 查询返回 `EINVAL`；无 `clock_getres`/`clock_settime`/`gettimeofday`。相邻 tick 读取间隔须 <55 ms | [M6 报告](../reports/2026-10-19-stage5-m6-time.md) |
| P5-THREAD-01 | `pthread_create/join`、mutex/cond | POSIX 线程 | 无 Thread/TLS/WaitObject | unsupported | 阶段 5 不承诺；禁止用成功空实现占位 | P5-THREAD-01 |
| P5-NET-01 | `socket/bind/sendto/recvfrom` | BSD socket | 无网络服务 | unsupported | 阶段 5 不承诺；loopback IPC 不算网络 | P5-NET-01 |
| P5-LIB-01 | `strcmp`、`fputs`、`fputc`、`putchar`、`fflush`、`fclose`、`ferror` | musl/stdio | 静态 musl 基座（M3/M4/M5） | supported | **M5**：上述冻结符号由 sbase `echo` E1–E6 双轮验收（含 `fclose(stdout)` 与 `exit`）；stdio **文件** API（`fopen`/`fread`/`fwrite`）仍不在本行 | [musl 计划](../plans/2026-10-18-musl-port-plan.md)、[M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md) |

## 5. 目标程序冻结：sbase `echo`

| 项 | 冻结值 |
|---|---|
| 上游 | `https://git.suckless.org/sbase` |
| commit | `c546c3a5724c81cee9a11d816a38ccdf17472129`（master） |
| 许可 | MIT；`LICENSE` SHA-256 `eb0b719b3327af6c4b480cd21f7944e944a5ddf571c04572eb8d025f52e056c6` |
| 参与文件 | `echo.c` `117c10e0e5a5139a80490673a3f14dd1dfcc45856adb84a83b28e3b301f1b48c`；`libutil/putword.c` `5c1c875678f3367489cb9fcf5e76db51f51236710ae3bfcac245ff9640ecff5c`；`libutil/fshut.c` `a5ee307130cfbe8aaace3528f370ef42f0ef16b135e957084f3a5fa42b883773`；`libutil/eprintf.c` `9547578e4e4585bbc8429b580c2db409328214efd383c5be01b0bb9017f1bf1c`；`util.h` `341a70a1f9d449f28c4d65bb594898e2d6cf2eabc21da018abdf024b7f71e7d6`；`arg.h` `5f4ecca1bb057a6d6301aff8f2b403e723af5dc86a5e00e8c7e5c2f56cdb9c79`；`compat.h` `00fc3129294c2ddf3d39dee3b6a2f581d98b565f66250796deabf82d2fe78e07` |
| 构建 | 阶段 5 用锁定 musl sysroot **静态链接**；不链接宿主 glibc；补丁须可从该 commit 重放 |
| 依赖闭包 | `echo` 需要 `argc/argv`、`strcmp`、stdio（`fputs`/`fputc`/`putchar`/`fflush`/`fclose`/`ferror`）、`write(1)` 与退出状态；对应 P5-BOOT-01/P5-IO-01/P5-PROC-01/P5-LIB-01 |
| 已知阻塞 | 可写 `.data`/`.bss` 的装载阻塞已解除；M5 已用冻结源码重编译并双轮通过 E1–E6。剩余边界是完整 auxv/任意 argv（P5-BOOT-01 仍为 `partial`，当前 argv 上限 8 项/32 字节）与 stdio 文件 API |
| 锁定状态 | 目标已选定并锁定；S5-X3 **证据已具备**（2026-10-19：E1–E6 双轮 QEMU PASS，见 [M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)），待组织级复核后勾选 |

### 冻结用例（Linux 参考已用锁定源码实测）

| # | 参数 | 期望 stdout（hex） | 期望退出码 |
|---|---|---|---|
| E1 | （无参数） | `0a` | 0 |
| E2 | `-n` | （空） | 0 |
| E3 | `hello` | `68656c6c6f0a` | 0 |
| E4 | `-n hello` | `68656c6c6f` | 0 |
| E5 | `a b c` | `61206220630a` | 0 |
| E6 | `-- -n` | `2d2d202d6e0a` | 0 |

上表由锁定 commit 在 Linux 宿主以 `cc -O2 -std=c99 echo.c libutil/{putword,fshut,eprintf}.c -I.` 实测。
Prima 侧须逐字节比较 stdout 与退出码，并另测 `write` 的坏指针/非法 fd 失败路径（当前
P5-IO-01 仍为 partial，见矩阵）。

**ID 说明**：片 C 加入 tty 服务后 C 探针由 task 16 改为 17；早期报告中的 16 是历史见证，
当前 QEMU 校验一律使用 17。

## 6. 本冻结不代表的结论

- 不勾选 S5-D2/D3/D6/X1–X3；不表示 musl 已完成、文件系统/线程/网络可用。
- 不把自写 C 探针当作开源程序验收；`echo` 必须在 Prima 上重新编译并通过 E1–E6 才算 S5-X3。
- 动态链接、`fork/execve/wait4`、完整信号与 Rust `std` 全量覆盖仍待各自契约。
