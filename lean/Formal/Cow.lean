import Formal.Memseg

/-! Minimal reference model of the cosir `cow_refine` composite slice: COW
    establishment, write-fault delivery, private-copy remediation, scheduling and
    destruction over an abstract dual-view memory, task table and COW-pair list.

    The Rust composite keeps `RefineState` + `State` (capabilities, pending
    exceptions, subscriptions) + `TlbState` + `current` + COW pairs, and checks a
    large conjunction in `check_invariants`. Here capability authorization,
    exception sources, resume frames and the TLB are abstracted away; the model
    keeps task status/current, the COW-pair list, the memory mapping table and the
    frame-ownership predicates reused from `Formal.Memseg`.

    Not modelled: capability rights/holders, exception-source subscriptions,
    `exc_resume`/SRET frame validation, TLB invalidation after remap, the frame
    disjointness case of `check_invariants` beyond the mapped frame, and the
    `StillMapped` scan (a caller guard, as in `Formal.Memseg`). See README.md. -/
namespace Formal.Cow

open Formal.Memseg (Access Segment Mapping nth? setAt)

abbrev Frame := Nat
abbrev SegId := Nat
abbrev TaskId := Nat
abbrev VAddr := Nat

inductive TaskStatus where
  | ready
  | running
  | faulted
deriving DecidableEq, Repr

structure Task where
  status : TaskStatus
  vspace : VAddr → Option Mapping

structure PendingExc where
  va : VAddr
  pagePresent : Bool
  writable : Bool
deriving DecidableEq, Repr

structure State where
  segments : SegId → Option Segment
  tasks : TaskId → Option Task
  current : Option TaskId
  pending : TaskId → Option PendingExc
  cows : List (TaskId × TaskId × VAddr)

inductive CowError where
  | invalidTask
  | invalidAddress
  | mappingConflict
  | noSuchSegment
  | stillMapped
  | cpuBusy
deriving DecidableEq, Repr

inductive AccessKind where
  | read
  | write
deriving DecidableEq, Repr

def AccessKind.isWrite : AccessKind → Bool
  | .read => false
  | .write => true

def accessAllowed (m : Mapping) : AccessKind → Bool
  | .read => m.access.read
  | .write => m.access.write

/-- A read-only alias of a mapping (used for both sides of a COW pair). -/
def readOnly (m : Mapping) : Mapping :=
  { m with access := { read := true, write := false, exec := false } }

/-- A private RW mapping into a private segment at offset zero. -/
def privateRW (sid : SegId) : Mapping :=
  { segment := sid, offset := 0, access := { read := true, write := true, exec := false } }

/-- Successful COW establishment: both sides point at the same segment offset and
    lose write access; the pair is appended to the COW list. -/
def establishCow (st : State) (parent child : TaskId) (va : VAddr) : State × Option CowError :=
  match st.tasks parent, st.tasks child with
  | some tp, some tc =>
    if parent = child then (st, some .invalidTask)
    else
      match tp.vspace va, tc.vspace va with
      | some mp, none =>
        if mp.access.read && mp.access.write then
          let ro := readOnly mp
          ({ st with
            tasks := setAt (setAt st.tasks parent
              (some { tp with vspace := setAt tp.vspace va (some ro) }))
              child (some { tc with vspace := setAt tc.vspace va (some ro) }),
            cows := st.cows ++ [(parent, child, va)] }, none)
        else (st, some .mappingConflict)
      | _, _ => (st, some .mappingConflict)
  | _, _ => (st, some .invalidTask)

inductive Outcome where
  | allowed
  | faulted (pagePresent : Bool) (writable : Bool)
  | rejected (err : CowError)

structure AccessResult where
  outcome : Outcome
  state : State

/-- Access through the current running task. An absent mapping faults with
    `pagePresent = false`; a present mapping without the requested permission
    faults with `pagePresent = true`. A fault sets the task Faulted, clears
    `current`, and records the pending exception. Allowed/success paths leave the
    state unchanged. -/
