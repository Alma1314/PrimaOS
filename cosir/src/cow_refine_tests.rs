use crate::cow_refine::{CHILD, COW_VA, CowProbe, PARENT, base_exc, new_refine, probe_write};

#[test]
fn cross_model_state_is_deeply_comparable() {
    let pf = new_refine();
    let exc = base_exc();
    assert_eq!(pf, pf.clone());
    assert_eq!(exc, exc.clone());
    assert!(crate::memseg_refine::memseg_alpha(&pf), "fixture alpha");
}

#[test]
fn parent_and_child_share_read_only_at_child() {
    let pf = new_refine();
    let child_ref = pf.tasks[&crate::cow_refine::CHILD].vspace[&crate::cow_refine::COW_VA];
    assert_eq!(child_ref.access, crate::state::ACCESS_R);
}

#[test]
fn probe_parent_readwrite_is_writable() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
}

#[test]
fn probe_child_readonly_is_readonly() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::ReadOnly);
}

#[test]
fn probe_unmapped_is_not_mapped() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, 0x0600_0000), CowProbe::NotMapped);
}

#[test]
fn probe_has_no_side_effects() {
    let pf = new_refine();
    let before = pf.clone();
    let _ = probe_write(&pf, CHILD, COW_VA);
    assert_eq!(pf, before);
}
