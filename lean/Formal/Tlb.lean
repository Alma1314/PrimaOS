import Lean

/-! Per-task TLB reference model, separate from the full page walk.

    `CachedXlat` mirrors a cached translation (frame + permission bits) and may be
    stale with respect to the page table. `TlbState` is a per-task cache; a task's
    cache is independent of every other task. Address canonicality/alignment and
    task existence are abstracted as the `validAddr` / `taskExists` guards; the
    four-level page walk and frame ownership are outside this model.

    See README.md for the Rust mapping and the limits of this proof channel. -/
namespace Formal.Tlb

abbrev VAddr := Nat
abbrev TaskId := Nat

/-- A cached translation: physical frame plus independent R/W/X permission bits.
    Permission bits are abstract booleans, not the COSIR/u64 ABI encoding. -/
structure CachedXlat where
  framePhys : Nat
  accessRead : Bool
  accessWrite : Bool
  accessExec : Bool
deriving DecidableEq, Repr

/-- Per-task TLB: task → virtual address → cached translation. A function is the
    weakest faithful representation; the Rust `BTreeMap<TaskId, BTreeMap<..>>`
    only ever performs point lookups/updates, which this matches. -/
abbrev TlbState := TaskId → VAddr → Option CachedXlat

def hasTlb (tlb : TlbState) (task : TaskId) (va : VAddr) : Bool :=
  (tlb task va).isSome

/-- Fill one entry, leaving every other `(task, va)` untouched. -/
def insert (tlb : TlbState) (task : TaskId) (va : VAddr) (x : CachedXlat) : TlbState :=
  fun t v => if t = task ∧ v = va then some x else tlb t v

/-- Single-address invalidation (INVLPG): drop only this task's entry for `va`. -/
def invalidateVa (tlb : TlbState) (task : TaskId) (va : VAddr) : TlbState :=
  fun t v => if t = task ∧ v = va then none else tlb t v

/-- Whole-task invalidation (CR3 reload / INVPCID): drop every entry of `task`. -/
def flushAll (tlb : TlbState) (task : TaskId) : TlbState :=
  fun t v => if t = task then none else tlb t v

/-- Task destruction: Rust removes the per-task map outright; observationally the
    same as `flushAll`, with no task-existence precondition. -/
def removeTask (tlb : TlbState) (task : TaskId) : TlbState :=
  flushAll tlb task

inductive TlbError where
  | invalidAddress
  | noSuchTask
  | notMapped
deriving DecidableEq, Repr

inductive AccessKind where
  | read
  | write
  | execute
deriving DecidableEq, Repr

def allows (x : CachedXlat) : AccessKind → Bool
  | .read => x.accessRead
  | .write => x.accessWrite
  | .execute => x.accessExec

/-- A translation attempt carries the resulting TLB: rejected paths return the
    input state unchanged, exactly like the Rust API. -/
inductive Result (α : Type) where
  | ok (value : α) (tlb : TlbState)
  | error (err : TlbError) (tlb : TlbState)

/-- The TLB carried by a result, independent of success/failure. -/
def resultTlb {α : Type} : Result α → TlbState
  | .ok _ t => t
  | .error _ t => t

/-- Translate through the TLB: hit returns the cached (possibly stale) entry and
    leaves the cache untouched; a miss walks the abstract page table and fills the
    cache; an unmapped address fills nothing. -/
def translate (validAddr : VAddr → Bool) (taskExists : TaskId → Bool)
    (pt : TaskId → VAddr → Option CachedXlat)
    (tlb : TlbState) (task : TaskId) (va : VAddr) : Result CachedXlat :=
  if validAddr va = false then .error .invalidAddress tlb
  else if taskExists task = false then .error .noSuchTask tlb
  else match tlb task va with
    | some x => .ok x tlb
    | none =>
      match pt task va with
      | some x => .ok x (insert tlb task va x)
      | none => .error .notMapped tlb

/-- Explicit invalidation with the same guard order as `translate`; failures do
    not modify the TLB. -/
def invalidate (validAddr : VAddr → Bool) (taskExists : TaskId → Bool)
    (tlb : TlbState) (task : TaskId) (va : VAddr) : TlbState × Option TlbError :=
  if validAddr va = false then (tlb, some .invalidAddress)
  else if taskExists task = false then (tlb, some .noSuchTask)
  else (invalidateVa tlb task va, none)

