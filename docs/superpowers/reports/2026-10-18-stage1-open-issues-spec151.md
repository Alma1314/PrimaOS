# SPEC-151：阶段 1 遗留 Open ISSUE 整改（AUTH-003/004、AUDIT-007、CAP-004）

> 文档类型：整改报告（非证明、非组织级签署）
>
> 日期：2026-10-18
>
> 基线：`53f1108`（SPEC-150 提交后的 HEAD）
>
> 范围：关闭 SPEC-150 保留的 4 项 Open ISSUE；不改变 S1-X2/S1-X3 的既有签署。

## 0. 结论

SPEC-150 登记的 4 项 Open 已全部处置：

| ISSUE | 结论 |
|---|---|
| ISSUE-AUTH-003 | **Closed**：内部 `create_task` 路径为每个活动 Task 派生结构性根；W5 校验不再跳过任何活动 Task |
| ISSUE-AUTH-004 | **Closed**：init 在移交前派生 Namespace 副本；send 侧结构根守卫已启用 |
| ISSUE-AUDIT-007 | **Closed**：`CapRecord` 历史改为追加式（静态 arena），`created_at` 随记录保存 |
| ISSUE-CAP-004 | **Closed**：单调 `CapId` 独立于槽索引，`parent < child` 与槽位复用无关 |

## 1. 逐项处置

### ISSUE-AUTH-003：非 init Task 的结构性根

- `BootstrapState::create_task` 现在先建 Task 记录，再从 init 的结构性根 Namespace Capability 派生
  `{NsResolve}` 根并写入 `root_ns_cap`。平台 task 2 与 Faulted/回收见证 Task 均走该路径。
- 根 Capability 落在预留槽区间 `120..128`；`CapabilitySlotTable::alloc` 跳过该区间，避免平移
  init 机器码内嵌的派生句柄（S2-D2 证据）与运行期动态分配。
- `validate_authority` 删除 `root_ns_cap == Handle::ZERO` 的跳过：活动 Task 无根即
  `InvalidHolder`；回收后的 `Dead` Task 不受 W5 约束。

### ISSUE-AUTH-004：init 移交结构根

- `prima-initgen` 的 `SERVICE_SETUP_TAG` 路径改为：先向 wake 消息，随后
  `SYS_CAP_DERIVE([r15+OFF_NAMESPACE_CAP], NS_ROOT_RIGHTS_BITS)`，重新装入接收者/发送凭据，
  再用 `mov r8, rax` 把**派生副本**作为 cap0 发送。init 始终保留自己的结构根。
- `send_message_with_capabilities` 启用 §9.1 前置 6：任何活动 Task 的结构性根 Namespace
  Capability 返回 `IpcError::StructuralCap`（用户态 `EACCES`），并新增 host 反例。

### ISSUE-AUDIT-007：追加式 `CapRecord` 历史

- 新增 `CapabilityHistoryRecord` 与静态 `CAPABILITY_HISTORY_ARENA`；每次铸造/派生追加一条，
  `cap_drop`/撤销只删除活动位置与活动表项，**历史条目不变**。
- 每条记录携带 `created_at`（对应铸造/派生事件的审计序号）；Bootstrap 初始根以 0
  （`Bootstrapped`）为创建记录。
- 活动 `CapabilityRecordTable` 同步移入静态 arena，避免新增字段越过 `BootstrapState` 的
  13 KiB 守卫（实测仍在守卫内）。
- `validate_authority` 要求每个活动记录在历史中有字段一致的不可变副本；父已 drop 时，权限/
  目标/`CapId` 校验回退到历史记录（§4.2：子 Capability 不受父丢弃影响）。

### ISSUE-CAP-004：单调 `CapId`

- `CapabilityRecord` 与初始根新增单调 `cap_id`（初始根 1..3，`next_cap_id` 从 4 起）。
- `validate_authority` 以 `parent.cap_id < child.cap_id` 校验 §4.2 字面序，不再依赖槽索引；
  host 测试覆盖「drop 后复用同一槽仍保持单调序」。

## 2. 证据变化（逐条登记）

| 证据 | 变化 |
|---|---|
| init 机器码 golden hash | `0xf02f_6c19_ce2f_63de` → `0x8db7_9beb_2cef_cd61`（setup 路径新增派生与参数重装） |
| `task_ready` 的 `caps` / `caps_after` | 111 → **114**（task 2 根、活动 Faulted 见证根、init 移交的派生根各 +1） |
| `service_negotiated` token | `0x2_0000_0033` → **`0x1_0000_0037`**（新增派生占用最低空闲槽，后续派生整体平移） |
| `service_negotiated` manager cap | `0x1_0000_0038` → **`0x1_0000_0039`** |
| `state_audit_ready` | `total=182;...;used=604;cap=768`；新增根 `CapabilityDerived` 计入 `used`，范围断言通过 |

既有 PRIMA 事件序列与其余 data 字段未变。

## 3. 验证

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace --quiet` | 全部通过 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | fmt、Clippy `-D warnings`、bare-metal kernel、零 `unsafe` 全 PASS |
| `CARGO_NET_OFFLINE=true cargo xtask test` | `PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1/2`、`PASS stage1 boot slice` |

## 4. 独立性声明

本轮仍是 harness 内整改，不构成组织级第三方签署；不宣称 `REFINEMENT-PROOF`。
`ISSUE-AUTH-001` 的 §14 image/vspace 分叉与 EXO-3/EXO-4/DOC-007 后续硬化范围不变。
