//! WIN-M2 ring3 原型：在 Prima 内装载并运行一个原样 PE32+ 控制台程序。
//!
//! 只在 `winpe-probe` feature 且构建时设置 `PRIMA_WINPE_PROBE` 时由 servicemgr 调用。
//! 装载器在用户态完成：真实 PE 夹具经本 crate 的 PE 解析/平铺/重定位后写入自建 Segment，
//! 作为 held Task 的代码段映射为 R+X；同一次装载还把真实 `winpe-stub.dll` 的导出表
//! 解析为 EXE 的 IAT 目标，DLL 映像另段映射为 R+X；栈、结果页与退出桩另段映射。PE 通过
//! 真实 DLL 导出返回 42，退出桩把 42 写入共享结果页并自毁。装载前先跑一个“发布失败”
//! 负例：在子地址空间已建立可执行映像后强制发布失败，验证暂存映射被回收且同一 VA 可再
//! 装载。没有内核 PE 识别、没有 W+X 映射，也没有 Windows 参考差分。

use crate::pe::{ExportTarget, ImportThunk, PeImage};
use prima_runtime::{Capability, Error, Result, Rights, Runtime, SyscallTransport, TaskSpec};

const WINPE_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/winpe-probe.exe"));
const WINPE_DLL_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/winpe-stub.dll"));

const DLL_NAME: &[u8] = b"winpe-stub.dll";
const EXPORT_NAME: &[u8] = b"Stub";

const MAX_IMAGE: usize = 0x5000;
const IMAGE_PAGES: u64 = 5;
const DLL_MAX_IMAGE: usize = 0x3000;
const DLL_PAGES: u64 = 3;

const PREFERRED_BASE: u64 = 0x1_4000_0000;
const RELOCATED_BASE: u64 = 0x1_5000_0000;
/// `lld-link /dll` 的默认映像基址；DLL 无重定位，按首选基址映射。
const DLL_BASE: u64 = 0x1_8000_0000;

const PE_STAGE_VA: u64 = 0x0000_0A60_0000_0000;
const STACK_STAGE_VA: u64 = 0x0000_0A61_0000_0000;
const RESULT_STAGE_VA: u64 = 0x0000_0A62_0000_0000;
const STUB_STAGE_VA: u64 = 0x0000_0A63_0000_0000;
const DLL_STAGE_VA: u64 = 0x0000_0A64_0000_0000;

const CHILD_STACK_VA: u64 = 0x1_6000_0000;
const CHILD_RESULT_VA: u64 = 0x1_7000_0000;
const CHILD_EXIT_STUB_VA: u64 = 0x1_7000_2000;
const STACK_PAGES: u64 = 4;
const FRAME_SIZE: u64 = 4096;

const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
const RX: Rights = Rights::from_bits((1 << 1) | (1 << 3));
const RWX: Rights = Rights::from_bits((1 << 1) | (1 << 2) | (1 << 3));
const NS_RESOLVE: Rights = Rights::from_bits(1 << 8);

/// 串口见证行；xtask 只认这些完整行（`stage=` 行被忽略）。
const NEGATIVE_LINE: &[u8] = b"WINPE|unpublish=clean\n";
const DLL_LINE: &[u8] = b"WINPE|dll=winpe-stub.dll!Stub\n";
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
    let dll = PeImage::parse(WINPE_DLL_BYTES).map_err(|_| Error::InvalidInput)?;
    if pe.size_of_image() as usize > MAX_IMAGE || dll.size_of_image() as usize > DLL_MAX_IMAGE {
        return Err(Error::InvalidInput);
    }
    let export = dll
        .export_by_name(EXPORT_NAME)
        .map_err(|_| Error::InvalidInput)?
        .ok_or(Error::InvalidInput)?;
    let ExportTarget::Function { rva } = export.target() else {
        return Err(Error::InvalidInput);
    };
    let stub_va = DLL_BASE + u64::from(rva);
    serial(runtime, b"WINPE|stage=parsed\n");

    let namespace = runtime.namespace_create()?;
    let self_cap = runtime.self_capability()?;
    serial(runtime, b"WINPE|stage=namespace\n");

    run_publish_failure(runtime, self_cap, namespace, pe, stub_va)?;
    serial(runtime, NEGATIVE_LINE);
    serial(runtime, DLL_LINE);

    let relocated = run_child(
        runtime,
        self_cap,
        namespace,
        pe,
        dll,
        stub_va,
        RELOCATED_BASE,
    )?;
    serial(runtime, b"WINPE|stage=child1\n");
    let preferred = run_child(
        runtime,
        self_cap,
        namespace,
        pe,
        dll,
        stub_va,
        PREFERRED_BASE,
    )?;
    serial(runtime, b"WINPE|stage=child2\n");

    let _ = runtime.namespace_destroy(namespace);
    let _ = runtime.cap_drop(namespace);

    if relocated == 42 && preferred == 42 {
        Ok(())
    } else {
        Err(Error::InvalidResponse)
    }
}

