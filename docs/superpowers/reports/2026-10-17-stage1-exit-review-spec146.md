# SPEC-146：阶段 1 退出条件复核与签署

> 文档类型：阶段 1 退出条件复核/签署记录
>
> 日期：2026-10-17
>
> 基线：`0b63bc1`（阶段 0 签署后的 HEAD）
>
> 范围：S1-X2（init 只获得规范定义的初始 Authority）、S1-X3（活动 Segment 与空闲页帧唯一归属）；同时裁决 `ISSUE-FRAME-001`、`ISSUE-FRAME-002`、`ISSUE-AUTH-001`、`ISSUE-AUTH-002`。
>
> 结论：**S1-X2、S1-X3 通过（实现 + QEMU + host 测试层面）。** 阶段 1 退出条件勾选；`ISSUE-AUTH-001` 的两条 §14 语义偏差按“显式接受的细化分叉”处理，不阻塞 S1-X2 的 Authority 条件，但保留为阶段 2/4 的细化缺口。

## 1. 复核方法

- 以 `0b63bc1` 为基线，逐条回读 `kernel/src/bootstrap.rs`、`frames.rs`、`vspace.rs`、`protocol.rs`、`xtask/src/protocol.rs` 与阶段 1 报告/跟踪表。
- 运行 `CARGO_NET_OFFLINE=true cargo xtask test`：`PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1`、`PASS qemu bios run=2`、`PASS stage1 boot slice`，退出 0。
- 既有 `cargo test --workspace`（786 passed）与 `cargo xtask lint` 在本基线前后均通过；本复核不新增代码。

## 2. S1-X2 逐项核对

| 检查 | 代码/证据 | 结果 |
|---|---|---|
| 初始 Capability 恰为 3 条 | `INITIAL_CAPABILITY_COUNT=3`；`create_state` 的 `capabilities` 数组三条（image Segment / init Namespace / init Task） | 通过 |
| 权限精确匹配 §14 步骤 5 | `IMAGE_RIGHTS={Derive,SegMapR,SegMapX,SegDestroy}`；`NAMESPACE_RIGHTS` 为完整有限 Namespace 集；`TASK_RIGHTS={Derive,TaskSend,TaskManageVSpace,TaskDestroy}` | 通过 |
| 无父根/结构性根 | `with_init` 的 init `root_ns_cap=NAMESPACE_CAP_HANDLE`；`validate_authority` 检查 root cap 由 init 持有且含 `NsResolve` | 通过 |
| init 初始 `Ready`，首次调度 `Running` | `TaskTable::with_init` 置 `Ready`；`activate_init` 置 `Running` 并追加 `TaskStateChanged` | 通过 |
| 初始 Audit | `Bootstrapped` + image Segment/init Namespace `ObjectCreated` + `TaskCreated(init)`；`authority_ready` 的 `caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=2;init_running=1` | 通过 |
| `AlreadyBooted` | `is_installed()`/`BOOTSTRAP_STATE` 幂等；`create_state` 第二次返回 `AlreadyBooted` | 通过 |
| 入口 image 范围前置 | `entry_in_image(entry, image_base, image_len)`；host 单测覆盖边界；QEMU `entry_in_image=1` | 通过（语义偏差见 §4） |
| 平台初始化新增对象 | `platform_init_exception_sources`/平台 cap 在 `create_state` **之后**创建；S1-X2 的“初始 Authority”按 bootstrap 时的 3 条根计 | 通过（边界登记） |

## 3. S1-X3 逐项核对

