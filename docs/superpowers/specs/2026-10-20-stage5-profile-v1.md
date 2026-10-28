# 阶段 5 POSIX profile v1（delta，M8/M9/M10/M11/M12 提案）

**日期**：2026-10-22（M8 于 2026-10-20 提交，M9 追加元数据/工作目录行，M10 追加匿名内存/保护行，M11 追加文件树变更行，M12 追加启动栈边界，M13a 追加符号链接/rename 行，M13b 追加权限/stdio 行，M19 追加 loopback UDP 行，M14 追加 tty 输入/termios 行）
**状态**：**已并入 v2 冻结（2026-10-27，SPEC-152）**。本文件是历史 delta：其变更行由
[冻结 profile v2](2026-10-25-stage5-profile-v2.md) 解释，v0 仍是 supported 面的来源；
本文件不再单独发布，也不单独勾选 S5-D*/S5-X*。

**与 v0 的关系**：不删除、不重编号 v0 的任何行；`P5-FS-01`/`P5-FD-01`/`P5-FS-02`
只更新 partial 边界与证据，新增 `P5-FS-03`/`P5-FS-04`/`P5-FS-05`；M10 新增 `P5-MM-01` 的 partial 边界，M11 新增 `P5-FS-06` 的 partial 边界，M12 更新 `P5-BOOT-01` 的 partial 边界，M13a 新增 `P5-FS-07` 并更新 `P5-FS-01`/`03`/`04`/`05`/`06`，M13b 新增 `P5-FS-08`/`P5-LIB-02` 并更新 `P5-FS-04`/`P5-IO-02`，M19 新增 `P5-NET-01` 的 partial 边界，M14 新增 `P5-TTY-01` 并更新 `P5-IO-02`/`P5-FD-01`/`P5-BOOT-01`。未列出的行继续按
v0 执行。

## 1. 变更行

