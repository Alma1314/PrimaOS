import Lean
import Init.Core

/-! BLK-001 状态模型。
    映射用部分函数 `Key → Option Val`，可 `#eval` 亦可证明。
    映照契约：本结构与 `cosir::state` 的 `Capability`/`CapObject` 及
    `State.booted` 一一对应（见 README.md）。 -/

namespace Formal

/-- 对象引用：Segment/Task/ExcSource/Namespace 四类（对齐 cosir `CapObject`）。 -/
inductive Object where
  | segment (id : Nat)
  | task (id : Nat)
  | excSource (id : Nat)
  | namespace (id : Nat)
deriving DecidableEq, Repr

/-- §4.2/§4.4 派生树（seL4 CDT 对应物）：父 CapId 恒小于子 CapId，故本节点内派生天然无环。 -/
def capDerive : UInt16 := 0b0000_0000_0100_0000

/-- 能力（对齐 cosir `Capability` 的 owner/object/rights/active）。
    `parent`：§4.2 派生树的父 CapId，`none` = 随对象创建铸造的**根**。
    内核侧对应 `CapabilityRecord.parent : Option<Handle>`；cosir 侧已建模
    `Capability.parent : Option<CapId>`（2026-10-13，见 README 映照契约）。 -/
structure Capability where
  owner : Nat
  obj : Object
  rights : UInt16
  active : Bool
  parent : Option Nat
deriving DecidableEq, Repr

/-- 状态（对齐 cosir `State` 的最小子集：capabilities + booted）。 -/
structure State where
  /-- 能力表：部分函数，无记录 → 不存在该 CapId。 -/
  caps : Nat → Option Capability
  /-- POSIR bootstrap 标志（对齐 cosir `State.booted`）。 -/
  booted : Bool

/-- 自定义 `Repr`：`caps` 是函数域，无内置 `Repr`，故以标记打印；
    保留可评估结构展示（`#eval (State.mk (fun _ => none) false)`）。
    （偏差说明：原 brief 用 `deriving Repr`，因函数字段无法派生而改为手写。） -/
instance : Repr State where
  reprPrec s _ :=
    Std.Format.text ("{ caps := <function>, booted := " ++ toString s.booted ++ " }")

/-- 初始 init Task 的恒定量 ID（对齐 cosir `abstract_ops::init_task_id()` = 1）。 -/
def initTaskId : Nat := 1

/-- 世界良构：**能力派生树不变量**（seL4 CDT well-formedness 的对应物，非恒真）。
    对每条存在的能力 `cap`（CapId `c`）：
    - 若 `cap` 是派生能力（`parent = some p`），则 `p < c`（§4.2「父 CapId 必须小于子 CapId」，
      本节点内派生天然无环——这是 seL4 CDT 森林性的具体化）；
      且父 `p` 存在、活动、与子**指向同一对象**（§4.2「子 Capability 必须与父 Capability
      指向同一对象」）。

    子权限 ⊆ 父权限（§4.2 第三条）不在此谓词内：它需要 `derive` 时该性质已成立，作为
    `derive` 的前置条件在其保持性定理中携带（见 `Formal/Derive.lean`）。 -/
def WellFormed (s : State) : Prop :=
  ∀ (c : Nat) (cap : Capability), s.caps c = some cap →
    ∀ p, cap.parent = some p →
      p < c ∧ ∃ pcap, s.caps p = some pcap ∧ pcap.active = true ∧ pcap.obj = cap.obj

end Formal
