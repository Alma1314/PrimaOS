import Formal.Memseg

-- Audit the dependencies of every general Memseg theorem, excluding `#eval` anchors.
#print axioms Formal.Memseg.setAt_same
#print axioms Formal.Memseg.setAt_other
#print axioms Formal.Memseg.makePte_ok
#print axioms Formal.Memseg.viewsAgree_map
#print axioms Formal.Memseg.viewsAgree_unmap
#print axioms Formal.Memseg.mapEntry_ok
#print axioms Formal.Memseg.mapEntry_no_task
#print axioms Formal.Memseg.mapEntry_invalid_access
#print axioms Formal.Memseg.mapEntry_invalid_segment
#print axioms Formal.Memseg.mapEntry_overlap
#print axioms Formal.Memseg.unmapEntry_ok
#print axioms Formal.Memseg.unmapEntry_entry
#print axioms Formal.Memseg.unmapEntry_not_mapped
#print axioms Formal.Memseg.createSegment_ok
#print axioms Formal.Memseg.createSegment_owned
#print axioms Formal.Memseg.createSegment_other_frame
#print axioms Formal.Memseg.createSegment_fail_empty
#print axioms Formal.Memseg.createSegment_fail_access
#print axioms Formal.Memseg.destroySegment_ok
#print axioms Formal.Memseg.destroySegment_freed
#print axioms Formal.Memseg.destroySegment_removed
#print axioms Formal.Memseg.destroySegment_still
#print axioms Formal.Memseg.destroyTask_ok
#print axioms Formal.Memseg.destroyTask_freed
#print axioms Formal.Memseg.spawnTask_ok
#print axioms Formal.Memseg.spawnTask_views
#print axioms Formal.Memseg.mapEntry_success_views
#print axioms Formal.Memseg.unmapEntry_success_views
