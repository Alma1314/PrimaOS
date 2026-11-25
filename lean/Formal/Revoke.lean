import Formal.State

/-! Selective-revocation reference model for the bootstrap-host implementation
    described in Prima OS §6.2 (and COSIR §4.7): `revoke(root, holder)` makes the
    root and its whole `parent`-descendant subtree inactive and removes their
    `HeldBy` locations, while leaving `InMessage`/`InNamespace`/`InTransit`
    positions outside the revocation domain. The operation is atomic and
    idempotent; capability records and parent pointers are preserved.

    This is now an **executable** model: the subtree test is a fuel-bounded
    `Bool` walk (`descB`, fuel `c + 1`), not a classical decision over an
    inductive predicate. Under `Formal.WellFormed` the parent CapId strictly
    decreases along the chain, so fuel `c + 1` is sufficient; `descB` is proved
    sound unconditionally and complete w.r.t. `Desc` under `WellFormed`.

    The original `Formal.WellFormed` requires every parent (including a parent of
    an inactive child) to be active. After revocation both the parent and child are
    inactive, so that predicate is no longer the right shape for the subtree; the
    model instead proves preservation of `ActiveWellFormed`, which constrains only
    active capabilities. This is an explicit scope change, not a relaxation of the
    pre-revocation CDT invariant. See README.md for the Rust mapping and limits. -/
namespace Formal.Revoke

open Formal

/-- `c` is the root or reachable from it by following `parent` pointers. -/
inductive Desc (s : State) (root : Nat) : Nat → Prop where
  | self : Desc s root root
  | step {c p : Nat} {cap : Capability} :
      s.caps c = some cap → cap.parent = some p → Desc s root p → Desc s root c

/-- A state whose *active* capabilities keep the CDT parent invariants: the parent
    exists, is active, has a smaller CapId, and points at the same object. -/
def ActiveWellFormed (s : State) : Prop :=
  ∀ c cap, s.caps c = some cap → cap.active = true → ∀ p, cap.parent = some p →
    p < c ∧ ∃ pcap, s.caps p = some pcap ∧ pcap.active = true ∧ pcap.obj = cap.obj

/-- Executable fuel-bounded membership test: `c` is the root, or its parent chain
    reaches the root within `n` steps. Sound for every fuel; complete for fuel
    `c + 1` on a `WellFormed` state (CapIds strictly decrease upwards). -/
def descB : Nat → State → Nat → Nat → Bool
  | 0, _, _, _ => false
  | n + 1, s, root, c =>
      c == root ||
        (match s.caps c with
         | some cap =>
             match cap.parent with
             | some p => descB n s root p
             | none => false
         | none => false)

/-- The subtree membership bit used by `revoked`; `c + 1` steps suffice because a
    `WellFormed` chain from `c` decreases strictly and ends at `root ≤ c`. -/
def descAt (s : State) (root c : Nat) : Bool := descB (c + 1) s root c

/-- The root is always a member of its own subtree, for any state. Proved without
    classical decidability (the root disjunct is decided by `Nat.beq_refl`). -/
theorem descB_self (s : State) (root : Nat) : descB (root + 1) s root root = true := by
  simp only [descB]
  have h : (root == root) = true := Nat.beq_refl root
  rw [h]
  rfl

theorem descAt_self (s : State) (root : Nat) : descAt s root root = true :=
  descB_self s root

/-- `descB` is monotone in its fuel. -/
theorem descB_succ : ∀ (n : Nat) (s : State) (root c : Nat),
    descB n s root c = true → descB (n + 1) s root c = true := by
  intro n
  induction n with
  | zero =>
      intro s root c h
      simp [descB] at h
  | succ k ih =>
      intro s root c h
      by_cases hc : c == root
      · simp [descB, hc]
      · cases hcap : s.caps c with
        | none => simp [descB, hc, hcap] at h
        | some cap =>
            cases hp : cap.parent with
            | none => simp [descB, hc, hcap, hp] at h
            | some p =>
                have h' : descB k s root p = true := by
                  simpa [descB, hc, hcap, hp] using h
                simpa [descB, hc, hcap, hp] using ih s root p h'

