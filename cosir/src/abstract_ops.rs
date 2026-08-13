use crate::machine::MachineState;
use crate::state::{State, Task, TaskId, TaskStatus};

/// bootstrap 迁移的错误类型（最小集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    /// machine.booted 已为 true，重复 bootstrap。
    AlreadyBooted,
}

/// init Task 的恒定量 ID。
pub fn init_task_id() -> TaskId {
    1
}

/// 抽象函数 α：MachineState → State。
/// 两段投影：booted=false → EmptyState（booted=false 的空 State）；
///             booted=true  → 含 init Task 的 booted State。
pub fn abstract_state(m: &MachineState) -> State {
    let mut s = State {
        booted: m.booted,
        ..State::default()
    };
    if m.booted {
        let init = Task {
            id: init_task_id(),
            status: TaskStatus::Ready,
            vspace: Vec::new(),
            kernel_resume_frame: None,
        };
        s.tasks.insert(init_task_id(), init);
    }
    s
}

/// bootstrap 迁移：空态 → 含 init Task 的 booted 态。
/// 仅当 machine.booted == false 成功；否则返回 AlreadyBooted 且状态不变。
pub fn bootstrap(m: &MachineState) -> Result<State, BootstrapError> {
    if m.booted {
        return Err(BootstrapError::AlreadyBooted);
    }
    let booted_machine = MachineState {
        booted: true,
        ..m.clone()
    };
    Ok(abstract_state(&booted_machine))
}

#[cfg(test)]
mod tests {
    use super::{BootstrapError, abstract_state, bootstrap, init_task_id};
    use crate::machine::{
        BootInfoHandle, Cr3, GdtTable, InterruptState, MachineState, MapEntry, Tss,
    };
    use crate::state::{ACCESS_R, ACCESS_X, TaskStatus};

    fn pre_boot_machine() -> MachineState {
        MachineState {
            cr3: Cr3(0x1000),
            address_space: vec![MapEntry {
                vaddr: 0x4000_0000,
                size: 0x1000,
                access: ACCESS_R | ACCESS_X,
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
    fn alpha_pre_bootstrap_projects_to_empty_state() {
        let m = pre_boot_machine();
        let s = abstract_state(&m);
        assert!(!s.booted);
        assert!(s.tasks.is_empty());
    }

    #[test]
    fn bootstrap_produces_booted_state_with_init_task() {
        let m = pre_boot_machine();
        let s = bootstrap(&m).unwrap();
        assert!(s.booted);
        let init = s.tasks.get(&init_task_id()).expect("init task present");
        assert_eq!(init.status, TaskStatus::Ready);
        assert_eq!(s.tasks.len(), 1, "no extra objects");
    }

    #[test]
    fn bootstrap_idempotent_returns_already_booted() {
        let m = pre_boot_machine();
        let mut already = m.clone();
        already.booted = true;
        // bootstrap 返回 Result<State, BootstrapError>；State 未实现 Debug，
        // 故用 match 而非 unwrap_err（避免为测试给 State 派生 Debug 而触碰未列入提交的 state.rs）
        let err = match bootstrap(&already) {
            Err(e) => e,
            Ok(_) => panic!("expected AlreadyBooted"),
        };
        assert_eq!(err, BootstrapError::AlreadyBooted);
    }

    #[test]
    fn alpha_projection_preserves_booted_flag() {
        let mut m = pre_boot_machine();
        m.booted = true;
        let s = abstract_state(&m);
        assert!(s.booted);
        assert!(!s.tasks.is_empty());
    }

    #[test]
    fn dynamic_machine_steps_map_to_bootstrap_migration() {
        // 交接点（pre-boot）：内部配置步骤（GDT/IDT 装载）不改变抽象态
        let m = pre_boot_machine();
        let before = abstract_state(&m);
        let mut idle = m.clone();
        idle.gdt = GdtTable {
            base: 0x3000,
            limit: 0x40,
        }; // 内部步骤：改 GDT 基址
        idle.idt_ready = true; // 内部步骤：装载 IDT
        let internal = abstract_state(&idle);
        // 内部步骤不改变抽象态（仍是 EmptyState）
        assert_eq!(before.booted, internal.booted);
        assert!(internal.tasks.is_empty());
        // 可观察步骤：bootstrap → POSIR 迁移（EmptyState → booted init 态）
        let after = bootstrap(&idle).unwrap();
        assert!(after.booted && !after.tasks.is_empty());
        assert_ne!(internal.booted, after.booted);
    }
}
