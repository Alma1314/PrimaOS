import Formal.Location

/-! §4.6 Capability move reference semantics: only the enumerated transitions
    (`send`, `recv`, `task_create`, `ns_bind`, `ns_unbind`, deletion paths) change
    a token's location, and each change is a single atomic replacement that records
    the old and new positions. Reuses `Formal.TokenLocation.Snapshot`/`Consistent`/
    `replace`; this module adds the transition table, the recorded move event, and
    the W3 single-position statement (a location is an `Option`, and the carrier
    view has at most one true entry per CapId).

    `InTransit` is not part of `Formal.Location` in v1 (§4.6/W3: cross-node `send`
    moves no capability), so the model cannot produce it by construction. Object
    lifetimes, message/namespace containers, subscription termination, and Rust
    refinement are out of scope. -/
namespace Formal.Move

open Formal
open Formal.TokenLocation

/-- A single recorded position change: old position, new position (`none` for
    deletion), and the `MsgId` for direct delivery (`send`→`recv`). -/
structure Transition where
  src : Location
  dst : Option Location
  msgId : Option Nat
deriving Repr, DecidableEq

def queuedSend (sender msg : Nat) : Transition :=
  { src := .heldBy sender, dst := some (.inMessage msg), msgId := none }

def directSend (sender receiver msg : Nat) : Transition :=
  { src := .heldBy sender, dst := some (.heldBy receiver), msgId := some msg }

def recvDeliver (msg receiver : Nat) : Transition :=
  { src := .inMessage msg, dst := some (.heldBy receiver), msgId := some msg }

def taskCreate (creator newTask : Nat) : Transition :=
  { src := .heldBy creator, dst := some (.heldBy newTask), msgId := none }

def nsBind (caller ns : Nat) (path : String) : Transition :=
  { src := .heldBy caller, dst := some (.inNamespace ns path), msgId := none }

def nsUnbind (caller ns : Nat) (path : String) : Transition :=
  { src := .inNamespace ns path, dst := some (.heldBy caller), msgId := none }

/-- Deletion (``cap_drop``, `task_destroy`, `ns_unmount`, container/message
    destruction): new position is `None`. -/
def deletion (src : Location) : Transition :=
  { src := src, dst := none, msgId := none }

/-- The atomic move: succeeds only if the token currently sits at the recorded
    source; a successful move returns one new snapshot plus the recorded event. -/
def applyMove (s : Snapshot) (c : Nat) (t : Transition) : Option (Snapshot × Transition) :=
  if s.state.locations c = some t.src then some (replace s c t.dst, t) else none

