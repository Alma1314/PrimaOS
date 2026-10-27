//! 有界服务路径的分片暂存；不解析文件树、不取得 Authority。
//! 状态字段仅供受信任服务分派/执行代码使用；客户端必须通过 begin/append/commit。

use prima_runtime::protocol::{
    SERVICE_FILE_EINVAL, SERVICE_VFS_FLAG_STAT, SERVICE_VFS_OP_MKDIR, SERVICE_VFS_OP_RMDIR,
    SERVICE_VFS_OP_UNLINK, SERVICE_VFS_PAIR_OP_READLINK, SERVICE_VFS_PATH_MAX,
};

/// M11：暂存操作码（0 保留给 open/stat；线上 0..2 映射为 1..3）。
pub const STAGED_MKDIR: u8 = SERVICE_VFS_OP_MKDIR as u8 + 1;
pub const STAGED_UNLINK: u8 = SERVICE_VFS_OP_UNLINK as u8 + 1;
pub const STAGED_RMDIR: u8 = SERVICE_VFS_OP_RMDIR as u8 + 1;
/// M13b：chmod/fchmod 的单路径暂存（0..3 已分配）。
pub const STAGED_CHMOD: u8 = 4;
/// M8：一次 open 的分阶段状态。路径经 `OPEN`（前 3 字节）+ `PATH`（随后每段 5 字节）装入。
#[derive(Clone, Copy)]
pub struct OpenStaging {
    pub active: bool,
    /// 0 = open/stat；1 = mkdir；2 = unlink；3 = rmdir（M11）；4 = chmod（M13b）。
    pub op: u8,
    pub flags: u8,
    pub len: u8,
    pub got: u8,
    pub start: u8,
    pub path: [u8; SERVICE_VFS_PATH_MAX],
}

impl OpenStaging {
    pub const EMPTY: Self = Self {
        active: false,
        op: 0,
        flags: 0,
        len: 0,
        got: 0,
        start: 0,
        path: [0u8; SERVICE_VFS_PATH_MAX],
    };

    pub fn begin(&mut self, flags: u8, len: usize, start: u8, first: u64) -> u64 {
        // M9：STAT 允许空路径，表示“直接查询 start 节点”（fstat 用）。
        let stat_only = flags & SERVICE_VFS_FLAG_STAT as u8 != 0;
        if (len == 0 && !stat_only) || len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        self.start_path(0, flags, len, start, first);
        0
    }

    /// M11：开始一次单路径变更（mkdir/unlink/rmdir）；空路径一律拒绝。
    /// M13b：开始一次 chmod/fchmod；空路径合法（改 `start` 节点），故不复用
    /// `begin` 的“非 stat 必须非空路径”检查。
    pub fn begin_chmod(&mut self, len: usize, start: u8, first: u64) -> u64 {
        if len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        self.start_path(STAGED_CHMOD, 0, len, start, first);
        0
    }

    pub fn begin_mutate(&mut self, op: u8, len: usize, start: u8, first: u64) -> u64 {
        if !(STAGED_MKDIR..=STAGED_RMDIR).contains(&op) || len == 0 || len > SERVICE_VFS_PATH_MAX {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        let status = self.begin(0, len, start, first);
        if status == 0 {
            self.op = op;
        }
        status
    }

    fn start_path(&mut self, op: u8, flags: u8, len: usize, start: u8, first: u64) {
        self.active = true;
        self.op = op;
        self.flags = flags;
        self.len = len as u8;
        self.start = start;
        self.got = start_path(&mut self.path, len, first);
    }

    pub fn append(&mut self, offset: usize, bytes: u64) -> u64 {
        if !self.active || !append_path(&mut self.path, self.len, &mut self.got, offset, bytes) {
            self.active = false;
            return SERVICE_FILE_EINVAL;
        }
        0
    }
}

/// M13a：rename/symlink 的双字符串暂存，以及 readlink 最近一次目标。
///
/// 槽 0 与槽 1 各自按 `OPEN`（前 3 字节）+ `PATH`（随后每段 5 字节）填充；A 的
/// COMMIT 只确认，B 的 COMMIT（readlink 是 A）执行操作。两个槽都只保存字节，
/// 目标字符串不要求是合法路径。
#[derive(Clone, Copy)]
pub struct PairStaging {
    pub active: bool,
    pub op: u8,
    slot: u8,
    confirmed: bool,
    pub lens: [u8; 2],
    got: [u8; 2],
    pub paths: [[u8; SERVICE_VFS_PATH_MAX]; 2],
}

impl PairStaging {
    pub const EMPTY: Self = Self {
        active: false,
        op: 0,
        slot: 0,
        confirmed: false,
        lens: [0; 2],
        got: [0; 2],
        paths: [[0u8; SERVICE_VFS_PATH_MAX]; 2],
    };