/// 把一个 EXE 导入解析为真实 DLL 的导出地址；其他 DLL 或按序号导入一律拒绝。
fn resolve_import(stub_va: u64, dll: &[u8], thunk: ImportThunk<'_>) -> Option<u64> {
    match thunk {
        ImportThunk::Name { name, .. } if dll == DLL_NAME && name == EXPORT_NAME => Some(stub_va),
        _ => None,
    }
}

/// 发布失败负例：子地址空间已建立可执行映像后强制发布失败，清理后再验证暂存 VA 已释放。
fn run_publish_failure<T: SyscallTransport>(
    runtime: &Runtime<T>,
    self_cap: Capability,
    namespace: Capability,
    pe: PeImage<'static>,
    stub_va: u64,
) -> Result<()> {
    let image_segment = runtime.memseg_create(IMAGE_PAGES, RWX)?;
    serial(runtime, b"WINPE|stage=neg-segment\n");
    let mut held = None;
    let staged = (|| -> Result<()> {
        runtime.memseg_map(image_segment, self_cap, 0, PE_STAGE_VA, IMAGE_PAGES, RW)?;
        serial(runtime, b"WINPE|stage=neg-staged\n");
        let buffer = prima_ostd::mm::bytes_mut_at(PE_STAGE_VA, MAX_IMAGE);
        let loaded = pe
            .load_into(RELOCATED_BASE, buffer, |dll, thunk| {
                resolve_import(stub_va, dll, thunk)
            })
            .map_err(|_| Error::InvalidInput)?;
        serial(runtime, b"WINPE|stage=neg-loaded\n");
        // `task_create_held` 在子地址空间建立 R+X 代码映射；随后以未对齐目标强制
        // 发布失败，确保失败时确有可执行映射需要回收。
        let manager = match runtime.task_create_held(&TaskSpec {
            code_segment: image_segment,
            code_offset: 0,
            code_pages: IMAGE_PAGES,
            load_base: RELOCATED_BASE,
            entry: loaded.entry_va(),
            stack_top: CHILD_STACK_VA,
            parent_namespace: Some(namespace),
            root_rights: NS_RESOLVE,
            initial_capabilities: [None, None],
            name: 0,
        }) {
            Ok(manager) => manager,
            Err(error) => {
                let mut line = *b"WINPE|stage=neg-create=00\n";
                let code = error_tag(error);
                line[23] = b'0' + code / 10;
                line[24] = b'0' + code % 10;
                serial(runtime, &line);
                return Err(error);
            }
        };
        held = Some(manager);
        serial(runtime, b"WINPE|stage=neg-held\n");
        if runtime
            .memseg_map(image_segment, manager, 0, CHILD_STACK_VA + 1, 1, RX)
            .is_ok()
        {
            return Err(Error::InvalidResponse);
        }
        serial(runtime, b"WINPE|stage=neg-refused\n");
        Ok(())
    })();

    let _ = runtime.memseg_unmap(self_cap, PE_STAGE_VA, IMAGE_PAGES);
    if let Some(manager) = held.take() {
        let _ = runtime.task_destroy(manager);
        let _ = runtime.cap_drop(manager);
    }
    let _ = runtime.memseg_destroy(image_segment);
    let _ = runtime.cap_drop(image_segment);
    serial(runtime, b"WINPE|stage=neg-cleaned\n");
    staged?;
    serial(runtime, b"WINPE|stage=neg-unmapped\n");

    // 暂存 VA 已无映射：重复 unmap 必须失败（内核逐页返回 NotMapped）。
    if runtime
        .memseg_unmap(self_cap, PE_STAGE_VA, IMAGE_PAGES)
        .is_ok()
    {
        return Err(Error::InvalidResponse);
    }
    Ok(())
}

