# 阶段 5 / M19：用户态 loopback IPv4/UDP 子集设计

**日期**：2026-10-23
**状态**：已落地（M19）。端点表在 `prima-user/src/servicemgr.rs`，适配层在
`prima-user/c/prima_syscall.c`，探针 `prima-user/c/stage5_net.c`；证据见
[M19 报告](../reports/2026-10-23-stage5-m19-loopback-udp.md)。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M19；D5 工作决定：阶段 5 只做用户态 loopback IPv4/UDP，NIC/DNS/TCP 留阶段 7。
**范围**：`socket(AF_INET, SOCK_DGRAM, 0)`、`bind`、`getsockname`、`sendto`、
`recvfrom`、`close`；端口、数据报边界与源端口、错误路径。**不**做 TCP、
`connect`/`listen`/`accept`、`poll`/`select`/`epoll`、NIC、DNS、IPv6、广播/组播
与真实网络。

## 1. 位置与边界

端点表放在 **ring3 服务管理器**进程内，而不是新增第 9 个服务任务：

- 不改变启动时任务/能力/审计基线（默认探针的 IPC 计数与容量不变）；
- 管理器已经承担 C 适配层 ↔ 基础服务的同步转发，socket 请求复用同一
  `SEND`/`RECV` 通道与 `c_send` 回送；
- 数据报经 IPC 搬运，但对外暴露的是 socket/端口/数据报语义，不是 IPC 别名；
  这满足“loopback IPC 不算网络”的边界——端点、端口占用、数据报边界、
  源端口、错误码都按 UDP 语义实现。

后续若要拆成独立 `prima-net` 服务，协议与语义不变，只需把端点表迁出并把
`SERVICE_NET_REQUEST_TAG` 路由到新任务；本切片不承诺该拆分。

## 2. 协议

请求：`tag(0x5EE6) | op(44..47) | handle(40..43) | client(32..39) | arg(0..31)`；
应答：`tag(0x5EE7) | op(44..47) | handle(40..43) | status(32..39) | data(0..31)`。

| op | 语义 | arg / data |
|---|---|---|
| `OPEN` (0) | 分配端点 | data = handle |
| `BIND` (1) | 绑定端口；0 表示临时端口 | arg = port；data = 实际端口 |
| `GETSOCKNAME` (7) | 查询绑定端口 | data = port |
| `SENDTO_BEGIN` (2) | 开始一次发送 | arg = port(0..15) \| total(16..23) |
| `SENDTO_DATA` (3) | 追加 ≤3 B | arg = 3 字节 |
| `SENDTO_END` (4) | 入队并返回 | status |
| `RECVFROM` (5) | 取一个数据报；空队列阻塞 | data = source_port(0..15) \| len(16..23) |
| `RECV_CHUNK` (6) | 按 3 B 取数据报内容 | arg = offset；data = 3 字节 |
| `CLOSE` (8) | 释放端点 | — |

- 数据报上限 `SERVICE_NET_DATAGRAM_MAX=64` B；超出 `EMSGSIZE`。
- 适配层对每个应答校验 `op` 与 `handle`（`OPEN` 除外），跳过陈旧/错序消息；
  `close` 时用本地 `reply` 变量，绝不传空指针。
- 端点表 4 个 socket、每个 4 条队列；队满 `ENOSPC`。
- 临时端口从 49152 起分配，跳过已占用端口。
- `bind` 冲突返回 `EADDRINUSE`；未绑定目标：UDP 丢弃报文但 `sendto` 成功。

## 3. 适配层

| Linux 号 | 调用 | 实现 |
|---:|---|---|
| 41 | `socket` | 只接受 `AF_INET`/`SOCK_DGRAM`/0；分配本地 fd 与服务端 handle |
| 49 | `bind` | sockaddr_in 解析；只接受 127.0.0.1/0.0.0.0 |
| 51 | `getsockname` | 回填 127.0.0.1 + 端口 |
| 44 | `sendto` | BEGIN/DATA*/END；返回字节数 |
| 45 | `recvfrom` | 同步 RECVFROM（可能阻塞）+ 分段 RECV_CHUNK；回填源地址 |

- 本地 fd 新增 `PRIMA_FD_SOCKET`，`dup*` 共享服务端 handle 与引用计数，
  最后一个引用关闭时发 `CLOSE`；退出路径同样关闭。
- `read`/`write` 对 socket fd 返回 `ENOTSOCK`（不支持流式读写）。
- 地址族错误的 `socket` 返回 `EAFNOSUPPORT`；flags 非 0 返回 `EINVAL`。

## 4. 阻塞语义

`recvfrom` 在队列为空时由管理器记录 `(handle, client)` 并**不回复**；发送方
`sendto` 入队后，如果目标端点正是等待者，管理器立即用同一 `RECVFROM` 应答
唤醒客户端。C 任务阻塞在 `SYS_RECV`，没有忙等，也没有新增内核机制或 syscall。

## 5. 证据计划

- **host**：`cargo test --workspace`（本次无新增 Rust 单测；协议/管理器均经
  QEMU 与 Linux 差分覆盖）。
- **Linux 同源**：`stage5_net.c` 在宿主用真实 127.0.0.1 UDP，与 Prima 逐字节比较。
- **QEMU**：`N1` 双轮，`NET_EXPECTED_STDOUT` 逐字节一致；容量记录
  `[17, 0x5a, 0, 1, 1]`。

## 6. 边界

- 只支持 loopback、UDP、IPv4、`127.0.0.1`/`0.0.0.0`；无 TCP、IPv6、`connect`、
  `poll`、非阻塞、`MSG_*` flags、广播/组播。
- 端点/队列固定容量；无分片/重组、无 TTL 统计、无 ICMP、无校验和验证。
- 默认探针不创建 socket；只有使用 socket 的程序才承担端点表与 IPC 开销。
- D5 未组织级关闭；本切片是工作决定，不升级 `P5-NET-01` 到 supported。

## 7. 本设计不代表

- 不表示 NIC/真实网络/TCP 可用；
- 不表示阶段 5 退出条件或 S5-D3 已满足；
- 不表示 profile v1/v2 已发布。
