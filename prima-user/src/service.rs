//! SPEC-106（S4-D3）：三个**基础服务端点**（日志 / Namespace / 基础存储）的共用骨架。
//!
//! 三者是**各自独立的二进制**（`prima-log`/`prima-ns`/`prima-store`），仅 `NAME`/`KIND`/
//! `MARKER` 三个编译期参数相异——经 `#[path] mod service` 复用本骨架，避免三份近似副本。
//! 每个服务由 ring3 [`prima-servicemgr`](../../servicemgr.rs) 按服务表建立（入口由管理器自解
//! ELF 头得到），启动时向管理器自报：注册名 + 自管理凭据（供管理器回送路由）、以及**编译进自身
//! 二进制**的 40 位存在标记（证明三个服务各由不同程序运行）。
//!
//! 收到管理器的**请求**（[`SERVICE_SERVE_TAG`]）后，各服务按 `op` 语义处理：`ADVANCE` 把 `arg`
//! 并入**自身内部状态**，`QUERY` 只读回当前状态。应答带 `kind`（相异 ⇒ 路由到了正确的服务），
//! 且携带的是**状态**而非入参的回显——纯回声端点无法通过定序「推进/推进/查询」的判决。
//!
//! 服务的最小权限：初始持两条 cap——服务管理器从自身派生的 `TaskSend`（`initial_cap0`，指向
//! 管理器）与非零角色令牌（`initial_cap1`，由代码段派生）。**SPEC-111**：三个服务里只有 Namespace
//! 服务另经 [`SERVICE_NS_CAP_TAG`] 收到一份 `NsResolve` 只读 Namespace 副本（管理器从自身父
//! Namespace 派生，最小权限），据此代理 `SYS_NS_RESOLVE`；其余两个服务始终无 Namespace 权限。
//!
//! **SPEC-112（S4-D3 收口）**：日志/存储服务各自经 [`SERVICE_SEG_TAG`] 收到管理器 `SYS_MEMSEG_CREATE`
//! 铸出、运行期移交的**持久化 MemorySegment**，映射进自身根后据此**落日志/落数据**——`ADVANCE`
//! 把状态写进段、`QUERY` 从段读回（数据路径真正经过内核管理的内存）。Namespace 服务不持段。

use prima_runtime::{
    Capability, Rights, Runtime, SyscallTransport, TaskId,
    protocol::{
        SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_STORE, SERVICE_DATA_VA, SERVICE_FILE_ACCESS_SHIFT,
        SERVICE_FILE_ARG_MASK, SERVICE_FILE_CAPACITY, SERVICE_FILE_CHUNK, SERVICE_FILE_DATA_OFFSET,
        SERVICE_FILE_EAGAIN, SERVICE_FILE_EBADF, SERVICE_FILE_EFAULT, SERVICE_FILE_EINVAL,
        SERVICE_FILE_ENOENT, SERVICE_FILE_ENOSPC, SERVICE_FILE_ENOSYS, SERVICE_FILE_FD_SHIFT,
        SERVICE_FILE_FLAG_TRUNC, SERVICE_FILE_HANDLES, SERVICE_FILE_OP_CLOSE,
        SERVICE_FILE_OP_LSEEK, SERVICE_FILE_OP_OPEN, SERVICE_FILE_OP_READ, SERVICE_FILE_OP_SHIFT,
        SERVICE_FILE_OP_WRITE, SERVICE_FILE_OPEN_FLAGS_SHIFT, SERVICE_FILE_OPEN_PATH_LEN_SHIFT,
        SERVICE_FILE_OPEN_PATH_MASK, SERVICE_FILE_PATH, SERVICE_FILE_REQUEST_TAG,
        SERVICE_FILE_SEEK_OFFSET_MASK, SERVICE_FILE_SEEK_WHENCE_SHIFT, SERVICE_FILE_SIZE_OFFSET,
        SERVICE_FILE_WRITE_COUNT_SHIFT, SERVICE_MARKER_MASK, SERVICE_NAME_MASK, SERVICE_NS_CAP_TAG,
        SERVICE_SEG_TAG, SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_OP_RESOLVE,
        SERVICE_SERVE_OP_SHIFT, SERVICE_SERVE_TAG, SERVICE_WAKE_TAG, pack_file_reply,
        pack_service_marker, pack_service_reply,
    },
};

