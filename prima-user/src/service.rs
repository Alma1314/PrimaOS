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
//!
//! **M8（阶段 5 片 D）**：存储服务在同一个段的偏移 8 起格式化并托管一个**用户态 VFS/rootfs**
//! （`prima_runtime::vfs`）：12 个静态节点、多级目录、普通文件与 `/dev/null` 占位。文件/目录
//! 句柄、路径解析、`O_CREAT`/`O_TRUNC`/`O_APPEND`/`O_EXCL`/`O_DIRECTORY` 与目录字节流都在本
//! 服务内完成；管理器只做授权与路由，内核不解析文件树。

use prima_runtime::{
    Capability, Rights, Runtime, SyscallTransport, TaskId,
    protocol::{
        SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_STORE, SERVICE_DATA_VA, SERVICE_FILE_ARG_MASK,
        SERVICE_FILE_CHUNK, SERVICE_FILE_EACCES, SERVICE_FILE_EAGAIN, SERVICE_FILE_EBADF,
        SERVICE_FILE_EEXIST, SERVICE_FILE_EFAULT, SERVICE_FILE_EINVAL, SERVICE_FILE_ELOOP,
        SERVICE_FILE_ENOENT, SERVICE_FILE_ENOSPC, SERVICE_FILE_ENOSYS, SERVICE_FILE_ENOTDIR,
        SERVICE_FILE_FD_SHIFT, SERVICE_FILE_HANDLES, SERVICE_FILE_OP_CLOSE, SERVICE_FILE_OP_LSEEK,
        SERVICE_FILE_OP_OPEN, SERVICE_FILE_OP_READ, SERVICE_FILE_OP_SHIFT, SERVICE_FILE_OP_STAT,
        SERVICE_FILE_OP_WRITE, SERVICE_FILE_REQUEST_TAG, SERVICE_FILE_SEEK_OFFSET_MASK,
        SERVICE_FILE_SEEK_WHENCE_SHIFT, SERVICE_FILE_SIZE_OFFSET, SERVICE_FILE_WRITE_COUNT_SHIFT,
        SERVICE_MARKER_MASK, SERVICE_NAME_MASK, SERVICE_NS_CAP_TAG, SERVICE_SEG_TAG,
        SERVICE_SERVE_OP_ADVANCE, SERVICE_SERVE_OP_RESOLVE, SERVICE_SERVE_OP_SHIFT,
        SERVICE_SERVE_TAG, SERVICE_VFS_ACCESS_SHIFT, SERVICE_VFS_CHMOD_MODE_MASK,
        SERVICE_VFS_CHMOD_MODE_SHIFT, SERVICE_VFS_CHMOD_TAG, SERVICE_VFS_COMMIT_TAG,
        SERVICE_VFS_FIRST_BYTES_MASK, SERVICE_VFS_FLAG_APPEND, SERVICE_VFS_FLAG_CREATE,
        SERVICE_VFS_FLAG_DIRECTORY, SERVICE_VFS_FLAG_EXCLUSIVE, SERVICE_VFS_FLAG_STAT,
        SERVICE_VFS_FLAG_TRUNC, SERVICE_VFS_FLAGS_MASK, SERVICE_VFS_FLAGS_SHIFT,
        SERVICE_VFS_LINK_CHUNK, SERVICE_VFS_LINK_OFFSET_SHIFT, SERVICE_VFS_LINK_READ_TAG,
        SERVICE_VFS_LINK_REPLY_TAG, SERVICE_VFS_MUTATE_OP_MASK, SERVICE_VFS_MUTATE_OP_SHIFT,
        SERVICE_VFS_MUTATE_TAG, SERVICE_VFS_OP_MKDIR, SERVICE_VFS_OP_RMDIR, SERVICE_VFS_OP_UNLINK,
        SERVICE_VFS_OPEN_TAG, SERVICE_VFS_PAIR_COMMIT_TAG, SERVICE_VFS_PAIR_OP_MASK,
        SERVICE_VFS_PAIR_OP_READLINK, SERVICE_VFS_PAIR_OP_RENAME, SERVICE_VFS_PAIR_OP_SHIFT,
        SERVICE_VFS_PAIR_OP_SYMLINK, SERVICE_VFS_PAIR_SLOT_SHIFT, SERVICE_VFS_PAIR_TAG,
        SERVICE_VFS_PATH_BYTES_MASK, SERVICE_VFS_PATH_LEN_SHIFT, SERVICE_VFS_PATH_MAX,
        SERVICE_VFS_PATH_OFFSET_SHIFT, SERVICE_VFS_PATH_TAG, SERVICE_VFS_START_SHIFT,
        SERVICE_VFS_STATUS_CLIENT_SHIFT, SERVICE_VFS_STATUS_COUNT_MASK, SERVICE_VFS_STATUS_TAG,
        SERVICE_WAKE_TAG, pack_file_reply, pack_service_marker, pack_service_reply, pack_vfs_node,
        pack_vfs_pair_reply,
    },
    vfs::{self, AccessMode, HandleId, NodeKind, OpenFiles, OpenFlags, SeekFrom, Storage},
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
/// M8：VFS 镜像在持久化段内的起点。偏移 0..8 留给 SPEC-112 的状态字。
const SERVICE_VFS_BASE: u64 = SERVICE_FILE_SIZE_OFFSET;
/// M11：暂存操作码（0 保留给 open/stat；线上 0..2 映射为 1..3）。
const STAGED_MKDIR: u8 = SERVICE_VFS_OP_MKDIR as u8 + 1;
const STAGED_UNLINK: u8 = SERVICE_VFS_OP_UNLINK as u8 + 1;
const STAGED_RMDIR: u8 = SERVICE_VFS_OP_RMDIR as u8 + 1;
/// M13b：chmod/fchmod 的单路径暂存（0..3 已分配）。
const STAGED_CHMOD: u8 = 4;
/// M8：文件/目录服务端句柄的编号起点。
const SERVICE_FILE_FIRST_FD: u64 = 3;

/// M8：把持久化段实现为 [`Storage`]。偏移相对 VFS 镜像起点，不包含状态字。
struct SegmentStorage<'a, T: SyscallTransport> {
    runtime: &'a Runtime<T>,
}