| ID | API / 契约 | 状态 | v1 边界与证据 |
|---|---|---|---|
| P5-FS-01 | `openat(dirfd, path, flags, mode)` | partial | **M8/M9**：绝对/相对路径、多级目录、`.`/`..`、重复/尾随 `/`、`dirfd` 起始目录；**M9** 起 `AT_FDCWD` 的相对路径从当前 cwd 解析（cwd 由 `chdir` 维护，初始 `/`）；`O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY`；错误 `ENOENT`/`ENOTDIR`/`EACCES`/`EEXIST`/`EINVAL`/`ENOSPC`。**M13a**：`open` 跟随末项符号链接（相对目标以链接父目录为基准，绝对目标从根，深度上限 8，超限 `ELOOP`）；仍无 `O_NOFOLLOW`，悬空链接上 `O_CREAT` 未实现为创建目标。名称 ≤16 B、路径 ≤64 B、分量 ≤6。静态 rootfs 只有 `/tmp` 可新建普通文件。证据：[M8 报告](../reports/2026-10-20-stage5-m8-user-vfs.md) V1 双轮、[M9 报告](../reports/2026-10-21-stage5-m9-metadata-cwd.md) M1/L1–L3、[M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md) V2 双轮 |
| P5-FS-02 | `lseek(fd, off, whence)` | partial | **M8**：普通文件 SET/CUR/END；目录 fd 的 `lseek` 用于重置枚举游标；`O_APPEND` 在服务端每次写前定位 EOF。无 `pread`/`pwrite`，无跨 open 共享；`dup` 共享仍成立 |
| P5-FS-03 | `getdents64(fd, dirp, count)` | partial | **M8**：目录枚举返回 `linux_dirent64` 子集，含 `.`/`..`、`d_ino`（节点+1）、`d_type`（DT_DIR=4/DT_REG=8/DT_CHR=2/DT_LNK=10）与 8 字节对齐 `d_reclen`；非目录 `ENOTDIR`；缓冲不足 `EINVAL`；`d_off` 不是稳定 cookie，`telldir`/`seekdir` 未验收。M9 起经 musl `opendir`/`readdir` 的 `ls` 路径端到端验收。证据同上 |
| P5-FS-04 | `stat`/`lstat`/`fstat`/`fstatat`（x86_64 musl 经 kstat 4/5/6/262；`statx(332)` 亦实现） | partial | **M9**：x86_64 musl 的 stat 家族走 144 B kstat 布局，同一字段集也用于 256 B `statx(332)` 布局：`stx_mask=STATX_BASIC_STATS`、`stx_blksize=4096`、`stx_nlink`（目录 2、其余 1）、`stx_ino`（节点+1）、`stx_size`、`stx_blocks`、`stx_mode`（**M13b**：由节点 mode 位生成 `ifmt\|(mode & 0777)`；初始 rootfs 仍为目录 0555/0777、普通文件 0444/0666、`/dev/null` `S_IFCHR\|0666`）；接受 `AT_NO_AUTOMOUNT`（忽略）、`AT_SYMLINK_NOFOLLOW` 与 `AT_EMPTY_PATH`（按 fd 取元数据）；**M13a**：`AT_SYMLINK_NOFOLLOW` 仅被接受但**仍跟随**末项（`lstat` 对符号链接返回目标元数据，不是链接自身）；**M13b**：mode 位来自节点存储值，chmod 后 stat 立即反映；其它 flag `EINVAL`。时间戳、`st_dev`/`st_rdev` 恒 0，`stx_attributes` 未实现；不实现 `statx` 的 sync/mount 语义。证据同下 |
| P5-FS-05 | `chdir`/`getcwd` | partial | **M9**：每任务独立 cwd，初始 `/`；`chdir` 只接受存在的目录（文件→`ENOTDIR`），成功后保存**词法规范化**的绝对路径（无符号链接，故 `..` 按字面回退，根处钳制）；`getcwd` 返回含 NUL 的长度，缓冲不足 `ERANGE`；**M13a**：`fchdir` 接受目录 fd 并恢复打开时记录的规范化绝对路径（非目录 `ENOTDIR`，路径不可得 `EINVAL`）；无 cwd 继承、无 `getcwd` 分配形式（`buf=NULL`）；`chdir("../..")` 仍是词法回退，不按链接父目录重解析。证据：[M9 报告](../reports/2026-10-21-stage5-m9-metadata-cwd.md)、[M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md) |
| P5-FS-07 | `rename`/`renameat`、`symlink`/`symlinkat`、`readlink`/`readlinkat`、`fchdir` | partial | **M13a**：同一 VFS 内 `rename` 文件/目录/链接；目标可替换文件/链接或**空**目录，非空目录/文件覆盖目录用 `EACCES`/`ENOTDIR`（冻结错误集无 `ENOTEMPTY`/`EISDIR`）；目录禁止移进自身子树（`EINVAL`）；符号链接目标存内容区（≤64 B），`readlink` 缓冲不足按 Linux 截断；`fchdir` 见 `P5-FS-05`；硬链接/`linkat` 未接线（`ENOSYS`）。证据：[M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md) V2 双轮与同源差分 |
| P5-LIB-02 | stdio 文件 API：`fopen`/`fread`/`fwrite`/`fseek`/`fclose` | partial | **M13b**：经 musl 实际执行（不是只链接）；`fopen("w+")` 写入/回读、`fopen("r")` 读取、0444 文件写打开拒绝；依赖适配层的 `readv` 拆分。无 `fdopen`/`freopen`/`setvbuf` 验收，无缓冲语义/错误注入承诺 |
| P5-TTY-01 | tty 行规程与 `termios` 子集 | partial | **M14**：fd 0 输入；canonical（行缓冲 64 B、`\n`/`\r`、`0x7f`/`0x08` 删除、回显）与 raw（直通、`ECHO` 可关）；空行 `Ctrl-D` EOF；`tcgetattr`/`tcsetattr`（`TCGETS`/`TCSETS`/`TCSETSW`/`TCSETSF`）与 `TIOCGWINSZ`（24×80）；单次读 ≤3 B。无 job control/`SIGINT`/pty/流控/`VMIN`/`VTIME`；有读者时管理器轮询 COM1（待 M15 WaitObject/IRQ 替换），无读者时阻塞 `RECV`。证据：[M14 报告](../reports/2026-10-23-stage5-m14-tty-input.md) I1 双轮 |
| P5-NET-01 | `socket`/`bind`/`getpeername`/`sendto`/`recvfrom`/`close` | partial | **M19**：仅 `AF_INET`/`SOCK_DGRAM`/loopback 127.0.0.1（或 bind 0.0.0.0 本机）；4 端点 × 4 条 64 B 数据报队列；`bind(0)` 领临时端口、`getsockname` 可查；源端口/零长度/较大报文/未绑定目标丢弃/`EADDRINUSE`/`EAFNOSUPPORT`/`EMSGSIZE`/关闭后 `EBADF` 均有同源证据。无 TCP、IPv6、`connect`/`listen`/`accept`、`poll`/非阻塞、`MSG_*`、分片重组、NIC/DNS。证据：[M19 报告](../reports/2026-10-23-stage5-m19-loopback-udp.md) N1 双轮与同源差分 |
| P5-FS-08 | `chmod`/`fchmod`/`fchmodat`/`umask` | partial | **M13b**：节点 mode 低 12 位（本切片只接受 0..0o777）；`chmod` 跟随末项链接，`fchmod` 按 fd 节点，`fchmodat` 仅 flags=0；`umask` 为进程级值（默认 022），在 `O_CREAT`/`mkdir` 成功后由适配层按 `mode & ~umask` 收紧；单用户模型只强制 owner 读写位，无 uid/gid/组/other/ACL/setuid/setgid/sticky。证据：[M13b 报告](../reports/2026-10-22-stage5-m13b-permissions-stdio.md) V3 双轮与同源差分 |
| P5-MM-01 | `mmap/munmap/mprotect` | partial | **M10**：仅 `MAP_PRIVATE|MAP_ANONYMOUS`、fd=-1、offset=0；56 页固定 arena 内按页 first-fit 分配、`munmap` 真正拆 PTE、复用地址时复映射；`mprotect` 经内核 `memseg_protect(41)` 改权，RW↔R 等收缩/恢复可用；初始 prot 生效。`PROT_NONE` 近似为只读（COSIR 访问集不可为空）；匿名 `PROT_EXEC` 返回 `EACCES`（W^X）；无文件映射/`MAP_SHARED`/`mremap`/`madvise`；`MAP_FIXED` 仅限 arena（brk guard 页例外），区外 `ENOMEM`；无硬件 NX。证据：[M10 报告](../reports/2026-10-21-stage5-m10-anon-memory.md) A1 双轮与 M10 设计 |
| P5-FS-06 | `mkdir`/`mkdirat`/`unlink`/`unlinkat`/`rmdir` | partial | **M11**：单路径创建空目录与删除文件/空目录；`AT_FDCWD` 相对路径与 cwd 合成，`dirfd` 形式以目录节点为起点；`AT_REMOVEDIR` 支持，其它 `unlinkat` flags 返回 `EINVAL`。**M13a**：节点表 32 槽、删除后槽位复用；删除的内容区由写入压力触发的压实回收；带打开句柄的节点拒绝删除（`EACCES`）；非空目录/目录 `unlink` 用 `EACCES` 代替 `ENOTEMPTY`/`EISDIR`；`mode`/`umask`/`chmod` 不建模。无硬链接、无跨启动持久化；`rename`/符号链接见 `P5-FS-07`。证据：[M11 报告](../reports/2026-10-22-stage5-m11-file-mutation.md) D1 双轮、[M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md) |
| P5-FD-01 | `close(fd)`、`dup/dup2/dup3` | partial | **M8**：本地 fd 0..18；fd 0=null、1/2=tty、file/dir 项指向服务端句柄 3..10（8 个并发）；`dup*` 共享服务端句柄/offset/目录游标，最后引用才关闭；`dup2`/`dup3` 目标接受 0..18；**M14**：fd 0 由 null 改为 tty 输入（canonical/raw/EOF），fd 1/2 仍为输出；表满 `EAGAIN`；无 exec；**M12b**：退出前自动关闭本地 fd 与最后一个服务端句柄，`O_CLOEXEC` 只在本地表记录（无 exec 语义）；存储服务退出后句柄计数为 0 |
| P5-PROC-02 | `getpid` / `getppid` | partial | **M7**：`getpid` 返回当前 Prima Task id，任务存活期间稳定；**M12b**：`getppid` 返回管理器标识，保证非零且不同于 `getpid`，不是正式父子关系；无 Process/Thread 区分、无进程组 | [M12 报告](../reports/2026-10-22-stage5-m12-c-startup.md) |
| P5-IO-01 | `write(fd, buf, len)` | partial | **M8**：fd 0 写丢弃（返回长度）；fd 1/2 tty；fd 3..18 file/dir。目录写 `EBADF`；文件写经 VFS，3 字节分片；无短写/`EPIPE`/非阻塞 |
| P5-IO-02 | `read(fd, buf, len)`、`readv` | partial | **M8**：fd 0 `/dev/null` 语义返回 EOF；fd 3..18 文件读；目录不暴露给 `read`（`EBADF`），只经 `P5-FS-03`；tty `read`/`readv` 未实现。**M13b**：`readv(19)` 在适配层按 iovec 顺序拆成同一 fd 读路径（短读即停），供 musl stdio 使用；**M14**：fd 0 为 tty 输入，canonical/raw 行规程、`Ctrl-D` EOF、回显与短读（≤3 B/次）已接线；无 `VMIN`/`VTIME`、流控、job control |
| P5-BOOT-01 | C 入口与初始栈 | partial | **M8**：初始 fd 0 为 null、fd 1/2 为 tty；**M14**：fd 0 改为 tty 输入，`termios` 子集与 QEMU 串口输入已接线；**M12a**：固定探针 envp 扩展为 `PRIMA_ABI=1`/`M12_ENV=one`/`M12_EMPTY=`，auxv 冻结 `AT_PAGESZ`/`AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/`AT_ENTRY`/`AT_BASE`/`AT_EXECFN`/`AT_NULL`，参数块支持 legacy 与版本化 argv/envp（16/8 项上限）；仍为固定 argv，独立参数载体触发 ISSUE-START-ARGS-001，无 `AT_RANDOM`。**M14**：初始 fd 0 为 tty 输入（非 null），输入/回显/EOF 见 `P5-TTY-01` |

## 2. 不变的行

`P5-ABI-01`、`P5-PROC-01`、`P5-PROC-02`、`P5-TIME-01`、`P5-THREAD-01`、
`P5-NET-01` 见下表 `partial`；`P5-LIB-01` 按 v0；M8/M9 没有新增 `mmap`/线程/信号/网络能力，也没有升级
`P5-LIB-01` 的 stdio 文件 API（M9 的 `ls` 只经 `opendir`/`readdir` 与 `printf` 系列，
不执行 `fopen`/`fread`）。

## 3. 错误码

M9 新增 `ERANGE=34`（仅 `getcwd` 缓冲不足；POSIX 语义，v0 未登记）。其余沿用 v0 已冻结的
`ENOENT=2`、`EBADF=9`、`EAGAIN=11`、`ENOMEM=12`（M10：mmap 区耗尽/mprotect 未映射）、`EACCES=13`、`EFAULT=14`、`EEXIST=17`、`ENOTDIR=20`、
`EINVAL=22`、`ENOSPC=28`、`ENOSYS=38`。
未冻结的 `EISDIR=21`、`ENOTEMPTY=39`、`ENAMETOOLONG=36`、`ELOOP=40`、`ESPIPE=29`、`EMFILE=24` 不出现；M11 把非空目录/目录 `unlink` 映射为 `EACCES`。

## 4. 冻结用例（M8/M9）

| # | 场景 | Linux 参考 | Prima |
|---|---|---|---|
| V1 | `stage5_vfs.c` 全项 | 退出 0，stdout `VFS|ok=1;...` | 退出 0，stdout 逐字节一致；双轮 QEMU |
| M1 | `stage5_meta.c` 的 statx/chdir/getcwd 全项 | 退出 0，stdout `META|ok=1;...` | 退出 0，stdout 逐字节一致；双轮 QEMU |
| C1 | sbase `cat etc/motd` | 退出 0，stdout `welcome\n` | 逐字节一致；双轮 QEMU |
| L1 | sbase `ls etc` | 退出 0，stdout `hostname\nmotd\n` | 逐字节一致；双轮 QEMU |
| L2 | sbase `ls .` | 退出 0，stdout `README\ndev\netc\nf\ntmp\nusr\n` | 逐字节一致；双轮 QEMU |
| L3 | sbase `ls etc/motd` | 退出 0，stdout `etc/motd\n` | 逐字节一致；双轮 QEMU |
| A1 | `stage5_mem.c` 的 mmap/munmap/mprotect 全项 | 退出 0，stdout `MEM|ok=1;...` | 逐字节一致（`exec`/`native` 为各平台冻结期望）；双轮 QEMU |
| D1 | `stage5_mut.c` 的 mkdir/rmdir/unlink 全项 | 退出 0，stdout `MUT|ok=1;...` | 逐字节一致（`nonempty`/`unlink_dir` 为各平台冻结期望）；双轮 QEMU |

Linux 参考在一次性 cwd 中布置同形树（`etc/motd`、`etc/hostname`、`usr/lib/version`、
`README`、`f`、`tmp`、`dev`）并设置与 Prima 相同的 0444/0555/0666/0777 权限，
`stdin=/dev/null`；Prima 侧用 rootfs 的同一相对路径。`/dev/null` 字符设备只在 Prima 侧
存在（宿主非 root 不能 `mknod`），故 `M1` 的 `null` 位在 Linux 侧按“不适用”保留 1 并在
报告中登记。绝对路径解析另由 V1 的 `/etc/motd` 覆盖。

## 5. 本提案不代表

- 不表示 VFS 完整：`mkdir`/`rmdir`/`unlink` 仅到单路径 partial；仍无 `rename`、符号链接、
  `fchdir`、`statfs`、持久化、文件映射或并发写锁。M11 之后的顺序见
  [阶段 5 收尾路线图](../plans/2026-10-22-stage5-completion-roadmap.md)。
- 不表示时间戳、`st_dev`/`st_rdev`、`statx` mount/sync 字段可用。
- 不表示动态链接、`fork/execve/wait4`、线程、信号、网络、完整 POSIX `mmap` 可用；`P5-MM-01` 只是 56 页匿名 arena 子集。
- 不表示完整文件系统：M11 只支持单路径 `mkdir`/`unlink`/`rmdir`，无 `rename`/`mv`、链接、权限位、内容区回收或跨启动持久化。
- 不勾选 S5-D3/S5-X1–X3；S5-D1 仍需组织级确认。