/// SPEC-111：Namespace 服务代理解析的编译期路径（内核根 Namespace 里由 `init` 绑定，供
/// `SYS_NS_RESOLVE` 命中）。
const SERVICE_RESOLVE_PATH: &str = "svc";
/// SPEC-111：`SYS_NS_RESOLVE` **请求**的权限（`SegMapR`）——须是父项 `svc` 所绑 Capability
/// 权限的子集（`init` 绑定时用 `{Derive, SegMapR}`）；请求超出父项会得 `MissingRight`。
const NS_RESOLVE_REQUESTED: Rights = Rights::from_bits(1 << 1);
/// SPEC-111：代理不可用/失败时的哨兵（高位置 1，与任何合法 Capability 句柄不相交）。
const NS_RESOLVE_FAILED: u64 = u64::MAX;

/// 端点/服务自报的消息 tag（高 16 位；与 `prima-runtime::protocol`/内核/`beta.rs` 同款）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// 派生 `TaskSend` 用（服务→管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// SPEC-112：持久化段的映射权（`SegMapR|SegMapW`）——服务把移交来的段映射进自身根后读写状态。
const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// 引导任务（init）的 task id。对端是它时**不**发无 cap 唤醒。
const BOOTSTRAP_TASK: u32 = 1;

/// 基础服务主体：自报注册名/存在标记，随后应答请求（状态推进/查询，或 Namespace 代理解析）。
pub fn run(
    name: u64,
    kind: u64,
    marker: u64,
    self_cap_raw: u64,
    send_cap_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(peer_raw as u32);
    let (Ok(self_cap), Ok(send_cap)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
    ) else {
        idle();
    };
    if let Ok(id) = runtime.task_id()
        && let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS)
    {
        // 与端点同款：保留先唤醒、再带自管理凭据自报的两消息协议；
        // 阶段 2 直投修复后不再由内核限制要求此顺序。
        if peer.as_raw() != BOOTSTRAP_TASK {
            let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
        }
        let _ = runtime.send(
            peer,
            send_cap,
            SERVICE_ID_TAG | ((name & SERVICE_NAME_MASK) << 8) | (id.as_raw() as u64 & 0xff),
            &[authority],
        );
        // 存在标记：`kind` 归槽、`marker` 逐二进制相异。
        let _ = runtime.send(
            peer,
            send_cap,
            pack_service_marker(kind, marker & SERVICE_MARKER_MASK),
            &[],
        );
    }
    // SPEC-110：服务内部状态——`op=ADVANCE` 请求按 `kind` 语义并入 `arg`，`op=QUERY`（或未知 op）
    // 只读回。三服务的转移语义各异，故同一请求序列得到三个互异的状态值。
    let mut state: u64 = 0;
    // SPEC-111：Namespace 服务管理器**运行期移交**的 `NsResolve` 副本（仅 Namespace 服务会收到；
    // 其余服务永远为 `None`，`op=RESOLVE` 时返回哨兵）。
    let mut ns_cap: Option<Capability> = None;
    // SPEC-112：管理器**运行期移交**的持久化段（仅日志/存储服务会收到）。收到后映射进自身根，
    // 此后状态**落盘于该内核 MemorySegment**——日志服务据此落日志、存储服务据此落数据。
    let mut data_seg: Option<Capability> = None;
    // M4：存储服务的文件句柄表。fd 3..6 一一对应下标 0..3；close 立即释放并可复用。
    let mut files = [FileHandle::EMPTY; SERVICE_FILE_HANDLES];
    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        let tag = message.payload >> 48;
        if tag == SERVICE_NS_CAP_TAG >> 48 {
            // SPEC-111：接管移交的只读 Namespace 副本（带 cap 消息）。
            if let Some(cap) = message.capabilities[0] {
                ns_cap = Some(cap);
            }
        } else if tag == SERVICE_SEG_TAG >> 48 {
            // SPEC-112：接管移交的持久化段并映射进**自身根**（RW）。映射失败则退回纯寄存器语义
            // （不 panic）——但那样管理器侧的内存句柄见证会落空，故失败可见。
            if let Some(seg) = message.capabilities[0]
                && runtime
                    .memseg_map(seg, self_cap, 0, SERVICE_DATA_VA, 1, RW)
                    .is_ok()
            {
                data_seg = Some(seg);
            }
        } else if tag == SERVICE_SERVE_TAG >> 48 {
            let op = (message.payload >> SERVICE_SERVE_OP_SHIFT) & 0xff;
            let result = if op == SERVICE_SERVE_OP_ADVANCE {
                state = advance(kind, state, message.payload & 0xffff_ffff);
                // SPEC-112：落盘——推进后的状态写进移交来的段。
                if data_seg.is_some() {
                    let _ = runtime.write_user_bytes(SERVICE_DATA_VA, &state.to_le_bytes());
                }
                state
            } else if op == SERVICE_SERVE_OP_RESOLVE {
                // SPEC-111：代理一次内核 Namespace 解析——结果由**内核命名空间树**决定，非本服务的
                // 任何编译期常量。未收到 `NsResolve` 副本的服务返回哨兵（回声端点亦不可能产生句柄）。
                ns_cap.map_or(NS_RESOLVE_FAILED, |cap| resolve_namespace(&runtime, cap))
            } else {
                // SPEC-112：查询从**段**读回（而非本地寄存器）——数据路径真正经过内核管理的内存；
                // 无段的服务（Namespace）仍读本地状态，语义不变。
                if data_seg.is_some() {
                    runtime.read_user_u64(SERVICE_DATA_VA).unwrap_or(state)
                } else {
                    state
                }
            };
            let _ = runtime.send(peer, send_cap, pack_service_reply(kind, result), &[]);
        } else if tag == SERVICE_FILE_REQUEST_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M4：文件后端只在存储服务、且持久化段已映射后处理；否则明确拒绝而不是假装成功。
            let response = if data_seg.is_some() {
                file_request(&runtime, &mut files, message.payload)
            } else {
                pack_file_reply(
                    (message.payload >> SERVICE_FILE_OP_SHIFT) & 0xf,
                    (message.payload >> SERVICE_FILE_FD_SHIFT) & 0xf,
                    SERVICE_FILE_ENOSYS,
                    0,
                )
            };
            let _ = runtime.send(peer, send_cap, response, &[]);
        }
    }
}

