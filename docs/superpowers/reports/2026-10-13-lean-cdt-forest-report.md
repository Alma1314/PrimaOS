# 形式化推进（参考 seL4）：Lean 证成能力派生树的森林性（无环）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`lean/Formal/Forest.lean`（新增）、`lean/Formal.lean`、`lean/README.md`。
>
> 独立性边界（如实声明）：本报告由同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0「进行中」、BLK-001 / S0-X3 未勾选。

## 1. 定位

承 SPEC-097（把 `WellFormed` 提升为 §4.2 CDT 不变量并证保持性）。seL4 形式化的**头号结论**是「能力派生树（CDT）是一片森林」——即派生关系**无环**。规范 §4.2 仅在散文中声明：

> 父 `CapId` 必须小于子 `CapId`，因此本节点内的派生关系天然无环。

本切片把该散文要求证成**机器检查的定理**：沿父链走任意 `k ≥ 1` 步，CapId 严格递减，故不存在父链环。

## 2. 规范边界澄清（为何不做撤销）

§4.7 明写「**第一版不提供 `cap_revoke`**」：选择性撤销是内核探索期先行落地的特性（实施证据 SPEC-022/023），**不在 POSIR 基线内**；§4.7 还规定「丢弃父 Capability 不影响已经派生的子 Capability」。因此 **POSIR 上的 CDT 故事 = `derive`（§4.4，在规范内）+ 森林性**，撤销不建模（若建模即为把非规范行为当规范）。§4.5 `cap_drop` 与 §4.6 `HeldBy` 位置需独立的 location 表，留待后续切片。

## 3. 变更（`Formal/Forest.lean`）

```
parentOf s c : Option Nat := (s.caps c).bind (fun cap => cap.parent)
ancestry s : Nat → Nat → Option Nat          -- 沿父链走 k 步（k=0 停在 c）
```

- **`parentOf_lt`**（§4.2 单步下降）：`parentOf s c = some p → p < c`，由 `WellFormed` 的「父 `<` 子」直接推出。
- **`ancestry_lt`（森林性/无环）**：`ancestry s k c = some d → 0 < k → d < c`。**对 `k` 归纳**：`k = succ` 时按 `k = 0` / `k > 0` 分路，前者落到 `parentOf_lt`，后者用 `Nat.lt_trans (parentOf_lt …) (ih …)`。**非平凡**。
- **`ancestry_no_cycle`**：不存在 `k ≥ 1` 使 `ancestry s k c = some c`（否则 `c < c`，`Nat.lt_irrefl`）。
- 3 条 `native_decide` 可执行锚：booted 态 CapId 1（根）走 1 步得 `none`；派生 CapId 4 后走 1 步得 `some 1`、走 2 步得 `none`。

## 4. 影响面

- **仅 Lean 侧**：`cosir`/`kernel`/`xtask` 的 Rust 代码**零改动**。
- `lean/README.md`：模块表新增 `Formal.Forest`；「定理性质」段由「3 条实质证明」改为「5 条」（新增 `parentOf_lt`、`ancestry_lt`）；CDT 覆盖边界注明**无环性已独立证成**（不止于不变量中的局部「父 `<` 子」）。

## 5. 门禁

- `lake build`（`lean/`）⇒ **成功**（8 jobs），**零 warning、零 `sorry`/`admit`**。
- `cargo test --workspace` ⇒ 全绿：cosir **146**、cosir bin **42**、lean_backtrace **2**、kernel **367**、prima-ostd **21**、xtask **204**。
- 本切片**未触碰 Rust 代码**，无回归面。

## 6. 未消项 / 边界（如实登记）

- **cosir 侧无 CDT**（`ISSUE-FORMAL-001` 未消）：`state::Capability` 无 `parent`，仍无对拍项；本切片同为 **Lean-ahead**。
- 未建模 §4.5 `cap_drop`、§4.6 `HeldBy` 位置、W3 唯一位置；**不建模** §4.7 选择性撤销（规范第一版不提供）。
- 仍**不宣称** `REFINEMENT-PROOF`（内核 C 级细化）、`KERNEL-TRACE`、`COMPAT-TEST` 或机器证明。
- 闭合判断交复核者。
