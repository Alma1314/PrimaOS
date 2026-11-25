import Formal.Location

/-! `cap_drop` reference semantics (COSIR §4.5): drop deletes exactly the caller's
    single `HeldBy` position record, never propagates to derived children, never
    erases capability records, and refuses structural roots. It is orthogonal to
    the selective revocation of `Formal.Revoke` (COSIR §4.7 / Prima OS §6.2).

    Reuses `Formal.TokenLocation.Snapshot` and `dropToken`; this module adds the
    spec-level properties (non-propagation, record preservation, CDT well-formedness
    preservation) rather than re-implementing the atomic location update. -/
namespace Formal.Drop

open Formal
open Formal.TokenLocation

def drop (s : Snapshot) (caller c : Nat) : Option Snapshot :=
  dropToken s caller c

theorem drop_structural_fails (s : Snapshot) (caller c : Nat)
    (locked : s.structural c = true) : drop s caller c = none := by
  simp [drop, dropToken, locked]

theorem drop_wrong_holder_fails (s : Snapshot) (caller c : Nat)
    (h : s.state.locations c ≠ some (.heldBy caller)) : drop s caller c = none := by
  unfold drop dropToken
  simp [h]

theorem drop_clears {s after : Snapshot} {caller c : Nat}
    (success : drop s caller c = some after) : after.state.locations c = none := by
  unfold drop dropToken at success
  split at success
  · cases success
    simp [replace]
  · simp at success

theorem drop_keeps_other {s after : Snapshot} {caller c other : Nat}
    (success : drop s caller c = some after) (hne : other ≠ c) :
    after.state.locations other = s.state.locations other := by
  unfold drop dropToken at success
  split at success
  · cases success
    simp [replace, hne]
  · simp at success

/-- `cap_drop` does not erase capability records (children survive as tokens). -/
theorem drop_keeps_caps {s after : Snapshot} {caller c : Nat}
    (success : drop s caller c = some after) :
    after.state.caps = s.state.caps := by
  unfold drop dropToken at success
  split at success
  · cases success
    exact replace_keeps_all_records s c none
  · simp at success

/-- The core §4.5 statement: dropping a parent leaves a derived child's position
    untouched; `cap_drop` does not propagate. -/
theorem drop_does_not_propagate {s after : Snapshot} {caller parent child : Nat}
    {loc : Location} (success : drop s caller parent = some after)
    (hchild : s.state.locations child = some loc) (hne : child ≠ parent) :
    after.state.locations child = some loc := by
  rw [drop_keeps_other success hne]
  exact hchild

/-- The caller's own record is gone, while the child record remains: drop is a
    single-position operation, not a subtree operation. -/
theorem drop_parent_child_independent {s after : Snapshot} {caller parent child : Nat}
    {loc : Location} (success : drop s caller parent = some after)
    (hchild : s.state.locations child = some loc) (hne : child ≠ parent) :
    after.state.locations parent = none ∧ after.state.locations child = some loc := by
  refine ⟨drop_clears success, ?_⟩
  rw [drop_keeps_other success hne]
  exact hchild

/-- Dropping does not change the CDT invariant: capability records and parent
    pointers are untouched. -/
theorem drop_preserves_wellformed {s after : Snapshot} {caller c : Nat}
    (hwf : WellFormed s.state) (success : drop s caller c = some after) :
    WellFormed after.state := by
  unfold WellFormed at hwf ⊢
  rw [drop_keeps_caps success]
  exact hwf

theorem drop_consistency {s after : Snapshot} {caller c : Nat}
    (h : Consistent s) (success : drop s caller c = some after) : Consistent after :=
  drop_preserves_consistency s after caller c h success

/-- A second drop of the same token fails: the position is already gone. -/
theorem drop_idempotent {s after : Snapshot} {caller c : Nat}
    (success : drop s caller c = some after) :
    drop after caller c = none := by
  apply drop_wrong_holder_fails
  rw [drop_clears success]
  simp

-- Executable anchors: a root (id 1, structural) and a derived child (id 2), both
-- held by caller 9; dropping the child leaves the root record intact.

def rootCap : Capability :=
  { owner := 9, obj := .segment 7, rights := 0, active := true, parent := none }

def childCap : Capability :=
  { owner := 9, obj := .segment 7, rights := 0, active := true, parent := some 1 }

def dropState : Formal.State :=
  { caps := fun id => if id = 1 then some rootCap else if id = 2 then some childCap else none
    booted := true
    locations := fun id =>
      if id = 1 then some (.heldBy 9) else if id = 2 then some (.heldBy 9) else none }

def dropSnap : Snapshot :=
  { state := dropState
    carriers := fun loc id =>
      match loc with
      | .heldBy n => n == 9 && (id == 1 || id == 2)
      | _ => false
    structural := fun id => id == 1 }

#eval (drop dropSnap 9 2).isSome                                -- true
#eval (drop dropSnap 9 1).isSome                                -- false（structural root）
#eval (drop dropSnap 8 2).isSome                                -- false（wrong holder）
#eval ((drop dropSnap 9 2).map (fun s => s.state.locations 2))  -- some none
#eval ((drop dropSnap 9 2).map (fun s => s.state.locations 1))  -- some (some (heldBy 9))：父记录不受影响
#eval ((drop dropSnap 9 2).map (fun s => (s.state.caps 2).isSome)) -- true：子记录保留

end Formal.Drop
