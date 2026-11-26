import Formal.Audit

/-! §13.2 `audit(cap)` — query model: the parent chain (root → cap, fuel `capId + 1`
    justified by W2's parent `<` child under `WellFormed`), each chain capability's
    `created_at`, the capability's position history, and the target object's
    creation/destruction events. The query is pure: it returns an `Except` and
    never a new state.

    Not covered: proving the chain is complete for arbitrary states (fuel `capId+1`
    is only known to be enough under `WellFormed`, and the truncation/completeness
    argument is not formalized), `origin` for cross-node roots, content digests,
    the full §13.1 payload contract, or Rust refinement. -/
namespace Formal.AuditQuery

open Formal
open Formal.Container
open Formal.Subscription
open Formal.Audit

abbrev AState := Formal.Audit.State

/-- Fuel-bounded parent walk: `[c, parent c, …]` including only existing records. -/
def parentChain : Nat → World → Nat → List Nat
  | 0, _, _ => []
  | n + 1, w, c =>
      match w.snap.state.caps c with
      | none => []
      | some cap =>
          c :: match cap.parent with
               | some p => parentChain n w p
               | none => []

theorem parentChain_head {n : Nat} {w : World} {c : Nat} {cap : Capability}
    (hcap : w.snap.state.caps c = some cap) :
    parentChain (n + 1) w c
      = c :: (match cap.parent with
              | some p => parentChain n w p
              | none => []) := by
  simp [parentChain.eq_2, hcap]

theorem parentChain_length_le : ∀ (n : Nat) (w : World) (c : Nat),
    (parentChain n w c).length ≤ n := by
  intro n
  induction n with
  | zero => intro w c; simp [parentChain.eq_1]
  | succ k ih =>
      intro w c
      rw [parentChain.eq_2]
      split
      · simp
      · rename_i cap hcap
        split
        · rename_i p hp
          simp only [List.length_cons]
          exact Nat.add_le_add_right (ih w p) 1
        · simp

/-- The walk only contains CapIds that have records (W3's record existence). -/
theorem parentChain_sound : ∀ (n : Nat) (w : World) (c x : Nat),
    x ∈ parentChain n w c → ∃ cap, w.snap.state.caps x = some cap := by
  intro n
  induction n with
  | zero => intro w c x h; simp [parentChain.eq_1] at h
  | succ k ih =>
      intro w c x h
      rw [parentChain.eq_2] at h
      split at h
      · simp at h
      · rename_i cap hcap
        simp only [List.mem_cons] at h
        rcases h with hx | hmem
        · subst hx
          exact ⟨cap, hcap⟩
        · split at hmem
          · rename_i p hp
            exact ih w p x hmem
          · simp at hmem

/-- Root → cap order used by the query result. -/
def auditChain (s : AState) (capId : Nat) : List Nat :=
  (parentChain (capId + 1) s.world.base capId).reverse

theorem auditChain_sound {s : AState} {capId x : Nat}
    (h : x ∈ auditChain s capId) : ∃ cap, s.world.base.snap.state.caps x = some cap :=
  parentChain_sound _ _ _ _ (List.mem_reverse.mp h)

theorem auditChain_length_le (s : AState) (capId : Nat) :
    (auditChain s capId).length ≤ capId + 1 := by
  rw [auditChain, List.length_reverse]
  exact parentChain_length_le _ _ _

/-- The queried capability is the last chain element (root → cap). -/
theorem auditChain_last {s : AState} {capId : Nat} {cap : Capability}
    (hcap : s.world.base.snap.state.caps capId = some cap) :
    (auditChain s capId).getLast? = some capId := by
  rw [auditChain, List.getLast?_reverse, parentChain_head hcap]
  simp

/-- Object creation/destruction events for one object, with their `EventId`. -/
def objectId : Object → Nat
  | .segment i => i
  | .task i => i
  | .excSource i => i
  | .namespace i => i
  | .timer i => i
  | .node i => i
  | .waitObject i => i

def objectEvents (l : Log) (obj : Nat) : List Event :=
  (List.range l.entries.length).filterMap fun i =>
    match l.entries[i]? with
    | some e =>
        match e.kind with
        | .objectCreated o =>
            if o = obj then some { id := l.first + i, when := e.when, actor := e.actor, kind := e.kind }
            else none
        | .objectDestroyed o =>
            if o = obj then some { id := l.first + i, when := e.when, actor := e.actor, kind := e.kind }
            else none
        | _ => none
    | none => none

inductive AuditError where
  | invalidCap
  | notHeld
deriving DecidableEq, Repr

structure AuditTrace where
  chain : List Nat
  creations : List (Nat × Option Nat)
  positions : List (Option Location × Option Location)
  objectHistory : List Event
deriving Repr

/-- §13.2 query. Errors mirror the spec: an unknown CapId is `InvalidCap`; a known
    capability not in `HeldBy(caller)` is `NotHeld`. The target object may be dead. -/
def audit (s : AState) (caller capId : Nat) : Except AuditError AuditTrace :=
  match s.world.base.snap.state.caps capId with
  | none => .error .invalidCap
  | some cap =>
      match s.world.base.snap.state.locations capId with
      | some (.heldBy h) =>
          if h = caller then
            .ok { chain := auditChain s capId
                  creations := (auditChain s capId).map (fun c => (c, s.created c))
                  positions := capHistory s.log capId
                  objectHistory := objectEvents s.log (objectId cap.obj) }
          else .error .notHeld
      | _ => .error .notHeld

theorem audit_invalid {s : AState} {caller capId : Nat}
    (hcap : s.world.base.snap.state.caps capId = none) :
    audit s caller capId = .error .invalidCap := by
  simp [audit, hcap]

theorem audit_not_held {s : AState} {caller capId h : Nat} {cap : Capability}
    (hcap : s.world.base.snap.state.caps capId = some cap)
    (hloc : s.world.base.snap.state.locations capId = some (.heldBy h)) (hne : h ≠ caller) :
    audit s caller capId = .error .notHeld := by
  simp [audit, hcap, hloc, hne]

theorem audit_not_held_other {s : AState} {caller capId : Nat} {cap : Capability} {loc : Location}
    (hcap : s.world.base.snap.state.caps capId = some cap)
    (hloc : s.world.base.snap.state.locations capId = some loc)
    (hnot : ∀ h, loc = .heldBy h → h ≠ caller) :
    audit s caller capId = .error .notHeld := by
  cases loc with
  | heldBy h =>
      rw [audit, hcap, hloc]
      simp [hnot h rfl]
  | inMessage m =>
      rw [audit, hcap, hloc]
  | inNamespace n p =>
      rw [audit, hcap, hloc]

/-- Successful query implies the capability is valid and held by the caller. -/
theorem audit_ok_held {s : AState} {caller capId : Nat} {tr : AuditTrace}
    (h : audit s caller capId = .ok tr) :
    ∃ cap, s.world.base.snap.state.caps capId = some cap ∧
      s.world.base.snap.state.locations capId = some (.heldBy caller) := by
  unfold audit at h
  split at h <;> try cases h
  rename_i cap hcap
  split at h <;> try cases h
  rename_i hloc
  split at h <;> try cases h
  rename_i heq
  exact ⟨cap, hcap, by rw [hloc, heq]⟩

/-- Successful query returns the modelled fields: chain, creation ids, position
    history, and the target object's creation/destruction events. -/
theorem audit_ok_fields {s : AState} {caller capId : Nat} {tr : AuditTrace} {cap : Capability}
    (hcap : s.world.base.snap.state.caps capId = some cap)
    (h : audit s caller capId = .ok tr) :
    tr.chain = auditChain s capId ∧
      tr.creations = (auditChain s capId).map (fun c => (c, s.created c)) ∧
      tr.positions = capHistory s.log capId ∧
      tr.objectHistory = objectEvents s.log (objectId cap.obj) := by
  unfold audit at h
  split at h <;> try cases h
  rename_i cap' hcap'
  have hcapEq : cap' = cap := by
    rw [hcap] at hcap'
    exact (Option.some.inj hcap').symm
  subst hcapEq
  split at h <;> try cases h
  split at h <;> try cases h
  exact ⟨rfl, rfl, rfl, rfl⟩

/-- The target object being destroyed does not fail the query (§13.2). -/
theorem audit_dead_target_ok {s : AState} {caller capId : Nat} {cap : Capability}
    (hcap : s.world.base.snap.state.caps capId = some cap)
    (hloc : s.world.base.snap.state.locations capId = some (.heldBy caller)) :
    ∃ tr, audit s caller capId = .ok tr ∧ tr.positions = capHistory s.log capId := by
  refine ⟨{ chain := auditChain s capId
            creations := (auditChain s capId).map (fun c => (c, s.created c))
            positions := capHistory s.log capId
            objectHistory := objectEvents s.log (objectId cap.obj) }, ?_, rfl⟩
  simp [audit, hcap, hloc]

theorem audit_ok_creation_mem {s : AState} {caller capId : Nat} {tr : AuditTrace}
    {cap : Capability} (hcap : s.world.base.snap.state.caps capId = some cap)
    (h : audit s caller capId = .ok tr) {c : Nat} {e : Option Nat}
    (hmem : (c, e) ∈ tr.creations) : c ∈ tr.chain ∧ s.created c = e := by
  obtain ⟨hc, hcr, _, _⟩ := audit_ok_fields hcap h
  rw [hcr, List.mem_map] at hmem
  obtain ⟨x, hx, hpair⟩ := hmem
  have hxe : (x, s.created x) = (c, e) := hpair
  injection hxe with h1 h2
  subst h1
  subst h2
  exact ⟨by rwa [hc], rfl⟩

end Formal.AuditQuery
