import Formal.Derive

namespace Formal

/-! 能力派生树的**森林性**（seL4 CDT 的头号结论，规范 §4.2）。

    §4.2 规定「父 CapId 必须小于子 CapId，因此本节点内的派生关系天然无环」。本模块把该
    散文要求证成机器检查的结论：沿父链走任意 `k ≥ 1` 步，CapId **严格递减**，故不存在
    回到起点的环——派生图是一片森林。

    证据边界：同 `Formal/Derive.lean`（**REFINEMENT-DESIGN** 通道；不宣称内核 C 级细化或
    机器证明；cosir 侧已建模 `parent`，见 README 映照契约与固定实例对拍）。 -/

/-- 父单步：`c` 的父 CapId（`c` 无记录或无父 → `none`）。 -/
def parentOf (s : State) (c : Nat) : Option Nat :=
  (s.caps c).bind (fun cap => cap.parent)

/-- 沿父链走 `k` 步（`k = 0` 停在 `c`）。 -/
def ancestry (s : State) : Nat → Nat → Option Nat
  | 0,     c => some c
  | k + 1, c => (ancestry s k c).bind (parentOf s)

/-- 单步下降（§4.2）：父 CapId `<` 子 CapId。 -/
theorem parentOf_lt (s : State) (hwf : WellFormed s) {c p : Nat}
    (h : parentOf s c = some p) : p < c := by
  unfold parentOf at h
  cases hc : s.caps c with
  | none => simp only [hc, Option.bind_none] at h; cases h
  | some cap =>
      simp only [hc, Option.bind_some] at h
      exact (hwf c cap hc p h).1

/-- **森林性（无环）**：沿父链走 `k ≥ 1` 步到的 CapId 严格小于起点，故不存在父链环
    （seL4 CDT「派生图是森林」在 POSIR 上的具体化）。 -/
theorem ancestry_lt (s : State) (hwf : WellFormed s) :
    ∀ (k c d : Nat), ancestry s k c = some d → 0 < k → d < c := by
  intro k
  induction k with
  | zero => intro c d _ hk; exact absurd hk (Nat.lt_irrefl 0)
  | succ k ih =>
      intro c d h hk
      simp only [ancestry] at h
      rcases Nat.eq_zero_or_pos k with hk0 | hkpos
      · subst hk0
        simp only [ancestry, Option.bind_some] at h
        exact parentOf_lt s hwf h
      · cases hb : ancestry s k c with
        | none => simp only [hb, Option.bind_none] at h; cases h
        | some m =>
            simp only [hb, Option.bind_some] at h
            exact Nat.lt_trans (parentOf_lt s hwf h) (ih c m hb hkpos)

/-- 无自环（森林性的直接推论，与 `no_self_parent` 同源但由此定理导出）。 -/
theorem ancestry_no_cycle (s : State) (hwf : WellFormed s) (c : Nat) :
    ¬ (∃ k : Nat, 0 < k ∧ ancestry s k c = some c) := by
  rintro ⟨k, hk, h⟩
  exact absurd (ancestry_lt s hwf k c c h hk) (Nat.lt_irrefl c)

/-! 可执行锚（具体实例，非空洞）：booted 态上走父链——根无父链，故 1 步即 `none`。 -/

/-- CapId 1（image Segment 根，`parent = none`）走 1 步父链得 `none`。 -/
example : ancestry bootedState 1 1 = none := by native_decide

/-- 派生之后：CapId 4 的父是 CapId 1，走 1 步得 `some 1`，走 2 步（1 为根）得 `none`。 -/
example : ancestry (derive bootedState 1 4).get! 1 4 = some 1 := by native_decide
example : ancestry (derive bootedState 1 4).get! 2 4 = none := by native_decide

end Formal
