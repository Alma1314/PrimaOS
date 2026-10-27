//! 服务级打开对象。没有 syscall、errno、Linux fd 号或应用结构体布局。
//! 同一存储的多个表须由服务串行使用；表不实现 Clone，释放须显式 close/close_all。

use super::{self as fs, AccessMode, NodeId, NodeKind, OpenFlags, Storage, VfsError};

/// 表内句柄；仅是服务内部索引，不是 Capability，也不提供跨客户端授权。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandleId(pub usize);

/// 不包含磁盘/镜像布局的元数据。personality 自行转换 stat/dirent/status。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileInfo {
    pub id: NodeId,
    pub kind: NodeKind,
    pub size: u64,
    pub mode: u16,
}

impl FileInfo {
    pub(super) fn new(id: NodeId, node: &fs::Node) -> Self {
        Self {
            id,
            kind: node.kind,
            size: node.size as u64,
            mode: node.mode,
        }
    }
}

/// 相对游标的有符号位移；不依赖某个 OS 的 whence 数值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekFrom {
    Start(i64),
    Current(i64),
    End(i64),
}

#[derive(Clone, Copy)]
struct OpenFile {
    node: NodeId,
    offset: u64,
    access: AccessMode,
    append: bool,
    owners: u32,
}

/// 固定容量、无分配的打开文件表。一个表项对应一个打开对象；dup 的 fd 别名由
/// LibOS 管理并继续引用同一 HandleId。独立 open 得到独立游标。
pub struct OpenFiles<const N: usize> {
    files: [Option<OpenFile>; N],
    owner: u8,
}

impl<const N: usize> Default for OpenFiles<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> OpenFiles<N> {
    pub const fn new() -> Self {
        Self {
            files: [None; N],
            owner: 0,
        }
    }

    /// Select an authenticated service-side Process/reservation slot.
    pub fn set_owner(&mut self, owner: u8) -> Result<(), VfsError> {
        if owner >= 32 {
            return Err(VfsError::Invalid);
        }
        self.owner = owner;
        Ok(())
    }

    /// Reserve every inherited OFD before publishing a fork child. The VFS
    /// open reference counts OFDs, while these bits count Process holders.
    pub fn fork_owner(&mut self, from: u8, to: u8) -> Result<(), VfsError> {
        if from >= 32 || to >= 32 || from == to {
            return Err(VfsError::Invalid);
        }
        if self
            .files
            .iter()
            .flatten()
            .any(|f| f.owners & (1 << to) != 0)
        {
            return Err(VfsError::Invalid);
        }
        for file in self.files.iter_mut().flatten() {
            if file.owners & (1 << from) != 0 {
                file.owners |= 1 << to;
            }
        }
        Ok(())
    }

    pub fn close_owner<S: Storage>(&mut self, storage: &mut S, owner: u8) -> Result<(), VfsError> {
        let previous = self.owner;
        self.set_owner(owner)?;
        let result = self.close_all(storage);
        self.owner = previous;
        result
    }

    pub fn len(&self) -> usize {
        self.files.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, id: HandleId) -> Result<OpenFile, VfsError> {
        self.files
            .get(id.0)
            .copied()
            .flatten()
            .filter(|file| file.owners & (1 << self.owner) != 0)
            .ok_or(VfsError::BadHandle)
    }

    /// 先预留表容量，再调用可能 create/truncate 的后端；表满无文件系统副作用。
    pub fn open<S: Storage>(
        &mut self,
        storage: &mut S,
        start: NodeId,
        path: &[u8],
        flags: OpenFlags,
    ) -> Result<(HandleId, FileInfo), VfsError> {
        let index = self
            .files
            .iter()
            .position(Option::is_none)
            .ok_or(VfsError::TooManyOpen)?;
        let opened = fs::open(storage, start, path, flags)?;
        let mut node = opened.node;
        node.open_refs = node.open_refs.checked_add(1).ok_or(VfsError::TooManyOpen)?;
        fs::write_node(storage, opened.id, &node)?;
        self.files[index] = Some(OpenFile {
            node: opened.id,
            offset: 0,
            access: flags.access,
            append: flags.append,
            owners: 1 << self.owner,
        });
        Ok((HandleId(index), FileInfo::new(opened.id, &node)))
    }

    pub fn info<S: Storage>(&self, storage: &mut S, id: HandleId) -> Result<FileInfo, VfsError> {
        let file = self.get(id)?;
        fs::metadata(storage, file.node)
    }

    pub fn read<S: Storage>(
        &mut self,
        storage: &mut S,
        id: HandleId,
        out: &mut [u8],
    ) -> Result<usize, VfsError> {
        let file = self.get(id)?;
        let node = fs::read_node(storage, file.node)?;
        if !file.access.allows_read() || node.kind == NodeKind::Directory {
            return Err(VfsError::BadHandle);
        }
        let count = fs::read(storage, &node, file.offset, out)?;
        self.files[id.0].as_mut().unwrap().offset += count as u64;
        Ok(count)
    }

