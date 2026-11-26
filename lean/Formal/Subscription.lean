import Formal.Container

/-! §12.4 / W15 subscription lifecycle: an exception source keeps a subscriber set
    (one bit per Task, so "at most one active subscription per task" is by
    representation). W15 requires every subscriber to be an active Task and to
    own at least one usable `ExcSubscribe` capability pointing at the source.
    Losing the last such capability (`cap_drop`, send, bind, or any move out of
    `HeldBy`) terminates the subscription in the same step.

    The steps are stated as relations because they carry the source/capability
    witnesses the invariant needs ("this is the last usable capability" is a
    caller precondition). The model proves that subscribe/unsubscribe, coupled
    last-capability drop/send, and source destruction preserve W15 and clear or
    add the subscriber bit. It does **not** prove the implementation's lastness
    checker, subscription liveness while another capability remains, audit
    events (§13.1), source kinds, notification delivery, or Rust refinement.
    The `ExcSubscribe` rights bit is model-local (COSIR does not fix numeric bit
    assignments; `Derive` is 0x40 in this model, so `ExcSubscribe` uses 0x80). -/
namespace Formal.Subscription

open Formal
open Formal.TokenLocation
open Formal.Container

def capExcSubscribe : UInt32 := 0x80

structure Source where
  active : Bool
  subscribers : Nat → Bool

structure SubWorld where
  base : World
  sources : Nat → Option Source

/-- A usable `ExcSubscribe` capability: an active, held-by-`holder` record that
    points at `source` and carries the subscribe right. -/
def UsableFor (w : SubWorld) (holder source capId : Nat) : Prop :=
  ∃ cap, w.base.snap.state.locations capId = some (.heldBy holder) ∧
    w.base.snap.state.caps capId = some cap ∧ cap.active = true ∧
    cap.rights &&& capExcSubscribe ≠ 0 ∧ cap.obj = .excSource source

/-- W15: active tasks only, one bit per task (representation), and a usable
    capability witness for every subscriber. -/
def W15 (w : SubWorld) : Prop :=
  ∀ s src, w.sources s = some src → src.active = true →
    (∀ t, src.subscribers t = true → ∃ tk, w.base.tasks t = some tk ∧ tk.active = true) ∧
    (∀ t, src.subscribers t = true → ∃ capId, UsableFor w t s capId)

def addSubscriber (src : Source) (t : Nat) : Source :=
  { src with subscribers := fun u => u == t || src.subscribers u }

def clearSubscriber (src : Source) (t : Nat) : Source :=
  { src with subscribers := fun u => if u = t then false else src.subscribers u }

def setSource (w : SubWorld) (s : Nat) (src : Source) : SubWorld :=
  { w with sources := fun x => if x = s then some src else w.sources x }

def endSub (w : SubWorld) (s t : Nat) : SubWorld :=
  { w with sources := fun x =>
      if x = s then (w.sources s).map (fun src => clearSubscriber src t) else w.sources x }

theorem setSource_self (w : SubWorld) (s : Nat) (src : Source) :
    (setSource w s src).sources s = some src := by
  simp [setSource]

theorem setSource_other (w : SubWorld) (s : Nat) (src : Source) {x : Nat} (h : x ≠ s) :
    (setSource w s src).sources x = w.sources x := by
  simp [setSource, h]

theorem addSubscriber_self (src : Source) (t : Nat) : (addSubscriber src t).subscribers t = true := by
  simp only [addSubscriber]
  have h : (t == t) = true := Nat.beq_refl t
  rw [h]
  rfl

theorem addSubscriber_other (src : Source) {t u : Nat} (h : u ≠ t) :
    (addSubscriber src t).subscribers u = src.subscribers u := by
  have hf : (u == t) = false := beq_eq_false_iff_ne.mpr h
  simp [addSubscriber, hf]

theorem addSubscriber_active (src : Source) (t : Nat) :
    (addSubscriber src t).active = src.active := rfl

theorem clearSubscriber_self (src : Source) (t : Nat) :
    (clearSubscriber src t).subscribers t = false := by
  simp [clearSubscriber]

theorem clearSubscriber_other (src : Source) {t u : Nat} (h : u ≠ t) :
    (clearSubscriber src t).subscribers u = src.subscribers u := by
  simp [clearSubscriber, h]

theorem clearSubscriber_active (src : Source) (t : Nat) :
    (clearSubscriber src t).active = src.active := rfl

