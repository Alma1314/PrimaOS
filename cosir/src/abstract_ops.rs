use crate::machine::MachineState;
use crate::pagewalk::PAGE_SIZE;
use crate::state::{
    ACCESS_R, ACCESS_X, Actor, AuditKind, CAP_DERIVE, CAP_NS_BIND, CAP_NS_DESTROY, CAP_NS_MOUNT,
    CAP_NS_RESOLVE, CAP_NS_UNBIND, CAP_NS_UNMOUNT, CAP_SEG_DESTROY, CAP_SEG_MAP_R, CAP_SEG_MAP_X,
    CAP_TASK_DESTROY, CAP_TASK_MANAGE_VSPACE, CAP_TASK_SEND, CapId, CapObject, Capability,
    Namespace, NsId, PageMapping, PageMappingBacking, SegId, Segment, State, Task, TaskId,
    TaskStatus,
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
        s.insert_capability(
            1,
            Capability {
                owner: init,
                object: CapObject::Segment(IMAGE_SEG),
                rights: CAP_DERIVE | CAP_SEG_MAP_R | CAP_SEG_MAP_X | CAP_SEG_DESTROY,
                active: true,
                parent: None,
            },
        );
        s.insert_capability(
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
                parent: None,
            },
        );
        s.insert_capability(
            3,
            Capability {
                owner: init,
                object: CapObject::Task(init),
                rights: CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_MANAGE_VSPACE | CAP_TASK_DESTROY,
                active: true,
                parent: None,
            },
        );
        // §14 步骤 9 / §13.1：写入第一组 Audit 事件（`KernelActor`，bootstrap 无 Task 调用者）。
        // 顺序与形态镜像真实内核（`kernel/src/bootstrap.rs`）：`Bootstrapped` → 两个
        // `ObjectCreated`（image Segment、init Namespace；`segment` 标志区分同 id 的两种对象）
        // → init Task 创建 → 三条初始根 Capability 的 `CapMinted`（W13 创建记录）。
        s.push_audit(Actor::Kernel, AuditKind::Bootstrapped);
        s.push_audit(
            Actor::Kernel,
            AuditKind::ObjectCreated {
                object_id: IMAGE_SEG,
                segment: true,
            },
        );
        s.push_audit(
            Actor::Kernel,
            AuditKind::ObjectCreated {
                object_id: INIT_NS,
                segment: false,
            },
        );
        s.push_audit(
            Actor::Kernel,
            AuditKind::TaskCreated {
                task: init,
                status: TaskStatus::Ready,
            },
        );
        for (cap, holder) in [(1, init), (2, init), (3, init)] {
            s.push_audit(Actor::Kernel, AuditKind::CapabilityMinted { cap, holder });
        }
    }
    s
}

/// §4.2 能力派生树不变量（镜像 Lean `Formal.WellFormed`）：每条带父的能力满足
/// `parent < c`，且父存在、活动、同对象。**非恒真**——`derive_capability` 保持它。
pub fn well_formed(s: &State) -> bool {
    s.capabilities.iter().all(|(&c, cap)| match cap.parent {
        None => true,
        Some(p) => {
            p < c
                && s.capabilities
                    .get(&p)
                    .is_some_and(|pcap| pcap.active && pcap.object == cap.object)
        }
    })
}

/// §4.4 `cap_derive` 的失败原因（镜像 Lean `derive` 合取守卫的各分支）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeriveError {
    /// 父 CapId 无能力记录。
    NoSuchParent,
    /// 父能力非活动。
    ParentInactive,
    /// 父能力缺 `CapDerive`（§4.4 前置）。
    NoDeriveRight,
    /// 违反 §4.2「父 CapId < 子 CapId」。
    OrderViolation,
    /// 目标槽已有能力。
    SlotOccupied,
}

