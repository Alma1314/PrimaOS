# 阶段 5 / M11：文件树变更（`mkdir`/`rmdir`/`unlink`）设计

**日期**：2026-10-22
**状态**：设计已落地；实现与 QEMU 证据见
[2026-10-22-stage5-m11-file-mutation.md](../reports/2026-10-22-stage5-m11-file-mutation.md)。
**上位计划**：[阶段 5 修订计划](../plans/2026-10-18-stage5-revised-plan.md)片 D 与
[musl 移植计划](../plans/2026-10-18-musl-port-plan.md)。
**范围**：在 M8/M9 的用户态 VFS/rootfs 上补齐**单路径**目录/文件删除与目录创建：
`mkdir`/`mkdirat`、`unlink`/`unlinkat`、`rmdir`。不包含 `rename`/`mv`、硬链接、
符号链接、权限位、递归删除或跨启动持久化。

## 1. 取舍

M8 的 VFS 只有静态节点表与 `O_CREAT` 追加；节点删除/目录创建缺失，因此：

- `sbase mkdir`、`rm`、`rmdir` 不能运行；
- 目录树只能增长（`O_CREAT` 直到 `MAX_NODES`），不能回收；
- `unlinkat`/`rmdir` 是 `sbase rm -r` 的硬前置。

M11 选择**不新增内核原语**：VFS 是用户态存储服务的纯逻辑模型，变更完全在 ring3
完成，经现有 IPC 通道（MUTATE + PATH + COMMIT）到达存储服务。这样既保持
“内核不解析文件树”的 M8 边界，也不触碰 COSIR 的 Task/Process 语义。

## 2. VFS 模型

### 2.1 删除标记与槽位复用

节点表仍是 `MAX_NODES=16` 的连续数组，但删除不再需要压缩或改变格式版本：

```text
parent = 0xFF  → 已删除槽（FREE_PARENT；MAX_NODES < 255，不与真实父节点冲突）
```

- 删除只把 `parent` 置 `0xFF`，并清 `name/size/data`；`kind` 保留，使未关闭句柄
  在槽位复用前仍能读到旧内容（存储服务实际会先拒绝带打开句柄的删除，见 §3）。
- `lookup`、`dir_entry_count`、`dir_entry` 一律跳过 `FREE_PARENT`，因此删除后的
  名字不再可解析、不出现在目录枚举里。
- 新节点分配（`mkdir` 与 `open(O_CREAT)` 共用）先扫描 `[1, count)` 的空闲槽，
  没有才追加并把 `Header.count` 加一；槽位耗尽返回 `ENOSPC`。这条复用规则修复了
  “删除后高水位仍挡创建”的问题（D1 首轮暴露）。
- 内容区仍只追加：删除文件不回收 `data` 区。首版容量 4 KiB，报告登记为边界。

### 2.2 API 与前置

| 操作 | 前置 | 成功 | 失败 |
|---|---|---|---|
| `mkdir(start, path)` | 路径非空、末项非 `.`/`..`、父目录存在且可写、末项不存在 | 分配槽，写 `Directory` 节点（可写） | `EEXIST`（已存在/`.`/`..`/根）、`ENOENT`、`ENOTDIR`、`EACCES`、`ENOSPC` |
| `unlink(start, path, remove_dir=false)` | 父目录存在且可写、末项存在、不是目录/设备 | 标记删除 | `ENOENT`、`ENOTDIR`、`EACCES` |
| `rmdir`（`remove_dir=true`） | 同上，且目标为**空**目录 | 标记删除 | `ENOENT`、`ENOTDIR`、`EACCES` |

- 根、`.`、`..` 不能被删除；删除只读父目录下的节点返回 `EACCES`。
- 非空目录与“用 `unlink` 删目录”在冻结错误集中没有 `ENOTEMPTY`/`EISDIR`，
  统一返回 `EACCES`；这是显式偏差，探针按平台断言各自期望。
- 权限位不建模：新建目录统一可写（0777），`mode` 被忽略；`mkdir` 不创建中间目录
  （`-p` 由 sbase 逐级调用，后续切片验收）。

## 3. 存储服务

### 3.1 协议

新增 `SERVICE_VFS_MUTATE_TAG = 0x5EDB…`：

