//! SPEC-104/105/106：**ring3 服务管理器**——运行期注册表、按名解析、请求路由/转发，以及
//! **表驱动的端点创建**的宿主。
//!
//! 决策见 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`：把服务发现、
//! 路由与端点创建从 `init` 的手写 x86 机器码迁到本独立 ring3 任务。本映像**不含任何服务名字面量
//! 之外的对端/端点 task id**——端点与客户端都以本管理器为对端（对端 id 由创建时经入口 `r8`
//! 运行期指派），本管理器按自报消息里的名字建注册表、按客户端写入共享段的请求名解析、并把
//! `prepare` 与共享段 cap 转发给解析到的端点。
//!
//! **SPEC-105**：端点不再由 `init` 的手写汇编创建。`init` 只把建端点所需的 cap 移交本管理器
//! （父 Namespace + 每服务的代码段/栈段），本管理器按 [`SERVICE_TABLE`] 表驱动地
//! `task_create` + 映射栈；入口地址由本管理器**自行解析代码段 ELF 头的 `e_entry`** 得到。
//!
//! **SPEC-106（S4-D3）**：服务表从 2 项泛化为 [`SERVICES`] 项，新增三个**基础服务**端点
//! （日志 / Namespace / 基础存储，各为独立二进制 `prima-log`/`prima-ns`/`prima-store`）。三者与
//! `ALPHA`/`BETA` 走同一条表驱动创建路径，但不参与 wire 协商：各自向本管理器自报**存在标记**
//! （`kind` 归槽 + 每二进制相异的 40 位值），随后本管理器向三者各发一条**路由探针**并收回应答。
//! 注册名、存在标记与探针应答写回共享段，构成「三服务由本管理器按表建立、且请求路由到了正确的
//! 服务二进制」的逐字段见证。
//!
//! **SPEC-111/112（S4-D3 收口）**：本管理器另向三服务**运行期移交内核权限**——Namespace 服务收到
//! 一份 `NsResolve` 只读副本（代理 `SYS_NS_RESOLVE`）；日志/存储服务各收到一个 `SYS_MEMSEG_CREATE`
//! 铸出的**持久化 MemorySegment**（落日志/落数据）。两者的句柄写回共享段，供内核侧逐字段核验。
//!
//! **SPEC-114（S4-D4）**：服务表新增第 6 项——**用户态驱动** `prima-driver`（注册名 `ioprt`）。
//! 它与三个基础服务走同一条表驱动创建路径，但不参与 §15 的定序状态探针：本管理器
//! `SYS_DEV_IOPORT_CREATE` 铸出一个覆盖 COM1 的 `Device(IOPortRange)` Capability，经
//! `SERVICE_DEV_CAP_TAG` **运行期移交**该驱动，随后发两条受控端口请求（区间内往返 / 区间外探测），
//! 应答写回共享段供内核逐字段核验。驱动的 id 最大（末位创建）而在最小 id 优先的调度器上优先级
//! 最低，故收尾条件必须等待它的应答（见下方 ⑥ 的说明），否则它永不被调度。
//!
//! 与本映像一起构成判决性见证的，是本管理器写回**客户端共享段**固定偏移的自证字段（自身 task
//! id + 二进制标记）与解析结果——内核见证按共享段偏移读回，故「注册表/解析由独立 ring3 映像
//! 产出」是逐字节可核验的，而非仅凭 init 的回执。
//!
//! 消息处理是**顺序无关的事件循环**：客户端的 Hello 与各端点自报、done、标记与探针应答、以及
//! `init` 的建端点移交可按任意次序到达，状态齐全时才推进下一阶段。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Error, Rights, Runtime, SyscallTransport, TaskId,
    protocol::{
        ELF_ENTRY_OFFSET, ENDPOINT_CODE_PROBE_VA, ENDPOINT_CODE_VA, ENDPOINT_STACK_VA, IOPORT_BASE,
        IOPORT_COUNT, NATIVE_APP_BAD_LENGTH, NATIVE_APP_BYTES, NATIVE_APP_CANCELLED,
        NATIVE_APP_DATA_VA, NATIVE_APP_DONE_TAG, NATIVE_APP_OK, NATIVE_APP_OP_CANCEL,
        NATIVE_APP_OP_DATA, NATIVE_APP_OP_EXECUTE, NATIVE_APP_OP_SUBMIT, NATIVE_APP_QUEUED,
        NATIVE_APP_REQUEST_TAG, SERVICE_BEHAVIOR_ALPHA, SERVICE_BEHAVIOR_BETA,
        SERVICE_BEHAVIOR_DRIVER, SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_NONE, SERVICE_BEHAVIOR_NS,
        SERVICE_BEHAVIOR_STORE, SERVICE_DEV_CAP_TAG, SERVICE_ECHO_ARGS_OFFSET,
        SERVICE_FILE_CLIENT_SHIFT, SERVICE_FILE_ENOENT, SERVICE_FILE_FD_SHIFT,
        SERVICE_FILE_OP_OPEN, SERVICE_FILE_OP_SHIFT, SERVICE_FILE_REPLY_TAG,
        SERVICE_FILE_REQUEST_TAG, SERVICE_IMAGE_DIR_HEADER, SERVICE_IMAGE_ENTRY_LEN,
        SERVICE_IMAGE_KIND_C_PROBE, SERVICE_IMAGE_KIND_ECHO_ARGS, SERVICE_IMAGE_KIND_SERVICEMGR,
        SERVICE_IMAGE_KIND_TTY, SERVICE_IMAGE_MAGIC, SERVICE_MARKER_KIND_SHIFT,
        SERVICE_MARKER_MASK, SERVICE_MARKER_TAG, SERVICE_NAME_ALPHA, SERVICE_NAME_BETA,
        SERVICE_NAME_DRIVER, SERVICE_NAME_LOG, SERVICE_NAME_MASK, SERVICE_NAME_NS,
        SERVICE_NAME_OFFSET, SERVICE_NAME_STORE, SERVICE_NS_CAP_TAG, SERVICE_SEG_TAG,
        SERVICE_SERVE_ARG_A, SERVICE_SERVE_ARG_B, SERVICE_SERVE_OP_ADVANCE,
        SERVICE_SERVE_OP_IOPORT_OOR, SERVICE_SERVE_OP_IOPORT_RW, SERVICE_SERVE_OP_QUERY,
        SERVICE_SERVE_OP_RESOLVE, SERVICE_SERVE_PROBES, SERVICE_SERVE_TAG, SERVICE_SETUP_TAG,
        SERVICE_VFS_COMMIT_CLIENT_SHIFT, SERVICE_VFS_COMMIT_TAG, SERVICE_VFS_MUTATE_TAG,
        SERVICE_VFS_OPEN_TAG, SERVICE_VFS_PATH_TAG, SERVICE_VFS_ROOT_PATH,
        SERVICE_VMA_CAP_REQUEST_TAG, SERVICE_VMA_CLIENT_SHIFT, SERVICE_WAKE_TAG,
        SERVICEMGR_HELLO_TAG, STAGE5_C_PROBE_TAG, STAGE5_ECHO_WRITE_TAG, STAGE5_TTY_ACK_TAG,
        STAGE5_TTY_READY_TAG, STAGE5_TTY_SYNC_ACK_TAG, STAGE5_TTY_SYNC_TAG, STAGE5_TTY_WRITE_TAG,
        native_app_hash, native_app_reply, pack_file_reply, pack_service_name,
        pack_service_request, pack_vma_cap_reply,
    },
    task::TaskSpec,
    trace::{
        EVENT_NATIVE_APP, EVENT_SERVICE_NEGOTIATED, EVENT_SERVICE_REPLY, EVENT_SERVICE_SELECTED,
        EVENT_SERVICEMGR, EVENT_STAGE5_CAPACITY,
    },
};

/// 客户端把共享段映射到的 VA（与本进程自身根内的同一物理帧映射目标；与 `prima-user` 一致）。
const USER_SEG_VA: u64 = 0x0000_0A00_0000_7000;
const TRACE_BASE: u64 = USER_SEG_VA + prima_runtime::trace::SHARED_OFFSET;

fn trace_emit(
    runtime: &Runtime<impl SyscallTransport>,
    id: u64,
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    e: u64,
) -> prima_runtime::Result<()> {
    runtime.trace_append_shared(TRACE_BASE, [id, a, b, c, d, e])
}
/// 阶段 5 片 C：经受控端口路径把见证行写到 COM1。
fn serial_puts(
    runtime: &Runtime<impl SyscallTransport>,
    cap: Capability,
    bytes: &[u8],
) -> prima_runtime::Result<()> {
    for &byte in bytes {
        runtime.dev_ioport_out(cap, IOPORT_BASE, byte)?;
    }
    Ok(())
}

