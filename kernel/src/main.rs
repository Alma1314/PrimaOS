#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

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
            let mut allocator = match kernel::mem::FreeFrameAllocator::from_regions(
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
                                        &inventory,
                                    ) {
                                        Ok(state) => protocol::bootstrap_ready(&state.snapshot),
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // E：在空闲 PML4 槽自建一条映射，见证真实 PTE 翻译、
                                    // 重映射 + invlpg 刷新，以及一次真实写保护 #PF。
                                    match kernel::vspace::run_witness(pml4_phys, offset, || {
                                        allocator.allocate().ok()
                                    }) {
                                        Ok(witness) => protocol::vspace_ready(&witness),
                                        Err(_) => {
                                            protocol::panic_event();
                                            serial::flush();
                                            qemu::exit(qemu::PANIC)
                                        }
                                    }

                                    // F：真实 COW —— 只读映射写故障 → 逐字节复制 →
                                    // 换映射可写 → 重试原故障指令成功。
                                    if kernel::cow::prepare(&mut allocator, pml4_phys, offset)
                                        .is_err()
                                    {
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

                                    // D：用捕获到的 #PF 现场帧构造一帧验证过的恢复帧，
                                    // 经真实 `iretq` 跳入 resume_landing；此后不再返回
                                    // 本路径，`boot_complete` 由恢复后的落点发出。
                                    resume::run_and_exit();
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
