//! WIN-M2 ring3 原型：在 Prima 内装载并运行一个原样 PE32+ 控制台程序。
//!
//! 只在 `winpe-probe` feature 且构建时设置 `PRIMA_WINPE_PROBE` 时由 servicemgr 调用。
//! 装载器在用户态完成：真实 PE 夹具经本 crate 的 PE 解析/平铺/重定位/IAT 填充后写入
//! 自建 Segment，作为 held Task 的代码段映射为 R+X；栈、结果页和导入/退出桩另段映射。
//! PE 通过导入桩返回 42，退出桩把 42 写入共享结果页并自毁；父服务在子任务退出后继续
//! 第二次装载。没有内核 PE 识别、没有 W+X 映射，也没有 Windows 参考差分。

use crate::pe::PeImage;
use prima_runtime::{Capability, Error, Result, Rights, Runtime, SyscallTransport, TaskSpec};

const WINPE_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/winpe-probe.exe"));

const MAX_IMAGE: usize = 0x5000;

const PREFERRED_BASE: u64 = 0x1_4000_0000;
const RELOCATED_BASE: u64 = 0x1_5000_0000;
const IMAGE_PAGES: u64 = 5;

const PE_STAGE_VA: u64 = 0x0000_0A60_0000_0000;
const STACK_STAGE_VA: u64 = 0x0000_0A61_0000_0000;
const RESULT_STAGE_VA: u64 = 0x0000_0A62_0000_0000;
const STUB_STAGE_VA: u64 = 0x0000_0A63_0000_0000;

const CHILD_STACK_VA: u64 = 0x1_6000_0000;
const CHILD_RESULT_VA: u64 = 0x1_7000_0000;
const CHILD_IMPORT_STUB_VA: u64 = 0x1_7000_1000;
const CHILD_EXIT_STUB_VA: u64 = 0x1_7000_2000;
const STACK_PAGES: u64 = 4;
const FRAME_SIZE: u64 = 4096;

const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
const RX: Rights = Rights::from_bits((1 << 1) | (1 << 3));
const RWX: Rights = Rights::from_bits((1 << 1) | (1 << 2) | (1 << 3));
const NS_RESOLVE: Rights = Rights::from_bits(1 << 8);

const IMPORT_STUB: [u8; 6] = [0x89, 0xC8, 0x83, 0xC0, 0x01, 0xC3];

/// 串口见证行；xtask 只认这些完整行。
const PASS_LINE: &[u8] = b"WINPE|child1=42;child2=42;parent=alive\nWINPE|PASS\n";
const FAIL_LINE: &[u8] = b"WINPE|FAIL\n";

/// 入口：由 servicemgr 在专用构建中调用，永不返回。
pub fn run_or_exit<T: SyscallTransport>(runtime: &Runtime<T>) -> ! {
    serial(runtime, b"WINPE|stage=begin\n");
    let passed = run(runtime).is_ok();
    serial(runtime, if passed { PASS_LINE } else { FAIL_LINE });
    exit_qemu(runtime, passed)
}

fn run<T: SyscallTransport>(runtime: &Runtime<T>) -> Result<()> {
    let pe = PeImage::parse(WINPE_BYTES).map_err(|_| Error::InvalidInput)?;
    if pe.size_of_image() as usize > MAX_IMAGE {
        return Err(Error::InvalidInput);
    }
    serial(runtime, b"WINPE|stage=parsed\n");
    let namespace = runtime.namespace_create()?;
    let self_cap = runtime.self_capability()?;
    serial(runtime, b"WINPE|stage=namespace\n");

    let relocated = run_child(runtime, self_cap, namespace, pe, RELOCATED_BASE)?;
    serial(runtime, b"WINPE|stage=child1\n");
    let preferred = run_child(runtime, self_cap, namespace, pe, PREFERRED_BASE)?;
    serial(runtime, b"WINPE|stage=child2\n");

    let _ = runtime.namespace_destroy(namespace);
    let _ = runtime.cap_drop(namespace);

    if relocated == 42 && preferred == 42 {
        Ok(())
    } else {
        Err(Error::InvalidResponse)
    }
}