def faultedTask (t : Task) : Task := { t with status := .faulted }

def pendingRec (va : VAddr) (present : Bool) (kind : AccessKind) : PendingExc :=
  { va := va, pagePresent := present, writable := kind.isWrite }

def withVspace (t : Task) (vspace : VAddr → Option Mapping) : Task :=
  { t with vspace := vspace }

def readyPrivate (t : Task) (vspace : VAddr → Option Mapping) : Task :=
  withVspace ({ t with status := .ready }) vspace

def faultedState (st : State) (tid : TaskId) (t : Task) (va : VAddr) (present : Bool)
    (kind : AccessKind) : State :=
  { st with
    tasks := setAt st.tasks tid (some (faultedTask t)),
    current := none,
    pending := setAt st.pending tid (some (pendingRec va present kind)) }

def privatizedState (st : State) (tid : TaskId) (t : Task) (privSeg : SegId) (va : VAddr) : State :=
  { st with
    tasks := setAt st.tasks tid (some (readyPrivate t (setAt t.vspace va (some (privateRW privSeg))))),
    pending := setAt st.pending tid none }

def scheduledState (st : State) (tid : TaskId) (t : Task) : State :=
  { st with
    tasks := setAt st.tasks tid (some ({ t with status := .running })),
    current := some tid }

def yieldedState (st : State) (tid : TaskId) (t : Task) : State :=
  { st with
    tasks := setAt st.tasks tid (some ({ t with status := .ready })),
    current := none }

/-- Drop every COW pair that mentions `tid`. -/
def filterCows : List (TaskId × TaskId × VAddr) → TaskId → List (TaskId × TaskId × VAddr)
  | [], _ => []
  | (p, c, va) :: rest, tid =>
    if p = tid ∨ c = tid then filterCows rest tid else (p, c, va) :: filterCows rest tid

def destroyedState (st : State) (tid : TaskId) : State :=
  { st with
    tasks := setAt st.tasks tid none,
    pending := setAt st.pending tid none,
    cows := filterCows st.cows tid }

def establishedState (st : State) (parent child : TaskId) (va : VAddr) (tp tc : Task) (mp : Mapping) : State :=
  { st with
    tasks := setAt (setAt st.tasks parent (some ({ tp with vspace := setAt tp.vspace va (some (readOnly mp)) })))
      child (some ({ tc with vspace := setAt tc.vspace va (some (readOnly mp)) })),
    cows := st.cows ++ [(parent, child, va)] }

/-- Access through the current running task. An absent mapping faults with
    `pagePresent = false`; a present mapping without the requested permission
    faults with `pagePresent = true`. A fault sets the task Faulted, clears
    `current`, and records the pending exception. Allowed/success paths leave the
    state unchanged. -/
def access (st : State) (tid : TaskId) (va : VAddr) (kind : AccessKind) : AccessResult :=
  match st.tasks tid with
  | none => { outcome := .rejected .invalidTask, state := st }
  | some t =>
    if st.current = some tid ∧ t.status = .running then
      match t.vspace va with
      | none => { outcome := .faulted false kind.isWrite, state := faultedState st tid t va false kind }
      | some m =>
        if accessAllowed m kind then { outcome := .allowed, state := st }
        else { outcome := .faulted true kind.isWrite, state := faultedState st tid t va true kind }
    else { outcome := .rejected .invalidTask, state := st }

/-- Private-copy remediation after a writable COW fault: require a Faulted,
    non-current task with a pending writable fault at `va`, replace the read-only
    mapping by a RW mapping into the private segment, and return the task to Ready.
    The COW list is unchanged (other pairs keep their aliasing). -/
def privatize (st : State) (tid : TaskId) (privSeg : SegId) (va : VAddr) :
    State × Option CowError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some t =>
    if t.status = .faulted then
      if st.current = none then
        match st.pending tid with
        | some pe =>
          if pe.va = va ∧ pe.pagePresent = true ∧ pe.writable = true then
            match st.segments privSeg, t.vspace va with
            | some _, some old =>
              if old.access.write then (st, some .mappingConflict)
              else (privatizedState st tid t privSeg va, none)
            | _, _ => (st, some .invalidAddress)
          else (st, some .invalidTask)
        | none => (st, some .invalidTask)
      else (st, some .invalidTask)
    else (st, some .invalidTask)

