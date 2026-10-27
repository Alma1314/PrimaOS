// Auto-extracted from kernel/src/user.rs during EXO-2C init-loader refactor.
// The generator is host-side build-tool code; the kernel must not contain it.

use crate::Manifest;
use crate::syscall::*;
use prima_abi::boot_abi;

pub const USER_BASE_VA: u64 = 0x0000_0A00_0000_0000;

/// 用户代码页（只读、可执行、user）。
pub const USER_CODE_VA: u64 = USER_BASE_VA;
/// SPEC-087：task 1 的第二张代码页（用户代码页 4096→8192 B）。
///
/// `USER_STACK_VA` 紧贴 `USER_CODE_VA`（+0x1000），故第二页不连续，落在空闲槽 +0x5000；
/// 尾部追加的代码经 `jmp rel32` 从 `USER_CODE_VA + 4030` 跳到这里执行。
pub const USER_CODE2_VA: u64 = USER_BASE_VA + 0x9000;
/// 用户栈页（可写、user）。
pub const USER_STACK_VA: u64 = USER_BASE_VA + 0x1000;
/// COW 数据页 X（**只读**、user），写它触发用户态写保护 #PF。
pub const USER_X_VA: u64 = USER_BASE_VA + 0x2000;
/// 回执页 Y（可写、user），用户代码写入以证明 CPL3 执行与重试成功。
pub const USER_Y_VA: u64 = USER_BASE_VA + 0x3000;
/// Stage 4：Rust 用户客户端将 PRIM Hello 放入共享 Segment；init 经 IPC 收到 cap 后映射到此页。
pub const USER_HELLO_VA: u64 = USER_BASE_VA + 0x7000;
/// SPEC-087：第二个（存活的）Rust 客户端递交给 init 的共享 Segment 在 **init 根**内的映射 VA。
///
/// 与 `USER_HELLO_VA`（SPEC-085 首个客户端的段）分开，避免第二次 `memseg_map` 撞 `Overlap`。
pub const USER_HELLO2_VA: u64 = USER_BASE_VA + 0x8000;
/// SPEC-087：第二个客户端的 `recv` 出缓冲（避开 `TASK_RECV_BUF` 的 24 B）。
pub const CLIENT_RECV_BUF: u64 = USER_Y_VA + 0x340;
/// SPEC-087：双向握手各步的 Y 页回执偏移。
pub const CLIENT_RECEIPT_MGR: u64 = 744;
pub const CLIENT_RECEIPT_STACK: u64 = 752;
pub const CLIENT_RECEIPT_RECV: u64 = 760;
pub const CLIENT_RECEIPT_MAP: u64 = 800;
pub const CLIENT_RECEIPT_REPLY: u64 = 808;
/// SPEC-087：客户端把收到的应答 payload 写回共享段内的偏移。
pub const SERVICE_REPLY_OFFSET: u64 = 64;

/// SPEC-088：服务端点自报 id 的消息 tag（高 16 位；低 16 位为 id）。与 `prima-user` 一致。
pub const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// SPEC-088：服务端点完成协商的消息 tag。
pub const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
/// SPEC-088：broker 给服务端点的**无 cap 唤醒** payload（内核拒绝向 `BlockedRecv` 投递带 cap 消息）。
pub const SERVICE_PREPARE_TAG: u64 = 0x5EC3_0000_0000_0000;
/// SPEC-105：向常态阻塞在 `recv` 的服务管理器投递**带 cap** 消息前的**无 cap 唤醒** tag
/// （与 `prima-runtime::protocol::SERVICE_WAKE_TAG` 一致）。
pub const SERVICE_WAKE_TAG: u64 = 0x5ECA_0000_0000_0000;
/// SPEC-105：init 向服务管理器移交**建端点所需 cap 与参数**的消息 tag（与
/// `prima-runtime::protocol::SERVICE_SETUP_TAG` 一致）。payload 布局
/// `tag(48..63) | code_pages(40..47) | name(0..39)`；本 tag 的消息带**一条** cap（父 Namespace，
/// 此时 payload 仅 tag）。
pub const SERVICE_SETUP_TAG: u64 = 0x5ECC_0000_0000_0000;
/// SPEC-108：init 向服务管理器移交**服务代码段**的消息 tag（与
/// `prima-runtime::protocol::SERVICE_SETUP_CODE_TAG` 一致）。payload 布局
/// `tag(48..63) | code_pages(40..47) | name(0..39)`；带**一条** cap（cap0 = 代码段）。
/// 服务**栈段**不再移交——由管理器 `SYS_MEMSEG_CREATE` 自建（栈创建下沉到 ring3）。
pub const SERVICE_SETUP_CODE_TAG: u64 = 0x5ECD_0000_0000_0000;
/// SPEC-088：服务端点把协商结果写回的段内偏移（init 经 `USER_HELLO2_VA` 的同一物理帧读回）。
pub const SERVICE_NEGOTIATED_OFFSET: u64 = 128;
/// SPEC-088：服务端点把两个负例码写回的段内偏移。
pub const SERVICE_NEGATIVES_OFFSET: u64 = 136;
/// SPEC-088：broker 各步的 Y 页回执偏移与 recv 出缓冲。
pub const SVC_RECEIPT_TOKEN: u64 = 864;
pub const SVC_RECEIPT_MGR: u64 = 872;
pub const SVC_RECEIPT_STACK: u64 = 880;
pub const SVC_RECEIPT_ID: u64 = 888;
pub const SVC_RECEIPT_FORWARD: u64 = 896;
pub const SVC_RECEIPT_DONE: u64 = 904;
pub const SVC_RECEIPT_PREPARE: u64 = 920;
pub const SVC_RECV_BUF: u64 = USER_Y_VA + 936;

/// SPEC-090：客户端把**请求服务名**写入共享段的偏移（`init` 经 `USER_HELLO2_VA` 读回）。
pub const HELLO_SERVICE_NAME_OFFSET: u64 = 48;
/// SPEC-101：端点自报 payload 布局 `ID_TAG(48..63) | name(8..47) | id(0..7)`；id 为自身 task id
/// （≤ 255，取低 8 位）。
pub const SERVICE_ID_MASK: u64 = 0xff;
/// SPEC-090：`BETA` 行为的协商结果写在 `128 + (behavior-1)*16` = 144；判决性在 `max=1024`
/// （只可能来自被按名选中的 `BETA` offer，`ALPHA` 是 2048）。
pub const SERVICE_NEGOTIATED_B_OFFSET: u64 = 144;
/// SPEC-090：解析与二次路由的 Y 页回执（紧接 SPEC-088 的 `936..959`，`< NS_PATH_DATA_OFFSET`）。
pub const SVC_RECEIPT_NAME: u64 = 960;
pub const SVC_RECEIPT_RESOLVED: u64 = 968;
pub const SVC_RECEIPT_UNKNOWN: u64 = 976;
pub const SVC_RECEIPT_DONE_B: u64 = 984;

/// SPEC-092：第二个独立服务端点的 Y 页回执（紧接 SPEC-090 的 `984..991`，`< NS_PATH_DATA_OFFSET=1024`）。
pub const SVC_RECEIPT_MGR2: u64 = 992;
pub const SVC_RECEIPT_ID2: u64 = 1000;
pub const SVC_RECEIPT_STACK2: u64 = 1008;
pub const SVC_RECEIPT_SEG_B: u64 = 1016;
/// SPEC-095/101：运行期服务注册表的 Y 页槽——两个端点回报的注册名（取自自报消息的 8..47 位，
/// 即端点从创建指派处 `rcx` 读得的 ASCII 令牌）。
///
/// init 在收集端点自报时记下 `(name, id)`，解析请求名时**按名扫描**这两条（替代内联常量
/// `cmp`）。槽位取 Y 页空隙 `768/776`（`CLIENT_RECEIPT_RECV=760` 与 `CLIENT_RECEIPT_MAP=800`
/// 之间，无任何见证读取），避开 `runtime_hello` 的 `map_result`（`Y+736`）等既有回执。
pub const SVC_RECEIPT_REG_A_NAME: u64 = 768;
pub const SVC_RECEIPT_REG_B_NAME: u64 = 776;
/// SPEC-103：ring3 服务管理器向 init 自报存活的 tag（高 16 位；与 `prima-runtime::protocol` 一致）。
pub const SERVICEMGR_HELLO_TAG: u64 = 0x5E56_0000_0000_0000;
/// SPEC-103：init 收到的服务管理器回执在 Y 页的偏移——`784`＝创建返回的 task id、`792`＝自报
/// payload（`TAG ‖ id 低 8 位`）。取 Y 页空隙 `784/792`（`SVC_RECEIPT_REG_B_NAME=776` 与
/// `CLIENT_RECEIPT_MAP=800` 之间，无任何既有见证读取）。
pub const SERVICEMGR_RECEIPT_MGR: u64 = 784;
pub const SERVICEMGR_RECEIPT_HELLO: u64 = 792;

