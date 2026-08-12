# MemorySegment Refinement Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 MemorySegment/pagewalk host 模型的地址、整数、物理叶和帧集合问题，并把 SPEC-014 证据表述校准到实际证明等级。

**Architecture:** 保留现有 `pagewalk.rs` 四级索引树和 `memseg_refine.rs` 双视图结构；在 pagewalk 边界统一验证地址，在 memseg 迁移提交前完成全部 checked 验证，在 α 中直接遍历完整 PTE。阶段追踪器是 ISSUE 状态的唯一事实源。

**Tech Stack:** Rust workspace、`BTreeMap`/`BTreeSet`、Cargo tests、Clippy、Markdown 规范与阶段追踪器。

## Global Constraints

- 仅提供 `MODEL-TEST + REFINEMENT-DESIGN`，不得声称 `KERNEL-TRACE` 或 `REFINEMENT-PROOF`。
- 每个 ISSUE 先写失败测试，再写最小实现，并独立提交。
- `cargo xtask test` 只证明启动闭环无回归。
- 不新增依赖，不连接真实 kernel，不伪造非叶权限或 TLB 模型。
- 所有错误必须在状态修改前返回；失败路径整个 `RefineState` 不变。

## File Map

- Modify: `cosir/src/pagewalk.rs` — canonical 地址、叶遍历、FrameAllocator。
- Modify: `cosir/src/pagewalk_tests.rs` — pagewalk 与 allocator 负向测试。
- Modify: `cosir/src/memseg_refine.rs` — checked 迁移、α、create 语义。
- Modify: `cosir/src/memseg_refine_tests.rs` — 原子性、mutation 和生命周期测试。
- Modify: `cosir/src/main.rs` — 新接口和准确 PASS 文案。
- Modify: `Prima OS 实现阶段.md` — ISSUE 与证据事实源。
- Modify: `COSIR 原语规范.md` — §20/§20.5 边界。
- Modify: `Prima OS.md` — H1、跨架构和异构场景主张。

---

### Task 1: 登记审查问题和证据等级

**Files:**
- Modify: `Prima OS 实现阶段.md:43-70`

**Interfaces:**
- Produces: `ISSUE-MEM-001` 至 `ISSUE-MEM-006` 的唯一状态表；后续任务只更新对应行。

- [ ] **Step 1: 新增问题表**

在“证据索引”后加入：

```markdown
## 已知问题与纠偏计划

| ISSUE ID | 问题与影响 | 当前证据 | 修复阶段 | 可执行验收条件 | 状态 |
|---|---|---|---|---|---|
| ISSUE-MEM-001 | pagewalk 接受未对齐/非 canonical 地址，high-half 投影缺少符号扩展 | MODEL-TEST 暴露缺口 | 阶段一 | 地址别名、对齐、高低半测试通过 | Open |
| ISSUE-MEM-002 | 地址、页数、offset、usize 转换和 SegId 递增可能溢出 | MODEL-TEST 暴露缺口 | 阶段一 | 每个溢出返回稳定错误且状态不变 | Open |
| ISSUE-MEM-003 | α 未验证物理帧、offset 和 PTE 权限位 | REFINEMENT-DESIGN 不完整 | 阶段一 | 七项独立 mutation 均被拒绝 | Open |
| ISSUE-MEM-004 | allocator 未实施严格有限集合语义 | MODEL-TEST 暴露缺口 | 阶段一 | 重复、外部帧、重复归还均显式失败 | Open |
| ISSUE-MEM-005 | memseg_create 错误要求预存 Segment 映射权 | 与 POSIR §7.1 不一致 | 阶段一 | 删除 seg_cap_rights 参数 | Open |
| ISSUE-MEM-006 | SPEC-014/PASS 可能被外推为真实页表细化 | MODEL-TEST + REFINEMENT-DESIGN | 阶段一 | 三份权威文档和 harness 文案一致 | Open |
```

- [ ] **Step 2: 校准 SPEC-014 条目类型**

将证据类型改为 `MODEL-TEST + REFINEMENT-DESIGN`，暂时保留 ISSUE 为 Open；删除 SPEC-014 对 `cargo xtask test` 的直接引用。

- [ ] **Step 3: 检查并提交**

