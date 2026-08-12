# BLK-002 细化续：MemorySegment 页表↔vspace 细化实现计划

> **历史执行记录（已由后续纠偏取代）：** 本文保留 2026-08-12 的计划、历史代码块和当时的验收预期，不能作为当前可执行指令。证据等级、术语和模型边界以 [`../specs/2026-08-12-refinement-audit-remediation-design.md`](../specs/2026-08-12-refinement-audit-remediation-design.md) 为准：SPEC-014 仅为 host `MODEL-TEST + REFINEMENT-DESIGN`，不是实际 x86_64 kernel refinement。
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Historical Goal (superseded):** 在 BLK-002 Bootstrap 细化基础上，曾计划把 MemorySegment 的 CR3/四级页表↔vspace 对应写进 COSIR 规范 §20。现行表述是：`cosir` 探针提供 host 四级索引树的 `MODEL-TEST + REFINEMENT-DESIGN`，不是 S1-D4/S2 真实内核蓝本的实现证据。

**Historical Architecture (superseded):** 本计划曾把 `pagewalk.rs` 描述为真实四级页表机制、把 `Pte` 字段描述为真实硬件位，并将 `memseg_alpha` 叙为 W6/W7/W8 的完整验证。现行边界是：它们是 host 四级索引树与 x86_64-inspired candidate encoding；直接检查 W6/W7，并仅以 `memseg_unmap` 的 entry 页 guard 覆盖有限 W8 检查。它们不表示真实 CR3/PTE，也不覆盖完整 W8 α、非叶权限合成、页表页物理身份、TLB/多核或原子更新窗口。

**Current harness output:** 历史代码块中任何旧 `PASS memseg ...` 行均已被 `PASS model-test memseg page-walk dual view` 和 `PASS model-test memseg W6/W7 checks` 取代。

**Tech Stack:** Rust nightly-2026-07-30、Rust 2024 edition、Cargo workspace 已有 `kernel`/`xtask`/`cosir`。`cosir` 是 host 二进制，无新依赖。

## Global Constraints

- 历史上曾以 `docs/superpowers/specs/2026-08-12-memseg-pagewalk-refinement-design.md` 为权威设计；现以 `docs/superpowers/specs/2026-08-12-refinement-audit-remediation-design.md` 为证据、术语和模型边界的权威来源。
- 固定 Rust 工具链 `nightly-2026-07-30`；`cosir` 是 host 二进制，不依赖 `bootloader`/`bootloader_api`，不触碰 real IDT/trap。
- **四级固定走到底**：任意 vaddr 经 Pml4→Pdpt→Pd→leaf 四级索引（39/30/21/12 位移）到 4KB 叶；**不做大页递归/分裂/合并**。
- **叶 PTE 双字段** `Pte{mapping: SegmentRef, frame_phys}`；`SegmentRef{segment, offset, access}` 是细化层完整引用视图（规范 §5.2 PageMapping 的忠实表达）。
- **不复用、不改动** `state.rs` 的简化 `PageMapping=(VirtAddr,u8)` 与 `Task.vspace`（exc_resume 探针专用）；本切片用细化侧 `BTreeMap<VirtAddr, SegmentRef>` 作为每 Task 的 vspace 双视图，写树必写视图。
- `MachineState`/`SegmentRef`/`Pte` 与细化迁移都是**细化层概念**：不新增对 POSIR 可见的一等原语，不进 POSIR 抽象正文。
- 不覆盖 task_create 入口细化（边界清单 #16）与异常/trap 递送（S1-D4）——保持登记者状态。
- Access 编码：复用 `state.rs` 位域常量 `ACCESS_R=0b001`、`ACCESS_W=0b010`、`ACCESS_X=0b100`；`allowed_access` 与 `SegmentRef.access` 同为 `u8` 位掩码，`access ⊆ allowed` 用 `(allowed & access)==access` 判定。
- 现有阶段 1 启动闭环与 `cargo xtask test` 不得被本计划破坏。
- 现有 `cosir` 的 exc_resume 12 + bootstrap 5 测试必须保持通过；本计划不改动既有的 `state.rs`/`ops.rs`/`abstract_ops.rs`/`machine.rs` 语义。
- `.obsidian/` 是未跟踪目录，不读取、不修改、不提交。
- 每个任务仅提交该任务列出的文件，不使用 `git add .` 或 `git add -A`。
- **注意**：`Prima OS 实现阶段.md` 与 `COSIR 原语规范.md` 当前有 BLK-002 会话遗留的未提交改动（`COSIR 原语规范.md` 是 markdown 表格格式化调整；`Prima OS 实现阶段.md` 是 SPEC-012 的 9→12 同步）。**不要**在本计划任务里勾选这些历史遗留文件；本计划只提交它明确列出的部分。
- 验收命令：`cargo test -p cosir` 全绿、`cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零警告、`cargo xtask test` 六条 PASS。

---

## Historical File Structure

- Create: `docs/superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md` — 本计划文件（最后提交）。
- Modify: `COSIR 原语规范.md` — 在既有 §20 追加 MemorySegment 细化小节（四级表、页表位编码表、四操作细化对应、W6/W7/W8 承接）。
- Create: `cosir/src/pagewalk.rs` — host 四级索引树机制 + SegmentRef/Pte/FrameAllocator + translate/map/unmap/collect_leaves。
- Create: `cosir/src/memseg_refine.rs` — RefineState + 四操作细化迁移 + memseg_alpha + 错误集 + 测试。
- Modify: `cosir/src/main.rs` — 声明新模块；在验收线束追加 PASS 输出。
- Modify: `Prima OS 实现阶段.md` — 新增 SPEC-014 证据、更新 BLK-002 描述与当前快照。

> 历史说明（已纠偏）：设计文档 §2 的逐级结构（`Pml4`/`Pdpt`/`Pd`/`Pt` 各异构 struct）在本计划以**递归 enum + 显式级别标记**实现，保留四级固定走到底、512 槽、4KB 叶的 host 模型形状。它是 x86_64-inspired candidate encoding，不是“真实四级”页表或真实 CR3/PTE 表达。

---

### Task 1: 固定当前验收基线，确认 workspace 可扩展

**Files:**
- 无源码改动（仅验证）。

**Interfaces:**
- Consumes: 现有 workspace（`kernel`/`xtask`/`cosir`）。
- Produces: 证明当前 `cargo xtask test` 与 `cargo test -p cosir` 通过，为后续不破坏闭环提供基线。

- [ ] **Step 1: 运行现有验收，确认基线**

Run：

```bash
cargo test -p cosir
cargo xtask test
```

Historical expected output（测试数和 PASS 字符串已过时；当前以实际输出和纠偏设计为准）：`cargo test -p cosir` 当时预期输出 `test result: ok. 17 passed; 0 failed`（12 exc_resume + 5 bootstrap）；`cargo xtask test` 当时预期输出尾部：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

不得继续本计划的其他任务，除非 `cargo xtask test` 退出码为 0 且 17 测试全过。

- [ ] **Step 2: 记录基线快照与遗留未提交改动**

Run：`git status --short`
Expected：`COSIR 原语规范.md` 与 `Prima OS 实现阶段.md` 已被修改（BPK 会话遗留）、`.obsidian/` 未跟踪。确认这些是**历史遗留**，本计划不在任何任务里把它们卷入新模块提交。不创建任何 PASS 证据外的写入。

---

### Task 2: 建立 pagewalk.rs——host 四级索引树机制 + 帧分配器

**Files:**
- Create: `cosir/src/pagewalk.rs`
- Modify: `cosir/src/main.rs`（声明 `mod pagewalk;`）

**Interfaces:**
- Consumes: `crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, SegId}`。
- Produces（Task 3 起 memseg_refine.rs 依赖）：
  - `pub const PAGE_SIZE: u64`（0x1000）
  - `pub type PhysAddr = u64`
  - `pub struct SegmentRef { pub segment: SegId, pub offset: u64, pub access: u8 }`
  - `pub struct Pte { pub present: bool, pub writable: bool, pub user: bool, pub nx: bool, pub frame_phys: PhysAddr, pub mapping: SegmentRef }`
  - `pub enum NonLeafLevel { Pml4, Pdpt, Pd }`
  - `pub enum PageTableNode { NonLeaf { level: NonLeafLevel, slots: Box<[Option<PageTableNode>; 512]> }, Leaf(Box<[Option<Pte>; 512]>) }`
  - `pub struct PageTable { pub root: PageTableNode }`
  - `pub struct FrameAllocator { pub universe: Vec<PhysAddr>, pub free: Vec<PhysAddr> }`
  - `pub fn new_task_table() -> PageTable`
  - `pub fn translate(table: &PageTable, vaddr: u64) -> Option<&Pte>`
  - `pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte)`
  - `pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> bool`
  - `pub fn collect_leaves(table: &PageTable, out: &mut BTreeMap<u64, SegmentRef>)`
  - `FrameAllocator::new(universe: &[PhysAddr])`、`alloc(&mut self) -> Option<PhysAddr>`、`free_frame(&mut self, p: PhysAddr)`

- [ ] **Step 1: 在 main.rs 声明 pagewalk 模块**

在 `cosir/src/main.rs` 顶部模块声明处（`mod machine;` 附近）加入：

```rust
mod pagewalk;
```

- [ ] **Step 2: 写失败测试（TDD 红灯）**

创建一个独立测试文件 `cosir/src/pagewalk_tests.rs`，写 fail 测试（引用尚不存在的 translate/map_vaddr/unmap_vaddr/collect_leaves/FrameAllocator）：

```rust
use crate::pagewalk::{
    FrameAllocator, Pte, SegmentRef, collect_leaves, map_vaddr, new_task_table, translate,
    unmap_vaddr,
};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, SegId};
use std::collections::BTreeMap;

