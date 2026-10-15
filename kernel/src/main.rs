#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
// framekernel 边界（SPEC-076/079）：bin 侧同样零 unsafe——全部 asm 触面（含恢复落点）已在
// `prima-ostd`，由编译器强制。
#![deny(unsafe_code)]

mod config;
mod protocol;
mod qemu;
mod resume;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use kernel::mem;

/// 切片 C：#PF 捕获闭环的探测地址。
///
/// 选一个 bootloader 不映射的 canonical 低地址（内核镜像与物理直映射都在高地址）。
/// 触发前用 `translate` 自证该地址未映射（`PageNotMapped`），再触发真实 #PF，
/// handler 读回的 CR2 应等于该地址。xtask 侧用同一值断言。
const PROBE_FAULT_VA: u64 = 0x0000_0000_0004_0000;

entry_point!(kernel_main, config = &config::BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();

    let regions: &[bootloader_api::info::MemoryRegion] = &boot_info.memory_regions;
    match mem::build_inventory(regions, boot_info.kernel_addr, boot_info.kernel_len) {
        Ok(inventory) => {
            let allocator = match kernel::mem::FreeFrameAllocator::from_regions(
                regions,
                inventory.kernel_phys_start,
                inventory.kernel_phys_end,
            ) {
                Ok(allocator) if allocator.remaining() == inventory.free_frames => allocator,
                Ok(_) | Err(_) => {
                    protocol::panic_event();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            };
            // S1-X3：安装全局帧注册表——此后所有取帧都必须带 Segment 或页表帧归属。
            if kernel::frames::install(&inventory, allocator).is_err() {
                protocol::panic_event();
                serial::flush();
                qemu::exit(qemu::PANIC)
            }
            protocol::memory_ready(&inventory);

            // B1: 启用物理直映射后，读 boot info 的物理内存偏移；若为 None 说明配置未生效。
            match boot_info.physical_memory_offset.into_option() {
                Some(offset) => {
                    // 探测一个 bootloader 已映射、属 Usable 的物理帧：取 mem 清单第一个可用帧起始地址。
                    // 该地址由 build_inventory 保证是 4KB 对齐的 Usable 帧。
                    let probe_phys = mem::first_usable_frame_phys(regions);
                    match probe_phys {
                        Some(phys) => {
                            let (va, _byte) = mem::probe_physical_memory(phys, offset);
                            protocol::physical_map_ready(offset, va);

                            // B2: 读 cr3 得到 PML4 物理地址，经直映射转虚拟，translate 内核镜像
                            // 虚拟地址到物理，证明 bootloader 页表可读。
                            let pml4_phys = kernel::cpu::cr3().0 & 0x000f_ffff_ffff_f000;
                            let map = |phys: u64| mem::phys_to_virt(phys, offset);
                            if mem::validate_external_page_table_root(regions, pml4_phys).is_err() {
                                protocol::page_table_error();
                                serial::flush();
                                qemu::exit(qemu::PANIC)
                            }
                            let _ = kernel::cpu::audit_reachable_page_tables(
                                pml4_phys,
                                map,
                                |_| Ok(()),
                            );
                            let pml4_va = mem::phys_to_virt(pml4_phys, offset);
                            let target_va = boot_info.kernel_image_offset; // 内核镜像虚拟基址
                            match kernel::cpu::translate(pml4_va, target_va, map) {
                                Ok(translated_phys) => {
                                    protocol::page_table_ready(translated_phys);

                                    // B3: 装载 GDT + 最小 TSS（rsp0 = 内核栈顶）。
                                    let rsp0 =
                                        boot_info.kernel_stack_bottom + boot_info.kernel_stack_len;
                                    match kernel::gdt::init_kernel_gdt(rsp0) {
                                        Ok(()) => protocol::gdt_tss_ready(),
                                        Err(()) => {
                                            protocol::gdt_tss_error();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // B4: 装载 IDT，触发 int3，验证 trap → handler → iretq 闭环。
                                    match kernel::idt::init_idt() {
                                        Ok(()) => {
                                            // S2-D4：进入 CPL3 前屏蔽 8259 全部中断。上电默认
                                            // IRQ0→vector 8（=#DF 门），用户帧 IF=1 且用户阶段
                                            // 长于 PIT 默认周期时会被当成 #DF 交付并三重故障。
                                            kernel::preempt::mask_all_irqs();
                                            kernel::idt::trigger_int3();
                                            let seen = kernel::idt::trap_seen();
                                            protocol::trap_ready(seen);
                                        }
                                        Err(()) => {
                                            protocol::idt_error();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // C：触发一次真实 #PF。handler 已把 CR2 记入 fault_addr、
                                    // error code 记入 last_fault_err。分类由 CPU 给的 error code
                                    // 决定（读一个 absent 页 → P=0,W/R=0 → err=0x0 → not_present），
                                    // 不再需要软件先 translate 自证。
                                    kernel::idt::trigger_page_fault(PROBE_FAULT_VA);
                                    let fault_err = kernel::idt::last_fault_err();
                                    let class = kernel::fault::classify(fault_err);
                                    protocol::page_fault_ready(
                                        kernel::idt::fault_addr(),
                                        fault_err,
                                        class.as_str(),
                                    );

                                    // 观测：读 CR0.WP，看 supervisor 写只读页在当前环境是否故障。
                                    // 仅供日志/未来自建页表证实写保护现场；不硬断言、不加 PRIMA 事件，
                                    // 也不在 CR3 缺一个 present-只读页时伪造现场。
                                    let wp = kernel::cpu::read_cr0_wp();
                                    serial::write_bytes(b"[pf] cr0.wp=");
                                    serial::write_bytes(if wp { b"1" } else { b"0" });
                                    serial::write_bytes(b"\r\n");

                                    match kernel::bootstrap::create_state(
                                        boot_info.kernel_image_offset,
                                        boot_info.kernel_len,
                                        kernel_main as *const () as u64,
                                        &inventory,
                                    ) {
                                        Ok(state) => {
                                            protocol::bootstrap_ready(&state.snapshot);
                                            // K：安装授权表，供后续 syscall 授权查询使用。
                                            kernel::bootstrap::install(state);
                                            // EXO-1：bootloader 传入的通用 ramdisk；内核只解析
                                            // 格式头和 ELF program header，不按服务名索引 ELF。
                                            match (
                                                boot_info.ramdisk_addr.into_option(),
                                                boot_info.ramdisk_len,
                                            ) {
                                                (Some(addr), len) if len > 0 => {
                                                    if kernel::user::install_boot_image(addr, len)
                                                        .is_err()
                                                    {
                                                        protocol::panic_event();
                                                        serial::flush();
                                                        qemu::exit(qemu::PANIC)
                                                    }
                                                }
                                                _ => {
                                                    protocol::panic_event();
                                                    serial::flush();
                                                    qemu::exit(qemu::PANIC)
                                                }
                                            }
                                            // L：显式创建预定义异常源，并实证一次 pending 直投。
                                            if kernel::bootstrap::platform_init_exception_sources()
                                                .is_err()
                                            {
                                                protocol::panic_event();
                                                serial::flush();
                                                qemu::exit(qemu::PANIC)
                                            }
                                            // R：为 task 1/2 铸平台 Task Capability（供平台区域
                                            // 经 memseg_map 授权中介映射）。须在 platform_init
                                            // 建出 task 2 之后。
                                            if kernel::bootstrap::platform_prepare_task_caps()
                                                .is_err()
                                            {
                                                protocol::panic_event();
                                                serial::flush();
                                                qemu::exit(qemu::PANIC)
                                            }
                                            // S：播种用户 IPC 见证的两条 cap（task 1 的发送权、
                                            // task 2 的自管理权）。
                                            if kernel::bootstrap::prepare_user_ipc_caps().is_err() {
                                                protocol::panic_event();
                                                serial::flush();
                                                qemu::exit(qemu::PANIC)
                                            }
                                        }
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // E：在空闲 PML4 槽自建一条映射，见证真实 PTE 翻译、
                                    // 重映射 + invlpg 刷新，以及一次真实写保护 #PF。
                                    match kernel::vspace::run_witness(pml4_phys, offset) {
                                        Ok(witness) => protocol::vspace_ready(&witness),
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // F：真实 COW —— 只读映射写故障 → 逐字节复制 →
                                    // 换映射可写 → 重试原故障指令成功。
                                    if kernel::cow::prepare(pml4_phys, offset).is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    match kernel::cow::run() {
                                        Ok(witness) => protocol::cow_ready(&witness),
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // G：最小 ring3 用户任务 —— 进入 CPL3 执行用户代码，
                                    // 写只读用户页触发真实用户态 #PF，内核 COW 后重试成功，
                                    // 用户代码以 int3 陷入内核 → user_trap_exit 收尾（随后
                                    // 继续 D 的帧恢复见证并 boot_complete）。
                                    kernel::idt::set_user_trap_handler(resume::user_trap_exit);
                                    // X（ISSUE-AUTH-001）：构造**自建内核根**——自有 PML4 与全部
                                    // 中间页表，显式复刻内核镜像 / 物理直映射 / 启动栈三段，不再共享
                                    // bootloader 页表。此后所有任务根由它克隆，故内核高半区是自有的。
                                    // 必须在既有 E/F 见证（绑定 bootloader 根）之后，以免平移其帧序。
                                    let max_phys = regions
                                        .iter()
                                        .filter(|region| {
                                            region.kind
                                                == bootloader_api::info::MemoryRegionKind::Usable
                                        })
                                        .map(|region| region.end)
                                        .max()
                                        .unwrap_or(0);
                                    let self_root = match kernel::user::build_kernel_root(
                                        pml4_phys,
                                        offset,
                                        boot_info.kernel_image_offset,
                                        inventory.image_frames,
                                        max_phys,
                                        boot_info.kernel_stack_bottom,
                                        (boot_info.kernel_stack_len / kernel::mem::FRAME_SIZE)
                                            as usize,
                                    ) {
                                        Ok(root) => root,
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    };
                                    // 让后续 `SYS_TASK_CREATE` 也从自建根克隆地址空间。
                                    kernel::cow::rebase_root(self_root);
                                    if kernel::user::prepare(offset).is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // M：第二 ring3 任务的用户页面（与 task 1 区域不相交）。
                                    if kernel::user::prepare_task2(offset).is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // O：两任务各自的抢占空转页（同 VA、不同帧）。
                                    if kernel::user::prepare_spin(offset).is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // S2-D4：建新 Task 的代码段（用户区第一个 id）并铸两条 bring-up
                                    // Capability。必须放在既有取帧序列之后，避免平移 E/F/G/O 的
                                    // 物理帧序列。
                                    if kernel::user::prepare_task_create(offset).is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // SPEC-066：把 11 条用户可见平台段 cap 与 2 条平台 Task cap 按
                                    // 属主移交给 task 1 / task 2，使用户可自持经 memseg_map 使用。
                                    // 必须在既有 bring-up 映射（均以 KERNEL_ACTOR 为 caller）之后。
                                    if kernel::bootstrap::handover_platform_caps().is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // J：配置 syscall/sysret 快路径 MSR（EFER.SCE/STAR/LSTAR/FMASK），
                                    // 用户代码随后用 `syscall` 指令再次进入内核。
                                    kernel::syscall::configure_fast_path();
                                    // SPEC-070：在 `activate_init` **之前**采集 Faulted 见证——此刻尚无任何
                                    // Task 处于 Running，故「建见证 Task → 调度为 Running → §12.5 置
                                    // Faulted」是一条合法路径。见证 Task 用保留槽位，不占用普通 id、
                                    // 不铸 Capability、不取帧，既有的 id/帧/cap 序列不受扰动。
                                    if kernel::bootstrap::record_fault_witness().is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // SPEC-072：同一窗口采集 §8.2 内核回收见证（KernelActor 无 cap
                                    // 销毁不可恢复的 Faulted 任务；可恢复者必须被拒）。
                                    if kernel::bootstrap::record_reclaim_witness().is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // SPEC-073：同一窗口见证 §12.10 `exc_resume`（KernelActor 代故障任务
                                    // 调用）。帧的 landing/栈界取自切片 D 的恢复跳板与专用栈。
                                    let (landing, stack_bottom, stack_top) =
                                        resume::resume_target();
                                    if kernel::bootstrap::record_resume_witness(
                                        landing,
                                        stack_bottom,
                                        stack_top,
                                    )
                                    .is_err()
                                    {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    // SPEC-061：init 首次被调度——Ready→Running + TaskStateChanged。
                                    if kernel::bootstrap::activate_init().is_err() {
                                        protocol::panic_event();
                                        serial::flush();
                                        qemu::exit(qemu::PANIC)
                                    }
                                    kernel::user::enter();
                                }
                                Err(_) => {
                                    protocol::page_table_error();
                                    serial::flush();
                                    qemu::exit(qemu::PANIC)
                                }
                            }
                        }
                        None => {
                            protocol::physical_map_error();
                            serial::flush();
                            qemu::exit(qemu::PANIC)
                        }
                    }
                }
                None => {
                    protocol::physical_map_error();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            }
        }
        Err(error) => {
            protocol::memory_error(error);
            serial::flush();
            qemu::exit(qemu::PANIC)
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::init();
    protocol::panic_event();
    serial::flush();
    qemu::exit(qemu::PANIC)
}
