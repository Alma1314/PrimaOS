//! 用户态 RAM 文件系统与 OS ABI 无关的打开文件表。
//!
//! 本模块不接触任何 syscall 或 Capability：它把一段字节存储解释成含目录、文件、链接和 null 设备的
//! 小根文件系统。存储由 [`Storage`] 提供，ring3 存储服务用已映射的 MemorySegment 实现它，
//! host 单测用内存数组实现它；同一份解析/打开/读写/目录逻辑在两端得到相同结果。
//!
//! 单节点名称上限 16 字节、路径分量上限 6——足够覆盖首版 rootfs 与 `/usr/lib/version`
//! 这类多级路径，同时把解析栈帧压到服务端点的 1 页栈预算内。
//!
//! 镜像布局（从存储偏移 0 起）：
//!
//! ```text
//! 0..16      header：magic("PVFS") | version=3 | node_count | data_used
//! 16..1168   32 个 36 字节的节点条目（名称、父节点、类型、引用数、权限、大小、偏移）
//! 1168..4088 文件内容区（2920 字节）
//! ```
//!
//! 节点 0 固定为根目录。路径解析在用户态完成，支持绝对/相对路径、`.`、`..`、重复 `/`、
//! 尾随 `/` 与 `openat(dirfd, ...)` 的起始目录；错误保持为语义枚举，由服务/personality
//! 映射到各自的状态码。符号链接深度错误沿用 M13a 的 Loop。
//!
//! [`OpenFiles`] 管理打开对象、访问模式与游标；Linux fd、errno、stat/dirent 布局仍由
//! 适配层管理。目录名与打开对象分别回收；unlink/rename 不使已打开对象指向新文件。
//! **边界**：固定容量、单服务串行调用；无硬链接、跨启动持久化、挂载路由或文件映射。

mod handles;
pub use handles::{FileInfo, HandleId, OpenFiles, SeekFrom};

/// 单个节点的字节数。
/// M13b：节点 32 → 36 B，新增 16 位 mode。
pub const NODE_BYTES: usize = 36;
/// 节点条目上限（同时决定内容区起点，保证已分配的内容偏移稳定）。
/// M13a：节点槽 16 → 32。符号链接/rename 用例在 16 槽下会因临时节点耗尽返回
/// `ENOSPC`；当前容量按 32×36 B 节点表由 `CONTENT_CAPACITY` 重算。
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
/// v3 在节点保留字节中记录打开引用；旧镜像须显式重新 format，不做隐式迁移。
pub const VERSION: u16 = 3;

/// 根节点编号。
pub const ROOT: NodeId = 0;

/// 已删除节点的 `parent` 哨兵。`MAX_NODES` 远小于 255，故不会与真实父节点冲突。
/// 只有最后一个打开引用释放后，节点槽和内容才可复用。
pub const FREE_PARENT: NodeId = 0xff;
/// 已取消目录链接、仍由打开对象引用的节点；不参与路径查找，不可复用。
const ORPHAN_PARENT: NodeId = 0xfe;

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
    /// 打开句柄不存在，或访问模式不允许此操作。
    BadHandle,
    /// 打开文件表已满（由 personality 选择 errno/status 映射）。
    TooManyOpen,
}

/// 字节存储抽象。所有偏移都相对镜像起点（不含 SPEC-112 的状态字）。
pub trait Storage {
    /// 读 `out.len()` 字节；越界/不可读返回 `false`。
    fn read(&mut self, offset: usize, out: &mut [u8]) -> bool;
    /// 完整写入 `data`，或保持存储不变并返回 `false`；不得以 false 报告部分写入。
    /// 此约束不承诺持久化、进程崩溃原子性或多个 write 组成的事务。
    fn write(&mut self, offset: usize, data: &[u8]) -> bool;
}

/// 一个 VFS 节点。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: NodeId,
    /// RAM 镜像内打开对象的引用数；由 OpenFiles 管理，禁止复制表来复制引用。
    open_refs: u8,
    /// 权限位（低 9 位）。单用户模型只看 owner 读写位。
    pub mode: u16,
    pub size: u16,
    /// 内容区内的起始偏移（文件或符号链接）。
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

