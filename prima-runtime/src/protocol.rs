//! 服务 wire protocol 的版本和 feature 协商。

/// 消息头固定长度。各字段显式按 little-endian 编码，不依赖 Rust 结构体布局。
pub const MESSAGE_HEADER_LEN: usize = 20;
/// Hello offer 固定长度。
pub const OFFER_LEN: usize = 24;
const MAGIC: [u8; 4] = *b"PRIM";

/// SPEC-103：ring3 服务管理器向 init **自报存活**的消息 tag（高 16 位）；低 8 位为自身 task id。
/// 与内核 `user::SERVICEMGR_HELLO_TAG` 同款。
pub const SERVICEMGR_HELLO_TAG: u64 = 0x5E56_0000_0000_0000;

/// SPEC-104：端点/客户端向服务管理器发送**带 cap 消息之前**先发的**无 cap 唤醒** tag。
///
/// 内核拒绝向处于 `BlockedRecv` 的接收者投递带 cap 消息（`kernel/src/syscall.rs::send_call`），
/// 而服务管理器常态阻塞在 `recv`，故发送方先发一条无 cap 消息直投唤醒它，再发带 cap 消息。
/// 该 tag 不被任何接收分支匹配，接收方忽略之。
pub const SERVICE_WAKE_TAG: u64 = 0x5ECA_0000_0000_0000;

/// SPEC-105：`init` 向服务管理器移交**建端点所需 cap 与参数**的消息 tag。
///
/// payload 布局：`tag(48..63) | payload_bits(8..47) | subtype(0..7)`；subtype 见
/// [`SERVICE_SETUP_CODE`]/[`SERVICE_SETUP_STACK`]。与端点自报/完成 tag 不相交。
pub const SERVICE_SETUP_TAG: u64 = 0x5ECC_0000_0000_0000;
/// SPEC-108：`init` 向服务管理器移交**服务代码段**的消息 tag。
///
/// payload 布局与 [`SERVICE_SETUP_TAG`] 相同（`code_pages(40..47) | name(0..39)`），但带
/// **一条** cap（cap0 = 代码段）。服务**栈段**不再移交——由管理器 `SYS_MEMSEG_CREATE` 自建。
/// 与 [`SERVICE_SETUP_TAG`]（父 Namespace，仅 tag 无 payload 位）以高 16 位区分。
pub const SERVICE_SETUP_CODE_TAG: u64 = 0x5ECD_0000_0000_0000;
/// SPEC-105：subtype——本消息携带某服务的**代码段 cap**（另一条 cap 槽留给栈段前一条）。
/// `payload_bits` 为 `code_pages`（1..=255）。
pub const SERVICE_SETUP_CODE: u64 = 1;
/// SPEC-105：subtype——本消息携带某服务的**栈段 cap**；`payload_bits` 为该服务**注册名**。
pub const SERVICE_SETUP_STACK: u64 = 2;
/// SPEC-105：端点在服务管理器根内的**代码探测 VA**：服务管理器把代码段映射到此以读 ELF 头
/// （`e_entry` 在偏移 24..32，未被装载器覆写）取入口，随后卸载。
pub const ENDPOINT_CODE_PROBE_VA: u64 = 0x0000_0A00_0000_8000;
/// SPEC-105：端点的代码基址（`task_create` 的 `load_base`；入口 = 此 + `e_entry`）。
pub const ENDPOINT_CODE_VA: u64 = 0x0000_0B00_0000_0000;
/// SPEC-105：端点的用户栈基址（单页；`stack_top = 此 + 0x1000`）。须等于内核
/// `user::USER3_STACK_VA`（`USER3_BASE_VA + 0x10_0000`），否则新端点的 `rsp` 落错。
pub const ENDPOINT_STACK_VA: u64 = 0x0000_0B00_0010_0000;
/// SPEC-105：ELF 头 `e_entry` 的偏移。
pub const ELF_ENTRY_OFFSET: u64 = 24;

/// EXO-2B/2D：服务映像目录格式由 `prima-abi` 共享，内核只写 opaque `kind` 条目；
/// 名字与 kind→behavior 映射由 `prima-user/src/servicemgr.rs` 持有。
pub use prima_abi::boot_image::{
    SERVICE_IMAGE_DIR_HEADER, SERVICE_IMAGE_ENTRY_LEN, SERVICE_IMAGE_MAGIC,
};

/// EXO-2D：manifest 中 ring3 服务管理器自身的 opaque `kind`。
///
/// 服务 kind 复用既有 `SERVICE_BEHAVIOR_*`（`ALPHA=1`…`DRIVER=6`），故服务管理器只按
/// `kind` 建端点，不再依赖目录顺序或内核预排。
pub const SERVICE_IMAGE_KIND_SERVICEMGR: u64 = 0x100;
/// 阶段 5：无 libc C 探针在服务映像目录中的 opaque kind。
pub const SERVICE_IMAGE_KIND_C_PROBE: u64 = 0x101;
/// 阶段 5 片 C：用户态 tty 字节流服务在服务映像目录中的 opaque kind。
pub const SERVICE_IMAGE_KIND_TTY: u64 = 0x102;
/// M5：`echo` 的一次性 argv 参数块。格式 `[count][len, bytes]...`，由 xtask 按冻结用例生成。
pub const SERVICE_IMAGE_KIND_ECHO_ARGS: u64 = 0x104;
/// M5：argv 块在参数载体页内的偏移。内核要求每个 manifest 条目都是合法 ELF，
/// 故参数块包在一个单页 `PT_LOAD` 的 ET_DYN 头之后；读方只看这个固定偏移。
pub const SERVICE_ECHO_ARGS_OFFSET: u64 = 0x100;
/// C 探针完成消息：低字节是 Task ID，次低字节是 C 见证值，bits 16..23 是 main 返回码。
pub const STAGE5_C_PROBE_TAG: u64 = 0xc500_0000_0000_5a00;

/// 阶段 5 片 C：tty 服务向服务管理器自报**就绪**的消息 tag；低 8 位是自身 task id。
pub const STAGE5_TTY_READY_TAG: u64 = 0xc600_0000_0000_0000;
/// 阶段 5 片 C：`write(1/2)` 的字节流消息 tag。C 探针发给管理器，管理器原样转发给 tty
/// 服务；bits 16..23 是 fd（仅 1/2 合法），bits 24..31 是字节，bits 32..39 是发送方 task id。
pub const STAGE5_TTY_WRITE_TAG: u64 = 0xc602_0000_0000_0000;
/// 阶段 5 片 C：tty 服务完成收发与 ioport 正反例后的 ack tag（管理器据此收尾）。
pub const STAGE5_TTY_ACK_TAG: u64 = 0xc603_0000_0000_0000;
/// M5：`echo` 用例的字节流 tag。与探针的 `STAGE5_TTY_WRITE_TAG` 分开，tty 只为探针做
/// `TTYIN|`/`TTYOUT|` 自检和两字节 ACK，避免诊断行混入 `echo` 的冻结 stdout。
pub const STAGE5_ECHO_WRITE_TAG: u64 = 0xc606_0000_0000_0000;
/// M5：管理器在 `echo` 退出后发给 tty 的**排空屏障**；tty 按序处理完此前所有字节后回 ACK。
pub const STAGE5_TTY_SYNC_TAG: u64 = 0xc604_0000_0000_0000;
/// M5：tty 对排空屏障的应答（管理器据此才输出 `ECHO|` 见证并收尾）。
pub const STAGE5_TTY_SYNC_ACK_TAG: u64 = 0xc605_0000_0000_0000;

