import Formal.Revoke

-- Audit the dependencies of every general revocation theorem.
#print axioms Formal.Revoke.wellFormed_active
#print axioms Formal.Revoke.desc_trans
#print axioms Formal.Revoke.not_desc_parent
#print axioms Formal.Revoke.revoked_caps_desc
#print axioms Formal.Revoke.revoked_caps_outside
#print axioms Formal.Revoke.revoked_loc_desc_held
#print axioms Formal.Revoke.revoked_loc_desc_message
#print axioms Formal.Revoke.revoked_loc_desc_namespace
#print axioms Formal.Revoke.revoked_loc_outside
#print axioms Formal.Revoke.revoked_parent
#print axioms Formal.Revoke.revoke_fail
#print axioms Formal.Revoke.revoke_success
#print axioms Formal.Revoke.revoke_idempotent
#print axioms Formal.Revoke.revoked_caps
#print axioms Formal.Revoke.revoke_preserves_active_wf