fn serial_dec(
    runtime: &Runtime<impl SyscallTransport>,
    cap: Capability,
    mut value: u64,
) -> prima_runtime::Result<()> {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    loop {
        index -= 1;
        buf[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    serial_puts(runtime, cap, &buf[index..])
}

/// 客户端把 `PONG` 的 payload 写回共享段内的偏移（与 `prima-user`/内核一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
/// 协商结果/负例码在共享段内的偏移（与 `prima-user/src/main.rs` 一致）。
const SERVICE_NEGOTIATED_OFFSET: u64 = 128;
const SERVICE_NEGATIVES_OFFSET: u64 = 136;
const SERVICE_NEGOTIATED_B_OFFSET: u64 = 144;
const SERVICE_ALPHA_MARKER_OFFSET: u64 = 176;
const SERVICE_BETA_MARKER_OFFSET: u64 = 184;
const SERVICE_ENDPOINT_ALPHA_OFFSET: u64 = 160;
const SERVICE_ENDPOINT_BETA_OFFSET: u64 = 168;
/// EXO-3H：在 servicemgr 内读回自己映射的客户端共享段偏移；内核不再解释这些偏移。
fn read_shared(runtime: &Runtime<impl SyscallTransport>, offset: u64) -> u64 {
    runtime.read_user_u64(USER_SEG_VA + offset).unwrap_or(0)
}

fn read_native_app_hash(runtime: &Runtime<impl SyscallTransport>) -> Option<u32> {
    let mut chunk = [0u8; 64];
    let mut hash = 2_166_136_261u32;
    let mut offset = 0usize;
    while offset < NATIVE_APP_BYTES {
        runtime
            .read_user_bytes(NATIVE_APP_DATA_VA + offset as u64, &mut chunk)
            .ok()?;
        for byte in chunk {
            hash = native_app_hash(hash, byte);
        }
        offset += chunk.len();
    }
    Some(hash)
}

/// 端点用户栈的页大小。
const FRAME_SIZE: u64 = 0x1000;
/// 普通服务端点的栈页数。M8 保持既有 1 页布局，并把 VFS 解析栈帧压回该预算
/// （名称 ≤16、分量 ≤6）；没有改用 2 页。
const SERVICE_STACK_PAGES: u64 = 1;
/// C 入口栈页数：musl 的 `__libc_start_main` 与 `snprintf` 调用链合计超过 4 页；
/// 8 页容纳启动初始化与格式化路径，同时把映射条目控制在新的 vspace 预算内。
const C_STACK_PAGES: u64 = 8;
/// C 入口栈顶从最高页的页内 0xf00 开始，保持 16 字节对齐。
const C_STACK_OFFSET: u64 = C_STACK_PAGES * FRAME_SIZE - 0x100;
/// M3b/M9：C 任务的匿名内存 arena（96 页 RW），与栈**同一 Segment** 的不同偏移。
///
/// 适配层按固定 VA bump 分配（首 40 页 brk、其余 56 页 mmap），因此映射必须在任务运行
/// 前就位。与栈合并创建可复用同一次 `memseg_create`——C 探针运行期的容量快照已是
/// segment 41/48、audit 757/768，不能为堆再增一次审计与段槽。
/// **M9**：从 64 页扩到 96 页——sbase `ls` 启动时按 `PATH_MAX` 分配 64 KiB 的 visited
/// 表，64 页布局的 16 页 brk 无法容纳；brk 扩到 40 页，mmap 区保留 56 页供 140000
/// 字节的独立映射。该常量必须与 `prima_syscall.c` 的 `PRIMA_HEAP_VA/PRIMA_HEAP_PAGES`
/// 一致。
const C_HEAP_VA: u64 = 0x0000_0C00_0000_0000;
const C_HEAP_PAGES: u64 = 96;
/// 只读 + 可写映射权（`SegMapR|SegMapW`）。
const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR|SegMapW` + `Derive`（`memseg_map` 前置要求段 cap 含 `Derive`）。
const RW_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1) | (1 << 2));
/// 段 cap 的子集：`SegMapR` + `Derive`（交给 init 映射只读）。
const R_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 1));
/// 只读映射权（`SegMapR`）——自解 ELF 入口与 endpoint 令牌用。
const R_ONLY: Rights = Rights::from_bits(1 << 1);

/// ELF64 头/程序头字段偏移（相对 ELF 基址）。
const ELF_PHOFF: u64 = 32;
const ELF_PHENTSIZE: u64 = 54;
const ELF_PHNUM: u64 = 56;
const ELF_PH_SIZE: u64 = 56;
const PT_LOAD: u32 = 1;
const PF_W: u32 = 2;
/// 私有数据段的临时映射 VA（远高于探测映射，避免与任何端点/用户段相交）。
const DATA_PROBE_VA: u64 = 0x0000_0A20_0000_0000;
/// 单个 ELF 支持的可写 `PT_LOAD` 数（固定探针与 musl 程序都远小于此）。
const WRITABLE_SEGMENTS: usize = 4;

/// ELF 中一个可写 `PT_LOAD` 的页对齐范围。
#[derive(Clone, Copy)]
struct WritableSegment {
    /// 页对齐起始（相对 ELF 基址）。
    start: u64,
    pages: u64,
    /// 段内首字节在映像中的 VA：装载器按 vaddr 打包并已应用相对重定位，
    /// 故复制源和目标都必须以 vaddr 定位，不能用 ELF `p_offset`。
    vaddr: u64,
    /// 非 BSS 字节数（`p_filesz`）；其余部分由新段清零。
    file_size: u64,
}
/// 端点初始帧 `initial_cap0` 的权利（仅 `TaskSend`；指向本管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// M4：绑定到 Namespace 路径的文件访问令牌。`NsResolve` 要求被绑定项含 `Derive`
/// 才能派生子集，故令牌本身除 `TaskSend` 外必须带 `Derive`。
const TASK_SEND_DERIVE: Rights = Rights::from_bits((1 << 0) | (1 << 5));
/// 新端点结构性根的 Namespace 权利（仅 `NsResolve`）。
const NS_RESOLVE_RIGHTS: Rights = Rights::from_bits(1 << 8);
/// M10：交给 C 任务的 arena 映射授权（`Derive|SegMapR|SegMapW`，不含 `SegMapX`）。
/// 只允许对同一 Segment 重映射/改权，不允许执行，也不含 `SegDestroy`。
const ARENA_MAP_RIGHTS: Rights = Rights::from_bits((1 << 0) | (1 << 1) | (1 << 2));

/// 端点自报 id 的消息 tag（高 16 位；与 `prima-runtime::protocol`/内核一致）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// 端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
/// 转发 `prepare` 的消息 tag；payload 低 8 位携带路由到的行为码。
const SERVICE_PREPARE_TAG: u64 = 0x5EC3_0000_0000_0000;
/// 应答客户端的 payload（`"PONG"` ‖ opcode ‖ request_id；与内核 `SERVICE_REPLY_WORD` 一致）。
const SERVICE_REPLY_WORD: u64 = 0x0003_8001_474E_4F50;
/// 未注册名（解析拒绝负例的目标）。
const SERVICE_NAME_UNKNOWN: u64 = pack_service_name(b"ghost");

/// 管理器 → init 的控制消息 tag（`0x5E56` 是存活自报；以下三条互异、且与端点 tag 不相交）。
const SERVICEMGR_WAKE_TAG: u64 = 0x5E57_0000_0000_0000;
const SERVICEMGR_SEG_TAG: u64 = 0x5E58_0000_0000_0000;
const SERVICEMGR_FINISH_TAG: u64 = 0x5E59_0000_0000_0000;

/// 本映像编译进二进制的标记（见证按共享段偏移读回，证明字段来自**本独立映像**）。
const SERVICEMGR_BINARY_MARKER: u64 = 0x5345_5256_4D47_5203;

/// 管理器写回客户端共享段的固定偏移（内核 `user.rs` 侧同款）。
const OFF_SERVICEMGR_ID: u64 = 192;
const OFF_SERVICEMGR_MARKER: u64 = 200;
const OFF_RESOLVED: u64 = 208;
const OFF_UNKNOWN: u64 = 216;
const OFF_DONE_BETA: u64 = 224;
const OFF_REG_A_NAME: u64 = 232;
const OFF_REG_B_NAME: u64 = 240;
const OFF_BETA_SELF: u64 = 248;
const OFF_DONE_ALPHA: u64 = 256;
const OFF_CLIENT_ID: u64 = 264;
/// SPEC-105：见证管理器**确实创建了**端点及其收到的建端点参数。
const OFF_MGR_CREATED: u64 = 272;
const OFF_MGR_TOKEN: u64 = 280;
const OFF_MGR_MANAGER: u64 = 288;
/// SPEC-105：两个端点自解出的 ELF 入口（load_base 相对值），证明入口来自**本管理器的解析**。
const OFF_MGR_ENTRY_A: u64 = 304;
const OFF_MGR_ENTRY_B: u64 = 312;
/// SPEC-106：三个基础服务的 (id, 存在标记, 探针应答)，各占 8 B；以及已应答的基础服务数。
const OFF_LOG_ID: u64 = 320;
const OFF_LOG_MARK: u64 = 328;
const OFF_NS_ID: u64 = 336;
const OFF_NS_MARK: u64 = 344;
const OFF_STORE_ID: u64 = 352;
const OFF_STORE_MARK: u64 = 360;
const OFF_SERVICES: u64 = 368;
/// SPEC-110：每个基础服务的**定序状态见证**占 3 槽（推进 A / 推进 B / 查询），共 24 B。
const OFF_LOG_REPLY: u64 = 376;
const OFF_NS_REPLY: u64 = 400;
const OFF_STORE_REPLY: u64 = 424;
/// SPEC-111：Namespace 服务**代理解析**的内核返回句柄（第 4 条应答，单独一槽）。
const OFF_NS_RESOLVED: u64 = 448;
/// SPEC-112（S4-D3 收口）：日志/存储服务的**持久化段**内核句柄（管理器 `SYS_MEMSEG_CREATE` 铸出，
/// 运行期移交服务）。内核侧据此核验「确为活的内核 Segment 对象」。
const OFF_LOG_SEG: u64 = 456;
const OFF_STORE_SEG: u64 = 464;
/// SPEC-114（S4-D4）：用户态驱动的 (id, 存在标记, 内核设备句柄)；受控端口的两条应答各占 8 B。
const OFF_DRIVER_ID: u64 = 472;
const OFF_DRIVER_MARK: u64 = 480;
const OFF_DRIVER_DEV: u64 = 488;
const OFF_DRIVER_REPLY: u64 = 496;

/// 服务表条目数（`ALPHA`/`BETA` + 日志/Namespace/存储三个基础服务 + SPEC-114 的用户态驱动）。
const SERVICES: usize = 6;
/// 基础服务的表下标起点（`SERVICE_BEHAVIOR_LOG` = 3 ⇒ 下标 2）。
const BASE_FIRST: usize = 2;
const LOG_SLOT: usize = BASE_FIRST;
const STORE_SLOT: usize = BASE_FIRST + 2;
/// SPEC-114：**基础服务**（日志/Namespace/存储）的表项数——§15 的定序状态探针**只打这三个**，
/// 用户态驱动走自己的受控端口握手。
const BASE_COUNT: usize = 3;
/// SPEC-111：Namespace 服务的表下标（`SERVICE_BEHAVIOR_NS` = 4 ⇒ 下标 3）——三个基础服务里唯一
/// 代理内核机制者，故比其余两个**多一条**代理解析请求。
const NS_SLOT: usize = 3;
/// SPEC-114：用户态驱动的表下标（`SERVICE_BEHAVIOR_DRIVER` = 6 ⇒ 下标 5）。
const DRIVER_SLOT: usize = 5;
/// SPEC-114：驱动的受控端口请求条数（区间内往返 + 区间外探测）。
const DRIVER_PROBES: u64 = 2;

/// SPEC-111/114：某端点收到的**应答**条数（Namespace 服务多一条 `RESOLVE`；驱动两条端口探测）。
const fn probes_for(slot: usize) -> u64 {
    if slot == DRIVER_SLOT {
        DRIVER_PROBES
    } else if slot == NS_SLOT {
        SERVICE_SERVE_PROBES + 1
    } else {
        SERVICE_SERVE_PROBES
    }
}

/// 注册表条目：端点名 → 行为码、task id、发送凭据。
#[derive(Clone, Copy)]
struct Endpoint {
    id: u64,
    authority: Capability,
    behavior: u64,
}

/// SPEC-105/108：待创建的服务（`init` 移交的**代码段** cap + 运行期参数）。
///
/// SPEC-108：**栈段**不再由 `init` 移交——[`create_service`] 用 `SYS_MEMSEG_CREATE` 自建。
#[derive(Clone, Copy)]
struct Pending {
    code: Capability,
    offset_pages: u64,
    pages: u64,
    name: u64,
}

/// SPEC-105/106：**服务表**——名字 → 行为码。加一个服务 = 表里一行 + 一个二进制 + init 移交一对
/// cap（「init 零改动」尚未做到，见报告登记）。
const SERVICE_TABLE: [(u64, u64); SERVICES] = [
    (SERVICE_NAME_ALPHA, SERVICE_BEHAVIOR_ALPHA),
    (SERVICE_NAME_BETA, SERVICE_BEHAVIOR_BETA),
    (SERVICE_NAME_LOG, SERVICE_BEHAVIOR_LOG),
    (SERVICE_NAME_NS, SERVICE_BEHAVIOR_NS),
    (SERVICE_NAME_STORE, SERVICE_BEHAVIOR_STORE),
    (SERVICE_NAME_DRIVER, SERVICE_BEHAVIOR_DRIVER),
];

/// 名字 → 行为码（未注册 → `NONE`）。表驱动：SPEC-105 起扫描 [`SERVICE_TABLE`]。
fn behavior_for(name: u64) -> u64 {
    let mut index = 0;
    while index < SERVICES {
        if SERVICE_TABLE[index].0 == name {
            return SERVICE_TABLE[index].1;
        }
        index += 1;
    }
    SERVICE_BEHAVIOR_NONE
}

/// 服务管理器入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
///
/// 第 2 参数（`rsi` = `initial_caps[0]`）是 `init` 移交的**回发凭据**（目标 task 1 + `TaskSend`），
/// 本管理器用它向 init 自报存活、并在收尾把共享段只读副本交回 init。第 5 参数（`r8`）是该凭据
/// 所指向的**对端 task id**（由 `task_create` 运行期指派）。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    init_cap_raw: u64,
    image_cap_raw: u64,
    _name_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let (Ok(self_cap), Ok(init_cap)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(init_cap_raw),
    ) else {
        idle();
    };
    // SPEC-109：`initial_cap1` 是内核打包的**单一服务映像段**（零 IPC 移交）。
    let Ok(image_cap) = Capability::from_raw(image_cap_raw) else {
        idle();
    };
    let init_id = TaskId::from_raw(peer_raw as u32);
    let self_id = runtime.task_id().map_or(0, |task| task.as_raw() as u64);

    // 自报存活：init 据此见证「管理器已建起且 id 非 init」。
    let _ = runtime.send(
        init_id,
        init_cap,
        SERVICEMGR_HELLO_TAG | (self_id & 0xff),
        &[],
    );

    let mut segment: Option<Capability> = None;
    let mut client: Option<Capability> = None;
    let mut client_id = 0u64;
    let mut registry: [Option<Endpoint>; SERVICES] = [None; SERVICES];
    let mut alpha_forwarded = false;
    let mut alpha_done = false;
    let mut beta_forwarded = false;
    let mut beta_done = false;
    let mut pong_sent = false;
    let mut app_done = false;
    let mut app_flags = 0u64;
    let mut app_hashes = [0u32; 2];
    let mut app_data = 0u64;
    let mut app_queued = 0u64;
    let mut app_cancel = 0u64;
    let mut app_skipped = 0u64;
    let mut app_error = 0u64;
    let mut app_pending = false;
    let mut app_cancelled = false;
    let mut finished = false;
    let mut capacity_probe: Option<(Capability, u64, Capability, Option<Capability>)> = None;
    let mut capacity_done = false;
    let mut capacity_exit_status = u64::MAX;
    let mut capacity_id = 0u64;
    let mut capacity_stack_reclaimed = false;
    let mut capacity_rejected = false;
    // 阶段 5 片 C：tty 服务自己注册、管理器的串口见证设备、C 的两条 write 与本轮 ACK。
    let mut tty: Option<Endpoint> = None;
    let mut tty_created = false;
    let mut mgr_serial: Option<Capability> = None;
    let mut tty_writes = [(0u64, 0u64); 2];
    let mut tty_write_count = 0;
    let mut tty_ack = false;
    // M8：rootfs 授权入口（store 的 TaskSend 副本绑定到 isolated_ns 的 "fs"）、一次授权
    // 解析的结果、同步请求的客户端 id，以及管理器→C 任务的发送凭据（C 创建后派生，退出时回收）。
    let mut rootfs_bound = false;
    let mut file_authorized = false;
    let mut file_client: Option<u32> = None;
    let mut c_send: Option<Capability> = None;
    // SPEC-105/106：建端点状态。
    let mut ns: Option<Capability> = None;
    let mut isolated_ns: Option<Capability> = None;
    let mut pending: [Option<Pending>; SERVICES] = [None; SERVICES];
    let mut c_probe: Option<Pending> = None;
    let mut tty_pending: Option<Pending> = None;
    // M5：存在 args 块时，本轮 C 映像按 sbase `echo` 用例运行；否则是固定探针。
    let mut echo_args_pending: Option<(u64, u64)> = None;
    let mut echo_mode = false;
    let mut echo_len = 0u64;
    let mut echo_hash = 2_166_136_261u32;
    let mut tty_sync_requested = false;
    let mut tty_sync_ack = false;
    // SPEC-109：从映像段首页目录一次性读出全部服务的（名字 → 段内页偏移/页数）。
    let _ = read_image_directory(
        &runtime,
        &self_cap,
        image_cap,
        &mut pending,
        &mut c_probe,
        &mut tty_pending,
        &mut echo_args_pending,
    );
    let mut created = 0u64;
    let mut created_slot: [bool; SERVICES] = [false; SERVICES];
    // EXO-3M：端点栈段句柄（本管理器创建），供原始 trace 的通用活对象判定。
    let mut stack_segs: [u64; SERVICES] = [0; SERVICES];
    // SPEC-106：三个基础服务的存在标记与路由探针应答。
    let mut marker: [u64; SERVICES] = [0; SERVICES];
    let mut probe_sent = false;
    // SPEC-114：用户态驱动的受控 I/O 握手是否已发起。
    let mut driver_sent = false;
    // SPEC-110：每个基础服务已收到的应答条数（0..3）与**已答完 3 条**的服务数。
    let mut reply_count: [u64; SERVICES] = [0; SERVICES];
    let mut served = 0u64;
    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        if message.payload >> 48 == NATIVE_APP_REQUEST_TAG >> 48 && pong_sent {
            // 控制消息携带操作与长度；8 KiB 数据留在客户端移交的跨页共享 Segment 中。
            let op = (message.payload >> 40) & 0xff;
            let length = (message.payload & 0xffff_ffff) as usize;
            let (status, hash) = if op == NATIVE_APP_OP_SUBMIT && length == NATIVE_APP_BYTES {
                app_pending = true;
                app_cancelled = false;
                app_queued += 1;
                (NATIVE_APP_QUEUED, 0)
            } else if op == NATIVE_APP_OP_CANCEL && app_pending {
                app_pending = false;
                app_cancelled = true;
                app_cancel += 1;
                (NATIVE_APP_CANCELLED, 0)
            } else if op == NATIVE_APP_OP_EXECUTE && app_cancelled {
                app_cancelled = false;
                app_skipped += 1;
                (NATIVE_APP_CANCELLED, 0)
            } else if (op != NATIVE_APP_OP_DATA && op != NATIVE_APP_OP_EXECUTE)
                || (op == NATIVE_APP_OP_DATA && length != NATIVE_APP_BYTES)
                || (op == NATIVE_APP_OP_EXECUTE && !app_pending)
            {
                app_error += 1;
                (NATIVE_APP_BAD_LENGTH, 0)
            } else if let Some(hash) = read_native_app_hash(&runtime) {
                app_pending = false;
                if app_data < 2 {
                    app_hashes[app_data as usize] = hash;
                }
                app_data += 1;
                (NATIVE_APP_OK, hash)
            } else {
                app_error += 1;
                (NATIVE_APP_BAD_LENGTH, 0)
            };
            if let Some(client_cap) = client {
                let _ = runtime.send(
                    TaskId::from_raw(client_id as u32),
                    client_cap,
                    native_app_reply(status, hash),
                    &[],
                );
            }
        } else if message.payload >> 48 == NATIVE_APP_DONE_TAG >> 48 && pong_sent {
            app_flags = message.payload & 0xf;
            app_done = true;
        } else if message.payload >> 48 == SERVICE_SETUP_TAG >> 48 {
            // ⓪' SPEC-105：init 移交父 Namespace（单 cap，payload 仅 tag）。
            if let Some(cap0) = message.capabilities[0]
                && ns.is_none()
            {
                ns = Some(cap0);
                // 普通端点所需的结构性根只指向空 Namespace；全局解析权单独交给 NS 服务。
                isolated_ns = runtime.namespace_create().ok();
            }
        } else if message.capabilities[1].is_some() {
            // ① 客户端 Hello（唯一带**两条** cap 的消息：段 cap + 客户端自管理 cap）。
            if segment.is_none() {
                let (Some(seg), Some(client_cap)) =
                    (message.capabilities[0], message.capabilities[1])
                else {
                    continue;
                };
                client_id = message.payload & 0xff;
                if runtime
                    .memseg_map(seg, self_cap, 0, USER_SEG_VA, 1, RW)
                    .is_err()
                    || runtime
                        .memseg_map(seg, self_cap, 1, NATIVE_APP_DATA_VA, 3, RW)
                        .is_err()
                {
                    continue;
                }
                let _ =
                    runtime.write_user_bytes(USER_SEG_VA + OFF_CLIENT_ID, &client_id.to_le_bytes());
                let _ = runtime
                    .write_user_bytes(USER_SEG_VA + OFF_SERVICEMGR_ID, &self_id.to_le_bytes());
                let _ = runtime.write_user_bytes(
                    USER_SEG_VA + OFF_SERVICEMGR_MARKER,
                    &SERVICEMGR_BINARY_MARKER.to_le_bytes(),
                );
                segment = Some(seg);
                client = Some(client_cap);
            }
        } else if message.payload >> 48 == SERVICE_ID_TAG >> 48 {
            // ② 端点/服务自报（cap0 = 自管理凭据，管理器据此回送转发/路由探针）。
            let name = (message.payload >> 8) & SERVICE_NAME_MASK;
            let behavior = behavior_for(name);
            let slot = behavior.wrapping_sub(1) as usize;
            if let Some(authority) = message.capabilities[0]
                && behavior != SERVICE_BEHAVIOR_NONE
                && slot < SERVICES
                && registry[slot].is_none()
            {
                let id = message.payload & 0xff;
                registry[slot] = Some(Endpoint {
                    id,
                    authority,
                    behavior,
                });
                if let Some(seg) = segment {
                    if behavior == SERVICE_BEHAVIOR_ALPHA {
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_REG_A_NAME, &name.to_le_bytes());
                    } else if behavior == SERVICE_BEHAVIOR_BETA {
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_REG_B_NAME, &name.to_le_bytes());
                        // 把**管理器收集到的**端点 id 写回，与管理器收到的注册名同源。
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_BETA_SELF, &id.to_le_bytes());
                    } else {
                        let _ = runtime.write_user_bytes(
                            USER_SEG_VA + base_id_offset(slot),
                            &id.to_le_bytes(),
                        );
                    }
                    let _ = seg;
                }
            }
        } else if message.payload >> 48 == STAGE5_TTY_READY_TAG >> 48 {
            // 阶段 5 片 C：tty 服务就绪。管理器铸一块 COM1 设备给它做字节流后端，另铸一块
            // 只留给自己的见证行输出（两块独立权威，互不转移）。
            if let Some(authority) = message.capabilities[0]
                && tty.is_none()
            {
                let id = message.payload & 0xff;
                tty = Some(Endpoint {
                    id,
                    authority,
                    behavior: 0,
                });
                if let Ok(dev) = runtime.dev_ioport_create(IOPORT_BASE, IOPORT_COUNT)
                    && runtime
                        .send(
                            TaskId::from_raw(id as u32),
                            authority,
                            SERVICE_DEV_CAP_TAG,
                            &[dev],
                        )
                        .is_err()
                {
                    // 发送失败时 cap 未被移动，仍由管理器持有；显式归还避免泄漏。
                    let _ = runtime.cap_drop(dev);
                }
                if let Ok(serial) = runtime.dev_ioport_create(IOPORT_BASE, IOPORT_COUNT) {
                    mgr_serial = Some(serial);
                }
            }
        } else if message.payload >> 48 == STAGE5_TTY_WRITE_TAG >> 48 {
            // C 的 write(1/2)：管理器逐字节转发给 tty。字节流不设长度上限；只保留前两条用于
            // 固定探针的 CIO 见证。
            let fd = (message.payload >> 16) & 0xff;
            let byte = (message.payload >> 24) & 0xff;
            if fd == 1 || fd == 2 {
                if echo_mode {
                    echo_len += 1;
                    echo_hash = native_app_hash(echo_hash, byte as u8);
                }
                if tty_write_count < 2 {
                    tty_writes[tty_write_count] = (fd, byte);
                }
                tty_write_count += 1;
                if let Some(endpoint) = tty {
                    // M5：echo 走独立 tag，tty 不输出探针诊断行。
                    let payload = if echo_mode {
                        (message.payload & 0x0000_ffff_ffff_ffff) | STAGE5_ECHO_WRITE_TAG
                    } else {
                        message.payload
                    };
                    let _ = runtime.send(
                        TaskId::from_raw(endpoint.id as u32),
                        endpoint.authority,
                        payload,
                        &[],
                    );
                }
                if !echo_mode
                    && tty_write_count == 2
                    && let Some(serial) = mgr_serial
                {
                    let sender = (message.payload >> 32) & 0xff;
                    // tty 的数据字节没有换行；先换行，避免见证行被 'h'/'i' 前缀。
                    let _ = serial_puts(&runtime, serial, b"\r\nCIO|task=");
                    let _ = serial_dec(&runtime, serial, sender);
                    for (fd, byte) in tty_writes {
                        let _ = serial_puts(&runtime, serial, b";fd=");
                        let _ = serial_dec(&runtime, serial, fd);
                        let _ = serial_puts(&runtime, serial, b";ch=");
                        let _ = serial_dec(&runtime, serial, byte);
                    }
                    let _ = serial_puts(&runtime, serial, b"\r\n");
                }
            }
        } else if message.payload >> 48 == STAGE5_TTY_ACK_TAG >> 48 {
            tty_ack = true;
        } else if message.payload >> 48 == STAGE5_TTY_SYNC_ACK_TAG >> 48 {
            tty_sync_ack = true;
        } else if matches!(
            message.payload >> 48,
            tag if tag == SERVICE_VFS_OPEN_TAG >> 48
                || tag == SERVICE_VFS_PATH_TAG >> 48
                || tag == SERVICE_VFS_MUTATE_TAG >> 48
                || tag == SERVICE_VFS_COMMIT_TAG >> 48
        ) {
            // M8：C 适配层 → 管理器 → 存储服务。rootfs 授权在 C 创建时一次建立；这里只做
            // 路由，不解析文件路径。OPEN/PATH/COMMIT 都同步转发，只有 COMMIT 会设置应答槽。
            let is_commit = message.payload >> 48 == SERVICE_VFS_COMMIT_TAG >> 48;
            let client = if is_commit {
                ((message.payload >> SERVICE_VFS_COMMIT_CLIENT_SHIFT) & 0xff) as u32
            } else {
                0
            };
            let forwarded = if file_authorized {
                registry[STORE_SLOT].is_some_and(|store| {
                    runtime
                        .send(
                            TaskId::from_raw(store.id as u32),
                            store.authority,
                            message.payload,
                            &[],
                        )
                        .is_ok()
                })
            } else {
                false
            };
            if forwarded {
                if is_commit {
                    file_client = Some(client);
                }
            } else if let Some(send) = c_send {
                let _ = runtime.send(
                    TaskId::from_raw(client),
                    send,
                    pack_file_reply(SERVICE_FILE_OP_OPEN, 0, SERVICE_FILE_ENOENT, 0),
                    &[],
                );
            }
        } else if message.payload >> 48 == SERVICE_FILE_REQUEST_TAG >> 48 {
            // M4/M8：已授权的读/写/lseek/close。句柄只能由一次成功的 open 产生；管理器
            // 不解释文件内容或目录，VFS 完全在存储服务侧。
            let op = (message.payload >> SERVICE_FILE_OP_SHIFT) & 0xf;
            let client = ((message.payload >> SERVICE_FILE_CLIENT_SHIFT) & 0xff) as u32;
            let forwarded = if file_authorized {
                registry[STORE_SLOT].is_some_and(|store| {
                    runtime
                        .send(
                            TaskId::from_raw(store.id as u32),
                            store.authority,
                            message.payload,
                            &[],
                        )
                        .is_ok()
                })
            } else {
                false
            };
            if forwarded {
                file_client = Some(client);
            } else if let Some(send) = c_send {
                let fd = (message.payload >> SERVICE_FILE_FD_SHIFT) & 0xf;
                let _ = runtime.send(
                    TaskId::from_raw(client),
                    send,
                    pack_file_reply(op, fd, SERVICE_FILE_ENOENT, 0),
                    &[],
                );
            }
        } else if message.payload >> 48 == SERVICE_FILE_REPLY_TAG >> 48 {
            // M4：存储服务的应答按同步请求记录回送 C 任务；无等待者时丢弃。
            if let (Some(client), Some(send)) = (file_client.take(), c_send) {
                let _ = runtime.send(TaskId::from_raw(client), send, message.payload, &[]);
            }
        } else if message.payload >> 48 == SERVICE_VMA_CAP_REQUEST_TAG >> 48 {
            // M10：C 任务首次 mmap 时申请 arena Segment 的映射授权。只有 C 程序运行期
            // (`capacity_probe`) 才有可派生的 stack cap；不能回执时由 C 侧退化为
            // “只 free 不 unmap”，不得让 C 阻塞在 RECV。
            let client = ((message.payload >> SERVICE_VMA_CLIENT_SHIFT) & 0xff) as u32;
            let mut delivered = false;
            if let Some((manager, _, stack, _)) = capacity_probe {
                let authority = c_send.unwrap_or(manager);
                if let Ok(arena) = runtime.cap_derive(stack, ARENA_MAP_RIGHTS) {
                    if runtime
                        .send(
                            TaskId::from_raw(client),
                            authority,
                            pack_vma_cap_reply(0),
                            &[arena],
                        )
                        .is_ok()
                    {
                        delivered = true;
                    } else {
                        let _ = runtime.cap_drop(arena);
                    }
                }
                if !delivered {
                    let _ = runtime.send(
                        TaskId::from_raw(client),
                        authority,
                        pack_vma_cap_reply(1),
                        &[],
                    );
                }
            } else if let Some(send) = c_send {
                let _ = runtime.send(TaskId::from_raw(client), send, pack_vma_cap_reply(1), &[]);
            }
        } else if message.payload >> 48 == STAGE5_C_PROBE_TAG >> 48 {
            // C 程序（id 17）自报后调用 SYS_TASK_DESTROY(self)；内核 TASKCAP 独立核验
            // Dead 与根清理，不能只相信这条消息。
            // 屏蔽状态字节与 ID 字节后必须等于固定标签（含 0x5a 见证），再记录两者。
            if message.payload & !((0xff << 16) | 0xff) == (STAGE5_C_PROBE_TAG & !0xff) {
                capacity_id = message.payload & 0xff;
                capacity_exit_status = (message.payload >> 16) & 0xff;
            }
            if let Some((manager, entry_offset, stack, data)) = capacity_probe {
                let stack_reclaimed = runtime.memseg_destroy(stack).is_ok();
                let data_reclaimed = match data {
                    Some(data) => runtime.memseg_destroy(data).is_ok(),
                    None => true,
                };
                // 字段沿用 `stack_reclaimed`：C 的栈段与私有数据段都必须被回收。
                capacity_stack_reclaimed = stack_reclaimed && data_reclaimed;
                let _ = runtime.cap_drop(stack);
                if let Some(data) = data {
                    let _ = runtime.cap_drop(data);
                }
                if let Some(send) = c_send.take() {
                    let _ = runtime.cap_drop(send);
                }
                let _ = runtime.cap_drop(manager);
                if let (Some(spec), Some(parent_ns)) = (c_probe, isolated_ns) {
                    // Dead 记录仍占 ID 16；合法请求必须确定地返回 ENOSPC。
                    capacity_rejected = runtime.task_create(&TaskSpec {
                        code_segment: spec.code,
                        code_offset: spec.offset_pages,
                        code_pages: spec.pages,
                        load_base: ENDPOINT_CODE_VA,
                        entry: ENDPOINT_CODE_VA + entry_offset,
                        stack_top: ENDPOINT_STACK_VA + FRAME_SIZE,
                        parent_namespace: Some(parent_ns),
                        root_rights: NS_RESOLVE_RIGHTS,
                        initial_capabilities: [None, None],
                        name: spec.name,
                    }) == Err(Error::ResourceExhausted);
                }
            }
            capacity_done = true;
            // M5：C 已退出，说明全部字节已发给 tty；用屏障消息确认它们已落到串口，
            // 之后才允许输出 `ECHO|` 见证并收尾。
            if echo_mode
                && !tty_sync_requested
                && let Some(endpoint) = tty
            {
                let _ = runtime.send(
                    TaskId::from_raw(endpoint.id as u32),
                    endpoint.authority,
                    STAGE5_TTY_SYNC_TAG,
                    &[],
                );
                tty_sync_requested = true;
            }
        } else if message.payload >> 48 == SERVICE_DONE_TAG >> 48 {
            // ③ 端点完成协商。先转发的是 `ALPHA`，故首个 done 属 `ALPHA`。
            if !alpha_done {
                alpha_done = true;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + OFF_DONE_ALPHA,
                        &message.payload.to_le_bytes(),
                    );
                }
            } else {
                beta_done = true;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + OFF_DONE_BETA,
                        &message.payload.to_le_bytes(),
                    );
                }
            }
        } else if message.payload >> 48 == SERVICE_MARKER_TAG >> 48 {
            // ③' SPEC-106：基础服务自报存在标记（kind 归槽 + 每二进制相异的 marker）。
            let kind = (message.payload >> SERVICE_MARKER_KIND_SHIFT) & 0xff;
            let slot = kind.wrapping_sub(1) as usize;
            if (BASE_FIRST..SERVICES).contains(&slot) && marker[slot] == 0 {
                marker[slot] = message.payload & SERVICE_MARKER_MASK;
                if segment.is_some() {
                    let _ = runtime.write_user_bytes(
                        USER_SEG_VA + base_mark_offset(slot),
                        &marker[slot].to_le_bytes(),
                    );
                }
            }
        } else if message.payload >> 48 == SERVICE_SERVE_TAG >> 48 {
            // ③'' SPEC-106/110：基础服务的**定序状态应答**（kind 归槽，按到达顺序填该服务的第
            // 0/1/2 槽——同一发送者到同一服务保序，故推进 A、推进 B、查询的应答依序落位）。
            let kind = (message.payload >> 32) & 0xff;
            let slot = kind.wrapping_sub(1) as usize;
            let wanted = probes_for(slot);
            if (BASE_FIRST..SERVICES).contains(&slot) && reply_count[slot] < wanted {
                let index = reply_count[slot];
                reply_count[slot] += 1;
                if segment.is_some() {
                    // 前 `SERVICE_SERVE_PROBES` 条落状态槽；Namespace 服务的第 4 条落代理解析槽。
                    let offset = if index < SERVICE_SERVE_PROBES {
                        base_reply_offset(slot) + index * 8
                    } else {
                        OFF_NS_RESOLVED
                    };
                    let _ = runtime
                        .write_user_bytes(USER_SEG_VA + offset, &message.payload.to_le_bytes());
                    // 三个**基础服务**的定序应答齐 ⇒ 服务数 +1（`services` 语义不变，SPEC-114 的
                    // 用户态驱动走自己的受控端口握手，不并入该计数）。
                    if reply_count[slot] == wanted && slot < BASE_FIRST + BASE_COUNT {
                        served += 1;
                        let _ = runtime
                            .write_user_bytes(USER_SEG_VA + OFF_SERVICES, &served.to_le_bytes());
                    }
                }
            }
        }

        // ⓪' SPEC-105/106：cap、参数与共享段齐全即创建端点（表驱动）——建端点证据写回共享段，故
        // 须待客户端 Hello 把段映射进本根之后。
        if let (Some(parent_ns), Some(_)) = (isolated_ns, segment) {
            let mut slot = 0;
            while slot < SERVICES {
                if !created_slot[slot]
                    && let Some(spec) = pending[slot]
                    && let Some((_token, _manager, _entry, stack, _data)) = create_service(
                        &runtime,
                        &self_cap,
                        parent_ns,
                        &spec,
                        slot,
                        &mut created_slot,
                        false,
                        None,
                    )
                {
                    created += 1;
                    stack_segs[slot] = stack.as_raw();
                    let _ = runtime
                        .write_user_bytes(USER_SEG_VA + OFF_MGR_CREATED, &created.to_le_bytes());
                }
                slot += 1;
            }
            // 阶段 5 片 C：tty 服务在建端点阶段创建，早于 C 探针；它随后自报就绪。
            // 只尝试一次：失败后重试只会重复铸造/回滚 cap，缺失的证据由 QEMU 验收暴露。
            if !tty_created && let Some(spec) = tty_pending {
                let mut tty_slot = [false; SERVICES];
                let _ = create_service(
                    &runtime,
                    &self_cap,
                    parent_ns,
                    &spec,
                    DRIVER_SLOT,
                    &mut tty_slot,
                    false,
                    None,
                );
                tty_created = true;
            }
        }

        let Some(seg) = segment else {
            continue;
        };

        // ④ 首次转发：`ALPHA` 行为（既有 `service_negotiated` 见证的产出方）。以注册表就绪为准，
        // 与基础服务的注册互不耦合。
        if registry[0].is_some() && registry[1].is_some() && !alpha_forwarded {
            if let Some(alpha) = registry[0]
                && let Ok(copy) = runtime.cap_derive(seg, RW_DERIVE)
            {
                forward(&runtime, &alpha, copy);
            }
            alpha_forwarded = true;
        }

        // ⑤ 读客户端写入共享段的**请求名**并按名解析，再二次转发到解析到的行为。
        if alpha_done && !beta_forwarded {
            let requested = runtime
                .read_user_u64(USER_SEG_VA + SERVICE_NAME_OFFSET)
                .unwrap_or(0);
            let resolved = behavior_for(requested);
            let selected_unknown = behavior_for(SERVICE_NAME_UNKNOWN);
            let _ = runtime.write_user_bytes(USER_SEG_VA + OFF_RESOLVED, &resolved.to_le_bytes());
            let _ = runtime
                .write_user_bytes(USER_SEG_VA + OFF_UNKNOWN, &selected_unknown.to_le_bytes());
            let slot = resolved.wrapping_sub(1) as usize;
            if resolved != SERVICE_BEHAVIOR_NONE
                && slot < SERVICES
                && let Some(endpoint) = registry[slot]
                && let Ok(copy) = runtime.cap_derive(seg, RW_DERIVE)
            {
                forward(&runtime, &endpoint, copy);
            } else {
                // 未注册名（`NONE`）被拒绝、不转发：无第二个 done，直接允许收尾。
                beta_done = true;
            }
            beta_forwarded = true;
        }

        // ⑤' SPEC-106/110：三个基础服务都已注册即各发**定序三条**请求（推进 A、推进 B、查询，
        // 只发一次）。同一发送者保序，故服务按推进→推进→查询处理，应答依序落位各自的状态槽。
        if !probe_sent
            && registry[BASE_FIRST].is_some()
            && registry[BASE_FIRST + 1].is_some()
            && registry[BASE_FIRST + 2].is_some()
        {
            // SPEC-111：把客户端共享段的**只读派生副本**绑定到专用路径 `svc`，供 Namespace 服务
            // 代理解析——绑定是**纯 ring3 行为**（管理器持带 `NsBind` 的父 Namespace）。用派生副本
            // 而非原段：`bind` 会把 cap 移入 Namespace，原段仍须交回 init。
            if let Some(ns_cap) = ns
                && let Some(seg) = segment
                && let Ok(entry) = runtime.cap_derive(seg, RW_DERIVE)
            {
                let _ = runtime.namespace_bind(ns_cap, "svc", entry);
            }
            // SPEC-111：先给 Namespace 服务**运行期移交**一份 `NsResolve` 只读 Namespace 副本——
            // 保留无 cap 唤醒 + 带 cap 消息的两条连续发送协议；阶段 2 直投修复已解除
            // 内核限制。只有该服务收到，故 log/store 始终无 Namespace 权限（最小权限）。
            if let Some(ns_cap) = ns
                && let Some(ns_ep) = registry[NS_SLOT]
                && let Ok(copy) = runtime.cap_derive(ns_cap, NS_RESOLVE_RIGHTS)
            {
                let task = TaskId::from_raw(ns_ep.id as u32);
                let _ = runtime.send(task, ns_ep.authority, SERVICE_WAKE_TAG, &[]);
                let _ = runtime.send(task, ns_ep.authority, SERVICE_NS_CAP_TAG, &[copy]);
            }
            // SPEC-112：为日志/存储服务各 `SYS_MEMSEG_CREATE` 一个持久化段并**运行期移交**——
            // 服务把它映射进自身根后据此落日志/落数据。句柄先写回共享段（内核侧核验），再以
            // 「无 cap 唤醒 + 带 cap 消息」两条连发移交（接收方可能阻塞在 recv）。Namespace 服务
            // 不持段（最小权限）。移交先于下面的定序请求——同一发送者保序，故服务必先收到段。
            let mut slot = BASE_FIRST;
            while slot < BASE_FIRST + BASE_COUNT {
                if slot != NS_SLOT
                    && let Some(endpoint) = registry[slot]
                    && let Ok(storage) = runtime.memseg_create(1, RW)
                {
                    if let Some(seg) = segment {
                        let _ = runtime.write_user_bytes(
                            USER_SEG_VA + base_seg_offset(slot),
                            &storage.as_raw().to_le_bytes(),
                        );
                        let _ = seg;
                    }
                    let task = TaskId::from_raw(endpoint.id as u32);
                    let _ = runtime.send(task, endpoint.authority, SERVICE_WAKE_TAG, &[]);
                    let _ = runtime.send(task, endpoint.authority, SERVICE_SEG_TAG, &[storage]);
                }
                slot += 1;
            }
            let mut slot = BASE_FIRST;
            while slot < BASE_FIRST + BASE_COUNT {
                if let Some(endpoint) = registry[slot] {
                    let task = TaskId::from_raw(endpoint.id as u32);
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_ARG_A),
                        &[],
                    );
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_ARG_B),
                        &[],
                    );
                    let _ = runtime.send(
                        task,
                        endpoint.authority,
                        pack_service_request(SERVICE_SERVE_OP_QUERY, 0),
                        &[],
                    );
                    // SPEC-111：Namespace 服务多一条**代理解析**请求——它持有内核给的 `NsResolve`
                    // 结构性根，据此调 `SYS_NS_RESOLVE` 返回内核命名空间树的句柄。
                    if slot == NS_SLOT {
                        let _ = runtime.send(
                            task,
                            endpoint.authority,
                            pack_service_request(SERVICE_SERVE_OP_RESOLVE, 0),
                            &[],
                        );
                    }
                }
                slot += 1;
            }
            probe_sent = true;
        }

        // ⑤'' SPEC-114：**用户态驱动**的受控 I/O 握手。独立于上面的一次性基础服务探针块：那个块由
        // 「三个基础服务已注册」触发，而驱动不属于它们（`served` 也不计入驱动）。管理器
        // `SYS_DEV_IOPORT_CREATE` 铸一个覆盖 COM1 的 `Device(IOPortRange)`，句柄写回共享段（内核侧
        // 核验其为活的内核 `Device`），再运行期移交给驱动；随后两条受控端口请求。
        if !driver_sent
            && registry[DRIVER_SLOT].is_some()
            && segment.is_some()
            && let Some(driver) = registry[DRIVER_SLOT]
            && let Ok(dev) = runtime.dev_ioport_create(IOPORT_BASE, IOPORT_COUNT)
        {
            if let Some(seg) = segment {
                let _ = runtime
                    .write_user_bytes(USER_SEG_VA + OFF_DRIVER_DEV, &dev.as_raw().to_le_bytes());
                let _ = seg;
            }
            let task = TaskId::from_raw(driver.id as u32);
            let _ = runtime.send(task, driver.authority, SERVICE_WAKE_TAG, &[]);
            let _ = runtime.send(task, driver.authority, SERVICE_DEV_CAP_TAG, &[dev]);
            let _ = runtime.send(
                task,
                driver.authority,
                pack_service_request(SERVICE_SERVE_OP_IOPORT_RW, 0),
                &[],
            );
            let _ = runtime.send(
                task,
                driver.authority,
                pack_service_request(SERVICE_SERVE_OP_IOPORT_OOR, 0),
                &[],
            );
            driver_sent = true;
        }

        // ⑥ 两轮转发、三个基础服务探针与**用户态驱动的两条受控端口应答**都完成：应答客户端
        // （`PONG`），并把共享段只读副本交回 init。
        //
        // SPEC-114：驱动的**两条受控端口应答**必须并入收尾条件——这是调度层面的硬要求。调度器
        // `sched::pick_next` 取**最小 Ready id**，而驱动的 id 最大（服务表末位创建），只有在其余
        // 任务全部阻塞时才轮得到它。管理器每条消息处理完都会跑一遍本循环的尾部；若这里不等待驱动
        // 应答，管理器会**在同一轮迭代内**接着把 `PONG`/`FINISH` 发完并阻塞，之后 init（id 1）成为
        // 最小 Ready 任务，驱动再不被选中（实测：`task_status(15)` 恒为 `Ready`、`driver_io0/io1`
        // 恒为 0——这正是本片第一版的行为）。把驱动应答并入本条件后，管理器在等待期间只是阻塞在
        // `receive`，驱动因而成为唯一 Ready 任务而被调度。
        if beta_done
            && served == BASE_COUNT as u64
            && reply_count[DRIVER_SLOT] >= DRIVER_PROBES
            && !pong_sent
            && let Some(client_cap) = client
        {
            let _ = runtime.send(
                TaskId::from_raw(client_id as u32),
                client_cap,
                SERVICE_REPLY_WORD,
                &[],
            );
            // 等客户端把应答写回共享段（offset 64）后再收尾——否则 init 可能先 `int3`，内核见证
            // 会读到未写入的 0。客户端被 `PONG` 直投唤醒后必然写入。
            let echo = loop {
                let _ = runtime.yield_task();
                let value = runtime
                    .read_user_u64(USER_SEG_VA + SERVICE_REPLY_OFFSET)
                    .unwrap_or(0);
                if value != 0 {
                    break value;
                }
            };
            pong_sent = true;
            let _ = trace_emit(&runtime, EVENT_SERVICE_REPLY, client_id, echo, 0, 0, 0);
        }
        // tty 必须先完成注册并拿到设备，C 探针的 write 才有后端；未注册时继续事件循环，
        // 让 tty 在管理器阻塞于 receive 时被调度。
        if pong_sent && app_done && tty.is_some() && !finished {
            if capacity_probe.is_none() && !capacity_done {
                let c_args = match echo_args_pending {
                    Some((offset_pages, pages)) => {
                        read_c_args(&runtime, &self_cap, image_cap, offset_pages, pages)
                    }
                    None => None,
                };
                echo_mode = c_args.is_some();
                if echo_args_pending.is_some() && c_args.is_none() {
                    // argv 块不可解析：让本轮确定地失败，而不是退回探针参数。
                    echo_mode = true;
                    capacity_done = true;
                    echo_len = u64::MAX;
                    tty_sync_ack = true;
                } else if let (Some(spec), Some(parent_ns)) = (c_probe, isolated_ns) {
                    if let Some((_token, manager, entry, stack, data)) = create_service(
                        &runtime,
                        &self_cap,
                        parent_ns,
                        &spec,
                        DRIVER_SLOT,
                        &mut created_slot,
                        true,
                        c_args.as_ref(),
                    ) {
                        // M8：把存储服务的访问令牌绑定到 C 任务根继承的 Namespace 路径 "fs"，
                        // 并**一次**解析成功作为 rootfs 授权。必须在 C 创建之后做——创建前多占
                        // 一个 cap 槽会让 `task_create` 的 `live + 3 + initial` 越过上限而返回
                        // ENOSPC。授权只在创建时建立；后续 VFS 操作沿用该授权，不逐次解析文件
                        // 路径（Namespace 是授权入口，文件树在用户态存储服务里）。
                        if !rootfs_bound
                            && let Some(ns_cap) = isolated_ns
                            && let Ok(copy) = runtime.cap_derive(self_cap, TASK_SEND_DERIVE)
                        {
                            let result =
                                runtime.namespace_bind(ns_cap, SERVICE_VFS_ROOT_PATH, copy);
                            rootfs_bound = result.is_ok();
                            if !rootfs_bound {
                                let _ = runtime.cap_drop(copy);
                            } else if let Ok(resolve) =
                                runtime.cap_derive(ns_cap, NS_RESOLVE_RIGHTS)
                            {
                                let resolved = runtime.namespace_resolve(
                                    resolve,
                                    SERVICE_VFS_ROOT_PATH,
                                    TASK_SEND_RIGHTS,
                                );
                                let _ = runtime.cap_drop(resolve);
                                file_authorized = match resolved {
                                    Ok(cap) => {
                                        let _ = runtime.cap_drop(cap);
                                        true
                                    }
                                    Err(_) => false,
                                };
                            }
                        }
                        // M4：派生「管理器 → C 任务」的发送凭据，供文件应答回送；C 退出时回收。
                        if let Ok(send) = runtime.cap_derive(manager, TASK_SEND_RIGHTS) {
                            c_send = Some(send);
                        }
                        capacity_probe = Some((manager, entry, stack, data));
                    } else {
                        capacity_done = true; // 后续 trace 记录失败，QEMU 验收拒绝。
                    }
                } else {
                    capacity_done = true;
                }
            }
            // 探针：还要等 tty 完成两条 write 的处理与 ioport 正反例。
            // echo：等 tty 处理完排空屏障，确保冻结 stdout 已全部落到串口。
            if !capacity_done || (!echo_mode && !tty_ack) || (echo_mode && !tty_sync_ack) {
                continue;
            }
            if echo_mode && let Some(serial) = mgr_serial {
                let _ = serial_puts(&runtime, serial, b"\r\nECHO|status=");
                let _ = serial_dec(&runtime, serial, capacity_exit_status);
                let _ = serial_puts(&runtime, serial, b";len=");
                let _ = serial_dec(&runtime, serial, echo_len);
                let _ = serial_puts(&runtime, serial, b";hash=");
                let _ = serial_dec(&runtime, serial, echo_hash as u64);
                let _ = serial_puts(&runtime, serial, b"\r\n");
            }
            // 两次成功的数据读回、一次执行前取消、一次越界拒绝与客户端的四项检查，
            // 均由独立 ring3 管理器提交为原始 trace，xtask 负责判断其含义。
            let _ = trace_emit(
                &runtime,
                EVENT_NATIVE_APP,
                NATIVE_APP_BYTES as u64,
                app_hashes[0] as u64,
                app_hashes[1] as u64,
                (app_data << 32)
                    | (app_queued << 24)
                    | (app_cancel << 16)
                    | (app_skipped << 8)
                    | app_error,
                app_flags,
            );
            // EXO-3E：把 service-reply 的共享段字段作为通用 trace 记录交给内核透传，
            // 内核不再读 `USER_HELLO2_VA + {CLIENT_ID, REPLY}`。
            // EXO-3F：service-negotiated 的共享段字段也用通用 trace 透传。
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICE_NEGOTIATED,
                read_shared(&runtime, OFF_MGR_TOKEN),
                read_shared(&runtime, OFF_MGR_MANAGER),
                read_shared(&runtime, SERVICE_ENDPOINT_ALPHA_OFFSET),
                read_shared(&runtime, OFF_DONE_ALPHA),
                read_shared(&runtime, SERVICE_NEGOTIATED_OFFSET),
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICE_NEGOTIATED,
                read_shared(&runtime, SERVICE_NEGATIVES_OFFSET),
                0,
                0,
                0,
                0,
            );
            // EXO-3G：service-selected 的共享段字段用三条通用 trace 记录透传。
            let alpha_marker = runtime
                .read_user_u64(USER_SEG_VA + SERVICE_ALPHA_MARKER_OFFSET)
                .unwrap_or(0);
            let beta_marker = runtime
                .read_user_u64(USER_SEG_VA + SERVICE_BETA_MARKER_OFFSET)
                .unwrap_or(0);
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICE_SELECTED,
                read_shared(&runtime, 48),
                read_shared(&runtime, OFF_RESOLVED),
                read_shared(&runtime, OFF_UNKNOWN),
                read_shared(&runtime, OFF_DONE_BETA),
                read_shared(&runtime, SERVICE_NEGOTIATED_B_OFFSET),
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICE_SELECTED,
                read_shared(&runtime, SERVICE_ENDPOINT_ALPHA_OFFSET),
                read_shared(&runtime, SERVICE_ENDPOINT_BETA_OFFSET),
                read_shared(&runtime, SERVICE_ENDPOINT_BETA_OFFSET),
                alpha_marker,
                beta_marker,
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICE_SELECTED,
                client_id,
                read_shared(&runtime, OFF_REG_A_NAME),
                read_shared(&runtime, OFF_REG_B_NAME),
                // EXO-3M：端点栈段句柄（本管理器创建）作为通用原始值上报；内核只做活对象扫描。
                stack_segs[0],
                stack_segs[1],
            );
            // EXO-3H：servicemgr 的共享段字段用六条通用 trace 记录透传。
            let log_id = registry[LOG_SLOT].map_or(0, |endpoint| endpoint.id);
            let ns_id = registry[NS_SLOT].map_or(0, |endpoint| endpoint.id);
            let store_id = registry[STORE_SLOT].map_or(0, |endpoint| endpoint.id);
            let driver_id = registry[DRIVER_SLOT].map_or(0, |endpoint| endpoint.id);
            let log_reply = base_reply_offset(LOG_SLOT);
            let ns_reply = base_reply_offset(NS_SLOT);
            let store_reply = base_reply_offset(STORE_SLOT);
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                self_id,
                SERVICEMGR_BINARY_MARKER,
                created,
                read_shared(&runtime, OFF_MGR_ENTRY_A),
                read_shared(&runtime, OFF_MGR_ENTRY_B),
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                served,
                log_id,
                ns_id,
                store_id,
                marker[LOG_SLOT],
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                marker[NS_SLOT],
                marker[STORE_SLOT],
                read_shared(&runtime, log_reply),
                read_shared(&runtime, log_reply + 8),
                read_shared(&runtime, log_reply + 16),
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                read_shared(&runtime, ns_reply),
                read_shared(&runtime, ns_reply + 8),
                read_shared(&runtime, ns_reply + 16),
                read_shared(&runtime, store_reply),
                read_shared(&runtime, store_reply + 8),
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                read_shared(&runtime, store_reply + 16),
                read_shared(&runtime, OFF_NS_RESOLVED),
                read_shared(&runtime, OFF_LOG_SEG),
                read_shared(&runtime, OFF_STORE_SEG),
                driver_id,
            );
            let _ = trace_emit(
                &runtime,
                EVENT_SERVICEMGR,
                marker[DRIVER_SLOT],
                read_shared(&runtime, OFF_DRIVER_DEV),
                read_shared(&runtime, OFF_DRIVER_REPLY),
                read_shared(&runtime, OFF_DRIVER_REPLY + 8),
                0,
            );
            let _ = trace_emit(
                &runtime,
                EVENT_STAGE5_CAPACITY,
                capacity_id,
                STAGE5_C_PROBE_TAG >> 8 & 0xff,
                capacity_exit_status,
                u64::from(capacity_stack_reclaimed),
                u64::from(capacity_rejected),
            );
            // 三段**连发**（中间不让出）：唤醒先于带 cap 消息，宿主的 `BlockedRecv` 约束不触发。
            let _ = runtime.send(init_id, init_cap, SERVICEMGR_WAKE_TAG, &[]);
            if let Ok(read_only) = runtime.cap_derive(seg, R_DERIVE) {
                let _ = runtime.send(init_id, init_cap, SERVICEMGR_SEG_TAG, &[read_only]);
            }
            let _ = runtime.send(init_id, init_cap, SERVICEMGR_FINISH_TAG, &[]);
            finished = true;
        }
    }
}

/// SPEC-109：映射映像段**首页目录**（只读）读出服务表——名字 → 段内页偏移/页数。
///
/// 目录布局（little-endian u64）：`magic | count | [name | offset_pages | page_count] × count`。
/// 读毕即卸载探测映射。目录非法或无匹配时静默跳过（服务缺失由后续见证暴露）。
fn read_image_directory(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    image: Capability,
    pending: &mut [Option<Pending>; SERVICES],
    c_probe: &mut Option<Pending>,
    tty_pending: &mut Option<Pending>,
    echo_args: &mut Option<(u64, u64)>,
) -> Result<(), ()> {
    runtime
        .memseg_map(image, *self_cap, 0, ENDPOINT_CODE_PROBE_VA, 1, R_ONLY)
        .map_err(|_| ())?;
    let magic = runtime.read_user_u64(ENDPOINT_CODE_PROBE_VA).unwrap_or(0);
    let count = runtime
        .read_user_u64(ENDPOINT_CODE_PROBE_VA + 8)
        .unwrap_or(0);
    if magic == SERVICE_IMAGE_MAGIC {
        // EXO-2D：目录项为 `kind | offset_pages | pages`。kind 是用户态协议号
        // （既有 `SERVICE_BEHAVIOR_*`）；服务管理器自身 kind 被跳过。
        let mut index = 0u64;
        while index < count {
            let at =
                ENDPOINT_CODE_PROBE_VA + SERVICE_IMAGE_DIR_HEADER + index * SERVICE_IMAGE_ENTRY_LEN;
            let kind = runtime.read_user_u64(at).unwrap_or(0);
            let offset_pages = runtime.read_user_u64(at + 8).unwrap_or(0);
            let pages = runtime.read_user_u64(at + 16).unwrap_or(0);
            if kind == SERVICE_IMAGE_KIND_C_PROBE && pages != 0 {
                *c_probe = Some(Pending {
                    code: image,
                    offset_pages,
                    pages,
                    name: pack_service_name(b"cprog"),
                });
            } else if kind == SERVICE_IMAGE_KIND_TTY && pages != 0 {
                *tty_pending = Some(Pending {
                    code: image,
                    offset_pages,
                    pages,
                    name: pack_service_name(b"tty"),
                });
            } else if kind == SERVICE_IMAGE_KIND_ECHO_ARGS && pages != 0 {
                *echo_args = Some((offset_pages, pages));
            } else if kind != SERVICE_IMAGE_KIND_SERVICEMGR && pages != 0 {
                let Some(slot) = (kind as usize).checked_sub(1) else {
                    index += 1;
                    continue;
                };
                if slot < SERVICES && SERVICE_TABLE[slot].1 == kind {
                    pending[slot] = Some(Pending {
                        code: image,
                        offset_pages,
                        pages,
                        name: SERVICE_TABLE[slot].0,
                    });
                }
            }
            index += 1;
        }
    }
    let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, 1);
    Ok(())
}

/// SPEC-106/114：基础服务与用户态驱动（下标 ≥ [`BASE_FIRST`]）在共享段内的 id 见证偏移。
fn base_id_offset(slot: usize) -> u64 {
    [OFF_LOG_ID, OFF_NS_ID, OFF_STORE_ID, OFF_DRIVER_ID][slot - BASE_FIRST]
}

/// SPEC-106/114：基础服务与驱动在共享段内的存在标记见证偏移。
fn base_mark_offset(slot: usize) -> u64 {
    [OFF_LOG_MARK, OFF_NS_MARK, OFF_STORE_MARK, OFF_DRIVER_MARK][slot - BASE_FIRST]
}

/// SPEC-106/110/114：基础服务与驱动在共享段内的**应答见证起始**偏移（第 `index` 条读 `+ index*8`）。
fn base_reply_offset(slot: usize) -> u64 {
    [
        OFF_LOG_REPLY,
        OFF_NS_REPLY,
        OFF_STORE_REPLY,
        OFF_DRIVER_REPLY,
    ][slot - BASE_FIRST]
}

/// SPEC-112：日志/存储服务的**持久化段句柄**在共享段内的见证偏移（Namespace 服务不持段，槽位空）。
fn base_seg_offset(slot: usize) -> u64 {
    [OFF_LOG_SEG, 0, OFF_STORE_SEG][slot - BASE_FIRST]
}

/// SPEC-105/108：表驱动地创建一个端点——自解 ELF 入口 → **自建栈段** → `task_create` → 映射栈。
///
/// 创建者（本管理器）从自身 `self_cap` 派生指向自己的 `TaskSend` 凭据作 `initial_cap0`（端点据此
/// 自报/发 done 直达本管理器），从代码段派生只读令牌作 `initial_cap1`（非零 = 端点角色）。
///
/// **SPEC-108**：栈段由本管理器 `SYS_MEMSEG_CREATE` **自建**（不再由 `init` 移交）——服务任务的
/// 全部创建（代码段之外的栈）就此收敛到 ring3。`memseg_create` 铸出的根 cap 属主是本管理器，
/// 故随后可 `memseg_map` 进新端点根。
#[allow(clippy::too_many_arguments)]
fn create_service(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    ns_cap: Capability,
    spec: &Pending,
    slot: usize,
    created_slot: &mut [bool; SERVICES],
    c_stack: bool,
    c_args: Option<&CArgs>,
) -> Option<(Capability, Capability, u64, Capability, Option<Capability>)> {
    created_slot[slot] = true;
    let (Ok(initial0), Ok(initial1)) = (
        runtime.cap_derive(*self_cap, TASK_SEND_RIGHTS),
        runtime.cap_derive(spec.code, R_ONLY),
    ) else {
        return None;
    };
    // 自解 ELF 入口：把代码段只读映射到本管理器根内探测页读 `e_entry`（偏移 24..32，未被
    // 装载器覆写），随后卸载——入口不来自 init 的手写字面量。
    let Ok(()) = runtime.memseg_map(
        spec.code,
        *self_cap,
        spec.offset_pages,
        ENDPOINT_CODE_PROBE_VA,
        spec.pages,
        R_ONLY,
    ) else {
        let _ = runtime.cap_drop(initial0);
        let _ = runtime.cap_drop(initial1);
        return None;
    };
    let e_entry = runtime
        .read_user_u64(ENDPOINT_CODE_PROBE_VA + ELF_ENTRY_OFFSET)
        .unwrap_or(u64::MAX);
    let Some(writable) = parse_writable_segments(runtime, ENDPOINT_CODE_PROBE_VA, spec.pages)
    else {
        let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
        let _ = runtime.cap_drop(initial0);
        let _ = runtime.cap_drop(initial1);
        return None;
    };
    if e_entry == u64::MAX {
        let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
        let _ = runtime.cap_drop(initial0);
        let _ = runtime.cap_drop(initial1);
        return None;
    }
    // W^X：可写 `PT_LOAD` 不能映射共享映像页，也不能与可执行页同权。为每个可写范围建一块
    // **私有**段：按 vaddr 复制已重定位的映像字节、BSS 由新段清零，随后替换掉子任务中的
    // 只读代码页映射。
    let mut data_segments: [Option<(Capability, WritableSegment)>; WRITABLE_SEGMENTS] =
        [None; WRITABLE_SEGMENTS];
    for (slot, segment) in writable.into_iter().flatten().enumerate() {
        let Ok(data) = runtime.memseg_create(segment.pages, RW) else {
            drop_data_segments(runtime, &mut data_segments);
            let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
            let _ = runtime.cap_drop(initial0);
            let _ = runtime.cap_drop(initial1);
            return None;
        };
        if runtime
            .memseg_map(data, *self_cap, 0, DATA_PROBE_VA, segment.pages, RW)
            .is_err()
        {
            let _ = runtime.memseg_destroy(data);
            let _ = runtime.cap_drop(data);
            drop_data_segments(runtime, &mut data_segments);
            let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
            let _ = runtime.cap_drop(initial0);
            let _ = runtime.cap_drop(initial1);
            return None;
        }
        let destination = DATA_PROBE_VA + (segment.vaddr - segment.start);
        let mut copied = 0u64;
        let mut chunk = [0u8; 64];
        while copied < segment.file_size {
            let count = core::cmp::min(64, (segment.file_size - copied) as usize);
            let source = ENDPOINT_CODE_PROBE_VA + segment.vaddr + copied;
            if runtime
                .read_user_bytes(source, &mut chunk[..count])
                .is_err()
                || runtime
                    .write_user_bytes(destination + copied, &chunk[..count])
                    .is_err()
            {
                let _ = runtime.memseg_unmap(*self_cap, DATA_PROBE_VA, segment.pages);
                let _ = runtime.memseg_destroy(data);
                let _ = runtime.cap_drop(data);
                drop_data_segments(runtime, &mut data_segments);
                let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
                let _ = runtime.cap_drop(initial0);
                let _ = runtime.cap_drop(initial1);
                return None;
            }
            copied += count as u64;
        }
        let _ = runtime.memseg_unmap(*self_cap, DATA_PROBE_VA, segment.pages);
        data_segments[slot] = Some((data, segment));
    }
    let _ = runtime.memseg_unmap(*self_cap, ENDPOINT_CODE_PROBE_VA, spec.pages);
    // SPEC-108：自建栈段（`R|W`）；C 入口用多页，musl 初始化需要。
    // M3b：C 任务的匿名 arena 与栈同段，见 `C_HEAP_VA`。
    let stack_pages = if c_stack {
        C_STACK_PAGES
    } else {
        SERVICE_STACK_PAGES
    };
    let segment_pages = if c_stack {
        C_STACK_PAGES + C_HEAP_PAGES
    } else {
        SERVICE_STACK_PAGES
    };
    let Ok(stack) = runtime.memseg_create(segment_pages, RW) else {
        drop_data_segments(runtime, &mut data_segments);
        return None;
    };
    if c_stack && prepare_c_stack(runtime, self_cap, stack, c_args.unwrap_or(&PROBE_ARGS)).is_none()
    {
        let _ = runtime.memseg_destroy(stack);
        let _ = runtime.cap_drop(stack);
        let _ = runtime.cap_drop(initial0);
        let _ = runtime.cap_drop(initial1);
        drop_data_segments(runtime, &mut data_segments);
        return None;
    }
    let entry = ENDPOINT_CODE_VA + e_entry;
    let Ok(manager) = runtime.task_create(&TaskSpec {
        code_segment: spec.code,
        code_offset: spec.offset_pages,
        code_pages: spec.pages,
        load_base: ENDPOINT_CODE_VA,
        entry,
        stack_top: ENDPOINT_STACK_VA
            + if c_stack {
                C_STACK_OFFSET
            } else {
                SERVICE_STACK_PAGES * FRAME_SIZE
            },
        parent_namespace: Some(ns_cap),
        root_rights: NS_RESOLVE_RIGHTS,
        initial_capabilities: [Some(initial0), Some(initial1)],
        name: spec.name,
    }) else {
        let _ = runtime.memseg_destroy(stack);
        let _ = runtime.cap_drop(stack);
        let _ = runtime.cap_drop(initial0);
        let _ = runtime.cap_drop(initial1);
        drop_data_segments(runtime, &mut data_segments);
        return None;
    };
    // 独立栈段映射进新端点根（`memseg_map` 的目标根由 `manager` cap 的目标 task 派生）。
    // M3b：C 任务把同段的后续 64 页映射为固定 VA 的匿名 arena（无额外 create/audit）。
    let heap_mapped = !c_stack
        || runtime
            .memseg_map(stack, manager, C_STACK_PAGES, C_HEAP_VA, C_HEAP_PAGES, RW)
            .is_ok();
    let stack_mapped = runtime
        .memseg_map(stack, manager, 0, ENDPOINT_STACK_VA, stack_pages, RW)
        .is_ok();
    if !stack_mapped || !heap_mapped {
        let _ = runtime.task_destroy(manager);
        let _ = runtime.memseg_destroy(stack);
        let _ = runtime.cap_drop(stack);
        let _ = runtime.cap_drop(manager);
        drop_data_segments(runtime, &mut data_segments);
        return None;
    }
    // 用私有数据段替换子任务中的只读代码页：先解除 RX 映射，再以 RW 映射私有段。
    for data_slot in 0..WRITABLE_SEGMENTS {
        let Some((data, segment)) = data_segments[data_slot] else {
            continue;
        };
        let va = ENDPOINT_CODE_VA + segment.start;
        if runtime.memseg_unmap(manager, va, segment.pages).is_err()
            || runtime
                .memseg_map(data, manager, 0, va, segment.pages, RW)
                .is_err()
        {
            let _ = runtime.task_destroy(manager);
            let _ = runtime.memseg_destroy(stack);
            let _ = runtime.cap_drop(stack);
            let _ = runtime.cap_drop(manager);
            for (data, _) in data_segments.iter().flatten() {
                let _ = runtime.memseg_destroy(*data);
                let _ = runtime.cap_drop(*data);
            }
            return None;
        }
    }
    let data_cap = data_segments.iter().flatten().next().map(|(cap, _)| *cap);
    write_created_witness(runtime, slot, e_entry, initial1, manager);
    Some((initial1, manager, e_entry, stack, data_cap))
}

/// 失败回滚：销毁并归还所有已建的私有数据段（含尚未映射进子任务的那些）。
fn drop_data_segments(
    runtime: &Runtime<impl SyscallTransport>,
    segments: &mut [Option<(Capability, WritableSegment)>; WRITABLE_SEGMENTS],
) {
    for (data, _) in segments.iter_mut().flatten() {
        let _ = runtime.memseg_destroy(*data);
        let _ = runtime.cap_drop(*data);
    }
}

fn read_u16_at(runtime: &Runtime<impl SyscallTransport>, at: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    runtime.read_user_bytes(at, &mut bytes).ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u32_at(runtime: &Runtime<impl SyscallTransport>, at: u64) -> Option<u32> {
    let mut bytes = [0u8; 4];
    runtime.read_user_bytes(at, &mut bytes).ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// 从映射在 `image_va` 的 ELF 解析可写 `PT_LOAD` 范围。返回 `None` 表示 ELF 结构非法：
/// 宁可拒绝创建，也不静默丢掉数据段。
fn parse_writable_segments(
    runtime: &Runtime<impl SyscallTransport>,
    image_va: u64,
    image_pages: u64,
) -> Option<[Option<WritableSegment>; WRITABLE_SEGMENTS]> {
    let image_len = image_pages.checked_mul(FRAME_SIZE)?;
    let phoff = runtime.read_user_u64(image_va + ELF_PHOFF).ok()?;
    let phentsize = read_u16_at(runtime, image_va + ELF_PHENTSIZE)? as u64;
    let phnum = read_u16_at(runtime, image_va + ELF_PHNUM)? as u64;
    if phentsize < ELF_PH_SIZE || phnum > 16 {
        return None;
    }
    let table_end = phoff.checked_add(phnum.checked_mul(phentsize)?)?;
    if table_end > image_len {
        return None;
    }
    let mut segments: [Option<WritableSegment>; WRITABLE_SEGMENTS] = [None; WRITABLE_SEGMENTS];
    let mut count = 0usize;
    for index in 0..phnum {
        let at = image_va + phoff + index * phentsize;
        let kind = read_u32_at(runtime, at)?;
        let flags = read_u32_at(runtime, at + 4)?;
        if kind != PT_LOAD || flags & PF_W == 0 {
            continue;
        }
        let vaddr = runtime.read_user_u64(at + 16).ok()?;
        let file_size = runtime.read_user_u64(at + 32).ok()?;
        let mem_size = runtime.read_user_u64(at + 40).ok()?;
        if file_size > mem_size || mem_size == 0 {
            return None;
        }
        let start = vaddr & !(FRAME_SIZE - 1);
        let end = vaddr.checked_add(mem_size)?.checked_add(FRAME_SIZE - 1)? & !(FRAME_SIZE - 1);
        if end > image_len || vaddr.checked_add(file_size)? > image_len {
            return None;
        }
        let pages = (end - start) / FRAME_SIZE;
        if pages == 0 || count == WRITABLE_SEGMENTS {
            return None;
        }
        for previous in segments[..count].iter().flatten() {
            if start < previous.start + previous.pages * FRAME_SIZE
                && previous.start < start + pages * FRAME_SIZE
            {
                return None;
            }
        }
        segments[count] = Some(WritableSegment {
            start,
            pages,
            vaddr,
            file_size,
        });
        count += 1;
    }
    Some(segments)
}

/// M5：C 程序的 argv 集合（不含 envp；envp 固定为 `PRIMA_ABI=1`）。
#[derive(Clone, Copy)]
struct CArgs {
    count: u8,
    lens: [u8; 8],
    starts: [u8; 8],
    bytes: [u8; 32],
}

/// 固定探针的 argv：布局必须与 `stage5_probe.c` 的 `stage5_entry` 断言一致。
const PROBE_ARGS: CArgs = CArgs {
    count: 2,
    lens: [12, 11, 0, 0, 0, 0, 0, 0],
    starts: [0, 13, 0, 0, 0, 0, 0, 0],
    bytes: *b"stage5-probe\0stack-ready\0\0\0\0\0\0\0\0",
};

/// 读取 xtask 打包的 argv 块：`[count][len, bytes]...`。只映射一页，读毕即卸载。
fn read_c_args(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    image: Capability,
    offset_pages: u64,
    pages: u64,
) -> Option<CArgs> {
    if pages == 0 {
        return None;
    }
    let alias = ENDPOINT_CODE_PROBE_VA;
    runtime
        .memseg_map(image, *self_cap, offset_pages, alias, 1, R_ONLY)
        .ok()?;
    let blob = alias + SERVICE_ECHO_ARGS_OFFSET;
    let result = (|| {
        let mut header = [0u8; 1];
        runtime.read_user_bytes(blob, &mut header).ok()?;
        let count = header[0] as usize;
        if count == 0 || count > 8 {
            return None;
        }
        let mut args = CArgs {
            count: count as u8,
            lens: [0; 8],
            starts: [0; 8],
            bytes: [0; 32],
        };
        let mut at = 1u64;
        let mut used = 0usize;
        for index in 0..count {
            let mut length = [0u8; 1];
            runtime.read_user_bytes(blob + at, &mut length).ok()?;
            let len = length[0] as usize;
            if len == 0 || used + len > args.bytes.len() {
                return None;
            }
            runtime
                .read_user_bytes(blob + at + 1, &mut args.bytes[used..used + len])
                .ok()?;
            args.lens[index] = len as u8;
            args.starts[index] = used as u8;
            used += len;
            at += 1 + len as u64;
        }
        Some(args)
    })();
    let _ = runtime.memseg_unmap(*self_cap, alias, 1);
    result
}

/// 将 C 的 argc/argv/envp/auxv 写入同一栈段，再卸载管理器内的临时映射。
///
/// 布局冻结：rsp 指向 `[argc, argv...NULL, envp...NULL, auxv]`；字符串从栈顶页内
/// 偏移 0x80 起按 16 字节对齐排列，envp 固定为 `PRIMA_ABI=1`，auxv 仅
/// `AT_PAGESZ` + `AT_NULL`。
fn prepare_c_stack(
    runtime: &Runtime<impl SyscallTransport>,
    self_cap: &Capability,
    stack: Capability,
    args: &CArgs,
) -> Option<()> {
    let alias = ENDPOINT_CODE_PROBE_VA;
    runtime
        .memseg_map(stack, *self_cap, 0, alias, C_STACK_PAGES, RW)
        .ok()?;
    let base = ENDPOINT_STACK_VA + C_STACK_OFFSET;
    let count = args.count as usize;
    let mut words = [0u64; 16];
    if count + 8 > words.len() {
        let _ = runtime.memseg_unmap(*self_cap, alias, C_STACK_PAGES);
        return None;
    }
    words[0] = count as u64;
    let mut cursor = 0x80u64;
    let mut offsets = [0u64; 8];
    for index in 0..count {
        offsets[index] = cursor;
        words[1 + index] = base + cursor;
        cursor += args.lens[index] as u64 + 1;
        cursor = (cursor + 15) & !15;
    }
    words[1 + count] = 0;
    let env_offset = cursor;
    words[2 + count] = base + cursor;
    cursor += b"PRIMA_ABI=1\0".len() as u64;
    words[3 + count] = 0;
    words[4 + count] = 6;
    words[5 + count] = FRAME_SIZE;
    words[6 + count] = 0;
    words[7 + count] = 0;
    let limit = C_STACK_PAGES * FRAME_SIZE - C_STACK_OFFSET;
    let mut written = cursor <= limit;
    for (index, word) in words[..count + 8].iter().enumerate() {
        written &= runtime
            .write_user_bytes(
                alias + C_STACK_OFFSET + index as u64 * 8,
                &word.to_le_bytes(),
            )
            .is_ok();
    }
    for (index, offset) in offsets.iter().take(count).enumerate() {
        let len = args.lens[index] as usize;
        let start = args.starts[index] as usize;
        written &= runtime
            .write_user_bytes(
                alias + C_STACK_OFFSET + *offset,
                &args.bytes[start..start + len],
            )
            .is_ok();
        written &= runtime
            .write_user_bytes(alias + C_STACK_OFFSET + *offset + len as u64, &[0])
            .is_ok();
    }
    written &= runtime
        .write_user_bytes(alias + C_STACK_OFFSET + env_offset, b"PRIMA_ABI=1\0")
        .is_ok();
    let unmapped = runtime
        .memseg_unmap(*self_cap, alias, C_STACK_PAGES)
        .is_ok();
    (written && unmapped).then_some(())
}

/// SPEC-105：把「管理器确实创建了端点」的证据、收到的建端点参数与**自解出的 ELF 入口**
/// 写回共享段固定偏移（内核见证据此读回）。
fn write_created_witness(
    runtime: &Runtime<impl SyscallTransport>,
    slot: usize,
    entry_offset: u64,
    token: Capability,
    manager: Capability,
) {
    // 入口只记 `ALPHA`(槽 0)/`BETA`(槽 1) 的——基础服务不写该字段，避免覆盖既有见证。
    let entry_offset_field = match slot {
        0 => Some(OFF_MGR_ENTRY_A),
        1 => Some(OFF_MGR_ENTRY_B),
        _ => None,
    };
    if let Some(field) = entry_offset_field {
        let _ = runtime.write_user_bytes(USER_SEG_VA + field, &entry_offset.to_le_bytes());
    }
    // 令牌/管理 cap 只记 **ALPHA**（槽 0）的——内核 `service_negotiated_ready` 讲的是 ALPHA 的
    // 协商，而后建的端点/服务会覆盖同一偏移。
    if slot == 0 {
        let _ =
            runtime.write_user_bytes(USER_SEG_VA + OFF_MGR_TOKEN, &token.as_raw().to_le_bytes());
        let _ = runtime.write_user_bytes(
            USER_SEG_VA + OFF_MGR_MANAGER,
            &manager.as_raw().to_le_bytes(),
        );
    }
}

/// 转发一条 `prepare(behavior)` 无 cap 唤醒 + 一条携带共享段 cap 的消息给端点。
///
/// 保留两条**连续**发送的 SPEC-104 协议。阶段 2 直投修复后，内核已可把带 cap 消息
/// 直接递送给阻塞在 `recv` 的接收者；移除此唤醒需同步修改服务端协议和见证。
fn forward(runtime: &Runtime<impl SyscallTransport>, endpoint: &Endpoint, segment: Capability) {
    let receiver = TaskId::from_raw(endpoint.id as u32);
    let _ = runtime.send(
        receiver,
        endpoint.authority,
        SERVICE_PREPARE_TAG | endpoint.behavior,
        &[],
    );
    let _ = runtime.send(receiver, endpoint.authority, 0, &[segment]);
}

fn idle() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    idle()
}