def schedule (st : State) (tid : TaskId) : State × Option CowError :=
  if st.current = none then
    match st.tasks tid with
    | none => (st, some .invalidTask)
    | some t =>
      if t.status = .ready then (scheduledState st tid t, none)
      else (st, some .invalidTask)
  else (st, some .cpuBusy)

def yieldTask (st : State) (tid : TaskId) : State × Option CowError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some t =>
    if st.current = some tid ∧ t.status = .running then (yieldedState st tid t, none)
    else (st, some .invalidTask)

def destroyTask (st : State) (tid : TaskId) : State × Option CowError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some t =>
    if st.current = some tid ∨ t.status = .faulted then (st, some .invalidTask)
    else (destroyedState st tid, none)

def destroySegment (st : State) (sid : SegId) (stillMapped : Bool) :
    State × Option CowError :=
  match st.segments sid with
  | none => (st, some .noSuchSegment)
  | some _ =>
    if stillMapped then (st, some .stillMapped)
    else ({ st with segments := setAt st.segments sid none }, none)

-- Composite invariants kept by the model. `CowPairOk` follows the Rust check:
-- same segment+offset implies both sides are read-only; otherwise the mapped
-- frames differ (the full all-frames-disjoint check is outside the model).

def RunningOk (st : State) : Prop :=
  (∀ tid t, st.tasks tid = some t → st.current = some tid → t.status = .running) ∧
  (∀ tid t, st.tasks tid = some t → t.status = .running → st.current = some tid) ∧
  (∀ tid t tid' t', st.tasks tid = some t → t.status = .running →
    st.tasks tid' = some t' → t'.status = .running → tid = tid')

def FaultedOk (st : State) : Prop :=
  ∀ tid t, st.tasks tid = some t →
    (t.status = .faulted ↔ st.pending tid ≠ none) ∧ (t.status = .faulted → st.current ≠ some tid)

def frameOf (st : State) (m : Mapping) : Option Frame :=
  (st.segments m.segment).bind (fun s => nth? s.frames m.offset)

def CowPairOk (st : State) (parent child : TaskId) (va : VAddr) : Prop :=
  ∃ mp mc, (st.tasks parent).bind (fun t => t.vspace va) = some mp ∧
           (st.tasks child).bind (fun t => t.vspace va) = some mc ∧
           ((mp.segment = mc.segment ∧ mp.offset = mc.offset) →
              mp.access.write = false ∧ mc.access.write = false) ∧
           ((mp.segment ≠ mc.segment ∨ mp.offset ≠ mc.offset) →
              frameOf st mp ≠ frameOf st mc)

def CowsOk (st : State) : Prop :=
  ∀ p c va, (p, c, va) ∈ st.cows → CowPairOk st p c va

def WellFormed (st : State) : Prop :=
  RunningOk st ∧ FaultedOk st ∧ CowsOk st

-- Point lemmas.

theorem readOnly_write (m : Mapping) : (readOnly m).access.write = false := rfl

theorem privateRW_rw (sid : SegId) :
    (privateRW sid).access.read = true ∧ (privateRW sid).access.write = true := ⟨rfl, rfl⟩

theorem access_write_fault {st : State} {tid : TaskId} {va : VAddr} {t : Task} {m : Mapping}
    (hsrc : st.tasks tid = some t) (hcur : st.current = some tid)
    (hrun : t.status = .running) (hmap : t.vspace va = some m) (hro : m.access.write = false) :
    (access st tid va .write).outcome = .faulted true true ∧
    (access st tid va .write).state = faultedState st tid t va true .write := by
  unfold access faultedState
  simp [hsrc, hcur, hrun, hmap, accessAllowed, AccessKind.isWrite, hro]