/-- Whole-task flush; fails only on a missing task, without modifying the TLB. -/
def flush (taskExists : TaskId → Bool) (tlb : TlbState)
    (task : TaskId) : TlbState × Option TlbError :=
  if taskExists task = false then (tlb, some .noSuchTask)
  else (flushAll tlb task, none)

/-- A task's cache agrees with the page table on every cached entry. Stale hits
    are exactly the cached entries where this predicate fails. -/
def FreshFor (tlb : TlbState) (pt : TaskId → VAddr → Option CachedXlat)
    (task : TaskId) : Prop :=
  ∀ va x, tlb task va = some x → pt task va = some x

-- Point-update lemmas. Every general theorem below reduces to these plus the
-- guard structure of `translate`; none depend on `native_decide`.

theorem insert_same (tlb : TlbState) (task : TaskId) (va : VAddr) (x : CachedXlat) :
    insert tlb task va x task va = some x := by
  simp [insert]

theorem insert_other {tlb : TlbState} {task : TaskId} {va : VAddr} {x : CachedXlat}
    {t : TaskId} {v : VAddr} (h : ¬(t = task ∧ v = va)) :
    insert tlb task va x t v = tlb t v := by
  simp [insert, h]

theorem invalidate_clears (tlb : TlbState) (task : TaskId) (va : VAddr) :
    invalidateVa tlb task va task va = none := by
  simp [invalidateVa]

