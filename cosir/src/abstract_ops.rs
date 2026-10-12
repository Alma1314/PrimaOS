use crate::machine::MachineState;
use crate::pagewalk::PAGE_SIZE;
use crate::state::{
    ACCESS_R, ACCESS_X, CAP_DERIVE, CAP_NS_BIND, CAP_NS_DESTROY, CAP_NS_MOUNT, CAP_NS_RESOLVE,
    CAP_NS_UNBIND, CAP_NS_UNMOUNT, CAP_SEG_DESTROY, CAP_SEG_MAP_R, CAP_SEG_MAP_X, CAP_TASK_DESTROY,
    CAP_TASK_MANAGE_VSPACE, CAP_TASK_SEND, CapId, CapObject, Capability, Namespace, NsId,
    PageMapping, PageMappingBacking, SegId, Segment, State, Task, TaskId, TaskStatus,
};

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
///             booted=true  → §14 bootstrap 物化后的 booted State（init Task + image
///             Segment + 空 init Namespace + 三条无父根 Capability + image vspace + 结构性根）。
///
/// §14 步骤 4 的 image 逐页 `vspace` 由 `MachineState.image_frames`/`image_virtual_base`
/// 投影；步骤 7 的结构性根 `Task.root_ns_cap` 指向 init Namespace Capability。
pub fn abstract_state(m: &MachineState) -> State {
    let mut s = State {
        booted: m.booted,
        ..State::default()
    };
    if m.booted {
        const IMAGE_SEG: SegId = 1;
        const INIT_NS: NsId = 1;
        const INIT_NS_CAP: CapId = 2;
        let init = init_task_id();

        // §14 步骤 2：image Segment（访问集 {R,X}，pool=CPU；本探针不建模 frames 池与 pool）。
        s.segments.insert(
            IMAGE_SEG,
            Segment {
                bytes: Vec::new(),
                kernel_readable: true,
            },
        );
        // §14 步骤 3：空的 init Namespace。
        s.namespaces.insert(INIT_NS, Namespace);
        // §14 步骤 4：对每个 image 页 i 建立 vspace[image_virtual_base + i] =
        //   {segment = image_segment, offset = i, access = {R,X}}。
        let vspace = m
            .image_frames
            .iter()
            .enumerate()
            .map(|(i, &frame_phys)| PageMapping {
                base: m.image_virtual_base + (i as u64) * PAGE_SIZE,
                len: PAGE_SIZE,
                access: ACCESS_R | ACCESS_X,
                user: true, // init 以 ring3 执行该镜像（§14 交接点建立 init 上下文）
                backing: Some(PageMappingBacking {
                    segment: IMAGE_SEG,
                    offset: i as u64,
                    frame_phys,
                }),
            })
            .collect();
        // §14 步骤 4/7/8：init Task（状态 Ready；结构性根 = init Namespace Capability）。
        s.tasks.insert(
            init,
            Task {
                id: init,
                status: TaskStatus::Ready,
                vspace,
                root_ns_cap: INIT_NS_CAP,
                kernel_resume_frame: None,
            },
        );
        // §14 步骤 5/6：三条无父根 Capability（parent=None），放入 HeldBy(init)。
        s.capabilities.insert(
            1,
            Capability {
                owner: init,
                object: CapObject::Segment(IMAGE_SEG),
                rights: CAP_DERIVE | CAP_SEG_MAP_R | CAP_SEG_MAP_X | CAP_SEG_DESTROY,
                active: true,
            },
        );
        s.capabilities.insert(
            2,
            Capability {
                owner: init,
                object: CapObject::Namespace(INIT_NS),
                rights: CAP_NS_RESOLVE
                    | CAP_NS_BIND
                    | CAP_NS_UNBIND
                    | CAP_NS_MOUNT
                    | CAP_NS_UNMOUNT
                    | CAP_NS_DESTROY,
                active: true,
            },
        );
        s.capabilities.insert(
            3,
            Capability {
                owner: init,
                object: CapObject::Task(init),
                rights: CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_MANAGE_VSPACE | CAP_TASK_DESTROY,
                active: true,
            },
        );
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
    use crate::pagewalk::PAGE_SIZE;
    use crate::state::{
        ACCESS_R, ACCESS_X, CAP_DERIVE, CAP_NS_BIND, CAP_NS_DESTROY, CAP_NS_MOUNT, CAP_NS_RESOLVE,
        CAP_NS_UNBIND, CAP_NS_UNMOUNT, CAP_SEG_DESTROY, CAP_SEG_MAP_R, CAP_SEG_MAP_X,
        CAP_TASK_DESTROY, CAP_TASK_MANAGE_VSPACE, CAP_TASK_SEND, CapObject, Capability,
        PageMappingBacking, TaskStatus,
    };

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
            image_frames: vec![0x1000, 0x2000],
            image_virtual_base: 0x4000_0000,
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

        // §14 步骤 2/3：image Segment 与空 init Namespace 对象存在
        assert!(s.segments.contains_key(&1), "image Segment present");
        assert!(s.namespaces.contains_key(&1), "init Namespace present");

        // §14 步骤 4：image 逐页 vspace 映射（base/access/backing 与 frame 序对应）
        assert_eq!(init.vspace.len(), 2, "one mapping per image page");
        assert_eq!(init.vspace[0].base, 0x4000_0000);
        assert_eq!(init.vspace[1].base, 0x4000_1000);
        assert_eq!(init.vspace[0].len, PAGE_SIZE);
        assert_eq!(init.vspace[0].access, ACCESS_R | ACCESS_X);
        assert_eq!(
            init.vspace[1].backing,
            Some(PageMappingBacking {
                segment: 1,
                offset: 1,
                frame_phys: 0x2000,
            })
        );

        // §14 步骤 7：结构性根 = init Namespace Capability（CapId 2）
        assert_eq!(init.root_ns_cap, 2, "structural root is the namespace cap");

        // §14 步骤 5/6：三条无父根 Capability 恰好构成 init 的初始 Authority（§20.3）
        assert_eq!(s.capabilities.len(), 3, "exactly three root capabilities");
        assert_eq!(
            s.capabilities[&1],
            Capability {
                owner: init_task_id(),
                object: CapObject::Segment(1),
                rights: CAP_DERIVE | CAP_SEG_MAP_R | CAP_SEG_MAP_X | CAP_SEG_DESTROY,
                active: true,
            }
        );
        assert_eq!(
            s.capabilities[&2],
            Capability {
                owner: init_task_id(),
                object: CapObject::Namespace(1),
                rights: CAP_NS_RESOLVE
                    | CAP_NS_BIND
                    | CAP_NS_UNBIND
                    | CAP_NS_MOUNT
                    | CAP_NS_UNMOUNT
                    | CAP_NS_DESTROY,
                active: true,
            }
        );
        assert_eq!(
            s.capabilities[&3],
            Capability {
                owner: init_task_id(),
                object: CapObject::Task(1),
                rights: CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_MANAGE_VSPACE | CAP_TASK_DESTROY,
                active: true,
            }
        );
    }

    #[test]
    fn bootstrap_idempotent_returns_already_booted() {
        let m = pre_boot_machine();
        let mut already = m.clone();
        already.booted = true;
        assert_eq!(
            bootstrap(&already).unwrap_err(),
            BootstrapError::AlreadyBooted
        );
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
