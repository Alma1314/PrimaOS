# 切片 P 报告：Linux trace 与有界差分（`cow-write-fault`）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-15
>
> 对象：`xtask/src/{linux,compat,qemu,main}.rs`、`xtask/assets/linux/*`、
> `xtask/tests/fixtures/linux-trace/*`
>
> 目的：为冻结切片最后一节点（Linux trace）建立真实内外核 trace 对应，并与选定 Linux 行为做
> 有界差分

## 0. 结论速览

- 参考 Linux guest 打通（pinned `linux-image-6.8.0-31-generic` + busybox initramfs + KVM），
  `exceptions:page_fault_user` tracepoint 可用；`cow-write-fault` 场景在 Linux 侧得到
  **目标页单次故障 `error_code=0x7`**。
- Prima 侧 `user_cow_ready` 的 `err=0x7;class=write_protection;shared0=pattern(0);
  private0=VAL;private1=pattern(1);receipt=VAL` 与 Linux 侧逐项对应。
- `cargo xtask compat` ⇒ `PASS compat cow-write-fault`（离线，读已提交基线）。
- 三类篡改负例均被判定器捕获，反例保留于 §4。
- **证据名目为 `KERNEL-TRACE` + `COMPAT-TEST (bounded: cow-write-fault)`，不是完整
  `COMPAT-TEST`；不勾选 S6-X2。**

## 1. 方法与独立性声明

- 参考侧为**真实 Linux 6.8.0-31.31 generic 内核**（deb SHA512 强制校验）在 QEMU 中运行的
  trace，非文档推断；Prima 侧为**真实内核 QEMU 事件**。
- 差分判定器为宿主侧纯函数（`xtask/src/compat.rs`），带 14 条 host 单测（1 正例 + 各类
  divergence 反例）。
- 本报告的复核独立性为 **harness 内独立会话级别**（同 TEST-005/SPEC-055 口径），**不是组织级
  第三方审查**，也不构成 `REFINEMENT-PROOF`。它不改变 S0-X1 的独立性门槛，阶段 0 维持"进行中"。

## 2. 实现发现

| # | 发现 | 处置 |
|---|---|---|
| 1 | Linux 侧首版用 `mprotect(PROT_READ)` 造只读页，子进程写直接 `SIGSEGV`（`status=0xb`）——`mprotect` 改的是 VMA 权限，不是 COW | 改为 `mmap(MAP_PRIVATE\|ANONYMOUS\|MAP_POPULATE)` + `fork`：页 present、VMA 可写、PTE 被 fork 置只读，子写即 COW 写保护故障 |
| 2 | 不用 `MAP_POPULATE` 时目标页会先出现 `not_present` 的 `0x6` 写故障，混入目标页 | 加 `MAP_POPULATE`，使目标页只出现 COW 的 `0x7`；`normalize` 另要求目标页恰一次故障，>1 时直接报错而非静默接受 |
| 3 | `parse_prima` 首版取第一个带 `data=` 的事件（`memory_ready` 等也带 `data=`），会解析错事件 | 先按 `event=user_cow_ready` 过滤行；补单测（样例中加入早于它的 `memory_ready` 行）防回归 |
| 4 | 子进程 `printf` 输出与父进程输出在串口上不可靠（缓冲继承） | guest 程序 `emit` 改用 `snprintf` + `write(2)` |

## 3. 差分判定与实测

| 判定 | Prima 实测 | Linux 实测 |
|---|---|---|
| error code（P\|W\|US） | `0x7` | `0x7` |
| 分类 | `write_protection` | 由 error code 按同一规则推导为 `write_protection` |
| 页内偏移 | `USER_X_VA % 4096 = 0` | 目标页 `% 4096 = 0` |
| 重试写生效 | `private0 = receipt = VAL` | `write_retried_ok=1`（值 `0x5a5a...beef`） |
| 原副本不变 | `shared0 = pattern(0)` | `original_unchanged=1`（父进程读回 `0xc0ffee...0000`） |
| 页内其余内容 | `private1 = pattern(1)` | `page_rest_intact=1`（子进程读回 `0xc0ffee...0001`） |
| 事件偏序 | 存在故障记录（单条后置见证） | trace 窗口内恰一次目标页故障 |

实测命令与结果（2026-09-15）：

- `cargo xtask linux-trace --regenerate` ⇒ `PASS linux-trace regenerate (kvm)`
- `cargo xtask linux-trace --regenerate --tcg` ⇒ `PASS linux-trace regenerate (tcg,thread=single)`，
  且归一化基线与 KVM 路径**逐字节相同**（加速方式不改变语义观测；`--tcg` 为 KVM 不可用时的回退）
- `cargo xtask compat` ⇒ `PASS compat cow-write-fault`
- `cargo test --workspace` ⇒ cosir 145、cosir_state 40、lean_backtrace 2、kernel 206、xtask 120
  全绿
- `cargo clippy --workspace --all-targets -- -D warnings` ⇒ 零告警
- `cargo xtask test` ⇒ BIOS run=1/2、run=2/2 六条 PASS（既有 22 条事件精确序列不变；本切片无内核改动）

## 4. 负例（保留，不得隐藏）

用 `--baseline` 指向篡改副本注入，判定器均报出对应 divergence 并以非零退出：

| 篡改 | 判定输出 |
|---|---|
| `error_code=0x7 → 0x3` | `ErrBitsMismatch { prima: 7, linux: 3 }` |
| `count=1 → 0` | `MissingFault { prima: 1, linux: 0 }` |
| `order_ok=1 → 0` | `OrderViolation { prima: true, linux: false }` |

另有 host 单测覆盖 `ClassMismatch`、`PageOffsetMismatch`、`ResolutionMismatch` 与基线解析错误。

## 5. 自检

- 归一化基线**不含**绝对地址/pid/时间戳（单测断言地址字面量未泄漏）。
- 生成的基线在写盘前用 `compat::parse_linux_trace` 自检，避免入库不可解析工件。
- 既有 22 条内核事件与 `cargo xtask test` 的 30s/无网络/精确序列语义未改动。
- `linux-trace`（无参数）可离线校验已提交基线；`regenerate` 才联网。

## 6. 证据边界与下一步

- **有界**：非同一二进制、无 ELF 加载器/LibOS，不满足白皮书 §13.4 的完整差分要求；只覆盖
  `cow-write-fault` 单场景的 PF/COW/解析语义。
- **不对称**：Prima 侧为单条后置见证，无法观测故障重数；判定 5 因此只要求"存在故障记录"。
  Linux 侧基线要求恰一次，反向更严。
- **未比较**：页表级结构、CR3/PCID、TLB shootdown、文件后端、多任务/POSIX API。
- **平台**：`regenerate` 依赖 Linux 宿主（`apt-get`/`dpkg-deb`/`gcc`/`busybox`/`cpio`/`gzip`）；
  差分离线可跨平台读基线。
- 下一步候选：完整 `COMPAT-TEST`（需 LibOS 与同一测试二进制）、追加 `not_present` 与 TLB
  flush 场景；S0-X1 的组织级第三方复核仍是阶段 0 关闭的唯一前置。