/// 访问意图；各 personality/协议在边界处解码自己的 flag 数值。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccessMode {
    #[default]
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl AccessMode {
    pub const fn allows_read(self) -> bool {
        !matches!(self, Self::WriteOnly)
    }
    pub const fn allows_write(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

/// `open` 的规范化意图；不接受任何 OS 的原始 flag 位。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenFlags {
    pub access: AccessMode,
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
    if count == 0 || count as usize > MAX_NODES || data_used as usize > CONTENT_CAPACITY {
        return Err(VfsError::Unformatted);
    }
    Ok(Header { count, data_used })
}

fn encode_node(node: &Node) -> [u8; NODE_BYTES] {
    let mut bytes = [0u8; NODE_BYTES];
    bytes[0] = node.name_len;
    bytes[1] = node.kind.as_raw();
    bytes[2] = node.parent;
    bytes[3] = node.open_refs;
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
    let data = u16::from_le_bytes([bytes[6], bytes[7]]);
    let size = u16::from_le_bytes([bytes[4], bytes[5]]);
    if data as usize + size as usize > CONTENT_CAPACITY
        || (bytes[2] as usize >= MAX_NODES && bytes[2] != FREE_PARENT && bytes[2] != ORPHAN_PARENT)
    {
        return Err(VfsError::Unformatted);
    }
    let mut name = [0u8; MAX_NAME];
    name[..name_len as usize].copy_from_slice(&bytes[10..10 + name_len as usize]);
    Ok(Node {
        kind,
        parent: bytes[2],
        open_refs: bytes[3],
        mode: u16::from_le_bytes([bytes[8], bytes[9]]),
        size,
        data,
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

/// 与镜像布局无关的当前元数据；已回收槽不再作为对象返回。
pub fn metadata<S: Storage>(storage: &mut S, id: NodeId) -> Result<FileInfo, VfsError> {
    let node = read_node(storage, id)?;
    if is_free(&node) {
        return Err(VfsError::NoEnt);
    }
    Ok(FileInfo::new(id, &node))
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
const TINY_ELF: &[u8] = &tiny_elf();
const fn tiny_elf() -> [u8; 530] {
    let mut bytes = [0; 530];
    let fields = [
        (0, 127),
        (1, b'E'),
        (2, b'L'),
        (3, b'F'),
        (4, 2),
        (5, 1),
        (6, 1),
        (16, 2),
        (18, 62),
        (20, 1),
        (25, 2),
        (26, 64),
        (32, 64),
        (52, 64),
        (54, 56),
        (56, 2),
        (64, 1),
        (68, 5),
        (82, 64),
        (90, 64),
        (96, 18),
        (97, 2),
        (105, 16),
        (113, 16),
        (120, 4),
        (128, 176),
        (152, 44),
    ];
    let mut i = 0;
    while i < fields.len() {
        bytes[fields[i].0] = fields[i].1;
        i += 1;
    }
    let note = prima_abi::program_contract::note(prima_abi::program_contract::POSIX_STATIC, 1);
    i = 0;
    while i < note.len() {
        let word = note[i].to_le_bytes();
        let mut j = 0;
        while j < 4 {
            bytes[176 + i * 4 + j] = word[j];
            j += 1;
        }
        i += 1;
    }
    let code = [
        0x48, 0x8b, 0x04, 0x24, 0x48, 0x89, 0xc7, 0x48, 0xc7, 0xc0, 54, 0, 0, 0, 0x0f, 0x05, 0xeb,
        0xfe,
    ];
    i = 0;
    while i < code.len() {
        bytes[512 + i] = code[i];
        i += 1;
    }
    bytes
}

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

/// 挂载有效镜像；只有全零头的新存储才自动格式化。旧版本、损坏与 I/O 错误不触发覆盖。
pub fn mount<S: Storage>(storage: &mut S) -> Result<(), VfsError> {
    let mut bytes = [0u8; HEADER_BYTES];
    read_exact(storage, 0, &mut bytes)?;
    if bytes == [0u8; HEADER_BYTES] {
        format(storage)
    } else {
        read_header(storage).map(|_| ())
    }
}

/// 显式破坏性格式化。调用前必须丢弃该存储所有打开文件表；不承诺故障原子性。
pub fn format<S: Storage>(storage: &mut S) -> Result<(), VfsError> {
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
            open_refs: 0,
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

/// 服务启动且所有旧打开表已丢弃时，回收旧会话引用。禁止对仍有活动表的存储调用。
/// 这不是句柄迁移：存活目录项和内容保留，已 unlink 的对象被回收。
pub fn recover_open_files<S: Storage>(storage: &mut S) -> Result<(), VfsError> {
    let header = read_header(storage)?;
    for id in 0..header.count {
        let mut node = read_node(storage, id)?;
        if node.open_refs == 0 && node.parent != ORPHAN_PARENT {
            continue;
        }
        node.open_refs = 0;
        if node.parent == ORPHAN_PARENT {
            node.parent = FREE_PARENT;
            node.size = 0;
            node.data = 0;
        }
        write_node(storage, id, &node)?;
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
        if path.is_empty() || path.len() > MAX_PATH || path.contains(&0) {
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
    let mut require_directory = parsed.trailing_slash;
    while index < count {
        let component = pending[index];
        index += 1;
        if read_node(storage, current)?.kind != NodeKind::Directory {
            return Err(VfsError::NotDir);
        }
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
        if total > PENDING_COMPONENTS {
            return Err(VfsError::Invalid);
        }
        if remaining == 0 && target.trailing_slash {
            require_directory = true;
        }
        // 目标分量插到剩余分量之前；相对目标以链接父目录为基准。
        pending.copy_within(index..count, target.count as usize);
        pending[..target.count as usize]
            .copy_from_slice(&target.components[..target.count as usize]);
        index = 0;
        count = total;
        current = if target.absolute { ROOT } else { node.parent };
    }
    if require_directory && read_node(storage, current)?.kind != NodeKind::Directory {
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
    let parsed = ParsedPath::parse(path)?;
    // 独占创建按目录项判断，不能先跟随末项链接（包括悬空链接）。
    if flags.create
        && flags.exclusive
        && let Some(last) = parsed.last()
    {
        let parent = walk(storage, start, &parsed.prefix())?;
        if lookup(storage, parent, last.as_bytes())?.is_some() {
            return Err(VfsError::Exist);
        }
    }
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
    if parsed.trailing_slash {
        return Err(VfsError::NotDir);
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
        open_refs: 0,
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
    if node.open_refs == u8::MAX {
        return Err(VfsError::TooManyOpen);
    }
    if flags.create && flags.exclusive {
        return Err(VfsError::Exist);
    }
    if flags.directory && node.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    if flags.access.allows_read() && node.kind != NodeKind::Null && !node.readable() {
        return Err(VfsError::Access);
    }
    if flags.access.allows_write() {
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

/// 解除目录链接；已打开的节点保留内容，最后一次 close 才回收。
fn free_node<S: Storage>(storage: &mut S, id: NodeId) -> Result<(), VfsError> {
    let mut node = read_node(storage, id)?;
    // 打开目录暂不允许删除或被替换，保留其 `..` 与 dirfd 语义。
    node.parent = if node.open_refs == 0 {
        FREE_PARENT
    } else {
        ORPHAN_PARENT
    };
    node.name_len = 0;
    node.name = [0u8; MAX_NAME];
    if node.open_refs == 0 {
        node.size = 0;
        node.data = 0;
    }
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
        open_refs: 0,
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
/// 根、`.`、`..` 与只读父目录确定失败。打开目录暂不允许删除，普通文件可延迟回收。
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
        if node.open_refs != 0 {
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
        open_refs: 0,
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
    // ponytail: 最多 32 项的插入排序；节点上限显著扩大后再换索引，当前无需分配。
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
    if !read_node(storage, from_dir)?.writable() {
        return Err(VfsError::Access);
    }
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
            if target.open_refs != 0 {
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

/// 写文件；调用方负责已打开对象的 access 检查，mode 在 open 时检查。
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
    if data.is_empty() {
        return Ok(0);
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
        // seek/truncate 后留下的空洞必须清零，不能暴露旧文件内容。
        let zero = [0u8; 32];
        let mut position = node.size as usize;
        while position < offset as usize {
            let count = zero.len().min(offset as usize - position);
            write_all(
                storage,
                CONTENT_OFFSET + node.data as usize + position,
                &zero[..count],
            )?;
            position += count;
        }
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
        // 空文件的 data 只是旧位置提示；邻居增长或压实后可能已覆盖它。
        // 此时不能原地写入，须走压实/搬迁路径重新取得独占区间。
        if start <= data && data < start + node.size as usize {
            return Ok(0);
        }
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
        if is_free(&node) || node.size == 0 {
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

/// 调整普通文件长度；访问权限由打开对象检查，与 [`write`] 一致。
/// 增长复用稀疏写的容量检查、搬迁和清零，不改变任何打开对象的游标。
pub fn resize<S: Storage>(
    storage: &mut S,
    id: NodeId,
    node: &mut Node,
    size: u64,
) -> Result<(), VfsError> {
    if node.kind != NodeKind::File {
        return Err(VfsError::Invalid);
    }
    if size > CONTENT_CAPACITY as u64 {
        return Err(VfsError::NoSpace);
    }
    if size > node.size as u64 {
        write(storage, id, node, size - 1, &[0])?;
    } else if size != node.size as u64 {
        node.size = size as u16;
        write_node(storage, id, node)?;
    }
    Ok(())
}

/// 把文件截断到 `size`；保留原节点级接口的权限及只缩小契约。
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

/// 内容区分配高水位（供容量诊断）；压实后等于实际存活内容字节数。
pub fn content_used<S: Storage>(storage: &mut S) -> Result<usize, VfsError> {
    Ok(header_data_used(storage)? as usize)
}

#[cfg(test)]
mod tests;