Run: `git diff --check`
Expected: 无输出。

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: register memseg refinement remediation issues"
```

---

### Task 2: 拒绝地址别名并检查全部区间运算

**Files:**
- Modify: `cosir/src/pagewalk.rs:5-133`
- Modify: `cosir/src/pagewalk_tests.rs:1-80`
- Modify: `cosir/src/memseg_refine.rs:16-298`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Produces:
```rust
pub enum PageWalkError { UnalignedAddress, NonCanonicalAddress }
pub fn is_canonical_vaddr(vaddr: u64) -> bool;
pub fn validate_page_vaddr(vaddr: u64) -> Result<(), PageWalkError>;
pub fn translate(table: &PageTable, vaddr: u64) -> Result<Option<&Pte>, PageWalkError>;
pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte) -> Result<(), PageWalkError>;
pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> Result<bool, PageWalkError>;
```
- Adds `MemSegError::{InvalidAddress, ArithmeticOverflow, SegmentIdExhausted}`.

- [ ] **Step 1: 写 pagewalk 失败测试**

在 `pagewalk_tests.rs` 增加并迁移旧调用到 `.unwrap()`：

```rust
#[test]
fn map_rejects_page_offset_without_modifying_tree() {
    let mut table = new_task_table();
    let before = table.clone();
    assert_eq!(
        map_vaddr(&mut table, 0x4000_0001, pte(1, 0, 0x1000)),
        Err(PageWalkError::UnalignedAddress),
    );
    assert_eq!(table, before);
}

#[test]
fn pagewalk_rejects_noncanonical_alias() {
    let mut table = new_task_table();
    assert_eq!(
        map_vaddr(&mut table, 0x0000_8000_0000_0000, pte(1, 0, 0x1000)),
        Err(PageWalkError::NonCanonicalAddress),
    );
}

#[test]
fn high_half_leaf_roundtrips_with_sign_extension() {
    const VA: u64 = 0xffff_8000_0000_0000;
    let mut table = new_task_table();
    map_vaddr(&mut table, VA, pte(1, 0, 0x1000)).unwrap();
    assert!(translate(&table, VA).unwrap().is_some());
    let mut leaves = BTreeMap::new();
    collect_leaves(&table, &mut leaves);
    assert!(leaves.contains_key(&VA));
}
```

- [ ] **Step 2: 运行确认红灯**

Run: `cargo test -p cosir pagewalk_tests -- --nocapture`
Expected: FAIL，缺少 `PageWalkError` 或旧签名不匹配。

- [ ] **Step 3: 实现 pagewalk 输入边界**

在 `pagewalk.rs` 增加：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageWalkError { UnalignedAddress, NonCanonicalAddress }

pub fn is_canonical_vaddr(vaddr: u64) -> bool {
    let upper = vaddr >> 48;
    if vaddr & (1 << 47) == 0 { upper == 0 } else { upper == 0xffff }
}

pub fn validate_page_vaddr(vaddr: u64) -> Result<(), PageWalkError> {
    if vaddr & (PAGE_SIZE - 1) != 0 { return Err(PageWalkError::UnalignedAddress); }
    if !is_canonical_vaddr(vaddr) { return Err(PageWalkError::NonCanonicalAddress); }
    Ok(())
}

fn canonicalize_48bit(vaddr: u64) -> u64 {
    let low = vaddr & 0x0000_ffff_ffff_ffff;
    if low & (1 << 47) != 0 { low | 0xffff_0000_0000_0000 } else { low }
}
```

三项公开操作先调用 `validate_page_vaddr` 再遍历；`collect_leaves` 插入 `canonicalize_48bit(...)`。

- [ ] **Step 4: 写 memseg 失败原子性测试**

先让 `RefineState` 及递归字段派生 `PartialEq, Eq`，增加：

```rust
#[test]
fn map_rejects_range_crossing_canonical_boundary_atomically() {
    let mut st = state();
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, SEG_MAP_R, 2, ACCESS_R).unwrap();
    let before = st.clone();
    let err = memseg_map(&mut st, SEG_MAP_R, true, seg, 7, 0,
                         0x0000_7fff_ffff_f000, 2, ACCESS_R).unwrap_err();
    assert_eq!(err, MemSegError::InvalidAddress);
    assert_eq!(st, before);
}

#[test]
fn map_rejects_segment_range_overflow_atomically() {
    let mut st = state();
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, SEG_MAP_R, 1, ACCESS_R).unwrap();
    let before = st.clone();
    let err = memseg_map(&mut st, SEG_MAP_R, true, seg, 7, u64::MAX,
                         0x5000_0000, 1, ACCESS_R).unwrap_err();
    assert_eq!(err, MemSegError::ArithmeticOverflow);
    assert_eq!(st, before);
}

#[test]
fn create_rejects_exhausted_segment_id_atomically() {
    let mut st = state();
    st.next_seg = u64::MAX;
    let before = st.clone();
    assert_eq!(memseg_create(&mut st, SEG_MAP_R, 1, ACCESS_R),
               Err(MemSegError::SegmentIdExhausted));
    assert_eq!(st, before);
}
```

