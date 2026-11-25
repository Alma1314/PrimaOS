import Formal.Move

/-! W4 container/location consistency (COSIR §5 W4): `HeldBy(t)` requires an
    active task; `InMessage(m)` holds exactly when the active message's capability
    set contains the token; `InNamespace(n,p)` holds exactly when the active
    namespace's entry `p` references the token. The bidirectional readings are
    both obligations: a position implies its container reference, and a container
    reference implies the token's position.

    Messages expose their capabilities as a `Nat → Bool` membership test and
    namespaces their entries as `String → Option Nat`, so "no duplicate capability
    in a message" and "a path references at most one capability" are enforced by
    representation rather than proved over lists. `WorldOk` also carries the
    `Formal.TokenLocation.Consistent` view (W3: at most one position, and every
    located token has a record). The four coupled operations update location and
    container in one step and are proved to preserve `WorldOk`.

    Not covered: message/namespace object lifetime, subscription termination
    (§12.4), audit events, `InTransit`, and Rust refinement. -/
namespace Formal.Container

open Formal
open Formal.TokenLocation
open Formal.Move

structure Task where
  active : Bool

structure Message where
  caps : Nat → Bool
  active : Bool

structure Namespace where
  entries : String → Option Nat
  active : Bool

structure World where
  snap : Snapshot
  tasks : Nat → Option Task
  messages : Nat → Option Message
  namespaces : Nat → Option Namespace

/-- W4 as three bidirectional clauses (task activity, message membership,
    namespace entry reference). -/
def W4Ok (w : World) : Prop :=
  (∀ c t, w.snap.state.locations c = some (.heldBy t) →
    ∃ tk, w.tasks t = some tk ∧ tk.active = true) ∧
  (∀ c m, w.snap.state.locations c = some (.inMessage m) ↔
    ∃ msg, w.messages m = some msg ∧ msg.active = true ∧ msg.caps c = true) ∧
  (∀ c n p, w.snap.state.locations c = some (.inNamespace n p) ↔
    ∃ ns, w.namespaces n = some ns ∧ ns.active = true ∧ ns.entries p = some c)

/-- W3 carrier consistency plus W4 containers. -/
def WorldOk (w : World) : Prop := Consistent w.snap ∧ W4Ok w

def messageAdd (m : Message) (c : Nat) : Message :=
  { m with caps := fun x => x == c || m.caps x }

def messageRemove (m : Message) (c : Nat) : Message :=
  { m with caps := fun x => if x = c then false else m.caps x }

def namespaceSet (ns : Namespace) (path : String) (v : Option Nat) : Namespace :=
  { ns with entries := fun p => if p = path then v else ns.entries p }

def setMessage (w : World) (m : Nat) (msg : Message) : World :=
  { w with messages := fun x => if x = m then some msg else w.messages x }

def setNamespace (w : World) (n : Nat) (ns : Namespace) : World :=
  { w with namespaces := fun x => if x = n then some ns else w.namespaces x }

theorem inNamespace_inj {n n' : Nat} {p p' : String}
    (h : Location.inNamespace n p = Location.inNamespace n' p') : n = n' ∧ p = p' := by
  injection h with h1 h2
  exact ⟨h1, h2⟩

theorem replace_loc_self (s : Snapshot) (c : Nat) (dst : Option Location) :
    (replace s c dst).state.locations c = dst := by
  simp [replace]

theorem replace_loc_other (s : Snapshot) (c : Nat) (dst : Option Location) (d : Nat)
    (h : d ≠ c) : (replace s c dst).state.locations d = s.state.locations d := by
  simp [replace, h]

theorem messageAdd_self (m : Message) (c : Nat) : (messageAdd m c).caps c = true := by
  simp only [messageAdd]
  have h : (c == c) = true := Nat.beq_refl c
  rw [h]
  rfl

theorem messageAdd_other (m : Message) {c d : Nat} (h : d ≠ c) :
    (messageAdd m c).caps d = m.caps d := by
  have hf : (d == c) = false := beq_eq_false_iff_ne.mpr h
  simp [messageAdd, hf]

theorem messageRemove_self (m : Message) (c : Nat) :
    (messageRemove m c).caps c = false := by
  simp [messageRemove]

theorem messageRemove_other (m : Message) {c d : Nat} (h : d ≠ c) :
    (messageRemove m c).caps d = m.caps d := by
  simp [messageRemove, h]

theorem namespaceSet_self (ns : Namespace) (path : String) (v : Option Nat) :
    (namespaceSet ns path v).entries path = v := by
  simp [namespaceSet]