/// SPEC-090：客户端在共享段内写入**请求服务名**的偏移（位于 20 B 头 + 24 B offer 之后）。
/// 内核侧 init（服务管理器）从同一物理帧的映射读回该名字以解析端点。
pub const SERVICE_NAME_OFFSET: u64 = 48;
/// SPEC-101：服务名是**最多 5 字节的 ASCII 令牌**，按 little-endian 打包进一个 u64 的低 40 位
/// （尾部补 `0`）。取代 SPEC-095 的 64 位数字码——名字从此是字符串而非魔数。
pub const SERVICE_NAME_MAX: usize = 5;
/// SPEC-101：有效名字位掩码（低 40 位）。
pub const SERVICE_NAME_MASK: u64 = 0x0000_00ff_ffff_ffff;

/// SPEC-101：把 ASCII 令牌打包为服务名（超过 5 字节截断）。与内核 `user::service_name` 同款。
pub const fn pack_service_name(bytes: &[u8]) -> u64 {
    let mut packed = 0u64;
    let mut index = 0;
    while index < bytes.len() && index < SERVICE_NAME_MAX {
        packed |= (bytes[index] as u64) << (8 * index);
        index += 1;
    }
    packed
}

/// SPEC-101：`ALPHA` 端点的服务名 `"alpha"`（打包值 `0x6168_706c_61`）。
pub const SERVICE_NAME_ALPHA: u64 = pack_service_name(b"alpha");
/// SPEC-101：`BETA` 端点的服务名 `"beta"`（打包值 `0x6174_6562`）。被服务管理器解析为行为码 2；
/// 未注册名解析为 0（拒绝）。
pub const SERVICE_NAME_BETA: u64 = pack_service_name(b"beta");
/// SPEC-106（S4-D3）：三个基础服务的注册名——`"log"`/`"nsvc"`/`"store"`（各 ≤5 字节，打包进 u64 低 40 位）。
pub const SERVICE_NAME_LOG: u64 = pack_service_name(b"log");
pub const SERVICE_NAME_NS: u64 = pack_service_name(b"nsvc");
pub const SERVICE_NAME_STORE: u64 = pack_service_name(b"store");
/// SPEC-114：第一个 ring3 **用户态驱动**的注册名 `"ioprt"`（≤5 字节）。
pub const SERVICE_NAME_DRIVER: u64 = pack_service_name(b"ioprt");

/// SPEC-106：基础服务向服务管理器自报**编译进二进制的存在标记**的消息 tag。
///
/// payload 布局：`tag(48..63) | kind(40..47) | marker(0..39)`。`kind` 是服务的行为码
/// （[`SERVICE_BEHAVIOR_LOG`]/[`SERVICE_BEHAVIOR_NS`]/[`SERVICE_BEHAVIOR_STORE`]），供管理器把
/// 标记归到正确的注册表槽；`marker` 是**每个二进制相异**的 40 位常量（[`SERVICE_MARKER_LOG`] 等）。
/// 内核据此逐字段核验「三个服务各由**不同二进制**运行」。
pub const SERVICE_MARKER_TAG: u64 = 0x5EC6_0000_0000_0000;
/// SPEC-106：marker 字段的位掩码（低 40 位）。
pub const SERVICE_MARKER_MASK: u64 = 0x0000_00ff_ffff_ffff;
/// SPEC-106：`kind` 字段在 marker 消息 payload 中的左移位数。
pub const SERVICE_MARKER_KIND_SHIFT: u64 = 40;
/// SPEC-106：三个基础服务的 40 位存在标记（互异，且与 `ALPHA`/`BETA` 的 64 位标记风格不相交）。
pub const SERVICE_MARKER_LOG: u64 = 0x0000_004c_4f47_0001;
pub const SERVICE_MARKER_NS: u64 = 0x0000_004e_5356_0002;
pub const SERVICE_MARKER_STORE: u64 = 0x0000_0053_5452_0003;
/// SPEC-114：用户态驱动的 40 位存在标记。
pub const SERVICE_MARKER_DRIVER: u64 = 0x0000_0049_4f50_0004;

/// SPEC-106：把存在标记打包成 marker 消息 payload。
pub const fn pack_service_marker(kind: u64, marker: u64) -> u64 {
    SERVICE_MARKER_TAG
        | ((kind & 0xff) << SERVICE_MARKER_KIND_SHIFT)
        | (marker & SERVICE_MARKER_MASK)
}

/// SPEC-106：服务管理器向基础服务发**请求**的消息 tag（与 `SERVICE_WAKE_TAG`(0x5ECA) 不相交）。
///
/// SPEC-110：请求 payload = `tag(48..63) | op(40..47) | arg(0..31)`。[`SERVICE_SERVE_OP_ADVANCE`] 用
/// `arg` 推进服务的内部状态，[`SERVICE_SERVE_OP_QUERY`] 只读回当前状态（不改）。**定序**多条
/// （推进、推进、查询）即判决服务**有状态**：纯回声端点会回显 `arg`，无法产生查询返回的状态值。
pub const SERVICE_SERVE_TAG: u64 = 0x5EC9_0000_0000_0000;
/// SPEC-110：请求 payload 里 `op` 字段（操作语义）的左移位数。
pub const SERVICE_SERVE_OP_SHIFT: u64 = 40;
/// SPEC-110：`op=0` 推进状态（服务按自身语义把 `arg` 并入内部状态）。
pub const SERVICE_SERVE_OP_ADVANCE: u64 = 0;
/// SPEC-110：`op=1` 只读回当前状态（不改）。
pub const SERVICE_SERVE_OP_QUERY: u64 = 1;
/// SPEC-111：`op=2` 让 **Namespace 服务**代理一次内核 `SYS_NS_RESOLVE`（`arg` 选编译期路径），
/// 应答低 32 位是内核解析出的 Capability 句柄（失败为 errno）。只有持有 `NsResolve` 根的任务
/// （`root_ns_cap`，见内核入口第 6 参数）能真正执行；无权限者拿到 errno。
pub const SERVICE_SERVE_OP_RESOLVE: u64 = 2;
/// SPEC-111：服务管理器向 Namespace 服务**移交**一份 `NsResolve` 只读 Namespace 副本的消息 tag
/// （带 cap；前置一条无 cap 唤醒）。最小权限：仅 `NsResolve` 位，服务据此代理 `SYS_NS_RESOLVE`。
pub const SERVICE_NS_CAP_TAG: u64 = 0x5ECB_0000_0000_0000;
/// SPEC-112（S4-D3 收口）：服务管理器向**日志/存储服务**移交其**持久化 MemorySegment** 的消息 tag
/// （带 cap；前置一条无 cap 唤醒）。cap 由管理器 `SYS_MEMSEG_CREATE` 铸出、运行期移交给对应服务，
/// 服务把它映射进自身根并作为**落盘存储**——日志服务用其落日志、存储服务用其落数据。
/// 只有 log/store 收到；Namespace 服务不持段（最小权限）。
pub const SERVICE_SEG_TAG: u64 = 0x5ECE_0000_0000_0000;
/// SPEC-112：日志/存储服务把移交来的段映射到**自身根**内的 VA（RW），供随后的状态读写。
/// 落在端点代码基址（`ENDPOINT_CODE_VA`）之上一页对齐的独立区域，不与代码/栈映射相交。
pub const SERVICE_DATA_VA: u64 = 0x0000_0B00_0020_0000;

