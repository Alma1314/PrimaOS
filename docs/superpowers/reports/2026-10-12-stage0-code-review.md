# 阶段 0（语义基线）代码评审报告

> 文档类型：评审报告（非设计、非规范、非证明）
>
> 日期：2026-10-12
>
> 评审对象：阶段 0（语义基线）工件——`cosir` host 状态模型核心、`lean/Formal/*` 可执行参考模型、`cosir/tests/lean_backtrace.rs` 对拍，对标《COSIR 原语规范》§5.2/§6.2.1/§12.3/§12.9/§12.10/§13.1/§17/§20.2/§20.3。
>
> 方法：`requesting-code-review` skill（`obra/superpowers` v7.0.0，本机安装于 `~/.claude/skills/`）——派**只读** senior code-reviewer 子 agent（新上下文、不共享会话历史、不复用本会话结论），出 Strengths / Issues(Critical·Important·Minor) / Assessment；随后由本人逐条核对并按项目规矩修复。
>
> 独立性边界（如实声明）：本轮为 harness 内评审，**不等价于**组织级第三方审查；不改变阶段 0「进行中」与 S0-X1 未勾选。

## 结论

`Ready to merge?` → **With fixes**。**Critical 无**。`exc_resume` 的失败原子性与 §12.10 前置/提交顺序逐条吻合（强项，评审者独立确认）。四项 Important 均属**证据与实现不符**，不影响既有测试正确性；本轮已修复其中可修项，语义级缺口已登记。

## 发现与处置

| # | 严重度 | 发现 | 处置 |
|---|---|---|---|
| 1 | Important | Lean `bootedState` mint 1 条 init 占位能力，cosir `abstract_state(booted)` 不 mint，而 `lean/README.md` 声称 `caps ↔ capabilities` 逐字段 1:1；`bootstrap_parity_lean` 只断言 `tasks`、**不比较能力表**，分歧不可见 | **已修（文档+测试）**：`lean/README.md` 映照契约据实标注 booted 态不一致、对拍覆盖据实改写；`cosir/tests/lean_backtrace.rs` 增一条断言**钉住 cosir 侧现状**（防静默改动掩盖）。模型一致性/§14 兑现 → 登记 `ISSUE-REFINE-001` |
| 2 | Important | §20.3「α 投影后 init 能力集恰好等于 §14 初始 Authority」两侧均未兑现（cosir 空、Lean 仅 1 条） | **登记 `ISSUE-REFINE-001`**（含 §14 三条无父根能力 + init vspace；模型缺 Namespace `Object` 变体，补全须扩模型） |
| 3 | Important | `ops::raise_exception` 完成 `Running→Faulted` 却不追加任何 Audit，违反 §13.1 `TaskStateChanged`（§12.5 另要求 `ExceptionRaised`），且缺口此前**未登记** | **登记 `ISSUE-EXC-006`** |
| 4 | Important | Lean 9 条定理全部 `by rfl`/`trivial` 重言（`WellFormed := True`），`lake build` 报 unused-variable 警告；README/SPEC-019 措辞易高估保证 | **已修**：`Formal/State.lean`、`Formal/Bootstrap.lean` 前置下划线（`_s`/`_m`/`_h`）消警告；`lean/README.md` 增「定理性质（如实声明）」段 |
| 5 | Minor | `machine.rs` 注释引用不存在的 `abstract.rs` | **已修** → `abstract_ops.rs` |
| 6 | Minor | `state.rs` 注释称 `save_frame`/`Task.id`「不被构造/读取」（失真；`save_frame` 为只写字段、实际恢复帧取自用户 Segment） | **已修**：注释据实改写并点明 `save_frame` 只写 |
| 7 | Minor | `state.rs` 注释称 `SyscallTrap/ExternalInterrupt/TaskExit`「仅枚举保留」，但枚举无此变体 | **已修** |
| 8 | Minor | `abstract_ops.rs` 测试注释称「State 未实现 Debug」而 `State` 已 `derive(Debug)` | **已修**：改用 `unwrap_err()`，删过时理由 |
| 9 | Minor | `ops.rs` `is_user_canonical` 硬编码 48 位，LA57 下会误拒合法用户地址 | **已修**：补边界假设注释（未改行为） |
| 10 | Minor | `ops.rs:154` Task 缺失时错误码 `InvalidExcSource` 语义错位 | **未改**（退化态才可达；改错误码属语义变更，留复核者裁定） |