```text
payload = tag | op(44..47) | path_len(32..39) | start(24..31) | 前 3 字节
op      = 0 mkdir, 1 unlink, 2 rmdir
```

随后复用 `PATH` 片段（每段 5 字节）与 `COMMIT`；`COMMIT` 触发变更并回一条
`FILE_REPLY`（`op=OPEN, fd=0, status=errno, data=0`）。管理器只把该 tag 纳入既有
VFS 转发分支，不解释语义。

### 3.2 打开句柄保护

删除成功后节点槽可立即被复用；若仍有服务端打开句柄指向它，句柄会读到新节点的
内容。首版选择**拒绝**：`unlink/rmdir` 前扫描 `files`，任一 `open && node == id`
返回 `EACCES`。Linux 允许“删除后已打开句柄继续读”，该差异列入 partial 边界；
后续可用引用计数实现真正语义。

### 3.3 错误映射

`VfsError → errno` 沿用 M8：`NoEnt=2`、`Access=13`、`Exist=17`、`NotDir=20`、
`Invalid=22`、`NoSpace=28`。`AT_REMOVEDIR=0x200` 之外的 `unlinkat` flags 返回
`EINVAL`。

## 4. C 适配层

新增 Linux 号：`mkdir=83`、`rmdir=84`、`unlink=87`、`mkdirat=258`、`unlinkat=263`。

- `mkdirat(dirfd, path, mode)` / `unlinkat(dirfd, path, flags)` 共用
  `prima_path_target`：`AT_FDCWD` 的相对路径与 cwd 合成绝对路径；`dirfd` 形式以
  该目录节点为起点。
- `mkdir`/`rmdir`/`unlink` 分别落到 `AT_FDCWD` 的 `mkdirat`/`unlinkat`。
- 变更请求复用 OPEN/PATH/COMMIT 的分片逻辑；空路径 `ENOENT`，超长 `EINVAL`。
- `rename`/`renameat`/`renameat2`、`link`/`symlink` 仍返回 `ENOSYS`。

## 5. 冻结测试 D1

`prima-user/c/stage5_mut.c` 同一份源码在 Linux（glibc）与 Prima（静态 musl +
适配层）编译运行。Prima 使用 `/tmp`（可写）与 `/etc`（只读）；宿主在一次性 cwd
中运行，只读父目录由 xtask 预置 `ro/`（0555）。输出：

```text
MUT|ok=1;mkdir=1;exists=1;stat_dir=1;nested=1;nonempty=1;rmdir=1;unlink_file=1;unlink_dir=1;rmdir_file=1;readonly=1;missing=1;enoent=1;at=1
```

覆盖创建/重复创建/父不存在/只读父目录、非空目录、文件与目录的类型不匹配、
删除后 `ENOENT`、`mkdirat`/`unlinkat`（含 `AT_REMOVEDIR`）与目录 fd 上的
`openat`。`nonempty`/`unlink_dir` 是平台冻结期望（Linux `ENOTEMPTY`/`EISDIR`，
Prima `EACCES`）。

## 6. 已知边界

- 无 `rename`/`mv`、硬链接、符号链接、`linkat`、`unlinkat` 的 `AT_REMOVEDIR`
  以外 flags；
- 无权限位/`umask`/`chmod`；`mode` 忽略，目录统一 0777；
- 删除不回收内容区；节点表 16 槽，删除后可复用；
- 带打开句柄的节点拒绝删除（`EACCES`）；
- 非空目录/目录 `unlink` 用 `EACCES` 代替 `ENOTEMPTY`/`EISDIR`；
- 只支持单路径操作；`rename` 的两路径事务与 `mv` 需下一片；
- 持久化只在单次启动内（存储服务段的生命周期），跨启动仍从静态 rootfs 重新格式化；
- 不改变 `MAX_NODES`、`MAX_SEGMENT_FRAMES`、`MAX_AUDIT_EVENTS` 等容量常量。

## 7. 证据等级

- **host**：`prima-runtime::vfs` 单测（创建/删除/非空/只读/槽位复用/枚举跳过）；
- **QEMU**：D1 双轮 + 既有 TASKCAP/CAPACITY/FAULT 见证；
- **Linux/Prima 差分**：D1 stdout 逐字节一致；
- 不构成完整 POSIX 文件系统、`rename` 或持久化证明。