theorem access_read_allowed {st : State} {tid : TaskId} {va : VAddr} {t : Task} {m : Mapping}
    (hsrc : st.tasks tid = some t) (hcur : st.current = some tid)
    (hrun : t.status = .running) (hmap : t.vspace va = some m) (hro : m.access.read = true) :
    (access st tid va .read).outcome = .allowed ∧ (access st tid va .read).state = st := by
  unfold access
  simp [hsrc, hcur, hrun, hmap, accessAllowed, hro]

theorem access_unmapped_fault {st : State} {tid : TaskId} {va : VAddr} {t : Task}
    (hsrc : st.tasks tid = some t) (hcur : st.current = some tid)
    (hrun : t.status = .running) (hmap : t.vspace va = none) :
    (access st tid va .read).outcome = .faulted false false ∧
    (access st tid va .read).state = faultedState st tid t va false .read := by
  unfold access faultedState
  simp [hsrc, hcur, hrun, hmap, AccessKind.isWrite]

theorem access_rejects_not_current {st : State} {tid : TaskId} {va : VAddr}
    (h : st.current ≠ some tid) :
    (access st tid va .read).outcome = .rejected .invalidTask ∧
    (access st tid va .read).state = st := by
  unfold access
  split
  · simp
  · split
    · rename_i hc
      exact absurd hc.1 h
    · simp

theorem access_rejects_no_task {st : State} {tid : TaskId} {va : VAddr}
    (h : st.tasks tid = none) :
    (access st tid va .read).outcome = .rejected .invalidTask ∧
    (access st tid va .read).state = st := by
  unfold access
  simp [h]

theorem establishCow_ok {st : State} {parent child : TaskId} {va : VAddr}
    {tp tc : Task} {mp : Mapping}
    (hp : st.tasks parent = some tp) (hc : st.tasks child = some tc)
    (hne : parent ≠ child) (hmap : tp.vspace va = some mp)
    (hchild : tc.vspace va = none) (hrw : mp.access.read = true ∧ mp.access.write = true) :
    establishCow st parent child va = (establishedState st parent child va tp tc mp, none) := by
  unfold establishCow establishedState
  simp [hp, hc, hne, hmap, hchild, hrw]

theorem establishCow_cows {st : State} {parent child : TaskId} {va : VAddr}
    {tp tc : Task} {mp : Mapping}
    (hp : st.tasks parent = some tp) (hc : st.tasks child = some tc)
    (hne : parent ≠ child) (hmap : tp.vspace va = some mp)
    (hchild : tc.vspace va = none) (hrw : mp.access.read = true ∧ mp.access.write = true) :
    (establishCow st parent child va).1.cows = st.cows ++ [(parent, child, va)] := by
  rw [establishCow_ok hp hc hne hmap hchild hrw]
  rfl

theorem establishCow_missing {st : State} {parent child : TaskId} {va : VAddr}
    (hp : st.tasks parent = none) :
    establishCow st parent child va = (st, some .invalidTask) := by
  simp [establishCow, hp]

theorem establishCow_conflict_child_mapped {st : State} {parent child : TaskId}
    {va : VAddr} {tp tc : Task} {mp mc : Mapping}
    (hp : st.tasks parent = some tp) (hc : st.tasks child = some tc)
    (hne : parent ≠ child) (hmap : tp.vspace va = some mp)
    (hchild : tc.vspace va = some mc) :
    establishCow st parent child va = (st, some .mappingConflict) := by
  simp [establishCow, hp, hc, hne, hmap, hchild]

theorem establishCow_parent_not_rw {st : State} {parent child : TaskId} {va : VAddr}
    {tp tc : Task} {mp : Mapping}
    (hp : st.tasks parent = some tp) (hc : st.tasks child = some tc)
    (hne : parent ≠ child) (hmap : tp.vspace va = some mp)
    (hchild : tc.vspace va = none) (hro : mp.access.read = false ∨ mp.access.write = false) :
    establishCow st parent child va = (st, some .mappingConflict) := by
  unfold establishCow
  simp [hp, hc, hne, hmap, hchild]
  rcases hro with h | h <;> simp [h]