fn run_child<T: SyscallTransport>(
    runtime: &Runtime<T>,
    self_cap: Capability,
    namespace: Capability,
    pe: PeImage<'static>,
    dll: PeImage<'static>,
    stub_va: u64,
    base: u64,
) -> Result<u64> {
    let mut segments = [None; 5];
    let mut held = None;
    let outcome = (|| -> Result<u64> {
        segments[0] = Some(runtime.memseg_create(IMAGE_PAGES, RWX)?);
        segments[1] = Some(runtime.memseg_create(DLL_PAGES, RWX)?);
        segments[2] = Some(runtime.memseg_create(STACK_PAGES, RW)?);
        segments[3] = Some(runtime.memseg_create(1, RW)?);
        segments[4] = Some(runtime.memseg_create(1, RWX)?);
        let (image_segment, dll_segment, stack_segment, result_segment, stub_segment) = (
            segments[0].unwrap(),
            segments[1].unwrap(),
            segments[2].unwrap(),
            segments[3].unwrap(),
            segments[4].unwrap(),
        );
        serial(runtime, b"WINPE|stage=segments\n");

        runtime.memseg_map(image_segment, self_cap, 0, PE_STAGE_VA, IMAGE_PAGES, RW)?;
        runtime.memseg_map(dll_segment, self_cap, 0, DLL_STAGE_VA, DLL_PAGES, RW)?;
        runtime.memseg_map(stack_segment, self_cap, 0, STACK_STAGE_VA, STACK_PAGES, RW)?;
        runtime.memseg_map(result_segment, self_cap, 0, RESULT_STAGE_VA, 1, RW)?;
        runtime.memseg_map(stub_segment, self_cap, 0, STUB_STAGE_VA, 1, RW)?;
        serial(runtime, b"WINPE|stage=mapped\n");

        serial(runtime, b"WINPE|stage=load\n");
        let buffer = prima_ostd::mm::bytes_mut_at(PE_STAGE_VA, MAX_IMAGE);
        let loaded = pe
            .load_into(base, buffer, |dll_name, thunk| {
                resolve_import(stub_va, dll_name, thunk)
            })
            .map_err(|_| Error::InvalidInput)?;
        if loaded.imports_resolved() != 1 {
            return Err(Error::InvalidInput);
        }
        let dll_buffer = prima_ostd::mm::bytes_mut_at(DLL_STAGE_VA, DLL_MAX_IMAGE);
        let dll_loaded = dll
            .load_into(DLL_BASE, dll_buffer, |_, _| None)
            .map_err(|_| Error::InvalidInput)?;
        if dll_loaded.base() != DLL_BASE {
            return Err(Error::InvalidInput);
        }
        serial(runtime, b"WINPE|stage=loaded\n");

        runtime.write_user_bytes(
            STACK_STAGE_VA + STACK_PAGES * FRAME_SIZE - 8,
            &CHILD_EXIT_STUB_VA.to_le_bytes(),
        )?;
        runtime.write_user_bytes(RESULT_STAGE_VA, &0u64.to_le_bytes())?;
        runtime.write_user_bytes(STUB_STAGE_VA, &exit_stub())?;

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
        held = Some(manager);

        runtime.memseg_map(stack_segment, manager, 0, CHILD_STACK_VA, STACK_PAGES, RW)?;
        runtime.memseg_map(result_segment, manager, 0, CHILD_RESULT_VA, 1, RW)?;
        runtime.memseg_map(dll_segment, manager, 0, DLL_BASE, DLL_PAGES, RX)?;
        runtime.memseg_map(stub_segment, manager, 0, CHILD_EXIT_STUB_VA, 1, RX)?;
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
        held = None;
        let _ = runtime.cap_drop(manager);

        if value == 0 {
            Err(Error::Again)
        } else {
            Ok(value)
        }
    })();

    let _ = runtime.memseg_unmap(self_cap, PE_STAGE_VA, IMAGE_PAGES);
    let _ = runtime.memseg_unmap(self_cap, DLL_STAGE_VA, DLL_PAGES);
    let _ = runtime.memseg_unmap(self_cap, STACK_STAGE_VA, STACK_PAGES);
    let _ = runtime.memseg_unmap(self_cap, RESULT_STAGE_VA, 1);
    let _ = runtime.memseg_unmap(self_cap, STUB_STAGE_VA, 1);
    if let Some(manager) = held.take() {
        let _ = runtime.task_destroy(manager);
        let _ = runtime.cap_drop(manager);
    }
    for segment in segments.into_iter().flatten() {
        let _ = runtime.memseg_destroy(segment);
        let _ = runtime.cap_drop(segment);
    }
    outcome
}

fn error_tag(error: Error) -> u8 {
    match error {
        Error::NotFound => 1,
        Error::Again => 2,
        Error::Denied => 3,
        Error::Fault => 4,
        Error::AlreadyExists => 5,
        Error::NotDirectory => 6,
        Error::InvalidInput => 7,
        Error::ResourceExhausted => 8,
        Error::Unsupported => 9,
        Error::MalformedCapability => 10,
        Error::InvalidResponse => 11,
        Error::Interrupted => 12,
        Error::Transport(_) => 13,
        Error::UnknownErrno(_) => 14,
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
