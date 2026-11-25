import Lean

/-! Minimal reference model of cosir `memseg_refine`: per-task page-table/vspace
    dual views over address space plus a frame-ownership partition.

    The Rust state keeps a four-level page table, a vspace `BTreeMap`, a frame
    allocator, and checks W6/W7 with explicit set scans. Here the table and view
    are total functions, the frame partition is a single `owner` function (each
    frame has at most one owner; `free` is the free owner), and W7 is the pointwise
    agreement of the two views plus PTE well-formedness. The view scan that
    `destroy_verified` performs for `StillMapped` is abstracted as a caller-supplied
    guard (`stillMapped`): the model proves what follows from a clean guard but does
    not enumerate task views.

    Not modelled: the four-level tree and its table-frame ownership check, multi-page
    atomic map/unmap, address canonicality/alignment, `next_seg` overflow, and the
    concrete allocator order. See README.md for the Rust mapping and limits. -/
namespace Formal.Memseg

abbrev Frame := Nat
abbrev SegId := Nat
abbrev TaskId := Nat
abbrev VAddr := Nat

structure Access where
  read : Bool
  write : Bool
  exec : Bool
deriving DecidableEq, Repr

def Access.valid (a : Access) : Bool :=
  a.read || a.write || a.exec

/-- Every permission bit of `a` is allowed by `b` (Rust: `b & a = a`). -/
def Access.sub (a b : Access) : Bool :=
  (!a.read || b.read) && (!a.write || b.write) && (!a.exec || b.exec)

structure Segment where
  frames : List Frame
  allowed : Access
deriving DecidableEq, Repr

/-- Bounded list index lookup (Lean core has no `List.get?`). -/
def nth? {α : Type} : List α → Nat → Option α
  | [], _ => none
  | x :: _, 0 => some x
  | _ :: xs, n + 1 => nth? xs n

/-- Boolean no-duplicate check (the model uses `Bool` guards; `List.Nodup` is not
    used in executable conditions). -/
def nodupB {α : Type} [DecidableEq α] : List α → Bool
  | [] => true
  | x :: xs => !(xs.contains x) && nodupB xs

structure Mapping where
  segment : SegId
  offset : Nat
  access : Access
deriving DecidableEq, Repr

/-- The PTE fields that W7 checks are exactly the derived ones; the leaf frame and
    mapping are carried explicitly as in the Rust `Pte`. -/
structure Pte where
  present : Bool
  user : Bool
  writable : Bool
  nx : Bool
  frame : Frame
  mapping : Mapping
deriving DecidableEq, Repr

structure TaskCtx where
  table : VAddr → Option Pte
  vspace : VAddr → Option Mapping
  entryPage : VAddr
  tableFrames : List Frame

inductive Owner where
  | free
  | segment (sid : SegId)
  | table (tid : TaskId)
deriving DecidableEq, Repr

structure State where
  frameUniverse : Frame → Bool
  free : Frame → Bool
  owner : Frame → Option Owner
  segments : SegId → Option Segment
  tasks : TaskId → Option TaskCtx
  nextSeg : SegId

inductive MemSegError where
  | zeroSize
  | invalidAccess
  | noMemory
  | invalidSegment
  | invalidTask
  | rightsExceeded
  | outOfBounds
  | overlap
  | notMapped
  | entryMapping
  | stillMapped
deriving DecidableEq, Repr

def setAt {α β : Type} [DecidableEq α] (f : α → β) (a : α) (b : β) : α → β :=
  fun x => if x = a then b else f x

theorem setAt_same {α β : Type} [DecidableEq α] (f : α → β) (a : α) (b : β) :
    setAt f a b a = b := by
  simp [setAt]

theorem setAt_other {α β : Type} [DecidableEq α] (f : α → β) {a x : α} (b : β)
    (h : x ≠ a) : setAt f a b x = f x := by
  simp [setAt, h]

/-- W7's dual-view agreement: the leaf projection of the table equals the view. -/
def ViewsAgree (tc : TaskCtx) : Prop :=
  ∀ va, (tc.table va).map Pte.mapping = tc.vspace va

/-- A leaf PTE is well formed with respect to the active segments: present, user,
    derived W/NX bits, leaf frame matching the segment offset, and access within
    the segment's allowed set. -/
