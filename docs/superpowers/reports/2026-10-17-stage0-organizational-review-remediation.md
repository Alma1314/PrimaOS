# SPEC-139：组织级阶段 0 审查发现项整改（9 项）

> 文档类型：复核整改记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`f37eb31`
>
> 触发：组织级阶段 0 审查在 SPEC-137 之后又发现 9 项未登记问题，其中 4 项属于对象模型/生命周期（S0-D1、S0-X1），5 项属于操作失败语义或证据计数（S0-X2）。按维护规则 5，发现即登记并修订规范。
>
> 结论速览：9 项均已在 `COSIR 原语规范.md` 中逐项处置；S0-X1 仍不勾选——本轮整改本身仍需下一轮范畴外复核。S0-X2 的操作面按 §19 更正为 **46 项**（旧 26/34/35 口径作废），文本级失败语义缺口已修补。

## 1. 逐项处置

| # | ISSUE | 问题 | 规范修订 | 判定 |
|---|---|---|---|---|
| 1 | ISSUE-CAP-003 | 设备权限未纳入 §4.1 `Right` 联合类型与 `well_typed_rights` 表，设备 Capability 无法通过 W2 类型检查 | §4.1 增 8 个设备权限并增 `Device` 行；§15.3 声明 `DeviceRight ⊆ Right`，具体 `DeviceKind` 子集由创建操作定义 | Fixed（文本） |
| 2 | ISSUE-LIFE-009 | W16.1 与 `dev_mmio_create` 只检查 `free_frames`/活动 Segment 帧，遗漏 W6 保留帧（含页表帧）；页表帧释放回 `free_frames` 后可与其上的 MMIO 重叠 | §15.4 前置 2 与 §15.9/§16 W16.1 改为不与 `frame_universe` 中任何帧重叠（含 `free_frames`、活动 Segment 帧与 W6 保留帧） | Fixed（文本） |
| 3 | ISSUE-EXC-013 | §12.1 原则 5 与 W15 要求每 Task 对同一源至多一个活跃订阅，但 `exc_subscribe` 前置 5 只禁止“同一 `exception_kind`”重复，数据模型也不含 kind | §12.4 明确“一个 Task 对同一源至多一个活跃订阅，覆盖源允许的全部 kind”；前置 5 改为“尚未订阅此源”；`exc_unsubscribe` 前置同步 | Fixed（文本） |
| 4 | ISSUE-TASK-004 | `task_create` 接受非 CPU `kind`，但无条件要求代码 Segment 并建立 `vspace`；与 §5.2/§8.6（非 CPU `vspace` 为空）及 W8（每个活动 Task 的 entry 页可执行）矛盾 | §8.1 前置 1–3 与 vspace 映射改为仅 CPU；非 CPU 忽略代码参数、`vspace` 保持为空；`InvalidEntry`/`InvalidCodeRange` 标注仅 CPU；W8 与 §8.6 限定 CPU Task | Fixed（文本） |
| 5 | ISSUE-S0X2-002 | `wait_objects` 前置要求两列表长度一致，错误清单没有长度不一致的错误 | 新增并定义 `LengthMismatch`；加入 §6.2.1 新错误清单与 §21.2.1 错误清单 | Fixed（文本） |
| 6 | ISSUE-S0X2-003 | `ns_resolve` 前置要求 `ns_cap` 持有、可用且指向 Namespace，错误清单缺 `NotHeld`/`InvalidNamespace` | §11.7 错误清单补 `InvalidNamespace`、`NotHeld` | Fixed（文本） |
| 7 | ISSUE-S0X2-004 | 显式错误清单被 §6.2.1 定为失败域，但多个操作清单未覆盖“持有/可用”前置（`NotHeld`/`TargetDead`/`Invalid*`） | §6.2.1 增加“标准错误的隐式包含”：要求持有时 `NotHeld`、要求 `usable` 且目标已销毁时 `TargetDead`、目标类型不符时对应 `Invalid*`，清单无需重复；`exc_subscribe` 补显式项 | Fixed（文本） |
| 8 | ISSUE-S0X2-005 | S0-X2 采用 26+8+1=35 计数，但 §19 现行常规操作面为 46 项；SPEC-054 的 26 含内部 `advance_time` 且排除 Exception（5）与 Device（7） | SPEC-134 报告增 §11 更正附录，逐项列出 46 项分类；跟踪文档与白皮书同步更正；旧口径作废 | Fixed（文本/证据） |
| 9 | ISSUE-DEV-009 | `MMIORegion.size_pages` 注释要求 `> 0`，但 `dev_mmio_create` 未声明零长度、页对齐与区间溢出前置，也无对应错误 | §15.4 补 `size_pages > 0`、`base_paddr` 页对齐、区间不溢出前置；错误清单补 `ZeroSize`，并就地定义 `InvalidPhysRange` | Fixed（文本） |