theorem applyMove_requires_source {s after c t t'}
    (h : applyMove s c t = some (after, t')) : s.state.locations c = some t.src := by
  unfold applyMove at h
  split at h
  · rename_i guard; exact guard
  · simp at h

theorem applyMove_dest {s after c t t'}
    (h : applyMove s c t = some (after, t')) : after.state.locations c = t.dst := by
  unfold applyMove at h
  split at h
  · cases h
    simp [replace]
  · simp at h

theorem applyMove_event {s after c t t'}
    (h : applyMove s c t = some (after, t')) : t' = t := by
  unfold applyMove at h
  split at h
  · cases h; rfl
  · simp at h

/-- Moves never change capability records (§4.6). -/
theorem applyMove_keeps_caps {s after c t t'}
    (h : applyMove s c t = some (after, t')) : after.state.caps = s.state.caps := by
  unfold applyMove at h
  split at h
  · cases h
    exact replace_keeps_all_records s c t.dst
  · simp at h

theorem applyMove_preserves_consistency {s after c t t'} (hc : Consistent s)
    (h : applyMove s c t = some (after, t')) : Consistent after := by
  have hloc := applyMove_requires_source h
  unfold applyMove at h
  split at h
  · cases h
    exact replace_preserves_consistency s c t.dst hc (hc.2 c t.src hloc)
  · rename_i hnot
    exact absurd hloc hnot

/-- After a move the carrier view has the token exactly at the new position (and
    nowhere else when the transition deletes it). -/
theorem carriers_after_move {s after c t t' loc} (hc : Consistent s)
    (h : applyMove s c t = some (after, t')) :
    after.carriers loc c = true ↔ t.dst = some loc := by
  have hcons := applyMove_preserves_consistency hc h
  have hdest := applyMove_dest h
  constructor
  · intro hcar
    have := (hcons.1 c loc).mp hcar
    rw [hdest] at this
    exact this
  · intro ht
    apply (hcons.1 c loc).mpr
    rw [hdest]
    exact ht

/-- W3 at the representation level: a functional `locations` gives every CapId at
    most one position. Combined with `carriers_after_move`, a successful move has
    exactly one destination carrier. -/
theorem w3_unique_location (s : Formal.State) {c : Nat} {a b : Location}
    (ha : s.locations c = some a) (hb : s.locations c = some b) : a = b := by
  rw [ha] at hb
  exact Option.some.inj hb

theorem queuedSend_dest (sender msg : Nat) :
    (queuedSend sender msg).dst = some (.inMessage msg) := rfl

theorem directSend_msgId (sender receiver msg : Nat) :
    (directSend sender receiver msg).msgId = some msg := rfl

theorem recvDeliver_msgId (msg receiver : Nat) :
    (recvDeliver msg receiver).msgId = some msg := rfl

theorem nsBind_dest (caller ns : Nat) (path : String) :
    (nsBind caller ns path).dst = some (.inNamespace ns path) := rfl

theorem deletion_sink (src : Location) : (deletion src).dst = none := rfl

/-- Cross-node `send` v1: capability positions never change; a non-empty
    `transferred_caps` fails with `NonPortableCap` (§4.6/§8.5, W3). -/
def crossNodeSend (s : Snapshot) (transferred : List Nat) : Snapshot × Bool :=
  if transferred.isEmpty then (s, true) else (s, false)

theorem crossNodeSend_no_move (s : Snapshot) (transferred : List Nat) :
    (crossNodeSend s transferred).1.state.locations = s.state.locations := by
  unfold crossNodeSend
  split <;> rfl

theorem crossNodeSend_empty_ok (s : Snapshot) : (crossNodeSend s []).2 = true := by
  simp [crossNodeSend]

theorem crossNodeSend_nonportable_fails (s : Snapshot) (c : Nat) (rest : List Nat) :
    (crossNodeSend s (c :: rest)).2 = false := by
  simp [crossNodeSend]

-- Executable anchors: cap 1 held by task 9, cap 2 in message 5.

def mvCap1 : Capability :=
  { owner := 9, obj := .segment 7, rights := 0, active := true, parent := none }

def mvCap2 : Capability :=
  { owner := 8, obj := .segment 8, rights := 0, active := true, parent := none }

def mvSnap : Snapshot :=
  { state :=
      { caps := fun id => if id = 1 then some mvCap1 else if id = 2 then some mvCap2 else none
        booted := true
        locations := fun id =>
          if id = 1 then some (.heldBy 9)
          else if id = 2 then some (.inMessage 5)
          else none }
    carriers := fun loc id =>
      match loc with
      | .heldBy n => n == 9 && id == 1
      | .inMessage m => m == 5 && id == 2
      | .inNamespace _ _ => false
    structural := fun _ => false }

#eval (applyMove mvSnap 1 (queuedSend 9 5)).isSome                          -- true
#eval (applyMove mvSnap 1 (queuedSend 8 5)).isSome                          -- false（源不匹配）
#eval ((applyMove mvSnap 1 (queuedSend 9 5)).map (fun p => p.1.state.locations 1))
  -- some (some (inMessage 5))：单步到达新位置
#eval ((applyMove mvSnap 1 (queuedSend 9 5)).map (fun p => p.1.carriers (.inMessage 5) 1))
  -- some true：carrier 视图同步
#eval ((applyMove mvSnap 1 (directSend 9 7 3)).map (fun p => p.2.msgId))
  -- some (some 3)：直接交付记录 MsgId
#eval ((applyMove mvSnap 2 (recvDeliver 5 7)).map (fun p => p.1.state.locations 2))
  -- some (some (heldBy 7))
#eval ((applyMove mvSnap 1 (deletion (.heldBy 9))).map (fun p => p.1.state.locations 1))
  -- some none：删除位置
#eval ((applyMove mvSnap 1 (deletion (.heldBy 9))).map (fun p => p.1.carriers (.heldBy 9) 1))
  -- some false
#eval ((applyMove mvSnap 1 (queuedSend 9 5)).map (fun p => (p.1.state.caps 1).map (fun c => c.owner)))
  -- some (some 9)：CapRecord 不变
#eval (crossNodeSend mvSnap [3]).2                                          -- false（NonPortableCap）
#eval ((crossNodeSend mvSnap [3]).1.state.locations 1)                      -- some (heldBy 9)：位置不变
#eval (crossNodeSend mvSnap []).2                                           -- true

end Formal.Move
