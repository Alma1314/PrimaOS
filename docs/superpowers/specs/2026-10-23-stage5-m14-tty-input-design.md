# 阶段 5 / M14：tty 输入与最小 console 设计

**日期**：2026-10-23
**状态**：已落地（M14a）。行规程在 `prima-user/src/servicemgr.rs`，适配层在
`prima-user/c/prima_syscall.c`，探针 `prima-user/c/stage5_ttyin.c`；证据见
[M14 报告](../reports/2026-10-23-stage5-m14-tty-input.md)。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M14。**范围**：fd 0 的 `read`/`readv`、canonical/raw 最小行规程、`Ctrl-D` EOF、
回显、`termios` 子集与 QEMU 串口输入注入。**不**做 job control、`SIGINT`、
pty、流控、完整 `termios`、`VMIN`/`VTIME` 语义。

## 1. 等待模型与已知偏差

服务阶段 PIT IRQ0 被屏蔽、Timer 只在 syscall 边界推进，用户态阻塞 `RECV`
无法被串口数据唤醒；纯阻塞实现需要 M15 的 WaitObject 或内核串口 IRQ
（见 `ISSUE-TTYIN-001`）。M14a 采用：

- **空闲路径**：没有读者时，管理器阻塞在 `SYS_RECV`，不轮询、不忙等；
- **有读者路径**：管理器在 `TTY_READ` 处理内轮询 COM1 的 LSR/RBR，直到行完成/
  raw 有数据/EOF，然后回复。轮询只发生在“已经有人在等输入”的窗口内，属于
  已知的临时偏差，M15 落地后替换为阻塞等待。

这不是“无输入时忙等”：没有读请求就没有轮询。报告里明确登记该残余。

## 2. fd 与行规程

- fd 0 由 `/dev/null` 改为 **tty 输入**；fd 1/2 仍为 tty 输出；`/dev/null`
  仍可通过 `open` 使用。既有 V1 探针的“fd0 读 EOF”断言随之移除。
- 管理器持有 COM1 的 `Device(IOPortRange)` cap（既有 `mgr_serial`），用
  `SYS_DEV_IOPORT_IN` 读 LSR/RBR、`SYS_DEV_IOPORT_OUT` 回显。
- canonical（默认）：行缓冲 64 B；`\n`/`\r` 完成一行（统一追加 `\n`）；
  `0x7f`/`0x08` 删除前一字符并回显 `\b \b`；空行 `Ctrl-D` 置 EOF，非空行
  `Ctrl-D` 交付当前缓冲。
- raw：字节直接进入 ready 队列；是否回显由 termios `ECHO` 决定。
- 单次 `read` 最多返回 3 字节（tty 允许短读）；0 表示 EOF。非 canonical 的
  `read` 也可以只返回当前可用字节。

## 3. 协议

| tag | 方向 | 载荷 |
|---|---|---|
| `STAGE5_TTY_READ_TAG` (0x5EE8) | C→管理器 | `client(40..47) | max(32..39)` |
| `STAGE5_TTY_READ_REPLY_TAG` (0x5EE9) | 管理器→C | `count(24..31) | 3 字节(0..23)` |
| `STAGE5_TTY_MODE_TAG` (0x5EEA) | C→管理器 | `ICANON(0) | ECHO(1)` |

`count=0` 表示 EOF。`MODE` 在 `tcsetattr` 时单向通知；管理器据此更新行规程。

## 4. termios 子集

- 适配层保存 60 B Linux x86_64 `struct termios`（`_Static_assert`）。
- 默认 `c_lflag = ICANON | ECHO | ISIG`；`c_cc` 填 `VINTR`/`VQUIT`/`VERASE`/
  `VKILL`/`VEOF`/`VMIN`；`VMIN`/`VTIME` 只保存，不实现超时语义。
- `ioctl`：`TCGETS`、`TCSETS`/`TCSETSW`/`TCSETSF`、`TIOCGWINSZ`（24×80）；
  其它请求 `ENOTTY`；非 tty fd `ENOTTY`。
- `ISIG` 只保存；`Ctrl-C` 不产生 signal（M18 前无 signal 用户面）。

## 5. QEMU 输入注入

xtask 新增 marker 驱动的输入通道：

- QEMU 以 `stdin=piped` 启动；stdout 读取线程扫描已累积的串口输出；
- 每个 `(marker, bytes)` 在 marker 首次出现时把 bytes 写入 QEMU stdin
  （即 guest 串口输入），保证输入不会早于读者被丢弃；
- `I1` 脚本：`TTYREADY→"hi\n"`、`TTYRAW→"xy"`、`TTYEOF→0x04`。

探针前缀用 `TTYRD|`（避免与固定探针的 `TTYIN|`/`TTYOUT|` 见证冲突）。

## 6. 证据计划

- **Linux 同源**：`stage5_ttyin.c` 用管道 stdin 验证 canonical 行与 EOF；
  termios/回显/raw 需要真实 tty，Prima 侧由 QEMU 串口验证，差异在报告登记。
- **QEMU**：`I1` 双轮，含回显的 `TTYRD|...` stdout 逐字节一致；ECHO 见证的
  len/hash 计入回显字节（终端输出的一部分）。
- 既有 E1–E6/T1/F1/V1/V2/V3/N1/M1/C1/L1–L3/D1/A1 全量重跑。

## 7. 边界

- 无 IRQ/WaitObject；有读者时轮询（见 §1），M15 后替换。
- 无 job control、signal、pty、`ioctl` 全量、流控/`VMIN`/`VTIME`。
- 管理器单读者：同一时刻只有一个 C 任务在读 fd 0；不排队多个读者。
- 行缓冲/ready 队列各 64 B；超长行截断。

## 8. 本设计不代表

- 不表示 M15 的 WaitObject 已存在；
- 不表示信号、pty 或通用 console 已实现；
- 不表示阶段 5 退出条件已满足。