def PteOk (segments : SegId → Option Segment) (p : Pte) : Prop :=
  p.present = true ∧ p.user = true ∧ p.writable = p.mapping.access.write ∧
  p.nx = (!p.mapping.access.exec) ∧
  ∃ seg, segments p.mapping.segment = some seg ∧
    nth? seg.frames p.mapping.offset = some p.frame ∧
    p.mapping.access.sub seg.allowed = true

/-- W6 as a functional partition: free frames own exactly `Owner.free`, segment
    and table frames own their respective object, every universe frame has an owner,
    and each owned frame belongs to the universe. A single `owner` function makes
    cross-object disjointness automatic. -/
def OwnerOk (st : State) : Prop :=
  (∀ f, st.owner f = some Owner.free → st.free f = true ∧ st.frameUniverse f = true) ∧
  (∀ f, st.free f = true → st.owner f = some Owner.free) ∧
  (∀ f, st.frameUniverse f = true → ∃ o, st.owner f = some o) ∧
  (∀ sid seg f, st.segments sid = some seg → f ∈ seg.frames →
    st.owner f = some (Owner.segment sid) ∧ st.frameUniverse f = true) ∧
  (∀ tid tc f, st.tasks tid = some tc → f ∈ tc.tableFrames →
    st.owner f = some (Owner.table tid) ∧ st.frameUniverse f = true)

def WellFormed (st : State) : Prop :=
  (∀ tid tc, st.tasks tid = some tc → ViewsAgree tc) ∧
  (∀ tid tc va p, st.tasks tid = some tc → tc.table va = some p → PteOk st.segments p) ∧
  OwnerOk st

def makePte (sid : SegId) (seg : Segment) (offset : Nat) (access : Access) : Option Pte :=
  match nth? seg.frames offset with
  | none => none
  | some frame =>
    some { present := true, user := true, writable := access.write, nx := !access.exec,
           frame := frame, mapping := { segment := sid, offset := offset, access := access } }

theorem makePte_ok {sid : SegId} {seg : Segment} {offset : Nat} {access : Access}
    {segments : SegId → Option Segment} {p : Pte}
    (hseg : segments sid = some seg) (hsub : access.sub seg.allowed = true)
    (h : makePte sid seg offset access = some p) : PteOk segments p := by
  unfold makePte at h
  cases hget : nth? seg.frames offset with
  | none => simp [hget] at h
  | some frame =>
    simp [hget] at h
    cases h
    exact ⟨rfl, rfl, rfl, rfl, seg, hseg, hget, hsub⟩

/-- Single-page map. Rejected operations return the input state unchanged. The
    page table and view are updated together, which is the W7 invariant. -/
def mapEntry (st : State) (tid : TaskId) (va : VAddr) (sid : SegId)
    (offset : Nat) (access : Access) : State × Option MemSegError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some tc =>
    if access.valid then
      match st.segments sid with
      | none => (st, some .invalidSegment)
      | some seg =>
        if access.sub seg.allowed then
          if tc.vspace va = none then
            match makePte sid seg offset access with
            | none => (st, some .outOfBounds)
            | some pte =>
              ({ st with tasks := setAt st.tasks tid (some { tc with
                table := setAt tc.table va (some pte),
                vspace := setAt tc.vspace va (some pte.mapping) }) }, none)
          else (st, some .overlap)
        else (st, some .rightsExceeded)
    else (st, some .invalidAccess)

/-- Single-page unmap with the W8 entry-page guard; the entry check precedes the
    mapped check as in `unmap_verified`. -/
def unmapEntry (st : State) (tid : TaskId) (va : VAddr) : State × Option MemSegError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some tc =>
    if va = tc.entryPage then (st, some .entryMapping)
    else if tc.vspace va = none then (st, some .notMapped)
    else ({ st with tasks := setAt st.tasks tid (some { tc with
      table := setAt tc.table va none,
      vspace := setAt tc.vspace va none }) }, none)