/// M4：一个打开的文件句柄。offset 归服务端所有（Linux 的共享偏移语义），
/// `access` 为 0=只读、1=只写、2=读写。
#[derive(Clone, Copy)]
struct FileHandle {
    open: bool,
    offset: u64,
    access: u8,
}

impl FileHandle {
    const EMPTY: Self = Self {
        open: false,
        offset: 0,
        access: 0,
    };
}

fn file_size<T: SyscallTransport>(runtime: &Runtime<T>) -> u64 {
    runtime
        .read_user_u64(SERVICE_DATA_VA + SERVICE_FILE_SIZE_OFFSET)
        .unwrap_or(0)
}

fn file_set_size<T: SyscallTransport>(runtime: &Runtime<T>, size: u64) {
    let _ = runtime.write_user_bytes(
        SERVICE_DATA_VA + SERVICE_FILE_SIZE_OFFSET,
        &size.to_le_bytes(),
    );
}

/// M4：按 `fd` 取**已打开**的句柄；fd 3 起，越界/未打开统一 `None`（调用方回 `EBADF`）。
fn file_handle(files: &mut [FileHandle; SERVICE_FILE_HANDLES], fd: u64) -> Option<&mut FileHandle> {
    let index = fd.checked_sub(3)? as usize;
    if index >= SERVICE_FILE_HANDLES || !files[index].open {
        return None;
    }
    Some(&mut files[index])
}