    /// 现有原生目录记录流；Linux dirent 布局在 C 适配层生成。
    pub fn read_directory_bytes<S: Storage>(
        &mut self,
        storage: &mut S,
        id: HandleId,
        out: &mut [u8],
    ) -> Result<usize, VfsError> {
        let file = self.get(id)?;
        if !file.access.allows_read() {
            return Err(VfsError::BadHandle);
        }
        let count = fs::dir_bytes(storage, file.node, file.offset, out)?;
        self.files[id.0].as_mut().unwrap().offset += count as u64;
        Ok(count)
    }

    pub fn write<S: Storage>(
        &mut self,
        storage: &mut S,
        id: HandleId,
        bytes: &[u8],
    ) -> Result<usize, VfsError> {
        let file = self.get(id)?;
        let mut node = fs::read_node(storage, file.node)?;
        if !file.access.allows_write() || node.kind == NodeKind::Directory {
            return Err(VfsError::BadHandle);
        }
        if node.kind == NodeKind::Null {
            return Ok(bytes.len());
        }
        let offset = if file.append {
            node.size as u64
        } else {
            file.offset
        };
        let count = fs::write(storage, file.node, &mut node, offset, bytes)?;
        if count != 0 {
            self.files[id.0].as_mut().unwrap().offset = offset + count as u64;
        }
        Ok(count)
    }

    pub fn seek<S: Storage>(
        &mut self,
        storage: &mut S,
        id: HandleId,
        from: SeekFrom,
    ) -> Result<u64, VfsError> {
        let file = self.get(id)?;
        let node = fs::read_node(storage, file.node)?;
        let (end, limit) = match node.kind {
            NodeKind::File => (node.size as u64, fs::CONTENT_CAPACITY as u64),
            NodeKind::Directory => {
                let end = fs::dir_total(storage, file.node)? as u64;
                (end, end)
            }
            NodeKind::Null => (0, 0),
            NodeKind::Symlink => return Err(VfsError::Invalid),
        };
        let (base, offset) = match from {
            SeekFrom::Start(offset) => (0, offset),
            SeekFrom::Current(offset) => (file.offset, offset),
            SeekFrom::End(offset) => (end, offset),
        };
        let next = base
            .checked_add_signed(offset)
            .filter(|next| *next <= limit)
            .ok_or(VfsError::Invalid)?;
        self.files[id.0].as_mut().unwrap().offset = next;
        Ok(next)
    }

    /// 显式偏移读取；不触及共享 OFD 游标。
    pub fn read_at<S: Storage>(
        &self,
        storage: &mut S,
        id: HandleId,
        offset: u64,
        out: &mut [u8],
    ) -> Result<usize, VfsError> {
        let file = self.get(id)?;
        if !file.access.allows_read() {
            return Err(VfsError::BadHandle);
        }
        let node = fs::read_node(storage, file.node)?;
        if node.kind != NodeKind::File {
            return Err(VfsError::Invalid);
        }
        fs::read(storage, &node, offset, out)
    }

    /// 显式偏移写入，忽略 append 且不触及共享 OFD 游标。
    pub fn write_at<S: Storage>(
        &self,
        storage: &mut S,
        id: HandleId,
        offset: u64,
        bytes: &[u8],
    ) -> Result<usize, VfsError> {
        let file = self.get(id)?;
        if !file.access.allows_write() {
            return Err(VfsError::BadHandle);
        }
        let mut node = fs::read_node(storage, file.node)?;
        if node.kind != NodeKind::File {
            return Err(VfsError::Invalid);
        }
        fs::write(storage, file.node, &mut node, offset, bytes)
    }

    /// 按打开时授予的 access 调整长度；后续 chmod 不撤销该授权。
    pub fn resize<S: Storage>(
        &self,
        storage: &mut S,
        id: HandleId,
        size: u64,
    ) -> Result<(), VfsError> {
        let file = self.get(id)?;
        if !file.access.allows_write() {
            return Err(VfsError::BadHandle);
        }
        let mut node = fs::read_node(storage, file.node)?;
        fs::resize(storage, file.node, &mut node, size)
    }

    /// 更新镜像成功后才释放表项；I/O 失败可重试，不让槽位提前复用。
    pub fn close<S: Storage>(&mut self, storage: &mut S, id: HandleId) -> Result<(), VfsError> {
        let file = self.get(id)?;
        let remaining = file.owners & !(1 << self.owner);
        if remaining != 0 {
            self.files[id.0].as_mut().unwrap().owners = remaining;
            return Ok(());
        }
        let mut node = fs::read_node(storage, file.node)?;
        node.open_refs = node.open_refs.checked_sub(1).ok_or(VfsError::Invalid)?;
        if node.parent == fs::ORPHAN_PARENT && node.open_refs == 0 {
            node.parent = fs::FREE_PARENT;
            node.size = 0;
            node.data = 0;
        }
        fs::write_node(storage, file.node, &node)?;
        self.files[id.0] = None;
        Ok(())
    }

    /// 显式会话清理；首个 I/O 错误后保留剩余表项供重试。
    pub fn close_all<S: Storage>(&mut self, storage: &mut S) -> Result<(), VfsError> {
        for index in 0..N {
            if self.files[index].is_some_and(|file| file.owners & (1 << self.owner) != 0) {
                self.close(storage, HandleId(index))?;
            }
        }
        Ok(())
    }
}