/// 阶段 5 / M4：**存储服务文件后端**的请求 tag。C 适配层 → 服务管理器 → 存储服务。
///
/// payload 布局：`tag(48..63) | op(44..47) | fd(40..43) | client(32..39) | arg(0..31)`。
/// `client` 是发起请求的 Task id 低 8 位；管理器据此把应答回送给同一个 C 任务。
/// 单条消息不携带数据缓冲：write/read 每次最多 3 字节（[`SERVICE_FILE_CHUNK`]），由适配层分片，
/// 调用者仍看到完整的 `write`/`read` 语义。
pub const SERVICE_FILE_REQUEST_TAG: u64 = 0x5ED4_0000_0000_0000;
/// M4：存储服务的文件应答 tag（管理器据 `op`/`fd`/`status` 回送）。
pub const SERVICE_FILE_REPLY_TAG: u64 = 0x5ED5_0000_0000_0000;
pub const SERVICE_FILE_OP_SHIFT: u64 = 44;
pub const SERVICE_FILE_FD_SHIFT: u64 = 40;
pub const SERVICE_FILE_CLIENT_SHIFT: u64 = 32;
/// 应答布局复用 `32..39` 作为 `status`（0 或正 errno）。
pub const SERVICE_FILE_STATUS_SHIFT: u64 = 32;
pub const SERVICE_FILE_ARG_MASK: u64 = 0xffff_ffff;

pub const SERVICE_FILE_OP_OPEN: u64 = 0;
pub const SERVICE_FILE_OP_READ: u64 = 1;
pub const SERVICE_FILE_OP_WRITE: u64 = 2;
pub const SERVICE_FILE_OP_LSEEK: u64 = 3;
pub const SERVICE_FILE_OP_CLOSE: u64 = 4;
/// M9：路径元数据查询（`statx`）的应答 op；不复用 OPEN，便于区分“只读查询、无句柄”。
pub const SERVICE_FILE_OP_STAT: u64 = 5;

/// M4：open 的 `arg` 布局——`flags(28..31) | path_len(26..27) | path(0..23)`。
pub const SERVICE_FILE_OPEN_FLAGS_SHIFT: u64 = 28;
pub const SERVICE_FILE_OPEN_PATH_LEN_SHIFT: u64 = 26;
pub const SERVICE_FILE_OPEN_PATH_MASK: u64 = 0x00ff_ffff;
/// M4：适配层归一化后的 open flags。access 位于 bit 2..3（0=只读、1=只写、2=读写）。
pub const SERVICE_FILE_FLAG_CREAT: u64 = 1;
pub const SERVICE_FILE_FLAG_TRUNC: u64 = 2;
pub const SERVICE_FILE_ACCESS_SHIFT: u64 = 2;

/// M4：write 的 `arg` 布局——`count(24..31) | data(0..23)`。
pub const SERVICE_FILE_WRITE_COUNT_SHIFT: u64 = 24;
/// M4：lseek 的 `arg` 布局——`whence(30..31) | offset(0..29，二补码)`。
pub const SERVICE_FILE_SEEK_WHENCE_SHIFT: u64 = 30;
pub const SERVICE_FILE_SEEK_OFFSET_MASK: u64 = 0x3fff_ffff;

/// M4：单条消息可携带的字节数；适配层负责把更长的调用分片。
pub const SERVICE_FILE_CHUNK: u64 = 3;
/// M4：存储在共享段内的布局——偏移 0..8 仍是 SPEC-112 的 `u64` 状态，
/// 8..16 是文件长度，16 起是文件内容。
pub const SERVICE_FILE_SIZE_OFFSET: u64 = 8;
pub const SERVICE_FILE_DATA_OFFSET: u64 = 16;
pub const SERVICE_FILE_CAPACITY: u64 = 4096 - SERVICE_FILE_DATA_OFFSET;
/// M4：首批冻结的文件名（单层、≤3 字节、无 `/`）。保留为历史常量；M8 起 Namespace
/// 授权入口绑定的是根文件系统授权路径 [`SERVICE_VFS_ROOT_PATH`]。
pub const SERVICE_FILE_PATH: &str = "f";
/// M8：C 任务根 Namespace 中的 **rootfs 授权入口**。管理器在创建 C 任务后把存储服务的
/// `TaskSend` 令牌绑到该路径并解析一次；此后 VFS 操作由这次授权放行，不再逐次解析文件
/// 路径——Namespace 是授权入口，文件树在用户态存储服务里。
pub const SERVICE_VFS_ROOT_PATH: &str = "fs";
/// M8：rootfs 路径字节上限（含 `/`，不含结尾 NUL）。
pub const SERVICE_VFS_PATH_MAX: usize = 64;
/// M8：rootfs 节点上限（与 `prima_runtime::vfs::MAX_NODES` 一致）。
pub const SERVICE_VFS_NODES: usize = 32;

