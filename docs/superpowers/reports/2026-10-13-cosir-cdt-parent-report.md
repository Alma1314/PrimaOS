# cosir 侧补能力派生树 `parent` 并建模 CDT（关 ISSUE-FORMAL-001）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`cosir/src/state.rs`、`cosir/src/abstract_ops.rs`、`cosir/src/{cow_refine.rs,cow_refine_tests.rs,main.rs}`、`cosir/tests/lean_backtrace.rs`、`lean/README.md`。
>
> 独立性边界（如实声明）：本报告由同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0「进行中」、BLK-001 / S0-X3 未勾选。

## 1. 定位

承 SPEC-097/098：Lean 侧已引入 `Capability.parent`、非平凡 `WellFormed`（§4.2 CDT 不变量），并证成 `derive` 保持性与**森林性/无环**（`ancestry_lt`）。但 cosir 侧 `state::Capability` **无 `parent` 字段**，故 `lean_backtrace` 对拍缺位——登记为 `ISSUE-FORMAL-001`（Lean-ahead 分歧）。本切片消除该分歧：**cosir 补 `parent` 并建模同款 CDT 谓词/派生操作，令对拍逐项成立**。内核侧语义早已完备（`CapRecord.parent`、`derive_capability`、`CapabilityDerived`，`kernel/src/bootstrap.rs:1095/3862`），故这是**补实现缺口**，非改规范。

## 2. 变更

### 2.1 `state::Capability` 增字段（`cosir/src/state.rs`）

```rust
pub struct Capability {
    pub owner: TaskId,
    pub object: CapObject,
    pub rights: u16,
    pub active: bool,
    pub parent: Option<CapId>,   // 新增：None = 铸造根；Some(p) = 由 p 派生（§4.2/§4.4）
}
```

**21 处构造点**机械补 `parent: None`（`abstract_ops.rs` 6、`cow_refine.rs` 9、`cow_refine_tests.rs` 3、`main.rs` 3）——全部是**铸造根**（§14 三条根 Capability 与 `create_segment` 的随对象铸造），语义上 `None` 正确。

### 2.2 cosir 侧 CDT 谓词与派生（`cosir/src/abstract_ops.rs`）

- **`well_formed(&State) -> bool`**：镜像 Lean `Formal.WellFormed`——每条带父能力满足 `parent < c`、父存在、父活动、父子同对象（§4.2）。**非恒真**。
- **`derive_capability(&mut State, parent, new_id) -> Result<(), DeriveError>`**：镜像 Lean `Formal.derive`（§4.4）——守卫为「父存在 ∧ 父活动 ∧ 父含 `CapDerive` ∧ `parent < new_id` ∧ 空槽」，子能力 = 父能力副本但 `parent := Some(parent)`；**失败原子**（先查后写）。失败集 `DeriveError::{NoSuchParent, ParentInactive, NoDeriveRight, OrderViolation, SlotOccupied}` 与守卫各分支一一对应。

### 2.3 对拍（`cosir/tests/lean_backtrace.rs` 新增 `cdt_parity_lean`）

| cosir 断言 | 对应 Lean |
|---|---|
| 三条根 Cap（1/2/3）`parent == None` | `bootedState` 的 `parent := none`（`bootstrap_empty_mints_*`） |
| `well_formed(booted)` | `bootedState_wellformed` |
| `derive_capability(1, 4)` 后 `cap[4].parent == Some(1)`、父子 `object` 相同 | `derive bootedState 1 4` → `some 1`（`Forest` 锚） |
| 派生后 `well_formed` 仍成立 | `derive_preserves_wellformed` |
| 沿父链严格递减并终止于根（无环） | `ancestry_lt` / `ancestry_no_cycle` |
| 4 条失败锚：父 ≥ 子 / 无父 / 缺 `CapDerive`（CapId 2）/ 槽占用，且失败原子不变 | `derive` 的 4 条 `native_decide` 锚 |

## 3. 影响面

- **仅 cosir + 文档**：`kernel`/`xtask`/`prima-ostd` 的代码零改动（工作树内 kernel 的未提交改动 `exc_source_destroy` 与本切片无关，未触碰）。
- `lean/README.md`：映照契约中 `Capability.parent` 行由「cosir 不建模（Lean-ahead）」改为**三方一致**；`WellFormed` 行补 cosir 对应 `abstract_ops::well_formed`；对拍机制新增 `cdt_parity_lean` 条目；「已知不覆盖」的 CDT 分歧**标记闭合**，残留 §4.5/§4.6/W3/§4.7。

## 4. 门禁

- `cargo test --workspace` ⇒ 全绿：cosir **146**、cosir bin **42**、**lean_backtrace 3**（+1 `cdt_parity_lean`）、kernel **368**、prima-ostd **21**、xtask **204**。
- `cargo clippy --workspace --all-targets -- -D warnings` ⇒ **零告警**。
- 本切片**未触碰 Lean 代码**，`lake build` 无回归面。

## 5. 未消项 / 边界（如实登记）

- `derive_capability` 是**对拍用 host 操作**（忠实镜像 Lean 模型语义），**不是内核 C 级细化**；不宣称 `REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST` 或机器证明。
- cosir 侧 `well_formed` 是 Rust 侧布尔断言，**非** Lean 式机器检查；其强度受限于所断言的具体状态。
- **仍不建模**：§4.5 `cap_drop`、§4.6 `HeldBy` 位置、W3 唯一位置、§4.7 撤销（规范第一版不提供 `cap_revoke`）。
- 闭合判断交复核者。
