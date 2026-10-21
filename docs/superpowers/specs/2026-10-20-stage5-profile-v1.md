# 阶段 5 POSIX profile v1（delta，M8/M9 提案）

**日期**：2026-10-21（M8 于 2026-10-20 提交，M9 追加元数据/工作目录行）
**状态**：**提案**。v0 仍是当前冻结契约；本文件只列出 M8/M9 需要变更/新增的行，
供组织级评审后发布为 v1。S5-D1 的组织级确认仍未完成，因此本文件不自行勾选任何
S5-D*/S5-X*。

**与 v0 的关系**：不删除、不重编号 v0 的任何行；`P5-FS-01`/`P5-FD-01`/`P5-FS-02`
只更新 partial 边界与证据，新增 `P5-FS-03`/`P5-FS-04`/`P5-FS-05`。未列出的行继续按
v0 执行。

## 1. 变更行

| ID | API / 契约 | 状态 | v1 边界与证据 |
|---|---|---|---|
| P5-FS-01 | `openat(dirfd, path, flags, mode)` | partial | **M8/M9**：绝对/相对路径、多级目录、`.`/`..`、重复/尾随 `/`、`dirfd` 起始目录；**M9** 起 `AT_FDCWD` 的相对路径从当前 cwd 解析（cwd 由 `chdir` 维护，初始 `/`）；`O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY`；错误 `ENOENT`/`ENOTDIR`/`EACCES`/`EEXIST`/`EINVAL`/`ENOSPC`。无符号链接、`O_NOFOLLOW`、`mkdir`/`unlink`/`rename`、`fchdir`；名称 ≤16 B、路径 ≤64 B、分量 ≤6。静态 rootfs 只有 `/tmp` 可新建普通文件。证据：[M8 报告](../reports/2026-10-20-stage5-m8-user-vfs.md) V1 双轮与 [M9 报告](../reports/2026-10-21-stage5-m9-metadata-cwd.md) M1/L1–L3 |
| P5-FS-02 | `lseek(fd, off, whence)` | partial | **M8**：普通文件 SET/CUR/END；目录 fd 的 `lseek` 用于重置枚举游标；`O_APPEND` 在服务端每次写前定位 EOF。无 `pread`/`pwrite`，无跨 open 共享；`dup` 共享仍成立 |
| P5-FS-03 | `getdents64(fd, dirp, count)` | partial | **M8**：目录枚举返回 `linux_dirent64` 子集，含 `.`/`..`、`d_ino`（节点+1）、`d_type`（DT_DIR=4/DT_REG=8/DT_CHR=2）与 8 字节对齐 `d_reclen`；非目录 `ENOTDIR`；缓冲不足 `EINVAL`；`d_off` 不是稳定 cookie，`telldir`/`seekdir` 未验收。M9 起经 musl `opendir`/`readdir` 的 `ls` 路径端到端验收。证据同上 |
| P5-FS-04 | `stat`/`lstat`/`fstat`/`fstatat`（x86_64 musl 经 kstat 4/5/6/262；`statx(332)` 亦实现） | partial | **M9**：x86_64 musl 的 stat 家族走 144 B kstat 布局，同一字段集也用于 256 B `statx(332)` 布局：`stx_mask=STATX_BASIC_STATS`、`stx_blksize=4096`、`stx_nlink`（目录 2、其余 1）、`stx_ino`（节点+1）、`stx_size`、`stx_blocks`、`stx_mode`（目录 0555/0777、普通文件 0444/0666、`/dev/null` 为 `S_IFCHR\|0666`）；接受 `AT_NO_AUTOMOUNT`（忽略）、`AT_SYMLINK_NOFOLLOW`（无符号链接，等价）与 `AT_EMPTY_PATH`（按 fd 取元数据）；其它 flag `EINVAL`。时间戳、`st_dev`/`st_rdev` 恒 0，`stx_attributes` 未实现；不实现 `statx` 的 sync/mount 语义。证据同下 |
| P5-FS-05 | `chdir`/`getcwd` | partial | **M9**：每任务独立 cwd，初始 `/`；`chdir` 只接受存在的目录（文件→`ENOTDIR`），成功后保存**词法规范化**的绝对路径（无符号链接，故 `..` 按字面回退，根处钳制）；`getcwd` 返回含 NUL 的长度，缓冲不足 `ERANGE`；无 `fchdir`、无 cwd 继承、无 `getcwd` 分配形式（`buf=NULL`）。证据同下 |
| P5-FD-01 | `close(fd)`、`dup/dup2/dup3` | partial | **M8**：本地 fd 0..18；fd 0=null、1/2=tty、file/dir 项指向服务端句柄 3..10（8 个并发）；`dup*` 共享服务端句柄/offset/目录游标，最后引用才关闭；`dup2`/`dup3` 目标接受 0..18；无 fd 0 的 tty 输入；表满 `EAGAIN`；无 exec，`O_CLOEXEC` 无语义；无进程退出自动关闭 |
| P5-IO-01 | `write(fd, buf, len)` | partial | **M8**：fd 0 写丢弃（返回长度）；fd 1/2 tty；fd 3..18 file/dir。目录写 `EBADF`；文件写经 VFS，3 字节分片；无短写/`EPIPE`/非阻塞 |
| P5-IO-02 | `read(fd, buf, len)` | partial | **M8**：fd 0 `/dev/null` 语义返回 EOF；fd 3..18 文件读；目录不暴露给 `read`（`EBADF`），只经 `P5-FS-03`；tty `read`/`readv` 未实现 |
| P5-BOOT-01 | C 入口与初始栈 | partial | **M8**：初始 fd 0 为 null、fd 1/2 为 tty；其余固定探针形态不变 |

