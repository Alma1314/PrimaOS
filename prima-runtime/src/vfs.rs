//! 阶段 5 / M8：**用户态 VFS / rootfs 的纯逻辑模型**。
//!
//! 本模块不接触任何 syscall 或 Capability：它把一段字节存储解释成一个只含目录与普通文件的
//! 小根文件系统。存储由 [`Storage`] 提供，ring3 存储服务用已映射的 MemorySegment 实现它，
//! host 单测用内存数组实现它；同一份解析/打开/读写/目录逻辑在两端得到相同结果。
//!
//! 单节点名称上限 16 字节、路径分量上限 6——足够覆盖首版 rootfs 与 `/usr/lib/version`
//! 这类多级路径，同时把解析栈帧压到服务端点的 1 页栈预算内。
//!
//! 镜像布局（从存储偏移 0 起）：
//!
//! ```text
//! 0..16    header：magic("PVFS") | version | node_count | data_used
//! 16..528  16 个 32 字节的节点条目（名称、父节点、类型、大小、内容偏移）
//! 528..    文件内容区（总容量 3560 字节）
//! ```
//!
//! 节点 0 固定为根目录。路径解析在用户态完成，支持绝对/相对路径、`.`、`..`、重复 `/`、
//! 尾随 `/` 与 `openat(dirfd, ...)` 的起始目录；错误只用 profile v0 已冻结的 `ENOENT`、
//! `ENOTDIR`、`EACCES`、`EEXIST`、`EINVAL`、`ENOSPC` 语义，未引入 `ELOOP`/`EISDIR` 等新码。
//!
//! **边界**：不实现符号链接、`rename`/`unlink`/`mkdir`、持久化、并发写锁或文件映射；目录
//! 创建只发生在静态根树中，`O_CREAT` 只能在可写目录（当前为 `/tmp`）里新建普通文件。

/// 单个节点的字节数。
/// M13b：节点 32 → 36 B，新增 16 位 mode。
pub const NODE_BYTES: usize = 36;
/// 节点条目上限（同时决定内容区起点，保证已分配的内容偏移稳定）。
/// M13a：节点槽 16 → 32。符号链接/rename 用例在 16 槽下会因临时节点耗尽返回
/// `ENOSPC`；扩到 32 后内容区仍 ≥3 KiB（`CONTENT_CAPACITY` 按 32×32 B 节点表重算）。
pub const MAX_NODES: usize = 32;
/// 单个名称的字节上限。
pub const MAX_NAME: usize = 16;
/// 镜像头字节数。
pub const HEADER_BYTES: usize = 16;
/// 节点表起点。
pub const NODES_OFFSET: usize = HEADER_BYTES;
/// 文件内容区起点。
pub const CONTENT_OFFSET: usize = HEADER_BYTES + MAX_NODES * NODE_BYTES;
/// 一次 VFS 请求可携带的路径字节上限。
pub const MAX_PATH: usize = 64;
/// 路径分量上限。
pub const MAX_COMPONENTS: usize = 6;
/// 镜像 magic：ASCII `PVFS`。
pub const MAGIC: u32 = u32::from_le_bytes(*b"PVFS");
/// 镜像版本。
/// M13b：节点布局 v2（mode 字段）；v1 镜像确定拒绝，存储服务每启动重新格式化。
pub const VERSION: u16 = 2;

/// 根节点编号。
pub const ROOT: NodeId = 0;

/// 已删除节点的 `parent` 哨兵。`MAX_NODES` 远小于 255，故不会与真实父节点冲突。
/// 删除只标记、不清零 `kind/data`：未关闭句柄在槽位被复用前仍可读到旧内容；目录
/// 查找与枚举一律跳过该节点。
pub const FREE_PARENT: NodeId = 0xff;

/// 节点是否已被删除（槽位可复用）。
fn is_free(node: &Node) -> bool {
    node.parent == FREE_PARENT
}

/// 节点编号（0..MAX_NODES）。
pub type NodeId = u8;

/// 节点类型。数值是线上/镜像编码，不直接等于 Linux `d_type`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeKind {
    Directory = 0,
    File = 1,
    /// `/dev/null` 一类字符设备占位：读总是 EOF，写被丢弃。
    Null = 2,
    /// M13a：符号链接；目标字节保存在内容区（`data`/`size`），不占节点内联空间。
    Symlink = 3,
}

impl NodeKind {
    /// 从线上编码解析；未知编码确定失败。
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Directory),
            1 => Some(Self::File),
            2 => Some(Self::Null),
            3 => Some(Self::Symlink),
            _ => None,
        }
    }

    /// 线上编码。
    pub const fn as_raw(self) -> u8 {
        self as u8
    }
}

/// VFS 逻辑错误；存储服务把它们映射到冻结 errno。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VfsError {
    /// `ENOENT`。
    NoEnt,
    /// `ENOTDIR`。
    NotDir,
    /// `EACCES`。
    Access,
    /// `EEXIST`。
    Exist,
    /// `EINVAL`。
    Invalid,
    /// `ENOSPC`。
    NoSpace,
    /// 镜像未格式化/损坏。
    Unformatted,
    /// 调用方提供的存储读写失败。
    Io,
    /// `ELOOP`：符号链接展开超过冻结深度。
    Loop,
}

/// 字节存储抽象。所有偏移都相对镜像起点（不含 SPEC-112 的状态字）。
pub trait Storage {
    /// 读 `out.len()` 字节；越界/不可读返回 `false`。
    fn read(&mut self, offset: usize, out: &mut [u8]) -> bool;
    /// 写 `data`；越界/不可写返回 `false`。
    fn write(&mut self, offset: usize, data: &[u8]) -> bool;
}

/// 一个 VFS 节点。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: NodeId,
    /// POSIX 权限位（低 12 位）。单用户模型只看 owner 读写位。
    pub mode: u16,
    pub size: u16,
    /// 内容区内的起始偏移（仅普通文件有意义）。
    pub data: u16,
    pub name_len: u8,
    pub name: [u8; MAX_NAME],
}

impl Node {
    /// 节点名称字节。
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }

    /// owner 写位；阶段 5 的单用户权限模型只用这一位做写检查。
    pub fn writable(&self) -> bool {
        self.mode & 0o200 != 0
    }

    /// owner 读位。
    pub fn readable(&self) -> bool {
        self.mode & 0o400 != 0
    }
}

/// 目录枚举的一条记录。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub node: NodeId,
    pub kind: NodeKind,
    pub name_len: u8,
    pub name: [u8; MAX_NAME],
}

impl DirEntry {
    /// 名称字节。
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }
}

/// `open` 的规范化 flag 子集。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenFlags {
    /// 0=只读、1=只写、2=读写。
    pub access: u8,
    pub create: bool,
    pub exclusive: bool,
    pub truncate: bool,
    pub append: bool,
    pub directory: bool,
}

/// 一次成功 open 的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opened {
    pub id: NodeId,
    pub node: Node,
}

#[derive(Clone, Copy)]
struct Header {
    count: u8,
    data_used: u16,
}

impl Header {
    fn write<S: Storage>(self, storage: &mut S) -> bool {
        let mut bytes = [0u8; HEADER_BYTES];
        bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        bytes[4..6].copy_from_slice(&VERSION.to_le_bytes());
        bytes[6] = self.count;
        bytes[8..10].copy_from_slice(&self.data_used.to_le_bytes());
        storage.write(0, &bytes)
    }
}

fn read_exact<S: Storage>(storage: &mut S, offset: usize, out: &mut [u8]) -> Result<(), VfsError> {
    if storage.read(offset, out) {
        Ok(())
    } else {
        Err(VfsError::Io)
    }
}

fn write_all<S: Storage>(storage: &mut S, offset: usize, data: &[u8]) -> Result<(), VfsError> {
    if storage.write(offset, data) {
        Ok(())
    } else {
        Err(VfsError::Io)
    }
}

fn read_header<S: Storage>(storage: &mut S) -> Result<Header, VfsError> {
    let mut bytes = [0u8; HEADER_BYTES];
    read_exact(storage, 0, &mut bytes)?;
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != MAGIC {
        return Err(VfsError::Unformatted);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(VfsError::Unformatted);
    }
    let count = bytes[6];
    let data_used = u16::from_le_bytes([bytes[8], bytes[9]]);
    if count as usize > MAX_NODES || data_used as usize > CONTENT_CAPACITY {
        return Err(VfsError::Unformatted);
    }
    Ok(Header { count, data_used })
}

fn encode_node(node: &Node) -> [u8; NODE_BYTES] {
    let mut bytes = [0u8; NODE_BYTES];
    bytes[0] = node.name_len;
    bytes[1] = node.kind.as_raw();
    bytes[2] = node.parent;
    bytes[4..6].copy_from_slice(&node.size.to_le_bytes());
    bytes[6..8].copy_from_slice(&node.data.to_le_bytes());
    bytes[8..10].copy_from_slice(&node.mode.to_le_bytes());
    bytes[10..10 + node.name_len as usize].copy_from_slice(node.name_bytes());
    bytes
}

