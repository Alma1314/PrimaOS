import Formal.Drop

-- Audit the dependencies of every general cap_drop theorem, excluding `#eval` anchors.
#print axioms Formal.Drop.drop_structural_fails
#print axioms Formal.Drop.drop_wrong_holder_fails
#print axioms Formal.Drop.drop_clears
#print axioms Formal.Drop.drop_keeps_other
#print axioms Formal.Drop.drop_keeps_caps
#print axioms Formal.Drop.drop_does_not_propagate
#print axioms Formal.Drop.drop_parent_child_independent
#print axioms Formal.Drop.drop_preserves_wellformed
#print axioms Formal.Drop.drop_consistency
#print axioms Formal.Drop.drop_idempotent