fn rwx() -> u8 {
    ACCESS_R | ACCESS_W | ACCESS_X
}

fn pte(seg: SegId, off: u64, frame: PhysAddr) -> Pte {
    Pte {
        present: true,
        writable: true,
        user: true,
        nx: false,
        frame_phys: frame,
        mapping: SegmentRef {
            segment: seg,
            offset: off,
            access: rwx(),
        },
    }
}

fn base_tree() -> PageTable {
    // 在 0x4000_0000 放一个叶，物理帧 0x1000，segment 1 offset 0
    let mut t = new_task_table();
    map_vaddr(&mut t, 0x4000_0000, pte(1, 0, 0x1000));
    t
}

#[test]
fn translate_returns_leaf_after_map() {
    let t = base_tree();
    let got = translate(&t, 0x4000_0000).expect("mapped leaf present");
    assert_eq!(got.frame_phys, 0x1000);
    assert_eq!(got.mapping.segment, 1);
    assert!(got.mapping.access & ACCESS_R != 0);
}

#[test]
fn unmap_clears_leaf_and_reclaims_empty_tables() {
    let mut t = base_tree();
    assert!(translate(&t, 0x4000_0000).is_some());
    let empty = unmap_vaddr(&mut t, 0x4000_0000);
    assert_eq!(empty, true, "叶被清后整棵子树应回收为空");
    assert!(translate(&t, 0x4000_0000).is_none());
}

#[test]
fn collect_leaves_maps_vaddr_to_segment_ref() {
    let mut t = base_tree();
    map_vaddr(&mut t, 0x4001_0000, pte(1, 1, 0x2000));
    let mut leaves = BTreeMap::new();
    collect_leaves(&t, &mut leaves);
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[&0x4000_0000].offset, 0);
    assert_eq!(leaves[&0x4001_0000].offset, 1);
}

#[test]
fn frame_allocator_alloc_free_roundtrip() {
    let univ: Vec<PhysAddr> = vec![0x1000, 0x2000, 0x3000];
    let mut a = FrameAllocator::new(&univ);
    let p = a.alloc().expect("frame available");
    assert_eq!(p, 0x3000, "pop 取栈顶");
    assert_eq!(a.free.len(), 2);
    a.free_frame(p);
    assert!(a.free.contains(&p));
    assert_eq!(a.free.len(), 3);
}

#[test]
fn allocate_exhausts_universe_returns_none() {
    let univ: Vec<PhysAddr> = vec![0x1000];
    let mut a = FrameAllocator::new(&univ);
    assert!(a.alloc().is_some());
    assert!(a.alloc().is_none(), "耗尽后返回 None");
}
```

- [ ] **Step 3: 在 main.rs 挂上测试模块**

在 `cosir/src/main.rs`（`mod state;` 之后）加入：

```rust
#[cfg(test)]
mod pagewalk_tests;
```

- [ ] **Step 4: 运行测试确认编译失败（红灯）**

Run：`cargo test -p cosir pagewalk`
Expected：由于 `pagewalk` 模块中 `translate`/`map_vaddr` 等尚不存在，编译报 `cannot find function ... in this scope` 或类似 E0425/E0412、测试失败。

- [ ] **Step 5: 写最小实现 pagewalk.rs**

在 `cosir/src/pagewalk.rs` 写入：

```rust
#![allow(dead_code)] // 部分辅助待 memseg_refine 使用；本切片先保证规范保真度

use crate::state::SegId;
use std::collections::BTreeMap;

/// 4KB 页面大小。
pub const PAGE_SIZE: u64 = 0x1000;

/// 物理帧地址。
pub type PhysAddr = u64;

/// 规范 §5.2 PageMapping 的细化层忠实表达：指向活动 Segment 的某页及其访问权限。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentRef {
    pub segment: SegId,
    pub offset: u64, // PageOffset
    pub access: u8,  // 位掩码，复用 state::ACCESS_R/W/X
}

/// 4KB 叶页表项（方案 C：物理视图 + POSIR 引用视图并列）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pte {
    pub present: bool,   // real bit0
    pub writable: bool,  // real bit1
    pub user: bool,      // real bit2
    pub nx: bool,        // real bit63
    pub frame_phys: PhysAddr,
    pub mapping: SegmentRef,
}

/// 非叶级类型标记。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonLeafLevel {
    Pml4,
    Pdpt,
    Pd,
}

/// 四级页表结点：非叶（Pml4/Pdpt/Pd，每级 512 槽引用下一级）或叶（4KB 表）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageTableNode {
    NonLeaf {
        level: NonLeafLevel,
        slots: Box<[Option<PageTableNode>; 512]>,
    },
    Leaf(Box<[Option<Pte>; 512]>),
}

impl Default for PageTableNode {
    fn default() -> Self {
        PageTableNode::NonLeaf {
            level: NonLeafLevel::Pml4,
            slots: Default::default(),
        }
    }
}

/// 自根 PML4 起的一棵四级树。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PageTable {
    pub root: PageTableNode,
}

// 位移：bits 39/30/21 定位非叶 512 槽，12 定位叶槽。
fn pml4_index(v: u64) -> usize { ((v >> 39) & 0x1ff) as usize }
fn pdpt_index(v: u64) -> usize { ((v >> 30) & 0x1ff) as usize }
fn pd_index(v: u64) -> usize  { ((v >> 21) & 0x1ff) as usize }
fn pt_index(v: u64) -> usize  { ((v >> 12) & 0x1ff) as usize }

/// 新建一棵空树（根 PML4，全空）。
pub fn new_task_table() -> PageTable {
    PageTable::default()
}

/// 把 vaddr 翻译到叶；若路径表缺失返回 None。
pub fn translate(table: &PageTable, vaddr: u64) -> Option<&Pte> {
    let mut node = &table.root;
    for _ in 0..3 {
        match node {
            PageTableNode::NonLeaf { level, slots } => {
                let idx = match level {
                    NonLeafLevel::Pml4 => pml4_index(vaddr),
                    NonLeafLevel::Pdpt => pdpt_index(vaddr),
                    NonLeafLevel::Pd => pd_index(vaddr),
                };
                node = slots[idx].as_ref()?;
            }
            PageTableNode::Leaf(_) => panic!("translate: 路径不应提前到叶（Pml4 起始必有四级）"),
        }
    }
    match node {
        PageTableNode::NonLeaf { level: NonLeafLevel::Pd, slots } => {
            slots[pd_index(vaddr)].as_ref()
        }
        _ => None,
    }
}
```

> 上注：`translate` 手动展开四级（Pml4→Pdpt→Pd→leaf）。上面代码把前三层交给循环，最后一层显式取 `Pd` 的 `pt_index`。为保持与 `collect_leaves`/`map_vaddr` 的递归结构一致，**用下面的递归实现替换** `translate/map_vaddr/unmap_vaddr/collect_leaves`（Step 5 其余部分）：

```rust
// ---- 递归翻译 / 映射 / 回收 / 采集（统一用递归，避免手工展开不一致） ----

fn level_shift(level: NonLeafLevel) -> u32 {
    match level {
        NonLeafLevel::Pml4 => 39,
        NonLeafLevel::Pdpt => 30,
        NonLeafLevel::Pd => 21,
    }
}

fn child_level_for(parent: NonLeafLevel) -> Option<NonLeafLevel> {
    match parent {
        NonLeafLevel::Pml4 => Some(NonLeafLevel::Pdpt),
        NonLeafLevel::Pdpt => Some(NonLeafLevel::Pd),
        NonLeafLevel::Pd => None, // 最深非叶，其子为叶
    }
}

