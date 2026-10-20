# 阶段 5 POSIX profile v1（delta，M8 提案）

**日期**：2026-10-20
**状态**：**提案**。v0 仍是当前冻结契约；本文件只列出 M8 需要变更/新增的行，
供组织级评审后发布为 v1。S5-D1 的组织级确认仍未完成，因此本文件不自行勾选任何
S5-D*/S5-X*。

**与 v0 的关系**：不删除、不重编号 v0 的任何行；`P5-FS-01`/`P5-FD-01`/`P5-FS-02`
只更新 partial 边界与证据，新增 `P5-FS-03`。未列出的行继续按 v0 执行。

## 1. 变更行

| ID | API / 契约 | 状态 | v1 边界与证据 |
|---|---|---|---|
| P5-FS-01 | `openat(dirfd, path, flags, mode)` | partial | **M8**：绝对/相对路径、多级目录、`.`/`..`、重复/尾随 `/`、`dirfd` 起始目录；`O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY`；错误 `ENOENT`/`ENOTDIR`/`EACCES`/`EEXIST`/`EINVAL`/`ENOSPC`。无符号链接、`O_NOFOLLOW`、`mkdir`/`unlink`/`rename`、cwd、`stat`；名称 ≤16 B、路径 ≤64 B、分量 ≤6。静态 rootfs 只有 `/tmp` 可新建普通文件。证据：[M8 报告](../reports/2026-10-20-stage5-m8-user-vfs.md) V1 双轮与 Linux 同源差分 |
| P5-FS-02 | `lseek(fd, off, whence)` | partial | **M8**：普通文件 SET/CUR/END；目录 fd 的 `lseek` 用于重置枚举游标；`O_APPEND` 在服务端每次写前定位 EOF。无 `pread`/`pwrite`，无跨 open 共享；`dup` 共享仍成立 |
| P5-FS-03 | `getdents64(fd, dirp, count)` | partial | **M8**：目录枚举返回 `linux_dirent64` 子集，含 `.`/`..`、`d_ino`（节点+1）、`d_type`（DT_DIR=4/DT_REG=8/DT_CHR=2）与 8 字节对齐 `d_reclen`；非目录 `ENOTDIR`；缓冲不足 `EINVAL`；`d_off` 不是稳定 cookie，`telldir`/`seekdir` 未验收。证据同上 |
| P5-FD-01 | `close(fd)`、`dup/dup2/dup3` | partial | **M8**：本地 fd 0..18；fd 0=null、1/2=tty、file/dir 项指向服务端句柄 3..10（8 个并发）；`dup*` 共享服务端句柄/offset/目录游标，最后引用才关闭；`dup2`/`dup3` 目标接受 0..18；无 fd 0 的 tty 输入；表满 `EAGAIN`；无 exec，`O_CLOEXEC` 无语义；无进程退出自动关闭 |
| P5-IO-01 | `write(fd, buf, len)` | partial | **M8**：fd 0 写丢弃（返回长度）；fd 1/2 tty；fd 3..18 file/dir。目录写 `EBADF`；文件写经 VFS，3 字节分片；无短写/`EPIPE`/非阻塞 |
| P5-IO-02 | `read(fd, buf, len)` | partial | **M8**：fd 0 `/dev/null` 语义返回 EOF；fd 3..18 文件读；目录不暴露给 `read`（`EBADF`），只经 `P5-FS-03`；tty `read`/`readv` 未实现 |
| P5-BOOT-01 | C 入口与初始栈 | partial | **M8**：初始 fd 0 为 null、fd 1/2 为 tty；其余固定探针形态不变 |

## 2. 不变的行

`P5-ABI-01`、`P5-PROC-01`、`P5-PROC-02`、`P5-MM-01`、`P5-TIME-01`、`P5-THREAD-01`、
`P5-NET-01`、`P5-LIB-01` 按 v0；M8 没有新增 `mmap`/线程/信号/网络能力，也没有升级
`P5-LIB-01` 的 stdio 文件 API。

## 3. 错误码

不新增错误码。M8 使用 v0 已冻结的 `ENOENT=2`、`EBADF=9`、`EAGAIN=11`、`EACCES=13`、
`EFAULT=14`、`EEXIST=17`、`ENOTDIR=20`、`EINVAL=22`、`ENOSPC=28`、`ENOSYS=38`。
未冻结的 `EISDIR=21`、`ENAMETOOLONG=36`、`ELOOP=40`、`ESPIPE=29`、`EMFILE=24` 不出现。

## 4. 冻结用例（M8）

| # | 场景 | Linux 参考 | Prima |
|---|---|---|---|
| V1 | `stage5_vfs.c` 全项 | 退出 0，stdout `VFS|ok=1;...` | 退出 0，stdout 逐字节一致；双轮 QEMU |

Linux 参考在一次性 cwd 中布置同形树（`etc/motd`、`etc/hostname`、`usr/lib/version`、
`README`、`f`、`tmp`），`stdin=/dev/null`；Prima 侧用 rootfs 的同一路径。绝对路径
`/etc/motd` 只在 Prima 侧执行，Linux 侧用同义相对路径，故该行不是逐路径的绝对/相对
双端差分；路径解析本身由 `prima-runtime` 的 host 单测覆盖。

## 5. 本提案不代表

- 不表示 VFS 完整、目录/路径全部支持或 `ls`/`readdir` 已验收。
- 不表示动态链接、`fork/execve/wait4`、线程、信号、网络、通用 `mmap` 可用。
- 不勾选 S5-D3/S5-X1–X3；S5-D1 仍需组织级确认。