fn run_child<T: SyscallTransport>(
    runtime: &Runtime<T>,
    self_cap: Capability,
    namespace: Capability,
    pe: PeImage<'static>,
    base: u64,
) -> Result<u64> {
    let image_segment = runtime.memseg_create(IMAGE_PAGES, RWX)?;
    let stack_segment = runtime.memseg_create(STACK_PAGES, RW)?;
    let result_segment = runtime.memseg_create(1, RW)?;
    let stub_segment = runtime.memseg_create(2, RWX)?;
    serial(runtime, b"WINPE|stage=segments\n");

    runtime.memseg_map(image_segment, self_cap, 0, PE_STAGE_VA, IMAGE_PAGES, RW)?;
    serial(runtime, b"WINPE|stage=map-image\n");
    runtime.memseg_map(stack_segment, self_cap, 0, STACK_STAGE_VA, STACK_PAGES, RW)?;
    runtime.memseg_map(result_segment, self_cap, 0, RESULT_STAGE_VA, 1, RW)?;
    runtime.memseg_map(stub_segment, self_cap, 0, STUB_STAGE_VA, 2, RW)?;
    serial(runtime, b"WINPE|stage=mapped\n");

    serial(runtime, b"WINPE|stage=load\n");
    let buffer = prima_ostd::mm::bytes_mut_at(PE_STAGE_VA, MAX_IMAGE);
    let loaded = pe
        .load_into(base, buffer, |_, _| Some(CHILD_IMPORT_STUB_VA))
        .map_err(|_| Error::InvalidInput)?;
    serial(runtime, b"WINPE|stage=loaded\n");

    runtime.write_user_bytes(
        STACK_STAGE_VA + STACK_PAGES * FRAME_SIZE - 8,
        &CHILD_EXIT_STUB_VA.to_le_bytes(),
    )?;
    runtime.write_user_bytes(RESULT_STAGE_VA, &0u64.to_le_bytes())?;
    runtime.write_user_bytes(STUB_STAGE_VA, &IMPORT_STUB)?;
    runtime.write_user_bytes(STUB_STAGE_VA + FRAME_SIZE, &exit_stub())?;

    serial(runtime, b"WINPE|stage=staged\n");
    let manager = runtime.task_create_held(&TaskSpec {
        code_segment: image_segment,
        code_offset: 0,
        code_pages: IMAGE_PAGES,
        load_base: base,
        entry: loaded.entry_va(),
        stack_top: CHILD_STACK_VA + STACK_PAGES * FRAME_SIZE - 8,
        parent_namespace: Some(namespace),
        root_rights: NS_RESOLVE,
        initial_capabilities: [None, None],
        name: 0,
    })?;

    runtime.memseg_map(stack_segment, manager, 0, CHILD_STACK_VA, STACK_PAGES, RW)?;
    runtime.memseg_map(result_segment, manager, 0, CHILD_RESULT_VA, 1, RW)?;
    runtime.memseg_map(stub_segment, manager, 0, CHILD_IMPORT_STUB_VA, 1, RX)?;
    runtime.memseg_map(stub_segment, manager, 1, CHILD_EXIT_STUB_VA, 1, RX)?;
    serial(runtime, b"WINPE|stage=held\n");
    runtime.task_start_native(manager)?;
    serial(runtime, b"WINPE|stage=started\n");

    let mut value = 0u64;
    for _ in 0..4096 {
        value = runtime.read_user_u64(RESULT_STAGE_VA).unwrap_or(0);
        if value != 0 {
            break;
        }
        runtime.yield_task()?;
    }

    serial(runtime, b"WINPE|stage=result\n");
    // 子任务在写结果后立即自毁；多让几轮再回收，避免与退出路径竞争。
    for _ in 0..8 {
        let _ = runtime.yield_task();
    }
    let self_destroyed = runtime.task_destroy(manager).is_err();
    serial(
        runtime,
        if self_destroyed {
            b"WINPE|exit=self\n"
        } else {
            b"WINPE|exit=parent\n"
        },
    );
    let _ = runtime.cap_drop(manager);

    let _ = runtime.memseg_unmap(self_cap, PE_STAGE_VA, IMAGE_PAGES);
    let _ = runtime.memseg_unmap(self_cap, STACK_STAGE_VA, STACK_PAGES);
    let _ = runtime.memseg_unmap(self_cap, RESULT_STAGE_VA, 1);
    let _ = runtime.memseg_unmap(self_cap, STUB_STAGE_VA, 2);
    for segment in [image_segment, stack_segment, result_segment, stub_segment] {
        let _ = runtime.memseg_destroy(segment);
        let _ = runtime.cap_drop(segment);
    }

    if value == 0 {
        Err(Error::Again)
    } else {
        Ok(value)
    }
}

/// `mov rcx, RESULT_VA; mov [rcx], rax; mov eax, SYS_TASK_DESTROY; syscall; jmp $`
fn exit_stub() -> [u8; 25] {
    let mut code = [0u8; 25];
    code[0] = 0x48;
    code[1] = 0xB9;
    code[2..10].copy_from_slice(&CHILD_RESULT_VA.to_le_bytes());
    code[10] = 0x48;
    code[11] = 0x89;
    code[12] = 0x01;
    code[13] = 0xB8;
    code[14..18].copy_from_slice(&20u32.to_le_bytes());
    code[18] = 0x0F;
    code[19] = 0x05;
    code[20] = 0xEB;
    code[21] = 0xFE;
    code
}

fn serial<T: SyscallTransport>(runtime: &Runtime<T>, bytes: &[u8]) {
    let Ok(device) = runtime.dev_ioport_create(0x3f8, 8) else {
        return;
    };
    for &byte in bytes {
        let _ = runtime.dev_ioport_out(device, 0x3f8, byte);
    }
    let _ = runtime.dev_destroy(device);
    let _ = runtime.cap_drop(device);
}

fn exit_qemu<T: SyscallTransport>(runtime: &Runtime<T>, passed: bool) -> ! {
    if let Ok(device) = runtime.dev_ioport_create(0xf4, 1) {
        let _ = runtime.dev_ioport_out(device, 0xf4, if passed { 16 } else { 17 });
    }
    loop {
        let _ = runtime.yield_task();
    }
}
