# 切片 P 设计：Linux trace 与有界 COMPAT-TEST（`cow-write-fault`）

> 文档类型：真实内核 + 参考系统差分设计（切片 P — 冻结切片的最后节点）
>
> 日期：2026-09-15
>
> 项目：Prima OS（`kernel/`、`xtask/`，参考 Linux guest）
>
> 状态：已获用户确认（2026-09-15）

## 0. 定位与前置

垂直切片固定为 **COW/page fault → handler → `exc_resume` → TLB → Linux trace**
（[Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) 研究停止条件段）。前四节点已全部落地于真实内核：

- SPEC-040/041：真实 vector-14 #PF 捕获 + 硬件 error code 译码分类；
- SPEC-042：验证式 `exc_resume` + 真实 `iretq` 恢复跳板；
- SPEC-043：自建 4 级页表、真实陈旧 TLB 现场与 `invlpg` 刷新；
- SPEC-044：内核侧真实 COW（帧字节复制 + 换映射 + 重试原指令）；
- SPEC-045：ring3 用户态 COW（CPL3 写只读用户页 → `err=0x7` → 内核 COW → 回 CPL3 重试成功）。

本切片推进最后一个节点：为该路径建立**可审查的 trace 对应**，并与选定 Linux 行为做差分。
此前该节点被记为"本机尚不具备"，本轮已实测打通参考侧环境。

### 0.1 已确认的关键决策

与用户逐项收敛的决策（全部已确认）：

1. **只做单场景 `cow-write-fault`**。它是与 Prima 现存 ring3 见证最同构的场景；不追加
   `not_present` 缺页场景（需要给 Prima 增加新的用户态触发点），也不做 `tlb_flush` tracepoint
   对比（其格式随内核版本变动大）。
2. **提交归一化基线 + 离线比对**。联网只发生在 `linux-trace --regenerate`；日常差分为离线读
   已提交基线。内核 deb 不入库，只入库归一化 trace 与来源声明（含 deb SHA512）。
3. **差分作为独立命令，不并入 `cargo xtask test`**。标准套件保持 30s、无网络、既有 22 条事件
   的精确序列断言不变。
4. **Prima 侧零内核改动**。现有 `user_cow_ready`（`kernel/src/protocol.rs:186`）已携带
   `va;err;class;shared0;private0;private1;receipt`，字段级覆盖本场景契约，故既有证据
   **逐字节不变**，无回归面。

## 1. 场景契约 `cow-write-fault`

契约（机制中立，只约束可观察面）：

1. 存在一个 present、可写 VMA、用户可访问的页，初值 `pattern(i) = 0xC0FFEE00_0000_0000 | i`；
2. 两侧各自使该页处于"present 且只读"的 **TE/PTE 级**状态；
3. 用户态对该页写 8 字节 `VAL = 0x5A5A_1234_DEAD_BEEF`；
4. 期望：写保护故障，error code 的 P|W|US 三位全 1（`0x7`）；
5. 系统建立私有副本，重试写成功；
6. 读回 `VAL`；**原副本仍为 `pattern(0)`**；页内其余内容仍为 `pattern(1)`。

**两侧的实现（非同一二进制，同一契约）**：

| 侧 | "只读 present" 的建立 | 故障点 |
|---|---|---|
| Prima | ring3 任务将 `USER_X_VA` 映射为 user + 只读（`kernel/src/user.rs:535-543`） | 用户 `mov [rdx],rax`（3 字节） |
| Linux | `mmap(MAP_PRIVATE\|MAP_ANONYMOUS\|MAP_POPULATE)` + 填充后 `fork`，父子 PTE 被置只读、VMA 仍可写 | 子进程 `page[0] = VAL` |

Linux 侧选择 `fork`-COW 而非 `mmap` 只读文件/`mprotect`：`mprotect(PROT_READ)` 改的是 VMA 权限，
子进程写会直接 `SIGSEGV` 而非 COW；`MAP_PRIVATE` 文件映射首次写常为 `not_present`（`err` 不稳）。
`fork` 使页 present、VMA 可写、PTE 只读，与 Prima 语义一一对应（首轮实测即以 `SIGSEGV`=0xb 暴露
了 `mprotect` 方案不可行）。

## 2. 组件与文件级改动

### 2.1 参考 Linux guest（新增）

- `xtask/assets/linux/cow_write_fault.c`：静态编译的 guest 程序，实现上述契约；以
  `LINUX|event=<name>;value=<hex>;ok=<0|1>` 归一化行写串口。**程序自证语义结果，tracepoint 只证
  故障事实**（错误码、地址）。
- `xtask/assets/linux/init`：busybox init；挂 `proc/sysfs/tracefs`、开启
  `exceptions:page_fault_user`、运行程序、dump trace buffer、`poweroff -f`。tracepoint 缺失时打印
  `LINUX|error=no_page_fault_tracepoint`，由宿主侧判失败（不得降级为通过）。
