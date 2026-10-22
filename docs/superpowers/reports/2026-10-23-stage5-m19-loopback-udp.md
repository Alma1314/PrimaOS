# 阶段 5 / M19：用户态 loopback IPv4/UDP 子集实现报告

**日期**：2026-10-23
**范围**：路线图 M19。设计见
[M19 设计](../specs/2026-10-23-stage5-m19-loopback-udp-design.md)；D5 工作决定：
阶段 5 只做用户态 loopback IPv4/UDP，NIC/DNS/TCP 留阶段 7。
**结论**：socket/bind/getsockname/sendto/recvfrom/close 的冻结子集在 Prima 上
经同源差分与 QEMU `N1` 双轮通过；`P5-NET-01` 只按证据标 `partial`。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-runtime/src/protocol.rs` | 网络请求/应答 tag（0x5EE6/0x5EE7）、9 个 op、端口/长度位移、数据报/端点/队列上限、`EADDRINUSE` |
| `prima-user/src/servicemgr.rs` | `NetState`：4 端点 × 4 条 64 B 队列、临时端口、bind 冲突、sendto 分片聚合、recvfrom 阻塞挂起与入队唤醒；只在对端使用 socket 时分配 |
| `prima-user/c/prima_abi.h` | socket/bind/sendto/recvfrom/getsockname 号、sockaddr/错误码、net tag 与 op |
| `prima-user/c/prima_syscall.c` | `PRIMA_FD_SOCKET`、本地引用计数、`socket`/`bind`/`getsockname`/`sendto`/`recvfrom`、sockaddr_in 解析/回填；应答校验 op/handle；`close` 使用本地 reply 指针 |
| `prima-user/c/stage5_net.c` | 同源探针（pair/zero/large/drop/reuse/badfam/closed） |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Net`、`NET_EXPECTED_STDOUT`、Linux 参考与 `N1` 双轮 |

未新增内核 syscall、未新增服务任务或 Capability；默认探针的 IPC/容量基线不变
（管理器映像仍 21 页，端点表只在 socket 使用时初始化）。

## 2. 语义

- `socket`：仅 `AF_INET`/`SOCK_DGRAM`/protocol 0；否则 `EAFNOSUPPORT`/`EINVAL`。
- `bind`：`127.0.0.1`/`0.0.0.0`；端口 0 领临时端口（49152 起，跳过占用）；
  冲突 `EADDRINUSE`；`getsockname` 回读端口与地址。
- `sendto`：≤64 B；数据报由 `SENDTO_BEGIN/DATA*/END` 聚合；目标未绑定则丢弃但
  返回成功（UDP 语义）；入队后若有等待的 `recvfrom`，立即以 `RECVFROM` 应答
  唤醒（C 任务阻塞在 `SYS_RECV`，无忙等）。
- `recvfrom`：返回数据报边界与源端口；零长度数据报返回 0 且源端口有效；
  超过 64 B 的发送 `EMSGSIZE`；缓冲不足按 Linux 截断。
- `close`：最后一个本地引用关闭服务端端点；`read`/`write` 对 socket `ENOTSOCK`。

## 3. 证据

### 3.1 同源差分

`stage5_net.c` 在宿主（真实 127.0.0.1 UDP）与 Prima（端点表）上逐字节一致：

```text
NET|pair=1;zero=1;large=1;drop=1;reuse=1;badfam=1;closed=1|ok=1;err=0
```

Linux 参考 `PASS linux stage5 loopback UDP references`；QEMU `N1` 双轮 stdout、
退出码与容量记录一致。

### 3.2 host

`cargo test --workspace` 全绿（本切片未新增 Rust 单测；协议/管理器行为由
Linux 差分与 QEMU 双轮覆盖）。

### 3.3 QEMU 全量回归

```text
cargo xtask test
```

结果（76 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 run=1 / run=2
PASS qemu bios T1 / F1 / V1 / V2 / V3 / N1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS linux stage5 symlink/rename/fchdir / chmod/umask/stdio / loopback UDP references
PASS stage1 boot slice
```

`N1` 两轮的 stdout、退出码与容量记录均通过；默认探针与既有轮次无回归，
管理器映像仍 21 页，未新增任务/能力。

## 4. 调试记录（实现边界）

- 适配层必须校验应答 `op`/`handle`：服务端在同一通道上回复多类请求，错序/陈旧
  应答若只按 tag 接受会把后续请求“错位一帧”。本切片在 `prima_net_request`
  中按 `op`（`OPEN` 额外按新 handle）过滤。
- `close` 的 `prima_net_request` 原先把空指针当作 reply 输出，触发用户态写 0
  页故障；改为本地 `reply` 变量。该缺陷只在 socket `close` 路径出现，已由
  `closed` 字段与 QEMU 覆盖。

## 5. 边界

- 只支持 loopback、UDP、IPv4、127.0.0.1/0.0.0.0；无 TCP、IPv6、`connect`/
  `listen`/`accept`、`poll`/非阻塞、`MSG_*`、分片重组、校验和、NIC、DNS。
- 端点/队列容量固定（4×4×64 B）；满队列 `ENOSPC`；无背压/拥塞。
- 端点表位于服务管理器进程内；拆成独立 `prima-net` 是后续重构，不改变协议。
- D5 未组织级关闭；本切片不把 `P5-NET-01` 升级为 supported。

## 6. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 7. 本报告不代表

- 不表示 NIC/真实网络/TCP/DNS 可用；
- 不表示阶段 5 的 S5-D3 或退出条件已满足；
- 不表示 profile v1/v2 已发布。
