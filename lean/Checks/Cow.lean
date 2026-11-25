import Formal.Cow

-- Audit the dependencies of every general Cow theorem, excluding `#eval` anchors.
#print axioms Formal.Cow.readOnly_write
#print axioms Formal.Cow.privateRW_rw
#print axioms Formal.Cow.access_write_fault
#print axioms Formal.Cow.access_read_allowed
#print axioms Formal.Cow.access_unmapped_fault
#print axioms Formal.Cow.access_rejects_not_current
#print axioms Formal.Cow.access_rejects_no_task
#print axioms Formal.Cow.establishCow_ok
#print axioms Formal.Cow.establishCow_cows
#print axioms Formal.Cow.establishCow_missing
#print axioms Formal.Cow.establishCow_conflict_child_mapped
#print axioms Formal.Cow.establishCow_parent_not_rw
#print axioms Formal.Cow.privatize_ok
#print axioms Formal.Cow.privatized_task
#print axioms Formal.Cow.privatize_wrong_va
#print axioms Formal.Cow.privatize_not_faulted
#print axioms Formal.Cow.schedule_ok
#print axioms Formal.Cow.schedule_busy
#print axioms Formal.Cow.yieldTask_ok
#print axioms Formal.Cow.destroyTask_ok
#print axioms Formal.Cow.destroyTask_removed
#print axioms Formal.Cow.destroyTask_keeps_other
#print axioms Formal.Cow.destroyTask_rejects_faulted
#print axioms Formal.Cow.destroySegment_still
#print axioms Formal.Cow.destroySegment_ok