theorem privatize_ok {st : State} {tid : TaskId} {privSeg : SegId} {va : VAddr}
    {t : Task} {pe : PendingExc} {old : Mapping} {seg : Segment}
    (hsrc : st.tasks tid = some t) (hfaulted : t.status = .faulted)
    (hcur : st.current = none) (hpend : st.pending tid = some pe)
    (hva : pe.va = va) (hpresent : pe.pagePresent = true) (hwritable : pe.writable = true)
    (hseg : st.segments privSeg = some seg) (hmap : t.vspace va = some old)
    (hro : old.access.write = false) :
    privatize st tid privSeg va = (privatizedState st tid t privSeg va, none) := by
  unfold privatize privatizedState
  simp [hsrc, hfaulted, hcur, hpend, hva, hpresent, hwritable, hseg, hmap, hro]

theorem privatized_task (st : State) (tid : TaskId) (t : Task) (privSeg : SegId) (va : VAddr) :
    (privatizedState st tid t privSeg va).tasks tid =
        some (readyPrivate t (setAt t.vspace va (some (privateRW privSeg)))) ∧
    (privatizedState st tid t privSeg va).pending tid = none ∧
    (privatizedState st tid t privSeg va).cows = st.cows := by
  refine ⟨?_, ?_, rfl⟩
  · simp [privatizedState, setAt]
  · simp [privatizedState, setAt]

theorem privatize_wrong_va {st : State} {tid : TaskId} {privSeg : SegId} {va : VAddr}
    {t : Task} {pe : PendingExc}
    (hsrc : st.tasks tid = some t) (hfaulted : t.status = .faulted)
    (hcur : st.current = none) (hpend : st.pending tid = some pe)
    (hne : pe.va ≠ va) :
    privatize st tid privSeg va = (st, some .invalidTask) := by
  unfold privatize
  simp [hsrc, hfaulted, hcur, hpend, hne]

theorem privatize_not_faulted {st : State} {tid : TaskId} {privSeg : SegId} {va : VAddr}
    {t : Task}
    (hsrc : st.tasks tid = some t) (h : t.status ≠ .faulted) :
    privatize st tid privSeg va = (st, some .invalidTask) := by
  simp [privatize, hsrc, h]

theorem schedule_ok {st : State} {tid : TaskId} {t : Task}
    (hidle : st.current = none) (hsrc : st.tasks tid = some t) (hready : t.status = .ready) :
    schedule st tid = (scheduledState st tid t, none) := by
  unfold schedule scheduledState
  simp [hidle, hsrc, hready]

theorem schedule_busy {st : State} {tid : TaskId} (h : st.current ≠ none) :
    schedule st tid = (st, some .cpuBusy) := by
  simp [schedule, h]

theorem yieldTask_ok {st : State} {tid : TaskId} {t : Task}
    (hsrc : st.tasks tid = some t) (hcur : st.current = some tid)
    (hrun : t.status = .running) :
    yieldTask st tid = (yieldedState st tid t, none) := by
  unfold yieldTask yieldedState
  simp [hsrc, hcur, hrun]

theorem destroyTask_ok {st : State} {tid : TaskId} {t : Task}
    (hsrc : st.tasks tid = some t) (hcur : st.current ≠ some tid)
    (hnot : t.status ≠ .faulted) :
    destroyTask st tid = (destroyedState st tid, none) := by
  unfold destroyTask destroyedState
  simp [hsrc, hcur, hnot]

theorem destroyTask_removed {st : State} {tid : TaskId} {t : Task}
    (hsrc : st.tasks tid = some t) (hcur : st.current ≠ some tid)
    (hnot : t.status ≠ .faulted) :
    (destroyTask st tid).1.tasks tid = none := by
  rw [destroyTask_ok hsrc hcur hnot]
  simp [destroyedState, setAt]