/// 翻译 vaddr 到叶 PTE 引用。
pub fn translate<'a>(table: &'a PageTable, vaddr: u64) -> Option<&'a Pte> {
    fn go<'a>(node: &'a PageTableNode, vaddr: u64) -> Option<&'a Pte> {
        match node {
            PageTableNode::Leaf(slots) => slots[pt_index(vaddr)].as_ref(),
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child = slots[idx].as_ref()?;
                go(child, vaddr)
            }
        }
    }
    go(&table.root, vaddr)
}

/// 在 vaddr 处映射一个叶 PTE；路径表缺失时逐级创建（仅 4KB，四级固定）。
pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte) {
    fn go(node: &mut PageTableNode, vaddr: u64, pte: &Pte) {
        match node {
            PageTableNode::Leaf(slots) => {
                slots[pt_index(vaddr)] = Some(*pte);
            }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                match slots[idx] {
                    None => {
                        // 最深非叶 (Pd) 的子是叶，否则是下一级非叶
                        match child_level_for(*level) {
                            Some(cl) => {
                                slots[idx] = Some(PageTableNode::NonLeaf {
                                    level: cl,
                                    slots: Default::default(),
                                });
                            }
                            None => {
                                slots[idx] = Some(PageTableNode::Leaf(Default::default()));
                            }
                        }
                    }
                    Some(_) => {}
                }
                go(slots[idx].as_mut().unwrap(), vaddr, pte);
            }
        }
    }
    go(&mut table.root, vaddr, &pte);
}

/// 清除 vaddr 处叶并向上回收完全空的表结点；返回该子树是否因此变为全空（根不收）。
pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> bool {
    fn go(node: &mut PageTableNode, vaddr: u64) -> bool {
        match node {
            PageTableNode::Leaf(slots) => {
                let _ = slots[pt_index(vaddr)].take();
                slots.iter().all(|o| o.is_none())
            }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child_empty = match slots[idx].as_mut() {
                    Some(child) => go(child, vaddr),
                    None => false,
                };
                if child_empty {
                    slots[idx] = None;
                }
                slots.iter().all(|o| o.is_none())
            }
        }
    }
    go(&mut table.root, vaddr)
}

/// 采集一棵树全部叶的 (vaddr → SegmentRef) 映射视图（细化侧 vspace 双视图来源）。
pub fn collect_leaves(table: &PageTable, out: &mut BTreeMap<u64, SegmentRef>) {
    fn go(node: &PageTableNode, base: u64, level_base_shift: u32, out: &mut BTreeMap<u64, SegmentRef>) {
        match node {
            PageTableNode::Leaf(slots) => {
                for (i, o) in slots.iter().enumerate() {
                    if let Some(pte) = o {
                        let vaddr = base + ((i as u64) << 12);
                        out.insert(vaddr, pte.mapping);
                    }
                }
            }
            PageTableNode::NonLeaf { level, slots } => {
                let shift = level_shift(*level);
                for (i, o) in slots.iter().enumerate() {
                    if let Some(child) = o {
                        go(child, base | ((i as u64) << shift), level_base_shift, out);
                    }
                }
            }
        }
        let _ = level_base_shift; // 保留签名对称
    }
    out.clear();
    go(&table.root, 0, 0, out);
}

/// 帧分配器：universe 为物理帧全集，free 为当前空闲子集（W6 依据）。
#[derive(Clone, Debug)]
pub struct FrameAllocator {
    pub universe: Vec<PhysAddr>,
    pub free: Vec<PhysAddr>,
}

impl FrameAllocator {
    pub fn new(universe: &[PhysAddr]) -> Self {
        // 栈式 pop：倒排使 new 后 alloc() 从低位开始拿
        let mut free = universe.to_vec();
        free.reverse();
        FrameAllocator {
            universe: universe.to_vec(),
            free,
        }
    }

    pub fn alloc(&mut self) -> Option<PhysAddr> {
        self.free.pop()
    }

    /// 归还一帧；断言非重复归还（W6：不重复归属）。
    pub fn free_frame(&mut self, p: PhysAddr) {
        assert!(
            !self.free.contains(&p),
            "W6: 帧 {p:#x} 重复归还"
        );
        self.free.push(p);
    }
}
```

> 注：`PageTableNode::Default` 指定 `Pml4` 根；`collect_leaves` 中 `level_base_shift` 参数为冗余签名，保留以不影响调用方（Task 2 Step 6 测试用 `collect_leaves(&t, &mut leaves)` 两参形式。若 clippy 对未用参数告警，将 `collect_leaves` 第二参按注释保留并在函数内 `let _ =` 抑制，如上已含）。`new_task_table` 即 `PageTable::default()`。上方第一个（手工展开）`translate` 定义**是占位**，会被其后同签名递归 `translate` 定义覆盖——Rust 不允许同作用域重复定义函数，因此**必须删除第一版手工 `translate`**，只保留递归版。最终 pagewalk.rs 只含：常量、类型、索引函数、`new_task_table`、递归 `translate`、`map_vaddr`、`unmap_vaddr`、`collect_leaves`、`FrameAllocator`、`Default`（如含），共约 150 行，全部已列在上。**用下列完整文件替换** Step 5 第二块，直接作为最终内容（删去第一版手工 translate 占位）：

```rust
#![allow(dead_code)]

use crate::state::SegId;
use std::collections::BTreeMap;

pub const PAGE_SIZE: u64 = 0x1000;
pub type PhysAddr = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentRef {
    pub segment: SegId,
    pub offset: u64,
    pub access: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pte {
    pub present: bool,
    pub writable: bool,
    pub user: bool,
    pub nx: bool,
    pub frame_phys: PhysAddr,
    pub mapping: SegmentRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonLeafLevel { Pml4, Pdpt, Pd }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageTableNode {
    NonLeaf { level: NonLeafLevel, slots: Box<[Option<PageTableNode>; 512]> },
    Leaf(Box<[Option<Pte>; 512]>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PageTable { pub root: PageTableNode }

fn pt_index(v: u64) -> usize { ((v >> 12) & 0x1ff) as usize }
fn level_shift(level: NonLeafLevel) -> u32 {
    match level { NonLeafLevel::Pml4 => 39, NonLeafLevel::Pdpt => 30, NonLeafLevel::Pd => 21 }
}
fn child_level_for(parent: NonLeafLevel) -> Option<NonLeafLevel> {
    match parent {
        NonLeafLevel::Pml4 => Some(NonLeafLevel::Pdpt),
        NonLeafLevel::Pdpt => Some(NonLeafLevel::Pd),
        NonLeafLevel::Pd => None,
    }
}

pub fn new_task_table() -> PageTable { PageTable::default() }

pub fn translate<'a>(table: &'a PageTable, vaddr: u64) -> Option<&'a Pte> {
    fn go<'a>(node: &'a PageTableNode, vaddr: u64) -> Option<&'a Pte> {
        match node {
            PageTableNode::Leaf(slots) => slots[pt_index(vaddr)].as_ref(),
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child = slots[idx].as_ref()?;
                go(child, vaddr)
            }
        }
    }
    go(&table.root, vaddr)
}

pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte) {
    fn go(node: &mut PageTableNode, vaddr: u64, pte: &Pte) {
        match node {
            PageTableNode::Leaf(slots) => { slots[pt_index(vaddr)] = Some(*pte); }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                if slots[idx].is_none() {
                    slots[idx] = Some(match child_level_for(*level) {
                        Some(cl) => PageTableNode::NonLeaf {
                            level: cl,
                            slots: Default::default(),
                        },
                        None => PageTableNode::Leaf(Default::default()),
                    });
                }
                go(slots[idx].as_mut().unwrap(), vaddr, pte);
            }
        }
    }
    go(&mut table.root, vaddr, &pte);
}

pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> bool {
    fn go(node: &mut PageTableNode, vaddr: u64) -> bool {
        match node {
            PageTableNode::Leaf(slots) => {
                let _ = slots[pt_index(vaddr)].take();
                slots.iter().all(|o| o.is_none())
            }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child_empty = match slots[idx].as_mut() {
                    Some(child) => go(child, vaddr),
                    None => false,
                };
                if child_empty { slots[idx] = None; }
                slots.iter().all(|o| o.is_none())
            }
        }
    }
    go(&mut table.root, vaddr)
}

pub fn collect_leaves(table: &PageTable, out: &mut BTreeMap<u64, SegmentRef>) {
    fn go(node: &PageTableNode, base: u64, out: &mut BTreeMap<u64, SegmentRef>) {
        match node {
            PageTableNode::Leaf(slots) => {
                for (i, o) in slots.iter().enumerate() {
                    if let Some(pte) = o {
                        out.insert(base + ((i as u64) << 12), pte.mapping);
                    }
                }
            }
            PageTableNode::NonLeaf { level, slots } => {
                let shift = level_shift(*level);
                for (i, o) in slots.iter().enumerate() {
                    if let Some(child) = o {
                        go(child, base | ((i as u64) << shift), out);
                    }
                }
            }
        }
    }
    out.clear();
    go(&table.root, 0, out);
}