- [ ] **Step 5: 实现 checked 辅助与预验证**

```rust
fn checked_page_vaddr(base: u64, index: u64) -> Result<u64, MemSegError> {
    let delta = index.checked_mul(PAGE_SIZE).ok_or(MemSegError::ArithmeticOverflow)?;
    let va = base.checked_add(delta).ok_or(MemSegError::ArithmeticOverflow)?;
    validate_page_vaddr(va).map_err(|_| MemSegError::InvalidAddress)?;
    Ok(va)
}
```

map/unmap 在任何写入前验证 base、最后一页、所有 Segment offset；create 使用 `usize::try_from(page_count)` 和 `next_seg.checked_add(1)`。禁止边遍历边写；先构造待提交的 `(va, frame_index)` 列表。

- [ ] **Step 6: 运行测试并更新 ISSUE**

Run:
```bash
cargo test -p cosir pagewalk_tests -- --nocapture
cargo test -p cosir memseg_refine_tests -- --nocapture
cargo test -p cosir
```
Expected: PASS。

将 `ISSUE-MEM-001/002` 改为 Fixed，记录上述命令。

- [ ] **Step 7: 提交**

```bash
git add cosir/src/pagewalk.rs cosir/src/pagewalk_tests.rs cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs "Prima OS 实现阶段.md"
git commit -m "fix: validate memseg addresses and checked arithmetic"
```

---

### Task 3: 强化 α 的物理叶关系

**Files:**
- Modify: `cosir/src/pagewalk.rs:111-133`
- Modify: `cosir/src/memseg_refine.rs:123-181`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Produces: `pub fn visit_leaves(table: &PageTable, visitor: &mut impl FnMut(u64, &Pte));`

- [ ] **Step 1: 写七项 mutation tests**

建立 `mapped_pte_mut` helper，分别突变并断言 α 拒绝：

```rust
#[test]
fn alpha_rejects_wrong_frame_phys() {
    let mut st = mapped_state(ACCESS_R);
    mapped_pte_mut(&mut st).frame_phys = 0xdead_0000;
    assert!(!memseg_alpha(&st));
}

#[test] fn alpha_rejects_bad_offset() { /* mapping.offset = u64::MAX */ }
#[test] fn alpha_rejects_nonpresent_leaf() { /* present = false */ }
#[test] fn alpha_rejects_supervisor_leaf() { /* user = false */ }
#[test] fn alpha_rejects_writable_mismatch() { /* writable = true for R */ }
#[test] fn alpha_rejects_nx_mismatch() { /* nx = true for RX */ }
#[test] fn alpha_rejects_access_outside_segment() { /* 同步改 tree/vspace access */ }
```

最后一项必须同步修改 PTE mapping 和 `vspace`，避免只测试双视图不一致。

- [ ] **Step 2: 确认红灯**

Run: `cargo test -p cosir alpha_rejects -- --nocapture`
Expected: 新测试至少一项 FAIL。

- [ ] **Step 3: 实现完整叶遍历和检查**

`visit_leaves` 向回调提供 canonical VA 和 `&Pte`；`collect_leaves` 复用它。α 检查：

```rust
let offset = usize::try_from(pte.mapping.offset).ok();
let expected = offset.and_then(|i| seg.frames.get(i));
if expected != Some(&pte.frame_phys) { ok = false; }
if !pte.present || !pte.user { ok = false; }
if pte.writable != (pte.mapping.access & ACCESS_W != 0) { ok = false; }
if pte.nx != (pte.mapping.access & ACCESS_X == 0) { ok = false; }
if pte.mapping.access == 0 || pte.mapping.access & !0x07 != 0 { ok = false; }
if seg.allowed_access & pte.mapping.access != pte.mapping.access { ok = false; }
```

每个失败写入可定位诊断。

- [ ] **Step 4: 验证、更新和提交**

Run:
```bash
cargo test -p cosir alpha_rejects -- --nocapture
cargo test -p cosir
```
Expected: PASS。

更新 `ISSUE-MEM-003` 为 Fixed。

