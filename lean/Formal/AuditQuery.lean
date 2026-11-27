import Formal.Audit

/-! §13.2 `audit(cap)` — query model: the parent chain (root → cap, fuel `capId + 1`
    justified by W2's parent `<` child under `WellFormed`), each chain capability's
    `created_at`, the capability's position history, and the target object's
    creation/destruction events. The query is pure: it returns an `Except` and
    never a new state.

    Parent-chain completeness is proved under `WellFormed`. Not covered:
    completeness for arbitrary states, `origin` for cross-node roots, content digests,
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

/-- The walk stopped at a parentless capability (the chain is not truncated). -/
def lastParentNone (w : World) : List Nat → Prop
  | [] => False
  | [x] => ∃ cap, w.snap.state.caps x = some cap ∧ cap.parent = none
  | _ :: y :: rest => lastParentNone w (y :: rest)

theorem lastParentNone_getLast {w : World} {chain : List Nat}
    (h : lastParentNone w chain) :
    ∀ x, chain.getLast? = some x →
      ∃ cap, w.snap.state.caps x = some cap ∧ cap.parent = none := by
  induction chain with
  | nil => simp [lastParentNone] at h
  | cons a rest ih =>
      cases rest with
      | nil =>
          intro x hx
          simp at hx
          subst hx
          simpa [lastParentNone] using h
      | cons b rest' =>
          intro x hx
          exact ih h x (by simpa using hx)

theorem lastParentNone_cons_of_tail {w : World} {x : Nat} {tail : List Nat}
    (h : lastParentNone w tail) (hne : tail ≠ []) : lastParentNone w (x :: tail) := by
  cases tail with
  | nil => exact absurd rfl hne
  | cons y rest =>
      cases rest with
      | nil => simpa [lastParentNone] using h
      | cons z rest' => simpa [lastParentNone] using h

theorem parentChain_nonempty {n : Nat} {w : World} {c : Nat} {cap : Capability}
    (hn : 0 < n) (hcap : w.snap.state.caps c = some cap) : parentChain n w c ≠ [] := by
  cases n with
  | zero => exact absurd hn (Nat.lt_irrefl 0)
  | succ k =>
      intro h
      rw [parentChain_head hcap] at h
      simp at h

/-- A completed chain is stable under extra fuel. -/
theorem parentChain_succ_of_done : ∀ (n : Nat) (w : World) (c : Nat),
    lastParentNone w (parentChain n w c) →
      parentChain (n + 1) w c = parentChain n w c := by
  intro n
  induction n with
  | zero => intro w c h; exact False.elim h
  | succ k ih =>
      intro w c h
      cases hcap : w.snap.state.caps c with
      | none => simp [parentChain.eq_2, hcap]
      | some cap =>
          cases hp : cap.parent with
          | none => simp [parentChain.eq_2, hcap, hp]
          | some p =>
              have hleft : parentChain (k + 1) w c = c :: parentChain k w p := by
                simp [parentChain.eq_2, hcap, hp]
              rw [hleft] at h
              cases htail : parentChain k w p with
              | nil =>
                  rw [htail] at h
                  simp [lastParentNone, hcap, hp] at h
              | cons y rest =>
                  rw [htail] at h
                  have hdone : lastParentNone w (parentChain k w p) := by
                    rw [htail]
                    change lastParentNone w (y :: rest) at h
                    exact h
                  have hEq := ih w p hdone
                  rw [show parentChain (k + 2) w c = c :: parentChain (k + 1) w p by
                        simp [parentChain.eq_2, hcap, hp],
                      show parentChain (k + 1) w c = c :: parentChain k w p by
                        simp [parentChain.eq_2, hcap, hp],
                      hEq]

theorem parentChain_done_add : ∀ (k : Nat) {m : Nat} {w : World} {c : Nat},
    lastParentNone w (parentChain m w c) →
      parentChain (m + k) w c = parentChain m w c := by
  intro k
  induction k with
  | zero => intro m w c h; simp
  | succ k ih =>
      intro m w c h
      have hEq := ih (m := m) (w := w) (c := c) h
      have hdone : lastParentNone w (parentChain (m + k) w c) := by
        rw [hEq]
        exact h
      have hsucc := parentChain_succ_of_done (m + k) w c hdone
      rw [Nat.add_succ, hsucc, hEq]

theorem parentChain_complete {w : World} (hwf : WellFormed w.snap.state) :
    ∀ (c : Nat) (cap : Capability), w.snap.state.caps c = some cap →
      lastParentNone w (parentChain (c + 1) w c) := by
  intro c
  induction c using Nat.strongRecOn with
  | ind c ih =>
      intro cap hcap
      cases hp : cap.parent with
      | none => simp [parentChain.eq_2, hcap, hp, lastParentNone]
      | some p =>
          have hlt : p < c := (hwf c cap hcap p hp).1
          obtain ⟨pcap, hpcap, _⟩ := (hwf c cap hcap p hp).2
          have hdone := ih p hlt pcap hpcap
          have hle : p + 1 ≤ c := hlt
          obtain ⟨k, hk⟩ := Nat.le.dest hle
          rw [← hk] at hcap ⊢
          have hstable : parentChain (p + 1 + k) w p = parentChain (p + 1) w p :=
            parentChain_done_add k hdone
          have hp0 : 0 < p + 1 + k := by
            rw [Nat.add_right_comm]
            exact Nat.succ_pos (p + k)
          have hne : parentChain (p + 1 + k) w p ≠ [] :=
            parentChain_nonempty hp0 hpcap
          have hne' : parentChain (p + 1) w p ≠ [] := by
            rw [← hstable]
            exact hne
          rw [parentChain_head hcap, hp]
          change lastParentNone w ((p + 1 + k) :: parentChain (p + 1 + k) w p)
          rw [hstable]
          exact lastParentNone_cons_of_tail hdone hne'

/-- The root of the queried chain (chain head, the end of the parent walk) has no
    parent, under `WellFormed`. -/
theorem auditChain_root_parent_none {s : AState} {capId : Nat} {cap : Capability}
    (hwf : WellFormed s.world.base.snap.state)
    (hcap : s.world.base.snap.state.caps capId = some cap) :
    ∀ root, (auditChain s capId).head? = some root →
      ∃ rcap, s.world.base.snap.state.caps root = some rcap ∧ rcap.parent = none := by
  have hdone := parentChain_complete hwf capId cap hcap
  have hlast := lastParentNone_getLast hdone
  intro root hroot
  rw [auditChain, List.head?_reverse] at hroot
  exact hlast root hroot

/-- Object creation/destruction events retain the tagged Object identity. -/
def objectEvents (l : Log) (obj : Object) : List Event :=
  (List.range l.entries.length).filterMap fun i =>
    (l.eventAt? i).filter fun e =>
      match e.kind with
      | .objectCreated o | .objectDestroyed o => o == obj
      | _ => false

/-- Every returned event belongs to this exact object, including its type tag. -/
theorem objectEvents_sound {l : Log} {obj : Object} {e : Event}
    (h : e ∈ objectEvents l obj) :
    e.kind = .objectCreated obj ∨ e.kind = .objectDestroyed obj := by
  simp only [objectEvents, List.mem_filterMap] at h
  obtain ⟨i, _, hi⟩ := h
  have hp := (Option.filter_eq_some_iff.mp hi).2
  cases hk : e.kind <;> simp_all

/-- No matching lifecycle event is dropped, and its original envelope is retained. -/
theorem objectEvents_mem_iff {l : Log} {obj : Object} {e : Event} :
    e ∈ objectEvents l obj ↔ ∃ i, i < l.entries.length ∧ l.eventAt? i = some e ∧
      (e.kind = .objectCreated obj ∨ e.kind = .objectDestroyed obj) := by
  constructor
  · intro h
    have hkind := objectEvents_sound h
    simp only [objectEvents, List.mem_filterMap] at h
    obtain ⟨i, hi, he⟩ := h
    exact ⟨i, List.mem_range.mp hi, (Option.filter_eq_some_iff.mp he).1, hkind⟩
  · rintro ⟨i, hi, he, hk⟩
    simp only [objectEvents, List.mem_filterMap]
    refine ⟨i, List.mem_range.mpr hi, Option.filter_eq_some_iff.mpr ⟨he, ?_⟩⟩
    rcases hk with hk | hk <;> simp [hk]

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
                  objectHistory := objectEvents s.log cap.obj }
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
      tr.objectHistory = objectEvents s.log cap.obj := by
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
            objectHistory := objectEvents s.log cap.obj }, ?_, rfl⟩
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

/-- Successful audit cannot include another object's lifecycle events. -/
theorem audit_ok_object_mem {s : AState} {caller capId : Nat} {tr : AuditTrace}
    {cap : Capability} {e : Event}
    (hcap : s.world.base.snap.state.caps capId = some cap)
    (h : audit s caller capId = .ok tr) (he : e ∈ tr.objectHistory) :
    e.kind = .objectCreated cap.obj ∨ e.kind = .objectDestroyed cap.obj := by
  rw [(audit_ok_fields hcap h).2.2.2] at he
  exact objectEvents_sound he

end Formal.AuditQuery