#[derive(Clone, Debug)]
pub struct FrameAllocator {
    pub universe: Vec<PhysAddr>,
    pub free: Vec<PhysAddr>,
}

impl FrameAllocator {
    pub fn new(universe: &[PhysAddr]) -> Self {
        let mut free = universe.to_vec();
        free.reverse();
        FrameAllocator { universe: universe.to_vec(), free }
    }
    pub fn alloc(&mut self) -> Option<PhysAddr> { self.free.pop() }
    pub fn free_frame(&mut self, p: PhysAddr) {
        assert!(!self.free.contains(&p), "W6: 帧重复归还");
        self.free.push(p);
    }
}
```

- [ ] **Step 6: 运行测试确认通过（绿灯）**

Run：`cargo test -p cosir pagewalk`
Expected：`test result: ok. 5 passed; 0 failed`（四个 pagewalk_tests + 其它按名字过滤）。

- [ ] **Step 7: 运行既有 17 测试确认未破坏**

Run：`cargo test -p cosir`
Expected：`test result: ok. 22 passed; 0 failed`（17 既有 + 5 新增）。

- [ ] **Step 8: 提交 pagewalk 机制与测试**

```bash
git add cosir/src/pagewalk.rs cosir/src/pagewalk_tests.rs cosir/src/main.rs
git commit -m "feat: add x86_64 four-level page table walk refinement (pagewalk)"
```

> 注意：`main.rs` 本任务只加入 `mod pagewalk;` 与 `#[cfg(test)] mod pagewalk_tests;` 两行，不引入其它改动。

---

### Task 3: 建立 memseg_refine.rs——RefineState + 错误集 + memseg_create + α

**Files:**
- Create: `cosir/src/memseg_refine.rs`
- Modify: `cosir/src/main.rs`（声明 `mod memseg_refine;`）

**Interfaces:**
- Consumes: `pagewalk::{FrameAllocator, PageTable, PhysAddr, SegmentRef, Pte, new_task_table, map_vaddr, translate, unmap_vaddr, collect_leaves, PAGE_SIZE}`；`state::{ACCESS_R, ACCESS_W, ACCESS_X, SegId, TaskId, VirtAddr}`。
- Produces（Task 3-6 互依 + main.rs 线束）：
  - `pub const SEG_MAP_R: u8`、`SEG_MAP_W: u8`、`SEG_MAP_X: u8`、`SEG_DESTROY: u8`（cap 权限位掩码，见 §5；SEG_MAP_* 复用 state::ACCESS_* 值）
  - `pub struct Segment { pub frames: Vec<PhysAddr>, pub allowed_access: u8 }`
  - `pub struct TaskCtx { pub table: PageTable, pub vspace: BTreeMap<VirtAddr, SegmentRef>, pub entry: VirtAddr }`
  - `pub struct RefineState { pub segments: BTreeMap<SegId, Segment>, pub alloc: FrameAllocator, pub tasks: BTreeMap<TaskId, TaskCtx>, pub next_seg: SegId }`
  - `pub enum MemSegError { ZeroSize, InvalidAccess, NoMemory, InvalidSegment, InvalidTask, MissingMapRight, RightsExceeded, OutOfBounds, Overlap, NotMapped, EntryMapping, StillMapped, MissingRight }`
  - `RefineState::new(segment_universe: &[PhysAddr]) -> Self`（建空 allocator + root aspace task 关联由 Task 4 起）
  - `RefineState::spawn_task(&mut self, id: TaskId, entry: VirtAddr) -> Result<(), MemSegError>`（Task 3）
  - `pub fn memseg_create(st: &mut RefineState, seg_cap_rights: u8, page_count: u64, allowed_access: u8) -> Result<SegId, MemSegError>`（Task 3）
  - `pub fn memseg_map(st: &mut RefineState, seg_cap_rights: u8, task_manage_rights: bool, seg_id, task_id, seg_offset, target_base, page_count, access) -> Result<(), MemSegError>`（Task 4）
  - `pub fn memseg_unmap(st: &mut RefineState, task_manage_rights: bool, task_id, target_base, page_count) -> Result<(), MemSegError>`（Task 5）
  - `pub fn memseg_destroy(st: &mut RefineState, seg_destroy_right: bool, seg_id) -> Result<(), MemSegError>`（Task 6）
  - `pub fn memseg_alpha(st: &RefineState) -> bool`（Task 3，W6 后在 Task 6 扩展）
  - `pub fn memseg_alpha_verbose(st: &RefineState, out: &mut Vec<String>) -> bool`（Task 3，诊断输出供测试断言）

- [ ] **Step 1: 在 main.rs 声明 memseg_refine 模块**

在 `cosir/src/main.rs`（`mod pagewalk;` 后）加入：

```rust
mod memseg_refine;
```

- [ ] **Step 2: 写失败测试（TDD 红灯）**

创建 `cosir/src/memseg_refine_tests.rs`，引用尚不存在的类型/函数：

```rust
use crate::memseg_refine::{
    MemSegError, RefineState, memseg_alpha_verbose, memseg_create, memseg_destroy, memseg_map,
    memseg_unmap, spawn_task, SEG_DESTROY, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X,
};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X};

// 8 页物理帧
fn universe() -> Vec<u64> {
    (0u64..8).map(|i| 0x1000 + i * 0x1000).collect()
}

fn seg_map_rights() -> u8 {
    SEG_MAP_R | SEG_MAP_W | SEG_MAP_X
}

#[test]
fn create_allocates_frames_and_segment() {
    let mut st = RefineState::new(&universe());
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .expect("create ok");
    let s = st.segments.get(&seg).expect("segment present");
    assert_eq!(s.frames.len(), 2);
    assert_eq!(s.allowed_access, ACCESS_R | ACCESS_W);
    assert_eq!(st.alloc.free.len(), 6, "8-2=6 帧空闲");
}

#[test]
fn create_exhausts_memory_returns_no_memory() {
    let mut st = RefineState::new(&universe());
    let err = memseg_create(&mut st, seg_map_rights(), 9, ACCESS_R).unwrap_err();
    assert_eq!(err, MemSegError::NoMemory);
    assert_eq!(st.alloc.free.len(), 8, "缺帧不改空闲池");
}

#[test]
fn spawn_task_registers_aspace_with_entry() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).expect("spawn ok");
    assert!(st.tasks.contains_key(&7));
    assert_eq!(st.tasks[&7].entry, 0x4000_0000);
    assert!(st.tasks[&7].vspace.is_empty());
}

#[test]
fn alpha_validates_empty_state() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "空树空视图应满足 W6/W7");
}

#[test]
fn alpha_detects_vspace_view_divergence() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    // 人为在 vspace 视图加一个树中不存在的映射 → α 应判 False
    let task = st.tasks.get_mut(&7).unwrap();
    task.vspace.insert(0x6000_0000, crate::pagewalk::SegmentRef {
        segment: 1,
        offset: 0,
        access: ACCESS_R,
    });
    let mut diag = Vec::new();
    assert!(!memseg_alpha_verbose(&st, &mut diag), "视图与树不一致应被 α 捕获");
}
```

- [ ] **Step 3: 在 main.rs 挂上测试模块**

在 `cosir/src/main.rs`（`mod pagewalk_tests;` 后）加入：

```rust
#[cfg(test)]
mod memseg_refine_tests;
```

- [ ] **Step 4: 运行测试确认编译失败（红灯）**

Run：`cargo test -p cosir memseg_refine`
Expected：编译报 `cannot find ... in module` / E0433 等，测试失败。

- [ ] **Step 5: 写最小实现 memseg_refine.rs**

在 `cosir/src/memseg_refine.rs` 写入：