```bash
git add cosir/src/pagewalk.rs cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs "Prima OS 实现阶段.md"
git commit -m "fix: validate physical and permission state in memseg alpha"
```

---

### Task 4: 实施 FrameAllocator 有限集合语义

**Files:**
- Modify: `cosir/src/pagewalk.rs:135-150`
- Modify: `cosir/src/pagewalk_tests.rs`
- Modify: `cosir/src/memseg_refine.rs:57-64,152-179,301-323`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `cosir/src/main.rs`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
```rust
pub enum FrameAllocatorError { DuplicateFrame, UnknownFrame, DuplicateFree }
pub fn FrameAllocator::new(&[PhysAddr]) -> Result<Self, FrameAllocatorError>;
pub fn free_frame(&mut self, PhysAddr) -> Result<(), FrameAllocatorError>;
pub fn RefineState::new(&[PhysAddr]) -> Result<Self, MemSegError>;
```

- [ ] **Step 1: 写 allocator 负向测试**

```rust
#[test]
fn allocator_rejects_duplicate_universe() {
    assert_eq!(FrameAllocator::new(&[0x1000, 0x1000]),
               Err(FrameAllocatorError::DuplicateFrame));
}

#[test]
fn allocator_rejects_unknown_return_atomically() {
    let mut a = FrameAllocator::new(&[0x1000]).unwrap();
    let before = a.clone();
    assert_eq!(a.free_frame(0x2000), Err(FrameAllocatorError::UnknownFrame));
    assert_eq!(a, before);
}

#[test]
fn allocator_rejects_duplicate_return_atomically() {
    let mut a = FrameAllocator::new(&[0x1000]).unwrap();
    let f = a.alloc().unwrap();
    a.free_frame(f).unwrap();
    let before = a.clone();
    assert_eq!(a.free_frame(f), Err(FrameAllocatorError::DuplicateFree));
    assert_eq!(a, before);
}
```

- [ ] **Step 2: 确认红灯并实现显式 Result**

Run: `cargo test -p cosir frame_allocator -- --nocapture`
Expected: FAIL。

使用 `BTreeSet` 验证 universe 唯一；`free_frame` 先验证成员和重复，再 push。

- [ ] **Step 3: 写 W6 和 destroy 原子性测试**

```rust
#[test]
fn alpha_rejects_duplicate_free_frame() {
    let mut st = state();
    st.alloc.free[1] = st.alloc.free[0];
    assert!(!memseg_alpha(&st));
}

#[test]
fn destroy_rejects_corrupt_allocator_atomically() {
    let mut st = state();
    let seg = memseg_create(&mut st, SEG_MAP_R, 1, ACCESS_R).unwrap();
    let frame = st.segments[&seg].frames[0];
    st.alloc.free.push(frame);
    let before = st.clone();
    assert_eq!(memseg_destroy(&mut st, true, seg),
               Err(MemSegError::FrameAllocatorCorrupt));
    assert_eq!(st, before);
}
```

- [ ] **Step 4: 实现集合划分和 destroy 预检**

W6 分别构造 `universe/free/used` 的 `BTreeSet`，检查每个源 Vec 无重复、均属于 universe、used 与 free 不交且并集等于 universe。destroy 先验证所有帧可归还，再删除 Segment 和批量归还。

- [ ] **Step 5: 迁移构造调用并验证**

所有 `RefineState::new(...)` 和 `FrameAllocator::new(...)` 增加 `.unwrap()`；运行：

```bash
cargo test -p cosir frame_allocator -- --nocapture
cargo test -p cosir
```
Expected: PASS。

更新 `ISSUE-MEM-004` 为 Fixed，并提交：

```bash
git add cosir/src/pagewalk.rs cosir/src/pagewalk_tests.rs cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs cosir/src/main.rs "Prima OS 实现阶段.md"
git commit -m "fix: enforce finite-set semantics for frame allocation"
```

---

### Task 5: 对齐 `memseg_create` 与 POSIR

**Files:**
- Modify: `cosir/src/memseg_refine.rs:85-121`
- Modify: `cosir/src/memseg_refine_tests.rs`
- Modify: `cosir/src/main.rs:118-143`
- Modify: `COSIR 原语规范.md:513-538,2151-2179`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
```rust
pub fn memseg_create(st: &mut RefineState, page_count: u64,
                     allowed_access: u8) -> Result<SegId, MemSegError>;
```