/-- Segment creation: take the caller-selected frames (any free, non-duplicate set
    is accepted, over-approximating the allocator's choice), transfer them from
    `free` to `Owner.segment sid`, and advance `nextSeg`. -/
def createSegment (st : State) (sid : SegId) (frames : List Frame) (allowed : Access) :
    State × Option MemSegError :=
  if frames = [] then (st, some .zeroSize)
  else if allowed.valid then
    if nodupB frames then
      if frames.all (fun f => st.free f && st.frameUniverse f) then
        ({ st with
          segments := setAt st.segments sid (some { frames := frames, allowed := allowed }),
          owner := fun f => if f ∈ frames then some (Owner.segment sid) else st.owner f,
          free := fun f => if f ∈ frames then false else st.free f,
          nextSeg := sid + 1 }, none)
      else (st, some .noMemory)
    else (st, some .invalidSegment)
  else (st, some .invalidAccess)

/-- Destroying a segment frees all of its frames. The `StillMapped` scan is the
    caller's guard; a true guard rejects without changing state. -/
def destroySegment (st : State) (sid : SegId) (stillMapped : Bool) :
    State × Option MemSegError :=
  match st.segments sid with
  | none => (st, some .invalidSegment)
  | some seg =>
    if stillMapped then (st, some .stillMapped)
    else ({ st with
      segments := setAt st.segments sid none,
      owner := fun f => if f ∈ seg.frames then some .free else st.owner f,
      free := fun f => if f ∈ seg.frames then true else st.free f }, none)

/-- Task destruction frees its table-structure frames. -/
def destroyTask (st : State) (tid : TaskId) : State × Option MemSegError :=
  match st.tasks tid with
  | none => (st, some .invalidTask)
  | some tc =>
    ({ st with
      tasks := setAt st.tasks tid none,
      owner := fun f => if f ∈ tc.tableFrames then some .free else st.owner f,
      free := fun f => if f ∈ tc.tableFrames then true else st.free f }, none)

/-- A task starts with empty dual views and one table frame taken from free. -/
def spawnTask (st : State) (tid : TaskId) (entryPage : VAddr) (tableFrame : Frame) :
    State × Option MemSegError :=
  match st.tasks tid with
  | some _ => (st, some .invalidTask)
  | none =>
    if st.free tableFrame then
      if st.owner tableFrame = some Owner.free then
        ({ st with
          tasks := setAt st.tasks tid (some {
            table := fun _ => none,
            vspace := fun _ => none,
            entryPage := entryPage,
            tableFrames := [tableFrame] }),
          owner := setAt st.owner tableFrame (some (Owner.table tid)),
          free := setAt st.free tableFrame false }, none)
      else (st, some .noMemory)
    else (st, some .noMemory)

-- Point lemmas for the view updates.

theorem viewsAgree_map {tc : TaskCtx} (hwf : ViewsAgree tc) {va : VAddr} {pte : Pte} :
    ViewsAgree { tc with
      table := setAt tc.table va (some pte),
      vspace := setAt tc.vspace va (some pte.mapping) } := by
  intro va'
  by_cases h : va' = va
  · rw [h]
    simp [setAt]
  · simp [setAt, h, hwf va']

theorem viewsAgree_unmap {tc : TaskCtx} (hwf : ViewsAgree tc) {va : VAddr} :
    ViewsAgree { tc with
      table := setAt tc.table va none,
      vspace := setAt tc.vspace va none } := by
  intro va'
  by_cases h : va' = va
  · rw [h]
    simp [setAt]
  · simp [setAt, h, hwf va']

-- Success equations pin the exact resulting state; failures are shown separately.

theorem mapEntry_ok {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} {tc : TaskCtx} {seg : Segment} {pte : Pte}
    (hsrc : st.tasks tid = some tc) (hvalid : access.valid = true)
    (hseg : st.segments sid = some seg) (hsub : access.sub seg.allowed = true)
    (hfree : tc.vspace va = none) (hpte : makePte sid seg offset access = some pte) :
    mapEntry st tid va sid offset access =
      ({ st with tasks := setAt st.tasks tid (some { tc with
        table := setAt tc.table va (some pte),
        vspace := setAt tc.vspace va (some pte.mapping) }) }, none) := by
  simp [mapEntry, hsrc, hvalid, hseg, hsub, hfree, hpte]

theorem mapEntry_no_task {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} (h : st.tasks tid = none) :
    mapEntry st tid va sid offset access = (st, some MemSegError.invalidTask) := by
  simp [mapEntry, h]

theorem mapEntry_invalid_access {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) (hinvalid : access.valid = false) :
    mapEntry st tid va sid offset access = (st, some MemSegError.invalidAccess) := by
  simp [mapEntry, hsrc, hinvalid]

theorem mapEntry_invalid_segment {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) (hvalid : access.valid = true) (hseg : st.segments sid = none) :
    mapEntry st tid va sid offset access = (st, some MemSegError.invalidSegment) := by
  simp [mapEntry, hsrc, hvalid, hseg]

theorem mapEntry_overlap {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} {tc : TaskCtx} {seg : Segment} {m : Mapping}
    (hsrc : st.tasks tid = some tc) (hvalid : access.valid = true)
    (hseg : st.segments sid = some seg) (hsub : access.sub seg.allowed = true)
    (hocc : tc.vspace va = some m) :
    mapEntry st tid va sid offset access = (st, some MemSegError.overlap) := by
  simp [mapEntry, hsrc, hvalid, hseg, hsub, hocc]

theorem unmapEntry_ok {st : State} {tid : TaskId} {va : VAddr} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) (hentry : va ≠ tc.entryPage)
    (hmapped : tc.vspace va ≠ none) :
    unmapEntry st tid va =
      ({ st with tasks := setAt st.tasks tid (some { tc with
        table := setAt tc.table va none,
        vspace := setAt tc.vspace va none }) }, none) := by
  simp [unmapEntry, hsrc, hentry, hmapped]

theorem unmapEntry_entry {st : State} {tid : TaskId} {va : VAddr} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) (h : va = tc.entryPage) :
    unmapEntry st tid va = (st, some MemSegError.entryMapping) := by
  simp [unmapEntry, hsrc, h]

theorem unmapEntry_not_mapped {st : State} {tid : TaskId} {va : VAddr} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) (hentry : va ≠ tc.entryPage)
    (h : tc.vspace va = none) :
    unmapEntry st tid va = (st, some MemSegError.notMapped) := by
  simp [unmapEntry, hsrc, hentry, h]

