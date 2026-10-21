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
pub const NODE_BYTES: usize = 32;
/// 节点条目上限（同时决定内容区起点，保证已分配的内容偏移稳定）。
pub const MAX_NODES: usize = 16;
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
pub const VERSION: u16 = 1;

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
}

impl NodeKind {
    /// 从线上编码解析；未知编码确定失败。
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Directory),
            1 => Some(Self::File),
            2 => Some(Self::Null),
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
    pub writable: bool,
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
    bytes[3] = u8::from(node.writable);
    bytes[4..6].copy_from_slice(&node.size.to_le_bytes());
    bytes[6..8].copy_from_slice(&node.data.to_le_bytes());
    bytes[8..8 + node.name_len as usize].copy_from_slice(node.name_bytes());
    bytes
}

fn decode_node(bytes: &[u8; NODE_BYTES]) -> Result<Node, VfsError> {
    let name_len = bytes[0];
    if name_len as usize > MAX_NAME {
        return Err(VfsError::Unformatted);
    }
    let kind = NodeKind::from_raw(bytes[1]).ok_or(VfsError::Unformatted)?;
    let mut name = [0u8; MAX_NAME];
    name[..name_len as usize].copy_from_slice(&bytes[8..8 + name_len as usize]);
    Ok(Node {
        kind,
        parent: bytes[2],
        writable: bytes[3] & 1 != 0,
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
struct InitialNode {
    name: &'static [u8],
    parent: NodeId,
    kind: NodeKind,
    writable: bool,
    content: &'static [u8],
}

/// 静态根文件系统。顺序必须保持：父节点先于子节点，节点 0 固定为根。
const INITIAL: [InitialNode; 12] = [
    InitialNode {
        name: b"",
        parent: ROOT,
        kind: NodeKind::Directory,
        writable: false,
        content: b"",
    },
    InitialNode {
        name: b"f",
        parent: ROOT,
        kind: NodeKind::File,
        writable: true,
        content: b"",
    },
    InitialNode {
        name: b"README",
        parent: ROOT,
        kind: NodeKind::File,
        writable: false,
        content: b"prima rootfs\n",
    },
    InitialNode {
        name: b"etc",
        parent: ROOT,
        kind: NodeKind::Directory,
        writable: false,
        content: b"",
    },
    InitialNode {
        name: b"motd",
        parent: 3,
        kind: NodeKind::File,
        writable: false,
        content: b"welcome\n",
    },
    InitialNode {
        name: b"hostname",
        parent: 3,
        kind: NodeKind::File,
        writable: false,
        content: b"prima\n",
    },
    InitialNode {
        name: b"usr",
        parent: ROOT,
        kind: NodeKind::Directory,
        writable: false,
        content: b"",
    },
    InitialNode {
        name: b"lib",
        parent: 6,
        kind: NodeKind::Directory,
        writable: false,
        content: b"",
    },
    InitialNode {
        name: b"version",
        parent: 7,
        kind: NodeKind::File,
        writable: false,
        content: b"1.0\n",
    },
    InitialNode {
        name: b"tmp",
        parent: ROOT,
        kind: NodeKind::Directory,
        writable: true,
        content: b"",
    },
    InitialNode {
        name: b"dev",
        parent: ROOT,
        kind: NodeKind::Directory,
        writable: false,
        content: b"",
    },
    InitialNode {
        name: b"null",
        parent: 10,
        kind: NodeKind::Null,
        writable: false,
        content: b"",
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
            writable: initial.writable,
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

fn walk<S: Storage>(
    storage: &mut S,
    start: NodeId,
    parsed: &ParsedPath,
) -> Result<NodeId, VfsError> {
    let mut current = if parsed.absolute { ROOT } else { start };
    if !parsed.absolute && read_node(storage, current)?.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    for index in 0..parsed.count as usize {
        let component = parsed.component(index);
        if component.is_dot() {
            continue;
        }
        if component.is_dot_dot() {
            current = read_node(storage, current)?.parent;
            continue;
        }
        current = lookup(storage, current, component.as_bytes())?.ok_or(VfsError::NoEnt)?;
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
    if !parent.writable {
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
        writable: true,
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
        } else if node.kind != NodeKind::File || !node.writable {
            return Err(VfsError::Access);
        }
    }
    if flags.truncate {
        if node.kind != NodeKind::File || !node.writable {
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
    if !parent.writable {
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
        writable: true,
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
    if !parent.writable {
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
    if !node.writable {
        return Err(VfsError::Access);
    }
    if offset > CONTENT_CAPACITY as u64 {
        return Err(VfsError::NoSpace);
    }
    let limit = CONTENT_CAPACITY.saturating_sub(node.data as usize);
    let Some(end) = (offset as usize).checked_add(data.len()) else {
        return Err(VfsError::NoSpace);
    };
    if end > limit {
        return Err(VfsError::NoSpace);
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
    }
    Ok(data.len())
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
    if !node.writable {
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
        assert_eq!(content_used(&mut storage), Ok(31));
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
        assert_eq!(count, 8);
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
        let mut next = 1u8;
        loop {
            let mut name = [b'/'; 6];
            name[0] = b'/';
            name[1] = b't';
            name[2] = b'm';
            name[3] = b'p';
            name[4] = b'/';
            name[5] = b'a' + (next % 20);
            let path = &name[..6];
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
        let chunk = [b'z'; 512];
        let mut offset = 0usize;
        loop {
            match write(&mut storage, opened.id, &mut node, offset as u64, &chunk) {
                Ok(512) => offset += 512,
                Err(VfsError::NoSpace) => break,
                other => panic!("unexpected write result {other:?}"),
            }
        }
        assert_eq!(offset, (CONTENT_CAPACITY / 512) * 512);
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
        assert!(node.writable);
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
}