## 2. 不变的行

`P5-ABI-01`、`P5-PROC-01`、`P5-PROC-02`、`P5-MM-01`、`P5-TIME-01`、`P5-THREAD-01`、
`P5-NET-01`、`P5-LIB-01` 按 v0；M8/M9 没有新增 `mmap`/线程/信号/网络能力，也没有升级
`P5-LIB-01` 的 stdio 文件 API（M9 的 `ls` 只经 `opendir`/`readdir` 与 `printf` 系列，
不执行 `fopen`/`fread`）。

## 3. 错误码

M9 新增 `ERANGE=34`（仅 `getcwd` 缓冲不足；POSIX 语义，v0 未登记）。其余沿用 v0 已冻结的
`ENOENT=2`、`EBADF=9`、`EAGAIN=11`、`EACCES=13`、`EFAULT=14`、`EEXIST=17`、`ENOTDIR=20`、
`EINVAL=22`、`ENOSPC=28`、`ENOSYS=38`。
未冻结的 `EISDIR=21`、`ENAMETOOLONG=36`、`ELOOP=40`、`ESPIPE=29`、`EMFILE=24` 不出现。

## 4. 冻结用例（M8/M9）

| # | 场景 | Linux 参考 | Prima |
|---|---|---|---|
| V1 | `stage5_vfs.c` 全项 | 退出 0，stdout `VFS|ok=1;...` | 退出 0，stdout 逐字节一致；双轮 QEMU |
| M1 | `stage5_meta.c` 的 statx/chdir/getcwd 全项 | 退出 0，stdout `META|ok=1;...` | 退出 0，stdout 逐字节一致；双轮 QEMU |
| C1 | sbase `cat etc/motd` | 退出 0，stdout `welcome\n` | 逐字节一致；双轮 QEMU |
| L1 | sbase `ls etc` | 退出 0，stdout `hostname\nmotd\n` | 逐字节一致；双轮 QEMU |
| L2 | sbase `ls .` | 退出 0，stdout `README\ndev\netc\nf\ntmp\nusr\n` | 逐字节一致；双轮 QEMU |
| L3 | sbase `ls etc/motd` | 退出 0，stdout `etc/motd\n` | 逐字节一致；双轮 QEMU |

Linux 参考在一次性 cwd 中布置同形树（`etc/motd`、`etc/hostname`、`usr/lib/version`、
`README`、`f`、`tmp`、`dev`）并设置与 Prima 相同的 0444/0555/0666/0777 权限，
`stdin=/dev/null`；Prima 侧用 rootfs 的同一相对路径。`/dev/null` 字符设备只在 Prima 侧
存在（宿主非 root 不能 `mknod`），故 `M1` 的 `null` 位在 Linux 侧按“不适用”保留 1 并在
报告中登记。绝对路径解析另由 V1 的 `/etc/motd` 覆盖。

## 5. 本提案不代表

- 不表示 VFS 完整：无 `mkdir`/`rmdir`/`unlink`/`rename`、符号链接、`fchdir`、`statfs`、
  持久化、文件映射或并发写锁。
- 不表示时间戳、`st_dev`/`st_rdev`、`statx` mount/sync 字段可用。
- 不表示动态链接、`fork/execve/wait4`、线程、信号、网络、通用 `mmap` 可用。
- 不勾选 S5-D3/S5-X1–X3；S5-D1 仍需组织级确认。
