# SPEC-147：阶段 2 退出条件复核与签署

> 文档类型：阶段 2 退出条件复核/签署记录
>
> 日期：2026-10-17
>
> 基线：`8e901de`（阶段 1 签署后的 HEAD）
>
> 范围：S2-D1–D5、S2-X1–X4；同时收尾 `ISSUE-TASK-001`/`ISSUE-TASK-003`，确认 `ISSUE-CAP-002` 已闭合。
>
> 结论：**S2-D1–D5、S2-X1–X4 通过（实现 + host 测试 + QEMU 集成断言）。** 阶段 2 退出条件勾选；阶段 2 状态改为“已完成”，当前阶段推进到阶段 3。

## 1. 复核方法

- 基线 `8e901de`；逐项回读 `kernel/src/handle.rs`、`bootstrap.rs`、`frames.rs`、`vspace.rs`、`syscall.rs`、`protocol.rs`、`xtask/src/protocol.rs` 与 SPEC-062–067、071 报告。
- 运行 `CARGO_NET_OFFLINE=true cargo xtask test`：`PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1`、`PASS qemu bios run=2`、`PASS stage1 boot slice`，退出 0。
- 既有 `cargo test --workspace` 786 passed（含 kernel 377）与 `cargo xtask lint` 通过；本复核不新增代码。

## 2. 交付项

| ID | 证据 | 结果 |
|---|---|---|
| S2-D1 | `Handle` encode/decode + generation；`CapabilitySlotTable` 最小空闲槽、释放递增 generation；`stale_handle_is_rejected_after_slot_reuse`、`malformed_out_of_range_handles_fail_safety_not_panic` 等 host 单测 | 通过 |
| S2-D2 | `SYS_CAP_DERIVE`/`SYS_CAP_DROP`；派生权限收窄、结构性根保护、stale token 可 drop；host 单测 | 通过 |
| S2-D3 | `SYS_MEMSEG_CREATE/MAP/UNMAP/DESTROY`；真实 PTE 装/卸、空表帧回收、帧归还、新段清零；`memseg_ready` QEMU 证据 | 通过 |
| S2-D4 | `SYS_TASK_CREATE`（完整 §8.1 + 失败回滚）、`SYS_TASK_DESTROY`（§8.2）、`SYS_YIELD`；`task_ready` QEMU 证据 | 通过 |
| S2-D5 | `SYS_SEND`/`SYS_RECV`、FIFO 邮箱、阻塞/直投唤醒、接收者绑定授权；`ipc_ready` QEMU 证据 | 通过 |

## 3. 退出条件

| ID | 证据 | 结果 |
|---|---|---|
| S2-X1 | host 单测覆盖成功/失败路径；QEMU 负例注入：`task_ready` 的 `bad_entry=EINVAL`、`no_x=EACCES`、`deny=EACCES`、`again=EACCES`、`map_dead=ENOENT`；调度迁移 `TaskStateChanged` 由 `state_audit_ready`（`total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1`）钉住；`ISSUE-TASK-003` 关闭 | 通过 |
| S2-X2 | `ipc_ready`：`create=0x100000008;map1=0;send1=0;send2=0;recv1=0x51a5000000000001;cap0=0x100000008;magic=0x51a5000000000001;map2=0;cross=EACCES;send_denied=EACCES`；task 2 只能经授权消息/共享 Segment 交互；`task_ready` 的 `map_own=0x0;map_value=0xc0ffee0000000000;map_other=EACCES` 交叉验证 | 通过 |
| S2-X3 | `memseg_create` 在 `create_segment_object` 中对每个新帧调用 `mem::zero_frame`，且在映射/可见前完成；`memseg_ready.zero=0` 证明新段首帧清零，`create2`+`zero2=0` 证明归还复用帧也清零 | 通过 |
| S2-X4 | `CapabilitySlotTable` 的 generation 在释放时递增；`authorize` 先查槽位状态，stale 句柄在槽位阶段被拒；`stale_handle_is_rejected_after_slot_reuse` 与畸形句柄越界测试 | 通过 |

## 4. 已收尾/保留的边界

- `ISSUE-TASK-001`：**Closed（SPEC-070 + SPEC-072/073/074 + SPEC-147）**——`Faulted`、`pending_exc`、`kernel_resume_frame`、§12.10 恢复与 §8.2 回收均已实现并有 QEMU 见证；本复核确认其不再阻塞阶段 2。
- `ISSUE-TASK-003`：**Closed（SPEC-071 + SPEC-147）**——四条调度迁移路径均追加 `TaskStateChanged`，`state_audit_ready` 钉住总量。
- `ISSUE-CAP-002`：**Closed（SPEC-094）**——Namespace 根槽区间已顶端对齐，与 usercap 不交。
- `ISSUE-CAP-001`：**Open（host 模型缺口，非内核缺陷）**——`cosir` 的 Capability 派生链/位置表仍不完整；阶段 2 的证据来自真实内核 host 单测与 QEMU，不依赖该模型；保留为后续模型补齐项，不阻塞 S2。
- 平台路径经 `platform_create_segment`、VSPACE/COW 保留内核持有：属平台初始化边界（`ISSUE-AUTH-002` 已关闭的实现偏差），不改变 S2-D3 的用户 `memseg_*` 语义。
- `ISSUE-AUTH-001` 的 §14 image/vspace 细化分叉：阶段 1 已按显式分叉接受；阶段 2 的 Segment/Task 证据不依赖它，保留到阶段 3/4。

## 5. 结论与签署

- **S2-D1–D5、S2-X1–X4 全部通过。** 阶段 2 退出条件勾选，状态改为“已完成（2026-10-17，SPEC-147）”。
- 当前阶段推进到阶段 3（Namespace、Timer 与 Audit）。
- 证据等级：真实内核实现 + host 单测 + QEMU 集成断言；**不构成** `REFINEMENT-PROOF`、机器证明或所有 §17 安全结论的完整证明。

## 6. 边界

- 本复核未新增 QEMU 事件；QEMU 断言来自既有 `cargo xtask test` 的 47 条事件序列与负例注入。
- `ISSUE-CAP-001` 的 host 模型缺口仍是模型侧后续工作，不影响本阶段“真实内核 + QEMU”证据链，但不能据此宣称形式化模型已覆盖 Capability 派生/位置。
- 阶段 3 仍需独立复核 Namespace/Timer/Audit 的 S3-D1–D5、S3-X1–X4。