/// SPEC-104：管理器 → init 的三段控制消息 tag（唤醒 / 交回共享段只读副本 / finish）。
///
/// 与 `prima-user/src/servicemgr.rs` 一致。三段**连发**（中间不让出）确保唤醒先于带 cap 消息。
pub const SERVICEMGR_WAKE_TAG: u64 = 0x5E57_0000_0000_0000;
pub const SERVICEMGR_SEG_TAG: u64 = 0x5E58_0000_0000_0000;
pub const SERVICEMGR_FINISH_TAG: u64 = 0x5E59_0000_0000_0000;
/// SPEC-104：管理器写回客户端共享段的固定偏移（与 `prima-user/src/servicemgr.rs` 一致）。
///
/// 内核见证改从这些偏移读回「由管理器产出」的注册表/解析结果；值本身沿用既有语义，
/// 故事件除**产出方与 task id** 外逐字段不变。
pub const SERVICEMGR_SELF_ID_OFFSET: u64 = 192;
pub const SERVICEMGR_MARKER_OFFSET: u64 = 200;
/// SPEC-105：管理器写回共享段的**建端点证据**——已创建端点数（应 2）、端点令牌 cap、
/// 管理 cap，以及两个端点**由管理器自解 ELF 头**（偏移 24..32）得到的入口偏移。
pub const SERVICEMGR_CREATED_OFFSET: u64 = 272;
pub const SERVICEMGR_TOKEN_OFFSET: u64 = 280;
pub const SERVICEMGR_MANAGER_OFFSET: u64 = 288;
pub const SERVICEMGR_ENTRY_A_OFFSET: u64 = 304;
pub const SERVICEMGR_ENTRY_B_OFFSET: u64 = 312;
/// SPEC-106（S4-D3）：三个基础服务（日志/Namespace/存储）写回共享段的 (id, 存在标记, 探针应答)
/// 及其计数——见证据此证明三服务由 ring3 管理器按表建立、且**请求路由到了正确的服务二进制**。
pub const SERVICEMGR_LOG_ID_OFFSET: u64 = 320;
pub const SERVICEMGR_LOG_MARK_OFFSET: u64 = 328;
pub const SERVICEMGR_NS_ID_OFFSET: u64 = 336;
pub const SERVICEMGR_NS_MARK_OFFSET: u64 = 344;
pub const SERVICEMGR_STORE_ID_OFFSET: u64 = 352;
pub const SERVICEMGR_STORE_MARK_OFFSET: u64 = 360;
pub const SERVICEMGR_SERVICES_OFFSET: u64 = 368;
/// SPEC-110：每个基础服务的**定序状态见证起始**偏移（占 3 槽：推进 A / 推进 B / 查询）。
pub const SERVICEMGR_LOG_REPLY_OFFSET: u64 = 376;
pub const SERVICEMGR_NS_REPLY_OFFSET: u64 = 400;
pub const SERVICEMGR_STORE_REPLY_OFFSET: u64 = 424;
/// SPEC-111：Namespace 服务**代理解析**返回的内核句柄（内核命名空间树的 `SYS_NS_RESOLVE` 结果）。
pub const SERVICEMGR_NS_RESOLVED_OFFSET: u64 = 448;
/// SPEC-112（S4-D3 收口）：日志/存储服务**持久化段**的内核句柄（管理器 `SYS_MEMSEG_CREATE` 铸出、
/// 运行期移交服务）。内核据此证明两服务各持有**真实的内核 Segment 对象**，而非仅内部状态。
pub const SERVICEMGR_LOG_SEG_OFFSET: u64 = 456;
pub const SERVICEMGR_STORE_SEG_OFFSET: u64 = 464;
/// SPEC-114（S4-D4）：用户态驱动的 (id, 存在标记, 内核设备句柄)；受控端口的两条应答各占 8 B。
pub const SERVICEMGR_DRIVER_ID_OFFSET: u64 = 472;
pub const SERVICEMGR_DRIVER_MARK_OFFSET: u64 = 480;
pub const SERVICEMGR_DRIVER_DEV_OFFSET: u64 = 488;
pub const SERVICEMGR_DRIVER_REPLY_OFFSET: u64 = 496;
/// SPEC-114：受控端口往返的期望读回值（写入 Scratch 寄存器再读回的魔数）。
pub const IOPORT_MAGIC_READBACK: u64 = 0x5A;
pub const SERVICE_RESOLVED_OFFSET: u64 = 208;
pub const SERVICE_UNKNOWN_OFFSET: u64 = 216;
pub const SERVICE_DONE_BETA_OFFSET: u64 = 224;
pub const SERVICE_REG_A_NAME_OFFSET: u64 = 232;
pub const SERVICE_REG_B_NAME_OFFSET: u64 = 240;
pub const SERVICE_BETA_SELF_OFFSET: u64 = 248;
pub const SERVICE_DONE_ALPHA_OFFSET: u64 = 256;
pub const SERVICE_CLIENT_ID_OFFSET: u64 = 264;
/// SPEC-104：`prima-servicemgr` 映像写回共享段的**二进制标记**（与 `prima-user/src/servicemgr.rs`
/// 一致）——见证据此证明上述结果来自**独立 ring3 映像**，而非 init 的手写块。
pub const SERVICEMGR_BINARY_MARKER: u64 = 0x5345_5256_4D47_5203;
/// SPEC-104：`TaskSend` 权限位（`Right::TaskSend` 为枚举第 5 项）——init 从管理器的管理 cap
/// 派生出指向管理器的发送凭据（移交端点/客户端作 `initial_cap0`）。
const ACCESS_TASKSEND_BITS: u64 = 1 << 5;
/// SPEC-092：服务端点把自己的 task id 写入共享段的偏移（`160 + (behavior-1)*8`；与 `prima-user` 一致）。
pub const SERVICE_ENDPOINT_OFFSET: u64 = 160;
/// SPEC-092：`ALPHA` 端点（第一个服务任务）自报 id 的共享段偏移。
pub const SERVICE_ENDPOINT_ALPHA_OFFSET: u64 = SERVICE_ENDPOINT_OFFSET;
/// SPEC-092：`BETA` 端点（第二个服务任务）自报 id 的共享段偏移。
pub const SERVICE_ENDPOINT_BETA_OFFSET: u64 = SERVICE_ENDPOINT_OFFSET + 8;
/// SPEC-093：服务端点写入**自身二进制标记**的共享段偏移（`176 + (behavior-1)*8`；与 `prima-user` 一致）。
pub const SERVICE_BINARY_MARKER_OFFSET: u64 = 176;
/// SPEC-093：`ALPHA` 端点标记的共享段偏移。
pub const SERVICE_ALPHA_MARKER_OFFSET: u64 = SERVICE_BINARY_MARKER_OFFSET;
/// SPEC-093：`BETA` 端点标记的共享段偏移。
pub const SERVICE_BETA_MARKER_OFFSET: u64 = SERVICE_BINARY_MARKER_OFFSET + 8;

/// 切片 M：第二个 ring3 任务的区域基址（独立 PML4 槽 21，避免与 task 1 的槽 20
/// 共享 `slot_free` 只按 PML4 present 判定的粒度）。
pub const USER2_BASE_VA: u64 = 0x0000_0A80_0000_0000;
/// 第二任务代码页（只读、可执行、user）。
pub const USER2_CODE_VA: u64 = USER2_BASE_VA;
/// 第二任务栈页（可写、user）。
pub const USER2_STACK_VA: u64 = USER2_BASE_VA + 0x1000;
/// 第二任务回执页（可写、user）。
pub const USER2_Y_VA: u64 = USER2_BASE_VA + 0x2000;

/// 任务 1 用户区所在的 PML4 槽。
pub const USER_SLOT: usize = 20;
/// 任务 2 用户区所在的 PML4 槽。
pub const USER2_SLOT: usize = 21;

/// S2-D4：`task_create` 创建的任务所用的区域基址（独立 PML4 槽 22）。
///
/// 与 task 1/2 一样独占一个 PML4 槽：新根由 `clone_root` 克隆 bootloader 根后清空槽
/// 20/21/22，故新 Task 的代码/栈不会与既有任务的用户区相互可见。
pub const USER3_BASE_VA: u64 = 0x0000_0B00_0000_0000;
/// `task_create` 创建的任务的代码页 VA（`load_base`/`entry`）。
pub const USER3_CODE_VA: u64 = USER3_BASE_VA;
/// `task_create` 创建的任务的栈页 VA（调用者经 manager cap 映射到此）。
// Rust ELF 占用 USER3_BASE_VA 起始 4 页；Task 3/4 共用的可写栈放到独立 2 MiB 区域，
// 防止入口代码 Segment 与栈映射互相覆盖。
pub const USER3_STACK_VA: u64 = USER3_BASE_VA + 0x10_0000;
/// 新任务用户区所在的 PML4 槽。
pub const USER3_SLOT: usize = 22;

/// 切片 O：抢占空转代码/数据页（两任务**同 VA**，各自根下映射到不同帧，用于实证隔离）。
///
/// 取槽 20 内更高的 2 MiB 区（i2=2/3），避开 [`crate::syscall::BAD_PTR`]（`+0x20_0000`，
/// i2=1）——后者必须始终未映射，否则 COPY_IN 的 `EFAULT` 证据会失效。
pub const SPIN_CODE_VA: u64 = USER_BASE_VA + 0x40_0000;
/// 抢占空转写标记的数据页 VA。
pub const SPIN_DATA_VA: u64 = USER_BASE_VA + 0x41_0000;

/// 抢占空转代码长度（`mov rax, imm64` + `mov [rax], imm32` + `jmp $`）。
pub const SPIN_CODE_LEN: usize = 19;

/// 每任务内核栈大小（叶子单元页，直接映射在各自 PML4 克隆出的内核高半区）。
/// SPEC-106：每任务内核栈大小。从 16 KiB 提到 **32 KiB**——S4-D3 让 `SYS_TASK_DESTROY` 的栈帧
/// 增大（其 `[0u64; MAX_VSPACE_ENTRIES]` 快照数组随 `MAX_VSPACE_ENTRIES` 扩容），16 KiB 栈**向下
/// 溢出**并覆写了相邻 BSS（实测把 `KERNEL_ROOT`/`KERNEL_IMAGE` 清零，使 `self_root_ready` 报
/// `None` → `panic` 事件）。与 `config.rs` 里启动栈 80→256 KiB 的既有取向一致。
pub const USER_STORE_VALUE: u64 = 0x5A5A_1234_DEAD_BEEF;

/// 用户代码字节长度。
/// S2-D4：`task_create` 创建的任务的代码长度（`task3_code()`）。
pub const TASK3_CODE_LEN: usize = 134;
/// 切片 V：`task4_code()` 在**同一代码页内**的偏移。
///
/// 复用切片 T 的代码段（1 页 4 KiB，`task3_code()` 仅占 134 字节），故不新增代码段帧——
/// `frames_ready.segment` 不变。第二次 `SYS_TASK_CREATE` 的 `entry` 指向
/// `USER3_CODE_VA + TASK4_CODE_OFFSET`（§8.1 只要求 `entry ∈ [load_base, load_base + pages*4KiB)`）。
/// 页内 0x800：避开 ELF header/program header（首个服务页的 0..0x200 必须保持完整，
/// 供 ring3 解析可写 `PT_LOAD`）；该处原属 `.rela.dyn`，重定位已在装载期消费。
pub const TASK4_CODE_OFFSET: u64 = 0x800;
/// 切片 V：task 4 的代码长度（自报 id → 让出 → 自毁 → 不该到达）。
///
/// **必须先让出再自毁**：§8.2 的 `task_destroy` 会丢弃 sender 为该 Task 的消息，若在同一
/// 时间片内发送后立即自毁，刚排队的 id 会被自己的销毁清理掉（实测确认），父任务将取不到。
pub const TASK4_CODE_LEN: usize = 109;
/// S2-D4：子任务若被再次调度时写入的哨兵（正常路径下销毁后不再被调度，故该值不应出现）。
pub const TASK3_RESCHEDULED_MAGIC: u64 = 0xDEAD_0000_0000_0003;

pub const SERVICE_IMAGE_MAGIC: u64 = 0x5345_5256_494D_4701;

/// 用户代码占据一个 4 KiB 页；路径文本放在独立的 Y 页。
pub const USER_CODE_LEN: usize = 8192;
/// SPEC-087：task 1 代码占用的页数（页 0 在 `USER_CODE_VA`、页 1 在 `USER_CODE2_VA`）。
pub const USER_CODE_PAGES: usize = USER_CODE_LEN / FRAME_SIZE as usize;

pub const NS_PATH_BYTES: &[u8] = b"lib\0sub\0mnt\0mnt/sub\0/lib\0x\0y\0";

/// 第二任务代码字节长度。
pub const TASK2_CODE_LEN: usize = 496;

/// 把 `bytes` 写入 `code[at..]`（const 友好，避免 const 上下文里的迭代器）。
const fn put<const N: usize>(code: &mut [u8; N], at: usize, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        code[at + i] = bytes[i];
        i += 1;
    }
}

/// 写入 `opcode0 opcode1 <imm64>` 形态的指令（`mov r64, imm64`）。
const fn put_imm64<const N: usize>(
    code: &mut [u8; N],
    at: usize,
    opcode0: u8,
    opcode1: u8,
    imm: u64,
) {
    code[at] = opcode0;
    code[at + 1] = opcode1;
    put(code, at + 2, &imm.to_le_bytes());
}

/// `mov eax, imm32`（5 字节）——写入 syscall 调用号。
///
/// 32 位写入会清零 `rax` 高 32 位；调用号都 ≤ `u32::MAX`，故语义不变，但比
/// `mov rax, imm64` 省 5 字节（代码页已接近单页上限，SPEC-083 起采用）。
const fn put_syscall<const N: usize>(code: &mut [u8; N], at: usize, number: u64) {
    assert!(number <= u32::MAX as u64);
    code[at] = 0xB8;
    put(code, at + 1, &(number as u32).to_le_bytes());
}

/// `mov [rcx+disp32], rax`（7 字节）——把 syscall 返回值写入 Y 回执页。
const fn put_store_rax<const N: usize>(code: &mut [u8; N], at: usize, disp: u32) {
    put(code, at, &[0x48, 0x89, 0x81]);
    put(code, at + 3, &disp.to_le_bytes());
}