/// M8：`openat` 的分阶段路径传输——先 OPEN 元数据，再若干 PATH 片段，最后 COMMIT。
///
/// 64 位 payload 只有 48 位可用，路径上限 64 字节，故不能塞进单条消息：
/// - [`SERVICE_VFS_OPEN_TAG`]：`tag | flags(40..47) | path_len(32..39) | start(24..31) |`
///   `前 3 字节(0..23)`，清空暂存路径并写入第一段；
/// - [`SERVICE_VFS_PATH_TAG`]：`tag | offset(40..47) | 5 字节(0..39)`；
/// - [`SERVICE_VFS_COMMIT_TAG`]：`tag | client(32..39)`，解析暂存路径并回复。
///
/// OPEN/PATH 只入队由服务端暂存，只有 COMMIT 同步等待一条应答；适配层据此不会一次
/// 灌满全局消息队列。
pub const SERVICE_VFS_OPEN_TAG: u64 = 0x5ED6_0000_0000_0000;
pub const SERVICE_VFS_PATH_TAG: u64 = 0x5ED7_0000_0000_0000;
pub const SERVICE_VFS_COMMIT_TAG: u64 = 0x5ED8_0000_0000_0000;
pub const SERVICE_VFS_FLAGS_SHIFT: u64 = 40;
pub const SERVICE_VFS_PATH_LEN_SHIFT: u64 = 32;
pub const SERVICE_VFS_START_SHIFT: u64 = 24;
pub const SERVICE_VFS_FIRST_BYTES_MASK: u64 = 0x00ff_ffff;
pub const SERVICE_VFS_PATH_OFFSET_SHIFT: u64 = 40;
pub const SERVICE_VFS_PATH_BYTES_MASK: u64 = 0x0000_00ff_ffff_ffff;
pub const SERVICE_VFS_COMMIT_CLIENT_SHIFT: u64 = 32;

/// M8/M9：适配层归一化后的 open/stat flags（低 8 位）。access 位于 bit 2..3。
pub const SERVICE_VFS_FLAG_CREATE: u64 = 1 << 0;
pub const SERVICE_VFS_FLAG_TRUNC: u64 = 1 << 1;
pub const SERVICE_VFS_ACCESS_SHIFT: u64 = 2;
pub const SERVICE_VFS_FLAG_APPEND: u64 = 1 << 4;
pub const SERVICE_VFS_FLAG_EXCLUSIVE: u64 = 1 << 5;
pub const SERVICE_VFS_FLAG_DIRECTORY: u64 = 1 << 6;
/// M9：只解析路径并返回元数据，不分配服务端句柄、不做 access 检查。
pub const SERVICE_VFS_FLAG_STAT: u64 = 1 << 7;
/// M9：`flags` 字段宽度（bit 40..47）。
pub const SERVICE_VFS_FLAGS_MASK: u64 = 0xff;

/// M8/M9/M13b：open/stat 成功应答的 `data` 布局——
/// `node(0..7) | kind(8..9) | writable(10) | mode(11..19) | size(20..31)`。
/// mode 只承载 0..0o777；`size` 上限 4095（内容区实际更小）。
pub const SERVICE_VFS_REPLY_NODE_MASK: u64 = 0xff;
pub const SERVICE_VFS_REPLY_KIND_SHIFT: u64 = 8;
pub const SERVICE_VFS_REPLY_KIND_MASK: u64 = 0x3;
pub const SERVICE_VFS_REPLY_WRITABLE_SHIFT: u64 = 10;
pub const SERVICE_VFS_REPLY_MODE_SHIFT: u64 = 11;
pub const SERVICE_VFS_REPLY_MODE_MASK: u64 = 0x1ff;
pub const SERVICE_VFS_REPLY_SIZE_SHIFT: u64 = 20;
pub const SERVICE_VFS_REPLY_SIZE_MASK: u64 = 0xfff;

/// M8：打包 OPEN 元数据（`first` 至多 3 字节）。
pub fn pack_vfs_open(flags: u64, path_len: usize, start: u64, first: &[u8]) -> u64 {
    let mut bytes = 0u64;
    let mut index = 0;
    while index < first.len() && index < 3 {
        bytes |= (first[index] as u64) << (8 * index);
        index += 1;
    }
    SERVICE_VFS_OPEN_TAG
        | ((flags & SERVICE_VFS_FLAGS_MASK) << SERVICE_VFS_FLAGS_SHIFT)
        | (((path_len as u64) & 0xff) << SERVICE_VFS_PATH_LEN_SHIFT)
        | ((start & 0xff) << SERVICE_VFS_START_SHIFT)
        | bytes
}

/// M8：打包 PATH 片段（`bytes` 至多 5 字节）。
pub fn pack_vfs_path(offset: usize, bytes: &[u8]) -> u64 {
    let mut packed = 0u64;
    let mut index = 0;
    while index < bytes.len() && index < 5 {
        packed |= (bytes[index] as u64) << (8 * index);
        index += 1;
    }
    SERVICE_VFS_PATH_TAG | (((offset as u64) & 0xff) << SERVICE_VFS_PATH_OFFSET_SHIFT) | packed
}

/// M8：打包 COMMIT，`client` 是发起 open 的 Task id 低 8 位。
pub const fn pack_vfs_commit(client: u64) -> u64 {
    SERVICE_VFS_COMMIT_TAG | ((client & 0xff) << SERVICE_VFS_COMMIT_CLIENT_SHIFT)
}

/// M8/M9/M13b：打包 open/stat 应答的 `data` 字段。`mode` 只取低 9 位（0..0o777）。
pub const fn pack_vfs_node(node: u64, kind: u64, size: u64, mode: u16) -> u64 {
    (node & SERVICE_VFS_REPLY_NODE_MASK)
        | ((kind & SERVICE_VFS_REPLY_KIND_MASK) << SERVICE_VFS_REPLY_KIND_SHIFT)
        | (((mode & 0o200 != 0) as u64) << SERVICE_VFS_REPLY_WRITABLE_SHIFT)
        | (((mode as u64) & SERVICE_VFS_REPLY_MODE_MASK) << SERVICE_VFS_REPLY_MODE_SHIFT)
        | ((size & SERVICE_VFS_REPLY_SIZE_MASK) << SERVICE_VFS_REPLY_SIZE_SHIFT)
}
/// M11：单路径文件树变更（`mkdir`/`unlink`/`rmdir`）。
///
/// `tag | op(44..47) | path_len(32..39) | start(24..31) | 前 3 字节`，随后复用
/// [`SERVICE_VFS_PATH_TAG`] 片段；COMMIT 触发变更并回复一条 `FILE_REPLY`。
pub const SERVICE_VFS_MUTATE_TAG: u64 = 0x5EDB_0000_0000_0000;
pub const SERVICE_VFS_MUTATE_OP_SHIFT: u64 = 44;
pub const SERVICE_VFS_MUTATE_OP_MASK: u64 = 0xf;
pub const SERVICE_VFS_OP_MKDIR: u64 = 0;
pub const SERVICE_VFS_OP_UNLINK: u64 = 1;
pub const SERVICE_VFS_OP_RMDIR: u64 = 2;

/// M11：打包一次变更请求的元数据（`first` 至多 3 字节）。
pub fn pack_vfs_mutate(op: u64, path_len: usize, start: u64, first: &[u8]) -> u64 {
    let mut bytes = 0u64;
    for (index, byte) in first.iter().take(3).enumerate() {
        bytes |= (*byte as u64) << (8 * index);
    }
    SERVICE_VFS_MUTATE_TAG
        | ((op & SERVICE_VFS_MUTATE_OP_MASK) << SERVICE_VFS_MUTATE_OP_SHIFT)
        | (((path_len as u64) & 0xff) << SERVICE_VFS_PATH_LEN_SHIFT)
        | ((start & 0xff) << SERVICE_VFS_START_SHIFT)
        | bytes
}

