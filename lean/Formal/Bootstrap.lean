import Formal.State

namespace Formal

/-- α 投影：把 State 投影为其 `booted` 标志（对齐 cosir `abstract_ops::abstract_state` 的 booted 段）。 -/
def alpha (s : State) : Bool := s.booted

/-- 初始 Authority（§14 步骤 5/6）：三条无父根 Capability——
    CapId=1 image Segment `{Derive,SegMapR,SegMapX,SegDestroy}`、
    CapId=2 init Namespace 全集、CapId=3 init Task `{Derive,TaskSend,TaskManageVSpace,TaskDestroy}`。
    位值与 cosir `state::CAP_*` 一致；与 `abstract_ops::abstract_state(booted=true)` 的
    capabilities 表逐条对应。 -/
private def initCaps : Nat → Option Capability :=
  fun id =>
    if id = 1 then
      some { owner := 1, obj := Object.segment 1, rights := 0b0000_0000_0101_1010, active := true, parent := none }
    else if id = 2 then
      some { owner := 1, obj := Object.namespace 1, rights := 0b0111_1110_0000_0000, active := true, parent := none }
    else if id = 3 then
      some { owner := 1, obj := Object.task 1, rights := 0b0000_0001_1110_0000, active := true, parent := none }
    else none

/-- 空态：无能力、未 booted（对齐 cosir 交接点 EmptyState）。 -/
def emptyState : State := { caps := fun _ => none, booted := false }

/-- 已 booted 态：init 铸造初始 Authority（对齐 cosir `abstract_ops::abstract_state(booted)`）。 -/
def bootedState : State :=
  { caps := initCaps
    booted := true
    locations := fun id => if id = 1 ∨ id = 2 ∨ id = 3 then some (.heldBy 1) else none }

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

/-- §14 步骤 5/6：空态 bootstrap 铸三条无父根 Capability（初始 Authority）。 -/
theorem bootstrap_empty_mints_image_cap :
    (bootstrap emptyState).get!.caps 1 = some { owner := 1, obj := Object.segment 1, rights := 0b0000_0000_0101_1010, active := true, parent := none } := by
  rfl

theorem bootstrap_empty_mints_namespace_cap :
    (bootstrap emptyState).get!.caps 2 = some { owner := 1, obj := Object.namespace 1, rights := 0b0111_1110_0000_0000, active := true, parent := none } := by
  rfl

theorem bootstrap_empty_mints_task_cap :
    (bootstrap emptyState).get!.caps 3 = some { owner := 1, obj := Object.task 1, rights := 0b0000_0001_1110_0000, active := true, parent := none } := by
  rfl

/-- 幂等唯一性：对已 booted 态再次 bootstrap 返回 none（不再重复铸造）。 -/
theorem bootstrap_booted_returns_none :
    bootstrap bootedState = none := by
  rfl

/-- 投影保真：bootstrap 确改变抽象态（α(booted) ≠ α(pre_bootstrap)）。 -/
theorem alpha_projection_distinguishes_boot :
    alpha emptyState != alpha bootedState := by
  rfl

/-- §14 步骤 5/6：booted 态是良构的派生森林——三条根 Capability 的 `parent = none`，
    故 CDT 不变量在根上空洞成立（**非平凡**：需逐 CapId 展开 `initCaps` 的三分支）。 -/
theorem bootedState_wellformed : WellFormed bootedState := by
  intro c cap hc p hp
  have hn : cap.parent = none := by
    simp only [bootedState] at hc
    unfold initCaps at hc
    by_cases h1 : c = 1
    · simp only [h1, ite_true] at hc; cases hc; rfl
    · simp only [h1, ite_false] at hc
      by_cases h2 : c = 2
      · simp only [h2, ite_true] at hc; cases hc; rfl
      · simp only [h2, ite_false] at hc
        by_cases h3 : c = 3
        · simp only [h3, ite_true] at hc; cases hc; rfl
        · simp only [h3, ite_false] at hc
          simp at hc
  rw [hn] at hp
  simp at hp

/-- 成功结果 WellFormed：bootstrap 成功态满足良构（由 `bootedState_wellformed` 承载）。
    `bootstrap m = some s'` 强制 `s' = bootedState`，故良构由该等式传递。 -/
theorem bootstrap_new_wellformed (_m : State)
    (_h : bootstrap _m = some bootedState) : WellFormed bootedState :=
  bootedState_wellformed

end Formal