theorem createSegment_ok {st : State} {sid : SegId} {frames : List Frame} {allowed : Access}
    (hne : frames ≠ []) (hvalid : allowed.valid = true) (hnodup : nodupB frames = true)
    (hfree : frames.all (fun f => st.free f && st.frameUniverse f) = true) :
    createSegment st sid frames allowed =
      ({ st with
        segments := setAt st.segments sid (some { frames := frames, allowed := allowed }),
        owner := fun f => if f ∈ frames then some (Owner.segment sid) else st.owner f,
        free := fun f => if f ∈ frames then false else st.free f,
        nextSeg := sid + 1 }, none) := by
  simp [createSegment, hne, hvalid, hnodup, hfree]

theorem createSegment_owned {st : State} {sid : SegId} {frames : List Frame} {allowed : Access}
    (hne : frames ≠ []) (hvalid : allowed.valid = true) (hnodup : nodupB frames = true)
    (hfree : frames.all (fun f => st.free f && st.frameUniverse f) = true)
    {f : Frame} (hf : f ∈ frames) :
    (createSegment st sid frames allowed).1.owner f = some (Owner.segment sid) ∧
    (createSegment st sid frames allowed).1.free f = false := by
  rw [createSegment_ok hne hvalid hnodup hfree]
  simp [hf]

theorem createSegment_other_frame {st : State} {sid : SegId} {frames : List Frame}
    {allowed : Access} (hne : frames ≠ []) (hvalid : allowed.valid = true)
    (hnodup : nodupB frames = true)
    (hfree : frames.all (fun f => st.free f && st.frameUniverse f) = true)
    {f : Frame} (hf : f ∉ frames) :
    (createSegment st sid frames allowed).1.owner f = st.owner f ∧
    (createSegment st sid frames allowed).1.free f = st.free f := by
  rw [createSegment_ok hne hvalid hnodup hfree]
  simp [hf]

theorem createSegment_fail_empty {st : State} {sid : SegId} {allowed : Access} :
    createSegment st sid [] allowed = (st, some MemSegError.zeroSize) := by
  simp [createSegment]

theorem createSegment_fail_access {st : State} {sid : SegId} {frames : List Frame}
    {allowed : Access} (hne : frames ≠ []) (hvalid : allowed.valid = false) :
    createSegment st sid frames allowed = (st, some MemSegError.invalidAccess) := by
  simp [createSegment, hne, hvalid]

theorem destroySegment_ok {st : State} {sid : SegId} {seg : Segment} {stillMapped : Bool}
    (hseg : st.segments sid = some seg) (hstill : stillMapped = false) :
    destroySegment st sid stillMapped =
      ({ st with
        segments := setAt st.segments sid none,
        owner := fun f => if f ∈ seg.frames then some Owner.free else st.owner f,
        free := fun f => if f ∈ seg.frames then true else st.free f }, none) := by
  simp [destroySegment, hseg, hstill]

