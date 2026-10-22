# 阶段 5 / M13b：权限位、umask 与 stdio 文件 API 设计

**日期**：2026-10-22
**状态**：已落地（M13b）。VFS 权限位在 `prima-runtime/src/vfs.rs`，协议/服务在
`prima-runtime/src/protocol.rs`、`prima-user/src/service.rs`，适配层与 umask 在
`prima-user/c/prima_syscall.c`；证据见
[M13b 报告](../reports/2026-10-22-stage5-m13b-permissions-stdio.md)。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M13；[用户态 VFS 设计](2026-10-20-stage5-user-vfs-design.md)；[M13a 设计](2026-10-22-stage5-m13a-vfs-links-design.md)。
**范围**：给 VFS 节点加 POSIX 权限位，补 `chmod`/`fchmod`/`fchmodat` 与
`umask` 的冻结子集；让 musl 的 `fopen`/`fread`/`fwrite`/`fclose` 路径真实运行。
**不**做 uid/gid、组/other 权限强制、setuid/setgid、ACL、符号链接上的
`lchmod` 或跨启动持久化。

## 1. 节点与权限模型

- 节点布局升级为 **v2**：`NODE_BYTES` 32 → 36，新增 16 位 `mode`（低 12 位，
  本切片只接受 0..0o777）。
  - 字节 0 name_len、1 kind、2 parent、3 保留、4..6 size、6..8 data、
    8..10 mode、10..26 name。
  - `VERSION=2`；v1 镜像确定拒绝（存储服务每次启动在零化段上重新 `mount`，
    不存在迁移问题）。
  - 内容区起点 `16 + 32×36 = 1168`，`CONTENT_CAPACITY = 4096 − 8 − 1168 = 2920`。
- 初始 rootfs 的 mode 与 M9 冻结值一致：目录 0555（`/tmp` 0777）、文件 0444
  （`f` 0666）、`/dev/null` 0666、符号链接 0777。
- 单用户模型只看 **owner 位**：写检查 `mode & 0o200`，读检查 `mode & 0o400`；
  不建模 uid/gid。父目录写位决定创建/删除/rename/symlink。
- `chmod` 跟随末项符号链接；空路径表示节点自身（`fchmod`）。模式位高于 0o777
  返回 `EINVAL`（不建模 setuid/setgid/sticky）。

## 2. 创建 mode 与 umask

服务端创建文件/目录时固定用 0666/0777；**适配层**在创建成功后按
`mode & ~umask` 发一次 `chmod` 收紧。理由：`OPEN` 消息的 48 位 payload 已被
flags/path_len/start/前 3 字节占满，为本切片扩展线上格式不值得；创建与 chmod
在同一任务内顺序执行，无并发窗口。

- 适配层持有进程级 `umask`（默认 022），`umask(2)` 返回旧值并设置新值。
- `open(..., O_CREAT, mode)` 与 `mkdir`/`mkdirat` 都应用 umask。
- 0666/0777 的默认值保持与之前一致；未调用 `umask` 的程序看到 0644/0755。

## 3. 协议与应答布局

### 3.1 chmod 请求

```text
OPEN:   tag(0x5EE2) | start(24..31) | path_len(32..39) | 前 3 字节(0..23)
PATH*:  SERVICE_VFS_PATH_TAG（每段 5 字节）
COMMIT: SERVICE_VFS_COMMIT_TAG | client(32..39) | mode(8..16)
```

`mode` 在 COMMIT 中携带 9 位；服务端用 `STAGED_CHMOD` 暂存态区分，空路径时改
`start` 节点（`fchmod`）。应答复用 `FILE_REPLY`（op=OPEN，status=0/errno）。

### 3.2 stat 应答的 mode

`data` 布局调整为：

```text
node(0..7) | kind(8..9) | writable(10) | mode(11..19) | size(20..31)
```

- mode 9 位（0..0o777），writable 位由 `mode & 0o200` 派生；
- size 12 位（≤4095，内容区实际 2920）；
- `statx`/kstat 的 `st_mode = ifmt | (mode & 0o777)`，`ifmt` 覆盖目录/普通/字符/
  符号链接。

### 3.3 readv

musl 的 stdio 读路径使用 `readv(19)`；适配层此前只实现 `writev(20)`。M13b 增加
`readv`，按 iovec 顺序调用同一 fd 读路径，短读即停；这使 `fread` 真正经过
`read` 到存储服务。

## 4. 适配层接口

| Linux 号 | 调用 | 实现 |
|---:|---|---|
| 90 | `chmod` | 路径规范化 + CHMOD 暂存 |
| 91 | `fchmod` | fd 节点 + 空路径 |
| 95 | `umask` | 进程级值，返回旧值 |
| 268 | `fchmodat` | flags=0 时同 chmod，其它 `EINVAL` |
| 19 | `readv` | 顺序 fd 读 |

## 5. 证据计划

- **host**：`vfs` 单测覆盖 mode 更新、owner 读写位、目录 mode 对创建的限制、
  非法 mode/缺失路径、空路径 `fchmod`。
- **Linux 同源**：`stage5_vfs3.c` 覆盖 umask(0)/umask(077) 下的文件/目录 mode、
  `chmod`/`fchmod`、0444 写拒绝、stdio `fopen("w+")` 往返、只读 `fopen("r")`
  与写打开拒绝、缺失路径 chmod。
- **QEMU**：`V3` 双轮，`VFS3_EXPECTED_STDOUT` 逐字节一致；容量记录
  `[17, 0x5a, 0, 1, 1]`。

## 6. 边界

- 无 uid/gid/组/other 强制；`chmod` 只保存并检查 owner 位。
- 不支持 setuid/setgid/sticky、ACL、`lchmod`、符号链接自身的 mode 修改。
- `chmod` 的 umask 收紧是一次额外同步请求；不改变 `O_EXCL` 的原子性。
- 不改变路径/名称/分量上限；不实现跨启动持久化。

## 7. 本设计不代表

- 不表示 D8 已关闭或 profile v1/v2 已发布；
- 不表示 M13 之外的进程/线程/网络能力；
- 不表示阶段 5 退出条件已满足。