/// M12b：存储服务只读状态查询——返回当前打开的句柄数（0..8）。
///
/// `tag | client(32..39)`；应答 `tag | client(32..39) | open_count(0..7)`。
pub const SERVICE_VFS_STATUS_TAG: u64 = 0x5EDC_0000_0000_0000;
pub const SERVICE_VFS_STATUS_CLIENT_SHIFT: u64 = 32;
pub const SERVICE_VFS_STATUS_COUNT_MASK: u64 = 0xff;

/// M13a：双字符串路径操作（rename/symlink）与 readlink 的分段暂存。
///
/// 复用 [`SERVICE_VFS_PATH_TAG`] 的片段格式，但服务端有两个暂存槽：
/// - A（slot 0）：rename 的源路径、symlink 的链接路径、readlink 的查询路径；
/// - B（slot 1）：rename 的目标路径、symlink 的目标字符串（不按路径解析）。
///
/// OPEN：`tag | slot(47) | op(44..46) | path_len(32..39) | start(24..31) | 前 3 字节`；
/// COMMIT：`tag | slot(47) | op(44..46) | client(32..39)`。A 的 COMMIT 只确认暂存，
/// B 的 COMMIT（或 readlink 的 A COMMIT）执行操作并回复
/// [`SERVICE_VFS_PAIR_REPLY_TAG`]。readlink 的目标字节随后用
/// [`SERVICE_VFS_LINK_READ_TAG`] 按 3 字节分段取回。
pub const SERVICE_VFS_PAIR_TAG: u64 = 0x5EDD_0000_0000_0000;
pub const SERVICE_VFS_PAIR_SLOT_SHIFT: u64 = 47;
pub const SERVICE_VFS_PAIR_OP_SHIFT: u64 = 44;
pub const SERVICE_VFS_PAIR_OP_MASK: u64 = 0x7;
pub const SERVICE_VFS_PAIR_OP_RENAME: u64 = 0;
pub const SERVICE_VFS_PAIR_OP_SYMLINK: u64 = 1;
pub const SERVICE_VFS_PAIR_OP_READLINK: u64 = 2;

/// 打包一次双路径 OPEN（`first` 至多 3 字节）。
pub fn pack_vfs_pair_open(op: u64, slot: u64, path_len: usize, start: u64, first: &[u8]) -> u64 {
    let mut bytes = 0u64;
    for (index, byte) in first.iter().take(3).enumerate() {
        bytes |= (*byte as u64) << (8 * index);
    }
    SERVICE_VFS_PAIR_TAG
        | ((slot & 1) << SERVICE_VFS_PAIR_SLOT_SHIFT)
        | ((op & SERVICE_VFS_PAIR_OP_MASK) << SERVICE_VFS_PAIR_OP_SHIFT)
        | (((path_len as u64) & 0xff) << SERVICE_VFS_PATH_LEN_SHIFT)
        | ((start & 0xff) << SERVICE_VFS_START_SHIFT)
        | bytes
}

/// COMMIT：`tag | slot(47) | op(44..46) | client(32..39)`。
pub const SERVICE_VFS_PAIR_COMMIT_TAG: u64 = 0x5EDE_0000_0000_0000;

/// 打包 COMMIT，`client` 是发起操作的 Task id 低 8 位。
pub const fn pack_vfs_pair_commit(op: u64, slot: u64, client: u64) -> u64 {
    SERVICE_VFS_PAIR_COMMIT_TAG
        | ((slot & 1) << SERVICE_VFS_PAIR_SLOT_SHIFT)
        | ((op & SERVICE_VFS_PAIR_OP_MASK) << SERVICE_VFS_PAIR_OP_SHIFT)
        | ((client & 0xff) << SERVICE_VFS_STATUS_CLIENT_SHIFT)
}

/// 双路径操作应答：`tag | op(44..46) | status(32..39) | data(0..31)`。
pub const SERVICE_VFS_PAIR_REPLY_TAG: u64 = 0x5EDF_0000_0000_0000;
pub const SERVICE_VFS_PAIR_REPLY_OP_SHIFT: u64 = 44;

/// 打包双路径操作应答；readlink 的 `data` 是目标长度。
pub const fn pack_vfs_pair_reply(op: u64, status: u64, data: u64) -> u64 {
    SERVICE_VFS_PAIR_REPLY_TAG
        | ((op & SERVICE_VFS_PAIR_OP_MASK) << SERVICE_VFS_PAIR_REPLY_OP_SHIFT)
        | ((status & 0xff) << SERVICE_VFS_STATUS_CLIENT_SHIFT)
        | (data & SERVICE_FILE_ARG_MASK)
}

/// readlink 目标分段读取：`tag | offset(32..39) | client(40..47)`；
/// 应答 `tag | status(32..39) | 3 字节(0..23)`。
pub const SERVICE_VFS_LINK_READ_TAG: u64 = 0x5EE0_0000_0000_0000;
/// readlink 目标分段的应答（与请求分开，管理器据此单向转发）。
pub const SERVICE_VFS_LINK_REPLY_TAG: u64 = 0x5EE1_0000_0000_0000;
pub const SERVICE_VFS_LINK_OFFSET_SHIFT: u64 = 32;
pub const SERVICE_VFS_LINK_CLIENT_SHIFT: u64 = 40;
pub const SERVICE_VFS_LINK_REPLY_SHIFT: u64 = 32;
pub const SERVICE_VFS_LINK_CHUNK: usize = 3;

/// M13b：`chmod`/`fchmod` 的单路径请求。`tag | start(24..31) | path_len(32..39) |
/// 前 3 字节`；PATH 片段复用 [`SERVICE_VFS_PATH_TAG`]；COMMIT 的 bits 8..16 携带
/// mode（9 位），bits 32..39 仍是 client。路径为空时改 `start` 节点（`fchmod`）。
pub const SERVICE_VFS_CHMOD_TAG: u64 = 0x5EE2_0000_0000_0000;
pub const SERVICE_VFS_CHMOD_MODE_SHIFT: u64 = 8;
pub const SERVICE_VFS_CHMOD_MODE_MASK: u64 = 0x1ff;

/// 打包一次 chmod 的 OPEN 元数据（`first` 至多 3 字节）。
pub fn pack_vfs_chmod(start: u64, path_len: usize, first: &[u8]) -> u64 {
    let mut bytes = 0u64;
    for (index, byte) in first.iter().take(3).enumerate() {
        bytes |= (*byte as u64) << (8 * index);
    }
    SERVICE_VFS_CHMOD_TAG
        | ((start & 0xff) << SERVICE_VFS_START_SHIFT)
        | (((path_len as u64) & 0xff) << SERVICE_VFS_PATH_LEN_SHIFT)
        | bytes
}

