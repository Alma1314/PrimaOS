import Formal.State

/-! Selective-revocation reference model for the bootstrap-host implementation
    described in Prima OS §6.2 (and COSIR §4.7): `revoke(root, holder)` makes the
    root and its whole `parent`-descendant subtree inactive and removes their
    `HeldBy` locations, while leaving `InMessage`/`InNamespace`/`InTransit`
    positions outside the revocation domain. The operation is atomic and
    idempotent; capability records and parent pointers are preserved.

    The original `Formal.WellFormed` requires every parent (including a parent of
    an inactive child) to be active. After revocation both the parent and child are
    inactive, so that predicate is no longer the right shape for the subtree; the
    model instead proves preservation of `ActiveWellFormed`, which constrains only
    active capabilities. This is an explicit scope change, not a relaxation of the
    pre-revocation CDT invariant. See README.md for the Rust mapping and limits. -/
namespace Formal.Revoke

open Classical
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

/-- Revoked state: every subtree capability is inactive; every subtree `HeldBy`
    position is removed; non-`HeldBy` positions and all out-of-subtree state are
    untouched. -/
noncomputable def revoked (s : State) (root _holder : Nat) : State :=
  { s with
    caps := fun c => (s.caps c).map (fun cap =>
      if Desc s root c then { cap with active := false } else cap),
    locations := fun c =>
      if Desc s root c then
        match s.locations c with
        | some (.heldBy _) => none
        | other => other
      else s.locations c }

/-- The guarded operation. It only accepts a root currently held by `holder`;
    otherwise the state is returned unchanged (the host's `HolderMismatch`). -/
noncomputable def revoke (s : State) (root holder : Nat) : State × Bool :=
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
    (hd : Desc s root c) (hc : s.caps c = some cap) :
    (revoked s root holder).caps c = some { cap with active := false } := by
  simp [revoked, hc, hd]

theorem revoked_caps_outside {s : State} {root holder c : Nat}
    (h : ¬ Desc s root c) :
    (revoked s root holder).caps c = s.caps c := by
  cases hc : s.caps c <;> simp [revoked, hc, h]

theorem revoked_loc_desc_held {s : State} {root holder c : Nat} {task : Nat}
    (hd : Desc s root c) (hl : s.locations c = some (.heldBy task)) :
    (revoked s root holder).locations c = none := by
  simp [revoked, hd, hl]

theorem revoked_loc_desc_message {s : State} {root holder c m : Nat}
    (hd : Desc s root c) (hl : s.locations c = some (.inMessage m)) :
    (revoked s root holder).locations c = some (.inMessage m) := by
  simp [revoked, hd, hl]

theorem revoked_loc_desc_namespace {s : State} {root holder c ns : Nat} {path : String}
    (hd : Desc s root c) (hl : s.locations c = some (.inNamespace ns path)) :
    (revoked s root holder).locations c = some (.inNamespace ns path) := by
  simp [revoked, hd, hl]

theorem revoked_loc_outside {s : State} {root holder c : Nat}
    (h : ¬ Desc s root c) :
    (revoked s root holder).locations c = s.locations c := by
  simp [revoked, h]

/-- Records and parent pointers are preserved; only `active` may change. -/
theorem revoked_parent {s : State} {root holder c : Nat} {cap : Capability}
    (hc : s.caps c = some cap) :
    ∃ cap', (revoked s root holder).caps c = some cap' ∧ cap'.parent = cap.parent := by
  by_cases hd : Desc s root c
  · exact ⟨{ cap with active := false }, revoked_caps_desc hd hc, rfl⟩
  · exact ⟨cap, by rw [revoked_caps_outside hd]; exact hc, rfl⟩

theorem revoke_fail {s : State} {root holder : Nat}
    (h : s.locations root ≠ some (.heldBy holder)) :
    revoke s root holder = (s, false) := by
  simp [revoke, h]

theorem revoke_success {s : State} {root holder : Nat}
    (h : s.locations root = some (.heldBy holder)) :
    revoke s root holder = (revoked s root holder, true) := by
  simp [revoke, h]

/-- After a successful revoke the root is no longer held, so a second revoke
    fails and leaves the state unchanged. -/
theorem revoke_idempotent {s : State} {root holder : Nat}
    (h : s.locations root = some (.heldBy holder)) :
    revoke (revoke s root holder).1 root holder = ((revoke s root holder).1, false) := by
  rw [revoke_success h]
  have hroot : (revoked s root holder).locations root = none :=
    revoked_loc_desc_held Desc.self h
  have hne : (revoked s root holder).locations root ≠ some (.heldBy holder) := by
    rw [hroot]; simp
  rw [revoke_fail hne]

theorem revoked_caps {s : State} {root holder c : Nat} :
    (revoked s root holder).caps c =
      (s.caps c).map (fun cap => if Desc s root c then { cap with active := false } else cap) := rfl

/-- Successful revocation keeps the active-capability CDT invariants: an active
    capability is necessarily outside the subtree, so neither it nor its parent
    was modified (a parent in the subtree would put the child in the subtree). -/
theorem revoke_preserves_active_wf {s : State} {root holder : Nat}
    (hwf : ActiveWellFormed s) :
    ActiveWellFormed (revoked s root holder) := by
  intro c cap' hcap' hactive p hparent
  rw [revoked_caps] at hcap'
  cases hc : s.caps c with
  | none => rw [hc] at hcap'; simp at hcap'
  | some cap =>
      rw [hc] at hcap'
      simp at hcap'
      by_cases hd : Desc s root c
      · simp [hd] at hcap'
        rw [← hcap'] at hactive
        simp at hactive
      · simp [hd] at hcap'
        rw [← hcap'] at hparent hactive ⊢
        have hnotp : ¬ Desc s root p := not_desc_parent hc hparent hd
        have hpcap := hwf c cap hc hactive p hparent
        refine ⟨hpcap.1, ?_⟩
        obtain ⟨pcap, hpdef, hpactive, hpobj⟩ := hpcap.2
        exact ⟨pcap, by rw [revoked_caps_outside hnotp]; exact hpdef, hpactive, hpobj⟩

end Formal.Revoke
