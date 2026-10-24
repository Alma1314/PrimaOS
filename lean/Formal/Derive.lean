import Formal.Bootstrap

namespace Formal

/-! 能力派生树的迁移模型（seL4 `cap_derive` 的对应物，规范 §4.4）。

    本模块把 §4.2 的 CDT 不变量（`WellFormed`）从恒真提升为**可保持的实质性质**：
    - `derive` 按 §4.4 从父 Capability 派生一个子 Capability；
    - `derive_preserves_wellformed`：良构在派生下保持（**非 `by rfl`**）；
    - `no_self_parent`：无自环（seL4 CDT 森林性的最小见证）。

    证据边界：本模块为 **REFINEMENT-DESIGN** 通道（独立 Lean 证明），不宣称内核 C 级细化
    或机器证明；cosir 侧已建模 `parent`，由 `lean_backtrace` 提供固定实例对拍。 -/

/-- 能力表单点更新（`Function.update` 的最小等价物——本模型只用 `Init.Core`，无该 API）。
    把 `f` 在 `a` 处的取值改为 `v`，其余不变。 -/
def update (f : Nat → Option Capability) (a : Nat) (v : Option Capability) :
    Nat → Option Capability :=
  fun c => if c = a then v else f c

@[simp] theorem update_self (f : Nat → Option Capability) (a : Nat) (v : Option Capability) :
    update f a v a = v := by simp [update]

@[simp] theorem update_of_ne {f : Nat → Option Capability} {a a' : Nat}
    (h : a' ≠ a) (v : Option Capability) : update f a v a' = f a' := by
  simp [update, h]

/-- §4.4 `cap_derive` 的前置守卫（§4.2 + §4.4）：父 Capability 存在、**活动**、含
    `CAP_DERIVE`、父 CapId `< newId`、且 `newId` 槽位为空。 -/
def deriveGuard (s : State) (parent newId : Nat) : Prop :=
  ∃ pcap, s.caps parent = some pcap ∧ pcap.active = true
    ∧ pcap.rights &&& capDerive ≠ 0 ∧ parent < newId ∧ s.caps newId = none

/-- §4.4 `cap_derive`：以 `parent` 为父派生一个子 Capability（新 CapId `newId`，权限沿用父）。
    失败返回 `none`（状态不变，与 `excResume` 同约定）。 -/
def derive (s : State) (parent newId : Nat) : Option State :=
  match s.caps parent with
  | none => none
  | some pcap =>
      if pcap.active = true ∧ pcap.rights &&& capDerive ≠ 0
          ∧ parent < newId ∧ s.caps newId = none then
        some { s with caps := update s.caps newId (some ({ pcap with parent := some parent })) }
      else none

/-- §4.4 保持性：**若状态良构，则 `derive` 成功后的状态仍良构**（本切片的实质证明义务）。
    新能力 `newId` 的 `parent = some parent` 且 `parent < newId`（守卫）；其余槽位不变。 -/
theorem derive_preserves_wellformed (s s' : State) (parent newId : Nat)
    (hwf : WellFormed s) (h : derive s parent newId = some s') : WellFormed s' := by
  intro c cap hc q hq
  unfold derive at h
  split at h
  · simp at h
  · rename_i pcap hpcap
    split at h
    case isTrue hguard =>
      obtain ⟨hact, -, hlt, hfresh⟩ := hguard
      cases h
      by_cases hc_eq : c = newId
      · -- 新能力：`parent = some parent` 且 `parent < newId`；父槽位未变故仍指 `pcap`。
        subst hc_eq
        simp only [update_self] at hc
        cases hc
        have hqp : parent = q := by simpa using hq
        refine ⟨hqp ▸ hlt, pcap, ?_, hact, ?_⟩
        · rw [← hqp]
          simp only [update_of_ne (Nat.ne_of_lt hlt), hpcap]
        · rfl
      · -- 旧能力：槽位未变，落回 hwf；父槽位 `q` 亦 ≠ `newId`（否则与空槽 `hfresh` 冲突）。
        simp only [update_of_ne hc_eq] at hc
        obtain ⟨hlt', p, hp, ha, ho⟩ := hwf c cap hc q hq
        have hq_ne : q ≠ newId := by
          intro hqe
          have hcontra : s.caps newId = some p := by rw [← hqe]; exact hp
          rw [hfresh] at hcontra
          cases hcontra
        exact ⟨hlt', p, by simp only [update_of_ne hq_ne, hp], ha, ho⟩
    case isFalse => simp at h

/-- 无自环：没有能力是自己的父（CDT 森林性的最小见证；由 §4.2 `parent < c` 直接推出）。 -/
theorem no_self_parent (s : State) (hwf : WellFormed s) (c : Nat) (cap : Capability)
    (hc : s.caps c = some cap) : cap.parent ≠ some c := by
  intro h
  exact absurd (hwf c cap hc c h).1 (Nat.lt_irrefl c)

/-! 可执行锚（具体实例，非空洞）：以 `native_decide` 在 booted 态上求值，钉住 `derive` 的
    成功/失败语义。booted 态三条根 Cap 中，CapId 1（image Segment）含 `CAP_DERIVE`。 -/

/-- 从 image Segment 根 Cap（CapId 1）派生 CapId 4：成功，且新槽位非空。 -/
example : (derive bootedState 1 4).isSome = true := by native_decide
example : ((derive bootedState 1 4).get!.caps 4).isSome = true := by native_decide

/-- §4.2 父 CapId 必须 `<` 子 CapId：`parent = newId = 1` 失败。 -/
example : derive bootedState 1 1 = none := by native_decide

/-- 父不存在（CapId 99 无记录）：失败。 -/
example : derive bootedState 99 100 = none := by native_decide

/-- §4.4 父须含 `CAP_DERIVE`：CapId 2（init Namespace 根，权限仅 `CAP_NS_*`，无 `CAP_DERIVE`）
    派生失败。（CapId 1/3 的根 Cap 均含 `CAP_DERIVE`，故不选。） -/
example : derive bootedState 2 4 = none := by native_decide

end Formal