theorem destroySegment_freed {st : State} {sid : SegId} {seg : Segment} {stillMapped : Bool}
    (hseg : st.segments sid = some seg) (hstill : stillMapped = false)
    {f : Frame} (hf : f ∈ seg.frames) :
    (destroySegment st sid stillMapped).1.owner f = some Owner.free ∧
    (destroySegment st sid stillMapped).1.free f = true := by
  rw [destroySegment_ok hseg hstill]
  simp [hf]

theorem destroySegment_removed {st : State} {sid : SegId} {seg : Segment}
    {stillMapped : Bool} (hseg : st.segments sid = some seg) (hstill : stillMapped = false) :
    (destroySegment st sid stillMapped).1.segments sid = none := by
  rw [destroySegment_ok hseg hstill]
  simp [setAt]

theorem destroySegment_still {st : State} {sid : SegId} {seg : Segment} {stillMapped : Bool}
    (hseg : st.segments sid = some seg) (hstill : stillMapped = true) :
    destroySegment st sid stillMapped = (st, some MemSegError.stillMapped) := by
  simp [destroySegment, hseg, hstill]

theorem destroyTask_ok {st : State} {tid : TaskId} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) :
    destroyTask st tid =
      ({ st with
        tasks := setAt st.tasks tid none,
        owner := fun f => if f ∈ tc.tableFrames then some Owner.free else st.owner f,
        free := fun f => if f ∈ tc.tableFrames then true else st.free f }, none) := by
  simp [destroyTask, hsrc]

theorem destroyTask_freed {st : State} {tid : TaskId} {tc : TaskCtx}
    (hsrc : st.tasks tid = some tc) {f : Frame} (hf : f ∈ tc.tableFrames) :
    (destroyTask st tid).1.owner f = some Owner.free ∧
    (destroyTask st tid).1.free f = true := by
  rw [destroyTask_ok hsrc]
  simp [hf]

theorem spawnTask_ok {st : State} {tid : TaskId} {entryPage : VAddr} {tableFrame : Frame}
    (htask : st.tasks tid = none) (hfree : st.free tableFrame = true)
    (howner : st.owner tableFrame = some Owner.free) :
    spawnTask st tid entryPage tableFrame =
      ({ st with
        tasks := setAt st.tasks tid (some {
          table := fun _ => none,
          vspace := fun _ => none,
          entryPage := entryPage,
          tableFrames := [tableFrame] }),
        owner := setAt st.owner tableFrame (some (Owner.table tid)),
        free := setAt st.free tableFrame false }, none) := by
  simp [spawnTask, htask, hfree, howner]

theorem spawnTask_views {st : State} {tid : TaskId} {entryPage : VAddr} {tableFrame : Frame}
    (htask : st.tasks tid = none) (hfree : st.free tableFrame = true)
    (howner : st.owner tableFrame = some Owner.free) :
    ∃ tc, (spawnTask st tid entryPage tableFrame).1.tasks tid = some tc ∧ ViewsAgree tc := by
  refine ⟨⟨fun _ => none, fun _ => none, entryPage, [tableFrame]⟩, ?_, ?_⟩
  · rw [spawnTask_ok htask hfree howner]
    exact setAt_same _ _ _
  · intro va
    simp

-- Preservation of the two W7 conjuncts across a successful map/unmap.