```rust
#![allow(dead_code)]

use crate::pagewalk::{
    PAGE_SIZE, FrameAllocator, PageTable, PhysAddr, Pte, SegmentRef, collect_leaves, map_vaddr,
    new_task_table, unmap_vaddr,
};
use crate::state::{SegId, TaskId, VirtAddr};
use std::collections::BTreeMap;

// Capability 权限位掩码（细化层用于前置授权检查；非 POSIR 一等原语）
// SEG_MAP_* 与 state::ACCESS_* 数值一致(R=0b001/W=0b010/X=0b100)。
pub const SEG_MAP_R: u8 = crate::state::ACCESS_R;
pub const SEG_MAP_W: u8 = crate::state::ACCESS_W;
pub const SEG_MAP_X: u8 = crate::state::ACCESS_X;
pub const SEG_DESTROY: u8 = 0x08;

/// 细化错误集：沿用规范错误名，不新增对 POSIR 可见的错误原语。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemSegError {
    ZeroSize,
    InvalidAccess,
    NoMemory,
    InvalidSegment,
    InvalidTask,
    MissingMapRight,
    RightsExceeded,
    OutOfBounds,
    Overlap,
    NotMapped,
    EntryMapping,
    StillMapped,
    MissingRight,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub frames: Vec<PhysAddr>,
    pub allowed_access: u8,
}

#[derive(Clone, Debug)]
pub struct TaskCtx {
    pub table: PageTable,
    pub vspace: BTreeMap<VirtAddr, SegmentRef>,
    pub entry: VirtAddr,
}

/// 细化域状态：活动 segment、物理帧池、每 Task 的 CR3 树 + vspace 双视图 + entry。
#[derive(Clone, Debug)]
pub struct RefineState {
    pub segments: BTreeMap<SegId, Segment>,
    pub alloc: FrameAllocator,
    pub tasks: BTreeMap<TaskId, TaskCtx>,
    pub next_seg: SegId,
}

impl RefineState {
    pub fn new(universe: &[PhysAddr]) -> Self {
        RefineState {
            segments: BTreeMap::new(),
            alloc: FrameAllocator::new(universe),
            tasks: BTreeMap::new(),
            next_seg: 1,
        }
    }

    /// 为 Task 建立空的 CR3 树 + 空 vspace 双视图，记录入口（W8 依据）。
    pub fn spawn_task(&mut self, id: TaskId, entry: VirtAddr) -> Result<(), MemSegError> {
        if self.tasks.contains_key(&id) {
            return Err(MemSegError::InvalidTask);
        }
        self.tasks.insert(
            id,
            TaskCtx {
                table: new_task_table(),
                vspace: BTreeMap::new(),
                entry,
            },
        );
        Ok(())
    }
}

/// memseg_create(page_count, allowed_access) 的细化迁移：
/// 帧分配(W6) + 建 Segment + 铸造根 SegCap（本层返回 seg id）。
pub fn memseg_create(
    st: &mut RefineState,
    seg_cap_rights: u8,
    page_count: u64,
    allowed_access: u8,
) -> Result<SegId, MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if allowed_access == 0 || allowed_access & !0x07 != 0 {
        return Err(MemSegError::InvalidAccess);
    }
    // 授权：需含 Seg 映射权限（R/W/X 任一）才可创建用户可见 segment
    if seg_cap_rights & 0x07 == 0 {
        return Err(MemSegError::MissingMapRight);
    }
    // 先检查容量，避免部分分配（原子）
    if st.alloc.free.len() < page_count as usize {
        return Err(MemSegError::NoMemory);
    }
    let mut frames = Vec::with_capacity(page_count as usize);
    for _ in 0..page_count {
        frames.push(st.alloc.alloc().expect("容量已保证"));
    }
    let id = st.next_seg;
    st.next_seg += 1;
    st.segments.insert(
        id,
        Segment {
            frames,
            allowed_access,
        },
    );
    Ok(id)
}

/// α 投影：返回该细化态是否满足 W6 + W7。细节写入 diag 供测试断言。
pub fn memseg_alpha_verbose(st: &RefineState, diag: &mut Vec<String>) -> bool {
    let mut ok = true;

    // W7 双视图一致 + 指向活动 Segment + access≤allowed
    for (tid, tc) in &st.tasks {
        let mut leaves: BTreeMap<VirtAddr, SegmentRef> = BTreeMap::new();
        collect_leaves(&tc.table, &mut leaves);
        if &leaves != &tc.vspace {
            ok = false;
            diag.push(format!("W7: task {tid} 树↔视图不一致"));
        }
        for (va, sref) in &leaves {
            match st.segments.get(&sref.segment) {
                None => {
                    ok = false;
                    diag.push(format!("W7: task {tid} {va:#x} 指向不活动 segment {}", sref.segment));
                }
                Some(seg) => {
                    if seg.allowed_access & sref.access != sref.access {
                        ok = false;
                        diag.push(format!("W7: {va:#x} access 超出段权限"));
                    }
                }
            }
        }
    }

    // W6 物理页唯一归属
    // 收集所有活动 segment 帧
    let mut used: BTreeMap<PhysAddr, SegId> = BTreeMap::new();
    for (sid, seg) in &st.segments {
        for f in &seg.frames {
            if let Some(prev) = used.insert(*f, *sid) {
                ok = false;
                diag.push(format!("W6: 帧 {f:#x} 被 segment {prev} 与 {sid} 共同引用"));
            }
        }
    }
    // 段帧与空闲帧不交
    for f in &st.alloc.free {
        if used.contains_key(f) {
            ok = false;
            diag.push(format!("W6: 空闲帧 {f:#x} 同时被 segment {} 引用", used[f]));
        }
    }
    // 并集 = universe
    let mut combined: Vec<PhysAddr> = used.keys().copied().collect();
    combined.extend(st.alloc.free.iter().copied());
    combined.sort_unstable();
    let mut univ = st.alloc.universe.clone();
    univ.sort_unstable();
    if combined != univ {
        ok = false;
        diag.push("W6: 段帧∪空闲帧 ≠ frame_universe".to_string());
    }

    ok
}

/// α 便捷封装（Bool 返回值）。
pub fn memseg_alpha(st: &RefineState) -> bool {
    let mut diag = Vec::new();
    memseg_alpha_verbose(st, &mut diag)
}
```

> 依赖顺序说明：`memseg_alpha_verbose` 中引用了 `sref.access` 与 `allowed_access` 均为 u8，用 `&` 位运算比较，语义上 `access ⊆ allowed`；Task 3 Step 5 以「仅 create + spawn + α」先落地，`memseg_map/unmap/destroy` 与 `memseg_refine_tests` 里对它们的调用**留到 Task 4/5/6**。上方测试文件 Step 2 已引用 `memseg_map/unmap/destroy`——为不拖沓，Task 3 先只写 create+spawn+alpha 的实现与测试，`memseg_map/unmap/destroy` 的函数签名**先以 `todo!()` 承接收尾**加入文件末尾占位，保证编译通过，Task 4/5/6 依次实现替换。占位如下（添加到同文件末尾）：

```rust
pub fn memseg_map(
    st: &mut RefineState,
    seg_cap_rights: u8,
    task_manage_rights: bool,
    seg_id: SegId,
    task_id: TaskId,
    seg_offset: u64,
    target_base: VirtAddr,
    page_count: u64,
    access: u8,
) -> Result<(), MemSegError> {
    todo!("Task 4");
}

pub fn memseg_unmap(
    st: &mut RefineState,
    task_manage_rights: bool,
    task_id: TaskId,
    target_base: VirtAddr,
    page_count: u64,
) -> Result<(), MemSegError> {
    todo!("Task 5");
}

pub fn memseg_destroy(
    st: &mut RefineState,
    seg_destroy_right: bool,
    seg_id: SegId,
) -> Result<(), MemSegError> {
    todo!("Task 6");
}
```

- [ ] **Step 6: 运行测试确认通过（绿灯）**

Run：`cargo test -p cosir memseg_refine`
注意：Task 3 的 `memseg_refine_tests` 中 Step 2 已含 `alpha_detects_vspace_view_divergence` 依赖 `SegmentRef` 构造与 `vspace.insert`，对应实现已就位；但未含对 `memseg_map/unmap/destroy` 的调用，因此占位 `todo!()` 不会触发 panic。`test result: ok. 5 passed; 0 failed`。

- [ ] **Step 7: 运行全量确认未破坏**

Run：`cargo test -p cosir`
Expected：`test result: ok. 27 passed; 0 failed`（17 既有 + 5 pagewalk + 5 memseg_refine）。

- [ ] **Step 8: 提交 create + spawn + α**

```bash
git add cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs cosir/src/main.rs
git commit -m "feat: add memseg refinement core (create/spawn/alpha) with W6 W7 validation"
```

> 说明：此时 `memseg_map/unmap/destroy` 为 `todo!()` 占位，属 Task 3 已知受控状态（后续任务逐一替换）。**为避免 `qemu xtask test` 触及 host 探针 panic，确保这些 `todo!()` 占位不被任何入口调用**——它们在 main.rs 线束触发前会在 Task 4/5/6 被替换。

---

### Task 4: memseg_map 细化迁移（写四级树叶 + 双视图同步，W7）

**Files:**
- Modify: `cosir/src/memseg_refine.rs`（实现占位 `memseg_map`）
- Modify: `cosir/src/memseg_refine_tests.rs`（加 map 测试）

