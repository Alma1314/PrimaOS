import Lean
import Init.Core

/-! BLK-001 状态模型。
    映射用部分函数 `Key → Option Val`，可 `#eval` 亦可证明。
    映照契约：本结构与 `cosir::state` 的 `Capability`/`CapObject` 及
    `State.booted` 一一对应（见 README.md）。 -/

namespace Formal

/-- 对象引用：SegId/TaskId/ExcSourceId 三类（对齐 cosir `CapObject`）。 -/
inductive Object where
  | segment (id : Nat)
  | task (id : Nat)
  | excSource (id : Nat)
deriving DecidableEq, Repr

/-- 能力（对齐 cosir `Capability` 的 owner/object/rights/active）。 -/
structure Capability where
  owner : Nat
  obj : Object
  rights : UInt8
  active : Bool
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

/-- 世界良构：capabilities 域有限。本谓词在最小模型里恒真，作为 WellFormed 的锚。 -/
def WellFormed (s : State) : Prop := True

end Formal
