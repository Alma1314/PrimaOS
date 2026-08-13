# exception 恢复细化：exc_resume 原语设计

> 文档类型：阶段 0 细化层历史设计
>
> 关联规范：[[COSIR 原语规范]]（§12.9、§12.10、§18.2 细化要求 #16、边界清单 #1/#2）
>
> 状态：已由 [`2026-08-12-refinement-audit-remediation-design.md`](2026-08-12-refinement-audit-remediation-design.md) §5 修订；本文已同步修订后的现行接口
>
> **2026-08-12 amendment：** 早期草案曾使用额外恢复地址参数和较短帧。该方案已废止，不可作为现行接口或实现依据。现行语义以 COSIR §12.9/§12.10、2026-08-12 纠偏设计和当前 `cosir/src/ops.rs` / `state.rs` 为准。

## 1. 目标、范围与证据边界

补齐 COSIR 细化层的异常恢复机制：内核只提供原子的“一帧恢复”，不解释恢复策略；重播故障指令、跳处理程序或终止进程由用户态 LibOS/运行时决定。它不改变异常源、订阅、递送、`ExceptionKind` 或 POSIR 安全边界。

本切片只提供 host 可执行状态模型及其 `MODEL-TEST`：

- 不实现真实 IDT/trap 入口，不连接 QEMU kernel；
- 不实现或声称真实 `iretq`；
- 不提供 `KERNEL-TRACE` 或 `REFINEMENT-PROOF`，形式化义务留给 BLK-001；
- 不新增 POSIR 一等原语。

## 2. 抽象语义与授权

### 2.1 现行原语签名

```text
exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset) -> ()
```

调用者以两个 Capability 和 Segment 内偏移指定授权源与恢复帧；恢复 RIP 是帧字段，不是独立参数。

### 2.2 纯验证前置条件

在修改任何状态前，实现必须完成全部检查：

1. source Capability 由调用者持有、active、指向 active `ExceptionSource`，并含 `ExcSubscribe`；
2. 调用者位于 source 的 active subscription 集合；
3. frame Capability 由调用者持有、active、指向 Segment，并含调用者写权限；Segment `kernel_readable`；
4. `frame_offset` 为 16-byte aligned，`frame_offset + 0xA0` 不溢出且完全位于 Segment bytes；
5. 调用者处于 `Faulted` 且有 pending exception；pending 的 source 与 kind 分别匹配 Capability 指向的 source 及其 supported kind；
6. pending kind 可恢复：`PageFault`、`GeneralProtectionFault`、`DivideError` 或 `InvalidOpcode`。`DoubleFault` 等不可恢复异常返回 `UnrecoverableFault`，Task 保持 `Faulted`；
7. 解码后的 RIP/RSP 为 canonical 用户地址，RIP 位于可执行用户映射，RSP 位于可写用户映射，CS/SS 符合用户态 selector 策略，RFLAGS 不含禁止的特权或保留位。

错误域为 `InvalidExcSource`、`MissingRight`、`NotSubscribed`、`InvalidFrameAddr`、`InvalidFrame`、`NoPendingException`、`UnrecoverableFault`。任一失败都保持完整 State 不变，包括 Task、pending exception、capabilities、subscriptions、Segment bytes、kernel frame 和 Audit log。

### 2.3 严格成功提交顺序

全部验证成功后，按固定顺序原子提交：

```text
clear consumed Segment frame bytes
→ clear pending exception
→ save validated kernel resume frame
→ append TaskResumed Audit event
→ Faulted → Ready
```

`TaskResumed` 记录 actor、source ID、source/frame Capability ID、frame Segment ID、offset 与帧中 RIP。`schedule` 只负责后续 `Ready → Running`，不会从 `Faulted` 隐式恢复或重播故障指令。

## 3. x86_64 `SretFrame` wire format

Segment 保存实际 bytes。帧采用 16-byte alignment、固定 `0xA0`（160）字节、little-endian u64 编码：

```text
struct SretFrame align 16, size 0xA0 (160)
  0x00  rax:    u64
  0x08  rbx:    u64
  0x10  rcx:    u64
  0x18  rdx:    u64
  0x20  rsi:    u64
  0x28  rdi:    u64
  0x30  rbp:    u64
  0x38  rsp:    u64
  0x40  r8:     u64
  0x48  r9:     u64
  0x50  r10:    u64
  0x58  r11:    u64
  0x60  r12:    u64
  0x68  r13:    u64
  0x70  r14:    u64
  0x78  r15:    u64
  0x80  rflags: u64
  0x88  rip:    u64
  0x90  cs:     u64
  0x98  ss:     u64
```

内核从 Segment bytes 解码并验证完整帧。成功时将帧复制到内核持有的 `kernel_resume_frame`，随后清零 Segment 中已消费的 `0xA0` 字节；失败时两者均不改变。

## 4. 状态模型工件

host Rust 模型位于：

```text
cosir/src/state.rs  // Capability、subscription、Segment bytes、SretFrame、pending/kernel frame、Audit
cosir/src/ops.rs    // raise_exception、exc_resume、schedule 状态迁移
cosir/src/tests.rs  // host MODEL-TEST
```

模型检查授权 mutation、wire format、RIP/RSP/CS/SS/RFLAGS 验证、不可恢复异常、重复消费、严格成功提交和失败完整 State 原子性。它只证明该 host 模型在这些输入上的行为，不证明真实 kernel/trap 返回，也不解除 BLK-001。

## 5. 层归属

`Faulted`、pending exception、kernel resume frame 与 x86_64 帧布局是 COSIR 细化状态；POSIR 不观察这些内部表示。`exc_resume` 的授权和状态转换由 COSIR §12.10 定义。其他架构必须另行定义各自帧 ABI，并经反例检验；本文不据此声称跨架构实现已经成立。
