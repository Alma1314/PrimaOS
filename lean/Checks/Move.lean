import Formal.Move

-- Audit the dependencies of every §4.6 move theorem, excluding `#eval` anchors.
#print axioms Formal.Move.applyMove_requires_source
#print axioms Formal.Move.applyMove_dest
#print axioms Formal.Move.applyMove_event
#print axioms Formal.Move.applyMove_keeps_caps
#print axioms Formal.Move.applyMove_preserves_consistency
#print axioms Formal.Move.carriers_after_move
#print axioms Formal.Move.w3_unique_location
#print axioms Formal.Move.queuedSend_dest
#print axioms Formal.Move.directSend_msgId
#print axioms Formal.Move.recvDeliver_msgId
#print axioms Formal.Move.nsBind_dest
#print axioms Formal.Move.deletion_sink
#print axioms Formal.Move.crossNodeSend_no_move
#print axioms Formal.Move.crossNodeSend_empty_ok
#print axioms Formal.Move.crossNodeSend_nonportable_fails
