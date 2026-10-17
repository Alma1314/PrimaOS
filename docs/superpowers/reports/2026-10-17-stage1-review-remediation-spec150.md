# SPEC-150：阶段 1 补充复核与整改

> 文档类型：复核/整改报告（非设计、非规范修订、非证明）
>
> 日期：2026-10-17
>
> 基线：`5a137eb`（SPEC-149 阶段 4 复核后的 HEAD）；本轮代码改动位于工作树
>
> 范围：S1-X2（init 初始 Authority）与 S1-X3（帧唯一归属）的**补充对抗复核**；对照
> `COSIR 原语规范.md` §4.1/§4.2/§9.1/§13.1/§14/§16、`kernel/src/{bootstrap,rights,frames}.rs`、
> `cosir` 与 `xtask` 证据。

## 0. 结论

SPEC-146 的 S1-X2/S1-X3 签署**继续有效**：本轮未推翻其 Authority 与帧归属结论。但 SPEC-146
遗漏了 8 类实现/规范一致性问题；本轮已逐项处置（6 类闭合，2 类保留 Open），并把 4 项需要
跨切片处理的问题登记为 ISSUE。所有修复均通过 `cargo test --workspace`、
`cargo xtask lint` 与 `cargo xtask test`（BIOS run=1/2、`PASS stage1 boot slice`）。

**证据边界**：本轮仍为真实内核 + host 模型 + QEMU 证据，不是 `REFINEMENT-PROOF`；不改变
阶段 1 的既有签署，也不把 `ISSUE-AUTH-001` 的 §14 image/vspace 偏差当作已闭合。

## 1. 发现与处置

### F1 初始 Audit 缺少三条 `CapMinted`（已修复）

- 现象：§14 步骤 9 要求 bootstrap 追加三条初始根 Capability 的铸造事件；`create_state`
  只写 `Bootstrapped`、两个 `ObjectCreated` 与 `TaskCreated`，`cosir` 同形。
- 修复：内核在 `TaskCreated` 后为 `IMAGE_CAP_HANDLE`/`NAMESPACE_CAP_HANDLE`/
  `TASK_CAP_HANDLE` 追加 `CapabilityMinted`；`cosir` 的 `AuditKind` 增加
  `CapabilityMinted` 并在 `abstract_state(booted)` 追加三条。初始 Audit 由 4 条变为 7 条。
- 证据：`kernel/src/bootstrap.rs` 的 `create_state`；`cosir/src/{state,abstract_ops,main}.rs`；
  host 单测 `bootstrap_audit_records_the_three_initial_capability_mints`。

### F2 W5 校验只覆盖 init，且未检查目标 Namespace（已修复）

- 现象：`validate_authority` 只检查 init 的 `root_ns_cap` 由 init 持有且含 `NsResolve`；
  未检查目标是活动 Namespace。`move_capability`/`revoke_capability` 也没有结构性根守卫。
- 修复：W5 循环覆盖所有非 `Dead` 且 `root_ns_cap != Handle::ZERO` 的活动 Task，并检查
  「目标为活动 Namespace」；`move_capability`/`revoke_capability` 拒绝结构性根，
  `move_capability` 还要求目标 Task 活动。
- 仍未闭合：平台 witness task（2/5/6/7 等）仍以 `Handle::ZERO` 表示无结构根；W5 的
  「所有活动非 RemoteProxy Task 都有结构根」尚未成立 → **ISSUE-AUTH-003（Open）**。

### F3 `send` 缺少 `usable` 检查（已修复）；结构性根发送守卫延后

- 现象：§9.1 前置 5 要求待传 Capability 目标仍存活；实现只检查槽位 `Live` 与持有者。
- 修复：`send_message_with_capabilities` 增加 `target_live` 检查（失败 `InvalidCapability`），
  host 反例 `send_rejects_a_capability_whose_target_is_dead`。
- **未启用**：§9.1 前置 6 的结构性根拒绝。ring3 init 的启动协议
  （`prima-initgen/src/code.rs` 的 `SERVICE_SETUP_TAG`）当前直接把 init 的
  `NAMESPACE_CAP_HANDLE` 移交给服务管理器；启用该守卫会在 boot 早期中断启动。该协议需要
  改为「先派生 Namespace 子 cap 再移交」，涉及手写机器码布局与证据重测，登记为
  **ISSUE-AUTH-004（Open）**，本轮不静默放宽规范。

### F4 `Right`/`ObjectKind` 与 §4.1 不完整（已修复）

- 现象：内核 `Right` 缺少 `WaitWait`/`WaitSignal`/`WaitDestroy` 与
  `DevMMIOR`/`DevMMIOW`/`DevDMAR`/`DevDMAW`/`DevInterruptSubscribe`；`ObjectKind::Device`
  不区分设备种类，`well_typed` 无法按 §4.1 表做 W2 检查。
- 修复：`ALL_RIGHTS` 23→31；新增 `DeviceKind` 与 `ObjectKind::Device(DeviceKind)`，
  `ObjectRef::Device(id, kind)` 携带种类；`well_typed` 按 MMIO/IOPort/DMA/IRQ 分别匹配。
  新增权利追加在末尾，既有权利的位序与渲染顺序不变。