fn decode_node(bytes: &[u8; NODE_BYTES]) -> Result<Node, VfsError> {
    let name_len = bytes[0];
    if name_len as usize > MAX_NAME {
        return Err(VfsError::Unformatted);
    }
    let kind = NodeKind::from_raw(bytes[1]).ok_or(VfsError::Unformatted)?;
    let mut name = [0u8; MAX_NAME];
    name[..name_len as usize].copy_from_slice(&bytes[10..10 + name_len as usize]);
    Ok(Node {
        kind,
        parent: bytes[2],
        mode: u16::from_le_bytes([bytes[8], bytes[9]]),
        size: u16::from_le_bytes([bytes[4], bytes[5]]),
        data: u16::from_le_bytes([bytes[6], bytes[7]]),
        name_len,
        name,
    })
}

fn store_node<S: Storage>(storage: &mut S, id: NodeId, node: &Node) -> Result<(), VfsError> {
    let offset = NODES_OFFSET + id as usize * NODE_BYTES;
    write_all(storage, offset, &encode_node(node))
}

/// 读节点；`id` 越界或节点头损坏都确定性失败。
pub fn read_node<S: Storage>(storage: &mut S, id: NodeId) -> Result<Node, VfsError> {
    let header = read_header(storage)?;
    if id as usize >= header.count as usize {
        return Err(VfsError::NoEnt);
    }
    let mut bytes = [0u8; NODE_BYTES];
    read_exact(storage, NODES_OFFSET + id as usize * NODE_BYTES, &mut bytes)?;
    decode_node(&bytes)
}

fn write_node<S: Storage>(storage: &mut S, id: NodeId, node: &Node) -> Result<(), VfsError> {
    let header = read_header(storage)?;
    if id as usize >= header.count as usize {
        return Err(VfsError::NoEnt);
    }
    store_node(storage, id, node)
}

/// 初始化用的一条静态节点。
/// M17c-4：rootfs 内的最小静态 ELF（以 argc 为退出码），供适配层 execve 探针使用。
const TINY_ELF: &[u8] = &[
    0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x3e, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x12, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x48, 0x8b, 0x04, 0x24, 0x48, 0x89, 0xc7, 0x48, 0xc7, 0xc0, 0x36, 0x00, 0x00, 0x00, 0x0f, 0x05,
    0xeb, 0xfe,
];

struct InitialNode {
    name: &'static [u8],
    parent: NodeId,
    kind: NodeKind,
    mode: u16,
    content: &'static [u8],
}

/// 静态根文件系统。顺序必须保持：父节点先于子节点，节点 0 固定为根。
const INITIAL: [InitialNode; 13] = [
    InitialNode {
        name: b"",
        parent: ROOT,
        kind: NodeKind::Directory,
        mode: 0o555,
        content: b"",
    },
    InitialNode {
        name: b"f",
        parent: ROOT,
        kind: NodeKind::File,
        mode: 0o666,
        content: b"",
    },
    InitialNode {
        name: b"README",
        parent: ROOT,
        kind: NodeKind::File,
        mode: 0o444,
        content: b"prima rootfs\n",
    },
    InitialNode {
        name: b"etc",
        parent: ROOT,
        kind: NodeKind::Directory,
        mode: 0o555,
        content: b"",
    },
    InitialNode {
        name: b"motd",
        parent: 3,
        kind: NodeKind::File,
        mode: 0o444,
        content: b"welcome\n",
    },
    InitialNode {
        name: b"hostname",
        parent: 3,
        kind: NodeKind::File,
        mode: 0o444,
        content: b"prima\n",
    },
    InitialNode {
        name: b"usr",
        parent: ROOT,
        kind: NodeKind::Directory,
        mode: 0o555,
        content: b"",
    },
    InitialNode {
        name: b"lib",
        parent: 6,
        kind: NodeKind::Directory,
        mode: 0o555,
        content: b"",
    },
    InitialNode {
        name: b"version",
        parent: 7,
        kind: NodeKind::File,
        mode: 0o444,
        content: b"1.0\n",
    },
    InitialNode {
        name: b"tmp",
        parent: ROOT,
        kind: NodeKind::Directory,
        mode: 0o777,
        content: b"",
    },
    InitialNode {
        name: b"dev",
        parent: ROOT,
        kind: NodeKind::Directory,
        mode: 0o555,
        content: b"",
    },
    InitialNode {
        name: b"null",
        parent: 10,
        kind: NodeKind::Null,
        mode: 0o666,
        content: b"",
    },
    InitialNode {
        name: b"tiny.elf",
        kind: NodeKind::File,
        parent: ROOT,
        mode: 0o755,
        content: TINY_ELF,
    },
];

/// 文件内容区容量。
pub const CONTENT_CAPACITY: usize = 4096 - 8 - CONTENT_OFFSET;

/// 格式化存储为首版根文件系统。已格式化且节点数一致的镜像保持不变（幂等）。
pub fn mount<S: Storage>(storage: &mut S) -> Result<(), VfsError> {
    if read_header(storage).is_ok() {
        return Ok(());
    }
    // 先写 magic 为零的头，格式化完成后再写回；中途失败不会留下一个可被误认的镜像。
    let blank = [0u8; HEADER_BYTES];
    write_all(storage, 0, &blank)?;
    let zero = [0u8; NODE_BYTES];
    for index in 0..MAX_NODES {
        write_all(storage, NODES_OFFSET + index * NODE_BYTES, &zero)?;
    }
    let mut data_used = 0usize;
    for (id, initial) in INITIAL.iter().enumerate() {
        if initial.name.len() > MAX_NAME {
            return Err(VfsError::Invalid);
        }
        let mut node = Node {
            kind: initial.kind,
            parent: initial.parent,
            mode: initial.mode,
            size: 0,
            data: 0,
            name_len: initial.name.len() as u8,
            name: [0u8; MAX_NAME],
        };
        node.name[..initial.name.len()].copy_from_slice(initial.name);
        if initial.kind == NodeKind::File && !initial.content.is_empty() {
            if data_used + initial.content.len() > CONTENT_CAPACITY {
                return Err(VfsError::NoSpace);
            }
            node.data = data_used as u16;
            node.size = initial.content.len() as u16;
            write_all(storage, CONTENT_OFFSET + data_used, initial.content)?;
            data_used += initial.content.len();
        }
        store_node(storage, id as NodeId, &node)?;
    }
    if !(Header {
        count: INITIAL.len() as u8,
        data_used: data_used as u16,
    })
    .write(storage)
    {
        return Err(VfsError::Io);
    }
    Ok(())
}

fn header_data_used<S: Storage>(storage: &mut S) -> Result<u16, VfsError> {
    Ok(read_header(storage)?.data_used)
}

#[derive(Clone, Copy)]
struct Component {
    len: u8,
    bytes: [u8; MAX_NAME],
}

impl Component {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    fn is_dot(&self) -> bool {
        self.as_bytes() == b"."
    }

    fn is_dot_dot(&self) -> bool {
        self.as_bytes() == b".."
    }
}

struct ParsedPath {
    count: u8,
    components: [Component; MAX_COMPONENTS],
    absolute: bool,
    trailing_slash: bool,
}

impl ParsedPath {
    fn parse(path: &[u8]) -> Result<Self, VfsError> {
        if path.is_empty() {
            return Err(VfsError::Invalid);
        }
        let absolute = path[0] == b'/';
        let trailing_slash = path.last() == Some(&b'/');
        let mut parsed = Self {
            count: 0,
            components: [Component {
                len: 0,
                bytes: [0u8; MAX_NAME],
            }; MAX_COMPONENTS],
            absolute,
            trailing_slash,
        };
        let mut index = 0usize;
        while index < path.len() {
            if path[index] == b'/' {
                index += 1;
                continue;
            }
            let start = index;
            while index < path.len() && path[index] != b'/' {
                index += 1;
            }
            let bytes = &path[start..index];
            if bytes.len() > MAX_NAME {
                return Err(VfsError::Invalid);
            }
            if parsed.count as usize == MAX_COMPONENTS {
                return Err(VfsError::Invalid);
            }
            let mut component = Component {
                len: bytes.len() as u8,
                bytes: [0u8; MAX_NAME],
            };
            component.bytes[..bytes.len()].copy_from_slice(bytes);
            parsed.components[parsed.count as usize] = component;
            parsed.count += 1;
        }
        if !absolute && parsed.count == 0 {
            return Err(VfsError::Invalid);
        }
        Ok(parsed)
    }

    fn component(&self, index: usize) -> &Component {
        &self.components[index]
    }

    fn prefix(&self) -> Self {
        let mut prefix = Self {
            count: self.count.saturating_sub(1),
            components: self.components,
            absolute: self.absolute,
            trailing_slash: false,
        };
        prefix.count = prefix.count.min(MAX_COMPONENTS as u8);
        prefix
    }

    fn last(&self) -> Option<&Component> {
        if self.count == 0 {
            None
        } else {
            Some(&self.components[self.count as usize - 1])
        }
    }
}