**Interfaces:**
- Consumes: Task 3 的 `RefineState`/`MemSegError`/`Segment`/`TaskCtx`、`pagewalk` 现成函数。
- Produces: 可用 `memseg_map`（写树 + 双视图 + 授权/权限/越界/重叠检查）。

- [ ] **Step 1: 写失败测试（TDD 红灯）**

在 `cosir/src/memseg_refine_tests.rs` 追加：

```rust
#[test]
fn map_inserts_leaf_and_vspace() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(
        &mut st,
        seg_map_rights(),
        true,
        seg,
        7,
        0,
        0x5000_0000,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("map ok");
    let task = &st.tasks[&7];
    assert!(task.vspace.contains_key(&0x5000_0000));
    assert_eq!(
        task.vspace[&0x5000_0000].segment,
        seg
    );
    let pte = crate::pagewalk::translate(&task.table, 0x5000_0000).expect("leaf present");
    assert!(pte.mapping.access & ACCESS_W != 0);
}

#[test]
fn map_overlap_rejected_state_unchanged() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(
        &mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W,
    )
    .unwrap();
    // 精确重叠：再 map 覆盖已映射块
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_W,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::Overlap);
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "失败后状态仍 W6/W7 有效");
}

#[test]
fn map_rights_exceeded_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    // 段只允许 R|W，却请求含 X → RightsExceeded
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R | ACCESS_W)
        .unwrap();
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 1,
        ACCESS_R | ACCESS_W | ACCESS_X,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::RightsExceeded);
}

#[test]
fn map_missing_seg_map_right_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R | ACCESS_W)
        .unwrap();
    // 缺 SEG_MAP 授权 → MissingMapRight
    let err = memseg_map(
        &mut st, 0, true, seg, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_W,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::MissingMapRight);
}

#[test]
fn map_out_of_bounds_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 1, 0x5000_0000, 1, ACCESS_R,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::OutOfBounds);
}
```

- [ ] **Step 2: 运行测试确认编译失败/运行失败（红灯）**

Run：`cargo test -p cosir memseg_refine::tests::map`
Expected：现有实现是 `todo!()` 占位，调用 `memseg_map` 会 `panic!`（`todo!()`）或测试失败。

- [ ] **Step 3: 实现 memseg_map**

在 `cosir/src/memseg_refine.rs`，把 `memseg_map` 的 `todo!()` 占位替换为：

```rust
pub fn memseg_map(
    st: &mut RefineState,
    seg_cap_rights: u8,
    task_manage_rights: bool,
    seg_id: SegId,
    task_id: TaskId,
    seg_offset: u64,
    target_base: VirtAddr,
    page_count: u64,
    access: u8,
) -> Result<(), MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if access == 0 || access & !0x07 != 0 {
        return Err(MemSegError::InvalidAccess);
    }
    let seg = st.segments.get(&seg_id).ok_or(MemSegError::InvalidSegment)?;
    let seg = seg.clone(); // 借出来后释放，便于下面可变迁用
    if !st.tasks.contains_key(&task_id) {
        return Err(MemSegError::InvalidTask);
    }
    // 授权：segment 映射权限须含各请求位；Task vspace 管理须授权
    let map_bits = match access {
        a if a & crate::state::ACCESS_X != 0 => SEG_MAP_X,
        a if a & crate::state::ACCESS_W != 0 => SEG_MAP_W,
        _ => SEG_MAP_R,
    };
    if seg_cap_rights & map_bits == 0 {
        return Err(MemSegError::MissingMapRight);
    }
    if !task_manage_rights {
        return Err(MemSegError::MissingRight);
    }
    // access ⊆ allowed_access
    if seg.allowed_access & access != access {
        return Err(MemSegError::RightsExceeded);
    }
    // 区间在段内
    if seg_offset + page_count > seg.frames.len() as u64 {
        return Err(MemSegError::OutOfBounds);
    }
    // 目标区间全空（Overlap）——检查 vspace 视图
    for i in 0..page_count {
        let va = target_base + i * PAGE_SIZE;
        if st.tasks[&task_id].vspace.contains_key(&va) {
            return Err(MemSegError::Overlap);
        }
    }
    // 逐页：写叶 PTE（物理视图 + POSIR 引用视图）+ 双视图同步
    for i in 0..page_count {
        let va = target_base + i * PAGE_SIZE;
        let frame = seg.frames[(seg_offset + i) as usize];
        let writable = access & crate::state::ACCESS_W != 0;
        let nx = access & crate::state::ACCESS_X == 0;
        let pte = Pte {
            present: true,
            writable,
            user: true,
            nx,
            frame_phys: frame,
            mapping: SegmentRef {
                segment: seg_id,
                offset: seg_offset + i,
                access,
            },
        };
        let task = st.tasks.get_mut(&task_id).unwrap();
        map_vaddr(&mut task.table, va, pte);
        task.vspace.insert(va, pte.mapping);
    }
    Ok(())
}
```

> 注：`map_bits` 匹配把 `access` 拆为「含 X 优先取 SEG_MAP_X、含 W 取 SEG_MAP_W、否则取 SEG_MAP_R」用于判定是否持有所需映射权限——这是细化层对"持有所需读写执行授权"的简化建模（`seg_cap_rights` 表达调用方对段的 R/W/X 映射授权掩码）。若测试用 `seg_map_rights()`（R|W|X 全开）调用，`map_bits` 匹配 `access & ACCESS_X != 0` 命中 `SEG_MAP_X=0x04`，`seg_cap_rights & 0x04` 为真，放行。**（本 Task 结束前请复核：`state::ACCESS_R/W/X` 与 `SEG_MAP_R/W/X` 数值不同，前者 0b001/0b010/0b100，后者 0x01/0x02/0x04，语义对应 R↔SEG_MAP_R 等。若发现混淆，改用一个共享位域常量——见 Step 5 迁移。）**

- [ ] **Step 4: 运行测试确认通过（绿灯）**

Run：`cargo test -p cosir memseg_refine`
Expected：`test result: ok. 10 passed; 0 failed`（Task 3 的 5 + map 的 5）。

- [ ] **Step 5: 复核常量位域一致性**

确认 `SEG_MAP_R=0x01` 与 `ACCESS_R=0b001`、`SEG_MAP_W=0x02` 与 `ACCESS_W=0b010`、`SEG_MAP_X=0x04` 与 `ACCESS_X=0b100` 数值一致（前者已是 1/2/4）。若已一致，Step 3 的 `map_bits` 匹配可简化为按位检查 `seg_cap_rights & (access & 0x07) != 0`。为清晰，将 Step 3 的 `map_bits` 分支替换为：

```rust
    // 授权：调用方持有所请求访问对应的全部映射权限（SEG_MAP_R/W/X 复用 state ACCESS 位码，值相同）
    if seg_cap_rights & access != access {
        return Err(MemSegError::MissingMapRight);
    }
```

并删除 `map_bits` 局部变量。运行全量测试确认不变。

- [ ] **Step 6: 运行全量确认未破坏**

Run：`cargo test -p cosir`
Expected：`test result: ok. 32 passed; 0 failed`（27 + 5）。

- [ ] **Step 7: 提交 memseg_map**

```bash
git add cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs
git commit -m "feat: implement memseg_map page-walk insert and vspace sync"
```

---

### Task 5: memseg_unmap 细化迁移（清叶 + 回收 + W8 entry 保护）

**Files:**
- Modify: `cosir/src/memseg_refine.rs`（实现占位 `memseg_unmap`）
- Modify: `cosir/src/memseg_refine_tests.rs`（加 unmap 测试）

**Interfaces:**
- Consumes: Task 3/4 的 `RefineState`/`TaskCtx`。
- Produces: 可用 `memseg_unmap`（清树 + 双视图 + NotMapped/EntryMapping/ZeroSize/MissingRight 检查）。

- [ ] **Step 1: 写失败测试（TDD 红灯）**

在 `cosir/src/memseg_refine_tests.rs` 追加：

```rust
#[test]
fn unmap_clears_leaf_and_vspace() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut st, true, 7, 0x5000_0000, 2).expect("unmap ok");
    let task = &st.tasks[&7];
    assert!(!task.vspace.contains_key(&0x5000_0000));
    assert!(crate::pagewalk::translate(&task.table, 0x5000_0000).is_none());
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "unmap 后仍 W6/W7 有效");
}

#[test]
fn unmap_entry_page_rejected() {
    let mut st = RefineState::new(&universe());
    // entry 设在 0x5000_0000 所在页
    spawn_task(&mut st, 7, 0x5000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W | ACCESS_X)
        .unwrap();
    // 映射包含 entry 页
    memseg_map(&mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_X).unwrap();
    let err = memseg_unmap(&mut st, true, 7, 0x5000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::EntryMapping);
    // 树与视图不应被清除
    let task = &st.tasks[&7];
    assert!(task.vspace.contains_key(&0x5000_0000));
    assert!(crate::pagewalk::translate(&task.table, 0x5000_0000).is_some());
}

#[test]
fn unmap_not_mapped_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let err = memseg_unmap(&mut st, true, 7, 0x6000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::NotMapped);
}

#[test]
fn unmap_missing_task_vspace_right_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 1, ACCESS_R).unwrap();
    let err = memseg_unmap(&mut st, false, 7, 0x5000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::MissingRight);
}
```