- 证据：`kernel/src/rights.rs`、`kernel/src/bootstrap.rs`；
  单测 `all_spec_4_1_rights_parse_and_round_trip`、`device_ioport_rights_are_typed_to_devices_only`。

### F5 §14 步骤 3/4 的 creator/kind/node 未表示（已修复）

- 现象：`TaskRecord` 无 `creator`/`kind`/`node`；bootstrap 的 image Segment 与 init
  Namespace 的 `creator` 写成 `INIT_TASK_ID`，而 §14 要求 `KernelActor`；Namespace 无 `node`。
- 修复：新增 `TaskKind::Cpu`、`LOCAL_NODE` 与 `TaskRecord.{creator,kind,node}`；
  bootstrap 的 image Segment/init Namespace/init Task 均以 `KERNEL_ACTOR`/`Cpu`/`LOCAL_NODE`
  初始化；`task_create` 成功后覆写 `creator = caller`；`NamespaceObject` 增加 `node`。
- 证据：host 单测 `init_task_represents_spec_creator_kind_and_node`。

### F6 §14 前置 3 的入口范围按字节而非按页（已修复）

- 现象：`create_state` 用 `image_len`（字节）判断入口；镜像末页只部分使用时，规范按
  `image_frames.length` 页上取整，实现会误拒末页内入口。
- 修复：改用 `inventory.image_frames * PAGE_SIZE` 作为 image 虚拟区间长度；host 单测
  `entry_range_is_checked_in_image_pages_not_partial_bytes`。

### F7 `AuditLog::append` 无容量断言（已修复）

- 现象：`append` 直接写 `base + count`；若调用方漏掉 `can_append` 预检且容量耗尽，会写出
  本实例的 arena 区段。`activate_init` 与两条 witness 路径确实没有预检。
- 修复：`append` 增加容量断言（宁可确定性 panic 也不越界写）；`activate_init` 增加
  `can_append(1)` 预检并返回 `BootstrapError::AuditFull`；fault/reclaim witness 各自预检。
  容量仍为 768（QEMU 双跑通过，未触发上限）。

### F8 阶段 1 证据文本过期（已修复）

- `docs/superpowers/specs/2026-09-16-stage1-closure-design.md` 与
  `docs/superpowers/reports/2026-09-16-stage1-closure-report.md` 仍写
  `TaskStateChanged(Ready)`、`root_ns=2`、`frames_ready` 六字段与「无逐帧 owner 映射」。
- 现状：初始创建事件是 `TaskCreated`；`root_ns` 是严格句柄 `0x100000002`；
  `frames_ready` 含 `disjoint=1` 且 `ISSUE-FRAME-002` 已关闭。两份文档顶部已加
  「SPEC-150 更正」注记，历史结论保留。

## 2. 登记的 Open ISSUE

| ISSUE | 内容 | 处置 |
|---|---|---|
| ISSUE-AUTH-003 | 平台 witness task 与 `create_task` 辅助路径仍无结构根；W5 的全局量化未成立 | 后续切片补齐根 cap 或显式把 witness task 移出 POSIR 状态；不得静默豁免 |
| ISSUE-AUTH-004 | ring3 init 的 `SERVICE_SETUP_TAG` 直接移交结构性根 Namespace cap，与 §9.1 前置 6/W5 冲突 | 改 init 机器码为先 `SYS_CAP_DERIVE` 子 cap 再移交，重测全部启动事件；在此之前 send 侧守卫不启用 |
| ISSUE-AUDIT-007 | `CapRecord` 在 `cap_drop`/撤销时被删除，非 §4.2 的追加式历史；`created_at` 字段未表示 | 引入历史表或只追加记录；与 SPEC-142 的 ISSUE-AUDIT-003/004 一并处理 |
| ISSUE-CAP-004 | 实现的 `CapId` = 槽索引 + generation，槽复用使 §4.2 的 `parent < child` 字面序不成立 | 规范已承认派生无环由构造保证；是否引入单调 CapId 留待 Capability 模型切片 |

## 3. 验证

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace --quiet` | cosir 146 + 42 + 3、kernel 383、xtask 195 + 21 + 1 + 1，全部通过 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | fmt、Clippy `-D warnings`、bare-metal kernel、`kernel/src` 零 `unsafe` 全 PASS |
| `CARGO_NET_OFFLINE=true cargo xtask test` | `PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1/2`、`PASS stage1 boot slice` |

**事件证据**：新增的三条 `CapMinted` 只进入内部 Audit 日志；`cargo xtask test` 的 PRIMA
事件序列与既有 `data` 断言均未变化。`state_audit_ready.used` 的 xtask 校验是范围断言
（`used >= total` 且 `used <= cap`），不钉绝对数，故本轮未修改 golden。

## 4. 独立性声明

本轮复核与整改仍是 harness 内会话，不构成新的组织级第三方签署。SPEC-146 的阶段 1 签署
基于其自身证据；本报告只补充其遗漏项并修复可闭合部分，Open ISSUE 不因本报告而关闭。
