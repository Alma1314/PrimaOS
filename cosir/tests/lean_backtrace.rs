//! BLK-001 对拍：Lean 参考语义 ↔ cosir host 模型。
//! 证据等级：MODEL-TEST。Lean 侧 `#eval`/`theorem` 是参考；本文件把同一输入
//! 在 cosir 跑一遍，断言与 Lean 语义一致（bootstrap 阶段）。
//! 注意：抽象函数在 Rust 侧名为 `abstract_state`（abstract_ops.rs），对应 Lean `alpha`；
//! Rust 侧返回 `cosir::state::State`（含 `booted`/`tasks`），Lean `alpha` 返回 `s.booted`。

use cosir::abstract_ops::{abstract_state, bootstrap, init_task_id};
use cosir::machine::{BootInfoHandle, Cr3, GdtTable, InterruptState, MachineState, MapEntry, Tss};
use cosir::ops::exc_resume;
use cosir::state::*;

/// 交接点 pre-boot 机器态：booted=false、无任务（对齐 Lean `emptyState`）。
fn pre_boot_machine() -> MachineState {
    MachineState {
        cr3: Cr3(0x1000),
        address_space: vec![MapEntry {
            vaddr: 0x4000_0000,
            size: 0x1000,
            access: cosir::state::ACCESS_R | cosir::state::ACCESS_X,
        }],
        gdt: GdtTable {
            base: 0x2000,
            limit: 0x40,
        },
        tss: Tss { rsp0: 0x7000_0000 },
        rip: 0x5000_0000,
        rsp: 0x6fff_f000,
        boot_info: BootInfoHandle {
            phys_mem_start: 0x0,
            phys_mem_len: 0x8000,
        },
        idt_ready: false,
        interrupt_state: InterruptState::Masked,
        booted: false,
    }
}

#[test]
fn bootstrap_parity_lean() {
    // 投影保真：α(booted) ≠ α(pre_bootstrap) —— 对应 Lean `alpha_projection_distinguishes_boot`
    let before = abstract_state(&pre_boot_machine());
    assert!(!before.booted && before.tasks.is_empty());
    let booted = bootstrap(&pre_boot_machine()).expect("pre-boot bootstrap succeeds");
    assert!(booted.booted);
    assert!(
        booted.tasks.contains_key(&init_task_id()),
        "init task present"
    );
    assert_ne!(before.booted, booted.booted, "α 投影保真");
}

#[test]
fn exc_resume_failure_parity_lean() {
    // 构造一个未订阅异常的态：最小模型里 caller 无源能力 → Err(InvalidExcSource)
    let mut st: State = Default::default();
    let before = st.clone();
    let err = exc_resume(&mut st, 1, 99, 99, 0).unwrap_err();
    assert!(matches!(err, cosir::ops::ResumeError::InvalidExcSource));
    // Lean `excResume` 最小模型对无能力返回 none 且状态不变 → cosir Err 且 st==before
    assert_eq!(st, before, "失败路径必须保持状态完全不变(§17 基础保持性 4)");
}