- [ ] **Step 2: 运行测试确认失败（红灯）**

Run：`cargo test -p cosir memseg_refine::tests::unmap`
Expected：现有 `todo!()` → `panic!`，测试失败。

- [ ] **Step 3: 实现 memseg_unmap**

替换 `todo!()` 占位：

```rust
pub fn memseg_unmap(
    st: &mut RefineState,
    task_manage_rights: bool,
    task_id: TaskId,
    target_base: VirtAddr,
    page_count: u64,
) -> Result<(), MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if !task_manage_rights {
        return Err(MemSegError::MissingRight);
    }
    if !st.tasks.contains_key(&task_id) {
        return Err(MemSegError::InvalidTask);
    }
    // entry 页保护（W8）：目标区间不包含 entry 所在页
    let entry_page = st.tasks[&task_id].entry & !(PAGE_SIZE - 1);
    for i in 0..page_count {
        let va = target_base + i * PAGE_SIZE;
        if va == entry_page {
            return Err(MemSegError::EntryMapping);
        }
    }
    // 区间全映射
    for i in 0..page_count {
        let va = target_base + i * PAGE_SIZE;
        if !st.tasks[&task_id].vspace.contains_key(&va) {
            return Err(MemSegError::NotMapped);
        }
    }
    // 清叶 + 双视图
    for i in 0..page_count {
        let va = target_base + i * PAGE_SIZE;
        let task = st.tasks.get_mut(&task_id).unwrap();
        let _ = unmap_vaddr(&mut task.table, va);
        task.vspace.remove(&va);
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过（绿灯）**

Run：`cargo test -p cosir memseg_refine`
Expected：`test result: ok. 14 passed; 0 failed`（10 + 4）。

- [ ] **Step 5: 运行全量确认未破坏**

Run：`cargo test -p cosir`
Expected：`test result: ok. 36 passed; 0 failed`（32 + 4）。

- [ ] **Step 6: 提交 memseg_unmap**

```bash
git add cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs
git commit -m "feat: implement memseg_unmap with W8 entry protection and page reclamation"
```

---

### Task 6: memseg_destroy 细化迁移 + 全流程三件套测试

**Files:**
- Modify: `cosir/src/memseg_refine.rs`（实现占位 `memseg_destroy`）
- Modify: `cosir/src/memseg_refine_tests.rs`（加 destroy 与全流程测试）

**Interfaces:**
- Consumes: Task 3-5 全部函数。
- Produces: 完整 `memseg_create/map/unmap/destroy` 四操作 + 动态/幂等/回收测试，α 全绿。

- [ ] **Step 1: 写失败测试（TDD 红灯）**

在 `cosir/src/memseg_refine_tests.rs` 追加：

```rust
#[test]
fn destroy_returns_frames_and_removes_segment() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R).unwrap();
    let before = st.alloc.free.len(); // = 6
    memseg_destroy(&mut st, true, seg).expect("destroy ok");
    assert!(!st.segments.contains_key(&seg));
    assert_eq!(st.alloc.free.len(), before + 2, "两帧归还");
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "destroy 后仍 W6 有效");
}

#[test]
fn destroy_still_mapped_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    // 映射进 task 7 → destroy 应拒绝
    memseg_map(&mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 1, ACCESS_R).unwrap();
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::StillMapped);
    assert!(st.segments.contains_key(&seg), "段不删除");
}

#[test]
fn destroy_missing_right_rejected() {
    let mut st = RefineState::new(&universe());
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    let err = memseg_destroy(&mut st, false, seg).unwrap_err();
    assert_eq!(err, MemSegError::MissingRight);
    assert!(st.segments.contains_key(&seg));
}

#[test]
fn destroy_invalid_segment_rejected() {
    let mut st = RefineState::new(&universe());
    let err = memseg_destroy(&mut st, true, 999).unwrap_err();
    assert_eq!(err, MemSegError::InvalidSegment);
}

#[test]
fn destroy_idempotent_double_frees_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    memseg_destroy(&mut st, true, seg).unwrap();
    // 再次 destroy → InvalidSegment（段已不在）
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::InvalidSegment);
}

