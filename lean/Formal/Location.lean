import Formal.State

/-! Atomic location/carrier update, independent of object-specific preflight.
No claim about FIFO scheduling, object lifetime, namespace parsing, or Rust
refinement follows from this model. Both indexes are represented independently
so consistency is a preservation obligation, not a representation tautology. -/
namespace Formal.TokenLocation

structure Snapshot where
  state : Formal.State
  carriers : Location → Nat → Bool
  structural : Nat → Bool

def Consistent (s : Snapshot) : Prop :=
  (∀ c loc, s.carriers loc c = true ↔ s.state.locations c = some loc) ∧
  (∀ c loc, s.state.locations c = some loc → ∃ cap, s.state.caps c = some cap)

def replace (s : Snapshot) (c : Nat) (destination : Option Location) : Snapshot :=
  { s with
    state := { s.state with locations := fun id => if id = c then destination else s.state.locations id }
    carriers := fun loc id => if id = c then decide (destination = some loc) else s.carriers loc id }

/-- Failure returns none; history and carrier state are never partly changed. -/
def transfer (s : Snapshot) (caller c : Nat) (destination : Location) : Option Snapshot :=
  if s.state.locations c = some (.heldBy caller) ∧ s.structural c = false then
    some (replace s c (some destination))
  else none

def dropToken (s : Snapshot) (caller c : Nat) : Option Snapshot :=
  if s.state.locations c = some (.heldBy caller) ∧ s.structural c = false then
    some (replace s c none)
  else none

theorem replace_preserves_consistency (s : Snapshot) (c : Nat) (dest : Option Location)
    (h : Consistent s) (present : ∃ cap, s.state.caps c = some cap) :
    Consistent (replace s c dest) := by
  constructor
  · intro id loc
    by_cases eq : id = c
    · simp [replace, eq]
    · simpa [replace, eq] using h.1 id loc
  · intro id loc located
    by_cases eq : id = c
    · subst id
      simpa [replace] using present
    · apply h.2 id loc
      simpa [replace, eq] using located

theorem transfer_preserves_consistency (s after : Snapshot) (caller c : Nat) (dest : Location)
    (h : Consistent s) (success : transfer s caller c dest = some after) : Consistent after := by
  unfold transfer at success
  split at success
  · rename_i guard
    cases success
    exact replace_preserves_consistency s c (some dest) h (h.2 c (.heldBy caller) guard.1)
  · simp at success

theorem drop_preserves_consistency (s after : Snapshot) (caller c : Nat)
    (h : Consistent s) (success : dropToken s caller c = some after) : Consistent after := by
  unfold dropToken at success
  split at success
  · rename_i guard
    cases success
    exact replace_preserves_consistency s c none h (h.2 c (.heldBy caller) guard.1)
  · simp at success

/-- A token cannot occupy two distinct containers, including after any move. -/
theorem unique_carrier (s : Snapshot) (h : Consistent s) (c : Nat) (a b : Location)
    (ha : s.carriers a c = true) (hb : s.carriers b c = true) : a = b := by
  have ea := (h.1 c a).mp ha
  have eb := (h.1 c b).mp hb
  exact Option.some.inj (ea.symm.trans eb)

theorem replace_keeps_all_records (s : Snapshot) (c : Nat) (dest : Option Location) :
    (replace s c dest).state.caps = s.state.caps := by rfl

theorem drop_cannot_erase_descendant_records (s after : Snapshot) (caller c id : Nat)
    (success : dropToken s caller c = some after) : after.state.caps id = s.state.caps id := by
  unfold dropToken at success
  split at success
  · cases success; rfl
  · simp at success

theorem transfer_requires_current_holder (s after : Snapshot) (caller c : Nat) (dest : Location)
    (success : transfer s caller c dest = some after) : s.state.locations c = some (.heldBy caller) := by
  unfold transfer at success
  split at success
  · rename_i guard; exact guard.1
  · simp at success

theorem structural_transfer_fails (s : Snapshot) (caller c : Nat) (dest : Location)
    (locked : s.structural c = true) : transfer s caller c dest = none := by
  simp [transfer, locked]

end Formal.TokenLocation
