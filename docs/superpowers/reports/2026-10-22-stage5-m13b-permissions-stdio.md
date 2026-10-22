# 阶段 5 / M13b：权限位、umask 与 stdio 文件 API 报告

**日期**：2026-10-22
**范围**：路线图 M13b。设计见
[M13b 设计](../specs/2026-10-22-stage5-m13b-permissions-stdio-design.md)；
硬链接、跨启动持久化仍不在范围。
**结论**：节点 mode、`chmod`/`fchmod`/`fchmodat`、进程级 `umask`、`readv` 与
musl stdio 文件 API 已落地；host、Linux 同源差分与 QEMU `V3` 双轮通过。M13 的
冻结 VFS 子集（除硬链接/持久化）就此收口，profile 仍只按证据标 `partial`。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-runtime/src/vfs.rs` | 节点 v2 布局（36 B，新增 16 位 mode）；`Node::writable()/readable()`；`chmod`（路径跟随、空路径按节点）；`VERSION=2` |
| `prima-runtime/src/protocol.rs` | stat/open 应答加入 9 位 mode、size 收窄到 12 位；`SERVICE_VFS_CHMOD_TAG`（0x5EE2）与 mode/COMMIT 位移 |
| `prima-user/src/service.rs` | `STAGED_CHMOD` 暂存与执行；stat/open 应答使用节点 mode |
| `prima-user/src/servicemgr.rs` | 转发 chmod 请求；失败时回送**同族**应答（pair/link 不再等错标签） |
| `prima-user/c/prima_syscall.c` | `chmod`/`fchmod`/`fchmodat`/`umask`；`O_CREAT`/`mkdir` 后按 umask 收紧；`readv(19)`；statx/kstat 从节点 mode 生成 |
| `prima-user/c/prima_abi.h` | 新 syscall 号、CHMOD tag、mode 应答位移 |
| `prima-user/c/stage5_vfs3.c` | 新同源探针（umask/chmod/stdio） |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Vfs3`、`VFS3_EXPECTED_STDOUT`、Linux 参考与 `V3` 双轮 |
| `xtask/src/protocol.rs` | 默认探针 IPC 基线 55/164/172、`audit_used=1346`（每个创建多一次 chmod） |

未新增内核 syscall、未改 `prima-abi` 号段；`chmod` 的 umask 收紧是适配层的
一次额外同步请求，不是服务端隐藏语义。

## 2. 语义

- mode 只接受 0..0o777；单用户模型只强制 owner 读写位。无 uid/gid、组/other、
  ACL、setuid/setgid/sticky。
- `chmod` 跟随末项符号链接；`fchmod` 按 fd 的节点；`fchmodat` 仅 flags=0。
- `umask` 默认 022，进程级（适配层）；`open(O_CREAT)`/`mkdir`/`mkdirat` 创建后
  以 `mode & ~umask` 收紧。这保留了 0666/0777 的创建默认值，又让 `umask(077)`
  的 0600/0700 可观察。
- `stat`/`statx` 的 `st_mode = ifmt | (node.mode & 0o777)`；初始 rootfs 的
  0444/0555/0666/0777 不变。
- musl stdio 读走 `readv`，适配层按 iovec 顺序拆成 fd 读；写仍走 `writev`。

## 3. 证据

### 3.1 host

```
cargo test -p prima-runtime --lib vfs::
test result: ok. 18 passed; 0 failed

cargo test --workspace
（全部 test result: ok）
```

新增：`chmod_updates_mode_and_access_checks`（mode/owner 位、目录 mode 限制创建、
空路径 fchmod、非法 mode/缺失路径）。

### 3.2 同源差分

`stage5_vfs3.c` 在宿主参考树与 Prima 上逐字节一致：

```text
VFS3|umask0=1;umask077=1;chmod=1;readonly=1;stdio=1;stdio_ro=1;missing=1|ok=1;err=0
```

Linux 参考 `PASS linux stage5 chmod/umask/stdio references`；QEMU `V3` 双轮
stdout/退出码/容量记录一致。

### 3.3 全量回归

```text
cargo xtask test
```

结果（72 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 run=1 / run=2
PASS qemu bios T1 / F1 / V1 / V2 / V3 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS linux stage5 symlink/rename/fchdir references
PASS linux stage5 chmod/umask/stdio references
PASS stage1 boot slice
```

`V3` 两轮的 stdout、退出码与容量记录均通过；既有轮次无回归。

默认探针的 IPC 计数器因每个 `O_CREAT`/`mkdir` 多一次 chmod 请求而变化：
53/160/168 → **55/164/172**，`CAPACITY.audit_used` 1322 → **1346**。两项都在
`xtask/src/protocol.rs` 的冻结基线与 M13b 设计里登记，不是放宽断言。

## 4. 容量

| 项 | 旧值 | 新值 |
|---|---:|---:|
| `NODE_BYTES` | 32 | 36（mode 字段） |
| `CONTENT_OFFSET` | 1168 | 1168（16 + 32×36） |
| `CONTENT_CAPACITY` | 3048 | 2920 |
| `MAX_NODES` | 32 | 32 |
| 默认探针 audit | 1322 | 1346 |

服务映像段仍在 149 页内；未改 `MAX_SEGMENT_FRAMES`、`MAX_CAPABILITY_SLOTS`、
`MAX_AUDIT_EVENTS`。

## 5. 已知限制与后续

- 无 uid/gid/组/other/ACL/setuid/setgid/sticky；`chmod` 只保存 owner 位。
- 不支持 `lchmod`、符号链接自身 mode、`fdopen`/`freopen`/`setvbuf` 验收。
- `umask` 在适配层应用，创建与 chmod 之间是同一任务内的顺序请求；
  `O_EXCL` 原子性不受影响，但没有“创建即带 mode”的单消息语义。
- 硬链接与跨启动持久化仍未实现（profile 标 unsupported/阶段 7 候选）。

## 6. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 7. 本报告不代表

- 不表示 M13 之外的阶段 5 条件满足；
- 不表示 D8 已关闭或 profile v1/v2 已发布；
- 不表示硬链接、ACL 或持久化可用。