theorem endSub_self (w : SubWorld) (s t : Nat) :
    (endSub w s t).sources s = (w.sources s).map (fun src => clearSubscriber src t) := by
  simp [endSub]

theorem endSub_other (w : SubWorld) (s t : Nat) {x : Nat} (h : x ≠ s) :
    (endSub w s t).sources x = w.sources x := by
  simp [endSub, h]

def endSubCap (w : SubWorld) (cap : Capability) (t : Nat) : SubWorld :=
  match cap.obj with
  | .excSource s => endSub w s t
  | _ => w

theorem endSubCap_excSource {w : SubWorld} {cap : Capability} {s t : Nat}
    (h : cap.obj = .excSource s) : endSubCap w cap t = endSub w s t := by
  unfold endSubCap
  rw [h]

theorem w15_active {w : SubWorld} {s : Nat} {src : Source} (h15 : W15 w)
    (hsrc : w.sources s = some src) (hact : src.active = true) {t : Nat}
    (hsub : src.subscribers t = true) : ∃ tk, w.base.tasks t = some tk ∧ tk.active = true :=
  (h15 s src hsrc hact).1 t hsub

theorem w15_witness {w : SubWorld} {s : Nat} {src : Source} (h15 : W15 w)
    (hsrc : w.sources s = some src) (hact : src.active = true) {t : Nat}
    (hsub : src.subscribers t = true) : ∃ capId, UsableFor w t s capId :=
  (h15 s src hsrc hact).2 t hsub

