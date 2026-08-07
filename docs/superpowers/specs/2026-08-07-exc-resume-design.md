# exception 恢复细化：exc_resume 原语设计

> 文档类型：阶段 0 细化层设计
>
> 关联规范：[[COSIR 原语规范]]（§12.9 恢复路径、§18.2 细化要求 #16、边界清单 #1/#2）
>
> 状态：已获批准，待实现

## 1. 目标与范围

补齐 COSIR 细化层的异常恢复机制。§12.9 目前把 faulted Task 建模为 `Ready`，但"如何从异常返回执行"被显式留白（异常帧格式、`iretq` 返回路径、`exc_resume` 参数与语义）。本设计关闭该缺口。

职责分离是总原则：

- **内核只提供原子的"恢复异常上下文帧"原语**，不解释异常语义；
- 真正的返回目标（重播故障指令、跳处理程序、进程退出）由用户态 LibOS/运行时自管；
- 不改变现有异常源/订阅/递送语义、`ExceptionKind` 和 POSIR 安全边界。

产出：

1. COSIR 规范补充：`exc_resume` 原语、异常帧格式、缺省恢复路径、`Faulted` 细化态、错误码；
2. 一个独立主机可执行状态模型探针（Rust），验证状态转换单调性与错误路径；
3. 同步细化要求 #16 与边界清单 #1/#2。

明确不做：

- 不实现真实 IDT/trap 入口、不连接 QEMU kernel；
- 不做 Lean/Coq 形式化（留给 BLK-001）；
- 不新增对 POSIR 可见的一等原语，不引入架构耦合到抽象正文。

## 2. 抽象语义与状态建模

### 2.1 原语签名

```text
exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset, resume_vaddr) -> ()
```

调用者是当前 Running Task，且分别持有 `exc_source_cap` 与 `sret_frame_seg_cap` 两个 Capability。

### 2.2 前置条件

1. `exc_source_cap` 指向活跃 `ExceptionSource`，且调用者对该源有活跃订阅（`ExcSubscribe`）；
2. `sret_frame_seg_cap` 指向一个调用者可写且内核可读的 Segment（共享于内核与调用者地址域之间的可见 Segment）；
3. `frame_offset + sret_frame_len ≤ seg.len()`，其中 `sret_frame_len` 由具体架构的 `SretFrame` 定义决定（x86_64 为 `0x88`）；帧完全落在该 Segment 内，且 `frame_offset` 满足对齐约束；
4. 调用者当前有未处理的异常上下文（细化 `Faulted` 态，`pending_exc` 存在且 `source_id` 匹配）；
5. 该异常的 `ExceptionKind` 属于可恢复类型：`PageFault` / `GeneralProtectionFault` / `DivideError` / `InvalidOpcode`。不可恢复类型（如 `DoubleFault`）调用 `exc_resume` 失败。

### 2.3 状态转换

- `Task: Faulted → Ready`，原子清除待恢复帧标记 `pending_exc = None`；
- 追加 `TaskResumed(exc_source_id, resume_vaddr)` Audit 事件。

### 2.4 错误码

| 错误 | 触发条件 |
|---|---|
| `NoPendingException` | 调用者没有未恢复的异常帧（`pending_exc` 不存在；消费后再次调用同样落入此错误） |
| `UnrecoverableFault` | 待恢复异常的 `ExceptionKind` 属不可恢复（如 `DoubleFault`）；不销毁 Task，保留给用户态判断 |
| `InvalidFrameAddr` | 帧越界或 `frame_offset` 未对齐 |
| `MissingRight(...)` | 帧 Segment 无该 Task 写权限，或内核不可读；或 `exc_source_cap` 无 `ExcSubscribe` |
| `InvalidExcSource` | `exc_source_cap` 不指向活跃 `ExceptionSource` |
| 未订阅 | 调用者对该源没有活跃订阅 |

### 2.5 与 schedule 的对接

`exc_resume` 只把 `Faulted → Ready`；恢复帧在 `schedule` 选中该 Task 后由细化层交付（代替原始指令流）。保持 §12.9 的"`schedule` 选中 faulted Task 时进入异常返回路径"不变。

## 3. 异常帧格式（x86_64 细化）

帧是通用寄存器现场。恢复目标是 `resume_vaddr`（指令指针，由参数给出，不写入帧）；栈顶由帧内 `rsp` 字段决定。

```text
struct SretFrame  align 16, size 0x88 (136)
  0x00  rax: u64
  0x08  rbx: u64
  0x10  rcx: u64
  0x18  rdx: u64
  0x20  rsi: u64
  0x28  rdi: u64
  0x30  rbp: u64
  0x38  rsp: u64        // 恢复后的栈指针
  0x40  r8 : u64
  0x48  r9 : u64
  0x50  r10: u64
  0x58  r11: u64
  0x60  r12: u64
  0x68  r13: u64
  0x70  r14: u64
  0x78  r15: u64
  0x80  flags: u64     // RFLAGS（低 32 位有效；系统位由内核策略强制）
```