    pub fn begin(&mut self, op: u8, slot: u8, len: usize, first: u64) -> bool {
        if slot > 1
            || op > SERVICE_VFS_PAIR_OP_READLINK as u8
            || len == 0
            || len > SERVICE_VFS_PATH_MAX
        {
            self.active = false;
            return false;
        }
        if slot == 0 {
            *self = Self::EMPTY;
        } else if !self.active
            || self.op != op
            || self.slot != 0
            || !self.confirmed
            || op == SERVICE_VFS_PAIR_OP_READLINK as u8
        {
            self.active = false;
            return false;
        }
        self.active = true;
        self.op = op;
        self.slot = slot;
        self.confirmed = false;
        self.lens[slot as usize] = len as u8;
        self.got[slot as usize] = start_path(&mut self.paths[slot as usize], len, first);
        true
    }

    pub fn append(&mut self, offset: usize, bytes: u64) -> bool {
        let slot = self.slot as usize;
        if !self.active
            || self.confirmed
            || !append_path(
                &mut self.paths[slot],
                self.lens[slot],
                &mut self.got[slot],
                offset,
                bytes,
            )
        {
            self.active = false;
            return false;
        }
        true
    }

    /// false = 槽 0 已确认，继续等待槽 1；true = 消费完整请求，调用方可执行一次。
    /// 任何错误都取消整个暂存事务，不能恢复旧的路径。
    pub fn commit(&mut self, op: u8, slot: u8) -> Result<bool, u64> {
        if !self.active
            || self.op != op
            || self.slot != slot
            || self.confirmed
            || !self.ready(slot as usize)
        {
            self.active = false;
            return Err(SERVICE_FILE_EINVAL);
        }
        if slot == 0 && op != SERVICE_VFS_PAIR_OP_READLINK as u8 {
            self.confirmed = true;
            Ok(false)
        } else {
            self.active = false;
            Ok(true)
        }
    }

    pub fn ready(&self, slot: usize) -> bool {
        slot < 2 && self.lens[slot] != 0 && self.got[slot] == self.lens[slot]
    }
}

// 两类请求共用相同的 wire 分片规则；元数据/操作次序由各自状态机管理。
fn start_path(path: &mut [u8; SERVICE_VFS_PATH_MAX], len: usize, first: u64) -> u8 {
    *path = [0; SERVICE_VFS_PATH_MAX];
    let got = len.min(3);
    path[..got].copy_from_slice(&first.to_le_bytes()[..got]);
    got as u8
}

fn append_path(
    path: &mut [u8; SERVICE_VFS_PATH_MAX],
    len: u8,
    got: &mut u8,
    offset: usize,
    bytes: u64,
) -> bool {
    if offset != *got as usize || offset >= len as usize {
        return false;
    }
    let count = (len as usize - offset).min(5);
    path[offset..offset + count].copy_from_slice(&bytes.to_le_bytes()[..count]);
    *got = (offset + count) as u8;
    true
}

/// M13a：readlink 最近一次目标，供后续 3 字节分段读取。
#[derive(Clone, Copy)]
pub struct LinkTarget {
    pub len: u8,
    pub bytes: [u8; SERVICE_VFS_PATH_MAX],
}

impl LinkTarget {
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0u8; SERVICE_VFS_PATH_MAX],
    };
}

