use super::*;
use std::vec;
use std::vec::Vec;

struct TestStorage {
    bytes: Vec<u8>,
    fail_read: bool,
    fail_write: bool,
}

impl TestStorage {
    fn new() -> Self {
        Self {
            bytes: vec![0u8; 4096 - 8],
            fail_read: false,
            fail_write: false,
        }
    }
}

impl Storage for TestStorage {
    fn read(&mut self, offset: usize, out: &mut [u8]) -> bool {
        if self.fail_read {
            return false;
        }
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
        if self.fail_write {
            return false;
        }
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

fn create_flags() -> OpenFlags {
    OpenFlags {
        access: AccessMode::ReadWrite,
        create: true,
        ..OpenFlags::default()
    }
}

#[test]
fn full_open_table_does_not_create_or_truncate() {
    let mut storage = mounted();
    let mut files = OpenFiles::<1>::new();
    let (fd, _) = files
        .open(&mut storage, ROOT, b"/f", create_flags())
        .unwrap();
    files.write(&mut storage, fd, b"keep").unwrap();
    let before = storage.bytes.clone();
    assert_eq!(
        files.open(&mut storage, ROOT, b"/tmp/new", create_flags()),
        Err(VfsError::TooManyOpen)
    );
    let trunc = OpenFlags {
        truncate: true,
        ..create_flags()
    };
    assert_eq!(
        files.open(&mut storage, ROOT, b"/f", trunc),
        Err(VfsError::TooManyOpen)
    );
    assert_eq!(storage.bytes, before);
    assert_eq!(read_file(&mut storage, b"/f"), b"keep");
    files.close(&mut storage, fd).unwrap();
    assert_eq!(files.close(&mut storage, fd), Err(VfsError::BadHandle));
    assert_eq!(
        files.read(&mut storage, HandleId(usize::MAX), &mut [0]),
        Err(VfsError::BadHandle)
    );
}

#[test]
fn shared_node_reference_limit_rejects_truncate_before_mutation() {
    let mut storage = mounted();
    let mut tables = Vec::new();
    for _ in 0..u8::MAX {
        let mut files = OpenFiles::<1>::new();
        files
            .open(&mut storage, ROOT, b"/f", create_flags())
            .unwrap();
        tables.push(files);
    }
    tables[0].write(&mut storage, HandleId(0), b"keep").unwrap();
    let before = storage.bytes.clone();
    let mut extra = OpenFiles::<1>::new();
    let flags = OpenFlags {
        truncate: true,
        ..create_flags()
    };
    assert_eq!(
        extra.open(&mut storage, ROOT, b"/f", flags),
        Err(VfsError::TooManyOpen)
    );
    assert_eq!(storage.bytes, before);
    for mut files in tables {
        files.close_all(&mut storage).unwrap();
    }
    assert!(extra.open(&mut storage, ROOT, b"/f", flags).is_ok());
    extra.close_all(&mut storage).unwrap();
}

#[test]
fn directory_handles_keep_parent_relationship_until_close() {
    let mut storage = mounted();
    let directory = mkdir(&mut storage, ROOT, b"/tmp/directory").unwrap();
    mkdir(&mut storage, ROOT, b"/tmp/source").unwrap();
    let mut files = OpenFiles::<1>::new();
    let (fd, _) = files
        .open(&mut storage, ROOT, b"/tmp/directory", OpenFlags::default())
        .unwrap();
    let mut bytes = [0u8; 16];
    assert_eq!(
        files.read(&mut storage, fd, &mut bytes),
        Err(VfsError::BadHandle)
    );
    assert_eq!(
        files.read_directory_bytes(&mut storage, fd, &mut bytes),
        Ok(11)
    );
    assert_eq!(files.seek(&mut storage, fd, SeekFrom::Start(0)), Ok(0));
    assert_eq!(
        unlink(&mut storage, ROOT, b"/tmp/directory", true),
        Err(VfsError::Access)
    );
    assert_eq!(
        rename(&mut storage, ROOT, b"/tmp/source", b"/tmp/directory"),
        Err(VfsError::Access)
    );
    assert_eq!(
        resolve(&mut storage, directory, b".."),
        resolve(&mut storage, ROOT, b"/tmp")
    );
    files.close_all(&mut storage).unwrap();
    unlink(&mut storage, ROOT, b"/tmp/directory", true).unwrap();
    assert_eq!(metadata(&mut storage, directory), Err(VfsError::NoEnt));
}

#[test]
fn unlinked_open_objects_survive_across_tables_until_last_close() {
    let mut storage = mounted();
    let mut first = OpenFiles::<2>::new();
    let mut second = OpenFiles::<1>::new();
    let (a, old) = first
        .open(&mut storage, ROOT, b"/tmp/old", create_flags())
        .unwrap();
    first.write(&mut storage, a, b"old").unwrap();
    let (b, _) = second
        .open(&mut storage, ROOT, b"/tmp/old", OpenFlags::default())
        .unwrap();
    unlink(&mut storage, ROOT, b"/tmp/old", false).unwrap();
    assert_eq!(
        resolve(&mut storage, ROOT, b"/tmp/old"),
        Err(VfsError::NoEnt)
    );
    let tmp = resolve(&mut storage, ROOT, b"/tmp").unwrap();
    assert_eq!(dir_entry_count(&mut storage, tmp), Ok(2));
    let (new, info) = first
        .open(&mut storage, ROOT, b"/tmp/old", create_flags())
        .unwrap();
    assert_ne!(old.id, info.id);
    first.write(&mut storage, new, b"new").unwrap();
    let mut out = [0u8; 3];
    assert_eq!(second.read(&mut storage, b, &mut out), Ok(3));
    assert_eq!(&out, b"old");
    first.close(&mut storage, a).unwrap();
    assert_eq!(
        read_node(&mut storage, old.id).unwrap().parent,
        ORPHAN_PARENT
    );
    second.close(&mut storage, b).unwrap();
    let reused = open(&mut storage, ROOT, b"/tmp/reused", create_flags()).unwrap();
    assert_eq!(reused.id, old.id);
    first.close_all(&mut storage).unwrap();
    assert!(first.is_empty());
}

#[test]
fn rename_replacement_keeps_open_target_and_symlink_unlink_does_not_touch_target() {
    let mut storage = mounted();
    let mut files = OpenFiles::<3>::new();
    let (source, _) = files
        .open(&mut storage, ROOT, b"/tmp/source", create_flags())
        .unwrap();
    files.write(&mut storage, source, b"src").unwrap();
    let (target, old) = files
        .open(&mut storage, ROOT, b"/tmp/target", create_flags())
        .unwrap();
    files.write(&mut storage, target, b"old").unwrap();
    rename(&mut storage, ROOT, b"/tmp/source", b"/tmp/target").unwrap();
    assert_eq!(read_file(&mut storage, b"/tmp/target"), b"src");
    files
        .seek(&mut storage, target, SeekFrom::Start(0))
        .unwrap();
    let mut out = [0u8; 3];
    files.read(&mut storage, target, &mut out).unwrap();
    assert_eq!(&out, b"old");
    assert_eq!(files.info(&mut storage, target).unwrap().id, old.id);
    symlink(&mut storage, ROOT, b"target", b"/tmp/link").unwrap();
    unlink(&mut storage, ROOT, b"/tmp/link", false).unwrap();
    assert_eq!(read_file(&mut storage, b"/tmp/target"), b"src");
    files.close_all(&mut storage).unwrap();
}

#[test]
fn compaction_preserves_orphan_data_and_reclaims_it_after_close() {
    let mut storage = mounted();
    let mut files = OpenFiles::<2>::new();
    let mut gap = open(&mut storage, ROOT, b"/tmp/gap", create_flags()).unwrap();
    write(&mut storage, gap.id, &mut gap.node, 0, &[b'g'; 700]).unwrap();
    let (orphan, _) = files
        .open(&mut storage, ROOT, b"/tmp/orphan", create_flags())
        .unwrap();
    files.write(&mut storage, orphan, &[b'o'; 900]).unwrap();
    unlink(&mut storage, ROOT, b"/tmp/gap", false).unwrap();
    unlink(&mut storage, ROOT, b"/tmp/orphan", false).unwrap();
    // 超过尾部空闲量，必须压实；仍被引用的孤立对象必须参与空间分配。
    let (new, _) = files
        .open(&mut storage, ROOT, b"/tmp/new", create_flags())
        .unwrap();
    files.write(&mut storage, new, &[b'n'; 1200]).unwrap();
    files
        .seek(&mut storage, orphan, SeekFrom::Start(0))
        .unwrap();
    let mut out = [0u8; 900];
    assert_eq!(files.read(&mut storage, orphan, &mut out), Ok(900));
    assert_eq!(out, [b'o'; 900]);
    files.close(&mut storage, orphan).unwrap();
    files.seek(&mut storage, new, SeekFrom::Start(0)).unwrap();
    files.write(&mut storage, new, &[b'x'; 1900]).unwrap();
    assert_eq!(read_file(&mut storage, b"/tmp/new"), vec![b'x'; 1900]);
}

#[test]
fn open_cursors_access_append_and_chmod_are_object_state() {
    let mut storage = mounted();
    let mut files = OpenFiles::<4>::new();
    let (a, _) = files
        .open(&mut storage, ROOT, b"/f", create_flags())
        .unwrap();
    files.write(&mut storage, a, b"abc").unwrap();
    let (b, _) = files
        .open(&mut storage, ROOT, b"/f", OpenFlags::default())
        .unwrap();
    let mut out = [0u8; 3];
    assert_eq!(files.read(&mut storage, a, &mut out), Ok(0));
    assert_eq!(files.read(&mut storage, b, &mut out[..1]), Ok(1));
    assert_eq!(out[0], b'a');
    assert_eq!(files.write(&mut storage, b, b"x"), Err(VfsError::BadHandle));
    let append = OpenFlags {
        access: AccessMode::WriteOnly,
        append: true,
        ..OpenFlags::default()
    };
    let (c, _) = files.open(&mut storage, ROOT, b"/f", append).unwrap();
    files.seek(&mut storage, c, SeekFrom::Start(0)).unwrap();
    files.write(&mut storage, c, b"d").unwrap();
    assert_eq!(
        files.read(&mut storage, c, &mut out),
        Err(VfsError::BadHandle)
    );
    assert_eq!(
        files.seek(&mut storage, c, SeekFrom::Current(i64::MAX)),
        Err(VfsError::Invalid)
    );
    assert_eq!(
        files.seek(&mut storage, c, SeekFrom::Start(-1)),
        Err(VfsError::Invalid)
    );
    assert_eq!(files.seek(&mut storage, c, SeekFrom::Current(0)), Ok(4));
    chmod(&mut storage, ROOT, b"/f", 0o000).unwrap();
    assert_eq!(
        files.open(&mut storage, ROOT, b"/f", OpenFlags::default()),
        Err(VfsError::Access)
    );
    // 权限在 open 时检查；后续 chmod 不撤销已授予的打开对象访问模式。
    assert_eq!(files.write(&mut storage, a, b"Z"), Ok(1));
    assert_eq!(files.read(&mut storage, b, &mut out), Ok(3));
    assert_eq!(&out, b"bcZ");
    assert_eq!(files.seek(&mut storage, b, SeekFrom::End(-1)), Ok(3));
    files.close_all(&mut storage).unwrap();
}

#[test]
fn empty_and_sparse_writes_do_not_expose_old_bytes() {
    let mut storage = mounted();
    let mut file = open(&mut storage, ROOT, b"/f", create_flags()).unwrap();
    write(&mut storage, file.id, &mut file.node, 0, b"secret").unwrap();
    truncate(&mut storage, file.id, &mut file.node, 0).unwrap();
    let before = storage.bytes.clone();
    assert_eq!(
        write(&mut storage, file.id, &mut file.node, u64::MAX, b""),
        Ok(0)
    );
    assert_eq!(storage.bytes, before);
    write(&mut storage, file.id, &mut file.node, 5, b"X").unwrap();
    assert_eq!(read_file(&mut storage, b"/f"), b"\0\0\0\0\0X");
}

#[test]
fn path_checks_apply_to_dot_components_links_and_creation() {
    let mut storage = mounted();
    for path in [b"/f/.".as_slice(), b"/f/..", b"/f/../README"] {
        assert_eq!(resolve(&mut storage, ROOT, path), Err(VfsError::NotDir));
    }
    assert_eq!(
        resolve(&mut storage, ROOT, &[b'/'; MAX_PATH + 1]),
        Err(VfsError::Invalid)
    );
    assert_eq!(
        resolve(&mut storage, ROOT, b"/tmp/a\0b"),
        Err(VfsError::Invalid)
    );
    assert_eq!(
        open(&mut storage, ROOT, b"/tmp/missing/", create_flags()),
        Err(VfsError::NotDir)
    );
    assert_eq!(
        resolve(&mut storage, ROOT, b"/tmp/missing"),
        Err(VfsError::NoEnt)
    );
    symlink(&mut storage, ROOT, b"/f/", b"/tmp/slash").unwrap();
    assert_eq!(
        resolve(&mut storage, ROOT, b"/tmp/slash"),
        Err(VfsError::NotDir)
    );
    symlink(&mut storage, ROOT, b"absent", b"/tmp/dangling").unwrap();
    assert_eq!(
        open(
            &mut storage,
            ROOT,
            b"/tmp/dangling",
            OpenFlags {
                exclusive: true,
                ..create_flags()
            }
        ),
        Err(VfsError::Exist)
    );
    chmod(&mut storage, ROOT, b"/etc", 0o777).unwrap();
    open(&mut storage, ROOT, b"/etc/movable", create_flags()).unwrap();
    chmod(&mut storage, ROOT, b"/etc", 0o555).unwrap();
    assert_eq!(
        rename(&mut storage, ROOT, b"/etc/movable", b"/tmp/moved"),
        Err(VfsError::Access)
    );
    assert!(resolve(&mut storage, ROOT, b"/etc/movable").is_ok());
}

#[test]
fn io_errors_preserve_cursors_and_retryable_close() {
    let mut storage = mounted();
    let mut files = OpenFiles::<1>::new();
    let (fd, _) = files
        .open(&mut storage, ROOT, b"/f", create_flags())
        .unwrap();
    files.write(&mut storage, fd, b"abc").unwrap();
    files.seek(&mut storage, fd, SeekFrom::Start(0)).unwrap();
    storage.fail_read = true;
    assert_eq!(files.read(&mut storage, fd, &mut [0]), Err(VfsError::Io));
    storage.fail_read = false;
    storage.fail_write = true;
    assert_eq!(files.write(&mut storage, fd, b"z"), Err(VfsError::Io));
    assert_eq!(files.close(&mut storage, fd), Err(VfsError::Io));
    assert_eq!(files.len(), 1);
    storage.fail_write = false;
    assert_eq!(files.seek(&mut storage, fd, SeekFrom::Current(0)), Ok(0));
    files.close(&mut storage, fd).unwrap();
    assert!(files.is_empty());
}

#[test]
fn mount_errors_never_silently_format_and_recovery_reaps_lost_sessions() {
    let mut storage = mounted();
    let before = storage.bytes.clone();
    storage.fail_read = true;
    assert_eq!(mount(&mut storage), Err(VfsError::Io));
    storage.fail_read = false;
    assert_eq!(storage.bytes, before);
    storage.bytes[4] = 2; // 旧版本必须显式处理。
    let old = storage.bytes.clone();
    assert_eq!(mount(&mut storage), Err(VfsError::Unformatted));
    assert_eq!(storage.bytes, old);
    storage.bytes = before;
    let mut files = OpenFiles::<2>::new();
    let (_, linked) = files
        .open(&mut storage, ROOT, b"/f", create_flags())
        .unwrap();
    let (lost, orphan) = files
        .open(&mut storage, ROOT, b"/tmp/lost", create_flags())
        .unwrap();
    files.write(&mut storage, lost, b"old").unwrap();
    unlink(&mut storage, ROOT, b"/tmp/lost", false).unwrap();
    // 模拟服务重启：旧表不再使用。
    let _lost_session = files;
    mount(&mut storage).unwrap();
    recover_open_files(&mut storage).unwrap();
    assert_eq!(read_node(&mut storage, linked.id).unwrap().open_refs, 0);
    assert_eq!(
        read_node(&mut storage, orphan.id).unwrap().parent,
        FREE_PARENT
    );
    assert_eq!(
        open(&mut storage, ROOT, b"/tmp/reused", create_flags())
            .unwrap()
            .id,
        orphan.id
    );
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
        access: AccessMode::ReadOnly,
        ..OpenFlags::default()
    };
    let opened = open(&mut storage, ROOT, b"/etc/motd", flags).unwrap();
    assert_eq!(opened.node.kind, NodeKind::File);
    let writable = OpenFlags {
        access: AccessMode::WriteOnly,
        ..OpenFlags::default()
    };
    assert_eq!(
        open(&mut storage, ROOT, b"/etc/motd", writable),
        Err(VfsError::Access)
    );
    let create = OpenFlags {
        access: AccessMode::ReadWrite,
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
                access: AccessMode::ReadWrite,
                create: true,
                ..OpenFlags::default()
            }
        ),
        Err(VfsError::Access)
    );
    let directory = OpenFlags {
        access: AccessMode::ReadOnly,
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
            access: AccessMode::ReadWrite,
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
            access: AccessMode::ReadWrite,
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
                access: AccessMode::ReadWrite,
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
            access: AccessMode::ReadWrite,
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
        access: AccessMode::ReadWrite,
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
            access: AccessMode::ReadWrite,
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
            access: AccessMode::ReadWrite,
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
            access: AccessMode::ReadWrite,
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
        access: AccessMode::ReadWrite,
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
        access: AccessMode::ReadWrite,
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
        access: AccessMode::ReadWrite,
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
        access: AccessMode::ReadWrite,
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
        access: AccessMode::WriteOnly,
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
        access: AccessMode::ReadWrite,
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