约束：

- 帧只含用户态通用寄存器 + RFLAGS；RIP 由 `resume_vaddr` 单独给出，不写入帧；
- 内核对每个字段做合法性检查：`resume_vaddr` 在调用者 vspace 已映射可执行页，`rsp` 在已映射可写页；
- 帧从 Segment 复制到内核后清零该 Segment 区域（防敏感寄存器残留）；
- `flags` 中的系统位（IF/RF/iopl/VM）由内核按平台策略强制：用户态不能借 `exc_resume` 提升特权。

### 3.1 iretd 缺省恢复路径

细化层在 schedule 选中待恢复 Task 后构建重建栈：`push RFLAGS → push CS → push RIP(resume_vaddr) → 加载 GPR → iretq`。这是纯细化层 ABI 细节，不进 POSIR 状态机。

## 4. 层归属与跨架构中立性

`SretFrame` 的字段布局（`rax`/`rsp`/`flags` 等）是 **x86_64 细化层的定义**，不入 POSIR 抽象正文。`exc_resume` 的抽象语义（恢复一帧到 `resume_vaddr`）架构中立；各架构以不同实现解释"帧"：

- x86_64：`iretq`，帧为上述 GPR + RFLAGS 布局；
- ARMv8-A：`eret`，返回地址写 `ELR_EL1`、状态写 `SPSR_EL1`，帧为 ARM ABI 寄存器集；
- RISC-V：`sret`，`mepc`/`sstatus` 语义，帧为 RISC-V ABI 寄存器集。

COSIR 规范只在抽象语义层定义 `exc_resume`，每个架构细化层各自定义帧格式。正文中的 x86_64 帧表仅作为已实现锚点，不构成跨架构约束。

## 5. Rust 可执行状态模型探针

独立主机状态机（仿 xtask 风格），纯内存模型，不连接真实 QEMU/kernel。建模 COSIR 抽象状态与 `exc_resume` 转换，用测试锁定转移单调性。

新增 workspace 成员 `cosir`：

```text
cosir/src/state.rs    // Task, ExcSource, CapRecord, Vspace 纯数据模型
cosir/src/ops.rs      // raise_exception, exc_resume, schedule 状态转移
cosir/src/main.rs     // 线束：运行固定场景并打印 PASS 摘要
```

建模内容：

```rust
enum TaskStatus { Ready, Running, BlockedRecv, Faulted }  // Faulted 为细化态

struct Task {
    status: TaskStatus,
    pending_exc: Option<PendingException>, // 待恢复帧隐藏标记
}

struct PendingException {
    source_id: ExcSourceId,
    kind: ExceptionKind,          // 可恢复 / 不可恢复可判定
    saved_frame: Box<SretFrame>,  // 内核保存的原始现场（可回滚）
}
```

验证的转换不变量（测试）：

1. `raise_exception`(可恢复) → `Running → Faulted`，`pending_exc` 置 `Some`；
2. `exc_resume` 仅在 Faulted 且 `pending_exc` 匹配源时成功 → `Ready`，帧清空；
3. `exc_resume` 在 Ready/Running 上 → `NoPendingException`；
4. `exc_resume` 在已消费帧后再次调用 → `NoPendingException`；
5. DoubleFault 后 `exc_resume` → `UnrecoverableFault`（不销毁 Task）；
6. `schedule` 选中 Faulted→Running 但尚未 `exc_resume` → 细化层交付异常帧（原始指令流不重播）；
7. 每项错误路径都有对应转移测试（呼应 S2-X1 风格）。

## 6. 文档同步与证据

### 6.1 设计文档

本文件是本次设计权威。

### 6.2 COSIR 规范同步（`COSIR 原语规范.md`）

- §12.9 增加"恢复路径"子节：定义 `exc_resume` 原语、帧格式、Faulted 态、恢复流程；
- §12 增加 `exc_resume` 操作条目（前置条件 / 成功动作 / 错误）；
- §18.2 细化要求 #16：标注"已定义，待实现与形式化验证"；
- 边界清单 #1、#2：标注从"留待细化"移入"已设计"；
- 更新最近规范修订。

### 6.3 实现阶段文档同步（`Prima OS 实现阶段.md`）

- 记录本工作在阶段 0 的产出证据（新证据 ID，如 `SPEC-012`）；
- 当前首要缺口中移除"exc_resume 缺口"表述，更新为已设计；
- BLK-001/BLK-002 保持 Open（本工作不实质形式化规避，不关闭它们）；
- 阶段 0 不标记为满足退出条件。

## 7. 范围与边界

- 不关闭 BLK-001 / BLK-002；
- 不把阶段 0 标记为满足退出条件；
- 不实现真实 trap 入口、不连 QEMU kernel；
- Faulted 态是细化层可见状态；除 `exc_resume` 外，POSIR 层对恢复机制仍不可见；
- 不新增 POSIR 一等原语。