/// `mov <reg>, [rcx+disp32]`（6 字节）——从 Y 回执页读回句柄。
///
/// `rex`/`modrm` 由调用点给出：`rdi`=`(0x48, 0xB9)`、`rsi`=`(0x48, 0xB1)`、
/// `rdx`=`(0x48, 0x91)`、`r10`=`(0x4C, 0x91)`。
const fn put_load_reg<const N: usize>(
    code: &mut [u8; N],
    at: usize,
    rex: u8,
    modrm: u8,
    disp: u32,
) {
    put(code, at, &[rex, 0x8B, modrm]);
    put(code, at + 3, &disp.to_le_bytes());
}

/// 从 Y 回执页加载实际返回的句柄；补齐原 `mov rdi, imm64` 的 10 字节槽位。
const fn put_receipt_rdi<const N: usize>(code: &mut [u8; N], at: usize, disp: u32) {
    put_load_reg(code, at, 0x48, 0xB9, disp);
    put(code, at + 7, &[0x90; 3]);
}

/// `mov rdi, [rcx+disp32]` 的 `modrm`。
const NS_MODRM_RDI: u8 = 0xB9;
/// `mov rsi, [rcx+disp32]`（mod=10, reg=rsi, rm=rcx）。
const NS_MODRM_RSI: u8 = 0xB1;

/// `add rdi, imm8`（4 字节）——SPEC-082 用来把 `time_now` 的结果加上相对偏移。
const fn put_add_rdi<const N: usize>(code: &mut [u8; N], at: usize, imm: u8) {
    put(code, at, &[0x48, 0x83, 0xC7, imm]);
}
/// `mov r10, [rcx+disp32]` 的 `modrm`（配合 REX.WR = 0x4C）。
const NS_MODRM_R10: u8 = 0x91;

/// SPEC-105：发一条 `SYS_SEND`——`rdi`（接收者 id）与 `rsi`（发送凭据）须已由调用点置好，
/// 本函数只写 `rdx`(payload)/`r10`(cap_count)/`r8`(cap0)/`r9`(cap1)。返回新 `at`。
const fn emit_send<const N: usize>(
    code: &mut [u8; N],
    at: usize,
    payload: u64,
    cap_count: u64,
    cap0: u64,
    cap1: u64,
) -> usize {
    put_imm64(code, at, 0x48, 0xBA, payload); // mov rdx, imm64
    put_imm64(code, at + 10, 0x49, 0xBA, cap_count); // mov r10, imm64
    put_imm64(code, at + 20, 0x49, 0xB8, cap0); // mov r8, imm64
    put_imm64(code, at + 30, 0x49, 0xB9, cap1); // mov r9, imm64
    put_syscall(code, at + 40, SYS_SEND);
    put(code, at + 45, &[0xCD, 0x80]);
    at + 47
}
/// SPEC-088：`mov rdx, [rcx+disp32]` 的 `modrm`（配合 REX.W = 0x48）。
const NS_MODRM_RDX: u8 = 0x91;

/// 组装用户代码机器码：
/// `mov rdx, X; mov rax, VAL; mov [rdx], rax;`（COW 故障点）
/// `mov rcx, Y; mov [rcx], rax;`（回执 0）
/// `mov rdi, ARG; mov rax, SYS_ECHO; int 0x80; mov [rcx+8], rax;`（回执 1）
/// `mov rdi/rsi/rdx/r10, SUM_ARGS; mov rax, SYS_SUM4; int 0x80; mov [rcx+16], rax;`（回执 2）
/// `mov rdi, X; mov rsi, 8; mov rax, SYS_COPY_IN; int 0x80; mov [rcx+24], rax;`（回执 3）
/// `mov rdi, BAD_PTR; mov rsi, 8; mov rax, SYS_COPY_IN; int 0x80; mov [rcx+32], rax;`（回执 4）
/// `mov rax, 0; int 0x80; mov [rcx+40], rax;`（回执 5）
/// `mov rax, SYS_BAD; int 0x80; mov [rcx+48], rax;`（回执 6）
/// 6 次异常订阅（SPEC-050）：订阅 page_fault 成功、重复（`EACCES`）、kind 不符（`EINVAL`）、
/// 订阅 syscall_trap 成功、取消成功、再次取消（`EACCES`），写 `Y+88..Y+128`；
/// `mov rdi, FAST_ARG; mov rax, SYS_ECHO; syscall; mov rcx, Y; mov [rcx+56], rax`；
/// 切片 S：建共享 Segment → 映射进自己的根 → 写魔数 → 经**排队路径**把段 cap 送给 task 2 →
/// 让出 → 再经**直投路径**送一条无 cap 消息唤醒阻塞的 task 2 → 让出；
/// S2-D4：建栈段 → `SYS_TASK_CREATE`（完整 §8.1）→ 两个创建负例（入口越界 / 缺 `SegMapX`）→
/// 把栈映射进**子任务**根 → 让出 → `SYS_RECV` 取子任务回发的 id → 缺 `TaskDestroy` 的销毁负例 →
/// 销毁成功 → 重复销毁与向已销毁任务映射两个负例 → `int3`。
/// Generated code reserves `r15` for the boot-info page pointer set by the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BootReg {
    Rax,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
}

/// Emit a 10-byte slot equivalent to `mov reg, [r15 + disp32]` followed by a
/// 2-byte NOP. Keeping the original `mov reg, imm64` length preserves all
/// hand-maintained code offsets and the existing byte-level regression tests.
fn put_boot_load<const N: usize>(code: &mut [u8; N], at: usize, reg: BootReg, disp: u32) {
    let (reg_bits, rex_r) = match reg {
        BootReg::Rax => (0u8, false),
        BootReg::Rcx => (1, false),
        BootReg::Rdx => (2, false),
        BootReg::Rsi => (6, false),
        BootReg::Rdi => (7, false),
        BootReg::R8 => (0, true),
        BootReg::R9 => (1, true),
        BootReg::R10 => (2, true),
    };
    // REX.W + REX.B (r15 base) (+ optional REX.R); SIB index=100 (none), base=111 (r15).
    code[at] = if rex_r { 0x4D } else { 0x49 };
    code[at + 1] = 0x8B;
    code[at + 2] = 0x80 | (reg_bits << 3) | 0x04;
    code[at + 3] = 0x27;
    code[at + 4..at + 8].copy_from_slice(&disp.to_le_bytes());
    code[at + 8..at + 10].copy_from_slice(&[0x66, 0x90]);
}

