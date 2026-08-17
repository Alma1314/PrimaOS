#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod config;
mod protocol;
mod qemu;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use kernel::mem;

entry_point!(kernel_main, config = &config::BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();

    let regions: &[bootloader_api::info::MemoryRegion] = &boot_info.memory_regions;
    match mem::build_inventory(regions, boot_info.kernel_addr, boot_info.kernel_len) {
        Ok(inventory) => {
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
                            protocol::boot_complete();
                            serial::flush();
                            qemu::exit(qemu::SUCCESS)
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
