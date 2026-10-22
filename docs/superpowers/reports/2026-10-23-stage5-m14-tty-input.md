# 阶段 5 / M14：tty 输入与最小 console 实现报告

**日期**：2026-10-23
**范围**：路线图 M14a。设计见
[M14 设计](../specs/2026-10-23-stage5-m14-tty-input-design.md)。
**结论**：fd 0 的 tty 输入、canonical/raw、`Ctrl-D` EOF、回显、termios 子集
与 QEMU marker 驱动输入注入已落地；`I1` 双轮与 Linux 管道参考通过。
有读者时的 COM1 轮询是已登记的临时偏差（`ISSUE-TTYIN-001` 部分关闭），
待 M15 WaitObject/IRQ 替换。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-runtime/src/protocol.rs` | TTY_READ/READ_REPLY/MODE tag（0x5EE8–0x5EEA）、client/max/count 位移、单次 3 B、行缓冲上限 |
| `prima-user/src/servicemgr.rs` | `TtyInput` 行规程（canonical/raw、回显、删除、`Ctrl-D`）；`tty_read_request` 轮询 COM1；MODE 处理；回显计入 ECHO 见证 |
| `prima-user/c/prima_syscall.c` | fd 0 改 tty 输入；`prima_tty_read`（单次 ≤3 B）；60 B `struct termios` 与 `TCGETS`/`TCSETS*`/`TIOCGWINSZ`；`readv` 复用 |
| `prima-user/c/prima_abi.h` | 输入 tag/位移、ioctl 号、`ICANON`/`ECHO`、`ENOTTY` |
| `prima-user/c/stage5_vfs.c` | 移除“fd0 读 EOF”断言（fd0 现为 tty 输入） |
| `prima-user/c/stage5_ttyin.c` | 新探针（termios/line/raw/EOF） |
| `xtask/src/qemu.rs` | marker 驱动 stdin 注入运行器与 `run_bios_test_input` |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Ttyin`、`TTYRD` 期望输出、Linux 管道参考与 `I1` 双轮 |

未新增内核 syscall、未改 `prima-abi` 号段、未新增服务任务。

## 2. 等待模型

- 没有读请求：管理器阻塞在 `SYS_RECV`（无忙等）。
- 有读请求：管理器在 `TTY_READ` 处理内轮询 COM1 LSR/RBR，直到行完成/raw
  有数据/EOF，然后回复；这是 M14a 的临时偏差，报告与 profile 明确登记。

## 3. 证据

### 3.1 同源与 QEMU

Prima `I1` 的串口输出（含输入回显）：

```text
TTYRD|termios=1
TTYREADY
hi
;line=1
TTYRAW
;raw=1
TTYEOF
;eof=1|ok=1;err=0
```

Linux 参考用管道 stdin，只验证行与 EOF（termios/回显/raw 需真实 tty）：

```text
TTYRD|line=1;eof=1|ok=1;err=0
```

差异在 profile `P5-TTY-01` 与本节登记。QEMU 输入由 marker 驱动：
`TTYREADY→"hi\n"`、`TTYRAW→"xy"`、`TTYEOF→0x04`。

### 3.2 host

`cargo test --workspace` 全绿。`stage5_vfs.c` 的 fd0 断言已随 M14 的初始 fd
语义移除；V1 期望串同步更新。

### 3.3 全量回归

```text
cargo xtask test
```

结果（80 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 run=1 / run=2
PASS qemu bios T1 / F1 / V1 / V2 / V3 / N1 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS linux stage5 symlink/rename/fchdir / chmod/umask/stdio / loopback UDP / tty input references
PASS stage1 boot slice
```

`I1` 两轮的 stdout（含回显）、退出码与 ECHO 见证（计入回显字节）一致；
V1 的 fd0 断言随初始 fd 语义更新后无回归。

## 4. 已知偏差与后续

- 有读者时轮询 COM1；M15 的 WaitObject 或内核串口 IRQ 递送落地后替换。
- 无 job control、`SIGINT`、pty、流控、`VMIN`/`VTIME`；`ISIG` 只保存。
- 单读者：同一时刻只有一个 C 任务读 fd 0；行/ready 各 64 B。
- Linux 参考不是 tty，因此 termios/回显/raw 只由 Prima QEMU 证据覆盖。

## 5. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 6. 本报告不代表

- 不表示 M15 的等待模型已存在；
- 不表示通用 console/信号/pty 可用；
- 不表示阶段 5 退出条件已满足。