theorem usableFor_snap {w w' : SubWorld} {holder s c capId : Nat} {dst : Option Location}
    (hsnap : w'.base.snap = replace w.base.snap capId dst) (hne : c ≠ capId)
    (huc : UsableFor w holder s c) : UsableFor w' holder s c := by
  obtain ⟨cap, hloc, hcap, hact, hright, hobj⟩ := huc
  refine ⟨cap, ?_, ?_, hact, hright, hobj⟩
  · rw [hsnap, replace_loc_other _ _ _ _ hne]
    exact hloc
  · rw [hsnap]
    simpa [replace] using hcap

theorem usableFor_snap_of_ne {w w' : SubWorld} {holder s0 s c capId : Nat}
    {cap0 : Capability} {dst : Option Location}
    (hcap0 : w.base.snap.state.caps capId = some cap0) (hobj0 : cap0.obj = .excSource s0)
    (hsnap : w'.base.snap = replace w.base.snap capId dst) (hne : s ≠ s0)
    (huc : UsableFor w holder s c) : UsableFor w' holder s c := by
  have hc : c ≠ capId := by
    intro hceq
    obtain ⟨cap, hloc, hcap, hact, hright, hobj⟩ := huc
    rw [hceq] at hcap
    rw [hcap0] at hcap
    have hcapEq : cap = cap0 := (Option.some.inj hcap).symm
    subst hcapEq
    rw [hobj0] at hobj
    have hs : s0 = s := by injection hobj
    exact hne hs.symm
  exact usableFor_snap hsnap hc huc

/-- `exc_subscribe` step: preconditions as in §12.4 (held usable capability,
    active caller, no existing subscription); adds the caller's bit. -/
def SubscribeStep (w : SubWorld) (caller capId s : Nat) (w' : SubWorld) : Prop :=
  ∃ h cap src tk, w.base.snap.state.locations capId = some (.heldBy h) ∧
    w.base.snap.state.caps capId = some cap ∧ w.sources s = some src ∧
    w.base.tasks caller = some tk ∧ h = caller ∧ cap.active = true ∧
    cap.rights &&& capExcSubscribe ≠ 0 ∧ cap.obj = .excSource s ∧
    src.active = true ∧ src.subscribers caller = false ∧ tk.active = true ∧
    w' = setSource w s (addSubscriber src caller)

/-- `exc_unsubscribe` step: clears the caller's bit and leaves capabilities
    untouched. -/
def UnsubscribeStep (w : SubWorld) (caller capId s : Nat) (w' : SubWorld) : Prop :=
  ∃ h cap src, w.base.snap.state.locations capId = some (.heldBy h) ∧
    w.base.snap.state.caps capId = some cap ∧ w.sources s = some src ∧
    h = caller ∧ cap.active = true ∧ cap.rights &&& capExcSubscribe ≠ 0 ∧
    cap.obj = .excSource s ∧ src.active = true ∧ src.subscribers caller = true ∧
    w' = setSource w s (clearSubscriber src caller)

/-- Coupled `cap_drop` of the caller's last usable `ExcSubscribe` capability:
    remove the position and terminate the holder's subscription for that source
    in the same step. -/
def DropLastStep (w : SubWorld) (capId holder : Nat) (w' : SubWorld) : Prop :=
  ∃ h cap s, w.base.snap.state.locations capId = some (.heldBy h) ∧ h = holder ∧
    w.base.snap.state.caps capId = some cap ∧ cap.obj = .excSource s ∧
    w' = { w with base := { w.base with snap := replace w.base.snap capId none },
                    sources := (endSubCap w cap holder).sources }

/-- Coupled queued `send` of the sender's last usable `ExcSubscribe` capability:
    move it to `InMessage(msg)` and terminate the sender's subscription. -/
def SendLastStep (w : SubWorld) (capId sender msg : Nat) (w' : SubWorld) : Prop :=
  ∃ cap s message, w.base.snap.state.caps capId = some cap ∧
    w.base.snap.state.locations capId = some (.heldBy sender) ∧
    cap.obj = .excSource s ∧ w.base.messages msg = some message ∧
    message.active = true ∧
    w' = { w with base :=
                    { w.base with
                        snap := replace w.base.snap capId (some (.inMessage msg))
                        messages := (setMessage w.base msg (messageAdd message capId)).messages },
                  sources := (endSubCap w cap sender).sources }

/-- Destroying the exception source removes it and all its subscriptions. -/
def DestroySourceStep (w : SubWorld) (s : Nat) (w' : SubWorld) : Prop :=
  ∃ src, w.sources s = some src ∧
    w' = { w with sources := fun x => if x = s then none else w.sources x }

theorem SubscribeStep_adds {w w' : SubWorld} {caller capId s : Nat}
    (h : SubscribeStep w caller capId s w') :
    ∃ src, w'.sources s = some src ∧ src.subscribers caller = true := by
  obtain ⟨h, cap, src, tk, hloc, hcap, hsrc, htk, hcaller, hactcap, hright, hobj,
    hactsrc, hfresh, hacttask, rfl⟩ := h
  exact ⟨addSubscriber src caller, setSource_self w s (addSubscriber src caller),
    addSubscriber_self src caller⟩

theorem SubscribeStep_preserves {w w' : SubWorld} {caller capId s : Nat}
    (h15 : W15 w) (h : SubscribeStep w caller capId s w') : W15 w' := by
  obtain ⟨h, cap, src, tk, hloc, hcap, hsrc, htk, hcaller, hactcap, hright, hobj,
    hactsrc, hfresh, hacttask, rfl⟩ := h
  intro s' src' hsrc' hact'
  by_cases hse : s' = s
  · rw [hse] at hsrc' ⊢
    rw [setSource_self] at hsrc'
    have hsrcEq : src' = addSubscriber src caller := (Option.some.inj hsrc').symm
    rw [hsrcEq]
    refine ⟨?_, ?_⟩
    · intro t hsub
      by_cases htc : t = caller
      · rw [htc] at hsub ⊢
        exact ⟨tk, htk, hacttask⟩
      · rw [addSubscriber_other src htc] at hsub
        exact w15_active h15 hsrc hactsrc hsub
    · intro t hsub
      by_cases htc : t = caller
      · rw [htc] at hsub ⊢
        refine ⟨capId, cap, ?_, hcap, hactcap, hright, hobj⟩
        rw [← hcaller]
        exact hloc
      · rw [addSubscriber_other src htc] at hsub
        obtain ⟨c, huc⟩ := w15_witness h15 hsrc hactsrc hsub
        exact ⟨c, huc⟩
  · rw [setSource_other w s (addSubscriber src caller) hse] at hsrc'
    exact h15 s' src' hsrc' hact'

theorem UnsubscribeStep_clears {w w' : SubWorld} {caller capId s : Nat}
    (h : UnsubscribeStep w caller capId s w') :
    ∃ src, w'.sources s = some src ∧ src.subscribers caller = false := by
  obtain ⟨h, cap, src, hloc, hcap, hsrc, hcaller, hactcap, hright, hobj,
    hactsrc, hsub, rfl⟩ := h
  exact ⟨clearSubscriber src caller, setSource_self w s (clearSubscriber src caller),
    clearSubscriber_self src caller⟩

theorem UnsubscribeStep_preserves {w w' : SubWorld} {caller capId s : Nat}
    (h15 : W15 w) (h : UnsubscribeStep w caller capId s w') : W15 w' := by
  obtain ⟨h, cap, src, hloc, hcap, hsrc, hcaller, hactcap, hright, hobj,
    hactsrc, hsub, rfl⟩ := h
  intro s' src' hsrc' hact'
  by_cases hse : s' = s
  · rw [hse] at hsrc' ⊢
    rw [setSource_self] at hsrc'
    have hsrcEq : src' = clearSubscriber src caller := (Option.some.inj hsrc').symm
    rw [hsrcEq] at hact' ⊢
    refine ⟨?_, ?_⟩
    · intro t hsub'
      by_cases htc : t = caller
      · rw [htc] at hsub'
        rw [clearSubscriber_self] at hsub'
        simp at hsub'
      · rw [clearSubscriber_other src htc] at hsub'
        exact w15_active h15 hsrc hact' hsub'
    · intro t hsub'
      by_cases htc : t = caller
      · rw [htc] at hsub'
        rw [clearSubscriber_self] at hsub'
        simp at hsub'
      · rw [clearSubscriber_other src htc] at hsub'
        obtain ⟨c, huc⟩ := w15_witness h15 hsrc hact' hsub'
        exact ⟨c, huc⟩
  · rw [setSource_other w s (clearSubscriber src caller) hse] at hsrc'
    exact h15 s' src' hsrc' hact'

theorem DropLastStep_clears {w w' : SubWorld} {capId holder : Nat}
    (h : DropLastStep w capId holder w') :
    ∃ s cap, w.base.snap.state.caps capId = some cap ∧ cap.obj = .excSource s ∧
      (w'.sources s = none ∨ ∃ src, w'.sources s = some src ∧ src.subscribers holder = false) := by
  obtain ⟨h, cap, s, hloc, hholder, hcap, hobj, rfl⟩ := h
  refine ⟨s, cap, hcap, hobj, ?_⟩
  cases hsrc : w.sources s with
  | none => left; simp [endSubCap, hobj, endSub, hsrc]
  | some src => right
                exact ⟨clearSubscriber src holder,
                  by simp [endSubCap, hobj, endSub, hsrc],
                  clearSubscriber_self src holder⟩

theorem DropLastStep_preserves {w w' : SubWorld} {capId holder : Nat}
    (h15 : W15 w) (h : DropLastStep w capId holder w') : W15 w' := by
  obtain ⟨h, cap, s, hloc, hholder, hcap, hobj, rfl⟩ := h
  rw [hholder] at hloc
  intro s' src' hsrc' hact'
  by_cases hse : s' = s
  · rw [hse] at hsrc' ⊢
    simp only [endSubCap, hobj, endSub, ite_true, Option.map_eq_some_iff] at hsrc'
    obtain ⟨src, hsrcOld, hsrcEq0⟩ := hsrc'
    have hsrcEq : src' = clearSubscriber src holder := hsrcEq0.symm
    rw [hsrcEq] at hact'
    rw [clearSubscriber_active] at hact'
    refine ⟨?_, ?_⟩
    · intro t hsub
      rw [hsrcEq] at hsub
      by_cases htc : t = holder
      · rw [htc] at hsub
        rw [clearSubscriber_self] at hsub
        simp at hsub
      · rw [clearSubscriber_other src htc] at hsub
        exact w15_active h15 hsrcOld hact' hsub
    · intro t hsub
      rw [hsrcEq] at hsub
      by_cases htc : t = holder
      · rw [htc] at hsub
        rw [clearSubscriber_self] at hsub
        simp at hsub
      · rw [clearSubscriber_other src htc] at hsub
        obtain ⟨c, ccap, hcloc, hccap, hcact, hcr, hcobj⟩ := w15_witness h15 hsrcOld hact' hsub
        have hcne : c ≠ capId := by
          intro hceq
          rw [hceq] at hcloc
          rw [hloc] at hcloc
          cases hcloc
          exact htc rfl
        exact ⟨c, usableFor_snap rfl hcne ⟨ccap, hcloc, hccap, hcact, hcr, hcobj⟩⟩
  · have hsrcOld : w.sources s' = some src' := by
      simpa [endSubCap, hobj, hse, endSub] using hsrc'
    refine ⟨?_, ?_⟩
    · intro t hsub
      exact w15_active h15 hsrcOld hact' hsub
    · intro t hsub
      obtain ⟨c, huc⟩ := w15_witness h15 hsrcOld hact' hsub
      exact ⟨c, usableFor_snap_of_ne hcap hobj rfl hse huc⟩

theorem SendLastStep_clears {w w' : SubWorld} {capId sender msg : Nat}
    (h : SendLastStep w capId sender msg w') :
    ∃ s cap, w.base.snap.state.caps capId = some cap ∧ cap.obj = .excSource s ∧
      (w'.sources s = none ∨ ∃ src, w'.sources s = some src ∧ src.subscribers sender = false) := by
  obtain ⟨cap, s, message, hcap, hloc, hobj, hm, hactive, rfl⟩ := h
  refine ⟨s, cap, hcap, hobj, ?_⟩
  cases hsrc : w.sources s with
  | none => left; simp [endSubCap, hobj, endSub, hsrc]
  | some src => right
                exact ⟨clearSubscriber src sender,
                  by simp [endSubCap, hobj, endSub, hsrc],
                  clearSubscriber_self src sender⟩

theorem SendLastStep_preserves {w w' : SubWorld} {capId sender msg : Nat}
    (h15 : W15 w) (h : SendLastStep w capId sender msg w') : W15 w' := by
  obtain ⟨cap, s, message, hcap, hloc, hobj, hm, hactive, rfl⟩ := h
  intro s' src' hsrc' hact'
  by_cases hse : s' = s
  · rw [hse] at hsrc' ⊢
    simp only [endSubCap, hobj, endSub, ite_true, Option.map_eq_some_iff] at hsrc'
    obtain ⟨src, hsrcOld, hsrcEq0⟩ := hsrc'
    have hsrcEq : src' = clearSubscriber src sender := hsrcEq0.symm
    rw [hsrcEq] at hact'
    rw [clearSubscriber_active] at hact'
    refine ⟨?_, ?_⟩
    · intro t hsub
      rw [hsrcEq] at hsub
      by_cases htc : t = sender
      · rw [htc] at hsub
        rw [clearSubscriber_self] at hsub
        simp at hsub
      · rw [clearSubscriber_other src htc] at hsub
        exact w15_active h15 hsrcOld hact' hsub
    · intro t hsub
      rw [hsrcEq] at hsub
      by_cases htc : t = sender
      · rw [htc] at hsub
        rw [clearSubscriber_self] at hsub
        simp at hsub
      · rw [clearSubscriber_other src htc] at hsub
        obtain ⟨c, ccap, hcloc, hccap, hcact, hcr, hcobj⟩ := w15_witness h15 hsrcOld hact' hsub
        have hcne : c ≠ capId := by
          intro hceq
          rw [hceq] at hcloc
          rw [hloc] at hcloc
          cases hcloc
          exact htc rfl
        exact ⟨c, usableFor_snap rfl hcne ⟨ccap, hcloc, hccap, hcact, hcr, hcobj⟩⟩
  · have hsrcOld : w.sources s' = some src' := by
      simpa [endSubCap, hobj, hse, endSub] using hsrc'
    refine ⟨?_, ?_⟩
    · intro t hsub
      exact w15_active h15 hsrcOld hact' hsub
    · intro t hsub
      obtain ⟨c, huc⟩ := w15_witness h15 hsrcOld hact' hsub
      exact ⟨c, usableFor_snap_of_ne hcap hobj rfl hse huc⟩

theorem DestroySourceStep_removes {w w' : SubWorld} {s : Nat}
    (h : DestroySourceStep w s w') : w'.sources s = none := by
  obtain ⟨src', hsrc', rfl⟩ := h
  simp

theorem DestroySourceStep_preserves {w w' : SubWorld} {s : Nat}
    (h15 : W15 w) (h : DestroySourceStep w s w') : W15 w' := by
  obtain ⟨src, hsrc, rfl⟩ := h
  intro s' src' hsrc' hact'
  by_cases hse : s' = s
  · subst hse
    simp at hsrc'
  · have hsrcOld : w.sources s' = some src' := by
      simpa [hse] using hsrc'
    exact h15 s' src' hsrcOld hact'

end Formal.Subscription
