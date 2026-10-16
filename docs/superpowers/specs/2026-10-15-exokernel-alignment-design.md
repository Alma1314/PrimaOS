# Exokernel 对齐：规划与实现逻辑优化设计（提案）

> 文档类型：设计提案（非权威）
>
> 日期：2026-10-15
>
> 基线：`bba61ef`
>
> 状态：待评审。本文不改变 POSIR/COSIR 的 observable semantics；任何进入
> `Prima OS.md`、`COSIR 原语规范.md` 或阶段跟踪的语义/路线变更，仍须按维护规则先评审、
> 后同步。

## 1. 目的

白皮书在 §16 把 Exokernel 列为设计来源之一：**向用户态暴露受保护资源机制，而不是把资源
管理策略和 personality 语义放进内核**。当前阶段 4 的实现已经暴露出相反趋势：内核不仅提供
隔离/授权/执行机制，还逐步理解了服务名称、服务注册表、服务协议、服务 ELF 映像和测试见证
字段。该趋势会侵蚀 H1（机制表达力）与 H3（小型可信机制层）。

本文的目标是给出一条可执行的纠偏路线：用 Exokernel 的 **“保护而非管理、secure binding、
visible revocation/abort、LibOS 掌策略”** 作为规划门槛和实现边界，优先把**服务装载、
服务注册与测试见证**移出内核，而不是继续在现有内核 witness 结构上叠加服务特性。

## 2. 对齐原则

1. **保护而非管理**：内核只强制不可绕过的保护、隔离和授权；资源分配策略、命名、日志、
   存储语义由 LibOS/用户态服务决定。
2. **secure binding 优先**：设备、页帧、CPU 时间等资源通过 capability 绑定给用户态持有者；
   内核不应为某个服务/设备发明专属快速路径。
3. **可见回收与 abort**：资源授权必须能被可观察地撤销；被撤销者必须有一条明确的 abort
   路径，而不是依赖内核知道其内部协议。
4. **OS personality 不得进入机制层**：内核代码不得出现 `"alpha"`、`"log"`、`"ioprt"` 等
   personality 名称，不得解析服务协议字段，也不得依赖 `prima-user` 的二进制。
5. **证据可去内核化**：内核只见证通用对象不变量（能力是否有效、任务是否创建、端口是否在
   区间内）；服务语义见证由用户态事件流/外部 harness 产生，不由内核读共享段魔法偏移。

## 3. 当前偏差证据

以下为基线 `bba61ef` 的静态取证（含注释与测试命中，作为趋势量化而非精确 TCB 分类）：

| 观察 | 位置 | 后果 |
|---|---|---|
| 内核构建期依赖用户态服务二进制 | `kernel/Cargo.toml` 依赖 `prima-user` 的 `artifact = "bin"` | 依赖方向反转为“机制层依赖 LibOS 产物” |
| 内核用 `include_bytes!` 嵌入 7 个用户 ELF | `kernel/src/user.rs` 顶部 | 内核知道有多少服务、哪个服务对应哪个映像；ELF 膨胀内核映像 |
| 内核定义服务名与行为码 | `SERVICE_NAME_ALPHA/BETA/LOG/NS/STORE/DRIVER` 等 | personality 名称进入机制层 |
| 内核包含大量服务协议 tag/偏移/期望值 | `SERVICE_*`、`SERVICEMGR_*`、`SERVICE_REPLY_*` 等 | 内核成为服务协议的共同维护者，测试夹具随服务变更 |
| 内核包含约 1458 行 `user_code()` 机器码生成器 | `kernel/src/user.rs:619` 起 | init/编排逻辑被编译进内核，而不是从用户态镜像装载 |
| 机制层 crate 体积 | `kernel/src` 约 27133 行：`bootstrap.rs` 10876、`user.rs` 4430、`syscall.rs` 3587、`protocol.rs` 1238 | “小型机制层”的绝对规模增长被服务脚手架掩盖 |
| 内核读取服务共享段固定偏移生成 witness | SPEC-106/110/112/113/114 的内核侧见证 | 内核与服务协议形成 ABI 耦合，内核侧证据无法证明用户态自主性 |
| `cargo xtask tcb` 只报占比 | `trusted/mechanism` 行数比 | 当机制 crate 增加服务脚手架时，占比反而可能下降，不能单独约束 H3 |
| 最新 SPEC-114 自述 | 报告“未决 3”：驱动仍借“服务表第 6 项”接入 | 尚无通用的 ramdisk/驱动装载/发现机制 |

