import Formal.AuditQuery

-- Audit the dependencies of every §13.2 audit-query theorem.
#print axioms Formal.AuditQuery.parentChain_head
#print axioms Formal.AuditQuery.parentChain_length_le
#print axioms Formal.AuditQuery.parentChain_sound
#print axioms Formal.AuditQuery.auditChain_sound
#print axioms Formal.AuditQuery.auditChain_length_le
#print axioms Formal.AuditQuery.auditChain_last
#print axioms Formal.AuditQuery.audit_invalid
#print axioms Formal.AuditQuery.audit_not_held
#print axioms Formal.AuditQuery.audit_not_held_other
#print axioms Formal.AuditQuery.audit_ok_held
#print axioms Formal.AuditQuery.audit_ok_fields
#print axioms Formal.AuditQuery.audit_dead_target_ok
#print axioms Formal.AuditQuery.audit_ok_creation_mem

-- Fuel completeness of the parent walk under WellFormed.
#print axioms Formal.AuditQuery.lastParentNone_getLast
#print axioms Formal.AuditQuery.lastParentNone_cons_of_tail
#print axioms Formal.AuditQuery.parentChain_nonempty
#print axioms Formal.AuditQuery.parentChain_succ_of_done
#print axioms Formal.AuditQuery.parentChain_done_add
#print axioms Formal.AuditQuery.parentChain_complete
#print axioms Formal.AuditQuery.auditChain_root_parent_none
