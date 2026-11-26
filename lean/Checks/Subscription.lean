import Formal.Subscription

-- Audit the dependencies of every §12.4/W15 subscription theorem.
#print axioms Formal.Subscription.setSource_self
#print axioms Formal.Subscription.setSource_other
#print axioms Formal.Subscription.addSubscriber_self
#print axioms Formal.Subscription.addSubscriber_other
#print axioms Formal.Subscription.addSubscriber_active
#print axioms Formal.Subscription.clearSubscriber_self
#print axioms Formal.Subscription.clearSubscriber_other
#print axioms Formal.Subscription.clearSubscriber_active
#print axioms Formal.Subscription.endSub_self
#print axioms Formal.Subscription.endSub_other
#print axioms Formal.Subscription.endSubCap_excSource
#print axioms Formal.Subscription.w15_active
#print axioms Formal.Subscription.w15_witness
#print axioms Formal.Subscription.usableFor_snap
#print axioms Formal.Subscription.usableFor_snap_of_ne
#print axioms Formal.Subscription.SubscribeStep_adds
#print axioms Formal.Subscription.SubscribeStep_preserves
#print axioms Formal.Subscription.UnsubscribeStep_clears
#print axioms Formal.Subscription.UnsubscribeStep_preserves
#print axioms Formal.Subscription.DropLastStep_clears
#print axioms Formal.Subscription.DropLastStep_preserves
#print axioms Formal.Subscription.SendLastStep_clears
#print axioms Formal.Subscription.SendLastStep_preserves
#print axioms Formal.Subscription.DestroySourceStep_removes
#print axioms Formal.Subscription.DestroySourceStep_preserves