/// §4.4 `cap_derive` 对应物（镜像 Lean `Formal.derive`）：由 `parent` 派生到空槽 `new_id`，
/// 子能力与父同对象、同权限、同拥有者，父指针置 `Some(parent)`。失败原子（状态不变）。
pub fn derive_capability(s: &mut State, parent: CapId, new_id: CapId) -> Result<(), DeriveError> {
    let pcap = s
        .capabilities
        .get(&parent)
        .cloned()
        .ok_or(DeriveError::NoSuchParent)?;
    if !pcap.active {
        return Err(DeriveError::ParentInactive);
    }
    if pcap.rights & CAP_DERIVE == 0 {
        return Err(DeriveError::NoDeriveRight);
    }
    if parent >= new_id {
        return Err(DeriveError::OrderViolation);
    }
    if s.capabilities.contains_key(&new_id) {
        return Err(DeriveError::SlotOccupied);
    }
    let location = s.locations.get(&parent).cloned();
    s.insert_capability(
        new_id,
        Capability {
            parent: Some(parent),
            ..pcap
        },
    );
    if let Some(location @ crate::state::CapLocation::HeldBy(_)) = location {
        s.locations.insert(new_id, location);
    } else {
        s.locations.remove(&new_id);
    }
    Ok(())
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
        ACCESS_R, ACCESS_X, Actor, AuditEvent, AuditKind, CAP_DERIVE, CAP_NS_BIND, CAP_NS_DESTROY,
        CAP_NS_MOUNT, CAP_NS_RESOLVE, CAP_NS_UNBIND, CAP_NS_UNMOUNT, CAP_SEG_DESTROY,
        CAP_SEG_MAP_R, CAP_SEG_MAP_X, CAP_TASK_DESTROY, CAP_TASK_MANAGE_VSPACE, CAP_TASK_SEND,
        CapObject, Capability, PageMappingBacking, TaskStatus,
    };

    /// 构造带 §13.1 信封的事件（bootstrap 无 `time_advance`，`State.now` 恒 0 ⇒ `at = 0`）。
    fn ev(id: u64, kind: AuditKind) -> AuditEvent {
        AuditEvent {
            id,
            at: 0,
            actor: Actor::Kernel,
            kind,
        }
    }

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
        // §14 步骤 9：第一组 Audit 只在 booted 态写入；pre-boot 态无事件。
        assert!(s.audit.is_empty(), "pre-boot 态无 Audit");
        assert_eq!(s.next_event, 0);
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
                parent: None,
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
                parent: None,
            }
        );
        assert_eq!(
            s.capabilities[&3],
            Capability {
                owner: init_task_id(),
                object: CapObject::Task(1),
                rights: CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_MANAGE_VSPACE | CAP_TASK_DESTROY,
                active: true,
                parent: None,
            }
        );

        // §14 步骤 9 / §13.1：第一组 Audit 事件（`KernelActor`），顺序与真实内核一致。
        // init Task 以 `TaskCreated` 记录（§13.1 的创建事件是 `TaskCreated`，创建非迁移）。
        assert_eq!(
            s.audit,
            vec![
                ev(0, AuditKind::Bootstrapped),
                ev(
                    1,
                    AuditKind::ObjectCreated {
                        object_id: 1,
                        segment: true,
                    }
                ),
                ev(
                    2,
                    AuditKind::ObjectCreated {
                        object_id: 1,
                        segment: false,
                    }
                ),
                ev(
                    3,
                    AuditKind::TaskCreated {
                        task: init_task_id(),
                        status: TaskStatus::Ready,
                    }
                ),
                ev(
                    4,
                    AuditKind::CapabilityMinted {
                        cap: 1,
                        holder: init_task_id(),
                    }
                ),
                ev(
                    5,
                    AuditKind::CapabilityMinted {
                        cap: 2,
                        holder: init_task_id(),
                    }
                ),
                ev(
                    6,
                    AuditKind::CapabilityMinted {
                        cap: 3,
                        holder: init_task_id(),
                    }
                ),
            ]
        );
        assert_eq!(s.next_event, 7, "W13：EventId 严格递增");
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