/// M4/M8：同时打开的文件/目录服务端句柄上限（服务端 fd 3..10）。
pub const SERVICE_FILE_HANDLES: usize = 8;

/// M10：C 任务向服务管理器申请 arena Segment 的派生映射授权。
///
/// 请求 `tag | client(16..23)`；应答 `tag | status(0..7)`，成功时随消息转移一条
/// `Derive|SegMapR|SegMapW` 的派生 Capability。C 任务只在首次 `mmap` 时申请一次，
/// 之后 `memseg_map`/`memseg_unmap`/`memseg_protect` 直接走原生号。
pub const SERVICE_VMA_CAP_REQUEST_TAG: u64 = 0x5ED9_0000_0000_0000;
pub const SERVICE_VMA_CAP_REPLY_TAG: u64 = 0x5EDA_0000_0000_0000;
pub const SERVICE_VMA_CLIENT_SHIFT: u64 = 16;
pub const SERVICE_VMA_STATUS_OK: u64 = 0;
pub const SERVICE_VMA_STATUS_UNAVAILABLE: u64 = 1;

/// M10：打包 arena 映射授权请求，`client` 是发起 mmap 的 Task id 低 8 位。
pub const fn pack_vma_cap_request(client: u64) -> u64 {
    SERVICE_VMA_CAP_REQUEST_TAG | ((client & 0xff) << SERVICE_VMA_CLIENT_SHIFT)
}

/// M10：打包 arena 映射授权应答（成功时由发送方附加 Capability）。
pub const fn pack_vma_cap_reply(status: u64) -> u64 {
    SERVICE_VMA_CAP_REPLY_TAG | (status & 0xff)
}

/// M4：文件服务的 errno（Linux x86_64 数值，profile v0 已冻结）。
pub const SERVICE_FILE_ENOENT: u64 = 2;
pub const SERVICE_FILE_EBADF: u64 = 9;
pub const SERVICE_FILE_EAGAIN: u64 = 11;
pub const SERVICE_FILE_EACCES: u64 = 13;
pub const SERVICE_FILE_EFAULT: u64 = 14;
pub const SERVICE_FILE_EEXIST: u64 = 17;
pub const SERVICE_FILE_ENOTDIR: u64 = 20;
pub const SERVICE_FILE_EINVAL: u64 = 22;
pub const SERVICE_FILE_ENOSPC: u64 = 28;
pub const SERVICE_FILE_ENOSYS: u64 = 38;
/// M13a：`ELOOP`（符号链接展开超过冻结深度）。
pub const SERVICE_FILE_ELOOP: u64 = 40;

/// M4：打包文件服务请求。
pub const fn pack_file_request(op: u64, fd: u64, client: u64, arg: u64) -> u64 {
    SERVICE_FILE_REQUEST_TAG
        | ((op & 0xf) << SERVICE_FILE_OP_SHIFT)
        | ((fd & 0xf) << SERVICE_FILE_FD_SHIFT)
        | ((client & 0xff) << SERVICE_FILE_CLIENT_SHIFT)
        | (arg & SERVICE_FILE_ARG_MASK)
}

/// M4：打包文件服务应答；`status` 为 0 或正 errno。
pub const fn pack_file_reply(op: u64, fd: u64, status: u64, data: u64) -> u64 {
    SERVICE_FILE_REPLY_TAG
        | ((op & 0xf) << SERVICE_FILE_OP_SHIFT)
        | ((fd & 0xf) << SERVICE_FILE_FD_SHIFT)
        | ((status & 0xff) << SERVICE_FILE_STATUS_SHIFT)
        | (data & SERVICE_FILE_ARG_MASK)
}

/// M4：打包 open 的 `arg`（`flags` 为适配层归一化后的 4 位）。
pub fn pack_file_open_arg(flags: u64, path: &[u8]) -> u64 {
    let mut path_bits = 0u64;
    let mut index = 0;
    while index < path.len() && index < 3 {
        path_bits |= (path[index] as u64) << (8 * index);
        index += 1;
    }
    ((flags & 0xf) << SERVICE_FILE_OPEN_FLAGS_SHIFT)
        | (((path.len() as u64) & 0x3) << SERVICE_FILE_OPEN_PATH_LEN_SHIFT)
        | (path_bits & SERVICE_FILE_OPEN_PATH_MASK)
}

/// 原生应用共享段的第 1..3 页在客户端和管理器中的独立映射，避开 Hello 页与 ELF 探针。
pub const NATIVE_APP_DATA_VA: u64 = 0x0000_0A10_0000_0000;
pub const NATIVE_APP_BYTES: usize = 8192;
pub const NATIVE_APP_SENTINEL_OFFSET: u64 = 8192;
pub const NATIVE_APP_SENTINEL: u64 = 0x5049_4e4e_4544_4f4b;
pub const NATIVE_APP_REQUEST_TAG: u64 = 0x5ED1_0000_0000_0000;
pub const NATIVE_APP_REPLY_TAG: u64 = 0x5ED2_0000_0000_0000;
pub const NATIVE_APP_DONE_TAG: u64 = 0x5ED3_0000_0000_0000;
pub const NATIVE_APP_OP_DATA: u64 = 1;
pub const NATIVE_APP_OP_CANCEL: u64 = 2;
pub const NATIVE_APP_OP_SUBMIT: u64 = 3;
pub const NATIVE_APP_OP_EXECUTE: u64 = 4;
pub const NATIVE_APP_OK: u64 = 0;
pub const NATIVE_APP_CANCELLED: u64 = 1;
pub const NATIVE_APP_BAD_LENGTH: u64 = 2;
pub const NATIVE_APP_QUEUED: u64 = 3;

pub const fn native_app_request(op: u64, length: u64) -> u64 {
    NATIVE_APP_REQUEST_TAG | ((op & 0xff) << 40) | (length & 0xffff_ffff)
}

pub const fn native_app_reply(status: u64, checksum: u32) -> u64 {
    NATIVE_APP_REPLY_TAG | ((status & 0xff) << 40) | checksum as u64
}

pub const fn native_app_byte(index: usize, seed: u8) -> u8 {
    (index as u8).wrapping_mul(17).wrapping_add(seed)
}

pub const fn native_app_hash(hash: u32, byte: u8) -> u32 {
    (hash ^ byte as u32).wrapping_mul(16_777_619)
}

