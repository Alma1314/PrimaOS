# 形式化推进（参考 seL4）：Lean 模型的能力派生树（CDT）不变量与保持性证明

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`lean/Formal/State.lean`、`lean/Formal/Bootstrap.lean`、`lean/Formal/ExcResume.lean`、`lean/Formal/Derive.lean`（新增）、`lean/Formal.lean`、`lean/README.md`。
>
> 独立性边界（如实声明）：本报告由同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0「进行中」、BLK-001 / S0-X3 未勾选。

## 1. 动机与定位（为何以 seL4 为范本）

seL4 形式化验证的骨架是 **能力派生树（capability derivation tree, CDT）**：能力构成森林，每个能力至多一个父，父 `CapId` 严格小于子 `CapId`（因此无环），子与父指向同一对象、子权限 ⊆ 父权限。所有迁移必须**保持**该不变量。

本项目是能力制 OS，POSIR 的能力系统与 seL4 同构：规范 §4.2 明写「父 `CapId` 必须小于子 `CapId`，因此本节点内的派生关系天然无环」，内核亦已实现（`CapRecord.parent: Option<Handle>`、`derive_capability`、`CapabilityDerived` 审计，见 `kernel/src/bootstrap.rs:1095/3862`）。

但形式模型**此前是空洞的**：`WellFormed := True` 恒真，11 条 `theorem` 全为 `by rfl`/`trivial`（定义展开），不提供任何独立的不变量保证。这正是与 seL4 方法落差最大之处。

本切片把 `WellFormed` 从恒真提升为**§4.2 的 CDT 森林不变量**，并证明其在既有操作（`bootstrap`/`excResume`）与新操作（`derive`）下**保持**——三条均为非平凡证明。这是形式模型首次具备「实质不变量 + 保持性证据」。

## 2. 变更

### 2.1 `Formal/State.lean`

- `Capability` 增字段 `parent : Option Nat`（`none` = 随对象创建铸造的根）；常量 `capDerive : UInt16`。
- `WellFormed` 由 `fun _ => True` 改为：

```
WellFormed s := ∀ c cap, s.caps c = some cap →
  ∀ p, cap.parent = some p →
    p < c ∧ ∃ pcap, s.caps p = some pcap ∧ pcap.active = true ∧ pcap.obj = cap.obj
```

即 §4.2 的三条：父 `<` 子（无环）、父存在且活动、父子同对象。

### 2.2 `Formal/Bootstrap.lean`

- `initCaps` 三条根 Cap 置 `parent := none`；三条 `bootstrap_empty_mints_*` 具体值加 `parent := none`。
- **`bootedState_wellformed : WellFormed bootedState`**：逐 `CapId` 展开 `initCaps` 三分支（`by_cases` + `cases hc`），证三条根满足不变量（根 `parent = none` ⇒ 空洞为真）。**非平凡**。
- `bootstrap_new_wellformed` 改由 `bootedState_wellformed` 承载。

### 2.3 `Formal/ExcResume.lean`

- `exc_resume_success_keeps_source_cap` 加 `parent := none`。
- **`exc_resume_preserves_wellformed`**：`excResume` 成功返回原状态 `some s`、失败返回 `none`，故良构不变。**非平凡**（`unfold` + `split`）。

### 2.4 `Formal/Derive.lean`（新增）

- `update`/`update_self`/`update_of_ne`：能力表单点更新（`Function.update` 的最小等价物——本模型只 `import Init.Core`，无该 API）。
- `deriveGuard`：§4.4 前置（父存在、活动、含 `CAP_DERIVE`、`parent < newId`、槽位空）。
- `derive s parent newId : Option State`：§4.4 `cap_derive` 对应物（新能力 = 父的副本 + `parent := some parent`）。
- **`derive_preserves_wellformed`**：良构在派生下保持。证明按「新槽位 `c = newId` / 旧槽位 `c ≠ newId`」两路：新槽位用守卫的 `parent < newId` 与父槽位未变（`parent < newId ⇒ parent ≠ newId`）；旧槽位落回 `hwf`，并需证其父槽位 `q ≠ newId`（否则与空槽前提 `s.caps newId = none` 冲突，由 `hwf` 的「父存在」推出）。**非平凡**。
- **`no_self_parent`**：无自环（`parent < c` 的直接推论）。
- 5 条 `native_decide` 可执行锚：派生成功且新槽位非空、`parent = newId` 失败、父不存在失败、父无 `CAP_DERIVE`（CapId 2，Namespace 根）失败。

## 3. 影响面

- **仅 Lean 侧**：`cosir`、`kernel`、`xtask` 的 Rust 代码**零改动**。
- 映照契约（`lean/README.md`）据实更新：新增 `Capability.parent` 行（标注为 **Lean-ahead**：cosir 无该字段）、`WellFormed` 行（改「非恒真」，cosir 无对应谓词）；「定理性质」段由「11 条全 `by rfl`」改写为「3 条实质证明 + 其余具体实例/求值」。
- 新增 `ISSUE-FORMAL-001`（cosir 侧 CDT 未建模，Lean-ahead 分歧，下一形式化切片入口）。

## 4. 门禁

- `lake build`（`lean/`）⇒ **成功**（7 jobs：`Formal.State`/`Bootstrap`/`ExcResume`/`Derive`/`Formal` 全编译），**零 warning、零 `sorry`/`admit`**。
- `cargo test --workspace` ⇒ 全绿：cosir **146**、cosir bin **42**、lean_backtrace **2**、kernel **367**、prima-ostd **21**、xtask **204**。
- 本切片**未触碰 Rust 代码**，故未跑 `cargo xtask test`/`lint`/`tcb`/`compat`（无回归面）；对拍测试 `lean_backtrace` 不受 Lean 模型新增 `parent` 字段影响（其断言在 cosir 侧）。

## 5. 未消项 / 边界（如实登记）

- **cosir 侧无 CDT**（`ISSUE-FORMAL-001`）：`state::Capability` 无 `parent`，故 §4.2 根性/派生链在 cosir 侧无对应断言、无对拍项。本切片为 **Lean-ahead**。
- `WellFormed` **未**表达「子权限 ⊆ 父权限」（该性质作为 `derive` 的前置条件在保持性定理中携带，非不变量本身）；亦未表达 §4.6 `HeldBy` 位置与 W3 唯一位置。
- **不建模** §4.5 撤销（`revoke`）与在途消息；§4.5 的「撤销域」是 seL4 CDT 的另一核心义务，留待后续切片（与 `ISSUE-FORMAL-001` 同批）。
- 仍**不宣称** `REFINEMENT-PROOF`（内核 C 级细化）、`KERNEL-TRACE`、`COMPAT-TEST` 或机器证明；本条属 `REFINEMENT-DESIGN`（独立 Lean 证明通道）。
- 闭合判断交复核者。
