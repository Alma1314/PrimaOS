import Formal.KernelHandle

/-! Finite-table composition of the single-slot lifecycle. Allocation may choose
    any available nonzero slot: this overapproximates ordinary minimum-index
    allocation and fixed-slot reservation, without proving their selection policy.
    Handles are decoded, in-range pairs; raw ABI encoding is outside this model. -/
namespace Formal.KernelHandleTable

open Formal.KernelHandle

abbrev Table (size : Nat) := Fin size → Slot

structure Handle (size : Nat) where
  index : Fin size
  generation : Nat

def replace {size : Nat} (t : Table size) (i : Fin size) (s : Slot) : Table size :=
  fun j => if j = i then s else t j

def WellFormed {size : Nat} (limit : Nat) (t : Table size) : Prop :=
  ∀ i, Formal.KernelHandle.WellFormed limit (t i)

def check {size : Nat} (t : Table size) (h : Handle size) : Bool :=
  if h.index.val = 0 then false else Formal.KernelHandle.check (t h.index) h.generation

def allocateAt {size : Nat} (t : Table size) (i : Fin size) :
    Option (Handle size × Table size) :=
  if i.val = 0 then none else
    match allocate (t i) with
    | none => none
    | some (g, s) => some (⟨i, g⟩, replace t i s)

def releaseAt {size : Nat} (limit : Nat) (t : Table size) (h : Handle size) :
    Option (Table size) :=
  if h.index.val = 0 then none else
    match release limit (t h.index) h.generation with
    | none => none
    | some s => some (replace t h.index s)

/-- Successful table operations have exactly one single-slot effect. -/
theorem allocateAt_effect {size : Nat} {t u : Table size} {i : Fin size}
    {h : Handle size} (ha : allocateAt t i = some (h, u)) :
    h.index = i ∧ allocate (t i) = some (h.generation, u i) ∧
      (∀ j, j ≠ i → u j = t j) := by
  unfold allocateAt at ha
  split at ha
  · cases ha
  · split at ha
    · cases ha
    · rename_i g s hs
      cases ha
      refine ⟨rfl, ?_, ?_⟩
      · simpa [replace] using hs
      · intro j hji; simp [replace, hji]

theorem releaseAt_effect {size limit : Nat} {t u : Table size} {h : Handle size}
    (hr : releaseAt limit t h = some u) :
    release limit (t h.index) h.generation = some (u h.index) ∧
      (∀ j, j ≠ h.index → u j = t j) := by
  unfold releaseAt at hr
  split at hr
  · cases hr
  · split at hr
    · cases hr
    · rename_i s hs
      cases hr
      constructor
      · simpa [replace] using hs
      · intro j hji; simp [replace, hji]

inductive Step {size : Nat} (limit : Nat) : Table size → Table size → Prop where
  | unchanged (t : Table size) : Step limit t t
  | allocated {t u : Table size} {i : Fin size} {h : Handle size} :
      allocateAt t i = some (h, u) → Step limit t u
  | released {t u : Table size} {h : Handle size} :
      releaseAt limit t h = some u → Step limit t u

inductive Reachable {size : Nat} (limit : Nat) : Table size → Table size → Prop where
  | refl (t : Table size) : Reachable limit t t
  | next {t u v : Table size} :
      Reachable limit t u → Step limit u v → Reachable limit t v

/-- Every table trace projects to an allowed trace of each individual slot. -/
theorem step_projects {size limit : Nat} {t u : Table size}
    (hs : Step limit t u) (i : Fin size) : Formal.KernelHandle.Step limit (t i) (u i) := by
  cases hs with
  | unchanged => exact .unchanged _
  | @allocated u j h ha =>
    obtain ⟨_, he, hother⟩ := allocateAt_effect ha
    by_cases hi : i = j
    · subst i; exact .allocated he
    · rw [hother i hi]; exact .unchanged _
  | @released u h hr =>
    obtain ⟨he, hother⟩ := releaseAt_effect hr
    by_cases hi : i = h.index
    · subst i; exact .released he
    · rw [hother i hi]; exact .unchanged _

theorem reachable_projects {size limit : Nat} {t u : Table size}
    (ht : Reachable limit t u) (i : Fin size) :
    Formal.KernelHandle.Reachable limit (t i) (u i) := by
  induction ht with
  | refl => exact .refl _
  | next _ hs ih => exact .next ih (step_projects hs i)

theorem reachable_preserves_wellformed {size limit : Nat} {t u : Table size}
    (hw : WellFormed limit t) (ht : Reachable limit t u) : WellFormed limit u := by
  intro i
  exact Formal.KernelHandle.reachable_preserves_wellformed (hw i) (reachable_projects ht i)

theorem allocate_returns_live {size : Nat} {t u : Table size} {i : Fin size}
    {h : Handle size} (ha : allocateAt t i = some (h, u)) : check u h = true := by
  unfold allocateAt at ha
  split at ha
  · cases ha
  · rename_i hi
    split at ha
    · cases ha
    · rename_i g s hs
      cases ha
      unfold allocate at hs
      split at hs
      · rename_i hg
        cases hs
        simp [check, replace, Formal.KernelHandle.check, hi, hg.1]
      · cases hs

theorem allocate_preserves_other_handle {size : Nat} {t u : Table size} {i : Fin size}
    {allocated other : Handle size} (ha : allocateAt t i = some (allocated, u))
    (hi : other.index ≠ i) : check u other = check t other := by
  simp [check, (allocateAt_effect ha).2.2 other.index hi]

/-- Non-target slots retain their complete records, not merely their validity. -/
theorem release_preserves_other_handle {size limit : Nat} {t u : Table size}
    {freed other : Handle size} (hr : releaseAt limit t freed = some u)
    (hi : other.index ≠ freed.index) : check u other = check t other := by
  simp [check, (releaseAt_effect hr).2 other.index hi]

theorem retired_slot_stays_retired {size limit : Nat} {t u : Table size} {i : Fin size}
    (hz : (t i).generation = 0) (ht : Reachable limit t u) : (u i).generation = 0 := by
  exact Formal.KernelHandle.retired_stays_retired hz (reachable_projects ht i)

/-- Arbitrary allocations, reservations, frees and rejected operations anywhere
    in the finite table cannot revive a previously freed handle. -/
theorem freed_handle_never_live {size limit : Nat} {t freed future : Table size}
    {h : Handle size} (hr : releaseAt limit t h = some freed)
    (ht : Reachable limit freed future) : check future h = false := by
  have he := (releaseAt_effect hr).1
  have hn := Formal.KernelHandle.freed_handle_never_live he (reachable_projects ht h.index)
  simp [check, hn]

-- Concrete anchors; the general theorems above do not use native_decide.
example : allocateAt (fun _ : Fin 3 => (⟨1, false⟩ : Slot)) ⟨0, by omega⟩ = none := by
  native_decide
example : releaseAt 3 (fun _ : Fin 3 => (⟨3, true⟩ : Slot))
    ⟨⟨0, by omega⟩, 3⟩ = none := by native_decide
example : check (replace (fun _ : Fin 3 => (⟨1, true⟩ : Slot)) ⟨1, by omega⟩ ⟨0, false⟩)
    ⟨⟨2, by omega⟩, 1⟩ = true := by native_decide

end Formal.KernelHandleTable
