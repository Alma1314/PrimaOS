import Refinement.Generated.Funs
import Formal.KernelHandle

/-! Source-extracted handle primitives refine the bounded lifecycle model.
    Charon/Aeneas and their Rust core models are part of this proof's TCB. -/
namespace Refinement.Handle

open Aeneas Aeneas.Std

abbrev SourceHandle := RustHandle.handle.Handle

/-- None at exhaustion is the kernel's permanent retirement marker. -/
def generationAfter (next : Option U32) : Nat :=
  (next.map UScalar.val).getD 0

theorem checked_successor_refines (g : U32) :
    generationAfter (U32.checked_add g 1#u32) =
      if g.val < Formal.KernelHandle.maxGeneration then g.val + 1 else 0 := by
  have hs := U32.checked_add_bv_spec g 1#u32
  cases hc : U32.checked_add g 1#u32 with
  | none =>
    simp only [hc] at hs
    have hn : ¬ g.val < Formal.KernelHandle.maxGeneration := by
      simp [U32.max, U32.numBits, Formal.KernelHandle.maxGeneration] at *
      omega
    simp [generationAfter, hn]
  | some next =>
    simp only [hc] at hs
    have hn : g.val < Formal.KernelHandle.maxGeneration := by
      simp [U32.max, U32.numBits, Formal.KernelHandle.maxGeneration] at *
      omega
    simp [generationAfter, hn, hs.2.1]

/-- This function is generated from the production Handle::next_generation body. -/
theorem source_successor_refines (h : SourceHandle) :
    RustHandle.handle.Handle.next_generation h =
      Aeneas.Std.Result.ok (U32.checked_add h.generation 1#u32) := by
  rfl

/-- A successful abstract release agrees with the extracted generation computation. -/
theorem source_release_generation_refines (h : SourceHandle)
    (s t : Formal.KernelHandle.Slot)
    (hr : Formal.KernelHandle.release Formal.KernelHandle.maxGeneration s
      h.generation.val = some t) :
    Aeneas.Std.bind (RustHandle.handle.Handle.next_generation h)
      (fun next => Aeneas.Std.Result.ok (generationAfter next)) =
      Aeneas.Std.Result.ok t.generation := by
  rw [source_successor_refines, Aeneas.Std.bind_ok]
  have he := checked_successor_refines h.generation
  unfold Formal.KernelHandle.release at hr
  split at hr
  · cases hr
    exact congrArg Aeneas.Std.Result.ok he
  · cases hr

/-- The safety property now starts from an actual source-extracted return value. -/
theorem source_successor_no_revival (h : SourceHandle) (next : Option U32)
    (hs : RustHandle.handle.Handle.next_generation h = Aeneas.Std.Result.ok next)
    (hn : h.generation.val ≠ 0) (future : Formal.KernelHandle.Slot)
    (ht : Formal.KernelHandle.Reachable Formal.KernelHandle.maxGeneration
      ⟨generationAfter next, false⟩ future) :
    Formal.KernelHandle.check future h.generation.val = false := by
  rw [source_successor_refines] at hs
  have he := Aeneas.Std.Result.ok_injective hs
  subst next
  apply Formal.KernelHandle.freed_handle_never_live
    (s := ⟨h.generation.val, true⟩) (ht := ht)
  simp [Formal.KernelHandle.release, hn, checked_successor_refines]

/-- The extracted constructor rejects exactly the forbidden zero components. -/
theorem source_constructor_refines (index generation : U32) :
    RustHandle.handle.Handle.new index generation = Aeneas.Std.Result.ok
      (if index = 0#u32 ∨ generation = 0#u32 then
        Aeneas.Std.core.result.Result.Err RustHandle.handle.HandleError.Malformed
       else Aeneas.Std.core.result.Result.Ok (RustHandle.handle.Handle.mk index generation)) := by
  unfold RustHandle.handle.Handle.new
  split <;> simp_all
  split <;> simp_all

end Refinement.Handle