impl<T: SyscallTransport> Storage for SegmentStorage<'_, T> {
    fn read(&mut self, offset: usize, out: &mut [u8]) -> bool {
        let address = SERVICE_DATA_VA + SERVICE_VFS_BASE + offset as u64;
        self.runtime.read_user_bytes(address, out).is_ok()
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> bool {
        let address = SERVICE_DATA_VA + SERVICE_VFS_BASE + offset as u64;
        self.runtime.write_user_bytes(address, data).is_ok()
    }
}

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
    // M4/M8：存储服务的文件/目录句柄表。服务端 fd 3..10 对应下标 0..7；close 立即释放并可复用。
    let mut files = OpenFiles::<SERVICE_FILE_HANDLES>::new();
    // M8：rootfs 是否已在该段内成功格式化/挂载。
    let mut vfs_ready = false;
    // M8：一次 `openat` 的分阶段路径暂存（OPEN / PATH* / COMMIT）。
    let mut staged = OpenStaging::EMPTY;
    // M13a：双路径/双字符串操作（rename/symlink/readlink）的暂存与最近一次链接目标。
    let mut pair = PairStaging::EMPTY;
    let mut link = LinkTarget::EMPTY;
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
                // M8：存储服务在自己的段上格式化 rootfs；日志服务继续只用 0..8 的状态字。
                if kind == SERVICE_BEHAVIOR_STORE {
                    let mut storage = SegmentStorage { runtime: &runtime };
                    vfs_ready = vfs::mount(&mut storage)
                        .and_then(|()| {
                            if files.is_empty() {
                                vfs::recover_open_files(&mut storage)
                            } else {
                                Ok(())
                            }
                        })
                        .is_ok();
                }
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
        } else if tag == SERVICE_VFS_OPEN_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M8/M9：open/stat 元数据。清空暂存路径并写入第一段字节。
            let flags =
                ((message.payload >> SERVICE_VFS_FLAGS_SHIFT) & SERVICE_VFS_FLAGS_MASK) as u8;
            let len = ((message.payload >> SERVICE_VFS_PATH_LEN_SHIFT) & 0xff) as usize;
            let start = ((message.payload >> SERVICE_VFS_START_SHIFT) & 0xff) as u8;
            let first = message.payload & SERVICE_VFS_FIRST_BYTES_MASK;
            if vfs_ready {
                pair.active = false;
                let _ = staged.begin(flags, len, start, first);
            } else {
                staged.active = false;
            }
        } else if tag == SERVICE_VFS_PATH_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            let offset = ((message.payload >> SERVICE_VFS_PATH_OFFSET_SHIFT) & 0xff) as usize;
            let bytes = message.payload & SERVICE_VFS_PATH_BYTES_MASK;
            if vfs_ready {
                if pair.active {
                    let _ = pair.append(offset, bytes);
                } else {
                    let _ = staged.append(offset, bytes);
                }
            } else {
                staged.active = false;
                pair.active = false;
            }
        } else if tag == SERVICE_VFS_COMMIT_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M8/M9：解析暂存路径。stat 只回元数据；open 成功时再分配服务端句柄。
            let reply = if vfs_ready {
                if staged.op != 0 {
                    mutate_staged(&runtime, &mut staged, message.payload)
                } else if staged.flags & SERVICE_VFS_FLAG_STAT as u8 != 0 {
                    stat_staged(&runtime, &mut staged)
                } else {
                    open_staged(&runtime, &mut staged, &mut files)
                }
            } else {
                pack_file_reply(SERVICE_FILE_OP_OPEN, 0, SERVICE_FILE_ENOSYS, 0)
            };
            let _ = runtime.send(peer, send_cap, reply, &[]);
        } else if tag == SERVICE_VFS_STATUS_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M12b：只读句柄计数，供服务管理器在 C 退出后核对清理结果。
            let client = (message.payload >> SERVICE_VFS_STATUS_CLIENT_SHIFT) & 0xff;
            let open = files.len() as u64;
            let reply = SERVICE_VFS_STATUS_TAG
                | (client << SERVICE_VFS_STATUS_CLIENT_SHIFT)
                | (open & SERVICE_VFS_STATUS_COUNT_MASK);
            let _ = runtime.send(peer, send_cap, reply, &[]);
        } else if tag == SERVICE_VFS_CHMOD_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M13b：chmod/fchmod 的路径暂存；mode 在 COMMIT 消息里携带。
            let len = ((message.payload >> SERVICE_VFS_PATH_LEN_SHIFT) & 0xff) as usize;
            let start = ((message.payload >> SERVICE_VFS_START_SHIFT) & 0xff) as u8;
            let first = message.payload & SERVICE_VFS_FIRST_BYTES_MASK;
            if vfs_ready {
                pair.active = false;
                let _ = staged.begin_chmod(len, start, first);
            } else {
                staged.active = false;
            }
        } else if tag == SERVICE_VFS_MUTATE_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M11：mkdir/unlink/rmdir 复用 OPEN/PATH/COMMIT 的分段路径传输。
            let op = ((message.payload >> SERVICE_VFS_MUTATE_OP_SHIFT) & SERVICE_VFS_MUTATE_OP_MASK)
                as u8;
            let len = ((message.payload >> SERVICE_VFS_PATH_LEN_SHIFT) & 0xff) as usize;
            let start = ((message.payload >> SERVICE_VFS_START_SHIFT) & 0xff) as u8;
            let first = message.payload & SERVICE_VFS_FIRST_BYTES_MASK;
            if vfs_ready {
                pair.active = false;
                let _ = staged.begin_mutate(op + 1, len, start, first);
            } else {
                staged.active = false;
            }
        } else if tag == SERVICE_VFS_PAIR_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M13a：双字符串操作的 OPEN（槽 0/1 的前 3 字节）。
            let op =
                ((message.payload >> SERVICE_VFS_PAIR_OP_SHIFT) & SERVICE_VFS_PAIR_OP_MASK) as u8;
            let slot = ((message.payload >> SERVICE_VFS_PAIR_SLOT_SHIFT) & 1) as u8;
            let len = ((message.payload >> SERVICE_VFS_PATH_LEN_SHIFT) & 0xff) as usize;
            let first = message.payload & SERVICE_VFS_FIRST_BYTES_MASK;
            if vfs_ready && pair.begin(op, slot, len, first) {
                staged.active = false;
            } else {
                pair.active = false;
            }
        } else if tag == SERVICE_VFS_PAIR_COMMIT_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M13a：槽 0 的 COMMIT 只确认暂存；readlink 或槽 1 的 COMMIT 执行操作。
            let op =
                ((message.payload >> SERVICE_VFS_PAIR_OP_SHIFT) & SERVICE_VFS_PAIR_OP_MASK) as u8;
            let slot = ((message.payload >> SERVICE_VFS_PAIR_SLOT_SHIFT) & 1) as u8;
            let reply = if vfs_ready && pair.active && pair.op == op {
                if slot == 0 && op != SERVICE_VFS_PAIR_OP_READLINK as u8 {
                    if pair.ready(0) {
                        pack_vfs_pair_reply(op as u64, 0, 0)
                    } else {
                        pair.active = false;
                        pack_vfs_pair_reply(op as u64, SERVICE_FILE_EINVAL, 0)
                    }
                } else {
                    pair_staged(&runtime, &mut pair, &mut link)
                }
            } else {
                pair.active = false;
                pack_vfs_pair_reply(op as u64, SERVICE_FILE_EINVAL, 0)
            };
            let _ = runtime.send(peer, send_cap, reply, &[]);
        } else if tag == SERVICE_VFS_LINK_READ_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M13a：readlink 目标按 3 字节分段取回。
            let offset = ((message.payload >> SERVICE_VFS_LINK_OFFSET_SHIFT) & 0xff) as usize;
            let mut status = 0u64;
            let mut bytes = 0u64;
            if offset > link.len as usize {
                status = SERVICE_FILE_EINVAL;
            } else {
                let count = (link.len as usize - offset).min(SERVICE_VFS_LINK_CHUNK);
                for index in 0..count {
                    bytes |= (link.bytes[offset + index] as u64) << (8 * index);
                }
            }
            let reply =
                SERVICE_VFS_LINK_REPLY_TAG | (status << SERVICE_VFS_LINK_OFFSET_SHIFT) | bytes;
            let _ = runtime.send(peer, send_cap, reply, &[]);
        } else if tag == SERVICE_FILE_REQUEST_TAG >> 48 && kind == SERVICE_BEHAVIOR_STORE {
            // M4/M8：读/写/lseek/close 只在存储服务、且持久化段已映射后处理；否则明确拒绝。
            let response = if vfs_ready {
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

/// M8：一次 open 的分阶段状态。路径经 `OPEN`（前 3 字节）+ `PATH`（随后每段 5 字节）装入。
#[derive(Clone, Copy)]
struct OpenStaging {
    active: bool,
    /// 0 = open/stat；1 = mkdir；2 = unlink；3 = rmdir（M11）；4 = chmod（M13b）。
    op: u8,
    flags: u8,
    len: u8,
    got: u8,
    start: u8,
    path: [u8; SERVICE_VFS_PATH_MAX],
}

impl OpenStaging {
    const EMPTY: Self = Self {
        active: false,
        op: 0,
        flags: 0,
        len: 0,
        got: 0,
        start: 0,
        path: [0u8; SERVICE_VFS_PATH_MAX],
    };

    fn begin(&mut self, flags: u8, len: usize, start: u8, first: u64) -> u64 {
        // M9：STAT 允许空路径，表示“直接查询 start 节点”（fstat 用）。
        let stat_only = flags & SERVICE_VFS_FLAG_STAT as u8 != 0;
        if (len == 0 && !stat_only) || len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        self.active = true;
        self.op = 0;
        self.flags = flags;
        self.len = len as u8;
        self.got = len.min(3) as u8;
        self.start = start;
        self.path = [0u8; SERVICE_VFS_PATH_MAX];
        for (index, byte) in self.path.iter_mut().take(self.got as usize).enumerate() {
            *byte = ((first >> (8 * index)) & 0xff) as u8;
        }
        0
    }

    /// M11：开始一次单路径变更（mkdir/unlink/rmdir）；空路径一律拒绝。
    /// M13b：开始一次 chmod/fchmod；空路径合法（改 `start` 节点），故不复用
    /// `begin` 的“非 stat 必须非空路径”检查。
    fn begin_chmod(&mut self, len: usize, start: u8, first: u64) -> u64 {
        if len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        self.active = true;
        self.op = STAGED_CHMOD;
        self.flags = 0;
        self.len = len as u8;
        self.got = len.min(3) as u8;
        self.start = start;
        self.path = [0u8; SERVICE_VFS_PATH_MAX];
        for index in 0..self.got as usize {
            self.path[index] = ((first >> (8 * index)) & 0xff) as u8;
        }
        0
    }

    fn begin_mutate(&mut self, op: u8, len: usize, start: u8, first: u64) -> u64 {
        if len == 0 || len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        let status = self.begin(0, len, start, first);
        if status == 0 {
            self.op = op;
        }
        status
    }

    fn append(&mut self, offset: usize, bytes: u64) -> u64 {
        if !self.active
            || offset > self.len as usize
            || offset + 5 < self.got as usize
            || offset > self.got as usize
        {
            return SERVICE_FILE_EINVAL;
        }
        let count = (self.len as usize - offset).min(5);
        for (index, byte) in self.path[offset..offset + count].iter_mut().enumerate() {
            *byte = ((bytes >> (8 * index)) & 0xff) as u8;
        }
        self.got = (offset + count) as u8;
        0
    }
}

/// M13a：rename/symlink 的双字符串暂存，以及 readlink 最近一次目标。
///
/// 槽 0 与槽 1 各自按 `OPEN`（前 3 字节）+ `PATH`（随后每段 5 字节）填充；A 的
/// COMMIT 只确认，B 的 COMMIT（readlink 是 A）执行操作。两个槽都只保存字节，
/// 目标字符串不要求是合法路径。
#[derive(Clone, Copy)]
struct PairStaging {
    active: bool,
    op: u8,
    slot: u8,
    lens: [u8; 2],
    got: [u8; 2],
    paths: [[u8; SERVICE_VFS_PATH_MAX]; 2],
}

impl PairStaging {
    const EMPTY: Self = Self {
        active: false,
        op: 0,
        slot: 0,
        lens: [0; 2],
        got: [0; 2],
        paths: [[0u8; SERVICE_VFS_PATH_MAX]; 2],
    };

    fn begin(&mut self, op: u8, slot: u8, len: usize, first: u64) -> bool {
        if slot > 1
            || op > SERVICE_VFS_PAIR_OP_READLINK as u8
            || len == 0
            || len > SERVICE_VFS_PATH_MAX
        {
            self.active = false;
            return false;
        }
        self.active = true;
        self.op = op;
        self.slot = slot;
        self.lens[slot as usize] = len as u8;
        self.got[slot as usize] = len.min(3) as u8;
        self.paths[slot as usize] = [0u8; SERVICE_VFS_PATH_MAX];
        for index in 0..self.got[slot as usize] as usize {
            self.paths[slot as usize][index] = ((first >> (8 * index)) & 0xff) as u8;
        }
        true
    }

    fn append(&mut self, offset: usize, bytes: u64) -> bool {
        if !self.active {
            return false;
        }
        let slot = self.slot as usize;
        let len = self.lens[slot] as usize;
        let got = self.got[slot] as usize;
        if offset != got || offset > len {
            return false;
        }
        let count = (len - offset).min(5);
        for index in 0..count {
            self.paths[slot][offset + index] = ((bytes >> (8 * index)) & 0xff) as u8;
        }
        self.got[slot] = (offset + count) as u8;
        true
    }

    fn ready(&self, slot: usize) -> bool {
        self.lens[slot] != 0 && self.got[slot] == self.lens[slot]
    }
}

/// M13a：readlink 最近一次目标，供后续 3 字节分段读取。
#[derive(Clone, Copy)]
struct LinkTarget {
    len: u8,
    bytes: [u8; SERVICE_VFS_PATH_MAX],
}

impl LinkTarget {
    const EMPTY: Self = Self {
        len: 0,
        bytes: [0u8; SERVICE_VFS_PATH_MAX],
    };
}

fn open_flags(bits: u8) -> Result<OpenFlags, vfs::VfsError> {
    let bits = bits as u64;
    let access = match (bits >> SERVICE_VFS_ACCESS_SHIFT) & 3 {
        0 => AccessMode::ReadOnly,
        1 => AccessMode::WriteOnly,
        2 => AccessMode::ReadWrite,
        _ => return Err(vfs::VfsError::Invalid),
    };
    Ok(OpenFlags {
        access,
        create: bits & SERVICE_VFS_FLAG_CREATE != 0,
        exclusive: bits & SERVICE_VFS_FLAG_EXCLUSIVE != 0,
        truncate: bits & SERVICE_VFS_FLAG_TRUNC != 0,
        append: bits & SERVICE_VFS_FLAG_APPEND != 0,
        directory: bits & SERVICE_VFS_FLAG_DIRECTORY != 0,
    })
}

fn vfs_status(error: vfs::VfsError) -> u64 {
    match error {
        vfs::VfsError::NoEnt => SERVICE_FILE_ENOENT,
        vfs::VfsError::NotDir => SERVICE_FILE_ENOTDIR,
        vfs::VfsError::Access => SERVICE_FILE_EACCES,
        vfs::VfsError::Exist => SERVICE_FILE_EEXIST,
        vfs::VfsError::Invalid => SERVICE_FILE_EINVAL,
        vfs::VfsError::NoSpace => SERVICE_FILE_ENOSPC,
        vfs::VfsError::Unformatted => SERVICE_FILE_ENOSYS,
        vfs::VfsError::Io => SERVICE_FILE_EFAULT,
        vfs::VfsError::Loop => SERVICE_FILE_ELOOP,
        vfs::VfsError::BadHandle => SERVICE_FILE_EBADF,
        vfs::VfsError::TooManyOpen => SERVICE_FILE_EAGAIN,
    }
}

/// 执行树变更；文件打开引用与目录项生命周期由 VFS 分别维护。
fn mutate_staged<T: SyscallTransport>(
    runtime: &Runtime<T>,
    staged: &mut OpenStaging,
    commit: u64,
) -> u64 {
    let reply = |status: u64| pack_file_reply(SERVICE_FILE_OP_OPEN, 0, status, 0);
    if !staged.active || staged.got != staged.len {
        staged.active = false;
        return reply(SERVICE_FILE_EINVAL);
    }
    let op = staged.op;
    let start = staged.start;
    let len = staged.len as usize;
    staged.active = false;
    let mut storage = SegmentStorage { runtime };
    let path = &staged.path[..len];
    if op == STAGED_CHMOD {
        let mode = (commit >> SERVICE_VFS_CHMOD_MODE_SHIFT) & SERVICE_VFS_CHMOD_MODE_MASK;
        return match vfs::chmod(&mut storage, start, path, mode as u16) {
            Ok(()) => reply(0),
            Err(error) => reply(vfs_status(error)),
        };
    }
    match op {
        STAGED_MKDIR => match vfs::mkdir(&mut storage, start, path) {
            Ok(_) => reply(0),
            Err(error) => reply(vfs_status(error)),
        },
        STAGED_UNLINK | STAGED_RMDIR => {
            match vfs::unlink(&mut storage, start, path, op == STAGED_RMDIR) {
                Ok(_) => reply(0),
                Err(error) => reply(vfs_status(error)),
            }
        }
        _ => reply(SERVICE_FILE_EINVAL),
    }
}

/// M8：把暂存路径交给 VFS 解析，并为成功结果分配一个服务端句柄。
fn open_staged<T: SyscallTransport>(
    runtime: &Runtime<T>,
    staged: &mut OpenStaging,
    files: &mut OpenFiles<SERVICE_FILE_HANDLES>,
) -> u64 {
    let reply = |status: u64| pack_file_reply(SERVICE_FILE_OP_OPEN, 0, status, 0);
    if !staged.active || staged.got != staged.len {
        staged.active = false;
        return reply(SERVICE_FILE_EINVAL);
    }
    let flags = match open_flags(staged.flags) {
        Ok(flags) => flags,
        Err(error) => {
            staged.active = false;
            return reply(vfs_status(error));
        }
    };
    let start = staged.start;
    let len = staged.len as usize;
    staged.active = false;
    let mut storage = SegmentStorage { runtime };
    let (handle, info) = match files.open(&mut storage, start, &staged.path[..len], flags) {
        Ok(opened) => opened,
        Err(error) => return reply(vfs_status(error)),
    };
    let fd = handle.0 as u64 + SERVICE_FILE_FIRST_FD;
    pack_file_reply(
        SERVICE_FILE_OP_OPEN,
        fd,
        0,
        pack_vfs_node(
            info.id as u64,
            info.kind.as_raw() as u64,
            info.size,
            info.mode,
        ),
    )
}

/// M9：只解析暂存路径并回复元数据，不分配句柄、不做 access 检查。
fn stat_staged<T: SyscallTransport>(runtime: &Runtime<T>, staged: &mut OpenStaging) -> u64 {
    let reply = |status: u64| pack_file_reply(SERVICE_FILE_OP_STAT, 0, status, 0);
    if !staged.active || staged.got != staged.len {
        staged.active = false;
        return reply(SERVICE_FILE_EINVAL);
    }
    let start = staged.start;
    let len = staged.len as usize;
    staged.active = false;
    let mut storage = SegmentStorage { runtime };
    let id = if len == 0 {
        start
    } else {
        match vfs::resolve(&mut storage, start, &staged.path[..len]) {
            Ok(id) => id,
            Err(error) => return reply(vfs_status(error)),
        }
    };
    let info = match vfs::metadata(&mut storage, id) {
        Ok(node) => node,
        Err(error) => return reply(vfs_status(error)),
    };
    pack_file_reply(
        SERVICE_FILE_OP_STAT,
        0,
        0,
        pack_vfs_node(id as u64, info.kind.as_raw() as u64, info.size, info.mode),
    )
}

/// M13a：执行一次双路径操作（rename/symlink/readlink），返回 `PAIR_REPLY`。
///
/// 调用方已确认 VFS 可用、暂存态与 `op` 一致。`readlink` 的目标写入 `link`，供
/// 后续分段读取；失败时清空暂存态，避免半条请求留在服务里。
fn pair_staged<T: SyscallTransport>(
    runtime: &Runtime<T>,
    pair: &mut PairStaging,
    link: &mut LinkTarget,
) -> u64 {
    let op = pair.op as u64;
    let reply = |status: u64, data: u64| pack_vfs_pair_reply(op, status, data);
    if !pair.ready(0) {
        pair.active = false;
        return reply(SERVICE_FILE_EINVAL, 0);
    }
    let mut storage = SegmentStorage { runtime };
    let first_len = pair.lens[0] as usize;
    if op == SERVICE_VFS_PAIR_OP_READLINK {
        let id = match vfs::resolve_link(&mut storage, vfs::ROOT, &pair.paths[0][..first_len]) {
            Ok(id) => id,
            Err(error) => {
                pair.active = false;
                return reply(vfs_status(error), 0);
            }
        };
        let node = match vfs::read_node(&mut storage, id) {
            Ok(node) => node,
            Err(error) => {
                pair.active = false;
                return reply(vfs_status(error), 0);
            }
        };
        if node.kind != NodeKind::Symlink {
            pair.active = false;
            return reply(SERVICE_FILE_EINVAL, 0);
        }
        let target = match vfs::read_link(&mut storage, &node) {
            Ok(target) => target,
            Err(error) => {
                pair.active = false;
                return reply(vfs_status(error), 0);
            }
        };
        let len = node.size as usize;
        link.bytes = [0u8; SERVICE_VFS_PATH_MAX];
        link.bytes[..len].copy_from_slice(&target[..len]);
        link.len = len as u8;
        pair.active = false;
        return reply(0, len as u64);
    }
    if !pair.ready(1) {
        pair.active = false;
        return reply(SERVICE_FILE_EINVAL, 0);
    }
    let second_len = pair.lens[1] as usize;
    let status = match op {
        SERVICE_VFS_PAIR_OP_RENAME => vfs::rename(
            &mut storage,
            vfs::ROOT,
            &pair.paths[0][..first_len],
            &pair.paths[1][..second_len],
        )
        .map_or_else(vfs_status, |_| 0),
        SERVICE_VFS_PAIR_OP_SYMLINK => vfs::symlink(
            &mut storage,
            vfs::ROOT,
            &pair.paths[1][..second_len],
            &pair.paths[0][..first_len],
        )
        .map_or_else(vfs_status, |_| 0),
        _ => SERVICE_FILE_EINVAL,
    };
    pair.active = false;
    reply(status, 0)
}

/// 线协议编解码边界；打开对象语义在可 host 测试的 OpenFiles 中。
fn file_request<T: SyscallTransport>(
    runtime: &Runtime<T>,
    files: &mut OpenFiles<SERVICE_FILE_HANDLES>,
    payload: u64,
) -> u64 {
    let op = (payload >> SERVICE_FILE_OP_SHIFT) & 0xf;
    let fd = (payload >> SERVICE_FILE_FD_SHIFT) & 0xf;
    let arg = payload & SERVICE_FILE_ARG_MASK;
    let reply = |status: u64, data: u64| pack_file_reply(op, fd, status, data);
    let Some(index) = fd.checked_sub(SERVICE_FILE_FIRST_FD) else {
        return reply(SERVICE_FILE_EBADF, 0);
    };
    let id = HandleId(index as usize);
    let mut storage = SegmentStorage { runtime };
    let result = match op {
        SERVICE_FILE_OP_READ => {
            let cap = ((arg & 0xff) as usize).min(SERVICE_FILE_CHUNK as usize);
            let mut bytes = [0u8; SERVICE_FILE_CHUNK as usize];
            let count = match files.info(&mut storage, id) {
                Ok(info) if info.kind == NodeKind::Directory => {
                    files.read_directory_bytes(&mut storage, id, &mut bytes[..cap])
                }
                Ok(_) => files.read(&mut storage, id, &mut bytes[..cap]),
                Err(error) => Err(error),
            };
            count.map(|count| {
                let mut data = 0u64;
                for (index, byte) in bytes[..count].iter().enumerate() {
                    data |= (*byte as u64) << (8 * index);
                }
                ((count as u64) << SERVICE_FILE_WRITE_COUNT_SHIFT) | data
            })
        }
        SERVICE_FILE_OP_WRITE => {
            let count = (((arg >> SERVICE_FILE_WRITE_COUNT_SHIFT) & 0xff) as usize)
                .min(SERVICE_FILE_CHUNK as usize);
            let mut bytes = [0u8; SERVICE_FILE_CHUNK as usize];
            for (index, byte) in bytes[..count].iter_mut().enumerate() {
                *byte = ((arg >> (8 * index)) & 0xff) as u8;
            }
            files
                .write(&mut storage, id, &bytes[..count])
                .map(|count| count as u64)
        }
        SERVICE_FILE_OP_LSEEK => {
            let raw = (arg & SERVICE_FILE_SEEK_OFFSET_MASK) as i64;
            let offset = if raw & 0x2000_0000 != 0 {
                raw - 0x4000_0000
            } else {
                raw
            };
            let from = match (arg >> SERVICE_FILE_SEEK_WHENCE_SHIFT) & 3 {
                0 => SeekFrom::Start(offset),
                1 => SeekFrom::Current(offset),
                2 => SeekFrom::End(offset),
                _ => return reply(SERVICE_FILE_EINVAL, 0),
            };
            files.seek(&mut storage, id, from)
        }
        SERVICE_FILE_OP_CLOSE => files.close(&mut storage, id).map(|()| 0),
        _ => Err(vfs::VfsError::Invalid),
    };
    match result {
        Ok(data) => reply(0, data),
        Err(error) => reply(vfs_status(error), 0),
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