theorem namespaceSet_other (ns : Namespace) (path : String) (v : Option Nat) {p : String}
    (h : p ≠ path) : (namespaceSet ns path v).entries p = ns.entries p := by
  simp [namespaceSet, h]

theorem setMessage_self (w : World) (m : Nat) (msg : Message) :
    (setMessage w m msg).messages m = some msg := by
  simp [setMessage]

theorem setMessage_other (w : World) (m : Nat) (msg : Message) {x : Nat} (h : x ≠ m) :
    (setMessage w m msg).messages x = w.messages x := by
  simp [setMessage, h]

theorem setNamespace_self (w : World) (n : Nat) (ns : Namespace) :
    (setNamespace w n ns).namespaces n = some ns := by
  simp [setNamespace]

theorem setNamespace_other (w : World) (n : Nat) (ns : Namespace) {x : Nat} (h : x ≠ n) :
    (setNamespace w n ns).namespaces x = w.namespaces x := by
  simp [setNamespace, h]

/-- §4.6 queued `send`: `HeldBy(sender) → InMessage(msg)` and `msg` gains the cap. -/
def queuedSend (w : World) (c sender msg : Nat) : Option World :=
  if w.snap.state.locations c = some (.heldBy sender) then
    match w.messages msg with
    | some message =>
        if message.active = true then
          some { w with
            snap := replace w.snap c (some (.inMessage msg))
            messages := (setMessage w msg (messageAdd message c)).messages }
        else none
    | none => none
  else none

/-- §4.6 mailbox `recv`: `InMessage(msg) → HeldBy(receiver)` and `msg` drops the cap. -/
def recvFrom (w : World) (c msg receiver : Nat) : Option World :=
  if w.snap.state.locations c = some (.inMessage msg) then
    match w.tasks receiver with
    | some tk =>
        if tk.active = true then
          match w.messages msg with
          | some message =>
              some { w with
                snap := replace w.snap c (some (.heldBy receiver))
                messages := (setMessage w msg (messageRemove message c)).messages }
          | none => none
        else none
    | none => none
  else none

/-- §4.6 `ns_bind`: `HeldBy(caller) → InNamespace(ns,path)` and the free path
    gains the cap. -/
def bindNs (w : World) (c caller ns : Nat) (path : String) : Option World :=
  if w.snap.state.locations c = some (.heldBy caller) then
    match w.namespaces ns with
    | some nso =>
        if nso.active = true ∧ nso.entries path = none then
          some { w with
            snap := replace w.snap c (some (.inNamespace ns path))
            namespaces := (setNamespace w ns (namespaceSet nso path (some c))).namespaces }
        else none
    | none => none
  else none

/-- §4.6 `ns_unbind`: `InNamespace(ns,path) → HeldBy(caller)` and the path is
    cleared. -/
def unbindNs (w : World) (c caller ns : Nat) (path : String) : Option World :=
  if w.snap.state.locations c = some (.inNamespace ns path) then
    match w.tasks caller with
    | some tk =>
        if tk.active = true then
          match w.namespaces ns with
          | some nso =>
              some { w with
                snap := replace w.snap c (some (.heldBy caller))
                namespaces := (setNamespace w ns (namespaceSet nso path none)).namespaces }
          | none => none
        else none
    | none => none
  else none

