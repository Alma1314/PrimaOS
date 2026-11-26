//! Manager-issued proxy sessions: one authenticated LibOS service submits file
//! batches for a kernel-authenticated application incarnation. The selector in
//! a request is never authority; it must name an entry bound to the sending
//! proxy cookie and the current application cookie.

use prima_runtime::protocol::{
    SERVICE_ROOT_LINUX, SERVICE_ROOT_NATIVE, SERVICE_ROOT_SERVICE, SERVICE_ROOT_SHARED,
};
use prima_runtime::vfs::{self, NodeKind, Storage, VfsError};

pub use prima_runtime::protocol::MAX_PROXIES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Full kernel cookie of the service that submits on the app's behalf.
    pub proxy: u64,
    /// Full application Task cookie at registration.
    pub app: u64,
    /// Application Process cookie; fork ownership is per Process.
    pub process: u64,
    /// Store-side OFD owner slot.
    pub owner: u8,
    /// Root policy id fixed by the store, not by the client payload.
    pub policy: u8,
    /// Next accepted batch sequence; 1 while no batch has been accepted.
    pub next: u64,
}

#[derive(Default)]
pub struct Table {
    entries: [Option<Entry>; MAX_PROXIES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    Missing,
    Sequence,
}

impl Table {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_PROXIES],
        }
    }

    /// Bind an application incarnation to a submitting proxy. Zero cookies or
    /// the legacy manager owner slot are rejected; duplicate registrations do
    /// not consume another slot.
    pub fn register(
        &mut self,
        proxy: u64,
        app: u64,
        process: u64,
        owner: u8,
        policy: u8,
    ) -> Option<usize> {
        if proxy == 0
            || app == 0
            || process == 0
            || owner == 0
            || self
                .entries
                .iter()
                .flatten()
                .any(|e| e.proxy == proxy && e.app == app)
        {
            return None;
        }
        let index = self.entries.iter().position(Option::is_none)?;
        self.entries[index] = Some(Entry {
            proxy,
            app,
            process,
            owner,
            policy,
            next: 1,
        });
        Some(index)
    }

    /// Resolve a selector validated by the caller (the full Task cookie read
    /// through kernel metadata) against the sending proxy.
    pub fn find(&self, proxy: u64, app: u64) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.is_some_and(|e| e.proxy == proxy && e.app == app))
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)?.as_ref()
    }

    pub fn entry_mut(&mut self, index: usize) -> Option<&mut Entry> {
        self.entries.get_mut(index)?.as_mut()
    }

    /// Consume the next sequence only after the batch was accepted; a lost
    /// completion is terminal and a replay is rejected.
    pub fn consume(&mut self, index: usize, sequence: u64) -> Result<(), Reject> {
        let entry = self
            .entries
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or(Reject::Missing)?;
        if sequence == 0 || sequence != entry.next {
            return Err(Reject::Sequence);
        }
        entry.next = sequence + 1;
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> Option<Entry> {
        self.entries.get_mut(index)?.take()
    }

    pub fn remove_app(&mut self, proxy: u64, app: u64) -> Option<Entry> {
        let index = self.find(proxy, app)?;
        self.remove(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &Entry)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.as_ref().map(|entry| (index, entry)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut Entry)> {
        self.entries
            .iter_mut()
            .enumerate()
            .filter_map(|(index, entry)| entry.as_mut().map(|entry| (index, entry)))
    }
}

/// Fixture bytes exist only for the explicit shared-service policies. They are
/// created lazily so images that never register a proxy keep the legacy root.
const LINUX_FIXTURE_FILE: &[u8] = b"hello from prima\n";
const LINUX_FIXTURE_ENTRIES: [(&[u8], &[u8]); 2] = [(b"alpha", b"a\n"), (b"beta", b"b\n")];
const SHARED_NOTE: &[u8] = b"shared\n";
/// P2b-5：原生策略视图的根资源，字节与服务根 `/README` 相同。
pub const NATIVE_README: &[u8] = b"prima rootfs\n";

fn ensure_directory<S: Storage>(
    storage: &mut S,
    path: &[u8],
) -> Result<(vfs::NodeId, bool), VfsError> {
    match vfs::resolve(storage, vfs::ROOT, path) {
        Ok(id) => {
            if vfs::read_node(storage, id)?.kind != NodeKind::Directory {
                return Err(VfsError::NotDir);
            }
            Ok((id, false))
        }
        Err(VfsError::NoEnt) => {
            // The service root is read-only by contract; restore it after
            // creating the policy fixture.
            let root_mode = vfs::read_node(storage, vfs::ROOT)?.mode;
            if root_mode & 0o200 == 0 {
                vfs::chmod(storage, vfs::ROOT, b"", root_mode | 0o200)?;
            }
            let result = vfs::mkdir(storage, vfs::ROOT, path);
            let restored = vfs::chmod(storage, vfs::ROOT, b"", root_mode);
            let id = result?;
            restored?;
            Ok((id, true))
        }
        Err(error) => Err(error),
    }
}

fn write_fixture<S: Storage>(storage: &mut S, path: &[u8], bytes: &[u8]) -> Result<(), VfsError> {
    let opened = vfs::open(
        storage,
        vfs::ROOT,
        path,
        vfs::OpenFlags {
            access: vfs::AccessMode::ReadWrite,
            create: true,
            ..Default::default()
        },
    )?;
    let mut node = opened.node;
    vfs::write(storage, opened.id, &mut node, 0, bytes).map(|_| ())
}

/// Resolve the store-fixed root policy for a proxy entry. Unknown policies are
/// rejected before any data request can select them.
pub fn prepare_policy<S: Storage>(storage: &mut S, policy: u8) -> Result<vfs::View, VfsError> {
    if policy == SERVICE_ROOT_SERVICE {
        return Ok(vfs::View::ROOT);
    }
    if policy != SERVICE_ROOT_SHARED
        && policy != SERVICE_ROOT_LINUX
        && policy != SERVICE_ROOT_NATIVE
    {
        return Err(VfsError::Invalid);
    }
    let (shared, shared_new) = ensure_directory(storage, b"/shared")?;
    if shared_new {
        write_fixture(storage, b"/shared/note", SHARED_NOTE)?;
        // P2b-5 根逃逸探针：绝对与相对目标都落在各自的私有视图之外，
        // 视图解析必须夹在根内并返回 ENOENT。
        vfs::symlink(storage, vfs::ROOT, b"/linux/fixture.txt", b"/shared/escape")?;
        vfs::symlink(storage, vfs::ROOT, b"../linux/fixture.txt", b"/shared/up")?;
    }
    if policy == SERVICE_ROOT_SHARED {
        return Ok(vfs::View::new(shared));
    }
    let (root, created) = ensure_directory(
        storage,
        if policy == SERVICE_ROOT_LINUX {
            b"/linux"
        } else {
            b"/native"
        },
    )?;
    if policy == SERVICE_ROOT_NATIVE && created {
        write_fixture(storage, b"/native/README", NATIVE_README)?;
    }
    if policy == SERVICE_ROOT_LINUX && created {
        write_fixture(storage, b"/linux/fixture.txt", LINUX_FIXTURE_FILE)?;
        ensure_directory(storage, b"/linux/fixturedir")?;
        for (name, bytes) in LINUX_FIXTURE_ENTRIES {
            let mut path = [0u8; 32];
            let prefix = b"/linux/fixturedir/";
            path[..prefix.len()].copy_from_slice(prefix);
            path[prefix.len()..prefix.len() + name.len()].copy_from_slice(name);
            write_fixture(storage, &path[..prefix.len() + name.len()], bytes)?;
        }
        // Match the Linux personality's read-only fixture root. Children keep
        // the mode mkdir assigned.
        vfs::chmod(storage, vfs::ROOT, b"/linux", 0o555)?;
    }
    Ok(vfs::View::shared(root, shared))
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROXY: u64 = (4 << 32) | 7;
    const APP: u64 = (5 << 32) | 8;

    #[test]
    fn registration_rejects_forgery_duplicates_and_legacy_owner_slot() {
        let mut table = Table::new();
        assert_eq!(table.register(0, APP, APP, 1, 1), None);
        assert_eq!(table.register(PROXY, 0, APP, 1, 1), None);
        assert_eq!(table.register(PROXY, APP, APP, 0, 1), None);
        let index = table.register(PROXY, APP, APP, 3, 1).unwrap();
        assert_eq!(table.register(PROXY, APP, APP, 4, 2), None);
        assert_eq!(table.entry(index).unwrap().owner, 3);
        assert_eq!(table.find(PROXY, APP), Some(index));
        assert_eq!(table.find(PROXY, APP + (1 << 32)), None);
        assert_eq!(table.find(PROXY + (1 << 32), APP), None);
        assert_eq!(table.find(APP, APP), None);
    }

    #[test]
    fn replay_jump_and_removed_entries_are_rejected_and_slots_are_reused() {
        let mut table = Table::new();
        let index = table.register(PROXY, APP, APP, 2, 1).unwrap();
        assert_eq!(table.consume(index, 1), Ok(()));
        assert_eq!(table.consume(index, 1), Err(Reject::Sequence));
        assert_eq!(table.consume(index, 3), Err(Reject::Sequence));
        assert_eq!(table.consume(index, 0), Err(Reject::Sequence));
        assert_eq!(table.consume(index, 2), Ok(()));
        let removed = table.remove_app(PROXY, APP).unwrap();
        assert_eq!(removed.next, 3);
        assert_eq!(table.consume(index, 3), Err(Reject::Missing));
        assert_eq!(
            table.register(PROXY, APP + (1 << 32), APP, 4, 1),
            Some(index)
        );
        assert_eq!(table.entry(index).unwrap().next, 1);
    }

    #[test]
    fn bounded_table_refuses_ninth_entry_without_displacing_others() {
        let mut table = Table::new();
        for slot in 0..MAX_PROXIES {
            let app = ((slot as u64 + 5) << 32) | 9;
            assert_eq!(
                table.register(PROXY, app, app, slot as u8 + 1, 1),
                Some(slot)
            );
        }
        let extra = ((MAX_PROXIES as u64 + 5) << 32) | 9;
        assert_eq!(table.register(PROXY, extra, extra, 9, 1), None);
        assert_eq!(table.iter().count(), MAX_PROXIES);
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    struct Ram([u8; 4088]);
    impl Storage for Ram {
        fn read(&mut self, offset: usize, out: &mut [u8]) -> bool {
            let Some(source) = self.0.get(offset..offset.saturating_add(out.len())) else {
                return false;
            };
            out.copy_from_slice(source);
            true
        }
        fn write(&mut self, offset: usize, data: &[u8]) -> bool {
            let Some(target) = self.0.get_mut(offset..offset.saturating_add(data.len())) else {
                return false;
            };
            target.copy_from_slice(data);
            true
        }
    }

    fn mounted() -> Ram {
        let mut storage = Ram([0; 4088]);
        vfs::mount(&mut storage).unwrap();
        storage
    }

    fn read_at(storage: &mut Ram, view: vfs::View, path: &[u8]) -> ([u8; 128], usize) {
        let id = vfs::resolve_in(storage, view, view.root, path).unwrap();
        let node = vfs::read_node(storage, id).unwrap();
        let mut out = [0u8; 128];
        let count = vfs::read(storage, &node, 0, &mut out).unwrap();
        (out, count)
    }
    fn same(storage: &mut Ram, view: vfs::View, path: &[u8], expected: &[u8]) -> bool {
        let (out, count) = read_at(storage, view, path);
        &out[..count] == expected
    }

    #[test]
    fn linux_policy_creates_private_fixture_and_shared_mount_once() {
        let mut storage = mounted();
        assert_eq!(
            vfs::resolve(&mut storage, vfs::ROOT, b"/linux"),
            Err(VfsError::NoEnt)
        );
        let view = prepare_policy(&mut storage, SERVICE_ROOT_LINUX).unwrap();
        assert!(same(
            &mut storage,
            view,
            b"/fixture.txt",
            LINUX_FIXTURE_FILE
        ));
        assert!(same(&mut storage, view, b"/fixturedir/alpha", b"a\n"));
        assert!(same(&mut storage, view, b"/shared/note", SHARED_NOTE));
        // Private paths stay under /linux; the service root is not the view.
        assert_eq!(
            vfs::resolve_in(&mut storage, view, view.root, b"/../fixture.txt"),
            vfs::resolve(&mut storage, vfs::ROOT, b"/linux/fixture.txt")
        );
        assert!(vfs::resolve(&mut storage, vfs::ROOT, b"/linux/fixturedir/beta").is_ok());
        // Existing content is not reseeded on a later registration.
        let id = vfs::resolve(&mut storage, vfs::ROOT, b"/linux/fixture.txt").unwrap();
        let mut node = vfs::read_node(&mut storage, id).unwrap();
        vfs::write(&mut storage, id, &mut node, 0, b"CHANGED").unwrap();
        prepare_policy(&mut storage, SERVICE_ROOT_LINUX).unwrap();
        let before = vfs::read_node(&mut storage, id).unwrap();
        assert_eq!(&before.name_bytes(), b"fixture.txt");
        assert!(!same(
            &mut storage,
            view,
            b"/fixture.txt",
            LINUX_FIXTURE_FILE
        ));
    }

    #[test]
    fn service_shared_and_native_policies_are_distinct_and_unknown_is_rejected() {
        let mut storage = mounted();
        assert_eq!(
            prepare_policy(&mut storage, SERVICE_ROOT_SERVICE),
            Ok(vfs::View::ROOT)
        );
        assert_eq!(
            vfs::resolve(&mut storage, vfs::ROOT, b"/linux"),
            Err(VfsError::NoEnt)
        );
        assert_eq!(prepare_policy(&mut storage, 9), Err(VfsError::Invalid));
        let shared = prepare_policy(&mut storage, SERVICE_ROOT_SHARED).unwrap();
        assert_eq!(shared.shared, None);
        assert!(same(&mut storage, shared, b"/note", SHARED_NOTE));
        assert_eq!(
            vfs::resolve_in(&mut storage, shared, shared.root, b"/linux"),
            Err(VfsError::NoEnt)
        );
        let native = prepare_policy(&mut storage, SERVICE_ROOT_NATIVE).unwrap();
        assert!(same(&mut storage, native, b"/shared/note", SHARED_NOTE));
        assert!(same(&mut storage, native, b"/README", NATIVE_README));
        assert_eq!(
            vfs::resolve(&mut storage, vfs::ROOT, b"/native"),
            Ok(native.root)
        );
        assert_ne!(native.root, shared.root);
    }

    /// P2b-5：绝对/相对符号链接都不能把策略视图带出根；同一链接在服务根里确实
    /// 指向目标文件，证明失败来自视图约束而不是损坏的 fixture。
    #[test]
    fn policy_views_contain_absolute_and_relative_symlink_targets() {
        let mut storage = mounted();
        let linux = prepare_policy(&mut storage, SERVICE_ROOT_LINUX).unwrap();
        let native = prepare_policy(&mut storage, SERVICE_ROOT_NATIVE).unwrap();
        let target = vfs::resolve(&mut storage, vfs::ROOT, b"/linux/fixture.txt").unwrap();
        assert_eq!(
            vfs::resolve(&mut storage, vfs::ROOT, b"/shared/escape"),
            Ok(target)
        );
        assert_eq!(
            vfs::resolve(&mut storage, vfs::ROOT, b"/shared/up"),
            Ok(target)
        );
        for view in [linux, native] {
            assert_eq!(
                vfs::resolve_in(&mut storage, view, view.root, b"/shared/escape"),
                Err(VfsError::NoEnt)
            );
            assert_eq!(
                vfs::resolve_in(&mut storage, view, view.root, b"/shared/up"),
                Err(VfsError::NoEnt)
            );
        }
        // 私有根互不可见：Linux 无 /native，原生无 /linux。
        assert_eq!(
            vfs::resolve_in(&mut storage, linux, linux.root, b"/linux/fixture.txt"),
            Err(VfsError::NoEnt)
        );
        assert_eq!(
            vfs::resolve_in(&mut storage, native, native.root, b"/linux/fixture.txt"),
            Err(VfsError::NoEnt)
        );
    }
}
