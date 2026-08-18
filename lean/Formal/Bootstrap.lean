import Formal.State

namespace Formal

/-- α 投影：把 State 投影为其 `booted` 标志（对齐 cosir `abstract_ops::abstract_state` 的 booted 段）。 -/
def alpha (s : State) : Bool := s.booted

/-- 初始能力表：init Task 持有对自身 Object.task 的主权（初始 Authority 的最小表示）。
    仅在 CapId=1 处有记录，其余处为空——与 cosir 空状态/初始 Authority 对应。 -/
private def initCaps : Nat → Option Capability :=
  fun id => if id = 1 then
      some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true }
    else none

/-- 空态：无能力、未 booted（对齐 cosir 交接点 EmptyState）。 -/
def emptyState : State := { caps := fun _ => none, booted := false }

/-- 已 booted 态：init 铸造初始 Authority（对齐 cosir `abstract_ops::abstract_state(booted)`）。 -/
def bootedState : State := { caps := initCaps, booted := true }

/-- bootstrap：空态 → booted 态；对已 booted 态返回 none（AlreadyBooted 且状态不变）。 -/
def bootstrap (s : State) : Option State :=
  if s.booted then none else some bootedState

/-- `State` 的函数字段令其不可自动派生 `Inhabited`；以 `emptyState` 为默认值补足，
    供 `.get!` 在 `bootstrap emptyState` 处使用（证据仅说明性，不影响语义）。 -/
instance : Inhabited State where
  default := emptyState

/-- 幂等唯一性：空态 bootstrap 成功，且结果 booted=true、init task 存在（CapId=1 有记录）。 -/
theorem bootstrap_empty_succeeds :
    alpha (bootstrap emptyState).get! = true := by
  rfl

theorem bootstrap_empty_mints_init_cap :
    (bootstrap emptyState).get!.caps 1 = some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true } := by
  rfl

/-- 幂等唯一性：对已 booted 态再次 bootstrap 返回 none（不再重复铸造）。 -/
theorem bootstrap_booted_returns_none :
    bootstrap bootedState = none := by
  rfl

/-- 投影保真：bootstrap 确改变抽象态（α(booted) ≠ α(pre_bootstrap)）。 -/
theorem alpha_projection_distinguishes_boot :
    alpha emptyState != alpha bootedState := by
  rfl

/-- 成功结果 WellFormed：bootstrap 成功态满足良构（最小模型恒真）。 -/
theorem bootstrap_new_wellformed (m : State)
    (h : bootstrap m = some bootedState) : WellFormed bootedState := by
  trivial

end Formal