theorem queuedSend_preserves {w w' : World} {c sender msg : Nat}
    (hok : WorldOk w) (h : queuedSend w c sender msg = some w') : WorldOk w' := by
  obtain ⟨hcons, hw⟩ := hok
  unfold queuedSend at h
  split at h
  · rename_i hloc
    split at h
    · rename_i message hm
      split at h
      · rename_i hactive
        cases h
        refine ⟨?_, ?_, ?_, ?_⟩
        · exact replace_preserves_consistency w.snap c (some (.inMessage msg)) hcons
            (hcons.2 c (.heldBy sender) hloc)
        · intro d t hd
          by_cases hdc : d = c
          · cases hdc
            rw [replace_loc_self] at hd
            simp at hd
          · rw [replace_loc_other _ _ _ _ hdc] at hd
            exact hw.1 d t hd
        · intro d m
          constructor
          · intro hd
            by_cases hdm : m = msg
            · cases hdm
              by_cases hdc : d = c
              · cases hdc
                refine ⟨messageAdd message c, ?_, hactive, messageAdd_self message c⟩
                exact setMessage_self w msg (messageAdd message c)
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                obtain ⟨m0, hm0, ha, hcaps⟩ := (hw.2.1 d msg).mp hd
                have hmEq : m0 = message := by
                  rw [hm] at hm0
                  exact (Option.some.inj hm0).symm
                cases hmEq
                refine ⟨messageAdd message c, ?_, hactive, ?_⟩
                · exact setMessage_self w msg (messageAdd message c)
                · rw [messageAdd_other message hdc]
                  exact hcaps
            · by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self] at hd
                have hmeq : msg = m := by
                  simpa using hd
                exact absurd hmeq.symm hdm
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                obtain ⟨m0, hm0, ha, hcaps⟩ := (hw.2.1 d m).mp hd
                refine ⟨m0, ?_, ha, hcaps⟩
                rw [setMessage_other w msg (messageAdd message c) hdm]
                exact hm0
          · intro hright
            obtain ⟨m0, hm0, ha, hcaps⟩ := hright
            by_cases hdm : m = msg
            · cases hdm
              by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self]
              · rw [replace_loc_other _ _ _ _ hdc]
                rw [setMessage_self] at hm0
                have hmEq : m0 = messageAdd message c := (Option.some.inj hm0).symm
                cases hmEq
                rw [messageAdd_other message hdc] at hcaps
                exact (hw.2.1 d msg).mpr ⟨message, hm, hactive, hcaps⟩
            · rw [setMessage_other w msg (messageAdd message c) hdm] at hm0
              by_cases hdc : d = c
              · cases hdc
                have hold := (hw.2.1 c m).mpr ⟨m0, hm0, ha, hcaps⟩
                rw [hloc] at hold
                simp at hold
              · rw [replace_loc_other _ _ _ _ hdc]
                exact (hw.2.1 d m).mpr ⟨m0, hm0, ha, hcaps⟩
        · intro d n p
          constructor
          · intro hd
            by_cases hdc : d = c
            · cases hdc
              rw [replace_loc_self] at hd
              simp at hd
            · rw [replace_loc_other _ _ _ _ hdc] at hd
              exact (hw.2.2 d n p).mp hd
          · intro hright
            by_cases hdc : d = c
            · cases hdc
              have hold := (hw.2.2 c n p).mpr hright
              rw [hloc] at hold
              simp at hold
            · rw [replace_loc_other _ _ _ _ hdc]
              exact (hw.2.2 d n p).mpr hright
      · simp at h
    · simp at h
  · simp at h

theorem recvFrom_preserves {w w' : World} {c msg receiver : Nat}
    (hok : WorldOk w) (h : recvFrom w c msg receiver = some w') : WorldOk w' := by
  obtain ⟨hcons, hw⟩ := hok
  unfold recvFrom at h
  split at h
  · rename_i hloc
    split at h
    · rename_i tk htk
      split at h
      · rename_i hactive
        split at h
        · rename_i message hm
          cases h
          have hmsgActive : message.active = true := by
            obtain ⟨m0, hm0, ha0, _⟩ := (hw.2.1 c msg).mp hloc
            rw [hm] at hm0
            exact (Option.some.inj hm0).symm ▸ ha0
          refine ⟨?_, ?_, ?_, ?_⟩
          · exact replace_preserves_consistency w.snap c (some (.heldBy receiver)) hcons
              (hcons.2 c (.inMessage msg) hloc)
          · intro d t hd
            by_cases hdc : d = c
            · cases hdc
              rw [replace_loc_self] at hd
              cases hd
              exact ⟨tk, htk, hactive⟩
            · rw [replace_loc_other _ _ _ _ hdc] at hd
              exact hw.1 d t hd
          · intro d m
            constructor
            · intro hd
              by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self] at hd
                simp at hd
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                obtain ⟨m0, hm0, ha, hcaps⟩ := (hw.2.1 d m).mp hd
                by_cases hdm : m = msg
                · cases hdm
                  have hmEq : m0 = message := by
                    rw [hm] at hm0
                    exact (Option.some.inj hm0).symm
                  cases hmEq
                  refine ⟨messageRemove message c, ?_, ha, ?_⟩
                  · exact setMessage_self w msg (messageRemove message c)
                  · rw [messageRemove_other message hdc]
                    exact hcaps
                · refine ⟨m0, ?_, ha, hcaps⟩
                  rw [setMessage_other w msg (messageRemove message c) hdm]
                  exact hm0
            · intro hright
              obtain ⟨m0, hm0, ha, hcaps⟩ := hright
              by_cases hdm : m = msg
              · cases hdm
                rw [setMessage_self] at hm0
                have hmEq : m0 = messageRemove message c := (Option.some.inj hm0).symm
                cases hmEq
                by_cases hdc : d = c
                · cases hdc
                  rw [messageRemove_self] at hcaps
                  simp at hcaps
                · rw [replace_loc_other _ _ _ _ hdc]
                  rw [messageRemove_other message hdc] at hcaps
                  exact (hw.2.1 d msg).mpr ⟨message, hm, hmsgActive, hcaps⟩
              · rw [setMessage_other w msg (messageRemove message c) hdm] at hm0
                by_cases hdc : d = c
                · cases hdc
                  have hold := (hw.2.1 c m).mpr ⟨m0, hm0, ha, hcaps⟩
                  rw [hloc] at hold
                  have hmeq : msg = m := by simpa using hold
                  exact absurd hmeq.symm hdm
                · rw [replace_loc_other _ _ _ _ hdc]
                  exact (hw.2.1 d m).mpr ⟨m0, hm0, ha, hcaps⟩
          · intro d n p
            constructor
            · intro hd
              by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self] at hd
                simp at hd
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                exact (hw.2.2 d n p).mp hd
            · intro hright
              by_cases hdc : d = c
              · cases hdc
                have hold := (hw.2.2 c n p).mpr hright
                rw [hloc] at hold
                simp at hold
              · rw [replace_loc_other _ _ _ _ hdc]
                exact (hw.2.2 d n p).mpr hright
        · simp at h
      · simp at h
    · simp at h
  · simp at h