theorem mapEntry_success_views {st : State} {tid : TaskId} {va : VAddr} {sid : SegId}
    {offset : Nat} {access : Access} {tc tc' : TaskCtx} {seg : Segment} {pte : Pte}
    (hsrc : st.tasks tid = some tc) (hvalid : access.valid = true)
    (hseg : st.segments sid = some seg) (hsub : access.sub seg.allowed = true)
    (hfree : tc.vspace va = none) (hpte : makePte sid seg offset access = some pte)
    (hwf : ViewsAgree tc)
    (hres : (mapEntry st tid va sid offset access).1.tasks tid = some tc') : ViewsAgree tc' := by
  let newTc : TaskCtx := { tc with
    table := setAt tc.table va (some pte),
    vspace := setAt tc.vspace va (some pte.mapping) }
  have hnew : (mapEntry st tid va sid offset access).1.tasks tid = some newTc := by
    rw [mapEntry_ok hsrc hvalid hseg hsub hfree hpte]
    exact setAt_same _ _ _
  rw [hnew] at hres
  injection hres with hEq
  rw [← hEq]
  exact viewsAgree_map hwf

theorem unmapEntry_success_views {st : State} {tid : TaskId} {va : VAddr} {tc tc' : TaskCtx}
    (hsrc : st.tasks tid = some tc) (hentry : va ≠ tc.entryPage) (hmapped : tc.vspace va ≠ none)
    (hwf : ViewsAgree tc)
    (hres : (unmapEntry st tid va).1.tasks tid = some tc') : ViewsAgree tc' := by
  let newTc : TaskCtx := { tc with
    table := setAt tc.table va none,
    vspace := setAt tc.vspace va none }
  have hnew : (unmapEntry st tid va).1.tasks tid = some newTc := by
    rw [unmapEntry_ok hsrc hentry hmapped]
    exact setAt_same _ _ _
  rw [hnew] at hres
  injection hres with hEq
  rw [← hEq]
  exact viewsAgree_unmap hwf

-- Executable anchors. State itself is a function record, so anchors observe the
-- printable projections (owner, free, segments, errors, the view at a VA).

def sampleUniverse (f : Frame) : Bool :=
  f == 0x1000 || f == 0x2000 || f == 0x3000 || f == 0x4000 || f == 0x5000

def emptyState : State :=
  { frameUniverse := sampleUniverse
    free := sampleUniverse
    owner := fun f => if sampleUniverse f then some Owner.free else none
    segments := fun _ => none
    tasks := fun _ => none
    nextSeg := 1 }

def rwAccess : Access := ⟨true, true, false⟩
def rAccess : Access := ⟨true, false, false⟩

def created : State := (createSegment emptyState 1 [0x1000, 0x2000] rwAccess).1
def spawned : State := (spawnTask created 1 0x4000 0x5000).1
def mapped : State := (mapEntry spawned 1 0x4000 1 0 rAccess).1

-- Segment creation: frames move from free to the segment owner.
#eval (createSegment emptyState 1 [0x1000, 0x2000] rwAccess).2
#eval ((createSegment emptyState 1 [0x1000, 0x2000] rwAccess).1.owner 0x1000)
#eval ((createSegment emptyState 1 [0x1000, 0x2000] rwAccess).1.free 0x1000)
#eval (createSegment emptyState 1 [] rwAccess).2
#eval (createSegment emptyState 1 [0x1000] ⟨false, false, false⟩).2
#eval (createSegment emptyState 1 [0x1000] ⟨false, false, true⟩).2

-- Spawn takes one table frame.
#eval ((spawnTask created 1 0x4000 0x5000).1.owner 0x5000)
#eval ((spawnTask created 1 0x4000 0x5000).1.free 0x5000)
#eval (spawnTask created 1 0x4000 0x5000).2

-- Map: success and each rejection face; access cannot exceed the segment.
#eval (mapEntry spawned 1 0x4000 1 0 rAccess).2
#eval ((mapEntry spawned 1 0x4000 1 0 rAccess).1.tasks 1).map (fun tc => tc.vspace 0x4000)
#eval (mapEntry spawned 1 0x4000 1 0 ⟨false, false, false⟩).2
#eval (mapEntry spawned 1 0x4000 1 0 ⟨true, false, true⟩).2
#eval (mapEntry spawned 1 0x4000 1 2 rAccess).2
#eval (mapEntry spawned 1 0x4000 9 0 rAccess).2
#eval (mapEntry mapped 1 0x4000 1 0 rAccess).2

-- Unmap: entry-page guard precedes the mapped check.
#eval (unmapEntry mapped 1 0x4000).2
#eval (unmapEntry mapped 1 0x8000).2
#eval ((unmapEntry mapped 1 0x4000).1.tasks 2).isSome

-- Destruction frees frames; the still-mapped guard rejects without effect.
#eval ((destroySegment created 1 false).1.owner 0x1000)
#eval ((destroySegment created 1 false).1.segments 1)
#eval (destroySegment created 1 true).2
#eval ((destroyTask spawned 1).1.owner 0x5000)
#eval ((destroyTask spawned 1).1.free 0x5000)

end Formal.Memseg
