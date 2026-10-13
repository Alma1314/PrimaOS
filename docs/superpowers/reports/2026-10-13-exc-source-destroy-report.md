# 异常源销毁 `exc_source_destroy`（§12.6 补齐）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`kernel/src/bootstrap.rs` 的 `BootstrapState::exc_source_destroy`（新增）与 host 单测。
>
> 独立性边界（如实声明）：同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0/2「进行中」与勾选状态。

## 1. 起点

`SPEC-050` 行的证据边界早已声明「**无 `exc_source_destroy`**、无订阅撤销/源销毁」。承 `ISSUE-EXC-011`/`012` 的 §13.1 合规扫查，本轮把这条边界扫到，并按项目规矩（§12.6 早已定义该操作、内核只有 `exc_source_create`）**补实现**而非继续挂账。

§12.6 `exc_source_destroy(exc_source_cap)` 成功动作 5 条：删 subscribers、状态 `Destroyed`、从活动表删除、追加销毁事件、指向该源的 Capability 变 stale token。错误：`InvalidExcSource`、`NotHeld`、`MissingRight(ExcSourceDestroy)`。

内核侧的对应关系：

| 规范 | 内核 |
|---|---|
| `InvalidExcSource` | `ExcError::NoSuchSource`（既有口径：cap 不指向任何异常源） |
| `NotHeld` | `ExcError::NotHolder` |
| `MissingRight(ExcSourceDestroy)` | `ExcError::MissingRight` |
| 动作 1/2/3 | 源条目整体置 `None`（订阅集合随对象消失，`count` 不减 ⇒ ID 不复用） |
| 动作 4 | `AuditEvent::ObjectDestroyed{object_id, segment: false}` |
| 动作 5 | 细化层以 `target_live` 表达（不表现为 `SlotState::Stale`） |

## 2. 实现

`exc_source_destroy(&mut self, caller, cap) -> Result<(), ExcError>`：

1. `tasks.is_live(caller)`（与 `exc_subscribe`/`exc_unsubscribe` 同口径）；
2. `slots.check(cap) != Live` → `NoSuchSource`；
3. `exc_sources.find_by_cap(cap)` → `NoSuchSource`（句柄非异常源）；
4. `record.holder != caller` → `NotHolder`；缺 `ExcSourceDestroy` → `MissingRight`；
5. `audit.can_append(1)` 预检（原子性）；
6. 源条目置 `None`；追加 `ObjectDestroyed`。

**两条刻意的细化层口径**（与既有对象销毁一致，如实登记）：

- **不回收槽位**：异常源 cap 本就不入 `positions`（见 `ExcSource` 结构注释），其槽位由 `MAX_CAPABILITY_SLOTS` 兜住——与 `ns_destroy` 的「细化层不回收无位置 Capability 的槽（泄漏）」同口径；
- **`ObjectRef` 的 id 空间按类型独立**：`ObjectDestroyed` 只有 `segment: bool`，故异常源与 Timer/Namespace 共用 `segment: false` 形态（SPEC-083 已登记该粗粒度）。

**Faulted 任务的归宿**：§12.6 明写源销毁不改变指向它的 `Faulted` Task 的状态、不产生通知；该 Task 只能经 §8.2 内核回收路径处置。本实现不动 `tasks`/`pending_exc`，与该语义一致。

## 3. 回归面

- **QEMU 事件流零扰动**：新操作无 syscall 入口、boot 流程不调用它，42 条事件与 seq 逐字段不变（`cargo xtask test` 断言通过）；
- `cargo test -p kernel` **367 → 368**（新增 1 条 host 单测）；
- TCB 复测：`trusted_lines=1262 / mechanism_lines=21024 / ratio=5.66%`（机制层 +70 行、可信面未动），机制层 `unsafe=0`。

## 4. 门禁

- `cargo test --workspace` 全绿（cosir 146、cosir_state 42、lean_backtrace 3、kernel **368**、prima-ostd 21、xtask 204）；
- `cargo xtask lint` 四条 PASS（fmt / clippy `-D warnings` / `x86_64-unknown-none` 下 `deny(unsafe_code)` 构建 / `kernel/src` 零 `unsafe`·27 文件）；
- `cargo xtask tcb` PASS（`ratio=5.66%`）；
- `cargo xtask test` 六条 PASS（`image bios` / `image uefi` / `qemu bios run=1` / `run=2` / `stage1 boot slice`）；
- `cargo xtask compat` ⇒ `PASS compat cow-write-fault`。

**方法边界（如实登记）**：门禁期间工作区含**另一并行会话**未提交的 `cosir/src/*` 与 `cosir/tests/lean_backtrace.rs` 改动（形式化切片 SPEC-099 进行中），故 `cargo test --workspace` 的 cosir/lean_backtrace 计数含其改动（`lean_backtrace 2→3`）；kernel 相关结果（368 passed）与 QEMU 事件流不受影响（cosir 是 host 模型，不进内核镜像）。

## 5. 边界

- 闭合判断交复核者；阶段 0/2 门槛与勾选状态不动；
- **无 ring3 入口**：未新增 `SYS_EXC_SOURCE_DESTROY`，故本片证据等级为 `MODEL-TEST`（host 单测），不是 ring3 可达证据；ring3 代码页空间已近满（SPEC-096 登记），入口留后续切片；
- 不建模「状态 `Destroyed`」为独立字段（细化层以「不在活动表中」表达），与 `memseg_destroy`/`ns_destroy` 同口径。