此外，`grep` 在 `kernel/src` 中对服务/协议相关标识符约命中 310 处（含注释与测试）。这不是
说所有这些行都进入最终 TCB，而是说明**内核知识面与服务生态数量正相关**；每新增一个服务，
内核常量、见证字段、ELF 嵌入和 xtask 夹具都会同步增长。

## 4. 优化后的规划逻辑

### 4.1 每个切片增加 “Exokernel Impact Note”

所有新增机制、用户态服务协议、设备路径和“临时 witness”在设计/计划文档中必须回答：

1. 它保护的是哪类物理/逻辑资源？
2. 它是否用既有 capability + IPC 表达 secure binding？
3. 如果移除内核支持、改由 LibOS 在既有机制上实现，安全性/原子性是否仍然成立？
4. 资源如何被可见地撤销？revocation 后对端如何 abort？
5. 内核是否因此获得任何 OS personality 专属对象、名称、协议或测试布局？

任一项答案违反 Exokernel 原则时，按 H1/H3 反例登记；不能以“性能/测试方便”静默加入。
`ISSUE-EXO-001` 将跟踪当前已存在的耦合。

### 4.2 用 Exokernel 垂直切片替换“服务表叠加”

阶段 4 后续切片按以下模板划分，而不是继续往 `SERVICE_TABLE` 增加条目：

```text
物理资源（页帧 / IOPort / MMIO / IRQ / 时间片）
  → 内核生成 capability（仅保护与记账）
  → LibOS / 驱动任务通过 secure binding 绑定
  → 用户态协议完成实际操作
  → 撤销/销毁，用户态 abort 与资源归还
```

每条切片必须同时包含正向路径、越权负例、撤销/销毁路径和资源守恒证据。服务发现、服务名
解析、注册表、日志/存储语义和驱动协议不得以“第 N 项服务”的形式写进内核。

### 4.3 TCB 指标改为“绝对规模 + 知识面”双门禁

保留现有 `cargo xtask tcb` 的占比，同时增加：

- `mechanism_lines` 的绝对值和变化量；
- `kernel` 中 `include_bytes!` 用户 ELF 数（目标 0）；
- `kernel/Cargo.toml` 是否依赖 `prima-user`（目标否）；
- 服务名/协议常量命中数（目标 0，或随重构单调下降）；
- 内核源码中直接读取服务共享段固定偏移的 witness 数（目标 0）。

占比下降但机制层绝对行数和服务知识面上升时，H3 应视为**恶化**而非改善。

### 4.4 测试证据的生产去内核化

内核侧断言只保留通用不变量，例如：

- 创建了若干任务、其中若干持有效 capability；
- `Device(IOPortRange)` 落在/不落在授权区间；
- capability 被撤销后对端 `send/recv/map` 失败；
- 页帧/映射守恒。

服务名、协议、状态机、返回值和对端选择等证据由用户态产生，经通用调试通道或共享 trace
段输出，由 `xtask` 解析。这样测试不再要求内核理解服务协议，也能让 QEMU/CI 测试真实
LibOS 行为。

## 5. 优化后的实现逻辑

### EXO-1：通用 boot image / ramdisk 装载

> **状态（2026-10-15）**：已实现并验证——〔SPEC-115〕
> `docs/superpowers/reports/2026-10-15-exo1-boot-image-report.md`。内核已无服务 ELF
> `include_bytes!` 和 `prima-user` 构建依赖；服务名/协议/witness 仍留 EXO-2/3。

- 使用 `bootloader_api = 0.11.17` 已支持的 `BootInfo.ramdisk_addr/ramdisk_len`，并在
  `BiosBoot::set_ramdisk` / `UefiBoot::set_ramdisk` 侧构建镜像。