/// M4：处理一条文件请求，返回应答 payload。调用方已确认持久化段可用。
///
/// 单条消息的数据上限是 [`SERVICE_FILE_CHUNK`] 字节；适配层分片后，调用者的
/// `read`/`write` 仍覆盖完整缓冲区。文件内容区从 `SERVICE_DATA_OFFSET` 开始，
/// 偏移 0..8 保留给 SPEC-112 的状态见证。
fn file_request<T: SyscallTransport>(
    runtime: &Runtime<T>,
    files: &mut [FileHandle; SERVICE_FILE_HANDLES],
    payload: u64,
) -> u64 {
    let op = (payload >> SERVICE_FILE_OP_SHIFT) & 0xf;
    let fd = (payload >> SERVICE_FILE_FD_SHIFT) & 0xf;
    let arg = payload & SERVICE_FILE_ARG_MASK;
    let reply = |status: u64, data: u64| pack_file_reply(op, fd, status, data);
    match op {
        SERVICE_FILE_OP_OPEN => {
            let flags = (arg >> SERVICE_FILE_OPEN_FLAGS_SHIFT) & 0xf;
            let path_len = ((arg >> SERVICE_FILE_OPEN_PATH_LEN_SHIFT) & 0x3) as usize;
            let path_bits = arg & SERVICE_FILE_OPEN_PATH_MASK;
            let expected = SERVICE_FILE_PATH.as_bytes();
            if path_len != expected.len() {
                return reply(SERVICE_FILE_ENOENT, 0);
            }
            for (index, byte) in expected.iter().enumerate() {
                if ((path_bits >> (8 * index)) & 0xff) as u8 != *byte {
                    return reply(SERVICE_FILE_ENOENT, 0);
                }
            }
            let access = (flags >> SERVICE_FILE_ACCESS_SHIFT) & 3;
            if access > 2 {
                return reply(SERVICE_FILE_EINVAL, 0);
            }
            // 唯一文件名始终存在：O_CREAT 不需创建动作；O_TRUNC 清空内容。
            if flags & SERVICE_FILE_FLAG_TRUNC != 0 {
                file_set_size(runtime, 0);
            }
            let Some(index) = files.iter().position(|handle| !handle.open) else {
                return reply(SERVICE_FILE_EAGAIN, 0);
            };
            files[index] = FileHandle {
                open: true,
                offset: 0,
                access: access as u8,
            };
            // open 的应答必须携带**新分配的 fd**，而不是请求里的占位 fd=0。
            pack_file_reply(op, index as u64 + 3, 0, file_size(runtime))
        }
        SERVICE_FILE_OP_READ => {
            let Some(handle) = file_handle(files, fd) else {
                return reply(SERVICE_FILE_EBADF, 0);
            };
            if handle.access != 0 && handle.access != 2 {
                return reply(SERVICE_FILE_EBADF, 0);
            }
            let wanted = arg & 0xff;
            let size = file_size(runtime);
            let available = size.saturating_sub(handle.offset);
            let count = wanted.min(SERVICE_FILE_CHUNK).min(available);
            let mut bytes = [0u8; SERVICE_FILE_CHUNK as usize];
            if count != 0
                && runtime
                    .read_user_bytes(
                        SERVICE_DATA_VA + SERVICE_FILE_DATA_OFFSET + handle.offset,
                        &mut bytes[..count as usize],
                    )
                    .is_err()
            {
                return reply(SERVICE_FILE_EFAULT, 0);
            }
            let mut data = 0u64;
            for (index, byte) in bytes[..count as usize].iter().enumerate() {
                data |= (*byte as u64) << (8 * index);
            }
            handle.offset += count;
            reply(0, (count << SERVICE_FILE_WRITE_COUNT_SHIFT) | data)
        }
        SERVICE_FILE_OP_WRITE => {
            let Some(handle) = file_handle(files, fd) else {
                return reply(SERVICE_FILE_EBADF, 0);
            };
            if handle.access != 1 && handle.access != 2 {
                return reply(SERVICE_FILE_EBADF, 0);
            }
            let wanted = (arg >> SERVICE_FILE_WRITE_COUNT_SHIFT) & 0xff;
            let data = arg & 0x00ff_ffff;
            let room = SERVICE_FILE_CAPACITY.saturating_sub(handle.offset);
            let count = wanted.min(SERVICE_FILE_CHUNK).min(room);
            if count == 0 {
                return reply(SERVICE_FILE_ENOSPC, 0);
            }
            let mut bytes = [0u8; SERVICE_FILE_CHUNK as usize];
            for (index, byte) in bytes[..count as usize].iter_mut().enumerate() {
                *byte = ((data >> (8 * index)) & 0xff) as u8;
            }
            if runtime
                .write_user_bytes(
                    SERVICE_DATA_VA + SERVICE_FILE_DATA_OFFSET + handle.offset,
                    &bytes[..count as usize],
                )
                .is_err()
            {
                return reply(SERVICE_FILE_EFAULT, 0);
            }
            handle.offset += count;
            if handle.offset > file_size(runtime) {
                file_set_size(runtime, handle.offset);
            }
            reply(0, count)
        }
        SERVICE_FILE_OP_LSEEK => {
            let Some(handle) = file_handle(files, fd) else {
                return reply(SERVICE_FILE_EBADF, 0);
            };
            let whence = (arg >> SERVICE_FILE_SEEK_WHENCE_SHIFT) & 0x3;
            let raw = (arg & SERVICE_FILE_SEEK_OFFSET_MASK) as i64;
            let offset = if raw & 0x2000_0000 != 0 {
                raw - 0x4000_0000
            } else {
                raw
            };
            let base = match whence {
                0 => 0,
                1 => handle.offset as i64,
                2 => file_size(runtime) as i64,
                _ => return reply(SERVICE_FILE_EINVAL, 0),
            };
            let Some(next) = base.checked_add(offset) else {
                return reply(SERVICE_FILE_EINVAL, 0);
            };
            if next < 0 || next as u64 > SERVICE_FILE_CAPACITY {
                return reply(SERVICE_FILE_EINVAL, 0);
            }
            handle.offset = next as u64;
            reply(0, next as u64)
        }
        SERVICE_FILE_OP_CLOSE => {
            let Some(handle) = file_handle(files, fd) else {
                return reply(SERVICE_FILE_EBADF, 0);
            };
            handle.open = false;
            reply(0, 0)
        }
        _ => reply(SERVICE_FILE_EINVAL, 0),
    }
}

