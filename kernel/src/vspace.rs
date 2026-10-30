//! 自建 4 级页表映射的细化（refine）模块：在 bootloader CR3 之上新增一条隔离的
//! 低半区映射。映射构建的**纯逻辑**（级别遍历、PTE 装载、缺页表页的预分配）可 host
//! 单测；真实 `invlpg` 由 [`prima_ostd::arch::invlpg`] 承载，仅内核/QEMU 内执行。
//!
//! **范围边界：** 本模块不替换 CR3、不重建整棵页表，只在空闲 PML4 槽（见
//! [`VSPACE_VA`]）下装一条 4KiB 映射，用于见证真实 PTE 翻译与 TLB 刷新。

/// PTE present 位（bit 0）。
pub const PT_PRESENT: u64 = 1;

/// PTE writable 位（bit 1）。
pub const PT_WRITABLE: u64 = 1 << 1;

/// PTE user 位（bit 2）：CPL3 可访问该页。
pub const PT_USER: u64 = 1 << 2;

/// PDPT/PD 叶项的大页位（bit 7）。
pub const PT_HUGE: u64 = 1 << 7;

/// M17b：软件 COW 位（PTE bit 9，available）。
pub const PT_COW: u64 = 1 << 9;
/// A retained user leaf temporarily inaccessible to CPL3 (PROT_NONE).
pub const PT_HIDDEN: u64 = 1 << 10;
/// A fork-shared data frame, including a read-only or hidden leaf.
pub const PT_SHARED: u64 = 1 << 11;

/// PTE 物理地址掩码（bits 12..51）。
pub const PT_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;

/// 演示映射使用的虚拟地址：i4 槽 1（低半区 `0x80_0000_0000`）。
///
/// 内核镜像映射在 i4=256（高半区）、bootloader 低映射在 i4=0，故槽 1 空闲。
pub const VSPACE_VA: u64 = 0x0000_0080_0000_0000;

/// 数据帧 A/B 的写入魔数（xtask 侧有镜像常量，需人工同步）。
pub const MAGIC_A: u64 = 0xAAAA_0000_1111_2222;
pub const MAGIC_B: u64 = 0xBBBB_0000_3333_4444;

/// 映射构建失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VspaceError {
    /// 页表或 COW 数据帧不足；失败副作用以各操作的契约为准。
    NoFrame,
    /// 目标映射的中间层级不存在，或路径上已有大页。
    BadRoot,
    /// 演示用的 PML4 槽已被占用（拒绝覆盖 bootloader 已有映射）。
    SlotOccupied,
    /// `unmap_4k` 目标页当前无映射。
    NotMapped,
    /// 独立用户 ELF 格式或受支持的重定位不合法。
    InvalidImage,
    /// EXO-1：boot image 尚未由 bootloader 传入或安装。
    NoBootImage,
}

/// 从虚拟地址取出 4 级索引 `[i4, i3, i2, i1]`。
pub const fn indices(va: u64) -> [usize; 4] {
    [
        ((va >> 39) & 0x1ff) as usize,
        ((va >> 30) & 0x1ff) as usize,
        ((va >> 21) & 0x1ff) as usize,
        ((va >> 12) & 0x1ff) as usize,
    ]
}

/// 叶 PTE 标志（present + 可选 writable + 可选 user）。
pub const fn pte_flags(writable: bool, user: bool) -> u64 {
    PT_PRESENT | if writable { PT_WRITABLE } else { 0 } | if user { PT_USER } else { 0 }
}

/// 中间层页表项标志：present + writable（写只读叶的故障必须落在叶上）。
pub const fn table_flags() -> u64 {
    PT_PRESENT | PT_WRITABLE
}

fn read_raw(addr: u64) -> u64 {
    prima_ostd::mm::read_u64(addr)
}

fn write_raw(addr: u64, value: u64) {
    prima_ostd::mm::write_u64(addr, value);
}

/// 把一页（512 项）清零。
fn zero_table(mapped_va: u64) {
    for index in 0..512 {
        write_raw(mapped_va + (index as u64) * 8, 0);
    }
}

