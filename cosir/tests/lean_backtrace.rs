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
        image_frames: vec![0x1000, 0x2000],
        image_virtual_base: 0x4000_0000,
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

    // §14 步骤 5/6 + §20.3：两侧均铸三条无父根 Capability（cosir 侧逐条钉死，
    // 与 Lean `bootedState` / `bootstrap_empty_mints_{image,namespace,task}_cap` 对应）。
    assert_eq!(
        booted.capabilities.len(),
        3,
        "init mints exactly three root capabilities"
    );
    assert_eq!(booted.capabilities[&1].owner, init_task_id());
    assert!(matches!(
        booted.capabilities[&1].object,
        CapObject::Segment(1)
    ));
    assert_eq!(
        booted.capabilities[&1].rights,
        CAP_DERIVE | CAP_SEG_MAP_R | CAP_SEG_MAP_X | CAP_SEG_DESTROY
    );
    assert!(matches!(
        booted.capabilities[&2].object,
        CapObject::Namespace(1)
    ));
    assert!(matches!(booted.capabilities[&3].object, CapObject::Task(1)));

    // §14 步骤 4/7：image 逐页 vspace 与结构性根。Lean `State` 无 `tasks` 字段、
    // 未建模这两项（见 lean/README.md「已知不覆盖」）；此处**钉住 cosir 侧现状**，
    // 防静默改动掩盖该投影。
    let init = &booted.tasks[&init_task_id()];
    assert_eq!(init.vspace.len(), 2, "one mapping per image page");
    assert_eq!(init.vspace[0].base, 0x4000_0000);
    assert_eq!(init.vspace[1].base, 0x4000_1000);
    assert_eq!(init.root_ns_cap, 2, "structural root = init Namespace cap");
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