pub fn native_app_expected_hash(seed: u8) -> u32 {
    let mut hash = 2_166_136_261u32;
    let mut index = 0;
    while index < NATIVE_APP_BYTES {
        hash = native_app_hash(hash, native_app_byte(index, seed));
        index += 1;
    }
    hash
}
/// SPEC-110：定序请求的两个推进入参（5 → 3）。三个基础服务的状态转移语义**各异**（日志累加 /
/// 存储移位追加 / Namespace 滚动指纹），故两次推进后查询返回的**三个值互异**——判决「请求经服务
/// 表路由到了各自的**有状态**服务二进制，而非任一无状态回声端点」。
pub const SERVICE_SERVE_ARG_A: u64 = 5;
pub const SERVICE_SERVE_ARG_B: u64 = 3;
/// SPEC-110：每个基础服务的定序请求条数（推进、推进、查询）。
pub const SERVICE_SERVE_PROBES: u64 = 3;
/// SPEC-110：把服务请求打包（`op` 为操作语义，`arg` 为低 32 位入参）。
pub const fn pack_service_request(op: u64, arg: u64) -> u64 {
    SERVICE_SERVE_TAG | ((op & 0xff) << SERVICE_SERVE_OP_SHIFT) | (arg & 0xffff_ffff)
}
/// SPEC-106：基础服务应答里 `kind` 字段的左移位数——应答 = `SERVICE_SERVE_TAG | kind<<32 | arg`。
/// 各服务应答的 `kind` 相异，构成「请求经服务表路由到了**正确的**服务二进制」的判决性字段。
pub const SERVICE_REPLY_KIND_SHIFT: u64 = 32;
/// SPEC-106/110：把服务应答打包（`kind` 为服务的行为码，`arg` 为低 32 位**当前状态**）。
pub const fn pack_service_reply(kind: u64, arg: u64) -> u64 {
    SERVICE_SERVE_TAG | ((kind & 0xff) << SERVICE_REPLY_KIND_SHIFT) | (arg & 0xffff_ffff)
}
/// SPEC-090：服务管理器可路由的行为码。`ALPHA`（默认，max 2048）与 `BETA`（max 1024）。
pub const SERVICE_BEHAVIOR_ALPHA: u64 = 1;
pub const SERVICE_BEHAVIOR_BETA: u64 = 2;
/// SPEC-106（S4-D3）：三个**基础服务**端点——日志、Namespace、基础存储。与 `ALPHA`/`BETA`
/// 共用同一服务表/注册表/创建路径，只是不参与 wire 协商，而是自报**二进制标记**作存在证据。
pub const SERVICE_BEHAVIOR_LOG: u64 = 3;
pub const SERVICE_BEHAVIOR_NS: u64 = 4;
pub const SERVICE_BEHAVIOR_STORE: u64 = 5;
/// SPEC-114（S4-D4）：**用户态驱动**端点——不参与 §15 的定序状态探针，而是收到一个内核
/// `Device(IOPortRange)` Capability 后经受控端口路径做一次 I/O（见 [`SERVICE_SERVE_OP_IOPORT_RW`]）。
pub const SERVICE_BEHAVIOR_DRIVER: u64 = 6;

/// SPEC-114 / §15.5：服务管理器向**用户态驱动**移交内核 `Device(IOPortRange)` Capability 的消息
/// tag（带 cap；前置一条无 cap 唤醒）。cap 由管理器 `SYS_DEV_IOPORT_CREATE` 铸出，驱动据此走
/// **受控端口路径**读/写端口——端口落在区间内才被内核放行。
pub const SERVICE_DEV_CAP_TAG: u64 = 0x5ECF_0000_0000_0000;
/// SPEC-114：`op=3` 受控端口往返——驱动在授权区间内的 Scratch 寄存器写一个魔数再读回，
/// 应答低 32 位是**读回的字节**（等于魔数即证明设备往返成立）。
pub const SERVICE_SERVE_OP_IOPORT_RW: u64 = 3;
/// SPEC-114：`op=4` 越界端口探测——驱动尝试写一个**区间外**端口；内核须拒绝（应答低 32 位 = 0
/// 表示被拒，= 1 表示竟被放行，即受控路径失效）。
pub const SERVICE_SERVE_OP_IOPORT_OOR: u64 = 4;
/// SPEC-114：驱动的授权端口区间（COM1，`0x3F8..0x400`）与其中的 Scratch 寄存器（offset 7）。
pub const IOPORT_BASE: u16 = 0x3F8;
pub const IOPORT_COUNT: u16 = 8;
pub const IOPORT_SCRATCH: u16 = 0x3FF;
/// SPEC-114：区间**外**的探测端口（COM2 数据口，未被委派）。
pub const IOPORT_OUT_OF_RANGE: u16 = 0x2F8;
/// SPEC-114：写入 Scratch 再读回的魔数（16550 SCR 按字节存，任意非零值均可往返）。
pub const IOPORT_MAGIC: u8 = 0x5A;
/// SPEC-090：名字未注册时的解析结果（服务管理器据此拒绝转发）。
pub const SERVICE_BEHAVIOR_NONE: u64 = 0;
/// SPEC-088：`ALPHA` 行为的服务端 offer。`max_message_size` 取 2048，与客户端 4096 取交后
/// 得到 `max=2048`——「结果只可能来自服务端」的判决性字段。
pub const SERVICE_OFFER_ALPHA: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 2048,
};
/// SPEC-090：`BETA` 行为的服务端 offer。`max_message_size` 取 1024 而非 2048，使协商结果
/// （`max=1024`）**只可能来自被按名选中的 BETA 行为**，构成路由的判决性字段。
pub const SERVICE_OFFER_BETA: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 1024,
};

/// SPEC-093：服务管理器路由到的行为码 → 服务端 offer。`ALPHA` 用 [`SERVICE_OFFER_ALPHA`]、
/// `BETA` 用 [`SERVICE_OFFER_BETA`]，使协商结果**承载路由决策**。
pub const fn service_offer(behavior: u64) -> ProtocolOffer {
    if behavior == SERVICE_BEHAVIOR_BETA {
        SERVICE_OFFER_BETA
    } else {
        SERVICE_OFFER_ALPHA
    }
}

/// SPEC-088：把协商结果打包成一个字（`major<<48 | minor<<32 | max<<16 | features 低 16 位`）。
pub const fn pack_negotiated(negotiated: NegotiatedProtocol) -> u64 {
    ((negotiated.major as u64) << 48)
        | ((negotiated.minor as u64) << 32)
        | ((negotiated.max_message_size as u64) << 16)
        | (negotiated.features & 0xffff)
}

/// SPEC-088：解析对端 Hello 并用给定服务端 offer 协商；成功返回打包结果，失败返回 0。
pub fn negotiation_word(
    header_bytes: &[u8; MESSAGE_HEADER_LEN],
    offer_bytes: &[u8; OFFER_LEN],
    server_offer: ProtocolOffer,
) -> u64 {
    if MessageHeader::decode(*header_bytes).is_err() {
        return 0;
    }
    let Ok(offer) = ProtocolOffer::decode(*offer_bytes) else {
        return 0;
    };
    match negotiate(offer, server_offer) {
        Ok(negotiated) => pack_negotiated(negotiated),
        Err(_) => 0,
    }
}