theorem destroyTask_keeps_other {st : State} {tid other : TaskId} {t : Task}
    (hsrc : st.tasks tid = some t) (hcur : st.current ≠ some tid)
    (hnot : t.status ≠ .faulted) (hne : other ≠ tid) :
    (destroyTask st tid).1.tasks other = st.tasks other := by
  rw [destroyTask_ok hsrc hcur hnot]
  simp [destroyedState, setAt, hne]

theorem destroyTask_rejects_faulted {st : State} {tid : TaskId} {t : Task}
    (hsrc : st.tasks tid = some t) (hfaulted : t.status = .faulted) :
    destroyTask st tid = (st, some .invalidTask) := by
  simp [destroyTask, hsrc, hfaulted]

theorem destroySegment_still {st : State} {sid : SegId} {seg : Segment}
    (hseg : st.segments sid = some seg) :
    destroySegment st sid true = (st, some .stillMapped) := by
  simp [destroySegment, hseg]

theorem destroySegment_ok {st : State} {sid : SegId} {seg : Segment}
    (hseg : st.segments sid = some seg) :
    destroySegment st sid false =
      ({ st with segments := setAt st.segments sid none }, none) := by
  simp [destroySegment, hseg]

-- Executable anchors.

def rwMapping : Mapping := { segment := 1, offset := 0, access := { read := true, write := true, exec := false } }
def roMapping : Mapping := { segment := 1, offset := 0, access := { read := true, write := false, exec := false } }

def seg1 : Segment := { frames := [0x1000], allowed := { read := true, write := true, exec := false } }
def seg2 : Segment := { frames := [0x2000], allowed := { read := true, write := true, exec := false } }

def baseState : State :=
  { segments := fun s => if s == 1 then some seg1 else if s == 2 then some seg2 else none
    tasks := fun t =>
      if t == 1 then some { status := .running, vspace := fun va => if va == 0x4000 then some rwMapping else none }
      else if t == 2 then some { status := .ready, vspace := fun _ => none }
      else none
    current := some 1
    pending := fun _ => none
    cows := [] }

def established : State := (establishCow baseState 1 2 0x4000).1

def childRunning : State :=
  { established with
    tasks := setAt established.tasks 2
      (some { status := .running,
              vspace := fun va => if va == 0x4000 then some roMapping else none })
    current := some 2 }

def faultedChild : State := (access childRunning 2 0x4000 .write).state

def idle : State := { baseState with current := none }

-- Establishment: both sides become read-only aliases; conflict faces are stable.
#eval (establishCow baseState 1 2 0x4000).2
#eval ((establishCow baseState 1 2 0x4000).1.cows.length)
#eval (establishCow baseState 1 2 0x8000).2
#eval (establishCow baseState 1 1 0x4000).2
#eval ((establishCow baseState 1 2 0x4000).1.tasks 2).map (fun t => t.vspace 0x4000)

-- Access: read allowed, write faults on the read-only alias, unmapped faults.
#eval (access childRunning 2 0x4000 .read).outcome
#eval (access childRunning 2 0x4000 .write).outcome
#eval ((access childRunning 2 0x4000 .write).state.tasks 2).map Task.status
#eval (access childRunning 2 0x8000 .read).outcome
#eval (access baseState 2 0x4000 .read).outcome

-- Private copy: returns the task to Ready and clears the pending fault.
#eval (privatize faultedChild 2 2 0x4000).2
#eval ((privatize faultedChild 2 2 0x4000).1.tasks 2).map Task.status
#eval ((privatize faultedChild 2 2 0x4000).1.pending 2).isSome
#eval ((privatize faultedChild 2 2 0x4000).1.cows.length)
#eval (privatize faultedChild 2 2 0x8000).2

-- Scheduling and destruction faces.
#eval (schedule baseState 2).2
#eval (schedule idle 2).2
#eval (yieldTask baseState 1).2
#eval (destroyTask idle 2).2
#eval ((destroyTask idle 2).1.tasks 2).isSome
#eval (destroyTask faultedChild 2).2
#eval (destroySegment baseState 1 true).2
#eval (destroySegment baseState 1 false).2

end Formal.Cow