/// Fixed private service-memory format, independent of Rust struct layout.
pub const SESSION_WIRE_BYTES: usize = 1088;
pub fn encode_session(
    open: &OpenStaging,
    pair: &PairStaging,
    link: &LinkTarget,
) -> [u8; SESSION_WIRE_BYTES] {
    let mut out = [0; SESSION_WIRE_BYTES];
    out[..6].copy_from_slice(&[
        open.active as u8,
        open.op,
        open.flags,
        open.len,
        open.got,
        open.start,
    ]);
    out[6..6 + SERVICE_VFS_PATH_MAX].copy_from_slice(&open.path);
    out[264..272].copy_from_slice(&[
        pair.active as u8,
        pair.op,
        pair.slot,
        pair.confirmed as u8,
        pair.lens[0],
        pair.lens[1],
        pair.got[0],
        pair.got[1],
    ]);
    out[272..272 + SERVICE_VFS_PATH_MAX].copy_from_slice(&pair.paths[0]);
    out[528..528 + SERVICE_VFS_PATH_MAX].copy_from_slice(&pair.paths[1]);
    out[792] = link.len;
    out[793..793 + SERVICE_VFS_PATH_MAX].copy_from_slice(&link.bytes);
    out
}
pub fn decode_session(
    input: &[u8; SESSION_WIRE_BYTES],
    open: &mut OpenStaging,
    pair: &mut PairStaging,
    link: &mut LinkTarget,
) {
    open.active = input[0] != 0;
    open.op = input[1];
    open.flags = input[2];
    open.len = input[3];
    open.got = input[4];
    open.start = input[5];
    open.path
        .copy_from_slice(&input[6..6 + SERVICE_VFS_PATH_MAX]);
    pair.active = input[264] != 0;
    pair.op = input[265];
    pair.slot = input[266];
    pair.confirmed = input[267] != 0;
    pair.lens = [input[268], input[269]];
    pair.got = [input[270], input[271]];
    pair.paths[0].copy_from_slice(&input[272..272 + SERVICE_VFS_PATH_MAX]);
    pair.paths[1].copy_from_slice(&input[528..528 + SERVICE_VFS_PATH_MAX]);
    link.len = input[792];
    link.bytes
        .copy_from_slice(&input[793..793 + SERVICE_VFS_PATH_MAX]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_fragment_invalidates_the_single_path() {
        let mut staged = OpenStaging::EMPTY;
        assert_eq!(staged.begin(0, 8, 0, 0x636261), 0);
        assert_eq!(staged.append(0, 0x787878), SERVICE_FILE_EINVAL);
        assert!(!staged.active);
    }

    #[test]
    fn new_pair_clears_both_paths() {
        let mut pair = PairStaging::EMPTY;
        assert!(pair.begin(0, 0, 3, 0x636261));
        assert_eq!(pair.commit(0, 0), Ok(false));
        assert!(pair.begin(0, 1, 3, u64::from_le_bytes(*b"old\0\0\0\0\0")));
        assert_eq!(pair.commit(0, 1), Ok(true));
        assert!(pair.begin(0, 0, 3, 0x666564));
        assert!(!pair.ready(1));
        assert_eq!(pair.paths[1], [0; SERVICE_VFS_PATH_MAX]);
    }

    #[test]
    fn second_slot_cannot_start_a_transaction() {
        let mut pair = PairStaging::EMPTY;
        assert!(!pair.begin(0, 1, 3, 0x636261));
        assert!(!pair.active);
    }

    fn word(bytes: &[u8]) -> u64 {
        let mut out = [0; 8];
        out[..bytes.len()].copy_from_slice(bytes);
        u64::from_le_bytes(out)
    }

    fn fill_pair(pair: &mut PairStaging, op: u8, slot: u8, bytes: &[u8]) {
        assert!(pair.begin(op, slot, bytes.len(), word(&bytes[..bytes.len().min(3)])));
        for offset in (3..bytes.len()).step_by(5) {
            assert!(pair.append(offset, word(&bytes[offset..(offset + 5).min(bytes.len())])));
        }
        assert!(pair.ready(slot as usize));
        assert_eq!(&pair.paths[slot as usize][..bytes.len()], bytes);
    }

    #[test]
    fn all_path_lengths_reassemble_exactly_and_restart_after_failure() {
        let bytes = core::array::from_fn::<_, SERVICE_VFS_PATH_MAX, _>(|index| index as u8 + 1);
        let mut staged = OpenStaging::EMPTY;
        for len in 1..=SERVICE_VFS_PATH_MAX {
            assert_eq!(staged.begin(0, len, 7, word(&bytes[..len.min(3)])), 0);
            for offset in (3..len).step_by(5) {
                assert_eq!(
                    staged.append(offset, word(&bytes[offset..(offset + 5).min(len)])),
                    0
                );
            }
            assert_eq!(staged.got as usize, len);
            assert_eq!(staged.start, 7);
            assert_eq!(&staged.path[..len], &bytes[..len]);
            let before = staged.path;
            assert_eq!(staged.append(len, 0), SERVICE_FILE_EINVAL);
            assert!(!staged.active);
            assert_eq!(staged.path, before);
        }
    }

    #[test]
    fn invalid_fragments_do_not_modify_bytes_or_allow_later_completion() {
        for offset in [0, 1, 2, 4, 8, 9, usize::MAX] {
            let mut staged = OpenStaging::EMPTY;
            assert_eq!(staged.begin(0, 8, 0, 0x636261), 0);
            let before = staged.path;
            assert_eq!(staged.append(offset, 0), SERVICE_FILE_EINVAL);
            assert_eq!(staged.path, before);
            assert_eq!(staged.got, 3);
            assert_eq!(staged.append(3, 0), SERVICE_FILE_EINVAL);

            let mut pair = PairStaging::EMPTY;
            assert!(pair.begin(0, 0, 8, 0x636261));
            let before = pair.paths;
            assert!(!pair.append(offset, 0));
            assert!(!pair.active);
            assert_eq!(pair.paths, before);
            assert_eq!(pair.commit(0, 0), Err(SERVICE_FILE_EINVAL));
        }
        let mut staged = OpenStaging::EMPTY;
        assert_eq!(staged.begin(0, 13, 0, 0x636261), 0);
        assert_eq!(staged.append(3, word(b"defgh")), 0);
        let before = staged.path;
        assert_eq!(staged.append(3, word(b"XXXXX")), SERVICE_FILE_EINVAL);
        assert_eq!(staged.path, before);
        assert_eq!(staged.append(8, word(b"ijklm")), SERVICE_FILE_EINVAL);

        let mut pair = PairStaging::EMPTY;
        assert!(pair.begin(0, 0, 13, 0x636261));
        assert!(pair.append(3, word(b"defgh")));
        let before = pair.paths;
        assert!(!pair.append(3, word(b"XXXXX")));
        assert_eq!(pair.paths, before);
        assert!(!pair.append(8, word(b"ijklm")));
    }

    #[test]
    fn empty_and_invalid_path_headers_obey_operation_boundaries() {
        let mut staged = OpenStaging::EMPTY;
        assert_eq!(staged.begin(0, 0, 0, 0), SERVICE_FILE_EINVAL);
        assert_eq!(staged.begin(SERVICE_VFS_FLAG_STAT as u8, 0, 7, 0), 0);
        assert_eq!(staged.got, staged.len);
        assert_eq!(staged.begin_chmod(0, 7, 0), 0);
        for op in STAGED_MKDIR..=STAGED_RMDIR {
            assert_eq!(staged.begin_mutate(op, 0, 0, 0), SERVICE_FILE_EINVAL);
            assert_eq!(staged.begin_mutate(op, 1, 0, 1), 0);
            assert_eq!(staged.op, op);
        }
        assert_eq!(staged.begin_mutate(255, 1, 0, 1), SERVICE_FILE_EINVAL);
        for len in [SERVICE_VFS_PATH_MAX + 1, usize::MAX] {
            assert_eq!(staged.begin(0, len, 0, 0), SERVICE_FILE_EINVAL);
            assert_eq!(staged.begin_chmod(len, 0, 0), SERVICE_FILE_EINVAL);
            assert!(!staged.active);
        }
    }

    #[test]
    fn pair_sequences_accept_all_lengths_and_consume_final_commit_once() {
        let bytes = [b'x'; SERVICE_VFS_PATH_MAX];
        for op in 0..=SERVICE_VFS_PAIR_OP_READLINK as u8 {
            for len in 1..=SERVICE_VFS_PATH_MAX {
                let mut pair = PairStaging::EMPTY;
                fill_pair(&mut pair, op, 0, &bytes[..len]);
                if op != SERVICE_VFS_PAIR_OP_READLINK as u8 {
                    assert_eq!(pair.commit(op, 0), Ok(false));
                    fill_pair(&mut pair, op, 1, &bytes[..SERVICE_VFS_PATH_MAX + 1 - len]);
                    assert_eq!(pair.commit(op, 1), Ok(true));
                    assert_eq!(pair.commit(op, 1), Err(SERVICE_FILE_EINVAL));
                } else {
                    assert_eq!(pair.commit(op, 0), Ok(true));
                    assert_eq!(pair.commit(op, 0), Err(SERVICE_FILE_EINVAL));
                }
                assert!(!pair.active);
            }
        }
    }

    #[test]
    fn pair_rejects_wrong_order_op_slot_and_replayed_confirmation() {
        let mut pair = PairStaging::EMPTY;
        for (op, slot) in [(0, 1), (1, 0), (0, 2)] {
            fill_pair(&mut pair, 0, 0, b"old");
            assert_eq!(pair.commit(op, slot), Err(SERVICE_FILE_EINVAL));
            assert!(!pair.active);
        }
        fill_pair(&mut pair, 0, 0, b"old");
        assert!(!pair.begin(0, 1, 3, 0)); // No confirmation.
        fill_pair(&mut pair, 0, 0, b"old");
        assert_eq!(pair.commit(0, 0), Ok(false));
        assert!(!pair.begin(1, 1, 3, 0)); // Changed operation.
        fill_pair(&mut pair, 0, 0, b"old");
        assert_eq!(pair.commit(0, 0), Ok(false));
        assert_eq!(pair.commit(0, 0), Err(SERVICE_FILE_EINVAL));
        fill_pair(&mut pair, 0, 0, b"old");
        assert_eq!(pair.commit(0, 0), Ok(false));
        assert!(!pair.append(3, 0)); // Cannot modify an acknowledged path.
        assert!(!pair.begin(0, 1, 3, 0));
        assert!(!pair.ready(usize::MAX));
    }

    #[test]
    fn pair_rejects_incomplete_and_invalid_headers_then_accepts_fresh_request() {
        let mut pair = PairStaging::EMPTY;
        for (op, slot, len) in [(0, 0, 0), (0, 0, 65), (3, 0, 3), (0, 2, 3)] {
            fill_pair(&mut pair, 0, 0, b"old");
            assert!(!pair.begin(op, slot, len, 0));
            assert_eq!(pair.commit(0, 0), Err(SERVICE_FILE_EINVAL));
        }
        assert!(pair.begin(0, 0, 8, 0));
        assert_eq!(pair.commit(0, 0), Err(SERVICE_FILE_EINVAL));
        fill_pair(&mut pair, 0, 0, b"new");
        assert_eq!(pair.commit(0, 0), Ok(false));
        assert!(pair.begin(0, 1, 8, 0));
        assert_eq!(pair.commit(0, 1), Err(SERVICE_FILE_EINVAL));
        fill_pair(&mut pair, SERVICE_VFS_PAIR_OP_READLINK as u8, 0, b"new");
        assert_eq!(pair.commit(SERVICE_VFS_PAIR_OP_READLINK as u8, 0), Ok(true));
        assert!(!pair.begin(SERVICE_VFS_PAIR_OP_READLINK as u8, 1, 3, 0));
    }
}
