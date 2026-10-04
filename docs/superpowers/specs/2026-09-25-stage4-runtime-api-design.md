# SPEC-084 设计：阶段 4 Prima 原生运行时 API 与版本规则

> 状态：探索设计 + 原型实现；尚未验收
>
> 目标：拆解 S4-D1「原生运行时 API 与版本规则」
>
> 前置参考：阶段 3 的 Namespace、Timer 与 `audit(cap)` 实现报告（SPEC-081–083）

## 1. 目标与边界

阶段 3 已提供 Capability、Task、Segment、IPC、Namespace、Timer 和 Audit 的 ring3 调用路径。本切片把这些底层操作收敛成原生应用可依赖的用户态接口约定，回答三个问题：应用依赖哪个 API 版本、服务协议怎样演进、每项操作从哪里取得权限。

本设计定义 S4-D1 的 API 与版本边界，并由 `prima-runtime/` 提供初始代码原型；它尚未实现用户态服务管理器、日志服务、存储服务、驱动或应用，也不冻结当前实验性 syscall 编号为公开 ABI。阶段 0–3 的退出条件仍受组织级独立复核门槛约束；本切片属于可并行开展的阶段 4 探索。

当前原型已增加 `prima-abi` 内部 crate，由内核分派与 `prima-runtime` 共用实验性 syscall 编号。该共享仅防止两份映射漂移，不把调用号升级为公开或稳定 ABI。

## 2. 设计约束

1. **无隐式 Authority**：运行时不持有全局根 Capability，不通过全局单例替应用取得资源。对象操作必须显式接收调用方持有的 Capability。
2. **内核语义保持中立**：API 面向用户态机制和服务协议；POSIX/Linux personality 不进入运行时核心接口。
3. **错误可判别**：保留拒绝、对象失效、资源耗尽、阻塞取消等不同结果；不得把所有失败折叠成 `false`、空值或 panic。
4. **内存安全不等于授权**：Rust 类型可以避免句柄混用和重复释放等客户端错误，但 Capability 是否有效及其权限仍由内核判断。
5. **同步语义明确**：每个调用标明是否可能阻塞、是否可取消以及返回时资源的所有权；异步适配不得改变底层操作的授权或原子性。

## 3. API 分层

### 3.1 Runtime crate

创建独立的 `prima-runtime` 用户态 crate，提供 Rust 源码 API。它负责把原始句柄和 syscall/IPC 传输封装为带类型的调用，不负责模拟内核授权或把策略搬入内核。

初始模块候选：

| 模块 | 对应能力 | 当前细化落点 |
|---|---|---|
| `cap` | 显式持有、派生、丢弃 Capability | `cap_derive` / `cap_drop` |
| `task` | 创建、让出、销毁 Task | Task 原语 |
| `memory` | 创建、映射、解除映射和销毁 Segment | MemorySegment 原语 |
| `ipc` | 发送、接收消息及显式传递 Capability | Send / Recv |
| `namespace` | 解析服务名并取得受委托 Capability | Namespace 原语 |
| `time` | 读取时间、设置/取消/销毁 Timer | Timer 原语 |

模块按实现成熟度逐步开放。首版不承诺六个模块一次全部可用；每个稳定模块都要列出支持的错误和阻塞语义。

### 3.2 传输层

Runtime 面向一个窄的 `Transport` 接口。传输层可以由当前 x86_64 syscall 入口实现，也可以由用户态服务消息实现；上层 API 不暴露 syscall 编号、寄存器分配或 `int 0x80`/`syscall` 选择。

传输层仅负责编码、调用和解码。初始 crate 将 `SyscallTransport` 作为公开 trait，以支持不同平台适配和 host 侧模拟；在 `0.x` 阶段允许按 SemVer 规则调整。Capability 检查仍由内核执行，服务自身的访问策略由服务端实现。`Transport` 不应提供不带 Capability 参数的通用“执行特权操作”接口。

### 3.3 服务协议

