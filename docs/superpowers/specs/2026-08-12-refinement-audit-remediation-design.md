# PrimaOS 细化审查纠偏设计

日期：2026-08-12

## 1. 目标

本设计关闭当前 COSIR host 探针中的已知正确性空洞，并校准 PrimaOS 对模型测试、细化设计、真实内核运行和研究主张的证据表述。

工作分为两个独立阶段：

1. 修复 MemorySegment/pagewalk 模型并校准证据；
2. 补全 `exc_resume` 的授权、恢复帧和失败原子性语义。

本设计不宣称完成真实 x86_64 kernel refinement。host 探针在机器证明和真实内核接入前只能提供 `MODEL-TEST` 与 `REFINEMENT-DESIGN` 证据。

## 2. 问题登记与事实源

在 `Prima OS 实现阶段.md` 增加“已知问题与纠偏计划”章节。每项问题使用稳定 ISSUE ID，并记录：

- 问题及影响；
- 当前证据等级；
- 修复阶段；
- 可执行验收条件；
- `Open`、`In Progress` 或 `Fixed` 状态。

不创建独立审查报告，避免与阶段追踪器形成两个可能失步的事实源。白皮书和 COSIR 规范中的相关表述随 ISSUE 修复同步更新。

## 3. 证据等级

| 等级 | 含义 |
|---|---|
| `MODEL-TEST` | host 可执行模型测试 |
| `SPEC-CONFORMANCE` | 独立规范 oracle 或差分测试 |
| `REFINEMENT-DESIGN` | 已定义状态关系及模拟义务 |
| `REFINEMENT-PROOF` | 机器检查的模拟证明 |
| `KERNEL-TRACE` | 真实 kernel/QEMU 功能轨迹 |
| `COMPAT-TEST` | 与参考系统的 API/ABI 差分证据 |

`cargo xtask test` 仅证明现有启动闭环无回归，不作为 SPEC-012、SPEC-013 或 SPEC-014 的直接功能证据。

## 4. 阶段一：MemorySegment/pagewalk

### 4.1 ISSUE-MEM-001：虚拟地址合法性

所有 map、unmap 和 pagewalk 输入必须满足：

- 4 KiB 对齐；
- 是 x86_64 48-bit canonical address；
- 整个页区间保持 canonical；
- high-half 地址在叶遍历投影中按 bit 47 正确符号扩展。

非法输入必须在修改状态前返回稳定错误。测试必须覆盖低半、高半、非 canonical、页内偏移以及高位别名反例。

### 4.2 ISSUE-MEM-002：有限整数安全

以下运算必须使用 `checked_add`、`checked_mul` 或 `try_from`：

- `seg_offset + page_count`；
- `target_base + page_index * PAGE_SIZE`；
- `page_count` 到 `usize` 的转换；
- Segment offset 到帧索引的转换；
- `next_seg` 递增。

任一溢出返回稳定错误，且整个 `RefineState` 保持不变。

### 4.3 ISSUE-MEM-003：强化 α

`memseg_alpha` 对每个叶检查：

```text
mapping.offset < segment.frames.len
frame_phys == segment.frames[mapping.offset]
present == true
user == true
writable == mapping.access.contains(W)
nx == !mapping.access.contains(X)
mapping.access ⊆ segment.allowed_access
```

测试分别突变 `frame_phys`、offset、present、user、writable、nx 和 access；每项损坏必须被 α 拒绝。

### 4.4 ISSUE-MEM-004：帧集合语义

FrameAllocator 必须实施有限集合语义：

- 初始化拒绝重复 FrameId；
- 拒绝 universe 外帧；
- 拒绝重复归还；
- W6 使用集合划分检查，而非排序后的多重集合比较。

初始化接口应以可测试错误报告非法 universe，不依赖仅在部分构建模式生效的隐式行为。

### 4.5 ISSUE-MEM-005：`memseg_create` 规范一致性

删除 `memseg_create` 的 `seg_cap_rights` 前置条件。创建操作只依赖：

- `page_count`；
- `allowed_access`；
- 可用帧；
- Segment ID 空间。

Segment 根能力属于创建后的结果，不得要求调用者预先持有尚不存在对象的映射权限。

### 4.6 ISSUE-MEM-006：模型边界

本阶段保留四级索引树，不伪装成完整 x86_64 权限模型。文档明确当前模型不覆盖：

- 非叶 U/S、R/W、NX 的有效权限合成；
- 页表页物理身份与分配回收；
- TLB 失效和多核可见性；
- 真实 CR3/PTE 编码及原子更新窗口。

SPEC-014 标为 `MODEL-TEST + REFINEMENT-DESIGN`。上述义务留给 `KERNEL-TRACE` 和 `REFINEMENT-PROOF`，不得由 host 测试替代。

## 5. 阶段二：`exc_resume`

### 5.1 Capability 与订阅

模型增加支持当前操作所需的最小授权状态：