## 2. 关键取舍

- **设备权限类型**：选择把设备权限并入统一 `Right` 全集，而不是保留两套互不相交的权限宇宙；这样 W2、`cap_derive` 的子集检查与 `well_typed_rights` 只需一套规则。按 `DeviceKind` 的可用子集仍由创建操作定义。
- **MMIO 与帧池**：选择“不与 `frame_universe` 全帧集重叠”，而不是只在创建时检查当时的 `free_frames`/Segment 帧；后者不是归纳不变量，页表帧回流即可破坏 W16.1。该选择把平台“MMIO 不在 RAM 内”的假设显式化，避免隐式依赖。
- **订阅粒度**：选择“每源一个订阅、覆盖全部 kind”，而不是给 `Subscriber` 增加 kind 并放宽 W15；前者与既有原则 5、W15 和数据模型一致，改动最小。
- **非 CPU `task_create`**：保留 `task_create` 作为统一创建原语，按 `kind` 分支，而不是新增 GPU 专用创建原语；非 CPU 的代码参数显式忽略，`vspace` 保持为空。
- **错误清单**：选择在 §6.2.1 增加全局隐式包含规则，而不是在 10 余个操作小节逐条重复 `NotHeld`/`TargetDead`；语义一致且避免后续清单继续漂移。
- **操作面计数**：以 §19 为唯一权威清单重算 46 项；SPEC-054 的 26 是历史口径，保留原文但由 SPEC-134 §11 明确其漏计范围。

## 3. 对退出条件的影响

- **S0-D1**：设备权限纳入 `Right`/`well_typed_rights` 后，对象能力状态模型重新一致。
- **S0-X1**：新增的 4 项对象模型/生命周期空洞（CAP-003、LIFE-009、EXC-013、TASK-004）已登记并修订；字面条件为“已知空洞均已记录且关闭”，但**组织级独立性门槛未跨过**（本轮审查与整改仍为 harness 内会话），S0-X1 保持未勾选。
- **S0-X2**：失败语义缺口（长度不一致、`ns_resolve` 清单、标准错误包含）已修补；操作面按 §19 更正为 46 项，旧 26/34/35 口径作废。仍仅为规范文本证据，不代表实现或证明。
- **S0-X3**：未改 Lean 模型；`lake build` 8 jobs 复验通过。

## 4. 复验

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p cosir --quiet` | 146 + 42 + 3 passed |
| `lake build`（`lean/`） | 8 jobs，成功 |
| `git status --short` | 见提交记录；本切片仅改规范、跟踪文档、白皮书、SPEC-134 追加与新增整改报告 |

以上复验只证明既有模型/形式化工件未回归，不证明新增规范条文已被实现或机器验证。

## 5. 证据边界

本记录是规范文本整改与文档登记；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。新增的 46 项操作分类是文本核对，不是逐项机器验证；`WaitObject`/`memseg_protect`/`gpu_memseg_create` 等新增条文在 cosir/Lean/内核中仍无对应实现。