- `xtask/src/linux.rs`：pinned 内核 deb（`linux-image-6.8.0-31-generic`，SHA512 强制校验）→
  `dpkg-deb -x` 取 vmlinuz → `gcc -static -O0 -no-pie` → busybox initramfs（`find | cpio -H newc |
  gzip`）→ QEMU `-kernel/-initrd -append "console=ttyS0 rdinit=/init quiet"`（独立 180s 超时）→
  归一化。默认 KVM（存在 `/dev/kvm` 时），`--tcg` 强制软件模拟。

### 2.2 差分比较器（新增）

- `xtask/src/compat.rs`：观测 schema（`CowObservation`）、`parse_prima`/`parse_linux_trace`、
  `compare_cow_write_fault`、`Divergence` 枚举，含 14 条 host 单测（1 正例 + 各类 divergence
  反例）。比较是**语义化**的，不比较整段日志字节。

### 2.3 CLI（修改）

- `xtask/src/main.rs`：`<build|run|test|compat|linux-trace>`。
  `compat [--baseline <path>]` 跑一次 Prima 镜像并与基线比对；
  `linux-trace` 离线校验基线可解析，`linux-trace --regenerate [--tcg]` 联网重建。
- `xtask/src/qemu.rs`：抽出通用捕获运行器 `run_captured_with(args, timeout)` 并暴露
  `run_bios_capture`（返回已通过退出码与事件序列校验的串口输出）。**BIOS 路径、退出码常量与
  30s 超时不变**。

### 2.4 基线工件（新增，提交入库）

- `xtask/tests/fixtures/linux-trace/cow-write-fault.trace`（归一化语义字段，不含绝对地址/pid/时间戳）
- `xtask/tests/fixtures/linux-trace/PROVENANCE.toml`（内核包/deb SHA512/QEMU 版本/accel/启动参数/日期）

## 3. 差分判定规则（语义化，允许规范允许的非确定性）

| # | 判定 | 对应 [Prima OS](../../../Prima%20OS.md) §13.4 项 |
|---|---|---|
| 1 | `err & 0x7 == 0x7` 两侧成立 | 返回值/错误语义 |
| 2 | 分类名一致（Linux 侧由 error code 按同一规则推导） | 错误语义 |
| 3 | 只比较页内偏移（`addr % 4096`），不比较绝对地址 | 允许非确定性 |
| 4 | 写生效 + 原副本不变 + 页内其余内容完好，两侧均成立 | 内存结果 |
| 5 | 事件偏序 `fault ≺ resolve ≺ retry_ok`（两侧均有故障记录） | 允许的事件顺序集合 |

Divergence 枚举：`MissingFault` / `ErrBitsMismatch` / `ClassMismatch` / `PageOffsetMismatch` /
`ResolutionMismatch` / `OrderViolation`。

## 4. 证据边界（不夸大）

- 本切片是**有界差分**，不是 [Prima OS](../../../Prima%20OS.md) §8.3/§13.4 的完整 `COMPAT-TEST`：后者要求 ELF 加载器、
  Linux 初始栈与 Prima LibOS 上运行**同一二进制**，当前阶段 5/6 均未开始。故证据名目为
  `KERNEL-TRACE` + **`COMPAT-TEST (bounded: cow-write-fault)`**，**不勾选 S6-X2**。
- Prima 侧的比较输入是**后置见证事件**（单条 `user_cow_ready`），无法观测故障重数；判定 5
  因此只要求"存在故障记录"，不宣称"目标页恰故障一次"。Linux 侧基线额外要求目标页恰一次
  （`normalize` 在 >1 时报错），避免把非冻结场景混入基线。
- 不比较：页表级结构、CR3/PCID、TLB shootdown、文件后端、绝对地址、进程/时间维度。
  TLB 节点由判定 5 间接覆盖——重试成功（两侧 `receipt`/`write_retried_ok`）要求重试时观测到
  已刷新的翻译，否则会再次 fault 而无法完成。
- 不修改 `COSIR 原语规范.md` 与 `Prima OS.md` 的语义；不宣称 `REFINEMENT-PROOF`。
- 若差分要求修改已冻结的 POSIR observable semantics 或新增架构专属原语，按 tracker 既有停止
  条件停止并记为 H1/H3 反例。

## 5. 测试面

- `cargo test -p xtask`：比较器正例 + 每类 divergence 反例 + guest 归一化反例。
- `cargo xtask linux-trace --regenerate`：联网生成基线并自检（生成的基线必须能被比较器解析）。
- `cargo xtask compat`：端到端差分 `PASS compat cow-write-fault`；`--baseline` 注入篡改副本时
  必须报出对应 divergence（负例保留）。
- `cargo xtask test`：既有 22 条事件精确序列与字段不变（本切片无内核改动）。
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`。

## 6. 范围未含

- 完整 `COMPAT-TEST`（需 LibOS/ELF 加载器与同一测试二进制）；
- Linux 侧其它场景（`not_present` 缺页、TLB flush、多任务/POSIX API 差分）；
- Windows/macOS 宿主支持的重生成路径（`regenerate` 依赖 Linux 宿主工具链）；
- 基线自动过期/重生成策略（基线随内核 pin 固定，重生成是显式人工动作）。
