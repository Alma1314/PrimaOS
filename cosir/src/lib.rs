//! cosir 库目标：把 host 状态模型的各模块外提为库，供 bin 与
//! `cosir/tests/` 集成测试共享引用。
//!
//! 证据边界：这些模块只提供 host `MODEL-TEST` 语义；不构成真实
//! CR3/PTE、KERNEL-TRACE 或 REFINEMENT-PROOF。

pub mod abstract_ops;
pub mod cow_refine;
pub mod machine;
pub mod memseg_refine;
pub mod ops;
pub mod pagewalk;
pub mod state;
pub mod tlb_refine;

// 内联单测模块：随 lib 编译单元（src）走，测试 lib 内模块。
// 注：cow_refine_tests 已在 cow_refine.rs 内部以 #[path] 声明，不在此重复。
#[cfg(test)]
mod memseg_refine_tests;
#[cfg(test)]
mod pagewalk_tests;
#[cfg(test)]
mod tlb_refine_tests;