| 检查 | 代码/证据 | 结果 |
|---|---|---|
| W6 三分类 + image | `FrameRegistry`：`image_frame_ids`（逐帧向量）、`owners`（segment 帧→标签）、`table_frames`（保留页表帧）、`alloc.remaining()`（free）；`owner_of` 逐帧查询 | 通过 |
| 守恒 | `check_conservation`：`free + segment + table + image == universe`；`install` 与 `witness` 调用 | 通过 |
| 不交性 | `check_disjoint` 逐帧检查 image/owner/table 三类无重叠；`witness.disjoint` 进入 `frames_ready` | 通过 |
| QEMU 断言 | xtask 要求 `conserved=1`、`disjoint=1`、`universe = free+segment+table+image`、非零 image/table/segment；`cargo xtask test` PASS | 通过 |
| `ISSUE-FRAME-001` | SPEC-068 已把 image `(base,count)` 改为逐帧向量；`owner_of`/`check_disjoint` 读它；`frames_ready` 有 `image` 逐帧计数 | **Closed** |
| `ISSUE-FRAME-002` | `owners` 逐帧 owner 表 + `check_disjoint`；host 反例覆盖“同帧计入两类归属”；QEMU `disjoint=1` | **Closed** |
| `ISSUE-AUTH-002` | SPEC-063/066 已把 13 个平台区域迁为真实 `SegmentObject` + 可授权根 Cap + `TaskVspaceTable` 记账 | **Closed** |

## 4. `ISSUE-AUTH-001` 的裁决

`ISSUE-AUTH-001` 登记两条 **§14 语义偏差**：

1. §14 要求 image 页映射 `{R,X}`，而实现的内核镜像同一段 VA 含可写 `.data`/`.bss`；前置 3 要求 `entry` 落在 `image_virtual_base` 区间内，别名映射被排除。该冲突在现有“无链接脚本、boot_info 不暴露段表”的前提下不可调和。
2. §14 的 `image` 是 **init 可执行镜像**，实现里 `image` = **内核镜像**，init 实际运行在独立的 ring3 用户段（`USER_CODE_VA + init.entry`）。

裁决：**接受为细化分叉，不阻塞 S1-X2。** 理由：

- S1-X2 的条件是“init 只获得规范定义的初始 Authority”；3 条根 Capability 的**数量、目标类型、权限、持有者、结构性根**均与 §14 步骤 5/6 一致，`validate_authority` 通过。
- 上述两条是 **§14 步骤 4 vspace/image 语义** 与实现装载方式的差异，属于 `REFINEMENT-DESIGN` 层；本轮不宣称 §14 image 已完全细化。
- 该分叉已在 `ISSUE-AUTH-001` 与阶段 2 决策日志登记；阶段 2/4 若声称“§14 image 细化完成”，必须先做链接脚本级段切分或修订 §14/§20.3，不能把本轮 S1-X2 的通过外推为“§14 vspace 已细化”。

## 5. 结论与签署

- **S1-X2：通过**（初始 Authority 的 3 条根、权限、root_ns、Ready→Running、Audit、AlreadyBooted、入口前置；实现 + QEMU + host 单测）。
- **S1-X3：通过**（逐帧 image 向量、owner 表、保留页表帧、守恒与不交性；QEMU `conserved=1/disjoint=1`）。
- **阶段 1 退出条件勾选，状态改为“已完成”。** 当前阶段推进到阶段 2（进行中）。
- `ISSUE-FRAME-001`、`ISSUE-FRAME-002`、`ISSUE-AUTH-002` 关闭；`ISSUE-AUTH-001` 标记为 **Accepted（refinement divergence，不阻塞 S1-X2）**，保留为阶段 2/4 的细化缺口。

## 6. 证据边界

- 本复核是真实内核实现 + QEMU 集成断言 + host 单测，证据等级为 `MODEL-TEST` + QEMU；**不构成** `REFINEMENT-PROOF`、机器证明或“§14 vspace 已细化”。
- `ISSUE-AUTH-001` 的两条偏差仍真实存在；本签署只覆盖 S1-X2 的 Authority 条件。
- `cargo xtask test` 的 PASS 只覆盖既定的 BIOS/UEFI 镜像与六条验收断言，不覆盖多核、真实硬件或所有 §14 细化义务。