#[test]
fn full_memseg_lifecycle_alpha_preserved() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut st, true, 7, 0x5000_0000, 1).unwrap(); // 释放第一页(0x5000_0000)
    // 剩余一页(0x5000_1000)映射仍引用段 → 不可 destroy
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::StillMapped);
    memseg_unmap(&mut st, true, 7, 0x5000_1000, 1).unwrap(); // 释放最后一页(0x5000_1000)
    memseg_destroy(&mut st, true, seg).expect("destroy ok after unmaps");
    assert!(!st.segments.contains_key(&seg));
    assert_eq!(st.alloc.free.len(), 8, "全部帧归还");
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "完整生命周期后 α 恒真");
}
```

- [ ] **Step 2: 运行测试确认失败（红灯）**

Run：`cargo test -p cosir memseg_refine::tests::destroy`
Expected：现有 `todo!()` → `panic!`，测试失败。

- [ ] **Step 3: 实现 memseg_destroy**

替换 `todo!()` 占位：

```rust
pub fn memseg_destroy(
    st: &mut RefineState,
    seg_destroy_right: bool,
    seg_id: SegId,
) -> Result<(), MemSegError> {
    if !seg_destroy_right {
        return Err(MemSegError::MissingRight);
    }
    let seg = st.segments.get(&seg_id).ok_or(MemSegError::InvalidSegment)?;
    let frames = seg.frames.clone();
    // StillMapped：任何活动 Task 的 vspace 都不能引用 target
    for tc in st.tasks.values() {
        for sref in tc.vspace.values() {
            if sref.segment == seg_id {
                return Err(MemSegError::StillMapped);
            }
        }
    }
    st.segments.remove(&seg_id);
    for f in frames {
        st.alloc.free_frame(f);
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过（绿灯）**

Run：`cargo test -p cosir memseg_refine`
Expected：`test result: ok. 20 passed; 0 failed`（14 + 6）。

- [ ] **Step 5: 运行全量确认未破坏**

Run：`cargo test -p cosir`
Expected：`test result: ok. 42 passed; 0 failed`（36 + 6）。

- [ ] **Step 6: 提交 memseg_destroy + 生命周期测试**

```bash
git add cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs
git commit -m "feat: implement memseg_destroy page return and full lifecycle tests"
```

---

### Task 7: main.rs 验收线束 + COSIR §20 细化小节 + 实施文档 SPEC-014

**Files:**
- Modify: `cosir/src/main.rs`（线束追加 PASS）
- Modify: `COSIR 原语规范.md`（追加 MemorySegment 细化小节到 §20）
- Modify: `Prima OS 实现阶段.md`（登记 SPEC-014、更新 BLK-002 描述与当前快照）

**Interfaces (historical):**
- Consumes: Task 3-6 全部导出。
- Produces: 历史 PASS 摘要和当时的规范/实施文档记录；现行证据结论以纠偏设计为准。

- [ ] **Step 1: main.rs 线束追加 PASS 摘要**

在 `cosir/src/main.rs` 的 `main()` 中（BLK-002 bootstrap 冒烟 `println!("PASS cosir bootstrap alpha projection");` 之后）追加：

```rust
    // MemorySegment 页表↔vspace 细化冒烟：create→map→unmap→destroy + α 校验
    {
        use memseg_refine::{
            RefineState, SEG_DESTROY, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha,
            memseg_create, memseg_destroy, memseg_map, memseg_unmap, spawn_task,
        };
        use state::{ACCESS_R, ACCESS_W, ACCESS_X};
        let universe: Vec<u64> = (0u64..4).map(|i| 0x1000 + i * 0x1000).collect();
        let mut st = RefineState::new(&universe);
        spawn_task(&mut st, 1, 0x4000_0000).expect("spawn init");
        let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
        let seg = memseg_create(&mut st, rights | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
            .expect("create");
        // memseg_map(seg_cap_rights, task_manage_rights, seg_id, task_id, seg_offset,
        //            target_base, page_count, access)
        memseg_map(&mut st, rights, true, seg, 1, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_W)
            .expect("map");
        assert!(memseg_alpha(&st), "map 后 α 成立");
        memseg_unmap(&mut st, true, 1, 0x5000_0000, 1).expect("unmap");
        memseg_destroy(&mut st, true, seg).expect("destroy");
        assert!(memseg_alpha(&st), "destroy 后 α 成立");
        // 历史计划中的旧 PASS 文本已废止；现行线束使用校准后的模型证据标签。
        println!("PASS model-test memseg page-walk dual view");
        println!("PASS model-test memseg W6/W7 checks");
        // W8 仅由 memseg_unmap 的 entry 页 guard 覆盖，不输出完整 W8 α PASS。
    }
```

> 注：上面 `memseg_map(st, ..., seg, 1, task_id, 0, 0x5000_0000, 1, ...)` 传参与函数签名 `(seg_cap_rights, task_manage_rights, seg_id, task_id, seg_offset, target_base, page_count, access)` 对应：`rights`(seg_cap_rights)、`true`(task_manage_rights)、`seg`(seg_id)、`1`(task_id)、`0`(seg_offset)、`0x5000_0000`(target_base)、`1`(page_count)、`ACCESS_R|ACCESS_W`(access)。**请按签名核对每个实参位置**，避免错位（尤其 `seg`/`task_id` 两个 u32 与 `0`/`0x5000_0000` 两个 u64 的实参与形参顺序）。

- [ ] **Step 2: 运行线束与 lint**

Run：`cargo test -p cosir`
Historical expected output（测试数已过时；当前以实际输出和纠偏设计为准）：`test result: ok. 42 passed; 0 failed`。

Run：`cargo clippy --workspace --all-targets --no-deps -- -D warnings`
Expected：无警告、退出码 0。若 `#![allow(dead_code)]` 或未用 `PAGE_SIZE` 告警，按 clippy 提示就地处理（保留允许属性即可）。

- [ ] **Step 3: 新增 COSIR 规范 §20.5「MemorySegment 页表↔vspace 细化」小节**

在 `COSIR 原语规范.md` 现有 §20 章节最末（§20.4 范围边界之后）追加小节。用 Read 定位 §20 末尾行后再 Edit。内容模板：

```markdown
### 20.5 MemorySegment 页表 ↔ vspace 细化

以下代码块是历史计划中的旧文本；现行 COSIR §20.5 与纠偏设计取代其证据表述。

本小节曾把 MemorySegment 的 CR3/四级页表↔vspace 对应作为细化层展开。现行解释是 host 四级索引树↔vspace 的 `REFINEMENT-DESIGN`，不新增对 POSIR 可见的一等原语，也不是真实 x86_64 CR3 页表实现。

**host 四级索引树（仅 4KB，四级固定走到底）。** 每 CPU Task 在 host 模型中持有一棵每级 512 槽的索引树，最终指向 4KB 叶；层级形状与权限字段只是 x86_64-inspired candidate encoding。

| POSIR Access | x86_64-inspired candidate encoding |
|---|---|
| R | `present`（候选只读由 `writable=false` 表达） |
| W | `writable` |
| X | `!nx` |
| 用户态可达 | `user` |

**host 叶双视图。** 模型叶 `Pte{frame_phys, mapping}` 检查物理地址视图与 POSIR 引用视图的一致性；它不是实际 PTE 或位级布局。`mapping = SegmentRef{segment, offset, access}` 是 §5.2 `PageMapping` 的模型视图。

**四操作候选细化对应。**

| POSIR 抽象（§7） | host refinement design |
|---|---|
| `memseg_create` | 从 `free_frames` 取 `page_count` 帧（W6），建 Segment；不建树 |
| `memseg_map` | 前置校验后遍历 host 树到底插入模型叶并同步 vspace 视图（W7） |
| `memseg_unmap` | 前置校验后清模型叶、回收空结点、同步 vspace；含 entry 页 guard |
| `memseg_destroy` | 前置校验（无活动 vspace 引用，StillMapped）后删 Segment，归还帧（W6） |

**有限不变量检查。** W6 和 W7 由 host 模型测试检查。W8 仅覆盖 `memseg_unmap` 拒绝 entry 页的 guard，不能表述为完整 W8 α。未覆盖非叶 U/S、R/W、NX 权限合成，页表页物理身份及回收，TLB/多核可见性，真实 CR3/PTE 编码和原子更新窗口。

**错误。** 沿用规范错误集（`MissingMapRight`/`RightsExceeded`/`OutOfBounds`/`Overlap`/`NotMapped`/`EntryMapping`/`StillMapped`/`ZeroSize`/`NoMemory`/`InvalidSegment`/`InvalidTask`/`MissingRight`）；页表遍历内的“路径深表缺失”等 host 机制状态折叠为对应 POSIR 错误，不外泄新错误原语。

**未覆盖（留后续切片）。** 大页（2MB/1GB）多粒度、task_create 入口细化（边界清单 #16）、异常/trap 递送（S1-D4）、叶 `mapping` 额外字段在真机 PTE 的落地。
```

- [ ] **Step 4: 更新实施文档**

在 `Prima OS 实现阶段.md`：

1. **证据索引**新增一行（追加到 SPEC-013 之后）：
   `| SPEC-014 | 细化规范与可执行模型 | COSIR 原语规范 §20.5、cosir/src/pagewalk.rs、cosir/src/memseg_refine.rs | 2026-08-12 | 定义 MemorySegment 四级页表↔vspace 细化（页表位编码、四操作树映射、W6/W7/W8 α 验证）；cargo test -p cosir 42 tests 通过；cargo xtask test 六条 PASS | S0-D4 |`
2. **当前快照**表 "BLK-002 细化（本切片）" 行更新为记录 MemorySegment 细化；"最近细化证据" 行改为引用 SPEC-014。
3. **BLK-002 阻塞项行**：把问题表述更新为「草案已放宽——已覆盖 MemorySegment 页表↔vspace 细化」；解除条件维持「定义 MachineState、抽象函数和至少 Bootstrap（及 MemorySegment）细化对应」；状态改为「草案已成」。

> 注意：`Prima OS 实现阶段.md` 当前有 SPEC-012 的 9→12 同步等**历史遗留未提交改动**。本任务**只增加 SPEC-014 相关行与 BLK-002 描述更新**，不整文件覆盖、不卷入历史遗留行的其它 diff（它们是否随本任务提交由你按仓库现状决定；若 `git status` 显示该文件已有未暂存改动，新增行会并进同一文件 diff，属可接受的叠加）。

- [ ] **Step 5: 全 workspace 静态与单元校验**

Run：`cargo clippy --workspace --all-targets --no-deps -- -D warnings`（应零警告）
Run：`cargo test -p cosir`（42 passed）

- [ ] **Step 6: 重新运行启动验收，确保未破坏闭环**

Run：`cargo xtask test`
Expected：退出码 0，输出尾部六条 PASS：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

- [ ] **Step 7: 提交规范与实施文档**

```bash
git add "COSIR 原语规范.md" "Prima OS 实现阶段.md"
git commit -m "docs: record memseg page-walk refinement (SPEC-014) in COSIR spec and stage tracker"
```

---

### Task 8: 最终验证与计划文件归档

**Files:**
- Create: `docs/superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md`（本计划，最后提交）

**Interfaces:**
- Produces: 关闭本切片的最终确认。

- [ ] **Step 1: 全量静态 + 单元 + 集成终极校验**

Run：`cargo clippy --workspace --all-targets --no-deps -- -D warnings`（0 警告）
Run：`cargo test -p cosir`（42 passed）
Run：`cargo xtask test`（六条 PASS，退出码 0）

- [ ] **Step 2: 校验差异与文档边界**

Run：`git status --short` 与 `git log --oneline -12`
核对：
- 既有 17 测试（exc_resume + bootstrap）未改语义；
- `state.rs`/`ops.rs`/`abstract_ops.rs`/`machine.rs` 无本计划产生的改动；
- `COSIR 原语规范.md`/`Prima OS 实现阶段.md` 的历史遗留未提交改动是否被提交或保留，与你在 Task 7 Step 4 的决定一致；
- BLK-002 状态/SPEC-014 记录与实现一致。

- [ ] **Step 3: 提交计划文件**

```bash
git add docs/superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md
git commit -m "docs: add memseg page-walk refinement implementation plan"
```

- [ ] **Step 4: 最终确认**

输出：
- `/cosir` 测试总数（预期 42）与既有 17 未回归；
- 新增 PASS 摘要数量；
- SPEC-014 已登记、BLK-002 描述已更新；
- 本切片未关闭 S0-X1/X2/X3 与 BLK-001（如实记录，不虚报）。
