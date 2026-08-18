import Formal.Bootstrap

import Formal.State

namespace Formal

/-- 最小 exc_resume 授权模型。
    若 caller 持有一个 active、owner=caller 的源能力 → 成功（返回 some s，不动状态，仅表达授权通过）
    ；否则失败返回 none。 -/
def excResume (s : State) (caller : Nat) (srcCap : Nat) : Option State :=
  match s.caps srcCap with
  | none => none
  | some cap =>
      if cap.owner != caller || !cap.active then none else some s

/-- 失败原子性：无能力被引用 → none（return none 表示失败，状态不变由返回值承载）。 -/
theorem exc_resume_missing_cap_returns_none :
    excResume emptyState 1 999 = none := by
  rfl

/-- 失败原子性：cap 存在但 owner 不是 caller → none。
    initCaps 在 CapId=1 处 owner=1；用 caller=2 触发 owner 不匹配。 -/
theorem exc_resume_foreign_owner_returns_none :
    excResume bootedState 2 1 = none := by
  rfl

/-- 成功返回 some：caller=1 持 CapId=1（owner=1 且 active）→ 返回 some。 -/
theorem exc_resume_success_returns_some :
    (excResume bootedState 1 1).isSome = true := by
  rfl

/-- 成功不吞掉能力：结果态在 CapId=1 的 caps 查找仍命中原值（投影断言，避免 funext）。 -/
theorem exc_resume_success_keeps_source_cap :
    let s' := (excResume bootedState 1 1).get!
    s'.caps 1 = some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true } := by
  rfl

end Formal