/// `va` 路径上缺失的页表页数（叶子所在 PT 计入）。
///
/// 切片 X 修正：原实现返回 `4 - k`，比实际多 1——`k` 级缺失时需新建的正是
/// PDPT/PD/PT 中自 `k` 起的那 `3 - k` 张表（叶 PT 已含在内），而非"3 个中间层 + 1 个叶 PT"。
/// 多出的那帧被分配后从不使用、也从不归还，**每个新建路径泄漏一帧**：小规模映射时不可见，
/// 逐 4 KiB 覆盖 128 MiB 直映射（64 张 PT）时泄漏 60+ 帧并撑爆 `MAX_TABLE_FRAMES`。
fn count_missing(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> Result<usize, VspaceError> {
    let idx = indices(va);
    let mut table = root_phys;
    for (k, &slot) in idx[..3].iter().enumerate() {
        let entry = read_raw(map(table) + (slot as u64) * 8);
        if entry & PT_PRESENT == 0 {
            return Ok(3 - k);
        }
        if entry & PT_HUGE != 0 {
            return Err(VspaceError::BadRoot);
        }
        table = entry & PT_ADDR_MASK;
    }
    Ok(0)
}

/// Count a contiguous range's missing table pages without changing the root.
/// Exclusive access to this root is required through the later commit.
pub fn missing_range_tables(
    root: u64,
    base: u64,
    pages: usize,
    map: impl Fn(u64) -> u64,
) -> Result<usize, VspaceError> {
    let mut last = [u64::MAX; 3];
    let mut needed = 0;
    for page in 0..pages {
        let va = base
            .checked_add(
                (page as u64)
                    .checked_mul(4096)
                    .ok_or(VspaceError::BadRoot)?,
            )
            .ok_or(VspaceError::BadRoot)?;
        let mut table = root;
        for (level, index) in indices(va)[..3].iter().enumerate() {
            let entry = read_raw(map(table) + 8 * *index as u64);
            if entry & PT_PRESENT == 0 {
                for (depth, previous) in last.iter_mut().enumerate().skip(level) {
                    let key = va >> (39 - 9 * depth);
                    if *previous != key {
                        needed += 1;
                        *previous = key;
                    }
                }
                break;
            }
            if entry & PT_HUGE != 0 {
                return Err(VspaceError::BadRoot);
            }
            table = entry & PT_ADDR_MASK;
        }
    }
    Ok(needed)
}

/// 建立 `va → frame_phys` 的 4KiB 映射，缺页表页时经 `alloc` 取新帧。
///
/// 两阶段：先统计缺失页表页并**一次性**分配；任一分配失败则返回 [`VspaceError::NoFrame`]
/// 且不产生任何写入（对分配失败保持原子）。随后沿路径装载，叶 PTE 按 `writable` 设位。
#[allow(clippy::too_many_arguments)] // Page context plus atomic allocation/rollback hooks.
pub fn map_4k(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    mut alloc: impl FnMut() -> Option<u64>,
    frame_phys: u64,
    writable: bool,
    user: bool,
    mut rollback: impl FnMut(u64),
) -> Result<(), VspaceError> {
    let needed = count_missing(root_phys, va, &map)?;
    let mut fresh = [0u64; 4];
    for index in 0..needed {
        match alloc() {
            Some(frame) => fresh[index] = frame,
            None => {
                for frame in &fresh[..index] {
                    rollback(*frame);
                }
                return Err(VspaceError::NoFrame);
            }
        }
    }

    // 中间层同样需要 U/S 位：CPL3 必须能穿越每一级；否则用户访问会在中间层被判为
    // supervisor-only 而保护性 #PF（取指时表现为 P=1,US=1,ID=1）。
    let intermediate = table_flags() | if user { PT_USER } else { 0 };

    let idx = indices(va);
    let mut table = root_phys;
    let mut fresh_iter = fresh[..needed].iter();
    for &slot in &idx[..3] {
        let entry_addr = map(table) + (slot as u64) * 8;
        let entry = read_raw(entry_addr);
        if entry & PT_PRESENT == 0 {
            let frame = *fresh_iter.next().expect("count_missing matched alloc");
            zero_table(map(frame));
            write_raw(entry_addr, frame | intermediate);
            table = frame;
        } else {
            // 已存在的中间项可能是 supervisor-only（如切片 E 建的），补上 U/S。
            if user && entry & PT_USER == 0 {
                write_raw(entry_addr, entry | PT_USER);
            }
            table = entry & PT_ADDR_MASK;
        }
    }
    write_raw(
        map(table) + (idx[3] as u64) * 8,
        frame_phys | pte_flags(writable, user),
    );
    Ok(())
}

/// 清除 `va` 的叶映射（present=0），并自底向上回收**变空的、由本内核分配的**页表结点。
///
/// 只回收登记在帧注册表里的页表帧（`release` 回调对 bootloader 页表帧返回 `Err`，此处忽略），
/// 故不会误放共享的上级表。返回是否确实清除了一个存在的叶映射。
pub fn unmap_4k(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    mut release: impl FnMut(u64) -> Result<(), VspaceError>,
) -> Result<(), VspaceError> {
    let idx = indices(va);
    let mut tables = [0u64; 4];
    tables[0] = root_phys;
    let mut table = root_phys;
    for level in 0..3 {
        let entry = read_raw(map(table) + (idx[level] as u64) * 8);
        if entry & PT_PRESENT == 0 {
            return Err(VspaceError::BadRoot);
        }
        if entry & PT_HUGE != 0 {
            return Err(VspaceError::BadRoot);
        }
        table = entry & PT_ADDR_MASK;
        tables[level + 1] = table;
    }
    let leaf_addr = map(table) + (idx[3] as u64) * 8;
    if read_raw(leaf_addr) & PT_PRESENT == 0 {
        return Err(VspaceError::NotMapped);
    }
    write_raw(leaf_addr, 0);
    // 自底向上：某一级表已无 present 项则清父项并归还该表帧。
    let mut level = 3;
    while level > 0 {
        let child = tables[level];
        let empty = (0..512).all(|slot| read_raw(map(child) + (slot as u64) * 8) & PT_PRESENT == 0);
        if !empty {
            break;
        }
        let parent = tables[level - 1];
        write_raw(map(parent) + (idx[level - 1] as u64) * 8, 0);
        let _ = release(child);
        level -= 1;
    }
    Ok(())
}

/// 改写已有映射的叶 PTE（中间层级必须已存在，否则 [`VspaceError::BadRoot`]）。
pub fn set_leaf(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    frame_phys: u64,
    writable: bool,
    user: bool,
) -> Result<(), VspaceError> {
    let table = walk_to_leaf_table(root_phys, va, &map)?;
    let idx = indices(va);
    write_raw(
        map(table) + (idx[3] as u64) * 8,
        frame_phys | pte_flags(writable, user),
    );
    Ok(())
}

/// 读取已有映射的叶 PTE 原始值（含标志位）。
pub fn read_leaf(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> Result<u64, VspaceError> {
    let table = walk_to_leaf_table(root_phys, va, &map)?;
    let idx = indices(va);
    Ok(read_raw(map(table) + (idx[3] as u64) * 8))
}

/// 沿已存在的中间层走到叶所在 PT 的物理帧；缺层返回 `BadRoot`。
fn walk_to_leaf_table(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
) -> Result<u64, VspaceError> {
    let idx = indices(va);
    let mut table = root_phys;
    for &slot in &idx[..3] {
        let entry = read_raw(map(table) + (slot as u64) * 8);
        if entry & PT_PRESENT == 0 {
            return Err(VspaceError::BadRoot);
        }
        if entry & PT_HUGE != 0 {
            return Err(VspaceError::BadRoot);
        }
        table = entry & PT_ADDR_MASK;
    }
    Ok(table)
}

/// 解析 `va` 的**叶物理地址**，支持 2 MiB / 1 GiB 大页。
///
/// [`crate::cpu::translate`] 只支持 4 KiB 叶（遇 `PS` 位直接 `PageNotMapped`），而
/// bootloader 根对物理直映射与启动栈用的是大页。自建根必须把 bootloader 根里已有的
/// 映射逐页复刻成 4 KiB，故需要一个能读大页的解析器。
///
/// 返回 `None` 表示路径缺失或叶项 present=0。
pub fn leaf_phys_or_none(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> Option<u64> {
    let idx = indices(va);
    let mut table = root_phys;
    let mut level = 0;
    while level < 3 {
        let entry = read_raw(map(table) + (idx[level] as u64) * 8);
        if entry & PT_PRESENT == 0 {
            return None;
        }
        if entry & PT_HUGE != 0 {
            // 大页：1 GiB（level 1，PDPT）或 2 MiB（level 2，PD）。页内偏移直接取自 VA 低位。
            let page_size = match level {
                1 => 1u64 << 30,
                2 => 1u64 << 21,
                _ => return None,
            };
            // 大页的 bit 12 是 PAT 标志，不属于物理基址；基址按页大小对齐。
            return Some((entry & PT_ADDR_MASK & !(page_size - 1)) + (va & (page_size - 1)));
        }
        table = entry & PT_ADDR_MASK;
        level += 1;
    }
    let entry = read_raw(map(table) + (idx[3] as u64) * 8);
    if entry & PT_PRESENT == 0 {
        None
    } else {
        Some((entry & PT_ADDR_MASK) + (va & (crate::mem::FRAME_SIZE - 1)))
    }
}

/// 检查 `va` 所在的 PML4 槽是否空闲（未被 bootloader 占用）。
pub fn slot_free(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> bool {
    let idx = indices(va);
    read_raw(map(root_phys) + (idx[0] as u64) * 8) & PT_PRESENT == 0
}

/// 把 `src_root` 的全部 512 个 PML4 项复制到 `dst_root`。
///
/// 复制是共享上级页表（不深拷贝），因此 `dst_root` 继承 `src_root` 的全部映射：内核镜像
/// 在 i4=256、bootloader 低映射在 i4=0。切换 CR3 后内核代码/数据/直映射仍然可见，这是
/// 「写 CR3 后取指合法」的前提。`dst_root` 必须是已清零的独立帧。
pub fn clone_root(src_root: u64, dst_root: u64, map: impl Fn(u64) -> u64) {
    let src = map(src_root);
    let dst = map(dst_root);
    for index in 0..512 {
        let value = read_raw(src + (index as u64) * 8);
        write_raw(dst + (index as u64) * 8, value);
    }
}

/// A new exec root shares only supervisor PML4 subtrees.
pub fn clone_supervisor_root(src: u64, dst: u64, map: impl Fn(u64) -> u64) {
    for index in 0..512u64 {
        let entry = read_raw(map(src) + 8 * index);
        write_raw(
            map(dst) + 8 * index,
            if entry & PT_USER == 0 { entry } else { 0 },
        );
    }
}

/// Check user access at every level, rather than trusting the leaf alone.
pub fn read_user_leaf(root: u64, va: u64, map: impl Fn(u64) -> u64) -> Result<u64, VspaceError> {
    read_owned_user_leaf(root, va, map, false)
}

pub(crate) fn read_owned_user_leaf(
    root: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    include_hidden: bool,
) -> Result<u64, VspaceError> {
    if va == 0 || va >> 47 != 0 {
        return Err(VspaceError::BadRoot);
    }
    let mut table = root;
    for (level, index) in indices(va).into_iter().enumerate() {
        let entry = read_raw(map(table) + 8 * index as u64);
        if entry & PT_PRESENT == 0
            || entry & PT_USER == 0 && !(include_hidden && level == 3 && entry & PT_HIDDEN != 0)
            || level < 3 && entry & PT_HUGE != 0
        {
            return Err(VspaceError::BadRoot);
        }
        if level == 3 {
            return Ok(entry);
        }
        table = entry & PT_ADDR_MASK;
    }
    Err(VspaceError::BadRoot)
}

/// 清零 `root` 的第 `slot` 个 PML4 项，用于隔离任务的用户槽（互不可见）。
pub fn zero_slot(root: u64, slot: usize, map: impl Fn(u64) -> u64) {
    write_raw(map(root) + (slot as u64) * 8, 0);
}

/// 切片 X：自建根需要复刻的一段**连续** `VA → 物理` 映射（逐 4 KiB 页）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfMap {
    pub va: u64,
    pub phys: u64,
    pub pages: usize,
    /// supervisor 可写（内核代码/数据、直映射、启动栈一律如此；W^X 未做，见设计 §5）。
    pub writable: bool,
}

/// 自建根流程中登记的表帧上限（回滚用；实测约 74 帧，留足余量）。
pub const MAX_SELF_ROOT_TABLES: usize = 256;

/// 切片 X：自建根的映射面（标量部分；两个逐页物理帧解析器另行传入）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfRootSpec {
    /// 物理直映射（恒等区间 `offset + 0 → [offset, offset + max_phys)`）。
    pub direct: SelfMap,
    pub image_va: u64,
    pub image_pages: usize,
    pub stack_va: u64,
    pub stack_pages: usize,
}

/// 切片 X：构造**自建内核根**——自有 PML4 与全部中间页表，**不复用 bootloader 页表的任何一级**。
///
/// 逐页复刻：内核镜像（`spec.image_va`/`image_pages`，物理帧由 `image_phys(index)` 给出）、
/// 物理直映射（恒等区间）、启动栈（物理帧由 `stack_phys(index)` 给出）。页表帧一律经 `alloc`
/// 取得（内核路径为 `frames::allocate_table_frame`），故每个中间表帧都有**保留页表帧归属**、
/// 可被 W6 守恒与帧注册表审计——这正是 COSIR §20.5 登记「不证明子页表物理所有权」所缺的那一环。
///
/// **镜像必须逐页给出物理帧**：内核 ELF 是 PIE，bootloader 按程序头逐段装载，各段
/// `vaddr = file_offset + k*4096`（偏移量逐段不同），故"整段镜像 VA→phys 单一常量偏移"不成立，
/// 统一偏移会在段边界之后读到错误物理页（实测该校验可捕获）。源根没有该镜像页映射时**跳过**，
/// 与源根保持同一有效 VA 集合；启动栈页缺失则确定性失败（不留半张根）。
///
/// 任一步失败即归还**本次**已分配的全部表帧并返回错误，不泄漏归属。
///
/// 调用方须保证 `map`（物理→虚拟）在本函数执行期间可用——即仍在旧根下运行，故本函数必须
/// **在切换 CR3 之前**调用。
pub fn build_self_root(
    map: impl Fn(u64) -> u64,
    mut alloc: impl FnMut() -> Option<u64>,
    spec: SelfRootSpec,
    mut image_phys: impl FnMut(usize) -> Option<u64>,
    mut stack_phys: impl FnMut(usize) -> Option<u64>,
) -> Result<u64, VspaceError> {
    let mut owned = [0u64; MAX_SELF_ROOT_TABLES];
    let mut owned_count = 0usize;

    let outcome = {
        let mut take =
            |owned: &mut [u64; MAX_SELF_ROOT_TABLES], owned_count: &mut usize| -> Option<u64> {
                let frame = alloc()?;
                if *owned_count == MAX_SELF_ROOT_TABLES {
                    // 容量不足：归还刚取到的这一帧，交由调用方按 NoFrame 失败。
                    let _ = crate::frames::release_table_frame(frame);
                    return None;
                }
                owned[*owned_count] = frame;
                *owned_count += 1;
                Some(frame)
            };

        (|| -> Result<u64, VspaceError> {
            let root = take(&mut owned, &mut owned_count).ok_or(VspaceError::NoFrame)?;
            zero_table(map(root));
            for index in 0..spec.image_pages {
                let Some(phys) = image_phys(index) else {
                    continue;
                };
                map_4k(
                    root,
                    spec.image_va + (index as u64) * crate::mem::FRAME_SIZE,
                    &map,
                    || take(&mut owned, &mut owned_count),
                    phys,
                    true,
                    false,
                    |_| {},
                )?;
            }
            for index in 0..spec.direct.pages {
                let delta = (index as u64) * crate::mem::FRAME_SIZE;
                map_4k(
                    root,
                    spec.direct.va + delta,
                    &map,
                    || take(&mut owned, &mut owned_count),
                    spec.direct.phys + delta,
                    spec.direct.writable,
                    false,
                    |_| {},
                )?;
            }
            for index in 0..spec.stack_pages {
                let phys = stack_phys(index).ok_or(VspaceError::BadRoot)?;
                map_4k(
                    root,
                    spec.stack_va + (index as u64) * crate::mem::FRAME_SIZE,
                    &map,
                    || take(&mut owned, &mut owned_count),
                    phys,
                    true,
                    false,
                    |_| {},
                )?;
            }
            Ok(root)
        })()
    };

    match outcome {
        Ok(root) => Ok(root),
        Err(error) => {
            for frame in owned[..owned_count].iter().copied() {
                let _ = crate::frames::release_table_frame(frame);
            }
            Err(error)
        }
    }
}

/// 一次自建映射见证的实测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Witness {
    pub va: u64,
    pub frame_a: u64,
    pub frame_b: u64,
    /// 映射到 A 后经 VA 读回的值（应 [`MAGIC_A`]）：证明 PTE 驱动翻译。
    pub read_before: u64,
    /// 改叶指向 B、未刷新时经 VA 读回的值（信息：陈旧=真 TLB，亦可能 QEMU 已失效）。
    pub stale_read: u64,
    /// `invlpg` 后经 VA 读回的值（应 [`MAGIC_B`]）：证明真实页表更新 + TLB 刷新。
    pub read_after: u64,
    /// `cpu::translate` 对同一 VA 解析出的物理帧（应等于 `frame_b`）。
    pub translated: u64,
    /// 写只读页 #PF 的硬件 error code（应 0x3）。
    pub wp_err: u64,
    /// 写只读页 #PF 的 CR2（应等于 `va`）。
    pub wp_addr: u64,
}

/// 在 `root_phys` 下运行一次自建映射见证（内核/QEMU 专用，host 不可执行）。
///
/// 步骤：分配 A/B 两帧并写入魔数 → 建 `VA→A` 映射并读回 → 改叶 `VA→B` 观察刷新
/// 前后的读值 → `cpu::translate` 对账 → 改叶为只读并触发真实写保护 #PF。
pub fn run_witness(root_phys: u64, offset: u64) -> Result<Witness, VspaceError> {
    let map = |phys: u64| crate::mem::phys_to_virt(phys, offset);
    if !slot_free(root_phys, VSPACE_VA, map) {
        return Err(VspaceError::SlotOccupied);
    }

    // S1-X3 / 切片 R：A/B 是数据帧，归入见证 Segment（真实 `SegmentObject` + 内核持有的
    // 可授权根 Capability）；页表帧走保留类。
    //
    // 本区域保留**内核直映射**（见 `bootstrap::platform_segment_is_direct`）：见证需要
    // `set_leaf` 换叶重映射，超出 `memseg_map` 的语义。
    let (_, frames) = crate::bootstrap::platform_create_segment(
        crate::frames::SEG_VSPACE,
        2,
        crate::rights::Rights::EMPTY,
        offset,
    )
    .map_err(|_| VspaceError::NoFrame)?;
    let frame_a = frames.frame_at(0).ok_or(VspaceError::NoFrame)?;
    let frame_b = frames.frame_at(1).ok_or(VspaceError::NoFrame)?;
    zero_table(map(frame_a));
    zero_table(map(frame_b));
    write_raw(map(frame_a), MAGIC_A);
    write_raw(map(frame_b), MAGIC_B);

    map_4k(
        root_phys,
        VSPACE_VA,
        map,
        || crate::frames::allocate_table_frame().ok(),
        frame_a,
        true,
        false,
        |frame| {
            let _ = crate::frames::release_table_frame(frame);
        },
    )?;
    prima_ostd::arch::invlpg(VSPACE_VA);
    let read_before = read_raw(VSPACE_VA);

    set_leaf(root_phys, VSPACE_VA, map, frame_b, true, false)?;
    let stale_read = read_raw(VSPACE_VA); // 信息：刷新前
    prima_ostd::arch::invlpg(VSPACE_VA);
    let read_after = read_raw(VSPACE_VA);

    let translated = crate::cpu::translate(map(root_phys), VSPACE_VA, map).unwrap_or(0);

    // 只读叶 + 写 → 真实 `present∧write` #PF（classify → WriteProtection）。
    set_leaf(root_phys, VSPACE_VA, map, frame_a, false, false)?;
    prima_ostd::arch::invlpg(VSPACE_VA);
    crate::idt::trigger_page_fault_write(VSPACE_VA);
    let wp_err = crate::idt::last_fault_err();
    let wp_addr = crate::idt::fault_addr();

    Ok(Witness {
        va: VSPACE_VA,
        frame_a,
        frame_b,
        read_before,
        stale_read,
        read_after,
        translated,
        wp_err,
        wp_addr,
    })
}

/// Visit the complete 4 KiB user mapping tree without borrowing page memory.
pub fn visit_user_leaves(
    root: u64,
    map: impl Fn(u64) -> u64 + Copy,
    mut visit: impl FnMut(u64, u64) -> Result<(), VspaceError>,
) -> Result<(), VspaceError> {
    fn walk(
        table: u64,
        level: u32,
        prefix: u64,
        map: impl Fn(u64) -> u64 + Copy,
        visit: &mut impl FnMut(u64, u64) -> Result<(), VspaceError>,
    ) -> Result<(), VspaceError> {
        for index in 0..512u64 {
            let entry = read_raw(map(table) + 8 * index);
            if entry & PT_PRESENT == 0
                || entry & PT_USER == 0 && !(level == 1 && entry & PT_HIDDEN != 0)
            {
                continue;
            }
            let va = prefix | (index << (12 + 9 * (level - 1)));
            if va >> 47 != 0 || level > 1 && entry & PT_HUGE != 0 {
                return Err(VspaceError::BadRoot);
            }
            if level == 1 {
                visit(va, entry)?;
            } else {
                walk(entry & PT_ADDR_MASK, level - 1, va, map, visit)?;
            }
        }
        Ok(())
    }
    walk(root, 4, 0, map, &mut visit)
}

pub fn write_leaf_raw(
    root: u64,
    va: u64,
    value: u64,
    map: impl Fn(u64) -> u64,
) -> Result<(), VspaceError> {
    let table = walk_to_leaf_table(root, va, &map)?;
    write_raw(map(table) + indices(va)[3] as u64 * 8, value);
    Ok(())
}

/// Clone an inactive child root, then write-protect the parent's user pages.
///
/// Requires distinct valid roots, a zeroed destination, exclusive page-table access,
/// and non-global 4 KiB user mappings. Supervisor subtrees remain shared. No child can run
/// until this returns. `alloc` returns exclusive table frames; `release` must
/// reclaim them without failure. On Err all new tables are released, dst is zero,
/// and src is unchanged. User huge pages are rejected rather than shared writable.
///
/// `flush_parent` MUST synchronously invalidate the current CPU's parent user
/// translations after PTE protection, before returning to any parent user code.
/// Prima currently has one CPU and no PCID; SMP needs remote shootdown as well.
/// Callbacks must not panic or reenter page-table mutation.
pub fn clone_root_cow<F: Fn(u64) -> u64 + Copy>(
    src_root: u64,
    dst_root: u64,
    map: F,
    mut alloc: impl FnMut() -> Option<u64>,
    mut release: impl FnMut(u64),
    flush_parent: impl FnOnce(),
) -> Result<(), VspaceError> {
    if src_root == dst_root {
        return Err(VspaceError::BadRoot);
    }
    if let Err(error) = clone_level(dst_root, src_root, 4, map, &mut alloc) {
        release_cow_tables(dst_root, 4, map, &mut release);
        return Err(error);
    }
    // No fallible work remains: only now alter the parent and invalidate stale
    // writable translations. A C return may immediately write the parent's stack.
    protect_cow_parent(src_root, 4, map);
    flush_parent();
    Ok(())
}

fn clone_level<F: Fn(u64) -> u64 + Copy>(
    dst_table: u64,
    src_table: u64,
    level: u32,
    map: F,
    alloc: &mut impl FnMut() -> Option<u64>,
) -> Result<(), VspaceError> {
    for index in 0..512u64 {
        let entry = read_raw(map(src_table) + index * 8);
        let at = map(dst_table) + index * 8;
        if entry & PT_PRESENT == 0
            || entry & PT_USER == 0 && !(level == 1 && entry & PT_HIDDEN != 0)
        {
            write_raw(at, entry);
        } else if level == 1 {
            // A CR3 reload does not evict global translations. User globals are
            // outside this prototype's mapping contract; reject rather than leak.
            if entry & (1 << 8) != 0 {
                return Err(VspaceError::BadRoot);
            }
            write_raw(at, cow_leaf(entry));
        } else {
            if entry & PT_HUGE != 0 {
                return Err(VspaceError::BadRoot);
            }
            let next = alloc().ok_or(VspaceError::NoFrame)?;
            zero_table(map(next));
            write_raw(at, next | (entry & !PT_ADDR_MASK));
            clone_level(next, entry & PT_ADDR_MASK, level - 1, map, alloc)?;
        }
    }
    Ok(())
}

pub(crate) fn cow_leaf(entry: u64) -> u64 {
    let entry = entry | PT_SHARED;
    if entry & PT_WRITABLE != 0 {
        (entry & !PT_WRITABLE) | PT_COW
    } else {
        entry
    }
}

fn protect_cow_parent(table: u64, level: u32, map: impl Fn(u64) -> u64 + Copy) {
    for index in 0..512u64 {
        let at = map(table) + index * 8;
        let entry = read_raw(at);
        if entry & PT_PRESENT == 0
            || entry & PT_USER == 0 && !(level == 1 && entry & PT_HIDDEN != 0)
        {
            continue;
        }
        if level == 1 {
            write_raw(at, cow_leaf(entry));
        } else {
            protect_cow_parent(entry & PT_ADDR_MASK, level - 1, map);
        }
    }
}

/// Release only private page-table nodes of an inactive COW clone; never data
/// frames or shared supervisor subtrees. Also clears the supplied root, which
/// remains owned by the caller. Valid before child publication, not after edits.
pub fn discard_cow_root(root: u64, map: impl Fn(u64) -> u64 + Copy, mut release: impl FnMut(u64)) {
    release_cow_tables(root, 4, map, &mut release);
}

fn release_cow_tables(
    table: u64,
    level: u32,
    map: impl Fn(u64) -> u64 + Copy,
    release: &mut impl FnMut(u64),
) {
    for index in 0..512u64 {
        let at = map(table) + index * 8;
        let entry = read_raw(at);
        if level > 1 && entry & (PT_PRESENT | PT_USER) == (PT_PRESENT | PT_USER) {
            let next = entry & PT_ADDR_MASK;
            release_cow_tables(next, level - 1, map, release);
            release(next);
        }
        write_raw(at, 0);
    }
}

/// M17b：解析一次 COW 写故障——复制叶帧并把 PTE 恢复为可写。
///
/// 要求目标页 present + user + `PT_COW`。返回新帧物理地址；调用方负责
/// `invlpg` 后重试指令。
pub fn resolve_cow<F: Fn(u64) -> u64 + Copy>(
    root: u64,
    va: u64,
    map: F,
    mut alloc: impl FnMut() -> Option<u64>,
) -> Result<u64, VspaceError> {
    let idx = indices(va);
    let pml4 = read_raw(map(root) + idx[0] as u64 * 8);
    if pml4 & (PT_PRESENT | PT_USER | PT_WRITABLE) != (PT_PRESENT | PT_USER | PT_WRITABLE) {
        return Err(VspaceError::BadRoot);
    }
    let pdpt = read_raw(map(pml4 & PT_ADDR_MASK) + idx[1] as u64 * 8);
    if pdpt & (PT_PRESENT | PT_USER | PT_WRITABLE) != (PT_PRESENT | PT_USER | PT_WRITABLE)
        || pdpt & PT_HUGE != 0
    {
        return Err(VspaceError::BadRoot);
    }
    let pd = read_raw(map(pdpt & PT_ADDR_MASK) + idx[2] as u64 * 8);
    if pd & (PT_PRESENT | PT_USER | PT_WRITABLE) != (PT_PRESENT | PT_USER | PT_WRITABLE)
        || pd & PT_HUGE != 0
    {
        return Err(VspaceError::BadRoot);
    }
    let pt = map(pd & PT_ADDR_MASK);
    let pte = read_raw(pt + idx[3] as u64 * 8);
    if pte & PT_PRESENT == 0 || pte & PT_USER == 0 || pte & PT_COW == 0 {
        return Err(VspaceError::BadRoot);
    }
    let old = pte & PT_ADDR_MASK;
    let Some(new) = alloc() else {
        return Err(VspaceError::NoFrame);
    };
    let src = map(old);
    let dst = map(new);
    for word in 0..512u64 {
        write_raw(dst + word * 8, read_raw(src + word * 8));
    }
    let flags = (pte | PT_WRITABLE) & !(PT_COW | PT_SHARED);
    write_raw(
        pt + idx[3] as u64 * 8,
        (new & PT_ADDR_MASK) | (flags & !PT_ADDR_MASK),
    );
    Ok(new)
}

/// Bounded user transfers touch at most two 4 KiB pages. This is an internal
/// mechanism limit, not an address authorization or a machine ABI promise.
pub const USER_COPY_LIMIT: usize = 4096;

#[derive(Clone, Copy, Default)]
struct UserChunk {
    physical: u64,
    len: usize,
}

fn user_chunks(ptr: u64, len: usize) -> Result<[UserChunk; 2], VspaceError> {
    let end = ptr.checked_add(len as u64).ok_or(VspaceError::BadRoot)?;
    if len > USER_COPY_LIMIT || len != 0 && (ptr == 0 || end > 1 << 47) {
        return Err(VspaceError::BadRoot);
    }
    let first = len.min(4096 - (ptr & 4095) as usize);
    Ok([
        UserChunk {
            physical: ptr,
            len: first,
        },
        UserChunk {
            physical: ptr + first as u64,
            len: len - first,
        },
    ])
}

/// All output pages are validated and private before this value is returned.
/// The caller must keep the root, mappings and frames exclusively owned until
/// write completes; do not store this across scheduling or a VM mutation.
/// This is a physical write plan, never a capability or a user-visible handle.
pub struct PreparedUserWrite {
    chunks: [UserChunk; 2],
    len: usize,
}

impl PreparedUserWrite {
    /// Commit without another fallible page walk or allocation.
    pub fn write(self, bytes: &[u8], map: impl Fn(u64) -> u64) {
        assert_eq!(
            bytes.len(),
            self.len,
            "preflight covers the complete output"
        );
        let mut copied = 0;
        for chunk in self.chunks {
            for index in 0..chunk.len {
                prima_ostd::mm::write_u8_at(
                    map(chunk.physical),
                    index as u64,
                    bytes[copied + index],
                );
            }
            copied += chunk.len;
        }
    }
}

/// Explicit-root copy-in. Validate the entire range before modifying the kernel
/// buffer, including user access on intermediate levels. The caller owns the
/// root for the whole operation; no implicit current-root lookup occurs here.
pub fn read_user_bytes(
    root: u64,
    ptr: u64,
    output: &mut [u8],
    map: impl Fn(u64) -> u64 + Copy,
) -> Result<(), VspaceError> {
    let mut chunks = user_chunks(ptr, output.len())?;
    for chunk in &mut chunks {
        if chunk.len != 0 {
            let leaf = read_user_leaf(root, chunk.physical, map)?;
            chunk.physical = (leaf & PT_ADDR_MASK) + (chunk.physical & 4095);
        }
    }
    let mut copied = 0;
    for chunk in chunks {
        for index in 0..chunk.len {
            output[copied + index] = prima_ostd::mm::read_u8_at(map(chunk.physical), index as u64);
        }
        copied += chunk.len;
    }
    Ok(())
}

/// Preflight a complete output, including writable intermediate page tables.
/// Reserve every COW frame before changing any PTE; allocation failure releases
/// all reservations, preserving PTEs, bytes and frame ownership. Allocation and
/// release callbacks receive the destination page so different Segment owners
/// can be handled without an implicit current Task. Allocated frames must be
/// fresh, owned and 4 KiB aligned; physical frame zero is valid. No callback may
/// schedule or mutate these mappings. `invalidate` must invalidate this root (or be a no-op
/// for an inactive root whose next activation flushes the TLB).
pub fn prepare_user_write(
    root: u64,
    ptr: u64,
    len: usize,
    map: impl Fn(u64) -> u64 + Copy,
    mut alloc: impl FnMut(u64) -> Option<u64>,
    mut release: impl FnMut(u64, u64),
    mut invalidate: impl FnMut(u64),
) -> Result<PreparedUserWrite, VspaceError> {
    let mut chunks = user_chunks(ptr, len)?;
    let mut slots = [(0, 0); 2];
    let mut reserved = [None; 2];
    // No allocation or mapping change until all permissions have passed.
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.len != 0 {
            slots[index] = user_write_leaf_slot(root, chunk.physical, map)?;
        }
    }
    for index in 0..2 {
        if chunks[index].len != 0 && slots[index].1 & PT_WRITABLE == 0 {
            let page = chunks[index].physical & !4095;
            let Some(frame) = alloc(page) else {
                for previous in (0..index).rev() {
                    if let Some(frame) = reserved[previous] {
                        release(chunks[previous].physical & !4095, frame);
                    }
                }
                return Err(VspaceError::NoFrame);
            };
            reserved[index] = Some(frame);
        }
    }
    // Nothing below can fail under the exclusive-root contract. Stage the full
    // original pages before committing either PTE; no fault-driven recursion.
    for index in 0..2 {
        if let Some(frame) = reserved[index] {
            let old = map(slots[index].1 & PT_ADDR_MASK);
            let new = map(frame);
            for word in 0..512u64 {
                write_raw(new + word * 8, read_raw(old + word * 8));
            }
        }
    }
    for index in 0..2 {
        if chunks[index].len == 0 {
            continue;
        }
        let (slot, old) = slots[index];
        let frame = if let Some(frame) = reserved[index] {
            let flags = (old | PT_WRITABLE) & !(PT_ADDR_MASK | PT_COW | PT_SHARED);
            write_raw(slot, frame | flags);
            invalidate(chunks[index].physical & !4095);
            frame
        } else {
            old & PT_ADDR_MASK
        };
        chunks[index].physical = frame + (chunks[index].physical & 4095);
    }
    Ok(PreparedUserWrite { chunks, len })
}