/// SPEC-088：两个负例——`major` 不匹配的协商与魔数错误的解码；命中给显式码，否则给 `0xffff`
/// 哨兵（使「逻辑没跑」不可能伪装成成功）。
pub fn negotiation_negatives() -> u64 {
    let client = ProtocolOffer {
        major: 1,
        minor: 0,
        features: 1,
        required_features: 1,
        max_message_size: 4096,
    };
    let mismatched = ProtocolOffer {
        major: 2,
        ..SERVICE_OFFER_ALPHA
    };
    let major_mismatch = match negotiate(client, mismatched) {
        Err(NegotiationError::MajorMismatch) => 1u64,
        _ => 0xffff,
    };
    let bad_magic = match MessageHeader::decode([0u8; MESSAGE_HEADER_LEN]) {
        Err(CodecError::BadMagic) => 0u64,
        _ => 0xffff,
    };
    (major_mismatch << 32) | bad_magic
}

/// 单条服务消息的 wire header。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    pub major: u16,
    pub minor: u16,
    pub opcode: u16,
    pub flags: u16,
    pub request_id: u32,
    pub payload_len: u32,
}

impl MessageHeader {
    /// 将消息头编码到固定长度字节数组。
    pub const fn encode(self) -> [u8; MESSAGE_HEADER_LEN] {
        let mut bytes = [0u8; MESSAGE_HEADER_LEN];
        bytes[0] = MAGIC[0];
        bytes[1] = MAGIC[1];
        bytes[2] = MAGIC[2];
        bytes[3] = MAGIC[3];
        put_u16(&mut bytes, 4, self.major);
        put_u16(&mut bytes, 6, self.minor);
        put_u16(&mut bytes, 8, self.opcode);
        put_u16(&mut bytes, 10, self.flags);
        put_u32(&mut bytes, 12, self.request_id);
        put_u32(&mut bytes, 16, self.payload_len);
        bytes
    }

    /// 解码消息头，并拒绝错误魔数或零主版本。
    pub const fn decode(bytes: [u8; MESSAGE_HEADER_LEN]) -> Result<Self, CodecError> {
        if bytes[0] != MAGIC[0]
            || bytes[1] != MAGIC[1]
            || bytes[2] != MAGIC[2]
            || bytes[3] != MAGIC[3]
        {
            return Err(CodecError::BadMagic);
        }
        let major = get_u16(&bytes, 4);
        if major == 0 {
            return Err(CodecError::InvalidVersion);
        }
        Ok(Self {
            major,
            minor: get_u16(&bytes, 6),
            opcode: get_u16(&bytes, 8),
            flags: get_u16(&bytes, 10),
            request_id: get_u32(&bytes, 12),
            payload_len: get_u32(&bytes, 16),
        })
    }
}

/// 一端支持的协议主次版本和可选能力。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolOffer {
    /// 不兼容变更时递增；必须非零且双方相同。
    pub major: u16,
    /// 同一主版本内向后兼容的扩展版本。
    pub minor: u16,
    /// 此端可用的 feature 位集。
    pub features: u64,
    /// 此端要求对端也支持的 feature 位集，必须是 `features` 子集。
    pub required_features: u64,
    /// 本端能处理的最大单条消息长度。
    pub max_message_size: u32,
}

impl ProtocolOffer {
    /// 按 wire format 编码为 little-endian 固定长度 payload。
    pub const fn encode(self) -> [u8; OFFER_LEN] {
        let mut bytes = [0u8; OFFER_LEN];
        put_u16(&mut bytes, 0, self.major);
        put_u16(&mut bytes, 2, self.minor);
        put_u64(&mut bytes, 4, self.features);
        put_u64(&mut bytes, 12, self.required_features);
        put_u32(&mut bytes, 20, self.max_message_size);
        bytes
    }

    /// 从固定长度 little-endian payload 解码服务协议 offer。
    pub const fn decode(bytes: [u8; OFFER_LEN]) -> Result<Self, CodecError> {
        let offer = Self {
            major: get_u16(&bytes, 0),
            minor: get_u16(&bytes, 2),
            features: get_u64(&bytes, 4),
            required_features: get_u64(&bytes, 12),
            max_message_size: get_u32(&bytes, 20),
        };
        if offer.major == 0
            || offer.max_message_size == 0
            || offer.required_features & !offer.features != 0
        {
            Err(CodecError::InvalidOffer)
        } else {
            Ok(offer)
        }
    }
}

/// Wire 编码错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    BadMagic,
    InvalidVersion,
    InvalidOffer,
}

const fn put_u16<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u16) {
    let encoded = value.to_le_bytes();
    bytes[offset] = encoded[0];
    bytes[offset + 1] = encoded[1];
}

const fn put_u32<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u32) {
    let encoded = value.to_le_bytes();
    bytes[offset] = encoded[0];
    bytes[offset + 1] = encoded[1];
    bytes[offset + 2] = encoded[2];
    bytes[offset + 3] = encoded[3];
}

const fn put_u64<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u64) {
    let encoded = value.to_le_bytes();
    let mut index = 0;
    while index < encoded.len() {
        bytes[offset + index] = encoded[index];
        index += 1;
    }
}

const fn get_u16<const N: usize>(bytes: &[u8; N], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

const fn get_u32<const N: usize>(bytes: &[u8; N], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn get_u64<const N: usize>(bytes: &[u8; N], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// 协商成功后的共同协议参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NegotiatedProtocol {
    pub major: u16,
    /// 取双方支持次版本的较小值。
    pub minor: u16,
    /// 双方 feature 位集的交集。
    pub features: u64,
    /// 双方消息尺寸上限的较小值。
    pub max_message_size: u32,
}

/// 协商失败的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NegotiationError {
    InvalidOffer,
    MajorMismatch,
    RequiredFeatureUnavailable,
}

/// 按固定规则协商服务协议。
///
/// 两端主版本必须相同且非零；共同次版本取较小值；必需 feature 必须在双方的 feature
/// 交集中；消息尺寸上限取较小值。调用方应在发送任何有副作用的服务请求前完成协商。
pub const fn negotiate(
    client: ProtocolOffer,
    server: ProtocolOffer,
) -> Result<NegotiatedProtocol, NegotiationError> {
    if client.major == 0
        || server.major == 0
        || client.max_message_size == 0
        || server.max_message_size == 0
        || client.required_features & !client.features != 0
        || server.required_features & !server.features != 0
    {
        return Err(NegotiationError::InvalidOffer);
    }
    if client.major != server.major {
        return Err(NegotiationError::MajorMismatch);
    }

    let features = client.features & server.features;
    if (client.required_features | server.required_features) & !features != 0 {
        return Err(NegotiationError::RequiredFeatureUnavailable);
    }

    Ok(NegotiatedProtocol {
        major: client.major,
        minor: if client.minor < server.minor {
            client.minor
        } else {
            server.minor
        },
        features,
        max_message_size: if client.max_message_size < server.max_message_size {
            client.max_message_size
        } else {
            server.max_message_size
        },
    })
}