## 门禁

`cargo test -p cosir` ⇒ 40 + 2（`lean_backtrace`）全绿；`lake build` ⇒ 成功（先前的 unused-variable vacuity 警告已消除）；`cargo fmt -p cosir --check` 无差异；`cargo clippy -p cosir --all-targets -- -D warnings` 零告警。本轮**不触碰内核非测试代码**，故 QEMU 事件流不受影响。

## 遗留（待复核者裁定）

- `ISSUE-EXC-006`（raise 路径 Audit）：**host 模型缺口**，需决定「补实现」还是「显式降级为范围边界」（本轮未动）。
- `ops.rs:162` 的 `kind` 比较（`ISSUE-EXC-007`）：**已于同日修复**，见下。

## 后续处置（2026-10-12，同日）

用户裁定推进两项语义级缺口，已实现：

- **§14 初始 Authority（`ISSUE-REFINE-001`，能力部分）**：`cosir` 新增 `CapObject::Namespace`/`Namespace`/`State.namespaces`，`Capability.rights` 由 `u8` 扩 `u16`（bit0..5 位值不变），`abstract_state(booted)` 物化 §14 步骤 2/3/5/6 的 image Segment、空 init Namespace、init Task 与三条无父根 Capability；Lean `Formal` 新增 `Object.namespace`、`rights : UInt16`、`bootedState` 铸同三条根 Cap，`bootstrap_empty_mints_{image,namespace,task}_cap` 三条定理；`bootstrap_parity_lean` 逐条断言能力表。**§14 步骤 4（image 逐页 vspace）与步骤 7（结构性根 `Task.root_ns_cap`）未投影**（`MachineState` 无 `image_frames`、`Task` 无该字段），登记为剩余。
- **kind 比较（`ISSUE-EXC-007`）**：`ops::exc_resume` 改按 `ExceptionKind` **变体**比较（`std::mem::discriminant`），符合 §12.3「类型签名」；`main.rs` 原固化旧行为的 stanza 改为正例 `exc_resume_accepts_same_kind_different_cause`。
- 门禁：`cargo test -p cosir` **41 + 2 全绿**、`lake build` 成功、clippy/`fmt --check` 干净；不触碰内核非测试代码，QEMU 事件流不变。

### 续（2026-10-12）：`ISSUE-REFINE-001` 剩余项闭合（§14 步骤 4/7）

- **扩模型**：`MachineState`（规范 §20.1）新增 `image_frames`/`image_virtual_base`，`Task`（规范 §5.2 已有 `root_ns_cap`）在 cosir 侧补齐该字段；`abstract_state(booted)` 由镜像帧投影 §14 步骤 4 的逐页 `vspace`（`PageMapping{backing}` 的 segment/offset/frame 与镜像帧一一对应，`access = {R,X}`），并把 init Namespace Capability 写入 `root_ns_cap`（步骤 7，满足 W5 的 `NsResolve`）。
- 取**扩展模型**而非在 §20.3 降级不变量（与既有「不变量不放宽」取齐）；规范 §20.1/§20.3 同步。
- `bootstrap_parity_lean` 增断言钉住 cosir 侧 `vspace` 条数/基址与 `root_ns_cap == 2`；Lean 侧不建模这两项（无 `tasks`/MachineState），已在 `lean/README.md` 映照契约表与「已知不覆盖」如实标注为 cosir-only。
- 门禁：`cargo test -p cosir` 全绿（含新增断言）、`lake build` 成功；两侧一致性不升级为 refinement 结论。