- [ ] **Step 1: 写编译失败测试**

```rust
#[test]
fn create_requires_no_preexisting_segment_authority() {
    let mut st = state();
    let seg = memseg_create(&mut st, 1, ACCESS_R | ACCESS_W).unwrap();
    assert_eq!(st.segments[&seg].allowed_access, ACCESS_R | ACCESS_W);
}
```

Run: `cargo test -p cosir create_requires_no_preexisting_segment_authority -- --nocapture`
Expected: FAIL，旧签名要求四个参数。

- [ ] **Step 2: 删除额外参数与检查**

删除 `seg_cap_rights` 和 `seg_cap_rights & 0x07 == 0`。保留 `MissingMapRight` 给 `memseg_map`。迁移 `main.rs` 和全部测试调用。

- [ ] **Step 3: 同步规范并验证**

在 COSIR §7.1/§20.5 明确根 Capability 是成功结果，而非创建前置条件。

Run:
```bash
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS。

更新 `ISSUE-MEM-005` 为 Fixed，提交：

```bash
git add cosir/src/memseg_refine.rs cosir/src/memseg_refine_tests.rs cosir/src/main.rs "COSIR 原语规范.md" "Prima OS 实现阶段.md"
git commit -m "fix: align memseg creation with POSIR authority semantics"
```

---

### Task 6: 校准 SPEC-014 与研究主张

**Files:**
- Modify: `cosir/src/main.rs:138-143`
- Modify: `COSIR 原语规范.md:1967-2179`
- Modify: `Prima OS.md:865-976,1010-1024`
- Modify: `Prima OS 实现阶段.md:43-70`

**Interfaces:**
- Produces: 统一证据词汇和研究停止条件。

- [ ] **Step 1: 降级 harness PASS 文案**

替换为：

```rust
println!("PASS model-test memseg page-walk dual view");
println!("PASS model-test memseg W6/W7 checks");
```

不得声称 α 已验证完整 W8；当前仅有 unmap entry-page guard。

- [ ] **Step 2: 修正 COSIR 内部矛盾**

§20 开头写明当前包含 Bootstrap 投影和 MemorySegment host 四级索引树；二者均为 `REFINEMENT-DESIGN`，Rust 测试仅为 `MODEL-TEST`。§20.5 列出未覆盖：非叶权限合成、页表页物理身份、TLB、多核、真实 CR3/PTE 和原子更新窗口。

- [ ] **Step 3: 降级白皮书主张**

将“跨架构完全适用”改成候选适用；GPU/NPU/分布式组合改成待验证映射假设；删除“为 H1 提供经验支撑”，替换为只有冻结 observable semantics 并形成 `COMPAT-TEST` 后才能支持。

- [ ] **Step 4: 增加研究停止条件和下一切片**

在 tracker 写入批准的停止条件，并登记下一垂直切片：COW/page fault → handler → `exc_resume` → TLB → Linux trace。

- [ ] **Step 5: 验证文档和输出**

Run:
```bash
git diff --check
cargo test -p cosir
cargo run -p cosir
```
Expected: PASS；输出不含未限定的 `refinement PASS`。

更新 `ISSUE-MEM-006` 为 Fixed，提交：

```bash
git add cosir/src/main.rs "COSIR 原语规范.md" "Prima OS.md" "Prima OS 实现阶段.md"
git commit -m "docs: calibrate SPEC-014 and portability evidence claims"
```

---

### Task 7: 阶段一总验收与独立审查门

**Files:**
- Modify only if evidence recording is needed: `Prima OS 实现阶段.md`

- [ ] **Step 1: 运行格式、测试和 lint**

```bash
cargo fmt --all --check
cargo test -p cosir
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

Expected: 全部退出码 0。最后一项仅支持 QEMU 启动无回归。

- [ ] **Step 2: 运行独立审查**

调用 verification agent，传入原始阶段一目标、改动文件和各 ISSUE；要求重点复核 canonical alias、失败原子性、α mutation 隔离、allocator partition 和证据表述。Expected verdict: PASS；若 FAIL/PARTIAL，保持阶段门未通过并创建具体修复任务。

- [ ] **Step 3: 记录实际证据**

仅在所有命令真实通过后，把日期、测试数量、命令和直接支持命题写入 tracker；不得复用旧数量。

- [ ] **Step 4: 提交证据记录（仅有文档变化时）**

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: record verified memseg remediation evidence"
```

**阶段门：** verification PASS 前不得执行阶段二计划。