- 调用者持有活跃 ExceptionSource capability；
- capability 包含 `ExcSubscribe`；
- 调用者位于 source 的活跃订阅集合；
- 调用者持有恢复帧 Segment capability；
- Segment 对调用者可写且内核可读。

所有授权失败都返回规范错误，并保持状态不变。

### 5.2 恢复帧

Segment 保存实际字节。`exc_resume` 从 `(frame_seg_id, frame_offset)` 解码固定布局的 `SretFrame`，并验证：

- RIP、RSP 是 canonical 用户地址；
- RIP 位于可执行用户映射；
- RSP 位于可写用户映射；
- CS、SS 满足用户态返回策略；
- RFLAGS 不含被禁止的特权或保留位。

成功后把已验证帧复制到内核持有的恢复状态，并清零用户 Segment 中已消费的帧区域。

### 5.3 状态转换与 Audit

仅允许 `Faulted` 且存在匹配 pending exception 的 Task 恢复。不可恢复 fault 保持 `Faulted`。

成功迁移原子执行：

```text
consume frame
→ clear pending exception
→ save validated kernel frame
→ append Audit event
→ Faulted → Ready
```

调度器仍只负责 `Ready → Running`，不隐式重播故障指令。

### 5.4 失败原子性

每个前置条件必须有独立 mutation test。错误返回前后的整个状态深拷贝必须相等，包括：

- Task 状态；
- pending exception；
- capabilities 和 subscriptions；
- Segment 字节；
- kernel resume frame；
- Audit log。

阶段二完成后仍只标为 `MODEL-TEST`；真实 trap/`iretq` 接入后才能增加 `KERNEL-TRACE`，机器检查模拟定理完成后才能增加 `REFINEMENT-PROOF`。

## 6. 研究主张校准

同步修改 `Prima OS.md`、`COSIR 原语规范.md` 和阶段追踪器：

- “跨架构完全适用”降级为“候选适用，待第二架构反例检验”；
- GPU、NPU 和分布式场景的原语分解标为待验证映射假设；
- 纸面组合不再称为 H1 的经验支撑；
- SPEC-014 明确为 host 双视图模型，而非真实 kernel refinement；
- `exc_resume` 若能通过结果、调度或 Audit 被客户端观察，则按 POSIR 可观察操作接受必要性与可信边界审查；
- 修正“仅 Bootstrap”与已存在 MemorySegment 细化设计之间的文档矛盾。

阶段追踪器增加研究停止条件：

> 连续两个迭代若仅增加 `MODEL-TEST`，而没有新增核心假设反例搜索、`KERNEL-TRACE`、`COMPAT-TEST` 或 `REFINEMENT-PROOF`，则暂停局部模型扩展。

两个修复阶段结束后，下一研究切片固定为：

```text
COW/page fault
→ ExceptionSource
→ handler
→ exc_resume
→ 页表更新与 TLB 失效
→ 用户态继续执行
→ 与 Linux observable trace 对比
```

## 7. 测试与验收

每个 ISSUE 采用测试驱动修复：

1. 添加复现问题的失败测试；
2. 实施最小修复；
3. 运行目标模块测试；
4. 运行 `cargo test -p cosir`；
5. 运行 workspace clippy；
6. 更新 ISSUE 状态和直接支持的命题。

阶段总验收命令：

```text
cargo test -p cosir
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

最后一项只证明启动闭环无回归。

阶段二还必须覆盖：

- 每项 Capability 和订阅失败；
- RIP、RSP、RFLAGS、CS、SS 非法；
- frame 越界、未对齐、成功消费和清零；
- source、kind 不匹配；
- 不可恢复 fault；
- 成功 Audit；
- 所有失败路径状态完全不变。

## 8. 提交策略

每个可验证问题使用独立提交，提交内同时包含实现、测试及对应文档状态。预期顺序为：

1. 登记问题和证据等级；
2. 修复地址合法性及溢出；
3. 强化 α 的物理帧和权限检查；
4. 实施 FrameAllocator 集合不变量；
5. 对齐 `memseg_create` 与 POSIR；
6. 校准 SPEC-014 和研究主张；
7. 完成阶段一总验收和独立审查；
8. 分项补全 `exc_resume` 授权、帧和 Audit；
9. 完成阶段二总验收和文档闭环。

canonical address 与地址溢出共享输入验证边界时可合并为一个提交；α、allocator 与 `exc_resume` 必须保持独立，以便审查和回归定位。

## 9. 完成标准

本设计仅在以下条件全部满足后关闭：

- tracker 中所有 `ISSUE-MEM-*` 和 `ISSUE-EXC-*` 均有明确状态及证据；
- 已知负向反例均有自动化测试；
- host 模型不再接受错误物理帧、权限、地址或未授权恢复；
- 规范、白皮书和 tracker 的证据等级一致；
- 所有阶段总验收命令通过；
- 独立审查未发现未登记的高严重级正确性空洞；
- 未把 host PASS 外推为真实 kernel、形式化证明、兼容性或跨架构证据。
