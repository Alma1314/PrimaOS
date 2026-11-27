import Formal.AuditQuery
import Formal.Bootstrap

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
#print axioms Formal.AuditQuery.objectEvents_sound
#print axioms Formal.AuditQuery.objectEvents_mem_iff
#print axioms Formal.AuditQuery.audit_ok_object_mem

-- Bootstrap already uses Segment(1) and Namespace(1): IDs are per object type.
open Formal Formal.Audit Formal.AuditQuery

private def collisionLog : Log :=
  { first := 10, entries := [
      { actor := .kernel, when := 0, kind := .objectCreated (.segment 1) },
      { actor := .kernel, when := 1, kind := .objectCreated (.namespace 1) },
      { actor := .kernel, when := 2, kind := .objectDestroyed (.namespace 1) },
      { actor := .kernel, when := 3, kind := .objectDestroyed (.segment 1) },
      { actor := .kernel, when := 4, kind := .objectCreated (.segment 2) },
      { actor := .kernel, when := 5, kind := .capMinted 1 }] }

example : (objectEvents collisionLog (.segment 1)).map Event.id = [10, 13] := by decide
example : (objectEvents collisionLog (.namespace 1)).map Event.id = [11, 12] := by decide
example : (objectEvents collisionLog (.segment 2)).map Event.id = [14] := by decide
example : objectEvents collisionLog (.task 1) = [] := by decide

example (s : Formal.Audit.State) :
    (auditedObjectDestroy (auditedObjectCreate s .kernel (.segment 1))
      .kernel (.namespace 1)).objects (.segment 1) = true := by
  simp [auditedObjectDestroy, auditedObjectCreate, State.createBit]

private def collisionState : AState :=
  { world := { base := { Formal.Container.emptyWorld with
                  snap := { Formal.Container.emptyWorld.snap with state := bootedState } }
               sources := fun _ => none }
    log := collisionLog, created := fun _ => none, objects := fun _ => false, now := 6 }

-- Queries still succeed for dead targets and now distinguish the two root caps.
example : (audit collisionState 1 1).toOption.map
    (fun tr => tr.objectHistory.map Event.id) = some [10, 13] := by decide
example : (audit collisionState 1 2).toOption.map
    (fun tr => tr.objectHistory.map Event.id) = some [11, 12] := by decide