服务以 Namespace 中显式委托的 Capability 发现。客户端先选择协议主版本对应的端点，再执行消息握手。握手字段为 `major`、`minor`、`features`、`required_features` 和 `max_message_size`；请求头包含 `major`、`minor`、`opcode`、`flags`、`request_id` 和 `payload_len`。初始 wire format 使用 `PRIM` 魔数，消息头固定 20 字节、Hello offer 固定 24 字节，整数按 little-endian 显式编码；不得把 Rust 结构体内存布局直接当作稳定 wire format。

初步命名约定：`services/<name>/v<major>`，例如 `services/log/v1`。路径只是 Authority 的发现入口；取得端点 Capability 后，后续调用必须使用该 Capability，不能因路径可解析而推导更广权限。

## 4. 版本规则

### Rust 源码 API

- 初期 crate 版本使用 `0.y.z`；在 `1.0.0` 前允许显式记录的源码兼容调整。
- 达到 `1.0.0` 后遵循 SemVer：兼容新增走 minor，兼容修正走 patch，破坏性源码变化升 major。
- Runtime crate 的 SemVer 只约束 Rust 源码接口，不承诺 Rust 编译器 ABI、二进制布局或 syscall 编号稳定。

### 服务 wire protocol

- `major` 表示不兼容协议；同一 major 内，`minor` 只增加可选字段、操作或能力特性，旧客户端必须能忽略未知可选项。
- 删除/重解释字段、改变已有 opcode 的含义或改变已有操作的权限/失败语义，必须提升 major。
- 客户端按 `major` 选择端点；双方在握手中协商共同支持的 `minor` 和 feature 子集。无共同主版本、必需 feature 缺失或消息尺寸超限时，握手失败且不执行请求。
- 同一服务可并行发布多个主版本端点；迁移由服务提供方完成，内核不理解服务版本。

### Capability 与资源版本

Capability 句柄不带 API 版本。句柄只表示内核管理的 Authority；协议版本由服务端点和消息握手表达。关闭 Rust wrapper 不自动销毁 Capability，除非 API 明确执行 `cap_drop`；派生、发送、接收、映射和销毁等操作的所有权变化必须在函数签名和文档中写明。

## 5. 错误与调用约定

Runtime 对稳定 API 返回结构化错误，至少区分：

- `Denied`：Capability 不足或调用者不匹配；
- `Stale`：句柄格式有效但其槽位代次已失效；
- `NotFound`：对象或 Namespace 项不存在；
- `InvalidInput`：参数不满足 API 前置条件；
- `ResourceExhausted`：内核或服务容量不足；
- `Interrupted` / `Cancelled`：阻塞操作被明确中断或取消；
- `Transport`：调用路径或消息传输失败。

这是运行时错误分类，不改变 COSIR 的具体错误集合。适配层保留原始内核错误码，只有在可证明一一映射时才折叠到同类错误；日志和诊断接口应能查看原始来源。

## 6. 安全与兼容性判据

设计进入实现前必须满足：

1. 每个特权 API 都显式接收所需 Capability，且无默认全局 Capability；
2. 两个不兼容服务主版本可在不同 Namespace 路径并存；
3. 未知可选 feature 可忽略，未知必需 feature 会在执行请求前失败；
4. 消息中的 Capability 转移与 payload 原子性沿用 IPC 原语语义；
5. 生命周期调用清楚标注 Capability 消费、借用或移动，不由 Rust Drop 隐式推断内核效果；
6. syscall 编号和 Rust 类型布局均不出现在稳定服务 wire protocol 中。

首轮验收建议以 host 侧协议编码/解码和错误映射检查开始，再接入真实 ring3 客户端；通过 host 模型不能作为硬件 syscall、调度、隔离或安全证明。

## 7. 未决项

1. Runtime 的异步模型应从阻塞调用起步，还是直接以 `Future` 为公共接口？建议先冻结阻塞语义和取消边界，再提供可选 async adapter。
2. 跨版本服务迁移如何处理已经发出的 Capability 和在途请求？首版应禁止原地重解释，并让旧端点继续服务到租约/显式关闭完成。
3. 结构化错误与现有负 errno 的精确映射表，留给后续运行时实现切片补全。

本切片不宣称阶段 4 已实现，不勾选 S4-D1 或任何阶段退出条件。
