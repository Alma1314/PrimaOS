use crate::cow_refine::{base_exc, new_refine};

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