/// SPEC-111：把编译期路径拷到本任务**栈上**（可读用户内存），调内核 `SYS_NS_RESOLVE`。
fn resolve_namespace<T: SyscallTransport>(runtime: &Runtime<T>, ns_cap: Capability) -> u64 {
    let path = SERVICE_RESOLVE_PATH.as_bytes();
    let mut buf = [0u8; 16];
    buf[..path.len()].copy_from_slice(path);
    let Ok(text) = core::str::from_utf8(&buf[..path.len()]) else {
        return NS_RESOLVE_FAILED;
    };
    runtime
        .namespace_resolve(ns_cap, text, NS_RESOLVE_REQUESTED)
        .map_or(NS_RESOLVE_FAILED, |cap| cap.as_raw())
}

/// SPEC-110：各基础服务的状态转移——**语义各异**（同一请求序列 → 三个互异的状态值）。
///
/// - 日志：`state += arg`（累计写入字节数）
/// - 存储：`state = (state << 8) | (arg & 0xff)`（追加低字节，像移位写入）
/// - Namespace：`state = (state << 4) ^ arg`（滚动指纹）
fn advance(kind: u64, state: u64, arg: u64) -> u64 {
    match kind {
        SERVICE_BEHAVIOR_LOG => state.wrapping_add(arg),
        SERVICE_BEHAVIOR_STORE => (state << 8) | (arg & 0xff),
        _ => (state << 4) ^ arg,
    }
}

fn idle() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    idle()
}