/// 按路径查找目录项（不创建）；返回 `None` 表示父目录存在但末项不存在。
fn lookup<S: Storage>(
    storage: &mut S,
    directory: NodeId,
    name: &[u8],
) -> Result<Option<NodeId>, VfsError> {
    let parent = read_node(storage, directory)?;
    if parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(VfsError::Invalid);
    }
    let header = read_header(storage)?;
    for id in 1..header.count {
        let node = read_node(storage, id)?;
        if !is_free(&node) && node.parent == directory && node.name_bytes() == name {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// M13a：符号链接展开的冻结深度上限。
pub const MAX_SYMLINK_DEPTH: u8 = 8;
/// 展开符号链接时待处理分量的上限（避免无限增长）。
const PENDING_COMPONENTS: usize = MAX_COMPONENTS * 2 + 4;

/// 读符号链接目标（不跟随）。目标长度受 [`MAX_PATH`] 与内容区边界约束。
pub fn read_link<S: Storage>(storage: &mut S, node: &Node) -> Result<[u8; MAX_PATH], VfsError> {
    if node.kind != NodeKind::Symlink {
        return Err(VfsError::NotDir);
    }
    let len = node.size as usize;
    if len > MAX_PATH {
        return Err(VfsError::Invalid);
    }
    let mut target = [0u8; MAX_PATH];
    if len != 0 {
        read_exact(
            storage,
            CONTENT_OFFSET + node.data as usize,
            &mut target[..len],
        )?;
    }
    Ok(target)
}

/// 按分量逐项走路径，并在遇到符号链接时展开。
///
/// 相对目标从链接所在目录开始，绝对目标从根开始；展开深度由
/// [`MAX_SYMLINK_DEPTH`] 冻结，剩余分量按原顺序接在目标分量之后。链接目标
/// 本身仍受 [`MAX_PATH`] 解析规则约束，故总分量数有界。
fn walk<S: Storage>(
    storage: &mut S,
    start: NodeId,
    parsed: &ParsedPath,
) -> Result<NodeId, VfsError> {
    let mut current = if parsed.absolute { ROOT } else { start };
    if !parsed.absolute && read_node(storage, current)?.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    let mut pending = [Component {
        len: 0,
        bytes: [0u8; MAX_NAME],
    }; PENDING_COMPONENTS];
    let mut count = 0usize;
    for index in 0..parsed.count as usize {
        pending[count] = *parsed.component(index);
        count += 1;
    }
    let mut index = 0usize;
    let mut depth = 0u8;
    while index < count {
        let component = pending[index];
        index += 1;
        if component.is_dot() {
            continue;
        }
        if component.is_dot_dot() {
            current = read_node(storage, current)?.parent;
            continue;
        }
        let id = lookup(storage, current, component.as_bytes())?.ok_or(VfsError::NoEnt)?;
        let node = read_node(storage, id)?;
        if node.kind != NodeKind::Symlink {
            current = id;
            continue;
        }
        if depth >= MAX_SYMLINK_DEPTH {
            return Err(VfsError::Loop);
        }
        depth += 1;
        let target = read_link(storage, &node)?;
        let len = node.size as usize;
        let target = ParsedPath::parse(&target[..len])?;
        let remaining = count - index;
        let total = remaining + target.count as usize;
        if count + target.count as usize > PENDING_COMPONENTS {
            return Err(VfsError::Invalid);
        }
        // 目标分量插到剩余分量之前；相对目标以链接父目录为基准。
        pending.copy_within(index..count, index + target.count as usize);
        for slot in 0..target.count as usize {
            pending[index + slot] = *target.component(slot);
        }
        count = index + total;
        current = if target.absolute { ROOT } else { node.parent };
    }
    if parsed.trailing_slash && read_node(storage, current)?.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    Ok(current)
}

/// 解析路径（不创建）。`start` 是相对路径的起始目录；绝对路径从根开始。
pub fn resolve<S: Storage>(
    storage: &mut S,
    start: NodeId,
    path: &[u8],
) -> Result<NodeId, VfsError> {
    walk(storage, start, &ParsedPath::parse(path)?)
}

/// M13a：解析到**末项本身**，不跟随末项的符号链接（`lstat`/`readlink` 语义）。
///
/// 前缀分量仍按 [`walk`] 展开链接；末项为空、`.`、`..` 或父目录不存在时确定失败。
pub fn resolve_link<S: Storage>(
    storage: &mut S,
    start: NodeId,
    path: &[u8],
) -> Result<NodeId, VfsError> {
    let parsed = ParsedPath::parse(path)?;
    if parsed.count == 0 {
        return Err(VfsError::Invalid);
    }
    let last = parsed.last().copied().ok_or(VfsError::Invalid)?;
    if last.is_dot() || last.is_dot_dot() {
        return Err(VfsError::Invalid);
    }
    let directory = walk(storage, start, &parsed.prefix())?;
    lookup(storage, directory, last.as_bytes())?.ok_or(VfsError::NoEnt)
}

/// 打开/创建文件或目录。
pub fn open<S: Storage>(
    storage: &mut S,
    start: NodeId,
    path: &[u8],
    flags: OpenFlags,
) -> Result<Opened, VfsError> {
    if flags.access > 2 {
        return Err(VfsError::Invalid);
    }
    let parsed = ParsedPath::parse(path)?;
    let resolved = walk(storage, start, &parsed);
    if let Ok(id) = resolved {
        return finish_open(storage, id, flags);
    }
    if resolved != Err(VfsError::NoEnt) || !flags.create {
        return Err(resolved.err().unwrap_or(VfsError::NoEnt));
    }
    // 只有**末项**缺失才允许创建；前缀缺失/非目录仍按原错误返回。
    let Some(last) = parsed.last().copied() else {
        return Err(VfsError::NoEnt);
    };
    if last.is_dot() || last.is_dot_dot() {
        return Err(VfsError::NoEnt);
    }
    let directory = walk(storage, start, &parsed.prefix())?;
    let parent = read_node(storage, directory)?;
    if parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if !parent.writable() {
        return Err(VfsError::Access);
    }
    if flags.directory {
        return Err(VfsError::NoEnt);
    }
    if lookup(storage, directory, last.as_bytes())?.is_some() {
        let id = walk(storage, start, &parsed)?;
        return finish_open(storage, id, flags);
    }
    // M11：新建文件同样复用已删除槽，避免删除后节点表高水位仍把创建挡在 ENOSPC。
    let id = alloc_node(storage)?;
    let data = read_header(storage)?.data_used;
    let mut name = [0u8; MAX_NAME];
    name[..last.len as usize].copy_from_slice(last.as_bytes());
    let node = Node {
        kind: NodeKind::File,
        parent: directory,
        mode: 0o666,
        size: 0,
        data,
        name_len: last.len,
        name,
    };
    store_node(storage, id, &node)?;
    Ok(Opened { id, node })
}

fn finish_open<S: Storage>(
    storage: &mut S,
    id: NodeId,
    flags: OpenFlags,
) -> Result<Opened, VfsError> {
    let mut node = read_node(storage, id)?;
    if flags.create && flags.exclusive {
        return Err(VfsError::Exist);
    }
    if flags.directory && node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if flags.access != 0 {
        // `/dev/null` 接受任意 access：读总是 EOF，写被丢弃。
        if node.kind == NodeKind::Null {
            // allowed
        } else if node.kind != NodeKind::File || !node.writable() {
            return Err(VfsError::Access);
        }
    }
    if flags.truncate {
        if node.kind != NodeKind::File || !node.writable() {
            return Err(VfsError::Access);
        }
        node.size = 0;
        write_node(storage, id, &node)?;
    }
    Ok(Opened { id, node })
}

/// 分配一个节点槽：优先复用已删除槽，否则追加到高水位；`MAX_NODES` 用尽返回 `ENOSPC`。
fn alloc_node<S: Storage>(storage: &mut S) -> Result<NodeId, VfsError> {
    let mut header = read_header(storage)?;
    for id in 1..header.count {
        if is_free(&read_node(storage, id)?) {
            return Ok(id);
        }
    }
    if header.count as usize >= MAX_NODES {
        return Err(VfsError::NoSpace);
    }
    let id = header.count;
    header.count += 1;
    if !header.write(storage) {
        return Err(VfsError::Io);
    }
    Ok(id)
}

/// 标记节点删除。内容区不回收（首版没有内容空闲表），槽位可被后续创建复用。
fn free_node<S: Storage>(storage: &mut S, id: NodeId) -> Result<(), VfsError> {
    let mut node = read_node(storage, id)?;
    node.parent = FREE_PARENT;
    node.name_len = 0;
    node.name = [0u8; MAX_NAME];
    node.size = 0;
    node.data = 0;
    store_node(storage, id, &node)
}

/// 目录是否仍有子节点（跳过已删除槽）。
fn has_children<S: Storage>(storage: &mut S, id: NodeId) -> Result<bool, VfsError> {
    let header = read_header(storage)?;
    for child in 1..header.count {
        let node = read_node(storage, child)?;
        if !is_free(&node) && node.parent == id {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `mkdir`：在可写目录中创建一个空目录（mode 由调用方忽略，统一 0777/可写）。
pub fn mkdir<S: Storage>(storage: &mut S, start: NodeId, path: &[u8]) -> Result<NodeId, VfsError> {
    let parsed = ParsedPath::parse(path)?;
    if parsed.count == 0 {
        return Err(VfsError::Exist);
    }
    let last = parsed.last().copied().ok_or(VfsError::Invalid)?;
    if last.is_dot() || last.is_dot_dot() {
        return Err(VfsError::Exist);
    }
    let directory = walk(storage, start, &parsed.prefix())?;
    let parent = read_node(storage, directory)?;
    if parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if !parent.writable() {
        return Err(VfsError::Access);
    }
    if lookup(storage, directory, last.as_bytes())?.is_some() {
        return Err(VfsError::Exist);
    }
    let id = alloc_node(storage)?;
    let mut name = [0u8; MAX_NAME];
    name[..last.len as usize].copy_from_slice(last.as_bytes());
    let node = Node {
        kind: NodeKind::Directory,
        parent: directory,
        mode: 0o777,
        size: 0,
        data: 0,
        name_len: last.len,
        name,
    };
    store_node(storage, id, &node)?;
    Ok(id)
}

/// `unlink`/`rmdir`：删除末项。目录必须为空（`remove_dir`），普通文件/设备按文件删除；
/// 根、`.`、`..` 与只读父目录确定失败。打开句柄的检查由存储服务在调用前完成。
pub fn unlink<S: Storage>(
    storage: &mut S,
    start: NodeId,
    path: &[u8],
    remove_dir: bool,
) -> Result<NodeId, VfsError> {
    let parsed = ParsedPath::parse(path)?;
    if parsed.count == 0 {
        return Err(VfsError::Access);
    }
    let last = parsed.last().copied().ok_or(VfsError::Invalid)?;
    if last.is_dot() || last.is_dot_dot() {
        return Err(VfsError::Invalid);
    }
    let directory = walk(storage, start, &parsed.prefix())?;
    let parent = read_node(storage, directory)?;
    if parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if !parent.writable() {
        return Err(VfsError::Access);
    }
    let id = lookup(storage, directory, last.as_bytes())?.ok_or(VfsError::NoEnt)?;
    let node = read_node(storage, id)?;
    // 尾随 `/` 要求目标是目录（Linux `unlink("file/")` → ENOTDIR）。
    if parsed.trailing_slash && node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if remove_dir {
        if node.kind != NodeKind::Directory {
            return Err(VfsError::NotDir);
        }
        if has_children(storage, id)? {
            // 冻结错误集没有 ENOTEMPTY；用 EACCES 表达“目录非空不可删”。
            return Err(VfsError::Access);
        }
    } else {
        if node.kind == NodeKind::Directory {
            // 冻结错误集没有 EISDIR；用 EACCES 表达“目录不能用 unlink 删除”。
            return Err(VfsError::Access);
        }
        if node.kind == NodeKind::Null {
            return Err(VfsError::Access);
        }
    }
    free_node(storage, id)?;
    Ok(id)
}

/// M13b：修改节点权限位（低 12 位）。路径跟随符号链接；空路径表示 `start` 节点
/// 本身（`fchmod`）。单用户模型不校验 uid/gid，只保存与检查 owner 读写位。
pub fn chmod<S: Storage>(
    storage: &mut S,
    start: NodeId,
    path: &[u8],
    mode: u16,
) -> Result<(), VfsError> {
    if mode > 0o777 {
        return Err(VfsError::Invalid);
    }
    let id = if path.is_empty() {
        read_node(storage, start)?; // 仅校验节点存在，不跟随
        start
    } else {
        resolve(storage, start, path)?
    };
    let mut node = read_node(storage, id)?;
    node.mode = mode;
    write_node(storage, id, &node)
}

/// M13a：在可写目录中创建符号链接，目标字节写入内容区。
///
/// 目标只做长度/非空校验，不要求当前存在（允许悬空链接）；解析时再按冻结深度
/// 展开。路径分量沿用 [`ParsedPath`] 上限，不复用 `path` 参数做拼接。
pub fn symlink<S: Storage>(
    storage: &mut S,
    start: NodeId,
    target: &[u8],
    path: &[u8],
) -> Result<NodeId, VfsError> {
    if target.is_empty() || target.len() > MAX_PATH {
        return Err(VfsError::Invalid);
    }
    let parsed = ParsedPath::parse(path)?;
    if parsed.count == 0 {
        return Err(VfsError::Exist);
    }
    let last = parsed.last().copied().ok_or(VfsError::Invalid)?;
    if last.is_dot() || last.is_dot_dot() {
        return Err(VfsError::Exist);
    }
    let directory = walk(storage, start, &parsed.prefix())?;
    let parent = read_node(storage, directory)?;
    if parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if !parent.writable() {
        return Err(VfsError::Access);
    }
    if lookup(storage, directory, last.as_bytes())?.is_some() {
        return Err(VfsError::Exist);
    }
    let data = alloc_content(storage, target.len())?;
    if !target.is_empty() {
        write_all(storage, CONTENT_OFFSET + data as usize, target)?;
    }
    let id = alloc_node(storage)?;
    let mut name = [0u8; MAX_NAME];
    name[..last.len as usize].copy_from_slice(last.as_bytes());
    let node = Node {
        kind: NodeKind::Symlink,
        parent: directory,
        mode: 0o777,
        size: target.len() as u16,
        data,
        name_len: last.len,
        name,
    };
    store_node(storage, id, &node)?;
    Ok(id)
}

/// 在内容区分配 `len` 字节；空间不足时先压实回收已删除文件留下的空洞。
fn alloc_content<S: Storage>(storage: &mut S, len: usize) -> Result<u16, VfsError> {
    let mut header = read_header(storage)?;
    if header.data_used as usize + len > CONTENT_CAPACITY {
        compact(storage)?;
        header = read_header(storage)?;
    }
    let start = header.data_used as usize;
    if start + len > CONTENT_CAPACITY {
        return Err(VfsError::NoSpace);
    }
    header.data_used = (start + len) as u16;
    if !header.write(storage) {
        return Err(VfsError::Io);
    }
    Ok(start as u16)
}

/// M13a：压实内容区，回收已删除节点留下的空洞。
///
/// 按当前 `data` 升序把存活文件/符号链接的内容左移并重写节点的 `data`；节点编号
/// 不变，打开句柄只持节点编号，故压实不破坏句柄。目标偏移总是不大于源偏移，
/// 分块前向拷贝不会覆盖尚未读取的字节。
fn compact<S: Storage>(storage: &mut S) -> Result<(), VfsError> {
    let header = read_header(storage)?;
    let mut entries = [(0u8, 0u16, 0u16); MAX_NODES];
    let mut count = 0usize;
    for id in 1..header.count {
        let node = read_node(storage, id)?;
        if is_free(&node) || node.size == 0 {
            continue;
        }
        if node.kind == NodeKind::File || node.kind == NodeKind::Symlink {
            entries[count] = (id, node.data, node.size);
            count += 1;
        }
    }
    // 16 项以内的插入排序，避免引入分配。
    for index in 1..count {
        let item = entries[index];
        let mut slot = index;
        while slot > 0 && entries[slot - 1].1 > item.1 {
            entries[slot] = entries[slot - 1];
            slot -= 1;
        }
        entries[slot] = item;
    }
    let mut used = 0usize;
    for (id, data, size) in entries[..count].iter().copied() {
        if data as usize != used {
            let mut copied = 0usize;
            while copied < size as usize {
                let chunk = core::cmp::min(32, size as usize - copied);
                let mut bytes = [0u8; 32];
                read_exact(
                    storage,
                    CONTENT_OFFSET + data as usize + copied,
                    &mut bytes[..chunk],
                )?;
                write_all(storage, CONTENT_OFFSET + used + copied, &bytes[..chunk])?;
                copied += chunk;
            }
            let mut node = read_node(storage, id)?;
            node.data = used as u16;
            write_node(storage, id, &node)?;
        }
        used += size as usize;
    }
    let mut updated = read_header(storage)?;
    updated.data_used = used as u16;
    if !updated.write(storage) {
        return Err(VfsError::Io);
    }
    Ok(())
}

/// `candidate` 是否位于 `ancestor` 之下（含自身）。
fn in_subtree<S: Storage>(
    storage: &mut S,
    ancestor: NodeId,
    candidate: NodeId,
) -> Result<bool, VfsError> {
    let mut current = candidate;
    for _ in 0..=MAX_NODES {
        if current == ancestor {
            return Ok(true);
        }
        if current == ROOT {
            return Ok(false);
        }
        current = read_node(storage, current)?.parent;
        if current == FREE_PARENT {
            return Ok(false);
        }
    }
    Ok(false)
}

/// M13a：同一 VFS 内重命名/移动节点；末项不跟随符号链接。
///
/// 目标已存在时：文件/链接可替换文件/链接，目录只能替换空目录；错误集沿用
/// 冻结的 `ENOENT`/`ENOTDIR`/`EACCES`/`EINVAL`，不新增 errno。
pub fn rename<S: Storage>(
    storage: &mut S,
    start: NodeId,
    from: &[u8],
    to: &[u8],
) -> Result<(), VfsError> {
    let from_parsed = ParsedPath::parse(from)?;
    if from_parsed.count == 0 {
        return Err(VfsError::Invalid);
    }
    let from_last = from_parsed.last().copied().ok_or(VfsError::Invalid)?;
    if from_last.is_dot() || from_last.is_dot_dot() {
        return Err(VfsError::Invalid);
    }
    let from_dir = walk(storage, start, &from_parsed.prefix())?;
    let from_id = lookup(storage, from_dir, from_last.as_bytes())?.ok_or(VfsError::NoEnt)?;
    let from_node = read_node(storage, from_id)?;
    if from_parsed.trailing_slash && from_node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    let to_parsed = ParsedPath::parse(to)?;
    if to_parsed.count == 0 {
        return Err(VfsError::Invalid);
    }
    let to_last = to_parsed.last().copied().ok_or(VfsError::Invalid)?;
    if to_last.is_dot() || to_last.is_dot_dot() {
        return Err(VfsError::Invalid);
    }
    let to_dir = walk(storage, start, &to_parsed.prefix())?;
    let to_parent = read_node(storage, to_dir)?;
    if to_parent.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if !to_parent.writable() {
        return Err(VfsError::Access);
    }
    if to_parsed.trailing_slash && from_node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if to_dir == from_id {
        return Err(VfsError::Invalid);
    }
    if from_node.kind == NodeKind::Directory && in_subtree(storage, from_id, to_dir)? {
        return Err(VfsError::Invalid);
    }
    if let Some(existing) = lookup(storage, to_dir, to_last.as_bytes())? {
        if existing == from_id {
            // 同一路径：Linux 返回成功且不改动。
            return Ok(());
        }
        let target = read_node(storage, existing)?;
        if target.kind == NodeKind::Directory {
            if from_node.kind != NodeKind::Directory {
                return Err(VfsError::NotDir);
            }
            if has_children(storage, existing)? {
                return Err(VfsError::Access);
            }
        } else if from_node.kind == NodeKind::Directory {
            return Err(VfsError::NotDir);
        }
        free_node(storage, existing)?;
    }
    let mut moved = from_node;
    moved.parent = to_dir;
    moved.name_len = to_last.len;
    moved.name = [0u8; MAX_NAME];
    moved.name[..to_last.len as usize].copy_from_slice(to_last.as_bytes());
    write_node(storage, from_id, &moved)
}

/// 读文件/设备。目录读取由 [`dir_bytes`] 单独提供，避免把目录当普通文件暴露给 `read(2)`。
pub fn read<S: Storage>(
    storage: &mut S,
    node: &Node,
    offset: u64,
    out: &mut [u8],
) -> Result<usize, VfsError> {
    match node.kind {
        NodeKind::Null => Ok(0),
        NodeKind::Directory => Err(VfsError::NotDir),
        // 符号链接的 fd 只在 readlink 语义下有意义；`read(2)` 返回 EINVAL。
        NodeKind::Symlink => Err(VfsError::Invalid),
        NodeKind::File => {
            let size = node.size as u64;
            if offset >= size || out.is_empty() {
                return Ok(0);
            }
            let available = (size - offset) as usize;
            let count = out.len().min(available);
            let start = CONTENT_OFFSET + node.data as usize + offset as usize;
            read_exact(storage, start, &mut out[..count])?;
            Ok(count)
        }
    }
}

/// 写文件；调用方负责 `access` 检查。返回实际写入的字节数。
///
/// 内容区按“独占区间”管理：写入不增长时原地覆盖；增长时先检查本文件到下一个
/// 存活区间的空隙与内容区尾部，不足则先压实，再把文件内容搬到尾部（旧区间成为
/// 空洞，由后续压实回收）。这样两个先后创建的空文件不会共享同一区间。
pub fn write<S: Storage>(
    storage: &mut S,
    id: NodeId,
    node: &mut Node,
    offset: u64,
    data: &[u8],
) -> Result<usize, VfsError> {
    if node.kind != NodeKind::File {
        return Err(VfsError::NotDir);
    }
    if !node.writable() {
        return Err(VfsError::Access);
    }
    if offset > CONTENT_CAPACITY as u64 {
        return Err(VfsError::NoSpace);
    }
    let Some(end) = (offset as usize).checked_add(data.len()) else {
        return Err(VfsError::NoSpace);
    };
    if end > CONTENT_CAPACITY {
        return Err(VfsError::NoSpace);
    }
    if end > node.size as usize && end > growth_limit(storage, id, node)? {
        // 原地放不下：先压实回收空洞，再按新的布局重试。
        compact(storage)?;
        *node = read_node(storage, id)?;
        if end > growth_limit(storage, id, node)? {
            relocate(storage, id, node, end)?;
        }
    }
    if !data.is_empty() {
        write_all(
            storage,
            CONTENT_OFFSET + node.data as usize + offset as usize,
            data,
        )?;
    }
    if end > node.size as usize {
        node.size = end as u16;
        write_node(storage, id, node)?;
        // 尾部高水位随原地增长前移，供下一次创建/搬迁使用。
        let mut header = read_header(storage)?;
        let used = node.data as usize + end;
        if used > header.data_used as usize {
            header.data_used = used as u16;
            if !header.write(storage) {
                return Err(VfsError::Io);
            }
        }
    }
    Ok(data.len())
}

/// 本文件在不覆盖其他存活区间的前提下可达到的最大长度（相对文件起点）。
fn growth_limit<S: Storage>(storage: &mut S, id: NodeId, node: &Node) -> Result<usize, VfsError> {
    let data = node.data as usize;
    let mut limit = CONTENT_CAPACITY.saturating_sub(data);
    let header = read_header(storage)?;
    for other in 1..header.count {
        if other == id {
            continue;
        }
        let node = read_node(storage, other)?;
        if is_free(&node) || node.size == 0 {
            continue;
        }
        let start = node.data as usize;
        if start >= data && start - data < limit {
            limit = start - data;
        }
    }
    Ok(limit)
}

/// 把文件内容搬到内容区尾部，为新长度腾出连续空间。
fn relocate<S: Storage>(
    storage: &mut S,
    id: NodeId,
    node: &mut Node,
    end: usize,
) -> Result<(), VfsError> {
    let mut header = read_header(storage)?;
    let mut tail = header.data_used as usize;
    for other in 1..header.count {
        let node = read_node(storage, other)?;
        if is_free(&node) {
            continue;
        }
        let used = node.data as usize + node.size as usize;
        if used > tail {
            tail = used;
        }
    }
    if tail + end > CONTENT_CAPACITY {
        return Err(VfsError::NoSpace);
    }
    let old_data = node.data as usize;
    let old_size = node.size as usize;
    let mut copied = 0usize;
    while copied < old_size {
        let chunk = core::cmp::min(32, old_size - copied);
        let mut bytes = [0u8; 32];
        read_exact(
            storage,
            CONTENT_OFFSET + old_data + copied,
            &mut bytes[..chunk],
        )?;
        write_all(storage, CONTENT_OFFSET + tail + copied, &bytes[..chunk])?;
        copied += chunk;
    }
    node.data = tail as u16;
    header.data_used = (tail + end) as u16;
    if !header.write(storage) {
        return Err(VfsError::Io);
    }
    write_node(storage, id, node)
}

/// 把文件截断到 `size`；只允许缩小或保持不变。
pub fn truncate<S: Storage>(
    storage: &mut S,
    id: NodeId,
    node: &mut Node,
    size: u64,
) -> Result<(), VfsError> {
    if node.kind != NodeKind::File {
        return Err(VfsError::NotDir);
    }
    if !node.writable() {
        return Err(VfsError::Access);
    }
    if size > node.size as u64 {
        return Err(VfsError::Invalid);
    }
    node.size = size as u16;
    write_node(storage, id, node)
}

/// 目录条目数（含 `.` 与 `..`）。
pub fn dir_entry_count<S: Storage>(storage: &mut S, id: NodeId) -> Result<usize, VfsError> {
    let node = read_node(storage, id)?;
    if node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    let header = read_header(storage)?;
    let mut count = 2usize;
    for child in 1..header.count {
        let node = read_node(storage, child)?;
        if !is_free(&node) && node.parent == id {
            count += 1;
        }
    }
    Ok(count)
}

/// 取目录的第 `index` 条记录；`0` 是 `.`、`1` 是 `..`。
pub fn dir_entry<S: Storage>(
    storage: &mut S,
    id: NodeId,
    index: usize,
) -> Result<Option<DirEntry>, VfsError> {
    let node = read_node(storage, id)?;
    if node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    let mut entry = DirEntry {
        node: id,
        kind: NodeKind::Directory,
        name_len: 0,
        name: [0u8; MAX_NAME],
    };
    if index == 0 {
        entry.name[0] = b'.';
        entry.name_len = 1;
        return Ok(Some(entry));
    }
    if index == 1 {
        entry.node = node.parent;
        entry.name[..2].copy_from_slice(b"..");
        entry.name_len = 2;
        return Ok(Some(entry));
    }
    let header = read_header(storage)?;
    let mut seen = 2usize;
    for child in 1..header.count {
        let child_node = read_node(storage, child)?;
        if is_free(&child_node) || child_node.parent != id {
            continue;
        }
        if seen == index {
            entry.node = child;
            entry.kind = child_node.kind;
            entry.name_len = child_node.name_len;
            entry.name = child_node.name;
            return Ok(Some(entry));
        }
        seen += 1;
    }
    Ok(None)
}

fn dir_record(entry: &DirEntry) -> usize {
    4 + entry.name_len as usize
}

/// 目录的序列化总字节数（每条记录：`node:u16 | kind:u8 | name_len:u8 | name`）。
pub fn dir_total<S: Storage>(storage: &mut S, id: NodeId) -> Result<usize, VfsError> {
    let count = dir_entry_count(storage, id)?;
    let mut total = 0usize;
    for index in 0..count {
        if let Some(entry) = dir_entry(storage, id, index)? {
            total += dir_record(&entry);
        }
    }
    Ok(total)
}

/// 从目录序列化字节流中读取 `offset..` 的一段，返回拷贝的字节数。
pub fn dir_bytes<S: Storage>(
    storage: &mut S,
    id: NodeId,
    offset: u64,
    out: &mut [u8],
) -> Result<usize, VfsError> {
    let count = dir_entry_count(storage, id)?;
    let mut position = 0usize;
    let mut written = 0usize;
    let begin = offset as usize;
    let end = begin.saturating_add(out.len());
    for index in 0..count {
        let Some(entry) = dir_entry(storage, id, index)? else {
            break;
        };
        let record_len = dir_record(&entry);
        if position >= end {
            break;
        }
        if position + record_len > begin {
            let record_start = position;
            let record = [entry.node, 0, entry.kind.as_raw(), entry.name_len];
            for (index, byte) in record.iter().enumerate() {
                let absolute = record_start + index;
                if absolute >= begin && absolute < end && written < out.len() {
                    let target = absolute - begin;
                    out[target] = *byte;
                    written += 1;
                }
            }
            for (index, byte) in entry.name_bytes().iter().enumerate() {
                let absolute = record_start + 4 + index;
                if absolute >= begin && absolute < end && written < out.len() {
                    let target = absolute - begin;
                    out[target] = *byte;
                    written += 1;
                }
            }
        }
        position += record_len;
    }
    Ok(written)
}

/// 内容区剩余字节（供容量诊断）。
pub fn content_used<S: Storage>(storage: &mut S) -> Result<usize, VfsError> {
    Ok(header_data_used(storage)? as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    struct TestStorage {
        bytes: Vec<u8>,
    }

    impl TestStorage {
        fn new() -> Self {
            Self {
                bytes: vec![0u8; 4096 - 8],
            }
        }
    }

    impl Storage for TestStorage {
        fn read(&mut self, offset: usize, out: &mut [u8]) -> bool {
            let Some(end) = offset.checked_add(out.len()) else {
                return false;
            };
            if end > self.bytes.len() {
                return false;
            }
            out.copy_from_slice(&self.bytes[offset..end]);
            true
        }

        fn write(&mut self, offset: usize, data: &[u8]) -> bool {
            let Some(end) = offset.checked_add(data.len()) else {
                return false;
            };
            if end > self.bytes.len() {
                return false;
            }
            self.bytes[offset..end].copy_from_slice(data);
            true
        }
    }

    fn mounted() -> TestStorage {
        let mut storage = TestStorage::new();
        assert_eq!(mount(&mut storage), Ok(()));
        storage
    }

    fn read_file(storage: &mut TestStorage, path: &[u8]) -> Vec<u8> {
        let id = resolve(storage, ROOT, path).expect("path resolves");
        let node = read_node(storage, id).expect("node reads");
        let mut out = vec![0u8; node.size as usize];
        let count = read(storage, &node, 0, &mut out).expect("file reads");
        out.truncate(count);
        out
    }

    #[test]
    fn mount_builds_the_multi_level_rootfs() {
        let mut storage = mounted();
        assert_eq!(read_file(&mut storage, b"/etc/motd"), b"welcome\n");
        assert_eq!(read_file(&mut storage, b"/etc/hostname"), b"prima\n");
        assert_eq!(read_file(&mut storage, b"/usr/lib/version"), b"1.0\n");
        assert_eq!(read_file(&mut storage, b"README"), b"prima rootfs\n");
        assert_eq!(read_file(&mut storage, b"etc/motd"), b"welcome\n");
        assert_eq!(read_file(&mut storage, b"etc/../etc/hostname"), b"prima\n");
        assert_eq!(
            read_file(&mut storage, b"usr/./lib/../lib/version"),
            b"1.0\n"
        );
        assert_eq!(resolve(&mut storage, ROOT, b"/").unwrap(), ROOT);
        assert_eq!(resolve(&mut storage, ROOT, b"..").unwrap(), ROOT);
        assert_eq!(resolve(&mut storage, ROOT, b"///").unwrap(), ROOT);
        assert_eq!(content_used(&mut storage), Ok(561));
    }

    #[test]
    fn tiny_elf_fixture_is_readable() {
        let mut storage = mounted();
        assert_eq!(read_file(&mut storage, b"/tiny.elf"), TINY_ELF);
        assert!(TINY_ELF.len() > 100);
        assert_eq!(&TINY_ELF[..4], b"\x7fELF");
    }

    #[test]
    fn resolution_reports_frozen_errors() {
        let mut storage = mounted();
        assert_eq!(resolve(&mut storage, ROOT, b""), Err(VfsError::Invalid));
        assert_eq!(resolve(&mut storage, ROOT, b"nope"), Err(VfsError::NoEnt));
        assert_eq!(
            resolve(&mut storage, ROOT, b"/etc/motd/x"),
            Err(VfsError::NotDir)
        );
        assert_eq!(
            resolve(&mut storage, ROOT, b"/etc/motd/"),
            Err(VfsError::NotDir)
        );
        let too_long = [b'a'; MAX_NAME + 1];
        assert_eq!(
            resolve(&mut storage, ROOT, &too_long),
            Err(VfsError::Invalid)
        );
        let too_many = b"a/a/a/a/a/a/a/a/a";
        assert_eq!(
            resolve(&mut storage, ROOT, too_many),
            Err(VfsError::Invalid)
        );
        let etc = resolve(&mut storage, ROOT, b"etc").unwrap();
        assert!(resolve(&mut storage, etc, b"motd").unwrap() > 0);
        assert_eq!(resolve(&mut storage, etc, b"motd/x"), Err(VfsError::NotDir));
    }

    #[test]
    fn open_enforces_access_create_and_exclusive() {
        let mut storage = mounted();
        let flags = OpenFlags {
            access: 0,
            ..OpenFlags::default()
        };
        let opened = open(&mut storage, ROOT, b"/etc/motd", flags).unwrap();
        assert_eq!(opened.node.kind, NodeKind::File);
        let writable = OpenFlags {
            access: 1,
            ..OpenFlags::default()
        };
        assert_eq!(
            open(&mut storage, ROOT, b"/etc/motd", writable),
            Err(VfsError::Access)
        );
        let create = OpenFlags {
            access: 2,
            create: true,
            exclusive: true,
            ..OpenFlags::default()
        };
        assert_eq!(
            open(&mut storage, ROOT, b"/etc/motd", create),
            Err(VfsError::Exist)
        );
        assert_eq!(
            open(
                &mut storage,
                ROOT,
                b"/nope",
                OpenFlags {
                    access: 2,
                    create: true,
                    ..OpenFlags::default()
                }
            ),
            Err(VfsError::Access)
        );
        let directory = OpenFlags {
            access: 0,
            directory: true,
            ..OpenFlags::default()
        };
        assert_eq!(
            open(&mut storage, ROOT, b"/etc/motd", directory),
            Err(VfsError::NotDir)
        );
        assert_eq!(
            open(&mut storage, ROOT, b"/tmp/scratch", create)
                .expect("create in writable dir")
                .node
                .kind,
            NodeKind::File
        );
    }

    #[test]
    fn write_truncate_and_append_round_trip() {
        let mut storage = mounted();
        let opened = open(
            &mut storage,
            ROOT,
            b"/tmp/scratch",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        let mut node = opened.node;
        assert_eq!(
            write(&mut storage, opened.id, &mut node, 0, b"abcdef"),
            Ok(6)
        );
        assert_eq!(node.size, 6);
        let mut out = [0u8; 8];
        assert_eq!(read(&mut storage, &node, 0, &mut out), Ok(6));
        assert_eq!(&out[..6], b"abcdef");
        assert_eq!(write(&mut storage, opened.id, &mut node, 6, b"ghi"), Ok(3));
        assert_eq!(node.size, 9);
        assert_eq!(truncate(&mut storage, opened.id, &mut node, 3), Ok(()));
        assert_eq!(node.size, 3);
        assert_eq!(read(&mut storage, &node, 0, &mut out), Ok(3));
        assert_eq!(&out[..3], b"abc");
    }

    #[test]
    fn directory_listing_includes_dot_and_dot_dot() {
        let mut storage = mounted();
        let count = dir_entry_count(&mut storage, ROOT).unwrap();
        assert_eq!(count, 9);
        let dot = dir_entry(&mut storage, ROOT, 0).unwrap().unwrap();
        assert_eq!(dot.name_bytes(), b".");
        let dot_dot = dir_entry(&mut storage, ROOT, 1).unwrap().unwrap();
        assert_eq!(dot_dot.name_bytes(), b"..");
        assert_eq!(dot_dot.node, ROOT);
        let etc = resolve(&mut storage, ROOT, b"etc").unwrap();
        let total = dir_total(&mut storage, etc).unwrap();
        assert_eq!(total, 5 + 6 + 8 + 12); // ., .., motd, hostname
        let mut bytes = vec![0u8; total];
        assert_eq!(dir_bytes(&mut storage, etc, 0, &mut bytes), Ok(total));
        assert_eq!(bytes[0], etc);
        assert_eq!(bytes[2], NodeKind::Directory.as_raw());
        assert!(bytes.windows(4).any(|window| window == b"motd"));
        let mut tail = [0u8; 4];
        assert_eq!(dir_bytes(&mut storage, etc, total as u64, &mut tail), Ok(0));
    }

    #[test]
    fn mount_is_idempotent_and_capacity_is_finite() {
        let mut storage = mounted();
        let opened = open(
            &mut storage,
            ROOT,
            b"/tmp/a",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        let mut node = opened.node;
        assert_eq!(write(&mut storage, opened.id, &mut node, 0, b"x"), Ok(1));
        assert_eq!(mount(&mut storage), Ok(()));
        assert_eq!(read_file(&mut storage, b"/tmp/a"), b"x");
        let mut next = 0u32;
        loop {
            let mut name = [b'/'; 8];
            name[1] = b't';
            name[2] = b'm';
            name[3] = b'p';
            name[4] = b'/';
            name[5] = b'a' + ((next / 10) % 10) as u8;
            name[6] = b'a' + (next % 10) as u8;
            let path = &name[..7];
            match open(
                &mut storage,
                ROOT,
                path,
                OpenFlags {
                    access: 2,
                    create: true,
                    ..OpenFlags::default()
                },
            ) {
                Ok(_) => next += 1,
                Err(error) => {
                    assert_eq!(error, VfsError::NoSpace);
                    break;
                }
            }
        }
        assert!(next >= 3);
    }

    #[test]
    fn content_capacity_is_enforced() {
        let mut storage = mounted();
        let opened = open(
            &mut storage,
            ROOT,
            b"/tmp/big",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        let mut node = opened.node;
        let initial = content_used(&mut storage).unwrap();
        let chunk = [b'z'; 512];
        let mut offset = 0usize;
        loop {
            match write(&mut storage, opened.id, &mut node, offset as u64, &chunk) {
                Ok(512) => offset += 512,
                Err(VfsError::NoSpace) => break,
                other => panic!("unexpected write result {other:?}"),
            }
        }
        assert_eq!(offset, ((CONTENT_CAPACITY - initial) / 512) * 512);
        assert_eq!(node.size as usize, offset);
    }

    #[test]
    fn file_creation_reuses_a_freed_slot() {
        let mut storage = mounted();
        let flags = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        let mut last: Option<(NodeId, u8)> = None;
        let mut index = 0u8;
        loop {
            let digit = b'0' + index;
            let path = [b'/', b't', b'm', b'p', b'/', b'f', digit];
            match open(&mut storage, ROOT, &path, flags) {
                Ok(opened) => {
                    last = Some((opened.id, digit));
                    index += 1;
                }
                Err(VfsError::NoSpace) => break,
                Err(error) => panic!("unexpected open result {error:?}"),
            }
        }
        let (id, digit) = last.expect("at least one file was created");
        let path = [b'/', b't', b'm', b'p', b'/', b'f', digit];
        assert_eq!(unlink(&mut storage, ROOT, &path, false), Ok(id));
        // 删除后计数仍为高水位，但创建应复用同一槽而不是 ENOSPC。
        let again = open(&mut storage, ROOT, b"/tmp/g1", flags).unwrap();
        assert_eq!(again.id, id);
        assert_eq!(again.node.name_bytes(), b"g1");
    }

    #[test]
    fn mkdir_creates_writable_empty_directories() {
        let mut storage = mounted();
        let a = mkdir(&mut storage, ROOT, b"/tmp/a").expect("mkdir in writable dir");
        let node = read_node(&mut storage, a).unwrap();
        assert_eq!(node.kind, NodeKind::Directory);
        assert_eq!(node.parent, resolve(&mut storage, ROOT, b"tmp").unwrap());
        assert!(node.writable());
        assert_eq!(mkdir(&mut storage, ROOT, b"/tmp/a"), Err(VfsError::Exist));
        assert_eq!(mkdir(&mut storage, ROOT, b"/etc/x"), Err(VfsError::Access));
        assert_eq!(mkdir(&mut storage, ROOT, b"/nope/x"), Err(VfsError::NoEnt));
        assert_eq!(
            mkdir(&mut storage, ROOT, b"/etc/motd/x"),
            Err(VfsError::NotDir)
        );
        assert_eq!(mkdir(&mut storage, ROOT, b"/"), Err(VfsError::Exist));
        assert_eq!(mkdir(&mut storage, ROOT, b"."), Err(VfsError::Exist));
        assert!(mkdir(&mut storage, ROOT, b"/tmp/a/b").is_ok());
    }

    #[test]
    fn unlink_and_rmdir_enforce_kind_empty_and_parent_rights() {
        let mut storage = mounted();
        let a = mkdir(&mut storage, ROOT, b"/tmp/a").unwrap();
        let f = open(
            &mut storage,
            ROOT,
            b"/tmp/a/f",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/a/f", false), Ok(f.id));
        assert_eq!(
            resolve(&mut storage, ROOT, b"/tmp/a/f"),
            Err(VfsError::NoEnt)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"/tmp/a/f", false),
            Err(VfsError::NoEnt)
        );
        // 非空目录不能 rmdir；目录不能用 unlink 删除；普通文件不能 rmdir。
        let _g = open(
            &mut storage,
            ROOT,
            b"/tmp/a/g",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        assert_eq!(
            unlink(&mut storage, ROOT, b"/tmp/a", true),
            Err(VfsError::Access)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"/tmp/a", false),
            Err(VfsError::Access)
        );
        let f2 = open(
            &mut storage,
            ROOT,
            b"/tmp/a/f2",
            OpenFlags {
                access: 2,
                create: true,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        // 普通文件不能 rmdir；只读父目录上的任何删除都是 EACCES。
        assert_eq!(
            unlink(&mut storage, ROOT, b"/tmp/a/f2", true),
            Err(VfsError::NotDir)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"/etc/motd", true),
            Err(VfsError::Access)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"/etc/motd", false),
            Err(VfsError::Access)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"/", true),
            Err(VfsError::Access)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b".", false),
            Err(VfsError::Invalid)
        );
        assert_eq!(
            unlink(&mut storage, ROOT, b"..", true),
            Err(VfsError::Invalid)
        );
        // 清空后可删除。
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/a/g", false), Ok(_g.id));
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/a/f2", false), Ok(f2.id));
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/a", true), Ok(a));
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/a"), Err(VfsError::NoEnt));
    }

    #[test]
    fn deleted_nodes_are_skipped_and_reused() {
        let mut storage = mounted();
        let tmp = resolve(&mut storage, ROOT, b"tmp").unwrap();
        let a = mkdir(&mut storage, ROOT, b"/tmp/a").unwrap();
        assert_eq!(dir_entry_count(&mut storage, tmp), Ok(3));
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/a", true), Ok(a));
        assert_eq!(dir_entry_count(&mut storage, tmp), Ok(2));
        // 已删除节点不出现在目录枚举里。
        for index in 0..dir_entry_count(&mut storage, tmp).unwrap() {
            let entry = dir_entry(&mut storage, tmp, index).unwrap().unwrap();
            assert_ne!(entry.name_bytes(), b"a");
        }
        // 槽位被复用，且新名字覆盖旧名字。
        let b = mkdir(&mut storage, ROOT, b"/tmp/b").unwrap();
        assert_eq!(b, a);
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/a"), Err(VfsError::NoEnt));
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/b"), Ok(b));
    }

    #[test]
    fn symlinks_create_read_and_resolve() {
        let mut storage = mounted();
        // 相对目标以链接父目录为基准（`/tmp/../etc/motd`）。
        symlink(&mut storage, ROOT, b"../etc/motd", b"/tmp/rel").unwrap();
        assert_eq!(read_file(&mut storage, b"/tmp/rel"), b"welcome\n");
        assert_eq!(read_file(&mut storage, b"tmp/rel"), b"welcome\n");
        // 绝对目标从根开始。
        symlink(&mut storage, ROOT, b"/etc/hostname", b"/tmp/abs").unwrap();
        assert_eq!(read_file(&mut storage, b"tmp/abs"), b"prima\n");
        // 中间分量是链接时按目标目录继续。
        symlink(&mut storage, ROOT, b"/etc", b"/tmp/dir").unwrap();
        assert_eq!(read_file(&mut storage, b"/tmp/dir/motd"), b"welcome\n");
        // readlink 返回目标原文；末项不跟随，前缀照常跟随。
        let id = resolve_link(&mut storage, ROOT, b"/tmp/rel").unwrap();
        let node = read_node(&mut storage, id).unwrap();
        assert_eq!(node.kind, NodeKind::Symlink);
        let followed = resolve(&mut storage, ROOT, b"/tmp/rel").unwrap();
        assert_eq!(
            read_node(&mut storage, followed).unwrap().kind,
            NodeKind::File
        );
        let target = read_link(&mut storage, &node).unwrap();
        assert_eq!(&target[..node.size as usize], b"../etc/motd");
        // 重复创建与非法目标确定失败。
        assert_eq!(
            symlink(&mut storage, ROOT, b"etc/motd", b"/tmp/rel"),
            Err(VfsError::Exist)
        );
        assert_eq!(
            symlink(&mut storage, ROOT, b"", b"/tmp/empty"),
            Err(VfsError::Invalid)
        );
        // 只读父目录拒绝创建。
        assert_eq!(
            symlink(&mut storage, ROOT, b"etc/motd", b"/usr/l"),
            Err(VfsError::Access)
        );
    }

    #[test]
    fn symlink_loops_and_dangling_targets_fail_deterministically() {
        let mut storage = mounted();
        symlink(&mut storage, ROOT, b"loop", b"/tmp/loop").unwrap();
        assert_eq!(
            resolve(&mut storage, ROOT, b"/tmp/loop"),
            Err(VfsError::Loop)
        );
        symlink(&mut storage, ROOT, b"missing", b"/tmp/dangling").unwrap();
        assert_eq!(
            resolve(&mut storage, ROOT, b"/tmp/dangling"),
            Err(VfsError::NoEnt)
        );
        // 互相指向的两级环在深度上限处确定失败。
        symlink(&mut storage, ROOT, b"s1", b"/tmp/s0").unwrap();
        symlink(&mut storage, ROOT, b"s0", b"/tmp/s1").unwrap();
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/s0"), Err(VfsError::Loop));
    }

    #[test]
    fn rename_moves_and_replaces_files() {
        let mut storage = mounted();
        let create = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        let mut opened = open(&mut storage, ROOT, b"/tmp/f", create).unwrap();
        write(&mut storage, opened.id, &mut opened.node, 0, b"abc").unwrap();
        assert_eq!(rename(&mut storage, ROOT, b"/tmp/f", b"/tmp/g"), Ok(()));
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/f"), Err(VfsError::NoEnt));
        assert_eq!(read_file(&mut storage, b"/tmp/g"), b"abc");
        // 覆盖已有文件：目标槽位被释放，内容来自源文件。
        let mut other = open(&mut storage, ROOT, b"/tmp/h", create).unwrap();
        write(&mut storage, other.id, &mut other.node, 0, b"xyz").unwrap();
        assert_eq!(rename(&mut storage, ROOT, b"/tmp/g", b"/tmp/h"), Ok(()));
        assert_eq!(read_file(&mut storage, b"/tmp/h"), b"abc");
        assert_eq!(resolve(&mut storage, ROOT, b"/tmp/g"), Err(VfsError::NoEnt));
        assert_eq!(rename(&mut storage, ROOT, b"/tmp/h", b"/tmp/h"), Ok(()));
    }

    #[test]
    fn rename_moves_directories_and_rejects_subtree() {
        let mut storage = mounted();
        let create = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        mkdir(&mut storage, ROOT, b"/tmp/d").unwrap();
        let mut inner = open(&mut storage, ROOT, b"/tmp/d/inner", create).unwrap();
        write(&mut storage, inner.id, &mut inner.node, 0, b"in").unwrap();
        assert_eq!(rename(&mut storage, ROOT, b"/tmp/d", b"/tmp/e"), Ok(()));
        assert_eq!(read_file(&mut storage, b"/tmp/e/inner"), b"in");
        // 目录不能移进自己的子树。
        mkdir(&mut storage, ROOT, b"/tmp/e/sub").unwrap();
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/e", b"/tmp/e/sub"),
            Err(VfsError::Invalid)
        );
    }

    #[test]
    fn rename_enforces_replace_and_error_rules() {
        let mut storage = mounted();
        let create = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        // 文件不能覆盖目录。
        let mut file = open(&mut storage, ROOT, b"/tmp/f", create).unwrap();
        write(&mut storage, file.id, &mut file.node, 0, b"f").unwrap();
        mkdir(&mut storage, ROOT, b"/tmp/y").unwrap();
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/f", b"/tmp/y"),
            Err(VfsError::NotDir)
        );
        // 目录不能覆盖非空目录；目标清空后可以替换。
        let k = open(&mut storage, ROOT, b"/tmp/y/k", create).unwrap();
        mkdir(&mut storage, ROOT, b"/tmp/x").unwrap();
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/x", b"/tmp/y"),
            Err(VfsError::Access)
        );
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/f", false), Ok(file.id));
        assert_eq!(unlink(&mut storage, ROOT, b"/tmp/y/k", false), Ok(k.id));
        mkdir(&mut storage, ROOT, b"/tmp/z").unwrap();
        assert_eq!(rename(&mut storage, ROOT, b"/tmp/z", b"/tmp/y"), Ok(()));
        // 冻结错误集。
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/none", b"/tmp/n2"),
            Err(VfsError::NoEnt)
        );
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/x", b"/usr/x"),
            Err(VfsError::Access)
        );
        // 冻结错误集：文件带尾随斜杠不能重命名（目录可以）。
        open(&mut storage, ROOT, b"/tmp/q", create).unwrap();
        assert_eq!(
            rename(&mut storage, ROOT, b"/tmp/q/", b"/tmp/q2"),
            Err(VfsError::NotDir)
        );
    }

    #[test]
    fn chmod_updates_mode_and_access_checks() {
        let mut storage = mounted();
        let create = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        let mut file = open(&mut storage, ROOT, b"/tmp/p", create).unwrap();
        assert!(file.node.writable());
        assert_eq!(file.node.mode, 0o666);
        assert_eq!(chmod(&mut storage, ROOT, b"/tmp/p", 0o444), Ok(()));
        let node = read_node(&mut storage, file.id).unwrap();
        assert!(!node.writable());
        assert!(node.readable());
        let write_flags = OpenFlags {
            access: 1,
            ..OpenFlags::default()
        };
        assert_eq!(
            open(&mut storage, ROOT, b"/tmp/p", write_flags),
            Err(VfsError::Access)
        );
        // 空路径 + start 节点等价于 fchmod。
        assert_eq!(chmod(&mut storage, file.id, b"", 0o600), Ok(()));
        assert!(read_node(&mut storage, file.id).unwrap().writable());
        // 目录 mode 决定创建/删除权限。
        assert_eq!(chmod(&mut storage, ROOT, b"/tmp", 0o555), Ok(()));
        assert_eq!(
            open(&mut storage, ROOT, b"/tmp/q", create),
            Err(VfsError::Access)
        );
        assert_eq!(chmod(&mut storage, ROOT, b"/tmp", 0o777), Ok(()));
        assert_eq!(
            open(&mut storage, ROOT, b"/tmp/q", create).unwrap().id,
            file.id + 1
        );
        // 非法 mode/缺失路径确定失败。
        assert_eq!(
            chmod(&mut storage, ROOT, b"/tmp/p", 0o10000),
            Err(VfsError::Invalid)
        );
        assert_eq!(
            chmod(&mut storage, ROOT, b"/tmp/none", 0o666),
            Err(VfsError::NoEnt)
        );
        let _ = write(&mut storage, file.id, &mut file.node, 0, b"");
    }

    #[test]
    fn deleted_content_is_reclaimed_by_compaction() {
        let mut storage = mounted();
        let create = OpenFlags {
            access: 2,
            create: true,
            ..OpenFlags::default()
        };
        let initial = content_used(&mut storage).unwrap();
        let mut first = open(&mut storage, ROOT, b"/tmp/first", create).unwrap();
        write(&mut storage, first.id, &mut first.node, 0, &[b'a'; 1200]).unwrap();
        let mut second = open(&mut storage, ROOT, b"/tmp/second", create).unwrap();
        write(&mut storage, second.id, &mut second.node, 0, &[b'b'; 900]).unwrap();
        assert_eq!(content_used(&mut storage), Ok(initial + 1200 + 900));
        assert_eq!(
            unlink(&mut storage, ROOT, b"/tmp/first", false),
            Ok(first.id)
        );
        // 删除后 `second` 的 `data` 仍靠后；下一次增长触发压实并把内容左移。
        let grown = write(&mut storage, second.id, &mut second.node, 0, &[b'c'; 1900]);
        assert_eq!(grown, Ok(1900));
        assert_eq!(read_file(&mut storage, b"/tmp/second"), vec![b'c'; 1900]);
        assert_eq!(content_used(&mut storage), Ok(1900 + initial));
    }
}
