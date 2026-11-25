import Formal.Container

-- Audit the dependencies of every W4 container theorem, excluding `#eval` anchors.
#print axioms Formal.Container.replace_loc_self
#print axioms Formal.Container.replace_loc_other
#print axioms Formal.Container.inNamespace_inj
#print axioms Formal.Container.messageAdd_self
#print axioms Formal.Container.messageAdd_other
#print axioms Formal.Container.messageRemove_self
#print axioms Formal.Container.messageRemove_other
#print axioms Formal.Container.namespaceSet_self
#print axioms Formal.Container.namespaceSet_other
#print axioms Formal.Container.setMessage_self
#print axioms Formal.Container.setMessage_other
#print axioms Formal.Container.setNamespace_self
#print axioms Formal.Container.setNamespace_other
#print axioms Formal.Container.queuedSend_preserves
#print axioms Formal.Container.recvFrom_preserves
#print axioms Formal.Container.bindNs_preserves
#print axioms Formal.Container.unbindNs_preserves
#print axioms Formal.Container.emptyWorld_ok
