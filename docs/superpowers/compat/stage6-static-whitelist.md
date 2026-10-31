---
id: compat-stage6-static-whitelist
kind: ledger
authority: frozen-whitelist
status: frozen-by-decision-d12
lifecycle: current
updated: 2026-10-30
---

# 阶段 6 静态 ELF 白名单（D12 冻结）

本台账是 `S6-X1`（"白名单中的静态 ELF 无需修改即可运行"）里"白名单"的唯一定义。
裁决见[阶段 6–7 前置裁决 D12](../specs/2026-10-28-stage6-7-release-decisions.md)。

**冻结含义**：成员、制品版本或 `sha256` 任一变化都算新裁决；`S6-X1` 只对本台账的
集合成立，不得用"换一个能跑起来的二进制"关闭它。本台账是**契约**，不是实现状态——
当前没有任何成员在本系统运行过（LibOS 递送未实现）。

## 制品

| 项 | 值 |
|---|---|
| 包 | Ubuntu noble `busybox-static` `1:1.36.1-6ubuntu3.1` |
| 路径 | `/usr/bin/busybox`（`dpkg -S` 归属 `busybox-static`） |
| 大小 | 2 124 608 B |
| `sha256` | `dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14` |
| 版本串 | `BusyBox v1.36.1 (Ubuntu 1:1.36.1-6ubuntu3.1) multi-call binary` |

必须同时满足下列 ELF 属性（2026-10-29 实测，`readelf -lW` / `objdump`）：

| 属性 | 观察值 | 为什么重要 |
|---|---|---|
| 类型 | `ET_EXEC`，无 `PT_INTERP` | D11：阶段 6 不做动态链接 |
| `PT_LOAD` 数 | 4 | 与装载器上限 8 的关系；第 4 段 `vaddr=0x5fc820`/`offset=0x1fb820` 同余而非页对齐 |
| `PT_TLS` | 存在（`0x5fc820`，`memsz=0x82`） | 未修改 libc 的 TLS 初始化路径 |
| `GNU_RELRO` | 存在（覆盖 `.data.rel.ro`/`.got`） | 装载期必须可写，只读化属装载后 `mprotect` |
| `GNU_STACK` | `RW`（非可执行） | 装载器拒绝可执行栈 |
| SSP | 开启：2 402 处 `%fs:0x28` | D13：必须提供 `AT_RANDOM` 兼容字段 |
| 页需求 | 526 页（`1+424+83+18`） | 当前 `MAX_SEGMENT_FRAMES=149` 不够（映像字节长度上界已放宽，见[本轮报告](../reports/2026-10-30-stage6-freeze-ledgers.md)），见剩余工作 |

复现命令（宿主）：

```sh
sha256sum /usr/bin/busybox
readelf -lW /usr/bin/busybox
LC_ALL=C objdump -d /usr/bin/busybox | grep -c 'fs:0x28'
```

## 成员

6 个 applet + 2 个自写探针。成员按"覆盖的 syscall 簇"选取，不按知名度。

| 成员 | 覆盖簇 | 通过判据 |
|---|---|---|
| `true` | `exit_group` | 退出码 0，无额外输出 |
| `echo hello` | `write` | stdout 逐字节等于 `hello\n` |
| `cat <file>` | `openat`/`read`/`close`/`write` | 输出等于输入文件字节 |
| `ls <dir>` | `getdents64`/`newfstatat`/`ioctl(TIOCGWINSZ)` | 条目集合与参考 guest 一致（排序后比对） |
| `uname` | `uname` | 字段值与参考 guest 一致（内核版本字段按本项目声明） |
| `sort`（stdin→stdout） | `read`/`write`/`brk`/`mmap` | 输出为输入行的字典序 |
| 探针 P1（自写） | 裸 `syscall` 指令 + errno/EINTR | 证明是"递送"而不是"重定向 libc"；负例得到确定性 errno |
| 探针 P2（自写） | `arch_prctl`/TLS/futex 式等待 | 设置 FS base 后立即可读回；等待/唤醒无丢失 |

约束：不涉及写盘、网络、账号或终端交互。需要这些的成员必须另开裁决。

## 差分与证据规则

1. 同一制品同时作为 Prima 侧与 Linux 参考侧的输入（复用 `xtask/src/linux.rs` 的 pinned
   guest），因此"同源"不是靠重新编译另一版本来凑。
2. 每成员的通过判据必须给出**具体命令与观察值**，且映射到 `S6-X1`/`S6-X2` 的某一项
   （见[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)的 S6-M5）。
3. 未支持 syscall 的失败必须可审计（`ENOSYS` + 审计记录），不是超时或崩溃。
4. 二进制不入库；入库的是本台账的 `sha256` + 属性 + 判据 + 探针源码。

## 当前状态

| 项 | 状态 |
|---|---|
| 白名单定义（D12） | **已冻结** |
| 装载器解析该制品 | 已成立（2026-10-28 装载切片；2026-10-30 用同一 `sha256` 的二进制重跑 `PRIMA_ELF_SAMPLE` 演练，解析、长度窗口与 526 页页数均按断言复核，见[本轮机械项报告](../reports/2026-10-30-stage6-freeze-ledgers.md)） |
| 任何成员在本系统运行 | **未发生**（缺 S6-M1 递送与 S6-M3 profile） |
| 容量缺口 | `MAX_SEGMENT_FRAMES=149`（需 526 页）与 8 页初始栈；映像**字节**长度上界已由 S6-M2 ① 放宽到 4 MiB，但它不是容量门 |

## 后继关系

- 上游裁决：[D11–D16](../specs/2026-10-28-stage6-7-release-decisions.md)（D12 白名单、D13 `AT_RANDOM`）
- 实现入口：[阶段 6 启动与首个 S6-M2 增量](../reports/2026-10-29-stage6-start.md)
- 相关台账：[compat-state.md](compat-state.md)（阶段 5 有界 POSIX 子集，不是本白名单）