theorem bindNs_preserves {w w' : World} {c caller ns : Nat} {path : String}
    (hok : WorldOk w) (h : bindNs w c caller ns path = some w') : WorldOk w' := by
  obtain ⟨hcons, hw⟩ := hok
  unfold bindNs at h
  split at h
  · rename_i hloc
    split at h
    · rename_i nso hm
      split at h
      · rename_i hguard
        have hactive : nso.active = true := hguard.1
        have hfree : nso.entries path = none := hguard.2
        cases h
        refine ⟨?_, ?_, ?_, ?_⟩
        · exact replace_preserves_consistency w.snap c (some (.inNamespace ns path)) hcons
            (hcons.2 c (.heldBy caller) hloc)
        · intro d t hd
          by_cases hdc : d = c
          · cases hdc
            rw [replace_loc_self] at hd
            simp at hd
          · rw [replace_loc_other _ _ _ _ hdc] at hd
            exact hw.1 d t hd
        · intro d m
          constructor
          · intro hd
            by_cases hdc : d = c
            · cases hdc
              rw [replace_loc_self] at hd
              simp at hd
            · rw [replace_loc_other _ _ _ _ hdc] at hd
              exact (hw.2.1 d m).mp hd
          · intro hright
            by_cases hdc : d = c
            · cases hdc
              have hold := (hw.2.1 c m).mpr hright
              rw [hloc] at hold
              simp at hold
            · rw [replace_loc_other _ _ _ _ hdc]
              exact (hw.2.1 d m).mpr hright
        · intro d n p
          constructor
          · intro hd
            by_cases hdc : d = c
            · cases hdc
              rw [replace_loc_self] at hd
              cases hd
              refine ⟨namespaceSet nso path (some c), ?_, hactive, ?_⟩
              · exact setNamespace_self w ns (namespaceSet nso path (some c))
              · exact namespaceSet_self nso path (some c)
            · rw [replace_loc_other _ _ _ _ hdc] at hd
              obtain ⟨ns0, hns0, ha, hentry⟩ := (hw.2.2 d n p).mp hd
              by_cases hnsEq : n = ns
              · cases hnsEq
                have hnsEq2 : ns0 = nso := by
                  rw [hm] at hns0
                  exact (Option.some.inj hns0).symm
                cases hnsEq2
                by_cases hpn : p = path
                · cases hpn
                  rw [hfree] at hentry
                  simp at hentry
                · refine ⟨namespaceSet nso path (some c), ?_, ha, ?_⟩
                  · exact setNamespace_self w ns (namespaceSet nso path (some c))
                  · rw [namespaceSet_other nso path (some c) hpn]
                    exact hentry
              · refine ⟨ns0, ?_, ha, hentry⟩
                rw [setNamespace_other w ns (namespaceSet nso path (some c)) hnsEq]
                exact hns0
          · intro hright
            obtain ⟨ns0, hns0, ha, hentry⟩ := hright
            by_cases hnsEq : n = ns
            · cases hnsEq
              rw [setNamespace_self] at hns0
              have hnsEq2 : ns0 = namespaceSet nso path (some c) := (Option.some.inj hns0).symm
              cases hnsEq2
              by_cases hpn : p = path
              · cases hpn
                rw [namespaceSet_self] at hentry
                have hdc' : d = c := (Option.some.inj hentry).symm
                cases hdc'
                rw [replace_loc_self]
              · rw [namespaceSet_other nso path (some c) hpn] at hentry
                have hold := (hw.2.2 d ns p).mpr ⟨nso, hm, hactive, hentry⟩
                by_cases hdc : d = c
                · cases hdc
                  rw [hloc] at hold
                  simp at hold
                · rw [replace_loc_other _ _ _ _ hdc]
                  exact hold
            · rw [setNamespace_other w ns (namespaceSet nso path (some c)) hnsEq] at hns0
              have hold := (hw.2.2 d n p).mpr ⟨ns0, hns0, ha, hentry⟩
              by_cases hdc : d = c
              · cases hdc
                rw [hloc] at hold
                simp at hold
              · rw [replace_loc_other _ _ _ _ hdc]
                exact hold
      · simp at h
    · simp at h
  · simp at h

theorem unbindNs_preserves {w w' : World} {c caller ns : Nat} {path : String}
    (hok : WorldOk w) (h : unbindNs w c caller ns path = some w') : WorldOk w' := by
  obtain ⟨hcons, hw⟩ := hok
  unfold unbindNs at h
  split at h
  · rename_i hloc
    split at h
    · rename_i tk htk
      split at h
      · rename_i hactive
        split at h
        · rename_i nso hm
          cases h
          refine ⟨?_, ?_, ?_, ?_⟩
          · exact replace_preserves_consistency w.snap c (some (.heldBy caller)) hcons
              (hcons.2 c (.inNamespace ns path) hloc)
          · intro d t hd
            by_cases hdc : d = c
            · cases hdc
              rw [replace_loc_self] at hd
              cases hd
              exact ⟨tk, htk, hactive⟩
            · rw [replace_loc_other _ _ _ _ hdc] at hd
              exact hw.1 d t hd
          · intro d m
            constructor
            · intro hd
              by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self] at hd
                simp at hd
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                exact (hw.2.1 d m).mp hd
            · intro hright
              by_cases hdc : d = c
              · cases hdc
                have hold := (hw.2.1 c m).mpr hright
                rw [hloc] at hold
                simp at hold
              · rw [replace_loc_other _ _ _ _ hdc]
                exact (hw.2.1 d m).mpr hright
          · intro d n p
            constructor
            · intro hd
              by_cases hdc : d = c
              · cases hdc
                rw [replace_loc_self] at hd
                simp at hd
              · rw [replace_loc_other _ _ _ _ hdc] at hd
                obtain ⟨ns0, hns0, ha, hentry⟩ := (hw.2.2 d n p).mp hd
                by_cases hnsEq : n = ns
                · cases hnsEq
                  have hnsEq2 : ns0 = nso := by
                    rw [hm] at hns0
                    exact (Option.some.inj hns0).symm
                  cases hnsEq2
                  by_cases hpn : p = path
                  · cases hpn
                    have hcEntry := (hw.2.2 c ns path).mp hloc
                    obtain ⟨nsC, hnsC, _, hentryC⟩ := hcEntry
                    have hnsCeq : nsC = nso := by
                      rw [hm] at hnsC
                      exact (Option.some.inj hnsC).symm
                    rw [hnsCeq] at hentryC
                    rw [hentryC] at hentry
                    have hdc' : d = c := (Option.some.inj hentry).symm
                    exact absurd hdc' hdc
                  · refine ⟨namespaceSet nso path none, ?_, ha, ?_⟩
                    · exact setNamespace_self w ns (namespaceSet nso path none)
                    · rw [namespaceSet_other nso path none hpn]
                      exact hentry
                · refine ⟨ns0, ?_, ha, hentry⟩
                  rw [setNamespace_other w ns (namespaceSet nso path none) hnsEq]
                  exact hns0
            · intro hright
              obtain ⟨ns0, hns0, ha, hentry⟩ := hright
              by_cases hnsEq : n = ns
              · cases hnsEq
                rw [setNamespace_self] at hns0
                have hnsEq2 : ns0 = namespaceSet nso path none := (Option.some.inj hns0).symm
                cases hnsEq2
                by_cases hpn : p = path
                · cases hpn
                  rw [namespaceSet_self] at hentry
                  simp at hentry
                · rw [namespaceSet_other nso path none hpn] at hentry
                  have hold := (hw.2.2 d ns p).mpr ⟨nso, hm, ha, hentry⟩
                  by_cases hdc : d = c
                  · rw [hdc] at hold
                    rw [hloc] at hold
                    have heq : Location.inNamespace ns path = Location.inNamespace ns p := Option.some.inj hold
                    exact absurd (inNamespace_inj heq).2.symm hpn
                  · rw [replace_loc_other _ _ _ _ hdc]
                    exact hold
              · rw [setNamespace_other w ns (namespaceSet nso path none) hnsEq] at hns0
                have hold := (hw.2.2 d n p).mpr ⟨ns0, hns0, ha, hentry⟩
                by_cases hdc : d = c
                · cases hdc
                  rw [hloc] at hold
                  simp at hold
                  exact absurd hold.1.symm hnsEq
                · rw [replace_loc_other _ _ _ _ hdc]
                  exact hold
        · simp at h
      · simp at h
    · simp at h
  · simp at h

def emptyWorld : World :=
  { snap :=
      { state := { caps := fun _ => none, booted := false }
        carriers := fun _ _ => false
        structural := fun _ => false }
    tasks := fun _ => none
    messages := fun _ => none
    namespaces := fun _ => none }

theorem emptyWorld_ok : WorldOk emptyWorld := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · constructor
    · intro c loc
      simp [emptyWorld]
    · intro c loc h
      simp [emptyWorld] at h
  · intro c t h
    simp [emptyWorld] at h
  · intro c m
    simp [emptyWorld]
  · intro c n p
    simp [emptyWorld]

-- Executable anchors: task 9 active, message 5 active and empty, namespace 1
-- active and empty; cap 1 is held by task 9, cap 2 sits in message 5.

def cTask : Task := { active := true }

def cMsg : Message := { caps := fun x => x == 2, active := true }

def cNs : Namespace := { entries := fun _ => none, active := true }

def cWorld : World :=
  { snap :=
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
    tasks := fun id => if id = 9 then some cTask else none
    messages := fun id => if id = 5 then some cMsg else none
    namespaces := fun id => if id = 1 then some cNs else none }

#eval (queuedSend cWorld 1 9 5).isSome        -- true
#eval (queuedSend cWorld 1 8 5).isSome        -- false（源不匹配）
#eval (queuedSend cWorld 1 9 6).isSome        -- false（无此消息）
#eval ((queuedSend cWorld 1 9 5).map (fun w => w.snap.state.locations 1))
  -- some (some (inMessage 5))
#eval ((queuedSend cWorld 1 9 5).map (fun w => (w.messages 5).map (fun m => m.caps 1)))
  -- some (some true)：消息容器同步
#eval ((recvFrom cWorld 2 5 7)).isSome        -- false（任务 7 不活跃）
#eval ((recvFrom cWorld 2 5 9).map (fun w => w.snap.state.locations 2))
  -- some (some (heldBy 9))
#eval ((recvFrom cWorld 2 5 9).map (fun w => (w.messages 5).map (fun m => m.caps 2)))
  -- some (some false)：消息容器移除
#eval ((bindNs cWorld 1 9 1 "dev").map (fun w => w.snap.state.locations 1))
  -- some (some (inNamespace 1 "dev"))
#eval ((bindNs cWorld 1 9 1 "dev").map (fun w => (w.namespaces 1).map (fun ns => ns.entries "dev")))
  -- some (some (some 1))：命名空间条目同步
#eval ((bindNs cWorld 1 9 9 "dev")).isSome    -- false（无此命名空间）
#eval ((unbindNs cWorld 2 9 1 "dev")).isSome  -- false（位置不匹配）
#eval ((queuedSend cWorld 1 9 5).map (fun w => (w.tasks 9).map (fun t => t.active)))
  -- some (some true)：Task 容器不变
#eval ((bindNs cWorld 1 9 1 "dev").bind (fun w => unbindNs w 1 9 1 "dev")).map
  (fun w => w.snap.state.locations 1)
  -- some (some (heldBy 9))：bind 后 unbind 回到调用者
#eval ((bindNs cWorld 1 9 1 "dev").map (fun w => (w.namespaces 1).map (fun ns => ns.entries "dev")))
  -- some (some (some 1))

end Formal.Container
