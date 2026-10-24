import Lean

/-! Single physical capability slot, separate from the abstract CDT.
    Nat models a checked bounded counter; zero is the permanent retirement marker.
    See README.md for the Rust mapping and limits of this proof channel. -/
namespace Formal.KernelHandle

def maxGeneration : Nat := 2 ^ 32 - 1

structure Slot where
  generation : Nat
  inUse : Bool
deriving DecidableEq, Repr

def WellFormed (limit : Nat) (s : Slot) : Prop :=
  s.generation ≤ limit ∧ (s.generation = 0 → s.inUse = false)

def check (s : Slot) (handle : Nat) : Bool :=
  decide (handle ≠ 0 ∧ s.inUse = true ∧ s.generation = handle)

def allocate (s : Slot) : Option (Nat × Slot) :=
  if s.generation ≠ 0 ∧ s.inUse = false then
    some (s.generation, { s with inUse := true })
  else none

def release (limit : Nat) (s : Slot) (handle : Nat) : Option Slot :=
  if handle ≠ 0 ∧ s.inUse = true ∧ s.generation = handle then
    some { generation := if handle < limit then handle + 1 else 0, inUse := false }
  else none

theorem allocate_preserves_wellformed {limit : Nat} {s t : Slot} {handle : Nat}
    (hwf : WellFormed limit s) (h : allocate s = some (handle, t)) :
    WellFormed limit t := by
  unfold allocate at h
  split at h
  · rename_i hg
    cases h
    exact ⟨hwf.1, fun hz => (hg.1 hz).elim⟩
  · cases h

theorem release_preserves_wellformed {limit : Nat} {s t : Slot} {handle : Nat}
    (h : release limit s handle = some t) : WellFormed limit t := by
  unfold release at h
  split at h
  · cases h
    constructor
    · split <;> dsimp <;> omega
    · intro _; rfl
  · cases h

theorem retired_cannot_allocate (s : Slot) (h : s.generation = 0) :
    allocate s = none := by simp [allocate, h]

theorem retired_cannot_release (limit : Nat) (s : Slot) (h : s.generation = 0)
    (handle : Nat) : release limit s handle = none := by
  simp [release, h]
  intro hn _ hz
  exact hn hz.symm

/-- An old generation has been passed or its physical slot has been retired. -/
def Invalidated (old : Nat) (s : Slot) : Prop :=
  s.generation = 0 ∨ old < s.generation

theorem release_invalidates {limit old : Nat} {s t : Slot}
    (h : release limit s old = some t) : Invalidated old t := by
  unfold release at h
  split at h
  · cases h
    unfold Invalidated
    split <;> simp_all
  · cases h

/-- Successful allocate/release, plus unchanged state for rejected operations
    or unrelated slots. No reset or generation-rewriting transition is allowed. -/
inductive Step (limit : Nat) : Slot → Slot → Prop where
  | unchanged (s : Slot) : Step limit s s
  | allocated {s t : Slot} {handle : Nat} :
      allocate s = some (handle, t) → Step limit s t
  | released {s t : Slot} {handle : Nat} :
      release limit s handle = some t → Step limit s t

inductive Reachable (limit : Nat) : Slot → Slot → Prop where
  | refl (s : Slot) : Reachable limit s s
  | next {s t u : Slot} : Reachable limit s t → Step limit t u → Reachable limit s u

theorem step_preserves_wellformed {limit : Nat} {s t : Slot}
    (hwf : WellFormed limit s) (h : Step limit s t) : WellFormed limit t := by
  cases h with
  | unchanged => exact hwf
  | allocated ha => exact allocate_preserves_wellformed hwf ha
  | released hr => exact release_preserves_wellformed hr

theorem reachable_preserves_wellformed {limit : Nat} {s t : Slot}
    (hwf : WellFormed limit s) (h : Reachable limit s t) : WellFormed limit t := by
  induction h with
  | refl => exact hwf
  | next _ hs ih => exact step_preserves_wellformed ih hs

theorem retired_stays_retired {limit : Nat} {s t : Slot}
    (hz : s.generation = 0) (h : Reachable limit s t) : t.generation = 0 := by
  induction h with
  | refl => exact hz
  | next _ hs ih =>
      cases hs with
      | unchanged => exact ih
      | allocated ha => rw [retired_cannot_allocate _ ih] at ha; cases ha
      | released hr => rw [retired_cannot_release _ _ ih] at hr; cases hr

theorem step_preserves_invalidation {limit old : Nat} {s t : Slot}
    (hi : Invalidated old s) (h : Step limit s t) : Invalidated old t := by
  cases h with
  | unchanged => exact hi
  | allocated ha =>
      unfold allocate at ha
      split at ha
      · cases ha; exact hi
      · cases ha
  | released hr =>
      unfold release at hr
      split at hr
      · rename_i hg
        cases hr
        unfold Invalidated at *
        split
        · right; dsimp; rcases hi with hz | hlt <;> omega
        · exact Or.inl rfl
      · cases hr

/-- General safety result over arbitrarily long operation traces. -/
theorem reachable_preserves_invalidation {limit old : Nat} {s t : Slot}
    (hi : Invalidated old s) (h : Reachable limit s t) : Invalidated old t := by
  induction h with
  | refl => exact hi
  | next _ hs ih => exact step_preserves_invalidation ih hs

theorem invalidated_not_live {old : Nat} {s : Slot} (hn : old ≠ 0)
    (hi : Invalidated old s) : check s old = false := by
  unfold check Invalidated at *
  rcases hi with hz | hlt <;> simp_all <;> omega

/-- Once a valid handle is freed, no later allowed operation can authorize it. -/
theorem freed_handle_never_live {limit old : Nat} {s freed future : Slot}
    (hr : release limit s old = some freed) (ht : Reachable limit freed future) :
    check future old = false := by
  have hn : old ≠ 0 := by
    unfold release at hr
    split at hr
    · rename_i hg; exact hg.1
    · cases hr
  exact invalidated_not_live hn
    (reachable_preserves_invalidation (release_invalidates hr) ht)

-- Executable anchors paired with the Rust boundary tests; no universal claim
-- relies on native_decide.
example : release maxGeneration ⟨maxGeneration - 1, true⟩ (maxGeneration - 1) =
    some ⟨maxGeneration, false⟩ := by native_decide
example : allocate ⟨maxGeneration, false⟩ =
    some (maxGeneration, ⟨maxGeneration, true⟩) := by native_decide
example : release maxGeneration ⟨maxGeneration, true⟩ maxGeneration =
    some ⟨0, false⟩ := by native_decide
example : allocate ⟨0, false⟩ = none := by native_decide
example : release maxGeneration ⟨1, false⟩ 1 = none := by native_decide
example : release maxGeneration ⟨2, true⟩ 1 = none := by native_decide

end Formal.KernelHandle