fn user_write_leaf_slot(
    root: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
) -> Result<(u64, u64), VspaceError> {
    let mut table = root;
    for (level, index) in indices(va).into_iter().enumerate() {
        let slot = map(table) + index as u64 * 8;
        let entry = read_raw(slot);
        if entry & (PT_PRESENT | PT_USER) != (PT_PRESENT | PT_USER) {
            return Err(VspaceError::BadRoot);
        }
        if level == 3 {
            return if entry & (PT_WRITABLE | PT_COW) != 0 {
                Ok((slot, entry))
            } else {
                Err(VspaceError::BadRoot)
            };
        }
        if entry & PT_HUGE != 0 || entry & PT_WRITABLE == 0 {
            return Err(VspaceError::BadRoot);
        }
        table = entry & PT_ADDR_MASK;
    }
    Err(VspaceError::BadRoot)
}

/// Write an int output through the preflighted physical plan.
pub fn write_user_u32(
    root: u64,
    ptr: u64,
    value: [u8; 4],
    map: impl Fn(u64) -> u64 + Copy,
    mut alloc: impl FnMut() -> Option<u64>,
    mut release: impl FnMut(u64),
    invalidate: impl FnMut(u64),
) -> Result<(), VspaceError> {
    prepare_user_write(
        root,
        ptr,
        4,
        map,
        |_| alloc(),
        |_, frame| release(frame),
        invalidate,
    )?
    .write(&value, map);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PagePool;

    fn identity(phys: u64) -> u64 {
        phys
    }

    fn table_snapshot(root: u64, level: u32, out: &mut Vec<(u64, Vec<u64>)>) {
        let words: Vec<u64> = (0..512).map(|i| read_raw(root + i * 8)).collect();
        if level > 1 {
            for &entry in &words {
                if entry & (PT_PRESENT | PT_USER) == PT_PRESENT | PT_USER && entry & PT_HUGE == 0 {
                    table_snapshot(entry & PT_ADDR_MASK, level - 1, out);
                }
            }
        }
        out.push((root, words));
    }

    #[test]
    fn hidden_and_readonly_leaves_are_retained_and_shared_on_fork() {
        let mut pool = PagePool::new();
        let parent = pool.alloc().unwrap();
        let child = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        let va = 0x400000;
        map_4k(
            parent,
            va,
            identity,
            || pool.alloc(),
            data,
            false,
            true,
            |_| {},
        )
        .unwrap();
        let original = read_leaf(parent, va, identity).unwrap();
        write_leaf_raw(parent, va, (original & !PT_USER) | PT_HIDDEN, identity).unwrap();
        clone_root_cow(parent, child, identity, || pool.alloc(), |_| {}, || {}).unwrap();
        let p = read_leaf(parent, va, identity).unwrap();
        let c = read_leaf(child, va, identity).unwrap();
        assert_eq!(p, c);
        assert_eq!(p & PT_ADDR_MASK, data);
        assert_eq!(p & (PT_HIDDEN | PT_SHARED), PT_HIDDEN | PT_SHARED);
        assert_eq!(p & (PT_USER | PT_WRITABLE), 0);
        assert!(read_user_leaf(child, va, identity).is_err());
        let mut pages = Vec::new();
        visit_user_leaves(parent, identity, |address, _| {
            pages.push(address);
            Ok(())
        })
        .unwrap();
        assert_eq!(pages, [va]);
        // Reenabling RW still leaves a write fault, then obtains a private copy.
        write_leaf_raw(child, va, (c | PT_USER | PT_COW) & !PT_HIDDEN, identity).unwrap();
        let private = resolve_cow(child, va, identity, || pool.alloc()).unwrap();
        assert_ne!(private, data);
        let c = read_leaf(child, va, identity).unwrap();
        assert_eq!(c & (PT_COW | PT_SHARED | PT_HIDDEN), 0);
        assert_ne!(c & PT_WRITABLE, 0);
        assert_eq!(read_leaf(parent, va, identity).unwrap(), p);
    }

    #[test]
    fn map_returns_each_unlinked_table_reservation_on_partial_allocation_failure() {
        for budget in 0..3 {
            let mut pool = PagePool::new();
            let root = pool.alloc().unwrap();
            let data = pool.alloc().unwrap();
            let mut allocated = Vec::new();
            let mut released = Vec::new();
            let result = map_4k(
                root,
                0x400000,
                identity,
                || {
                    if allocated.len() == budget {
                        return None;
                    }
                    let frame = pool.alloc()?;
                    allocated.push(frame);
                    Some(frame)
                },
                data,
                true,
                true,
                |frame| released.push(frame),
            );
            assert_eq!(result, Err(VspaceError::NoFrame));
            assert_eq!(allocated, released);
            assert!((0..512).all(|index| read_raw(root + 8 * index) == 0));
        }
    }

    #[test]
    fn range_planning_counts_shared_prefixes_once_across_pt_boundaries() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        assert_eq!(missing_range_tables(root, 0x1ff000, 2, identity), Ok(4));
        map_4k(
            root,
            0x1ff000,
            identity,
            || pool.alloc(),
            data,
            true,
            true,
            |_| {},
        )
        .unwrap();
        assert_eq!(missing_range_tables(root, 0x200000, 1, identity), Ok(1));
    }

    #[test]
    fn cow_clone_rolls_back_every_allocation_failure_and_flushes_only_after_commit() {
        // Two PML4 slots and two PTs in the first: seven private table pages.
        for budget in 0..=7 {
            let mut pool = PagePool::new();
            let src = pool.alloc().unwrap();
            let dst = pool.alloc().unwrap();
            let data = pool.alloc().unwrap();
            let vas = [VSPACE_VA, VSPACE_VA + 0x20_0000, VSPACE_VA * 2];
            for va in vas {
                map_4k(src, va, identity, || pool.alloc(), data, true, true, |_| {}).unwrap();
            }
            let mut before = Vec::new();
            table_snapshot(src, 4, &mut before);
            let mut allocated = Vec::new();
            let mut released = Vec::new();
            let mut flushes = 0;
            let result = clone_root_cow(
                src,
                dst,
                identity,
                || {
                    if allocated.len() == budget {
                        return None;
                    }
                    // Parent must be untouched throughout the fallible phase.
                    for (table, words) in &before {
                        for (i, word) in words.iter().enumerate() {
                            assert_eq!(read_raw(*table + i as u64 * 8), *word);
                        }
                    }
                    let frame = pool.alloc().unwrap();
                    // The production allocator may return dirty released frames.
                    for i in 0..512 {
                        write_raw(frame + i * 8, u64::MAX);
                    }
                    allocated.push(frame);
                    Some(frame)
                },
                |frame| released.push(frame),
                || {
                    flushes += 1;
                    for va in vas {
                        for root in [src, dst] {
                            let leaf = read_leaf(root, va, identity).unwrap();
                            assert_eq!(leaf & PT_WRITABLE, 0);
                            assert_ne!(leaf & PT_COW, 0);
                        }
                    }
                },
            );
            if budget < 7 {
                assert_eq!(result, Err(VspaceError::NoFrame));
                assert_eq!(flushes, 0);
                let mut after = Vec::new();
                table_snapshot(src, 4, &mut after);
                assert_eq!(after, before);
                assert!((0..512).all(|i| read_raw(dst + i * 8) == 0));
            } else {
                assert_eq!(result, Ok(()));
                assert_eq!(flushes, 1);
                assert!(released.is_empty());
                discard_cow_root(dst, identity, |frame| released.push(frame));
            }
            allocated.sort();
            released.sort();
            assert_eq!(
                allocated, released,
                "every private table must be reclaimed exactly once"
            );
            assert!(
                !released.contains(&src) && !released.contains(&dst) && !released.contains(&data)
            );
        }
    }

    #[test]
    fn cow_clone_rejects_user_huge_pages_and_alias_without_changing_parent() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        map_4k(
            src,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            data,
            true,
            true,
            |_| {},
        )
        .unwrap();
        let pdpt = read_raw(src + indices(VSPACE_VA)[0] as u64 * 8) & PT_ADDR_MASK;
        write_raw(
            pdpt + 8,
            0x4000_0000 | PT_PRESENT | PT_USER | PT_WRITABLE | PT_HUGE,
        );
        let mut before = Vec::new();
        table_snapshot(src, 4, &mut before);
        let mut allocated = Vec::new();
        let mut released = Vec::new();
        assert_eq!(
            clone_root_cow(
                src,
                dst,
                identity,
                || {
                    let frame = pool.alloc().unwrap();
                    allocated.push(frame);
                    Some(frame)
                },
                |frame| released.push(frame),
                || panic!("failed clone must not flush")
            ),
            Err(VspaceError::BadRoot)
        );
        let mut after = Vec::new();
        table_snapshot(src, 4, &mut after);
        assert_eq!(before, after);
        allocated.sort();
        released.sort();
        assert_eq!(allocated, released);
        assert!((0..512).all(|i| read_raw(dst + i * 8) == 0));
        assert_eq!(
            clone_root_cow(
                src,
                src,
                identity,
                || panic!("alias"),
                |_| panic!("alias"),
                || panic!("alias")
            ),
            Err(VspaceError::BadRoot)
        );
    }

    #[test]
    fn cow_resolve_preserves_nx_and_supervisor_siblings_are_never_protected() {
        const NX: u64 = 1 << 63;
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        let supervisor = pool.alloc().unwrap();
        map_4k(
            src,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            data,
            true,
            true,
            |_| {},
        )
        .unwrap();
        map_4k(
            src,
            VSPACE_VA + 4096,
            identity,
            || pool.alloc(),
            supervisor,
            true,
            false,
            |_| {},
        )
        .unwrap();
        let pt = walk_to_leaf_table(src, VSPACE_VA, identity).unwrap();
        let at = pt + indices(VSPACE_VA)[3] as u64 * 8;
        write_raw(at, read_raw(at) | NX);
        let sup_before = read_leaf(src, VSPACE_VA + 4096, identity).unwrap();
        clone_root_cow(src, dst, identity, || pool.alloc(), |_| {}, || {}).unwrap();
        assert_eq!(
            read_leaf(src, VSPACE_VA + 4096, identity).unwrap(),
            sup_before
        );
        assert_eq!(
            read_leaf(dst, VSPACE_VA + 4096, identity).unwrap(),
            sup_before
        );
        resolve_cow(dst, VSPACE_VA, identity, || pool.alloc()).unwrap();
        let leaf = read_leaf(dst, VSPACE_VA, identity).unwrap();
        assert_ne!(leaf & NX, 0);
        assert_ne!(leaf & PT_WRITABLE, 0);
        assert_eq!(leaf & PT_COW, 0);
    }

    #[test]
    fn user_int_write_resolves_both_cow_pages_and_preserves_neighbor_bytes_and_parent() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let a = pool.alloc().unwrap();
        let b = pool.alloc().unwrap();
        for (va, data) in [(VSPACE_VA, a), (VSPACE_VA + 4096, b)] {
            map_4k(src, va, identity, || pool.alloc(), data, true, true, |_| {}).unwrap();
            for word in 0..512 {
                write_raw(data + word * 8, u64::MAX);
            }
        }
        clone_root_cow(src, dst, identity, || pool.alloc(), |_| {}, || {}).unwrap();
        let mut invalidated = Vec::new();
        let value = [0x78, 0x56, 0x34, 0x12];
        write_user_u32(
            dst,
            VSPACE_VA + 4094,
            value,
            identity,
            || pool.alloc(),
            |_| panic!("successful reservation"),
            |va| invalidated.push(va),
        )
        .unwrap();
        assert_eq!(invalidated, [VSPACE_VA, VSPACE_VA + 4096]);
        let pa = read_leaf(dst, VSPACE_VA, identity).unwrap() & PT_ADDR_MASK;
        let pb = read_leaf(dst, VSPACE_VA + 4096, identity).unwrap() & PT_ADDR_MASK;
        assert_eq!(read_raw(pa + 4088), 0x5678_ffff_ffff_ffff);
        assert_eq!(read_raw(pb), 0xffff_ffff_ffff_1234);
        assert_eq!(read_raw(a + 4088), u64::MAX);
        assert_eq!(read_raw(b), u64::MAX);
        // Rewriting an already-private page needs no allocation/invalidation.
        write_user_u32(
            dst,
            VSPACE_VA,
            value,
            identity,
            || panic!("already writable"),
            |_| panic!("no reservation"),
            |_| panic!("already private"),
        )
        .unwrap();
        assert_eq!(read_raw(pa), 0xffff_ffff_1234_5678);
    }

    #[test]
    fn user_int_write_rejects_bad_ranges_and_all_protection_levels_without_data_changes() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            data,
            true,
            true,
            |_| {},
        )
        .unwrap();
        write_raw(data, 0xdead_beef_face_feed);
        for ptr in [0, u64::MAX - 1, 0x0000_7fff_ffff_fffe, VSPACE_VA + 4094] {
            assert_eq!(
                write_user_u32(
                    root,
                    ptr,
                    [0; 4],
                    identity,
                    || panic!("invalid"),
                    |_| panic!("no reservation"),
                    |_| panic!("invalid")
                ),
                Err(VspaceError::BadRoot)
            );
        }
        let idx = indices(VSPACE_VA);
        let mut table = root;
        for (level, slot) in idx.into_iter().enumerate() {
            let at = table + slot as u64 * 8;
            let saved = read_raw(at);
            for bit in [PT_USER, PT_WRITABLE, PT_PRESENT] {
                write_raw(at, saved & !bit);
                let mut input = [0xa5; 4];
                let read = read_user_bytes(root, VSPACE_VA, &mut input, identity);
                if bit == PT_WRITABLE {
                    assert_eq!(read, Ok(()));
                    assert_eq!(input, [0xed, 0xfe, 0xce, 0xfa]);
                } else {
                    assert_eq!(read, Err(VspaceError::BadRoot));
                    assert_eq!(input, [0xa5; 4]);
                }
                assert_eq!(
                    write_user_u32(
                        root,
                        VSPACE_VA,
                        [0; 4],
                        identity,
                        || panic!("invalid permission"),
                        |_| panic!("no reservation"),
                        |_| panic!("invalid permission")
                    ),
                    Err(VspaceError::BadRoot),
                    "level {level}, bit {bit}"
                );
                write_raw(at, saved);
            }
            table = saved & PT_ADDR_MASK;
        }
        assert_eq!(read_raw(data), 0xdead_beef_face_feed);
    }

    #[test]
    fn user_int_write_allocation_failure_never_partially_writes_bytes() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        for va in [VSPACE_VA, VSPACE_VA + 4096] {
            map_4k(src, va, identity, || pool.alloc(), data, true, true, |_| {}).unwrap();
        }
        write_raw(data, u64::MAX);
        write_raw(data + 4088, u64::MAX);
        clone_root_cow(src, dst, identity, || pool.alloc(), |_| {}, || {}).unwrap();
        let mut allocations = 0;
        let mut released = Vec::new();
        let mut before = Vec::new();
        table_snapshot(dst, 4, &mut before);
        assert_eq!(
            write_user_u32(
                dst,
                VSPACE_VA + 4094,
                [0; 4],
                identity,
                || {
                    allocations += 1;
                    if allocations == 1 { pool.alloc() } else { None }
                },
                |frame| released.push(frame),
                |_| panic!("failed reservation must not invalidate")
            ),
            Err(VspaceError::NoFrame)
        );
        assert_eq!(released.len(), 1);
        let mut after = Vec::new();
        table_snapshot(dst, 4, &mut after);
        assert_eq!(before, after);
        let first = read_leaf(dst, VSPACE_VA, identity).unwrap() & PT_ADDR_MASK;
        assert_eq!(read_raw(first + 4088), u64::MAX);
        assert_eq!(read_raw(data), u64::MAX);
    }

    #[test]
    fn bounded_copy_selects_explicit_roots_at_the_same_va() {
        let mut pool = PagePool::new();
        let roots = [pool.alloc().unwrap(), pool.alloc().unwrap()];
        for (which, root) in roots.into_iter().enumerate() {
            for page in 0..2 {
                let data = pool.alloc().unwrap();
                map_4k(
                    root,
                    VSPACE_VA + page * 4096,
                    identity,
                    || pool.alloc(),
                    data,
                    true,
                    true,
                    |_| {},
                )
                .unwrap();
                for offset in 0..4096 {
                    prima_ostd::mm::write_u8_at(
                        data,
                        offset,
                        if which == 0 { offset as u8 } else { 0xa5 },
                    );
                }
            }
        }
        for len in [0, 64, 4096] {
            let address = VSPACE_VA + 4090;
            let mut scratch = [0; USER_COPY_LIMIT];
            read_user_bytes(roots[0], address, &mut scratch[..len], identity).unwrap();
            prepare_user_write(
                roots[1],
                address,
                len,
                identity,
                |_| panic!("already writable"),
                |_, _| panic!("no reservation"),
                |_| panic!("no COW"),
            )
            .unwrap()
            .write(&scratch[..len], identity);
            let mut actual = [0; USER_COPY_LIMIT];
            read_user_bytes(roots[1], address, &mut actual[..len], identity).unwrap();
            assert_eq!(actual[..len], scratch[..len]);
        }
        let mut whole = [0; 4096];
        read_user_bytes(roots[0], VSPACE_VA, &mut whole, identity).unwrap();
        assert!(
            whole
                .iter()
                .enumerate()
                .all(|(index, byte)| *byte == index as u8)
        );
        let mut guard = [0; 6];
        read_user_bytes(roots[1], VSPACE_VA + 4084, &mut guard, identity).unwrap();
        assert_eq!(guard, [0xa5; 6]);
        read_user_bytes(roots[1], VSPACE_VA + 8186, &mut guard, identity).unwrap();
        assert_eq!(guard, [0xa5; 6]);
    }

    #[test]
    fn bounded_copy_preflights_ranges_and_all_source_pages_before_touching_buffers() {
        // Empty transfers do not dereference even an invalid root or pointer.
        for ptr in [0, u64::MAX] {
            read_user_bytes(0, ptr, &mut [], |_| panic!("empty read")).unwrap();
            prepare_user_write(
                0,
                ptr,
                0,
                |_| panic!("empty write"),
                |_| panic!("empty allocation"),
                |_, _| panic!("empty release"),
                |_| panic!("empty TLB"),
            )
            .unwrap()
            .write(&[], |_| panic!("empty commit"));
        }
        for (ptr, len) in [
            (0, 1),
            (u64::MAX, 1),
            ((1 << 47) - 1, 2),
            (1 << 47, 1),
            (VSPACE_VA, 4097),
        ] {
            let mut bytes = vec![0xa5; len];
            assert_eq!(
                read_user_bytes(0, ptr, &mut bytes, |_| panic!("invalid range")),
                Err(VspaceError::BadRoot)
            );
            assert_eq!(bytes, vec![0xa5; len]);
            assert!(matches!(
                prepare_user_write(
                    0,
                    ptr,
                    len,
                    |_| panic!("invalid range"),
                    |_| panic!("invalid allocation"),
                    |_, _| panic!("invalid release"),
                    |_| panic!("invalid TLB")
                ),
                Err(VspaceError::BadRoot)
            ));
        }
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            data,
            true,
            true,
            |_| {},
        )
        .unwrap();
        for offset in 0..4096 {
            prima_ostd::mm::write_u8_at(data, offset, 0x5a);
        }
        let mut bytes = [0xa5; 96];
        assert_eq!(
            read_user_bytes(root, VSPACE_VA + 4080, &mut bytes, identity),
            Err(VspaceError::BadRoot)
        );
        assert_eq!(bytes, [0xa5; 96]);
        // A COW first page must stay shared if the second page is absent/RO/hidden.
        let old = read_leaf(root, VSPACE_VA, identity).unwrap();
        write_leaf_raw(root, VSPACE_VA, cow_leaf(old), identity).unwrap();
        let second = pool.alloc().unwrap();
        map_4k(
            root,
            VSPACE_VA + 4096,
            identity,
            || pool.alloc(),
            second,
            false,
            true,
            |_| {},
        )
        .unwrap();
        let readonly = read_leaf(root, VSPACE_VA + 4096, identity).unwrap();
        for flags in [0, readonly, (readonly & !PT_USER) | PT_HIDDEN] {
            write_leaf_raw(root, VSPACE_VA + 4096, flags, identity).unwrap();
            assert!(matches!(
                prepare_user_write(
                    root,
                    VSPACE_VA + 4080,
                    96,
                    identity,
                    |_| panic!("both pages must pass before allocating"),
                    |_, _| panic!("no reservation"),
                    |_| panic!("no COW commit")
                ),
                Err(VspaceError::BadRoot)
            ));
            assert_eq!(read_leaf(root, VSPACE_VA, identity).unwrap(), cow_leaf(old));
            assert_eq!(read_raw(data + 4088), 0x5a5a5a5a5a5a5a5a);
        }
    }

    #[test]
    fn bounded_copy_accepts_a_reserved_physical_frame_zero() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let old = pool.alloc().unwrap();
        let zero_frame = pool.alloc().unwrap();
        let map = |phys| if phys < 4096 { zero_frame + phys } else { phys };
        map_4k(
            root,
            VSPACE_VA,
            map,
            || pool.alloc(),
            old,
            true,
            true,
            |_| {},
        )
        .unwrap();
        write_raw(old, u64::MAX);
        let pte = read_leaf(root, VSPACE_VA, map).unwrap();
        write_leaf_raw(root, VSPACE_VA, cow_leaf(pte), map).unwrap();
        let mut invalidated = Vec::new();
        prepare_user_write(
            root,
            VSPACE_VA + 1,
            2,
            map,
            |_| Some(0),
            |_, _| panic!("successful reservation"),
            |page| invalidated.push(page),
        )
        .unwrap()
        .write(&[0x12, 0x34], map);
        assert_eq!(invalidated, [VSPACE_VA]);
        assert_eq!(read_leaf(root, VSPACE_VA, map).unwrap() & PT_ADDR_MASK, 0);
        assert_eq!(read_raw(zero_frame), 0xffffffffff3412ff);
        assert_eq!(read_raw(old), u64::MAX);
    }

    #[test]
    fn bounded_copy_rolls_back_real_frame_ownership_on_second_cow_allocation_failure() {
        use crate::frames::{FrameOwner, FrameRegistry, Pool};
        use crate::mem::{FrameInventory, FreeFrameAllocator};
        use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
        use std::cell::RefCell;
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        for page in [VSPACE_VA, VSPACE_VA + 4096] {
            map_4k(
                root,
                page,
                identity,
                || pool.alloc(),
                data,
                true,
                true,
                |_| {},
            )
            .unwrap();
            let old = read_leaf(root, page, identity).unwrap();
            write_leaf_raw(root, page, cow_leaf(old), identity).unwrap();
        }
        let spare = pool.alloc().unwrap();
        let image = pool.alloc().unwrap();
        let regions = [MemoryRegion {
            start: spare,
            end: spare + 4096,
            kind: MemoryRegionKind::Usable,
        }];
        let inventory = FrameInventory {
            usable_frames: 1,
            image_frames: 1,
            free_frames: 1,
            usable_regions: 1,
            kernel_phys_start: image,
            kernel_phys_end: image + 4096,
        };
        let alloc = FreeFrameAllocator::from_regions(&regions, image, image + 4096).unwrap();
        let registry = RefCell::new(FrameRegistry::new(&inventory, alloc).unwrap());
        for segment in [2, 3] {
            registry
                .borrow_mut()
                .open_segment(segment, Pool::Cpu)
                .unwrap();
        }
        let before = registry.borrow().witness();
        let mut tables_before = Vec::new();
        table_snapshot(root, 4, &mut tables_before);
        let owner = |page| if page == VSPACE_VA { 2 } else { 3 };
        let mut released = Vec::new();
        assert!(matches!(
            prepare_user_write(
                root,
                VSPACE_VA + 4080,
                96,
                identity,
                |page| registry.borrow_mut().allocate_for_segment(owner(page)).ok(),
                |page, frame| {
                    released.push((page, frame));
                    registry
                        .borrow_mut()
                        .release_frame(owner(page), frame)
                        .unwrap();
                },
                |_| panic!("failed preflight cannot invalidate")
            ),
            Err(VspaceError::NoFrame)
        ));
        assert_eq!(released, [(VSPACE_VA, spare)]);
        assert_eq!(registry.borrow().witness(), before);
        assert_eq!(registry.borrow().owner_of(spare), FrameOwner::Free);
        let mut tables_after = Vec::new();
        table_snapshot(root, 4, &mut tables_after);
        assert_eq!(tables_before, tables_after);
        assert_eq!(
            registry.borrow_mut().allocate_for_segment(3).unwrap(),
            spare
        );
    }

    #[test]
    fn cow_clone_shares_frames_and_resolve_copies_on_write() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();
        map_4k(
            src,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            true,
            |_| {},
        )
        .unwrap();
        write_raw(frame, 0x1122_3344_5566_7788);
        clone_root_cow(src, dst, identity, || pool.alloc(), |_| {}, || {}).unwrap();

        let src_leaf = read_leaf(src, VSPACE_VA, identity).unwrap();
        assert_eq!(src_leaf & PT_ADDR_MASK, frame);
        assert_eq!(src_leaf & PT_WRITABLE, 0);
        assert_ne!(src_leaf & PT_COW, 0);
        let dst_leaf = read_leaf(dst, VSPACE_VA, identity).unwrap();
        assert_eq!(dst_leaf & PT_ADDR_MASK, frame);
        assert_ne!(dst_leaf & PT_COW, 0);

        let new = resolve_cow(dst, VSPACE_VA, identity, || pool.alloc()).unwrap();
        assert_ne!(new, frame);
        assert_eq!(read_raw(new), 0x1122_3344_5566_7788);
        let dst_leaf = read_leaf(dst, VSPACE_VA, identity).unwrap();
        assert_eq!(dst_leaf & PT_ADDR_MASK, new);
        assert_ne!(dst_leaf & PT_WRITABLE, 0);
        assert_eq!(dst_leaf & PT_COW, 0);
        // 父页仍共享旧帧、保持 COW。
        let src_leaf = read_leaf(src, VSPACE_VA, identity).unwrap();
        assert_eq!(src_leaf & PT_ADDR_MASK, frame);
        assert_ne!(src_leaf & PT_COW, 0);
        assert_eq!(read_raw(frame), 0x1122_3344_5566_7788);
    }

    #[test]
    fn cow_clone_keeps_readonly_shared_and_copies_supervisor_entries() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        let ro = pool.alloc().unwrap();
        let sup = pool.alloc().unwrap();
        map_4k(
            src,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            ro,
            false,
            true,
            |_| {},
        )
        .unwrap();
        map_4k(
            src,
            VSPACE_VA + 0x1000,
            identity,
            || pool.alloc(),
            sup,
            true,
            false,
            |_| {},
        )
        .unwrap();
        clone_root_cow(src, dst, identity, || pool.alloc(), |_| {}, || {}).unwrap();

        let ro_leaf = read_leaf(dst, VSPACE_VA, identity).unwrap();
        assert_eq!(ro_leaf & PT_ADDR_MASK, ro);
        assert_eq!(ro_leaf & PT_COW, 0);
        // supervisor 映射原样复制（同一 PTE，不复制页表）。
        let src_sup = read_leaf(src, VSPACE_VA + 0x1000, identity).unwrap();
        let dst_sup = read_leaf(dst, VSPACE_VA + 0x1000, identity).unwrap();
        assert_eq!(src_sup, dst_sup);
        assert_eq!(src_sup & PT_ADDR_MASK, sup);
    }

    #[test]
    fn pte_flags_sets_present_and_writable() {
        assert_eq!(pte_flags(true, false), PT_PRESENT | PT_WRITABLE);
        assert_eq!(pte_flags(false, false), PT_PRESENT);
        assert_eq!(pte_flags(true, true), PT_PRESENT | PT_WRITABLE | PT_USER);
        assert_eq!(pte_flags(false, true), PT_PRESENT | PT_USER);
        assert_eq!(table_flags(), PT_PRESENT | PT_WRITABLE);
    }

    #[test]
    fn indices_decode_vspace_va_into_slot_one() {
        assert_eq!(indices(VSPACE_VA), [1, 0, 0, 0]);
        // 叶索引测试：VA + 0x3000 → i1 = 3
        assert_eq!(indices(VSPACE_VA + 0x3000), [1, 0, 0, 3]);
    }

    #[test]
    fn map_4k_builds_all_levels_and_reads_leaf() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
            |_| {},
        )
        .unwrap();

        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame | pte_flags(true, false)
        );
        // 逐级 present：PML4[1] -> PDPT -> PD -> PT
        let idx = indices(VSPACE_VA);
        let pml4e = read_raw(root + (idx[0] as u64) * 8);
        assert!(pml4e & PT_PRESENT != 0);
        let pdpt = pml4e & PT_ADDR_MASK;
        let pdpte = read_raw(pdpt + (idx[1] as u64) * 8);
        assert!(pdpte & PT_PRESENT != 0);
        let pd = pdpte & PT_ADDR_MASK;
        let pde = read_raw(pd + (idx[2] as u64) * 8);
        assert!(pde & PT_PRESENT != 0);
    }

    #[test]
    fn map_4k_reuses_existing_path() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame_a = pool.alloc().unwrap();
        let frame_b = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame_a,
            true,
            false,
            |_| {},
        )
        .unwrap();
        let pages_after_first = pool.len();
        // 同一路径的第二条映射只改叶，不新建页表页。
        map_4k(
            root,
            VSPACE_VA + 0x1000,
            identity,
            || pool.alloc(),
            frame_b,
            true,
            false,
            |_| {},
        )
        .unwrap();
        assert_eq!(pool.len(), pages_after_first);

        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame_a | pte_flags(true, false)
        );
        assert_eq!(
            read_leaf(root, VSPACE_VA + 0x1000, identity).unwrap(),
            frame_b | pte_flags(true, false)
        );
    }

    #[test]
    fn map_4k_no_frame_leaves_state_untouched() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        // 空分配器：需要 4 个页表页，必然失败。
        let result = map_4k(
            root,
            VSPACE_VA,
            identity,
            || None,
            frame,
            true,
            false,
            |_| {},
        );
        assert_eq!(result, Err(VspaceError::NoFrame));
        // 未产生任何写入：PML4[1] 仍为 0。
        assert_eq!(read_raw(root + 8), 0);
    }

    #[test]
    fn four_k_walk_rejects_a_huge_page_in_the_path() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let pdpt = pool.alloc().unwrap();
        let data = pool.alloc().unwrap();
        let va = VSPACE_VA;
        let idx = indices(va);
        write_raw(root + idx[0] as u64 * 8, pdpt | PT_PRESENT);
        write_raw(pdpt + idx[1] as u64 * 8, data | PT_PRESENT | PT_HUGE);
        write_raw(data + idx[2] as u64 * 8, 0x1234);

        assert_eq!(
            map_4k(
                root,
                va,
                identity,
                || pool.alloc(),
                data,
                true,
                false,
                |_| {},
            ),
            Err(VspaceError::BadRoot)
        );
        assert_eq!(read_leaf(root, va, identity), Err(VspaceError::BadRoot));
        assert_eq!(
            set_leaf(root, va, identity, data, true, false),
            Err(VspaceError::BadRoot)
        );
        assert_eq!(
            unmap_4k(root, va, identity, |_| Ok(())),
            Err(VspaceError::BadRoot)
        );
        assert_eq!(read_raw(data + idx[2] as u64 * 8), 0x1234);
    }

    #[test]
    fn set_leaf_can_clear_writable_for_write_protect() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
            |_| {},
        )
        .unwrap();
        set_leaf(root, VSPACE_VA, identity, frame, false, false).unwrap();
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame | pte_flags(false, false)
        );
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap() & PT_WRITABLE,
            0
        );
    }

    #[test]
    fn set_leaf_without_path_is_bad_root() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();
        assert_eq!(
            set_leaf(root, VSPACE_VA, identity, frame, true, false),
            Err(VspaceError::BadRoot)
        );
    }

    #[test]
    fn read_leaf_without_path_is_bad_root() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap(); // 全零根：i4=1 项缺失
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity),
            Err(VspaceError::BadRoot)
        );
    }

    /// 切片 X：自建根逐页复刻镜像（VA 与 phys 由调用方给出，**不假设统一偏移**）。
    ///
    /// 反例：镜像若按"单一常量偏移"复刻，段边界之后会读到错误物理页——该反例在真实内核上
    /// 实测过（ELF 各段 `vaddr = file_offset + k*4096`，偏移量逐段不同），见 SPEC-069 报告。
    #[test]
    fn build_self_root_maps_each_page_to_its_own_physical_frame() {
        let mut pool = PagePool::new();
        // 镜像两页：故意让 VA 与 phys 偏移在第二页发生变化（模拟 ELF 段边界）。
        let image = [0x50_0000u64, 0x60_1000u64];
        let stack = [0x70_0000u64];

        let root = build_self_root(
            identity,
            || pool.alloc(),
            SelfRootSpec {
                direct: SelfMap {
                    va: 0x8000_0000u64,
                    phys: 0,
                    pages: 2,
                    writable: true,
                },
                image_va: 0x4000_0000u64,
                image_pages: 2,
                stack_va: 0x90_0000_0000u64,
                stack_pages: 1,
            },
            |index| Some(image[index]),
            |index| Some(stack[index]),
        )
        .unwrap();

        assert_eq!(
            leaf_phys_or_none(root, 0x4000_0000, identity),
            Some(0x50_0000)
        );
        assert_eq!(
            leaf_phys_or_none(root, 0x4000_1000, identity),
            Some(0x60_1000),
            "第二页必须用它自己的物理帧，而不是首帧 + 页偏移"
        );
        assert_eq!(
            leaf_phys_or_none(root, 0x4000_1234, identity),
            Some(0x60_1234)
        );
        assert_eq!(
            leaf_phys_or_none(root, 0x90_0000_0000, identity),
            Some(0x70_0000)
        );
        // 直映射是恒等区间。
        assert_eq!(leaf_phys_or_none(root, 0x8000_0000, identity), Some(0));
        assert_eq!(leaf_phys_or_none(root, 0x8000_1000, identity), Some(0x1000));
    }

    /// 切片 X：源根没有该镜像页时**跳过**（不建映射），与源根保持同一有效 VA 集合。
    #[test]
    fn build_self_root_skips_image_pages_absent_from_the_source() {
        let mut pool = PagePool::new();
        let root = build_self_root(
            identity,
            || pool.alloc(),
            SelfRootSpec {
                direct: SelfMap {
                    va: 0x8000_0000u64,
                    phys: 0,
                    pages: 1,
                    writable: true,
                },
                image_va: 0x4000_0000u64,
                image_pages: 3,
                stack_va: 0x90_0000_0000u64,
                stack_pages: 0,
            },
            |index| if index == 1 { None } else { Some(0x50_0000) },
            |_| None,
        )
        .unwrap();
        assert!(leaf_phys_or_none(root, 0x4000_0000, identity).is_some());
        assert_eq!(leaf_phys_or_none(root, 0x4000_1000, identity), None);
        assert!(leaf_phys_or_none(root, 0x4000_2000, identity).is_some());
    }

    /// 切片 X：源根的启动栈页可能缺失时必须确定性失败（不能建出半张根）。
    #[test]
    fn build_self_root_requires_every_stack_page() {
        let mut pool = PagePool::new();
        assert_eq!(
            build_self_root(
                identity,
                || pool.alloc(),
                SelfRootSpec {
                    direct: SelfMap {
                        va: 0x8000_0000u64,
                        phys: 0,
                        pages: 1,
                        writable: true,
                    },
                    image_va: 0x4000_0000u64,
                    image_pages: 0,
                    stack_va: 0x90_0000_0000u64,
                    stack_pages: 2,
                },
                |_| None,
                |index| if index == 1 { None } else { Some(0x70_0000) },
            ),
            Err(VspaceError::BadRoot)
        );
    }

    /// 切片 X：`leaf_phys_or_none` 必须能解析 2 MiB 大页（`cpu::translate` 只认 4 KiB 叶，
    /// 而 bootloader 根对直映射/启动栈用的是大页）。
    #[test]
    fn leaf_phys_or_none_resolves_two_mib_huge_pages() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let pdpt = pool.alloc().unwrap();
        let pd = pool.alloc().unwrap();
        let va = 0x4000_0000u64;
        let idx = indices(va);
        // PML4[i4] → PDPT，PDPT[i3] → PD，PD[i2] = 2 MiB 大页（PS=1）。
        write_raw(root + idx[0] as u64 * 8, pdpt | PT_PRESENT | PT_WRITABLE);
        write_raw(pdpt + idx[1] as u64 * 8, pd | PT_PRESENT | PT_WRITABLE);
        write_raw(
            pd + idx[2] as u64 * 8,
            0x20_0000 | PT_PRESENT | PT_WRITABLE | (1 << 7),
        );
        assert_eq!(leaf_phys_or_none(root, va, identity), Some(0x20_0000));
        assert_eq!(
            leaf_phys_or_none(root, va + 0x3_0000, identity),
            Some(0x23_0000),
            "大页内偏移必须取自 VA 低位"
        );
    }

    #[test]
    fn leaf_phys_or_none_ignores_huge_page_pat_bit() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let pdpt = pool.alloc().unwrap();
        let pd = pool.alloc().unwrap();
        let va = 0x4000_0000u64;
        let idx = indices(va);
        const PS: u64 = 1 << 7;
        const PAT: u64 = 1 << 12;

        write_raw(root + idx[0] as u64 * 8, pdpt | PT_PRESENT);
        write_raw(pdpt + idx[1] as u64 * 8, pd | PT_PRESENT);
        write_raw(pd + idx[2] as u64 * 8, 0x20_0000 | PT_PRESENT | PS | PAT);
        assert_eq!(leaf_phys_or_none(root, va, identity), Some(0x20_0000));
        assert_eq!(
            leaf_phys_or_none(root, va + 0x3000, identity),
            Some(0x20_3000)
        );

        write_raw(
            pdpt + idx[1] as u64 * 8,
            0x8000_0000 | PT_PRESENT | PS | PAT,
        );
        assert_eq!(leaf_phys_or_none(root, va, identity), Some(0x8000_0000));
        assert_eq!(
            leaf_phys_or_none(root, va + 0x3000, identity),
            Some(0x8000_3000)
        );
    }

    /// SPEC-069 修正：新建路径只应分配**恰好** `3 - k` 张表。原实现多分配一张且从不归还，
    /// 逐 4 KiB 覆盖 128 MiB 直映射时泄漏 60+ 帧并撑爆 `MAX_TABLE_FRAMES`。
    #[test]
    fn map_4k_fresh_path_allocates_exactly_the_tables_it_uses() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();
        let before = pool.len();
        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
            |_| {},
        )
        .unwrap();
        // 全新路径：PDPT + PD + PT，共 3 张。
        assert_eq!(pool.len() - before, 3);
        // 同路径第二条映射不新增表。
        let mid = pool.len();
        map_4k(
            root,
            VSPACE_VA + 0x1000,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
            |_| {},
        )
        .unwrap();
        assert_eq!(pool.len(), mid);
    }

    #[test]
    fn clone_root_copies_all_512_entries() {
        let mut pool = PagePool::new();
        let src = pool.alloc().unwrap();
        let dst = pool.alloc().unwrap();
        // src 的高半区（i4=256）与低槽（i4=1）各写一个可识别项，其余为零。
        write_raw(src + 256 * 8, 0xDEAD_B000 | PT_PRESENT | PT_WRITABLE);
        write_raw(src + 8, 0xBEEF_0000 | PT_PRESENT);
        clone_root(src, dst, identity);
        assert_eq!(
            read_raw(dst + 256 * 8),
            0xDEAD_B000 | PT_PRESENT | PT_WRITABLE
        );
        assert_eq!(read_raw(dst + 8), 0xBEEF_0000 | PT_PRESENT);
        // 共享而非深拷贝：其它 510 项逐字节对账。
        for index in 0..512usize {
            assert_eq!(
                read_raw(src + index as u64 * 8),
                read_raw(dst + index as u64 * 8)
            );
        }
    }

    #[test]
    fn zero_slot_clears_only_one_pml4_entry() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        write_raw(root + 20 * 8, 0x1234_5000 | PT_PRESENT | PT_USER);
        write_raw(root + 21 * 8, 0x1234_6000 | PT_PRESENT | PT_USER);
        zero_slot(root, 20, identity);
        assert_eq!(read_raw(root + 20 * 8), 0);
        // 只清目标槽：槽 21 与槽 256 不受影响。
        assert_eq!(read_raw(root + 21 * 8), 0x1234_6000 | PT_PRESENT | PT_USER);
        write_raw(root + 256 * 8, 0x9);
        zero_slot(root, 20, identity);
        assert_eq!(read_raw(root + 256 * 8), 0x9);
    }
}