- `xtask` 把 `prima-user` 的服务/init 二进制打包为版本化的 boot image（manifest 可包含
  名称、入口偏移、权限需求，但**由用户态解析**）。
- 内核只把 ramdisk 物理区间标记为保留帧，并为 init 铸造一个只读 `MemorySegment`
  capability；内核不解析 manifest、不按名索引服务、不 `include_bytes!` 具体 ELF。
- 删除 `kernel/Cargo.toml` 对 `prima-user` 的依赖，改成 `prima-bootimg`/xtask 构建步骤。

验收：`rg 'include_bytes!' kernel/src` 不再命中服务 ELF；`cargo xtask test` 从 ramdisk
启动同一组服务；`frames_ready` 把 ramdisk 帧纳入保留/不交性记账。

### EXO-2：用户态 init/loader 承担编排

> **状态（2026-10-16）**：进行中。已落 **EXO-2A**（SPEC-116，目录无名字）、
> **EXO-2B**（SPEC-117，目录格式提升为 `prima-abi` 共享常量）、**EXO-2C**
> （SPEC-120，init 机器码生成器移出内核，改由 host 生成、ramdisk opaque ELF 装载）与
> **EXO-2D**（SPEC-121，带 opaque `kind` 的通用 manifest + boot entry ABI；
> `prima-initgen` 不再镜像 capability 句柄）；
> 见 `docs/superpowers/reports/2026-10-15-exo2a-name-free-service-directory-report.md`、
> `docs/superpowers/reports/2026-10-15-exo2b-exo4a-report.md`、
> `docs/superpowers/reports/2026-10-16-exo2c-init-loader-report.md` 与
> `docs/superpowers/reports/2026-10-16-exo2d-manifest-boot-abi-report.md`。
> 共享段 witness 与真正的运行时 ring3 loader/trace 迁移仍未完成。

- 新建或强化真正的 ring3 `init`/`prima-servicemgr`：从 ramdisk 读 manifest，创建服务
  Task、映射代码/栈、建立注册表、按名解析和路由。
- 内核只保留 `task_create`、`memseg_*`、`send/recv`、通用 exception/device cap 机制。
- 删除 `kernel/src/user.rs` 中的 `user_code()` 生成器及 `SERVICE_TABLE` 相关机器码；
  init 的启动信息通过通用 entry ABI 和 capability 传递。

验收：内核不再含服务名；服务数量增加不改变 `kernel/src` 的服务常量；init 崩溃/重启路径
有用户态策略。

### EXO-3：通用 trace / witness 通道

> **状态（2026-10-16）**：进行中。已落 **EXO-3A**（SPEC-118，内核不再解释服务名）、
> **EXO-3B**（SPEC-119，marker/reply/协商期望值移出内核，由 `xtask` 校验）、
> **EXO-3C**（SPEC-122，协议 tag 常量与 witness tag 断言移出内核）、
> **EXO-3D**（SPEC-123，通用用户态 trace 通道；`runtime_hello_ready` 已改为内核
> 只透传 raw trace 记录）与 **EXO-3E**（SPEC-124，`service_reply_ready` 的
> `recv`/`echo` 已改由服务管理器 trace 上报）；
> 其余 `service_negotiated`/`service_selected`/`servicemgr` 共享段 witness 偏移/内核
> 读回下沉仍待完成。见 EXO-3A/3B/3C/3D/3E 报告。

- 定义内核无关的 `exo_trace` 或调试 ring：用户态可写事件字节流，内核只提供共享段/串口
  等资源，不解释内容。
- `xtask` 直接解析该 trace，把现由 `ServicemgrWitness`、`service_reply_witness()` 等
  读取的字段迁走。
- 内核 witness 仅保留对象句柄有效性、cap 授权、帧守恒、任务状态等通用不变量。

验收：删除 `SERVICEMGR_*` 偏移常量后，服务语义测试仍可判定；内核源码不再读服务共享段
固定偏移。

### EXO-4：secure binding 与可见撤销

