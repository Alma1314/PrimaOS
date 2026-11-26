import Formal.Subscription

/-! §13.1 / W13 audit log: envelope `{id, at, actor, kind}`, append-only sequence,
    strictly increasing `EventId`, per-Capability position history, creation records,
    and object creation events. The log is represented as a first id plus entries;
    the id of entry `i` is `first + i`, so strict monotonicity (W13) holds by
    construction rather than by an invariant over ids. `next_event` is therefore
    `first + length`.

    The audited operations (move, drop, mint, task-state, object create/destroy)
    append exactly one event and prove the log prefix, the new history entry, and
    the creation/object bookkeeping. Not covered: event payload completeness for
    every §13.1 event class, the `audit` query's parent-chain traversal, content
    digests, cross-node `transport_msg`, clocks (all events use the caller's `now`),
    and Rust refinement. -/
namespace Formal.Audit

open Formal
open Formal.TokenLocation
open Formal.Container
open Formal.Subscription

inductive Actor where
  | task (id : Nat)
  | kernel
deriving DecidableEq, Repr

inductive TaskStatus where
  | ready
  | running
  | blockedRecv
  | blockedWait
  | faulted
deriving DecidableEq, Repr

/-- Minimal §13.1 event kinds sufficient for the modelled slices. -/
inductive AuditKind where
  | bootstrapped
  | objectCreated (obj : Nat)
  | objectDestroyed (obj : Nat)
  | capMinted (cap : Nat)
  | capDerived (cap parent : Nat)
  | capMoved (cap : Nat) (src dst : Option Location)
  | capDropped (cap : Nat) (src : Location)
  | taskCreated (task : Nat)
  | taskStateChanged (task : Nat) (before after : TaskStatus)
  | excSubscribed (source task : Nat)
  | excUnsubscribed (source task : Nat)
deriving DecidableEq, Repr

structure Entry where
  actor : Actor
  when : Nat
  kind : AuditKind
deriving Repr

structure Event where
  id : Nat
  when : Nat
  actor : Actor
  kind : AuditKind
deriving Repr

/-- `first + i` is the `EventId` of entry `i`; `next_event` allocates the next. -/
structure Log where
  first : Nat
  entries : List Entry

def Log.nextEvent (l : Log) : Nat := l.first + l.entries.length

def Log.eventAt? (l : Log) (i : Nat) : Option Event :=
  (l.entries[i]?).map fun e => { id := l.first + i, when := e.when, actor := e.actor, kind := e.kind }

/-- Append allocations: entries are added in order; ids are derived from position. -/
def Log.append (l : Log) (actor : Actor) (t : Nat) (kinds : List AuditKind) : Log :=
  { l with entries := l.entries ++ kinds.map (fun k => { actor := actor, when := t, kind := k }) }

theorem append_entries (l : Log) (a : Actor) (t : Nat) (ks : List AuditKind) :
    (l.append a t ks).entries = l.entries ++ ks.map (fun k => { actor := a, when := t, kind := k }) :=
  rfl

theorem append_prefix (l : Log) (a : Actor) (t : Nat) (ks : List AuditKind) :
    (l.append a t ks).entries.take l.entries.length = l.entries := by
  simp [Log.append]

theorem append_nextEvent (l : Log) (a : Actor) (t : Nat) (ks : List AuditKind) :
    (l.append a t ks).nextEvent = l.nextEvent + ks.length := by
  simp [Log.nextEvent, Log.append, List.length_append, Nat.add_assoc]

/-- Old events keep their ids and envelope after an append. -/
theorem append_event_old {l : Log} {a : Actor} {t : Nat} {ks : List AuditKind}
    {i : Nat} (h : i < l.entries.length) :
    (l.append a t ks).eventAt? i = l.eventAt? i := by
  simp [Log.eventAt?, Log.append, List.getElem?_append_left h]

/-- W13 clause 4 at representation level: ids are strictly increasing in log order. -/
theorem ids_strict {l : Log} {i j : Nat} (h : i < j) {e1 e2 : Event}
    (h1 : l.eventAt? i = some e1) (h2 : l.eventAt? j = some e2) : e1.id < e2.id := by
  simp only [Log.eventAt?, Option.map_eq_some_iff] at h1 h2
  obtain ⟨a, _, rfl⟩ := h1
  obtain ⟨b, _, rfl⟩ := h2
  exact Nat.add_lt_add_left h l.first

/-- Position history of one Capability: every `CapMoved`/`CapDropped` as
    `(old, new)` pairs (deletion has `new = none`). -/
def capHistory (l : Log) (cap : Nat) : List (Option Location × Option Location) :=
  l.entries.filterMap fun e =>
    match e.kind with
    | .capMoved c src dst => if c = cap then some (src, dst) else none
    | .capDropped c src => if c = cap then some (some src, none) else none
    | _ => none

theorem capHistory_append_moved (l : Log) (a : Actor) (t : Nat) (cap : Nat)
    (src dst : Option Location) :
    capHistory (l.append a t [.capMoved cap src dst]) cap = capHistory l cap ++ [(src, dst)] := by
  simp [capHistory, Log.append, List.filterMap_append]

theorem capHistory_append_dropped (l : Log) (a : Actor) (t : Nat) (cap : Nat)
    (src : Location) :
    capHistory (l.append a t [.capDropped cap src]) cap
      = capHistory l cap ++ [(some src, none)] := by
  simp [capHistory, Log.append, List.filterMap_append]

theorem capHistory_append_moved_other {l : Log} {a : Actor} {t : Nat} {cap c : Nat}
    {src dst : Option Location} (h : c ≠ cap) :
    capHistory (l.append a t [.capMoved c src dst]) cap = capHistory l cap := by
  simp [capHistory, Log.append, List.filterMap_append, h]

/-- Audited state: a subscription world plus the append-only log, creation records
    (`capId → mint/derive event id`), object creation bits, and a clock value. -/
structure State where
  world : SubWorld
  log : Log
  created : Nat → Option Nat
  objects : Nat → Bool
  now : Nat

def State.createBit (s : State) (obj : Nat) (v : Bool) : Nat → Bool :=
  fun x => if x = obj then v else s.objects x

def State.withCreated (s : State) (cap : Nat) (eventId : Nat) : Nat → Option Nat :=
  fun x => if x = cap then some eventId else s.created x

/-- Move with a `CapMoved` event; `old` is read from the current position. -/
def auditedMove (s : State) (actor : Actor) (capId : Nat) (dst : Option Location) :
    Option State :=
  match s.world.base.snap.state.locations capId with
  | some old =>
      some { s with
        world := { s.world with
          base := { s.world.base with snap := replace s.world.base.snap capId dst } }
        log := s.log.append actor s.now [.capMoved capId (some old) dst] }
  | none => none

theorem auditedMove_spec {s s' : State} {actor : Actor} {capId : Nat} {dst : Option Location}
    (h : auditedMove s actor capId dst = some s') :
    ∃ old, s.world.base.snap.state.locations capId = some old ∧
      s'.log.entries = s.log.entries ++ [{ actor := actor, when := s.now,
                                            kind := .capMoved capId (some old) dst }] ∧
      capHistory s'.log capId = capHistory s.log capId ++ [(some old, dst)] ∧
      s'.created = s.created ∧ s'.objects = s.objects := by
  unfold auditedMove at h
  split at h
  · rename_i old hloc
    cases h
    refine ⟨old, hloc, ?_, ?_, rfl, rfl⟩
    · change (s.log.append actor s.now [.capMoved capId (some old) dst]).entries
        = s.log.entries ++ [{ actor := actor, when := s.now,
                              kind := .capMoved capId (some old) dst }]
      simp [Log.append]
    · change capHistory (s.log.append actor s.now [.capMoved capId (some old) dst]) capId
        = capHistory s.log capId ++ [(some old, dst)]
      rw [capHistory_append_moved]
  · simp at h

/-- Drop with a `CapDropped` event. -/
def auditedDrop (s : State) (actor : Actor) (capId holder : Nat) : Option State :=
  match s.world.base.snap.state.locations capId with
  | some (.heldBy h) =>
      if h = holder then
        some { s with
          world := { s.world with
            base := { s.world.base with snap := replace s.world.base.snap capId none } }
          log := s.log.append actor s.now [.capDropped capId (.heldBy holder)] }
      else none
  | _ => none

theorem auditedDrop_spec {s s' : State} {actor : Actor} {capId holder : Nat}
    (h : auditedDrop s actor capId holder = some s') :
    s.world.base.snap.state.locations capId = some (.heldBy holder) ∧
      capHistory s'.log capId
        = capHistory s.log capId ++ [(some (.heldBy holder), none)] ∧
      s'.created = s.created := by
  unfold auditedDrop at h
  split at h
  · rename_i hloc
    split at h
    · rename_i heq
      cases h
      refine ⟨?_, ?_, rfl⟩
      · rw [heq] at hloc
        exact hloc
      · change capHistory (s.log.append actor s.now [.capDropped capId (.heldBy holder)]) capId
          = capHistory s.log capId ++ [(some (.heldBy holder), none)]
        rw [capHistory_append_dropped]
    · simp at h
  · simp at h

/-- Mint: appends `CapMinted` and records `created_at` as that event id. -/
def auditedMint (s : State) (actor : Actor) (capId : Nat) : State :=
  { s with
    log := s.log.append actor s.now [.capMinted capId]
    created := s.withCreated capId s.log.nextEvent }

theorem auditedMint_created (s : State) (actor : Actor) (capId : Nat) :
    (auditedMint s actor capId).created capId = some s.log.nextEvent := by
  simp [auditedMint, State.withCreated]

theorem auditedMint_other_created {s : State} {actor : Actor} {capId x : Nat} (h : x ≠ capId) :
    (auditedMint s actor capId).created x = s.created x := by
  simp [auditedMint, State.withCreated, h]

theorem auditedMint_event (s : State) (actor : Actor) (capId : Nat) :
    (auditedMint s actor capId).log.entries
      = s.log.entries ++ [{ actor := actor, when := s.now, kind := .capMinted capId }] := by
  simp [auditedMint, Log.append]

/-- Task state transition always appends `TaskStateChanged` (§13.1). -/
def auditedTaskState (s : State) (actor : Actor) (task : Nat)
    (before after : TaskStatus) : State :=
  { s with log := s.log.append actor s.now [.taskStateChanged task before after] }

theorem auditedTaskState_event (s : State) (actor : Actor) (task : Nat)
    (before after : TaskStatus) :
    (auditedTaskState s actor task before after).log.entries
      = s.log.entries ++ [{ actor := actor, when := s.now,
                            kind := .taskStateChanged task before after }] := by
  simp [auditedTaskState, Log.append]

theorem auditedTaskState_keeps_world (s : State) (actor : Actor) (task : Nat)
    (before after : TaskStatus) :
    (auditedTaskState s actor task before after).world = s.world := rfl

/-- Object creation appends `ObjectCreated` and marks the object active. -/
def auditedObjectCreate (s : State) (actor : Actor) (obj : Nat) : State :=
  { s with
    log := s.log.append actor s.now [.objectCreated obj]
    objects := s.createBit obj true }

theorem auditedObjectCreate_event (s : State) (actor : Actor) (obj : Nat) :
    ∃ e, e ∈ (auditedObjectCreate s actor obj).log.entries ∧ e.kind = .objectCreated obj := by
  refine ⟨{ actor := actor, when := s.now, kind := .objectCreated obj }, ?_, rfl⟩
  simp [auditedObjectCreate, Log.append]

theorem auditedObjectCreate_live (s : State) (actor : Actor) (obj : Nat) :
    (auditedObjectCreate s actor obj).objects obj = true := by
  simp [auditedObjectCreate, State.createBit]

/-- Object destruction appends `ObjectDestroyed`; the creation event stays in the log. -/
def auditedObjectDestroy (s : State) (actor : Actor) (obj : Nat) : State :=
  { s with
    log := s.log.append actor s.now [.objectDestroyed obj]
    objects := s.createBit obj false }

theorem auditedObjectDestroy_event (s : State) (actor : Actor) (obj : Nat) :
    ∃ e, e ∈ (auditedObjectDestroy s actor obj).log.entries ∧ e.kind = .objectDestroyed obj := by
  refine ⟨{ actor := actor, when := s.now, kind := .objectDestroyed obj }, ?_, rfl⟩
  simp [auditedObjectDestroy, Log.append]

theorem auditedObjectDestroy_dead (s : State) (actor : Actor) (obj : Nat) :
    (auditedObjectDestroy s actor obj).objects obj = false := by
  simp [auditedObjectDestroy, State.createBit]

end Formal.Audit
