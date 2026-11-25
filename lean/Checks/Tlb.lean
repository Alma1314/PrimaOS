import Formal.Tlb

-- Audit the dependencies of every general TLB theorem, excluding `#eval` anchors.
#print axioms Formal.Tlb.insert_same
#print axioms Formal.Tlb.insert_other
#print axioms Formal.Tlb.invalidate_clears
#print axioms Formal.Tlb.invalidate_other_task
#print axioms Formal.Tlb.invalidate_other_va
#print axioms Formal.Tlb.flush_clears
#print axioms Formal.Tlb.flush_other_task
#print axioms Formal.Tlb.removeTask_clears
#print axioms Formal.Tlb.removeTask_other_task
#print axioms Formal.Tlb.hasTlb_insert
#print axioms Formal.Tlb.hasTlb_invalidate
#print axioms Formal.Tlb.hasTlb_flush
#print axioms Formal.Tlb.invalidate_idempotent
#print axioms Formal.Tlb.flush_idempotent
#print axioms Formal.Tlb.removeTask_idempotent
#print axioms Formal.Tlb.invalidate_ok
#print axioms Formal.Tlb.invalidate_invalid
#print axioms Formal.Tlb.invalidate_no_task
#print axioms Formal.Tlb.flush_ok
#print axioms Formal.Tlb.flush_no_task
#print axioms Formal.Tlb.translate_hit
#print axioms Formal.Tlb.translate_miss
#print axioms Formal.Tlb.translate_not_mapped
#print axioms Formal.Tlb.translate_invalid_address
#print axioms Formal.Tlb.translate_no_task
#print axioms Formal.Tlb.translate_after_invalidate
#print axioms Formal.Tlb.insert_preserves_other_task
#print axioms Formal.Tlb.invalidate_fresh_for
#print axioms Formal.Tlb.flush_fresh_for
#print axioms Formal.Tlb.removeTask_fresh_for
#print axioms Formal.Tlb.translate_miss_fresh