> **状态（2026-10-15）**：EXO-4A 已落——Device 对象有 `dev_destroy` 与可见撤销；
> MMIO/IRQ/DMA、IOMMU 与完整 abort 协议仍未完成。见 SPEC-117 报告。

优先于继续增加服务/驱动数量，完成：

- `Device(IOPortRange)` 的硬件路径（IOPL/TSS bitmap）或明确的内核代执行边界；
- `dev_destroy`、IRQ/MMIO/DMA/IOMMU 的最小授权与回收；
- 撤销/销毁后用户态任务的 abort 语义；
- 页帧、设备、端口资源的守恒断言。

这与 Exokernel 的 secure binding/visible revocation/abort protocol 直接对应，也是阶段 4
“故障服务不破坏无关应用域”和阶段 7 资源配额的前置。

### EXO-5：机制基线再评估

> **状态（2026-10-15）**：已完成一次审查（DOC-008 / EXO-5 报告），结论为保留机制、
> 收紧服务语义；不直接修改规范。

在 EXO-1..4 完成后，用同一“四问”重新评估 Namespace、Audit、Timer、WaitObject 等
跨 personality 机制：

- 哪些能用 `Capability + Send/Recv + Exception + Segment` 在用户态构造？
- 哪些不留在内核会破坏保护、原子性或测试可停止性？
- 哪些虽可留在内核，但不应成为服务注册/服务协议的一部分？

结论写入白皮书/规范评审，不得由本文直接改语义。任何下移都应先补用户态替代路径和等价
证据，再删除内核支持。

## 6. 验收条件

1. `kernel/Cargo.toml` 不再依赖 `prima-user`；内核可独立编到 `x86_64-unknown-none`。
2. `kernel/src` 中服务 ELF `include_bytes!` 为 0；服务名/协议常量重构后为 0 或单调下降。
3. 内核侧 witness 不再按服务协议偏移读取用户内存；服务语义证据由通用 trace + xtask 判定。
4. `cargo xtask test`、`cargo xtask lint`、`cargo xtask tcb`、`cargo test --workspace` 全绿，
   且现有 QEMU 事件流中被迁移的字段有逐条登记与等价替代。
5. `cargo xtask tcb` 同时报告机制层绝对行数、内核-服务耦合计数，并在 CI 中对增长设阈值。
6. 至少一条端到端 secure binding 切片包含：授权成功、越权拒绝、资源撤销、abort/归还与守恒。

## 7. 非目标

- 不在本文中直接删除 POSIR 机制或更改 observable semantics；
- 不要求所有服务立即迁出，允许按 EXO-1→EXO-3 逐步替换；
- 不把“代码少”本身当作正确性证明；抽象不变量、细化关系和 QEMU 证据仍按既有规则分级；
- 不放弃阶段 4/5 的兼容性目标；本提案只调整**实现位置**和**证据生产位置**。

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| bootloader ramdisk 在 BIOS/UEFI 行为不一致 | 两个 profile 都做启动门禁；保留内核内嵌过渡路径并逐步下线 |
| ramdisk 帧未进现有 `build_inventory` 守恒 | 把 ramdisk 区间作为保留区显式登记，`frames_ready` 加不交性断言 |
| 用户态 loader 成为新的单点策略 | 它只持所需 cap；内核仍可验证每个任务的最小权限 |
| witness 迁移导致既有事件回归 | 按项目维护规则逐条登记事件变化，保留旧结论的适用范围 |
| 重构期间暂停阶段 4 交付 | 先做 EXO-1/EXO-3 的“不改变外部服务行为”切片，再逐步替换 EXO-2/4/5 |

## 9. 与现有路线的关系

本提案不新增阶段，也不改变阶段 4 的退出条件。它把阶段 4 剩余的“服务/驱动数量”工作重新
排序为“先去除内核服务知识，再扩展服务生态”。若评审认为当前阶段 4 的服务表实现是必要
脚手架，也应把它登记为**临时过渡**，并设定 EXO-1..3 的退出条件，避免临时脚手架固化为
HexArc 的永久机制层。
