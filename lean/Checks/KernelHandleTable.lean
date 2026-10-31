import Formal.KernelHandleTable

-- Audit the dependencies of every general table theorem, excluding executable anchors.
#print axioms Formal.KernelHandleTable.allocateAt_effect
#print axioms Formal.KernelHandleTable.releaseAt_effect
#print axioms Formal.KernelHandleTable.step_projects
#print axioms Formal.KernelHandleTable.reachable_projects
#print axioms Formal.KernelHandleTable.reachable_preserves_wellformed
#print axioms Formal.KernelHandleTable.allocate_returns_live
#print axioms Formal.KernelHandleTable.allocate_preserves_other_handle
#print axioms Formal.KernelHandleTable.release_preserves_other_handle
#print axioms Formal.KernelHandleTable.retired_slot_stays_retired
#print axioms Formal.KernelHandleTable.freed_handle_never_live