theorem descB_mono {s : State} {root c : Nat} :
    ∀ {n m : Nat}, n ≤ m → descB n s root c = true → descB m s root c = true := by
  intro n m
  induction m with
  | zero =>
      intro hle h
      have hn : n = 0 := Nat.eq_zero_of_le_zero hle
      subst hn
      exact h
  | succ m ih =>
      intro hle h
      rcases Nat.eq_or_lt_of_le hle with heq | hlt
      · subst heq
        exact h
      · exact descB_succ m s root c (ih (Nat.lt_succ_iff.mp hlt) h)

/-- `descB` never reports a non-descendant. -/
theorem descB_sound :
    ∀ (n : Nat) (s : State) (root c : Nat), descB n s root c = true → Desc s root c := by
  intro n
  induction n with
  | zero => intro s root c h; simp [descB] at h
  | succ k ih =>
      intro s root c h
      by_cases hc : c == root
      · have hcr : c = root := by simpa using hc
        rw [hcr]
        exact Desc.self
      · cases hcap : s.caps c with
        | none => simp [descB, hc, hcap] at h
        | some cap =>
            cases hp : cap.parent with
            | none => simp [descB, hc, hcap, hp] at h
            | some p =>
                have h' : descB k s root p = true := by
                  simpa [descB, hc, hcap, hp] using h
                exact Desc.step hcap hp (ih s root p h')

/-- In a well-formed state, a descendant's CapId is at least the root's. -/
theorem desc_ge_root {s : State} {root c : Nat} (hwf : WellFormed s) (h : Desc s root c) :
    root ≤ c := by
  induction h with
  | self => exact Nat.le_refl root
  | @step c p cap hcap hpar _ ih =>
      exact Nat.le_trans ih (Nat.le_of_lt (hwf c cap hcap p hpar).1)

/-- `descB` with fuel `c + 1` recognises every descendant of a well-formed state. -/
theorem descB_complete {s : State} {root c : Nat} (hwf : WellFormed s) (h : Desc s root c) :
    descB (c + 1) s root c = true := by
  induction h with
  | self => exact descB_self s root
  | @step c p cap hcap hpar hd ih =>
      have hlt : p < c := (hwf c cap hcap p hpar).1
      have hrootlt : root < c := Nat.lt_of_le_of_lt (desc_ge_root hwf hd) hlt
      have hcf : (c == root) = false := by simp [Nat.ne_of_gt hrootlt]
      have hf : descB c s root p = true := descB_mono (Nat.succ_le_of_lt hlt) ih
      simp [descB, hcf, hcap, hpar, hf]

theorem descAt_sound {s : State} {root c : Nat} (h : descAt s root c = true) :
    Desc s root c :=
  descB_sound (c + 1) s root c h

theorem descAt_complete {s : State} {root c : Nat} (hwf : WellFormed s) (h : Desc s root c) :
    descAt s root c = true :=
  descB_complete hwf h

/-- Revoked state: every subtree capability is inactive; every subtree `HeldBy`
    position is removed; non-`HeldBy` positions and all out-of-subtree state are
    untouched. -/
def revoked (s : State) (root _holder : Nat) : State :=
  { s with
    caps := fun c => (s.caps c).map (fun cap =>
      if descAt s root c then { cap with active := false } else cap),
    locations := fun c =>
      if descAt s root c then
        match s.locations c with
        | some (.heldBy _) => none
        | other => other
      else s.locations c }

/-- The guarded operation. It only accepts a root currently held by `holder`;
    otherwise the state is returned unchanged (the host's `HolderMismatch`). -/
def revoke (s : State) (root holder : Nat) : State × Bool :=
  if s.locations root = some (.heldBy holder) then
    (revoked s root holder, true)
  else (s, false)

theorem wellFormed_active {s : State} (h : WellFormed s) : ActiveWellFormed s := by
  intro c cap hc _ p hp
  exact h c cap hc p hp

/-- Descendants are transitive: if `p` is under `root`, everything under `p` is
    under `root`. -/
theorem desc_trans {s : State} {root p c : Nat}
    (h1 : Desc s root p) (h2 : Desc s p c) : Desc s root c := by
  induction h2 with
  | self => exact h1
  | step hcap hpar _ ih => exact Desc.step hcap hpar ih

/-- If a capability is outside the subtree, its parent is outside too. -/
theorem not_desc_parent {s : State} {root c p : Nat} {cap : Capability}
    (hc : s.caps c = some cap) (hp : cap.parent = some p) (h : ¬ Desc s root c) :
    ¬ Desc s root p := by
  intro hd
  exact h (Desc.step hc hp hd)

theorem revoked_caps_desc {s : State} {root holder c : Nat} {cap : Capability}
    (hwf : WellFormed s) (hd : Desc s root c) (hc : s.caps c = some cap) :
    (revoked s root holder).caps c = some { cap with active := false } := by
  have hb : descAt s root c = true := descAt_complete hwf hd
  simp [revoked, hb, hc]

theorem revoked_caps_outside {s : State} {root holder c : Nat}
    (h : ¬ Desc s root c) :
    (revoked s root holder).caps c = s.caps c := by
  have hb : descAt s root c = false := by
    rw [Bool.eq_false_iff]
    intro ht
    exact h (descAt_sound ht)
  cases hc : s.caps c <;> simp [revoked, hb, hc]

theorem revoked_loc_desc_held {s : State} {root holder c : Nat} {task : Nat}
    (hwf : WellFormed s) (hd : Desc s root c) (hl : s.locations c = some (.heldBy task)) :
    (revoked s root holder).locations c = none := by
  have hb : descAt s root c = true := descAt_complete hwf hd
  simp [revoked, hb, hl]

theorem revoked_loc_desc_message {s : State} {root holder c m : Nat}
    (hwf : WellFormed s) (hd : Desc s root c) (hl : s.locations c = some (.inMessage m)) :
    (revoked s root holder).locations c = some (.inMessage m) := by
  have hb : descAt s root c = true := descAt_complete hwf hd
  simp [revoked, hb, hl]

theorem revoked_loc_desc_namespace {s : State} {root holder c ns : Nat} {path : String}
    (hwf : WellFormed s) (hd : Desc s root c)
    (hl : s.locations c = some (.inNamespace ns path)) :
    (revoked s root holder).locations c = some (.inNamespace ns path) := by
  have hb : descAt s root c = true := descAt_complete hwf hd
  simp [revoked, hb, hl]

theorem revoked_loc_outside {s : State} {root holder c : Nat}
    (h : ¬ Desc s root c) :
    (revoked s root holder).locations c = s.locations c := by
  have hb : descAt s root c = false := by
    rw [Bool.eq_false_iff]
    intro ht
    exact h (descAt_sound ht)
  simp [revoked, hb]

/-- Records and parent pointers are preserved; only `active` may change. -/
theorem revoked_parent {s : State} {root holder c : Nat} {cap : Capability}
    (hc : s.caps c = some cap) :
    ∃ cap', (revoked s root holder).caps c = some cap' ∧ cap'.parent = cap.parent := by
  by_cases hb : descAt s root c = true
  · exact ⟨{ cap with active := false }, by simp [revoked, hb, hc], rfl⟩
  · exact ⟨cap, by simp [revoked, Bool.eq_false_iff.mpr hb, hc], rfl⟩

theorem revoke_fail {s : State} {root holder : Nat}
    (h : s.locations root ≠ some (.heldBy holder)) :
    revoke s root holder = (s, false) := by
  simp [revoke, h]

theorem revoke_success {s : State} {root holder : Nat}
    (h : s.locations root = some (.heldBy holder)) :
    revoke s root holder = (revoked s root holder, true) := by
  simp [revoke, h]

/-- The root is its own subtree member, so a successful revoke removes its
    `HeldBy` position. This does not need `WellFormed`. -/
theorem revoked_loc_root {s : State} {root holder : Nat}
    (h : s.locations root = some (.heldBy holder)) :
    (revoked s root holder).locations root = none := by
  have hb : descAt s root root = true := descAt_self s root
  simp [revoked, hb, h]

/-- After a successful revoke the root is no longer held, so a second revoke
    fails and leaves the state unchanged. -/
theorem revoke_idempotent {s : State} {root holder : Nat}
    (h : s.locations root = some (.heldBy holder)) :
    revoke (revoke s root holder).1 root holder = ((revoke s root holder).1, false) := by
  rw [revoke_success h]
  have hroot : (revoked s root holder).locations root = none := revoked_loc_root h
  have hne : (revoked s root holder).locations root ≠ some (.heldBy holder) := by
    rw [hroot]; simp
  rw [revoke_fail hne]

theorem revoked_caps {s : State} {root holder c : Nat} :
    (revoked s root holder).caps c =
      (s.caps c).map (fun cap => if descAt s root c then { cap with active := false } else cap) :=
  rfl

/-- Successful revocation keeps the active-capability CDT invariants: an active
    capability is necessarily outside the subtree, so neither it nor its parent
    was modified (a parent in the subtree would put the child in the subtree). -/
theorem revoke_preserves_active_wf {s : State} {root holder : Nat}
    (hwf : WellFormed s) : ActiveWellFormed (revoked s root holder) := by
  intro c cap' hcap' hactive p hparent
  rw [revoked_caps] at hcap'
  cases hc : s.caps c with
  | none => rw [hc] at hcap'; simp at hcap'
  | some cap =>
      rw [hc] at hcap'
      simp at hcap'
      by_cases hb : descAt s root c = true
      · simp [hb] at hcap'
        rw [← hcap'] at hactive
        simp at hactive
      · have hb' : descAt s root c = false := Bool.eq_false_iff.mpr hb
        simp [hb'] at hcap'
        rw [← hcap'] at hparent hactive ⊢
        have hnotc : ¬ Desc s root c := fun hd => hb (descAt_complete hwf hd)
        have hnotp : ¬ Desc s root p := not_desc_parent hc hparent hnotc
        have hpcap := hwf c cap hc p hparent
        refine ⟨hpcap.1, ?_⟩
        obtain ⟨pcap, hpdef, hpactive, hpobj⟩ := hpcap.2
        exact ⟨pcap, by rw [revoked_caps_outside hnotp]; exact hpdef, hpactive, hpobj⟩

-- Executable anchors: root (id 1, held by 9) with derived child (id 2, held by 9),
-- an in-message descendant (id 3), and an out-of-subtree cap (id 4).

def revRoot : Capability :=
  { owner := 9, obj := .segment 7, rights := 0, active := true, parent := none }

def revChild : Capability :=
  { owner := 9, obj := .segment 7, rights := 0, active := true, parent := some 1 }

def revMsg : Capability :=
  { owner := 8, obj := .segment 7, rights := 0, active := true, parent := some 2 }

def revOutside : Capability :=
  { owner := 8, obj := .node 3, rights := 0, active := true, parent := none }

def revState : State :=
  { caps := fun id =>
      if id = 1 then some revRoot
      else if id = 2 then some revChild
      else if id = 3 then some revMsg
      else if id = 4 then some revOutside
      else none
    booted := true
    locations := fun id =>
      if id = 1 then some (.heldBy 9)
      else if id = 2 then some (.heldBy 9)
      else if id = 3 then some (.inMessage 5)
      else if id = 4 then some (.heldBy 8)
      else none }

#eval (revoke revState 1 9).2                                   -- true
#eval ((revoke revState 1 9).1.caps 1).map (fun c => c.active)   -- some false（根失效）
#eval ((revoke revState 1 9).1.caps 2).map (fun c => c.active)   -- some false（派生子项）
#eval ((revoke revState 1 9).1.locations 1)                      -- none（删 HeldBy）
#eval ((revoke revState 1 9).1.locations 2)                      -- none
#eval ((revoke revState 1 9).1.locations 3)                      -- some (inMessage 5)：非 HeldBy 保留
#eval ((revoke revState 1 9).1.caps 4).map (fun c => c.active)   -- some true：域外不变
#eval ((revoke revState 1 9).1.locations 4)                      -- some (heldBy 8)
#eval (revoke revState 1 8).2                                    -- false（HolderMismatch）
#eval ((revoke revState 1 8).1.locations 1)                      -- some (heldBy 9)：失败不改状态
#eval (revoke (revoke revState 1 9).1 1 9).2                     -- false（二次撤销失败）

end Formal.Revoke