theorem invalidate_other_task {tlb : TlbState} {task task' : TaskId} {va v : VAddr}
    (h : task' ≠ task) : invalidateVa tlb task va task' v = tlb task' v := by
  simp [invalidateVa, h]

theorem invalidate_other_va {tlb : TlbState} {task : TaskId} {va v : VAddr}
    (h : v ≠ va) : invalidateVa tlb task va task v = tlb task v := by
  simp [invalidateVa, h]

theorem flush_clears (tlb : TlbState) (task : TaskId) (v : VAddr) :
    flushAll tlb task task v = none := by
  simp [flushAll]

theorem flush_other_task {tlb : TlbState} {task task' : TaskId} {v : VAddr}
    (h : task' ≠ task) : flushAll tlb task task' v = tlb task' v := by
  simp [flushAll, h]

theorem removeTask_clears (tlb : TlbState) (task : TaskId) (v : VAddr) :
    removeTask tlb task task v = none := by
  simp [removeTask, flushAll]

theorem removeTask_other_task {tlb : TlbState} {task task' : TaskId} {v : VAddr}
    (h : task' ≠ task) : removeTask tlb task task' v = tlb task' v := by
  simp [removeTask, flushAll, h]

theorem hasTlb_insert (tlb : TlbState) (task : TaskId) (va : VAddr) (x : CachedXlat) :
    hasTlb (insert tlb task va x) task va = true := by
  simp [hasTlb, insert_same]

theorem hasTlb_invalidate (tlb : TlbState) (task : TaskId) (va : VAddr) :
    hasTlb (invalidateVa tlb task va) task va = false := by
  simp [hasTlb, invalidate_clears]

theorem hasTlb_flush (tlb : TlbState) (task : TaskId) (v : VAddr) :
    hasTlb (flushAll tlb task) task v = false := by
  simp [hasTlb, flushAll]

-- Idempotence: repeating an invalidation has no further effect.

theorem invalidate_idempotent (tlb : TlbState) (task : TaskId) (va : VAddr) :
    invalidateVa (invalidateVa tlb task va) task va = invalidateVa tlb task va := by
  funext t v
  by_cases h : t = task ∧ v = va
  · simp [invalidateVa, h]
  · simp [invalidateVa, h]

theorem flush_idempotent (tlb : TlbState) (task : TaskId) :
    flushAll (flushAll tlb task) task = flushAll tlb task := by
  funext t v
  by_cases h : t = task
  · simp [flushAll, h]
  · simp [flushAll, h]

theorem removeTask_idempotent (tlb : TlbState) (task : TaskId) :
    removeTask (removeTask tlb task) task = removeTask tlb task := by
  simp [removeTask, flush_idempotent]

-- Success/failure results of the guarded invalidation and flush wrappers.

theorem invalidate_ok {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {tlb : TlbState} {task : TaskId} {va : VAddr}
    (hv : validAddr va = true) (he : taskExists task = true) :
    invalidate validAddr taskExists tlb task va = (invalidateVa tlb task va, none) := by
  simp [invalidate, hv, he]

theorem invalidate_invalid {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {tlb : TlbState} {task : TaskId} {va : VAddr} (hv : validAddr va = false) :
    invalidate validAddr taskExists tlb task va = (tlb, some .invalidAddress) := by
  simp [invalidate, hv]

theorem invalidate_no_task {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {tlb : TlbState} {task : TaskId} {va : VAddr}
    (hv : validAddr va = true) (he : taskExists task = false) :
    invalidate validAddr taskExists tlb task va = (tlb, some .noSuchTask) := by
  simp [invalidate, hv, he]

theorem flush_ok {taskExists : TaskId → Bool} {tlb : TlbState} {task : TaskId}
    (he : taskExists task = true) :
    flush taskExists tlb task = (flushAll tlb task, none) := by
  simp [flush, he]

theorem flush_no_task {taskExists : TaskId → Bool} {tlb : TlbState} {task : TaskId}
    (he : taskExists task = false) :
    flush taskExists tlb task = (tlb, some .noSuchTask) := by
  simp [flush, he]

-- Translation: hits are served from the cache; misses from the page table.

theorem translate_hit {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr} {x : CachedXlat}
    (hv : validAddr va = true) (he : taskExists task = true) (h : tlb task va = some x) :
    translate validAddr taskExists pt tlb task va = .ok x tlb := by
  simp [translate, hv, he, h]

theorem translate_miss {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr} {x : CachedXlat}
    (hv : validAddr va = true) (he : taskExists task = true) (hm : tlb task va = none)
    (hp : pt task va = some x) :
    translate validAddr taskExists pt tlb task va = .ok x (insert tlb task va x) := by
  simp [translate, hv, he, hm, hp]

theorem translate_not_mapped {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr}
    (hv : validAddr va = true) (he : taskExists task = true) (hm : tlb task va = none)
    (hp : pt task va = none) :
    translate validAddr taskExists pt tlb task va = .error .notMapped tlb := by
  simp [translate, hv, he, hm, hp]

theorem translate_invalid_address {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr} (hv : validAddr va = false) :
    translate validAddr taskExists pt tlb task va = .error .invalidAddress tlb := by
  simp [translate, hv]

theorem translate_no_task {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr} (hv : validAddr va = true) (he : taskExists task = false) :
    translate validAddr taskExists pt tlb task va = .error .noSuchTask tlb := by
  simp [translate, hv, he]

/-- The central staleness bound: after an explicit single-address invalidation,
    a translation of that address is decided entirely by the current page table,
    never by a previously cached entry (valid address + live task). -/
theorem translate_after_invalidate {validAddr : VAddr → Bool} {taskExists : TaskId → Bool}
    {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState} {task : TaskId}
    {va : VAddr} (hv : validAddr va = true) (he : taskExists task = true) :
    translate validAddr taskExists pt (invalidateVa tlb task va) task va =
      (match pt task va with
       | some x => .ok x (insert (invalidateVa tlb task va) task va x)
       | none => .error .notMapped (invalidateVa tlb task va)) := by
  have hclear : (invalidateVa tlb task va) task va = none := invalidate_clears tlb task va
  simp [translate, hv, he, hclear]

/-- A cache fill only touches its own task; every other task's entry is intact.
    Since `translate`'s only state change is `insert`, this is the task-isolation
    property of a translation. -/
theorem insert_preserves_other_task {tlb : TlbState} {task : TaskId} {va : VAddr}
    {x : CachedXlat} {task' : TaskId} {v : VAddr} (hne : task' ≠ task) :
    insert tlb task va x task' v = tlb task' v := by
  exact insert_other (by intro hc; exact hne hc.1)

-- Freshness: an initially clean cache stays clean across invalidation, flush,
-- removal and page-table fills. A stale cache can only be repaired by explicit
-- invalidation (see `translate_after_invalidate`).

theorem invalidate_fresh_for {tlb : TlbState} {pt : TaskId → VAddr → Option CachedXlat}
    {task : TaskId} {va : VAddr} (hf : FreshFor tlb pt task) :
    FreshFor (invalidateVa tlb task va) pt task := by
  intro v y hy
  by_cases h : v = va
  · rw [h] at hy
    rw [invalidate_clears] at hy
    cases hy
  · rw [invalidate_other_va h] at hy
    exact hf v y hy

theorem flush_fresh_for (tlb : TlbState) (pt : TaskId → VAddr → Option CachedXlat)
    (task : TaskId) : FreshFor (flushAll tlb task) pt task := by
  intro v y hy
  rw [flush_clears] at hy
  cases hy

theorem removeTask_fresh_for (tlb : TlbState) (pt : TaskId → VAddr → Option CachedXlat)
    (task : TaskId) : FreshFor (removeTask tlb task) pt task :=
  flush_fresh_for tlb pt task

theorem translate_miss_fresh {pt : TaskId → VAddr → Option CachedXlat} {tlb : TlbState}
    {task : TaskId} {va : VAddr} {x : CachedXlat} (hf : FreshFor tlb pt task)
    (hp : pt task va = some x) :
    FreshFor (insert tlb task va x) pt task := by
  intro v y hy
  by_cases hv : v = va
  · rw [hv] at hy ⊢
    rw [insert_same] at hy
    simp only [Option.some.injEq] at hy
    rw [hp, hy]
  · have hnot : ¬(task = task ∧ v = va) := by
      intro hc
      exact hv hc.2
    rw [insert_other hnot] at hy
    exact hf v y hy

-- Executable anchors. `Repr` is available for the value/error projections only;
-- the TLB itself is a function and is observed through `hasTlb`.

def resultValue? : Result CachedXlat → Option CachedXlat
  | .ok v _ => some v
  | .error _ _ => none

def resultError? : Result CachedXlat → Option TlbError
  | .ok _ _ => none
  | .error e _ => some e

def validSample (va : VAddr) : Bool :=
  va == 0x4000 || va == 0x5000 || va == 0x6000 || va == 0x7000

def existsSample (task : TaskId) : Bool := task == 1 || task == 2

def samplePt (task : TaskId) (va : VAddr) : Option CachedXlat :=
  if task = 1 ∧ va = 0x4000 then some ⟨0x1000, true, false, false⟩
  else if task = 1 ∧ va = 0x5000 then some ⟨0x2000, true, true, false⟩
  else if task = 2 ∧ va = 0x4000 then some ⟨0x3000, true, false, false⟩
  else none

def emptyTlb : TlbState := fun _ _ => none

def sampleTlb : TlbState :=
  insert (insert emptyTlb 1 0x4000 ⟨0x1000, true, false, false⟩)
    2 0x4000 ⟨0x3000, true, false, false⟩

/-- A deliberately stale cache: `0x7000` is cached to frame `0x9000` while the
    page table currently maps it to `0xA000`. -/
def staleTlb : TlbState := insert emptyTlb 1 0x7000 ⟨0x9000, true, true, false⟩

def remappedPt (task : TaskId) (va : VAddr) : Option CachedXlat :=
  if task = 1 ∧ va = 0x7000 then some ⟨0xA000, true, true, false⟩ else samplePt task va

-- Miss fills the cache; the read-only permission is served but not write.
#eval resultValue? (translate validSample existsSample samplePt emptyTlb 1 0x4000)
#eval allows ⟨0x1000, true, false, false⟩ .read
#eval allows ⟨0x1000, true, false, false⟩ .write
#eval hasTlb (resultTlb (translate validSample existsSample samplePt emptyTlb 1 0x4000)) 1 0x4000

-- Without invalidation the stale entry wins over the remapped page table.
#eval resultValue? (translate validSample existsSample remappedPt staleTlb 1 0x7000)
-- After invalidation the next translation comes from the page table.
#eval resultValue?
  (translate validSample existsSample remappedPt
    (invalidate validSample existsSample staleTlb 1 0x7000).1 1 0x7000)
-- Invalidation is scoped to one address and one task.
#eval hasTlb (invalidate validSample existsSample sampleTlb 1 0x4000).1 1 0x4000
#eval hasTlb (invalidate validSample existsSample sampleTlb 1 0x4000).1 2 0x4000
-- Flush and removal clear exactly one task.
#eval hasTlb (flush existsSample sampleTlb 1).1 1 0x4000
#eval hasTlb (removeTask sampleTlb 1) 2 0x4000
-- Rejected operations carry their error and the unchanged state.
#eval resultError? (translate validSample existsSample samplePt emptyTlb 1 0x4001)
#eval resultError? (translate validSample existsSample samplePt emptyTlb 99 0x4000)
#eval resultError? (translate validSample existsSample samplePt emptyTlb 1 0x6000)
#eval (invalidate validSample existsSample sampleTlb 1 0x4001).2
#eval (flush existsSample sampleTlb 99).2

end Formal.Tlb