pub fn generate(manifest: &Manifest) -> [u8; USER_CODE_LEN] {
    let mut code = [0u8; USER_CODE_LEN];
    // 0: mov rdx, X
    put_imm64(&mut code, 0, 0x48, 0xBA, USER_X_VA);
    // 10: mov rax, VAL
    put_imm64(&mut code, 10, 0x48, 0xB8, USER_STORE_VALUE);
    // 20: mov [rdx], rax  （3 字节，与 FAULT_SKIP_LEN 成对）
    put(&mut code, 20, &[0x48, 0x89, 0x02]);
    // 23: mov rcx, Y
    put_imm64(&mut code, 23, 0x48, 0xB9, USER_Y_VA);
    // 33: mov [rcx], rax
    put(&mut code, 33, &[0x48, 0x89, 0x01]);
    // 36: mov rdi, SYS_ECHO_ARG
    put_imm64(&mut code, 36, 0x48, 0xBF, SYS_ECHO_ARG);
    // 46: mov rax, SYS_ECHO
    put_imm64(&mut code, 46, 0x48, 0xB8, SYS_ECHO);
    // 56: int 0x80
    put(&mut code, 56, &[0xCD, 0x80]);
    // 58: mov [rcx+8], rax
    put(&mut code, 58, &[0x48, 0x89, 0x41, 0x08]);
    // 62: mov rdi, SUM_ARGS[0]
    put_imm64(&mut code, 62, 0x48, 0xBF, SUM_ARGS[0]);
    // 72: mov rsi, SUM_ARGS[1]
    put_imm64(&mut code, 72, 0x48, 0xBE, SUM_ARGS[1]);
    // 82: mov rdx, SUM_ARGS[2]
    put_imm64(&mut code, 82, 0x48, 0xBA, SUM_ARGS[2]);
    // 92: mov r10, SUM_ARGS[3]
    put_imm64(&mut code, 92, 0x49, 0xBA, SUM_ARGS[3]);
    // 102: mov rax, SYS_SUM4
    put_imm64(&mut code, 102, 0x48, 0xB8, SYS_SUM4);
    // 112: int 0x80
    put(&mut code, 112, &[0xCD, 0x80]);
    // 114: mov [rcx+16], rax
    put(&mut code, 114, &[0x48, 0x89, 0x41, 0x10]);
    // 118: mov rdi, COPY_PTR
    put_imm64(&mut code, 118, 0x48, 0xBF, COPY_PTR);
    // 128: mov rsi, COPY_LEN
    put_imm64(&mut code, 128, 0x48, 0xBE, COPY_LEN);
    // 138: mov rax, SYS_COPY_IN
    put_imm64(&mut code, 138, 0x48, 0xB8, SYS_COPY_IN);
    // 148: int 0x80
    put(&mut code, 148, &[0xCD, 0x80]);
    // 150: mov [rcx+24], rax
    put(&mut code, 150, &[0x48, 0x89, 0x41, 0x18]);
    // 154: mov rdi, BAD_PTR
    put_imm64(&mut code, 154, 0x48, 0xBF, BAD_PTR);
    // 164: mov rsi, COPY_LEN
    put_imm64(&mut code, 164, 0x48, 0xBE, COPY_LEN);
    // 174: mov rax, SYS_COPY_IN
    put_imm64(&mut code, 174, 0x48, 0xB8, SYS_COPY_IN);
    // 184: int 0x80
    put(&mut code, 184, &[0xCD, 0x80]);
    // 186: mov [rcx+32], rax
    put(&mut code, 186, &[0x48, 0x89, 0x41, 0x20]);
    // 190: mov rax, 0（保留号）
    put_imm64(&mut code, 190, 0x48, 0xB8, 0);
    // 200: int 0x80
    put(&mut code, 200, &[0xCD, 0x80]);
    // 202: mov [rcx+40], rax
    put(&mut code, 202, &[0x48, 0x89, 0x41, 0x28]);
    // 206: mov rax, SYS_BAD
    put_imm64(&mut code, 206, 0x48, 0xB8, SYS_BAD);
    // 216: int 0x80
    put(&mut code, 216, &[0xCD, 0x80]);
    // 218: mov [rcx+48], rax
    put(&mut code, 218, &[0x48, 0x89, 0x41, 0x30]);
    // 222: mov rdi, AUTHORIZE_CAP
    put_boot_load(&mut code, 222, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    // 232: mov rsi, RIGHT_SEGMAPR
    put_imm64(&mut code, 232, 0x48, 0xBE, RIGHT_SEGMAPR);
    // 242: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 242, 0x48, 0xB8, SYS_AUTHORIZE);
    // 252: int 0x80 → 授予
    put(&mut code, 252, &[0xCD, 0x80]);
    // 254: mov [rcx+64], rax
    put(&mut code, 254, &[0x48, 0x89, 0x41, 0x40]);
    // 258: mov rsi, RIGHT_SEGMAPW（rdi 仍是 cap）
    put_imm64(&mut code, 258, 0x48, 0xBE, RIGHT_SEGMAPW);
    // 268: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 268, 0x48, 0xB8, SYS_AUTHORIZE);
    // 278: int 0x80 → 拒绝
    put(&mut code, 278, &[0xCD, 0x80]);
    // 280: mov [rcx+72], rax
    put(&mut code, 280, &[0x48, 0x89, 0x41, 0x48]);
    // 284: mov rdi, UNSET_CAP
    put_imm64(&mut code, 284, 0x48, 0xBF, UNSET_CAP);
    // 294: mov rsi, RIGHT_SEGMAPR
    put_imm64(&mut code, 294, 0x48, 0xBE, RIGHT_SEGMAPR);
    // 304: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 304, 0x48, 0xB8, SYS_AUTHORIZE);
    // 314: int 0x80 → ENOENT
    put(&mut code, 314, &[0xCD, 0x80]);
    // 316: mov [rcx+80], rax
    put(&mut code, 316, &[0x48, 0x89, 0x41, 0x50]);
    // 320: mov rdi, EXC_PAGE_FAULT_CAP
    put_boot_load(
        &mut code,
        320,
        BootReg::Rdi,
        boot_abi::OFF_EXC_PAGE_FAULT_CAP,
    );
    // 330: mov rsi, EXC_KIND_PAGEFAULT
    put_imm64(&mut code, 330, 0x48, 0xBE, EXC_KIND_PAGEFAULT);
    // 340: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 340, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 350: int 0x80 → 订阅成功（返回 source_id 1）
    put(&mut code, 350, &[0xCD, 0x80]);
    // 352: mov [rcx+88], rax
    put(&mut code, 352, &[0x48, 0x89, 0x41, 0x58]);
    // 356: mov rsi, EXC_KIND_PAGEFAULT（rdi 仍是 cap）
    put_imm64(&mut code, 356, 0x48, 0xBE, EXC_KIND_PAGEFAULT);
    // 366: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 366, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 376: int 0x80 → 重复订阅（EACCES）
    put(&mut code, 376, &[0xCD, 0x80]);
    // 378: mov [rcx+96], rax
    put(&mut code, 378, &[0x48, 0x89, 0x41, 0x60]);
    // 382: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 382, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 392: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 392, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 402: int 0x80 → kind 不符（EINVAL）
    put(&mut code, 402, &[0xCD, 0x80]);
    // 404: mov [rcx+104], rax
    put(&mut code, 404, &[0x48, 0x89, 0x41, 0x68]);
    // 408: mov rdi, EXC_SYSCALL_TRAP_CAP
    put_boot_load(
        &mut code,
        408,
        BootReg::Rdi,
        boot_abi::OFF_EXC_SYSCALL_TRAP_CAP,
    );
    // 418: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 418, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 428: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 428, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 438: int 0x80 → 订阅成功（返回 source_id 2）
    put(&mut code, 438, &[0xCD, 0x80]);
    // 440: mov [rcx+112], rax
    put(&mut code, 440, &[0x48, 0x89, 0x41, 0x70]);
    // 444: mov rdi, EXC_PAGE_FAULT_CAP
    put_boot_load(
        &mut code,
        444,
        BootReg::Rdi,
        boot_abi::OFF_EXC_PAGE_FAULT_CAP,
    );
    // 454: mov rax, SYS_EXC_UNSUBSCRIBE
    put_imm64(&mut code, 454, 0x48, 0xB8, SYS_EXC_UNSUBSCRIBE);
    // 464: int 0x80 → 取消成功（返回 source_id 1）
    put(&mut code, 464, &[0xCD, 0x80]);
    // 466: mov [rcx+120], rax
    put(&mut code, 466, &[0x48, 0x89, 0x41, 0x78]);
    // 470: mov rax, SYS_EXC_UNSUBSCRIBE（rdi 仍是 page_fault cap）
    put_imm64(&mut code, 470, 0x48, 0xB8, SYS_EXC_UNSUBSCRIBE);
    // 480: int 0x80 → 未订阅（EACCES）
    put(&mut code, 480, &[0xCD, 0x80]);
    // 482: mov [rcx+128], rax（disp32：0x80 作 disp8 会被符号扩展成 -128）
    put(&mut code, 482, &[0x48, 0x89, 0x81, 0x80, 0x00, 0x00, 0x00]);
    // 489: mov rdi, FAST_ARG
    put_imm64(&mut code, 489, 0x48, 0xBF, FAST_ARG);
    // 499: mov rax, SYS_ECHO
    put_imm64(&mut code, 499, 0x48, 0xB8, SYS_ECHO);
    // 509: 0F 05  syscall（快路径；破坏 rcx/r11）
    put(&mut code, 509, &[0x0F, 0x05]);
    // 511: mov rcx, Y（syscall 破坏了 rcx，需重载回执基址）
    put_imm64(&mut code, 511, 0x48, 0xB9, USER_Y_VA);
    // 521: mov [rcx+56], rax
    put(&mut code, 521, &[0x48, 0x89, 0x41, 0x38]);
    // 525: mov rax, SYS_TASK_ID
    put_imm64(&mut code, 525, 0x48, 0xB8, SYS_TASK_ID);
    // 535: int 0x80 → 返回 1
    put(&mut code, 535, &[0xCD, 0x80]);
    // 537: mov [rcx+136], rax（disp32：0x88 作 disp8 会被符号扩展）
    put(&mut code, 537, &[0x48, 0x89, 0x81, 0x88, 0, 0, 0]);
    // 544: mov rax, SYS_YIELD
    put_imm64(&mut code, 544, 0x48, 0xB8, SYS_YIELD);
    // 554: int 0x80（切走；恢复时从 556 继续，rax=0）
    put(&mut code, 554, &[0xCD, 0x80]);
    // 556: mov [rcx+144], rax
    put(&mut code, 556, &[0x48, 0x89, 0x81, 0x90, 0, 0, 0]);
    // 563: mov rax, SYS_TASK_ID（恢复后仍应是 task 1）
    put_imm64(&mut code, 563, 0x48, 0xB8, SYS_TASK_ID);
    // 573: int 0x80 → 返回 1
    put(&mut code, 573, &[0xCD, 0x80]);
    // 575: mov [rcx+152], rax
    put(&mut code, 575, &[0x48, 0x89, 0x81, 0x98, 0, 0, 0]);
    // 582: mov rdi, EXC_SYSCALL_TRAP_CAP
    put_boot_load(
        &mut code,
        582,
        BootReg::Rdi,
        boot_abi::OFF_EXC_SYSCALL_TRAP_CAP,
    );
    // 592: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 592, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 602: mov rax, SYS_EXC_WAIT
    put_imm64(&mut code, 602, 0x48, 0xB8, SYS_EXC_WAIT);
    // 612: int 0x80（无通知 → 阻塞切走；被 raise 唤醒后返回 payload）
    put(&mut code, 612, &[0xCD, 0x80]);
    // 614: mov [rcx+160], rax（disp32）
    put(&mut code, 614, &[0x48, 0x89, 0x81, 0xa0, 0, 0, 0]);
    // 621: S2-D2 能力生命周期：派生 → 越权拒绝 → drop → 重复 drop → 槽位复用 →
    //      stale 拒绝 → 授权 → 结构性根保护。回执写 Y+168..Y+232（disp32，≥128 不可用 disp8）。
    put_boot_load(&mut code, 621, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    put_imm64(&mut code, 631, 0x48, 0xBE, RIGHTS_DERIVE_BITS);
    put_imm64(&mut code, 641, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 651, &[0xCD, 0x80]);
    put(&mut code, 653, &[0x48, 0x89, 0x81, 168, 0, 0, 0]);
    // 越权：请求 image cap 不拥有的 SegMapW。
    put_boot_load(&mut code, 660, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    put_imm64(&mut code, 670, 0x48, 0xBE, RIGHTS_SEGMAPW_BITS);
    put_imm64(&mut code, 680, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 690, &[0xCD, 0x80]);
    put(&mut code, 692, &[0x48, 0x89, 0x81, 176, 0, 0, 0]);
    // drop 子句柄（成功）→ 重复 drop（ENOENT）。
    put_receipt_rdi(&mut code, 699, 168);
    put_imm64(&mut code, 709, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 719, &[0xCD, 0x80]);
    put(&mut code, 721, &[0x48, 0x89, 0x81, 184, 0, 0, 0]);
    put_receipt_rdi(&mut code, 728, 168);
    put_imm64(&mut code, 738, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 748, &[0xCD, 0x80]);
    put(&mut code, 750, &[0x48, 0x89, 0x81, 192, 0, 0, 0]);
    // 槽位复用：再派生应拿到同槽更高 generation 的新句柄。
    put_boot_load(&mut code, 757, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    put_imm64(&mut code, 767, 0x48, 0xBE, RIGHTS_DERIVE_BITS);
    put_imm64(&mut code, 777, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 787, &[0xCD, 0x80]);
    put(&mut code, 789, &[0x48, 0x89, 0x81, 200, 0, 0, 0]);
    // 旧句柄授权应被拒绝（stale），新句柄应被授予。
    put_receipt_rdi(&mut code, 796, 168);
    put_imm64(&mut code, 806, 0x48, 0xBE, RIGHT_DERIVE);
    put_imm64(&mut code, 816, 0x48, 0xB8, SYS_AUTHORIZE);
    put(&mut code, 826, &[0xCD, 0x80]);
    put(&mut code, 828, &[0x48, 0x89, 0x81, 208, 0, 0, 0]);
    put_receipt_rdi(&mut code, 835, 200);
    put_imm64(&mut code, 845, 0x48, 0xBE, RIGHT_DERIVE);
    put_imm64(&mut code, 855, 0x48, 0xB8, SYS_AUTHORIZE);
    put(&mut code, 865, &[0xCD, 0x80]);
    put(&mut code, 867, &[0x48, 0x89, 0x81, 216, 0, 0, 0]);
    // drop 新句柄（清理）。
    put_receipt_rdi(&mut code, 874, 200);
    put_imm64(&mut code, 884, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 894, &[0xCD, 0x80]);
    put(&mut code, 896, &[0x48, 0x89, 0x81, 224, 0, 0, 0]);
    // 结构性根 Namespace cap 不可丢弃（§4.5 StructuralCap → EACCES）。
    put_boot_load(&mut code, 903, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    put_imm64(&mut code, 913, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 923, &[0xCD, 0x80]);
    put(&mut code, 925, &[0x48, 0x89, 0x81, 232, 0, 0, 0]);
    // 932: NOP（cap 生命周期块末字节落在 931，此处补 1 字节使下一块按计划从 933 开始；
    //        不留零字节，否则会被当作 `add [rax], al` 执行并触发 #PF 错位）。
    code[932] = 0x90;
    // 933: S2-D3 Segment 原语：create → map → 读零 → 写回 → unmap → destroy →
    //      销毁后 map 被拒（stale）→ 再次 create（复用回收帧）→ map → 读零（S2-X3）→ 清理。
    put_imm64(&mut code, 933, 0x48, 0xBF, 1); // rdi = page_count
    put_imm64(&mut code, 943, 0x48, 0xBE, MEMSEG_RW_BITS); // rsi = allowed_access
    put_imm64(&mut code, 953, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 963, &[0xCD, 0x80]);
    put(&mut code, 965, &[0x48, 0x89, 0x81, 240, 0, 0, 0]);
    // map(seg, task, 0, SEG_VA, 1, R|W)
    put_receipt_rdi(&mut code, 972, 240);
    put_boot_load(&mut code, 982, BootReg::Rsi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 992, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1002, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1012, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1022, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1032, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1042, &[0xCD, 0x80]);
    put(&mut code, 1044, &[0x48, 0x89, 0x81, 248, 0, 0, 0]);
    // 读新页首 qword：必须为 0（S2-X3 清零后才可见）。
    put_imm64(&mut code, 1051, 0x48, 0xBA, USER_SEG_VA);
    put(&mut code, 1061, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1064, &[0x48, 0x89, 0x81, 0, 1, 0, 0]);
    // 写入模式并读回（证明真实可写 PTE）。
    put_imm64(&mut code, 1071, 0x48, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1081, 0x48, 0xB8, MEMSEG_PATTERN);
    put(&mut code, 1091, &[0x48, 0x89, 0x02]);
    put(&mut code, 1094, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1097, &[0x48, 0x89, 0x81, 8, 1, 0, 0]);
    // unmap(task, SEG_VA, 1)
    put_boot_load(&mut code, 1104, BootReg::Rdi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 1114, 0x48, 0xBE, USER_SEG_VA);
    put_imm64(&mut code, 1124, 0x48, 0xBA, 1);
    put_imm64(&mut code, 1134, 0x48, 0xB8, SYS_MEMSEG_UNMAP);
    put(&mut code, 1144, &[0xCD, 0x80]);
    put(&mut code, 1146, &[0x48, 0x89, 0x81, 16, 1, 0, 0]);
    // destroy(seg)
    put_receipt_rdi(&mut code, 1153, 240);
    put_imm64(&mut code, 1163, 0x48, 0xB8, SYS_MEMSEG_DESTROY);
    put(&mut code, 1173, &[0xCD, 0x80]);
    put(&mut code, 1175, &[0x48, 0x89, 0x81, 24, 1, 0, 0]);
    // 销毁后再 map：stale token 不可授权（EACCES）。
    put_receipt_rdi(&mut code, 1182, 240);
    put_boot_load(&mut code, 1192, BootReg::Rsi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 1202, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1212, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1222, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1232, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1242, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1252, &[0xCD, 0x80]);
    put(&mut code, 1254, &[0x48, 0x89, 0x81, 32, 1, 0, 0]);
    // 再次 create：应复用刚归还的帧（回收栈）。
    put_imm64(&mut code, 1261, 0x48, 0xBF, 1);
    put_imm64(&mut code, 1271, 0x48, 0xBE, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1281, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1291, &[0xCD, 0x80]);
    put(&mut code, 1293, &[0x48, 0x89, 0x81, 40, 1, 0, 0]);
    // map2 + 读零（复用帧必须先清零才对用户可见 → S2-X3）。
    put_receipt_rdi(&mut code, 1300, 296);
    put_boot_load(&mut code, 1310, BootReg::Rsi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 1320, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1330, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1340, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1350, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1360, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1370, &[0xCD, 0x80]);
    put(&mut code, 1372, &[0x48, 0x89, 0x81, 48, 1, 0, 0]);
    put_imm64(&mut code, 1379, 0x48, 0xBA, USER_SEG_VA);
    put(&mut code, 1389, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1392, &[0x48, 0x89, 0x81, 56, 1, 0, 0]);
    // unmap2 + destroy2（清理）。
    put_boot_load(&mut code, 1399, BootReg::Rdi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 1409, 0x48, 0xBE, USER_SEG_VA);
    put_imm64(&mut code, 1419, 0x48, 0xBA, 1);
    put_imm64(&mut code, 1429, 0x48, 0xB8, SYS_MEMSEG_UNMAP);
    put(&mut code, 1439, &[0xCD, 0x80]);
    put(&mut code, 1441, &[0x48, 0x89, 0x81, 64, 1, 0, 0]);
    put_receipt_rdi(&mut code, 1448, 296);
    put_imm64(&mut code, 1458, 0x48, 0xB8, SYS_MEMSEG_DESTROY);
    put(&mut code, 1468, &[0xCD, 0x80]);
    put(&mut code, 1470, &[0x48, 0x89, 0x81, 72, 1, 0, 0]);
    // 1477: 切片 S —— 共享 Segment：建段 → 映射进自己的根 → 写魔数 → 经授权消息把段 cap
    //       交给 task 2（此时 task 2 是 Ready，走**排队路径**）→ 让出 → 之后直投唤醒。
    put_imm64(&mut code, 1477, 0x48, 0xBF, 1); // rdi = page_count
    put_imm64(&mut code, 1487, 0x48, 0xBE, MEMSEG_RW_BITS); // rsi = allowed_access
    put_imm64(&mut code, 1497, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1507, &[0xCD, 0x80]);
    put(&mut code, 1509, &[0x48, 0x89, 0x81, 0x50, 0x01, 0, 0]);
    // map(seg, task1_self_cap, 0, SHARED_VA, 1, R|W)
    put(&mut code, 1516, &[0x48, 0x8B, 0xB9, 0x50, 0x01, 0, 0]); // rdi = [Y+336]（新段 cap）
    put_boot_load(&mut code, 1523, BootReg::Rsi, boot_abi::OFF_TASK_CAP);
    put_imm64(&mut code, 1533, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1543, 0x49, 0xBA, USER_SEG_SHARED_VA);
    put_imm64(&mut code, 1553, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1563, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1573, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1583, &[0xCD, 0x80]);
    put(&mut code, 1585, &[0x48, 0x89, 0x81, 0x58, 0x01, 0, 0]);
    // 写魔数（task 2 会在自己的根下读回同一物理帧）。
    put_imm64(&mut code, 1592, 0x48, 0xBA, USER_SEG_SHARED_VA);
    put_imm64(&mut code, 1602, 0x48, 0xB8, IPC_SHARED_MAGIC);
    put(&mut code, 1612, &[0x48, 0x89, 0x02]);
    // send(2, TASK2_SEND_CAP, MAGIC, cap_count=1, cap0=[Y+336])
    put_imm64(&mut code, 1615, 0x48, 0xBF, 2);
    put_boot_load(&mut code, 1625, BootReg::Rsi, boot_abi::OFF_TASK2_SEND_CAP);
    put_imm64(&mut code, 1635, 0x48, 0xBA, IPC_SHARED_MAGIC);
    put_imm64(&mut code, 1645, 0x49, 0xBA, 1);
    put(&mut code, 1655, &[0x4C, 0x8B, 0x81, 0x50, 0x01, 0, 0]); // r8 = [Y+336]
    put_imm64(&mut code, 1662, 0x49, 0xB9, 0);
    put_imm64(&mut code, 1672, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 1682, &[0xCD, 0x80]);
    put(&mut code, 1684, &[0x48, 0x89, 0x81, 0x60, 0x01, 0, 0]);
    // 让出给 task 2（它读走队列里的消息）。
    put_imm64(&mut code, 1691, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 1701, &[0xCD, 0x80]);
    // send(2, TASK2_SEND_CAP, DIRECT_PAYLOAD, cap_count=1, page-fault-source cap)
    // —— task 2 阻塞在 RECV；直投必须连同句柄和数量写回它的 out 缓冲。
    put_imm64(&mut code, 1703, 0x48, 0xBF, 2);
    put_boot_load(&mut code, 1713, BootReg::Rsi, boot_abi::OFF_TASK2_SEND_CAP);
    put_imm64(&mut code, 1723, 0x48, 0xBA, IPC_DIRECT_PAYLOAD);
    put_imm64(&mut code, 1733, 0x49, 0xBA, 1);
    put_boot_load(
        &mut code,
        1743,
        BootReg::R8,
        boot_abi::OFF_EXC_PAGE_FAULT_CAP,
    );
    put_imm64(&mut code, 1753, 0x49, 0xB9, 0);
    put_imm64(&mut code, 1763, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 1773, &[0xCD, 0x80]);
    put(&mut code, 1775, &[0x48, 0x89, 0x81, 0x68, 0x01, 0, 0]);
    // 让出，等 task 2 记录直投回执后交回。
    put_imm64(&mut code, 1782, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 1792, &[0xCD, 0x80]);
    // --- S2-D4：Task create/yield/destroy 用户原语 -------------------------------------------------
    // rcx = Y（回执），rdx = 请求块（用户栈页内，可写）。
    put_imm64(&mut code, 1794, 0x48, 0xB9, USER_Y_VA);
    put_imm64(&mut code, 1804, 0x48, 0xBA, TASK_REQ_VA);
    // 建请求块（10 个 qword）：code_seg_cap/code_offset/code_pages/load_base/entry/stack_top/
    // parent_ns_cap/root_rights/initial_cap0/initial_cap1。
    put_boot_load(
        &mut code,
        1814,
        BootReg::Rax,
        boot_abi::OFF_SERVICE_IMAGE_CAP,
    );
    put(&mut code, 1824, &[0x48, 0x89, 0x02]);
    put_boot_load(
        &mut code,
        1827,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_OFFSET_PAGES),
    );
    put(&mut code, 1837, &[0x48, 0x89, 0x42, 0x08]);
    put_boot_load(
        &mut code,
        1841,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_PAGES),
    );
    put(&mut code, 1851, &[0x48, 0x89, 0x42, 0x10]);
    put_imm64(&mut code, 1855, 0x48, 0xB8, USER3_CODE_VA);
    put(&mut code, 1865, &[0x48, 0x89, 0x42, 0x18]);
    put_boot_load(
        &mut code,
        1869,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_ENTRY_VA),
    );
    put(&mut code, 1879, &[0x48, 0x89, 0x42, 0x20]);
    put_imm64(&mut code, 1883, 0x48, 0xB8, USER3_STACK_VA + FRAME_SIZE);
    put(&mut code, 1893, &[0x48, 0x89, 0x42, 0x28]);
    put_boot_load(&mut code, 1897, BootReg::Rax, boot_abi::OFF_NAMESPACE_CAP);
    put(&mut code, 1907, &[0x48, 0x89, 0x42, 0x30]);
    put_boot_load(
        &mut code,
        1911,
        BootReg::Rax,
        boot_abi::OFF_TASK_CREATE_ROOT_NS_RIGHTS,
    );
    put(&mut code, 1921, &[0x48, 0x89, 0x42, 0x38]);
    put_boot_load(
        &mut code,
        1925,
        BootReg::Rax,
        boot_abi::OFF_TASK_CREATE_SEND_CAP,
    );
    put(&mut code, 1935, &[0x48, 0x89, 0x42, 0x40]);
    put_imm64(&mut code, 1939, 0x48, 0xB8, 0);
    put(&mut code, 1949, &[0x48, 0x89, 0x42, 0x48]);
    // 栈段 = SYS_MEMSEG_CREATE(1, R|W) → [Y+368]
    put_imm64(&mut code, 1953, 0x48, 0xBF, 1);
    put_imm64(&mut code, 1963, 0x48, 0xBE, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1973, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1983, &[0xCD, 0x80]);
    put(&mut code, 1985, &[0x48, 0x89, 0x81, 0x70, 0x01, 0, 0]);
    // mgr = SYS_TASK_CREATE(req)
    put(&mut code, 1992, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 1995, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2005, &[0xCD, 0x80]);
    put(&mut code, 2007, &[0x48, 0x89, 0x81, 0x78, 0x01, 0, 0]);
    // 负例 1：entry 越出代码段 → EINVAL。用**代码段末尾**（one-past-end）而非硬编码 `+0x9000`：
    // 后者在嵌入 ELF 增长到 ≥10 页（`code_end = +0xA000`）后会落入代码段内，使本负例改由
    // 后置的 initial_cap 检查以 EACCES 拒绝（SPEC-088 实测暴露）。
    put_boot_load(
        &mut code,
        2014,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_CODE_END_VA),
    );
    put(&mut code, 2024, &[0x48, 0x89, 0x42, 0x20]);
    put(&mut code, 2028, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2031, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2041, &[0xCD, 0x80]);
    put(&mut code, 2043, &[0x48, 0x89, 0x81, 0x80, 0x01, 0, 0]);
    put_imm64(&mut code, 2050, 0x48, 0xB8, USER3_CODE_VA);
    put(&mut code, 2060, &[0x48, 0x89, 0x42, 0x20]);
    // 负例 2：code_seg_cap 换成只有 R|W 的栈段 cap（缺 SegMapX）→ EACCES
    put(&mut code, 2064, &[0x48, 0x8B, 0x81, 0x70, 0x01, 0, 0]);
    put(&mut code, 2071, &[0x48, 0x89, 0x02]);
    put(&mut code, 2074, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2077, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2087, &[0xCD, 0x80]);
    put(&mut code, 2089, &[0x48, 0x89, 0x81, 0x88, 0x01, 0, 0]);
    put_boot_load(
        &mut code,
        2096,
        BootReg::Rax,
        boot_abi::OFF_SERVICE_IMAGE_CAP,
    );
    put(&mut code, 2106, &[0x48, 0x89, 0x02]);
    // 栈映射进**子任务**根：SYS_MEMSEG_MAP(栈段, mgr, 0, USER3_STACK_VA, 1, R|W)
    put(&mut code, 2109, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2116, &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2123, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2133, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2143, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2153, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2163, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2173, &[0xCD, 0x80]);
    put(&mut code, 2175, &[0x48, 0x89, 0x81, 0x90, 0x01, 0, 0]);
    // 第一次让出：此时 task 2 仍 Ready（id 2 < 3），会被优先调度；它恢复后进入代码尾部的
    // 阻塞接收循环，从而把 CPU 交回本任务。第二次让出才会真正轮到子任务（id 3）。
    put_imm64(&mut code, 2182, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2192, &[0xCD, 0x80]);
    put_imm64(&mut code, 2194, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2204, &[0xCD, 0x80]);
    // recv = SYS_RECV(TASK_RECV_BUF)（子任务回发自己的 id）
    put_imm64(&mut code, 2206, 0x48, 0xBF, TASK_RECV_BUF);
    put_imm64(&mut code, 2216, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 2226, &[0xCD, 0x80]);
    put(&mut code, 2228, &[0x48, 0x89, 0x81, 0x98, 0x01, 0, 0]);
    // 负例 3：用不含 TaskDestroy 的栈段 cap 销毁 → EACCES
    put(&mut code, 2235, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put_imm64(&mut code, 2242, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2252, &[0xCD, 0x80]);
    put(&mut code, 2254, &[0x48, 0x89, 0x81, 0xA0, 0x01, 0, 0]);
    // 销毁子任务：SYS_TASK_DESTROY(mgr)
    put(&mut code, 2261, &[0x48, 0x8B, 0xB9, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2268, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2278, &[0xCD, 0x80]);
    put(&mut code, 2280, &[0x48, 0x89, 0x81, 0xA8, 0x01, 0, 0]);
    // 负例 4：重复销毁（目标已销毁）→ 拒绝
    put(&mut code, 2287, &[0x48, 0x8B, 0xB9, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2294, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2304, &[0xCD, 0x80]);
    put(&mut code, 2306, &[0x48, 0x89, 0x81, 0xB0, 0x01, 0, 0]);
    // 负例 5：向已销毁的子任务地址空间映射 → 拒绝
    put(&mut code, 2313, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2320, &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2327, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2337, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2347, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2357, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2367, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2377, &[0xCD, 0x80]);
    put(&mut code, 2379, &[0x48, 0x89, 0x81, 0xB8, 0x01, 0, 0]);
    // 唤醒阻塞在尾部循环里的 task 2：收尾的抢占阶段需要存在 Ready 任务可轮转。
    put_imm64(&mut code, 2386, 0x48, 0xBF, 2);
    put_boot_load(&mut code, 2396, BootReg::Rsi, boot_abi::OFF_TASK2_SEND_CAP);
    put_imm64(&mut code, 2406, 0x48, 0xBA, TASK2_WAKE_PAYLOAD);
    put_imm64(&mut code, 2416, 0x49, 0xBA, 0);
    put_imm64(&mut code, 2426, 0x49, 0xB8, 0);
    put_imm64(&mut code, 2436, 0x49, 0xB9, 0);
    put_imm64(&mut code, 2446, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 2456, &[0xCD, 0x80]);
    // SPEC-066：用**自持**的平台段 cap（切片 U 已按属主移交）经 `SYS_MEMSEG_MAP` 建立新映射。
    // 正例：X 段 cap（槽 36）+ Task(1) 管理 cap（槽 45）→ 应在**用户侧**通过 `authorize`。
    put_boot_load(
        &mut code,
        2458,
        BootReg::Rdi,
        boot_abi::OFF_PLATFORM_TASK1_X_CAP,
    );
    put_boot_load(
        &mut code,
        2468,
        BootReg::Rsi,
        boot_abi::OFF_PLATFORM_TASK1_CAP,
    );
    put_imm64(&mut code, 2478, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2488, 0x49, 0xBA, USER_MAP_WITNESS_VA);
    put_imm64(&mut code, 2498, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2508, 0x49, 0xB9, ACCESS_R_BITS);
    put_imm64(&mut code, 2518, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2528, &[0xCD, 0x80]);
    put(&mut code, 2530, &[0x48, 0x89, 0x81, 0xC0, 0x01, 0, 0]);
    // 读回该映射：应等于 X 段首帧的 COW 模式值——证明 PTE 真的指向该 Segment 的帧，
    // 而不是「映射调用返回 0」而已。
    put_imm64(&mut code, 2537, 0x48, 0xBA, USER_MAP_WITNESS_VA);
    put(&mut code, 2547, &[0x48, 0x8B, 0x02]);
    put(&mut code, 2550, &[0x48, 0x89, 0x81, 0xC8, 0x01, 0, 0]);
    // 负例：用 **task 2 所属**的段 cap（槽 38）以 task 1 为 caller 再映射 → `EACCES`
    // （`authorize` 第 3 步的 `holder_of(cap) == caller` 对非属主成立拒绝）。
    put_boot_load(
        &mut code,
        2557,
        BootReg::Rdi,
        boot_abi::OFF_PLATFORM_TASK2_CODE_CAP,
    );
    put_boot_load(
        &mut code,
        2567,
        BootReg::Rsi,
        boot_abi::OFF_PLATFORM_TASK1_CAP,
    );
    put_imm64(&mut code, 2577, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2587, 0x49, 0xBA, USER_MAP_WITNESS_VA2);
    put_imm64(&mut code, 2597, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2607, 0x49, 0xB9, ACCESS_R_BITS);
    put_imm64(&mut code, 2617, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2627, &[0xCD, 0x80]);
    put(&mut code, 2629, &[0x48, 0x89, 0x81, 0xD0, 0x01, 0, 0]);
    // SPEC-067（切片 V）：task 4 承载自毁见证。
    // rdx 在 SPEC-066 块里被用作映射目标 VA，这里重设为请求块基址。
    put_imm64(&mut code, 2636, 0x48, 0xBA, TASK_REQ_VA);
    // 改写请求块：entry = 代码段首 + TASK4_CODE_OFFSET（同页内偏移），initial_cap0 = task4 的发送凭据。
    put_imm64(
        &mut code,
        2646,
        0x48,
        0xB8,
        USER3_CODE_VA + TASK4_CODE_OFFSET,
    );
    put(&mut code, 2656, &[0x48, 0x89, 0x42, 0x20]);
    put_boot_load(&mut code, 2660, BootReg::Rax, boot_abi::OFF_TASK4_SEND_CAP);
    put(&mut code, 2670, &[0x48, 0x89, 0x42, 0x40]);
    // create task 4
    put(&mut code, 2674, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2677, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2687, &[0xCD, 0x80]);
    put(&mut code, 2689, &[0x48, 0x89, 0x81, 0xD8, 0x01, 0, 0]);
    // 复用 task 3 的栈段 cap（段对象不随 task 3 销毁），把栈映射进 **task 4** 的根。
    put(&mut code, 2696, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2703, &[0x48, 0x8B, 0xB1, 0xD8, 0x01, 0, 0]);
    put_imm64(&mut code, 2710, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2720, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2730, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2740, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2750, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2760, &[0xCD, 0x80]);
    put(&mut code, 2762, &[0x48, 0x89, 0x81, 0xE0, 0x01, 0, 0]);
    // 让出两次：第一次排空刚被唤醒的 task 2（它执行尾部阻塞接收后变 BlockedRecv），
    // 第二次才轮到 task 4（id 4）。
    put_imm64(&mut code, 2769, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2779, &[0xCD, 0x80]);
    put_imm64(&mut code, 2781, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2791, &[0xCD, 0x80]);
    // task 4 自报 id 后自毁并切回本任务；独立缓冲区保留 task 3 移交的 Segment cap。
    put_imm64(&mut code, 2793, 0x48, 0xBF, TASK_RECV_BUF + 0x20);
    put_imm64(&mut code, 2803, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 2813, &[0xCD, 0x80]);
    put(&mut code, 2815, &[0x48, 0x89, 0x81, 0xE8, 0x01, 0, 0]);
    // task 4 已把 id 排队；让出一次让它完成自毁，再回来验证。
    put_imm64(&mut code, 2822, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2832, &[0xCD, 0x80]);
    // 向已自毁的 task 4 映射 → 应 ENOENT（其调度登记已清、地址空间已拆）。
    put(&mut code, 2834, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2841, &[0x48, 0x8B, 0xB1, 0xD8, 0x01, 0, 0]);
    put_imm64(&mut code, 2848, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2858, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2868, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2878, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2888, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2898, &[0xCD, 0x80]);
    put(&mut code, 2900, &[0x48, 0x89, 0x81, 0xF0, 0x01, 0, 0]);
    // 收尾的抢占阶段需要 Ready 任务可轮转：切片 V 的第一次让出已把 task 2 排空成
    // `BlockedRecv`，故这里再直投唤醒它一次（同切片 T 末尾的做法，位置后移）。
    put_imm64(&mut code, 2907, 0x48, 0xBF, 2);
    put_boot_load(&mut code, 2917, BootReg::Rsi, boot_abi::OFF_TASK2_SEND_CAP);
    put_imm64(&mut code, 2927, 0x48, 0xBA, TASK2_WAKE_PAYLOAD);
    put_imm64(&mut code, 2937, 0x49, 0xBA, 0);
    put_imm64(&mut code, 2947, 0x49, 0xB8, 0);
    put_imm64(&mut code, 2957, 0x49, 0xB9, 0);
    put_imm64(&mut code, 2967, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 2977, &[0xCD, 0x80]);

    // ---- SPEC-081：§11 Namespace 路径操作（切片 AJ）----
    // 布局：本块接在 SPEC-067 块之后；末尾 `int3` 收尾，路径文本数据放在代码页高偏移
    // （`int3` 之后不会被当作指令执行）。`rcx` 恒为回执页基址（syscall 返回后由内核
    // 保存/恢复全部用户寄存器，故一直有效）。
    let mut at = 2979usize;
    put_imm64(&mut code, at, 0x48, 0xB9, USER_Y_VA);
    at += 10;

    // child1/child2 = derive(image_cap, {Derive, SegMapR})——避免移动初始 image cap。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_BIND_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DERIVE1 as u32);
    at += 7;
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_IMAGE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_BIND_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DERIVE2 as u32);
    at += 7;

    // ns_create ×3（a / b / c），回执依次写 512/520/528。
    let mut created = 0u32;
    while created < 3 {
        put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_CREATE);
        at += 10;
        put(&mut code, at, &[0xCD, 0x80]);
        at += 2;
        put_store_rax(&mut code, at, NS_RECEIPT_CREATE_A as u32 + created * 8);
        at += 7;
        created += 1;
    }

    // b_copy / c_copy = derive(ns 根 cap, NAMESPACE_RIGHTS)——挂载会移走源 cap，副本留作
    // 环检测的 destination 与销毁见证。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_B as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_ROOT_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_COPY_B as u32);
    at += 7;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_C as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_ROOT_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_COPY_C as u32);
    at += 7;

    // mount(a, "x", b)：建立边 a → b（b 的 cap 移入 a）。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_A as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_X_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 1);
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x4C,
        NS_MODRM_R10,
        NS_RECEIPT_CREATE_B as u32,
    );
    at += 7;
    put_syscall(&mut code, at, SYS_NS_MOUNT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_A_B as u32);
    at += 7;

    // mount(b_copy, "y", a)：a 沿边可达 b ⇒ 成环 ⇒ EINVAL。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_COPY_B as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_Y_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 1);
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x4C,
        NS_MODRM_R10,
        NS_RECEIPT_CREATE_A as u32,
    );
    at += 7;
    put_syscall(&mut code, at, SYS_NS_MOUNT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_CYCLE as u32);
    at += 7;

    // bind(init_ns, "lib", child1) 成功；再 bind 同名 child2 ⇒ EEXIST。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE1 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_NS_BIND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND as u32);
    at += 7;

    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE2 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_NS_BIND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND_OVERLAP as u32);
    at += 7;

    // bind(c, "sub", child2)：为跨挂载解析准备子 Namespace 内的绑定项。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_C as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_SUB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE2 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_NS_BIND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND_SUB as u32);
    at += 7;

    // resolve(init_ns, "lib", {NsResolve})：同一 Namespace 内的末端绑定解析。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 1 << 1); // r10 = {SegMapR}（父项 child1 的合法子集）
    at += 10;
    put_syscall(&mut code, at, SYS_NS_RESOLVE);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_RESOLVE_LIB as u32);
    at += 7;

    // mount(init_ns, "mnt", c_copy)：把子 Namespace 挂到 init 下。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_MNT_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_COPY_C as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_NS_MOUNT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_INIT_C as u32);
    at += 7;

    // resolve(init_ns, "mnt/sub", {SegMapR})：跨挂载解析，父项是 c 内的 child2。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(
        &mut code,
        at,
        0x48,
        0xBE,
        NS_PATH_DATA_VA + NS_PATH_MNTSUB_OFF,
    );
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 7);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 1 << 1); // r10 = {SegMapR}
    at += 10;
    put_syscall(&mut code, at, SYS_NS_RESOLVE);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_RESOLVE_SUB as u32);
    at += 7;

    // bind(init_ns, "/lib", task_cap)：前导 `/` 产生空分量 ⇒ Path::parse 拒绝 ⇒ EINVAL。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(
        &mut code,
        at,
        0x48,
        0xBE,
        NS_PATH_DATA_VA + NS_PATH_ILLEGAL_OFF,
    );
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 4);
    at += 10;
    put_boot_load(&mut code, at, BootReg::R10, boot_abi::OFF_TASK_CAP);
    at += 10;
    put_syscall(&mut code, at, SYS_NS_BIND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BAD_PATH as u32);
    at += 7;

    // unbind(init_ns, "lib") → 取回 child1；unmount(init_ns, "mnt")。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_syscall(&mut code, at, SYS_NS_UNBIND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_UNBIND as u32);
    at += 7;

    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_MNT_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_syscall(&mut code, at, SYS_NS_UNMOUNT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_UNMOUNT as u32);
    at += 7;

    // destroy(c_copy)：清空 c 的条目、从活动表删除对象（c_copy 变 stale token）。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_COPY_C as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_NS_DESTROY);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DESTROY as u32);
    at += 7;

    // ---- SPEC-082：§10 Timer（切片 AK）----
    // `now` 在 ring3 阶段恒为 0（§10.3 的推进由 CPL3 抢占 tick 驱动），但这里仍经
    // `SYS_TIME_NOW` 读取并按相对偏移构造 deadline——不把「now 恰好是 0」写死进机码。
    put_syscall(&mut code, at, SYS_TIME_NOW);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_NOW as u32);
    at += 7;
    // t1 = timer_set(now + 2)：未来 deadline ⇒ Pending。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, TIMER_RECEIPT_NOW as u32);
    at += 7;
    put_add_rdi(&mut code, at, 2);
    at += 4;
    put_syscall(&mut code, at, SYS_TIMER_SET);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_T1 as u32);
    at += 7;
    // t2 = timer_set(0)：deadline ≤ now ⇒ 同一原子操作内 Fired（S3-D4）。
    put_imm64(&mut code, at, 0x48, 0xBF, 0);
    at += 10;
    put_syscall(&mut code, at, SYS_TIMER_SET);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_T2 as u32);
    at += 7;
    // t3 = timer_set(now + 1)：Pending，随后被显式取消两次。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, TIMER_RECEIPT_NOW as u32);
    at += 7;
    put_add_rdi(&mut code, at, 1);
    at += 4;
    put_syscall(&mut code, at, SYS_TIMER_SET);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_T3 as u32);
    at += 7;
    // timer_cancel(t3) → 0（Cancelled）；再次 → 2（AlreadyCancelled，状态返回）。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, TIMER_RECEIPT_T3 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_TIMER_CANCEL);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_CANCEL as u32);
    at += 7;
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, TIMER_RECEIPT_T3 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_TIMER_CANCEL);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_CANCEL_AGAIN as u32);
    at += 7;
    // timer_destroy(t2)：Fired 的已排队通知不撤回。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, TIMER_RECEIPT_T2 as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_TIMER_DESTROY);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, TIMER_RECEIPT_DESTROY as u32);
    at += 7;

    // §13.2 `audit(cap)`：只查询、不改状态。复用 Namespace 块取回的 child1（仍被 init 持有，
    // 派生自镜像根），以及位置已变、目标已销毁但仍被持有的 c_copy；两条回执交给内核侧重算比对。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_UNBIND as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_AUDIT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, AUDIT_RECEIPT_LIVE as u32);
    at += 7;
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_COPY_C as u32);
    at += 7;
    put_syscall(&mut code, at, SYS_AUDIT);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, AUDIT_RECEIPT_STALE as u32);
    at += 7;

    // Stage 4 S4-D5：接收 Rust ring3 客户端随 IPC 移交的共享 Segment，映射到 init 并核验
    // `PRIM` header/offer。Task 3 已在 TASK_RECV_BUF 中写入 cap0/cap_count。
    put_imm64(&mut code, at, 0x48, 0xB9, USER_Y_VA);
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        (TASK_RECV_BUF - USER_Y_VA) as u32,
    );
    at += 7;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rsi,
        boot_abi::OFF_PLATFORM_TASK1_CAP,
    );
    at += 10;
    put(&mut code, at, &[0x31, 0xD2]); // xor edx, edx（写入 32 位寄存器会清零 rdx）
    at += 2;
    put_imm64(&mut code, at, 0x49, 0xBA, USER_HELLO_VA);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB8, 1);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB9, ACCESS_R_BITS);
    at += 10;
    put_syscall(&mut code, at, SYS_MEMSEG_MAP);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, 736);
    at += 7;

    // SPEC-087：代码页扩到 2 页（`USER_CODE_LEN = 8192`）。页 0 在 `USER_CODE_VA`（与
    // `USER_STACK_VA` 相邻），页 1 落在空闲槽 `USER_CODE2_VA`（+0x5000），故从页 0 尾部
    // `jmp rel32` 跳过去执行追加块；两页之间的空隙保持为 0。
    let rel = USER_CODE2_VA - (USER_CODE_VA + at as u64 + 5);
    code[at] = 0xE9;
    put(&mut code, at + 1, &(rel as u32).to_le_bytes());
    let mut at = USER_CODE_LEN / 2;

    // ===== SPEC-104：ring3 服务管理器（**最先建**——端点/客户端以它为对端）=====
    // 决策见设计 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`。
    // 管理器的**运行期注册表/按名解析/路由转发**由 `prima-servicemgr` 承载（本块之后的手写块
    // 不再解析/转发）。init 只负责建它、派生**指向它的发送凭据**（移交端点/客户端作
    // `initial_cap0`），并在最后收下它交回的共享段只读副本（内核见证据此读回解析结果）。
    // 请求块在此**自足**写入（code_seg_cap/offset/pages/entry/initial_cap0/initial_cap1/name）。
    // 本块位于代码页 1 的**最前**，故须自行建立 Y 页基址（`rcx`）——后续各块的 `[rcx+disp]`
    // 回执依赖它。
    put_imm64(&mut code, at, 0x48, 0xB9, USER_Y_VA); // rcx = Y（回执页）
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, TASK_REQ_VA); // rdx = 请求块
    at += 10;
    put_boot_load(&mut code, at, BootReg::Rax, boot_abi::OFF_SERVICE_IMAGE_CAP);
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x02]); // mov [rdx+0x00], rax（code_seg_cap）
    at += 3;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.servicemgr_index, boot_abi::ENTRY_OFF_OFFSET_PAGES),
    ); // code_offset
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x08]);
    at += 4;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.servicemgr_index, boot_abi::ENTRY_OFF_PAGES),
    ); // code_pages
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x10]);
    at += 4;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.servicemgr_index, boot_abi::ENTRY_OFF_ENTRY_VA),
    );
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x20]); // mov [rdx+0x20], rax（entry）
    at += 4;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::OFF_SERVICEMGR_SEND_CAP,
    );
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x40]); // mov [rdx+0x40], rax（initial_cap0：→init 回发凭据）
    at += 4;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::OFF_SERVICE_IMAGE_MGR_CAP,
    ); // initial_cap1 = 服务映像段（移动进管理器，供其读目录 + 派生端点代码）
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x48]);
    at += 4;
    put_imm64(&mut code, at, 0x48, 0xB8, 0); // name = 0（服务管理器无服务名）
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x50]);
    at += 4;
    put(&mut code, at, &[0x48, 0x89, 0xD7]); // mov rdi, rdx
    at += 3;
    put_syscall(&mut code, at, SYS_TASK_CREATE);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, SERVICEMGR_RECEIPT_MGR as u32);
    at += 7;
    // 管理器栈向下增长：8 页连续映射（栈段容量见内核），栈顶仍为 USER3_STACK_VA+4K。
    put_boot_load(
        &mut code,
        at,
        BootReg::Rdi,
        boot_abi::OFF_SERVICE3_STACK_CAP,
    );
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RSI,
        SERVICEMGR_RECEIPT_MGR as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBA, 0);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, USER3_STACK_VA - 7 * FRAME_SIZE);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB8, 8);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB9, MEMSEG_RW_BITS);
    at += 10;
    put_syscall(&mut code, at, SYS_MEMSEG_MAP);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    // 让出：管理器运行、自报存活后阻塞在 recv。
    put_syscall(&mut code, at, SYS_YIELD);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    // recv 循环直到 payload 高 16 位 == SERVICEMGR_HELLO_TAG 高 16 位（0x5E56）。
    let mgr_loop = at;
    put_imm64(&mut code, at, 0x48, 0xBF, SVC_RECV_BUF);
    at += 10;
    put_syscall(&mut code, at, SYS_RECV);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put(&mut code, at, &[0x48, 0x89, 0xC2]); // mov rdx, rax
    at += 3;
    put(&mut code, at, &[0x48, 0xC1, 0xEA, 0x30]); // shr rdx, 48
    at += 4;
    put(&mut code, at, &[0x81, 0xFA]); // cmp edx, 0x5E56
    put(
        &mut code,
        at + 2,
        &((SERVICEMGR_HELLO_TAG >> 48) as u32).to_le_bytes(),
    );
    at += 6;
    put(&mut code, at, &[0x0F, 0x85]); // jne mgr_loop
    put(
        &mut code,
        at + 2,
        &((mgr_loop as i64 - (at as i64 + 6)) as i32).to_le_bytes(),
    );
    at += 6;
    put_store_rax(&mut code, at, SERVICEMGR_RECEIPT_HELLO as u32);
    at += 7;

    // ===== SPEC-087：第二个（**存活**）Rust 客户端任务 + init 双向握手 =====
    // rcx = Y（回执页），rdx = 请求块（用户栈页内，可写）。
    put_imm64(&mut code, at, 0x48, 0xB9, USER_Y_VA);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, TASK_REQ_VA);
    at += 10;
    // SPEC-104：页 1 起始处的服务管理器块已改写共享请求块，故此处**显式**写回本客户端
    // （`prima-smoke` 映像）的代码段字段，避免误用管理器的代码段。
    put_boot_load(&mut code, at, BootReg::Rax, boot_abi::OFF_SERVICE_IMAGE_CAP);
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x02]); // mov [rdx+0x00], rax（code_seg_cap）
    at += 3;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_OFFSET_PAGES),
    ); // code_offset
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x08]);
    at += 4;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_PAGES),
    ); // code_pages
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x10]);
    at += 4;
    // 改写请求块：其余字段（load_base/stack_top/parent_ns/root_rights）沿用 task 3 的。
    put_boot_load(
        &mut code,
        at,
        BootReg::Rax,
        boot_abi::entry_field(manifest.alpha_index, boot_abi::ENTRY_OFF_ENTRY_VA),
    );
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x20]); // mov [rdx+0x20], rax（entry）
    at += 4;
    // SPEC-104：initial_cap0 = **指向 servicemgr 的发送凭据**（从管理器的管理 cap 派生
    // `{TaskSend}`）。派生紧邻使用：`initial_caps` 在 `task_create` 中**移动**进新任务，
    // 故每次 create 前各派生一份。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        SERVICEMGR_RECEIPT_MGR as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, ACCESS_TASKSEND_BITS);
    at += 10;
    put_syscall(&mut code, at, SYS_CAP_DERIVE);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x40]); // mov [rdx+0x40], rax（initial_cap0）
    at += 4;
    // SPEC-109：请求块在页 1 被服务管理器块改用，`[rdx+0x48]` 已被写入映像段 cap（移交后失效）。
    // 客户端只带 initial_cap0，故显式清零 initial_cap1，避免复用一个已移动走的句柄。
    put_imm64(&mut code, at, 0x48, 0xB8, 0); // initial_cap1 = 0
    at += 10;
    put(&mut code, at, &[0x48, 0x89, 0x42, 0x48]); // mov [rdx+0x48], rax
    at += 4;
    // create：第三个 Rust 任务（存活，不再销毁）。
    put(&mut code, at, &[0x48, 0x89, 0xD7]); // mov rdi, rdx
    at += 3;
    put_syscall(&mut code, at, SYS_TASK_CREATE);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, CLIENT_RECEIPT_MGR as u32);
    at += 7;
    // 把栈段映射进客户端根：memseg_map(栈段, mgr, 0, USER3_STACK_VA, 1, R|W)
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        crate::syscall::TASK_RECEIPT_STACK_SEG as u32,
    );
    at += 7;
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RSI, CLIENT_RECEIPT_MGR as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBA, 0);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, USER3_STACK_VA);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB8, 1);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB9, MEMSEG_RW_BITS);
    at += 10;
    put_syscall(&mut code, at, SYS_MEMSEG_MAP);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, CLIENT_RECEIPT_STACK as u32);
    at += 7;
    // 让出一次：客户端（刚创建、处于 Ready）运行并向 **servicemgr** 发送 Hello（携带共享段 cap
    // + 自管理凭据），随后阻塞等待 `PONG`——init 不再参与该握手；共享段映射与应答都由管理器
    // 完成，结果经管理器交回的只读副本在收尾由内核读回。**只让出一次**：此刻只有刚建起的客户端
    // 必然 Ready，第二次让出会落到「无 Ready 任务」而使内核 panic。
    put_syscall(&mut code, at, SYS_YIELD);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;

    // ===== SPEC-105：把建端点所需的 cap 与参数移交 ring3 服务管理器 =====
    // 决策见设计 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`。
    // 端点（ALPHA/BETA）不再由 init 手写创建：init 只移交五种 cap——父 Namespace + 每服务的
    // 「代码段 + 栈段」——由管理器按表 `task_create` + 映射栈、并自解 ELF 入口。移交用三条带 cap
    // 的消息（≤2 cap/条）；每条**先发一条无 cap 唤醒**（管理器常态阻塞在 `recv`，内核拒绝向
    // `BlockedRecv` 投递带 cap 消息，见 `syscall.rs::send_call`）。同一 `at` 段内连续发、不让出，
    // 故唤醒必然先于带 cap 消息。接收者 id 取自管理器存活自报回执的低 8 位（非硬编码字面量）。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        SERVICEMGR_RECEIPT_HELLO as u32,
    );
    at += 7;
    put(&mut code, at, &[0x48, 0x81, 0xE7, 0xFF, 0x00, 0x00, 0x00]); // and rdi, 0xff
    at += 7;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RSI,
        SERVICEMGR_RECEIPT_MGR as u32,
    );
    at += 7;
    // SPEC-109：服务代码段不再由 `init` 逐条移交——**单一服务映像段**随管理器的 `initial_cap1`
    // 零 IPC 移交，管理器读段首目录按表建端点（见 `prima-servicemgr`）。此处只余父 Namespace。
    // 4) 父 Namespace（单 cap）——管理器的 `task_create` 前置要求持 `NsResolve|Derive`。
    at = emit_send(&mut code, at, SERVICE_WAKE_TAG, 0, 0, 0);
    // ISSUE-AUTH-004：移交的是 init 结构根的**派生副本**，不是根本身——§9.1 前置 6 禁止发送
    // 活动 Task 的结构性根；内核发送侧守卫已启用。派生紧邻 setup 发送，句柄直接取自 rax。
    put_boot_load(&mut code, at, BootReg::Rdi, boot_abi::OFF_NAMESPACE_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_ROOT_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    // 派生调用覆盖了 rdi/rsi（父句柄/权限位），setup 发送前必须重新装入接收者与发送凭据。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        SERVICEMGR_RECEIPT_HELLO as u32,
    );
    at += 7;
    put(&mut code, at, &[0x48, 0x81, 0xE7, 0xFF, 0x00, 0x00, 0x00]); // and rdi, 0xff
    at += 7;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RSI,
        SERVICEMGR_RECEIPT_MGR as u32,
    );
    at += 7;
    // 带 cap 的 setup：cap0 取自已派生的 rax 副本（不是 `[r15+OFF_NAMESPACE_CAP]` 的根）。
    put_imm64(&mut code, at, 0x48, 0xBA, SERVICE_SETUP_TAG);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 1);
    at += 10;
    put(&mut code, at, &[0x49, 0x89, 0xC0]); // mov r8, rax
    at += 3;
    put_imm64(&mut code, at, 0x49, 0xB9, 0);
    at += 10;
    put_syscall(&mut code, at, SYS_SEND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    // 让出：管理器按表创建两端点（入口自解）、端点自报、客户端 Hello 经管理器路由。
    put_syscall(&mut code, at, SYS_YIELD);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;

    // ===== SPEC-104：收下服务管理器交回的共享段只读副本（内核见证由此读回）=====
    // 管理器在完成注册表收集、按名解析与两轮转发后，把客户端共享段的只读副本**三段连发**交回：
    // 先无 cap 唤醒、再带 cap、最后 finish。init 循环 recv：收到带 cap 的消息即映射到
    // `USER_HELLO2_VA`（内核随后按此 VA 读回协商/解析结果）；收到 finish tag 即退出。三段连发
    // 中间不让出，故唤醒先于带 cap 消息，宿主的 `BlockedRecv` 约束不会触发（`syscall.rs::send_call`）。
    let mgr_loop = at;
    put_imm64(&mut code, at, 0x48, 0xBF, CLIENT_RECV_BUF);
    at += 10;
    put_syscall(&mut code, at, SYS_RECV);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    // 保存 payload（`rax`），供映射后判断 finish tag（映射会把 `rax` 覆盖成回执）。
    put_store_rax(&mut code, at, CLIENT_RECEIPT_RECV as u32);
    at += 7;
    put(&mut code, at, &[0x48, 0x83, 0xB9]); // cmp qword [rcx+disp32], 0（cap_count）
    put(
        &mut code,
        at + 3,
        &((CLIENT_RECV_BUF - USER_Y_VA + 16) as u32).to_le_bytes(),
    );
    code[at + 7] = 0x00;
    at += 8;
    let je_nocap = at;
    put(&mut code, at, &[0x0F, 0x84]); // je no_cap
    at += 6;
    // 把收到的段 cap（cap0）映射进 **init 根**：memseg_map(段 cap, 平台 task1 cap, 0, USER_HELLO2_VA, 1, R)。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        (CLIENT_RECV_BUF - USER_Y_VA) as u32,
    );
    at += 7;
    put_boot_load(
        &mut code,
        at,
        BootReg::Rsi,
        boot_abi::OFF_PLATFORM_TASK1_CAP,
    );
    at += 10;
    put(&mut code, at, &[0x31, 0xD2]); // xor edx, edx
    at += 2;
    put_imm64(&mut code, at, 0x49, 0xBA, USER_HELLO2_VA);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB8, 1);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB9, ACCESS_R_BITS);
    at += 10;
    put_syscall(&mut code, at, SYS_MEMSEG_MAP);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, CLIENT_RECEIPT_MAP as u32);
    at += 7;
    let no_cap = at;
    put(
        &mut code,
        je_nocap + 2,
        &((no_cap as i64 - (je_nocap as i64 + 6)) as i32).to_le_bytes(),
    );
    // 取出保存的 payload：高 16 位 == SERVICEMGR_FINISH_TAG 高 16 位（0x5E59）即退出。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDX,
        CLIENT_RECEIPT_RECV as u32,
    ); // mov rdx, [rcx+RECV]
    at += 7;
    put(&mut code, at, &[0x48, 0xC1, 0xEA, 0x30]); // shr rdx, 48
    at += 4;
    put(&mut code, at, &[0x81, 0xFA]); // cmp edx, 0x5E59
    put(
        &mut code,
        at + 2,
        &((SERVICEMGR_FINISH_TAG >> 48) as u32).to_le_bytes(),
    );
    at += 6;
    put(&mut code, at, &[0x0F, 0x85]); // jne mgr_loop
    put(
        &mut code,
        at + 2,
        &((mgr_loop as i64 - (at as i64 + 6)) as i32).to_le_bytes(),
    );
    at += 6;

    // 收尾的抢占阶段需要 Ready 任务可轮转：本块的两次让出已把 task 2 排空成 `BlockedRecv`
    // （同 S2-D4 块末尾的情形），故末尾再做一次直投唤醒 `send(2, TASK2_SEND_CAP,
    // TASK2_WAKE_PAYLOAD, 0, 0, 0)`，使 task 2 在 int3 时重新为 Ready。
    put_imm64(&mut code, at, 0x48, 0xBF, TASK2_ID as u64);
    at += 10;
    put_boot_load(&mut code, at, BootReg::Rsi, boot_abi::OFF_TASK2_SEND_CAP);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, TASK2_WAKE_PAYLOAD);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 0);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB8, 0);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xB9, 0);
    at += 10;
    put_syscall(&mut code, at, SYS_SEND);
    at += 5;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;

    // int3（收尾）：追加块不得越过第二页。
    assert!(at < USER_CODE_LEN, "SPEC-087 追加块越过第二页");
    code[at] = 0xCC;
    code
}
