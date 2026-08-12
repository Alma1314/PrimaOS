# exc_resume Security Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在阶段一审查通过后，补全 `exc_resume` host 模型的 Capability、订阅、恢复帧验证、清零、Audit 和失败原子性。

**Architecture:** 扩展现有 `state.rs` 的最小状态，不实现通用 Capability 系统；`ops.rs` 将恢复拆成纯验证与不可失败的原子提交。`SretFrame` 使用固定 little-endian wire format，从 Segment 字节读取 RIP/RSP/CS/SS/RFLAGS，不保留绕过授权的兼容接口。

**Tech Stack:** Rust std collections、固定字节编码、Cargo tests、Clippy、COSIR Markdown 规范。

## Global Constraints

- 前置条件：`2026-08-12-memseg-refinement-remediation.md` 的独立审查 verdict 为 PASS。
- 阶段二仍只提供 `MODEL-TEST`，不提供真实 trap/`iretq` 的 `KERNEL-TRACE`。
- 不新增依赖，不修改 kernel，不实现通用 Capability 派生、撤销或 Audit 查询系统。
- 每项失败测试必须比较完整 `State`，证明状态深拷贝相等。
- 删除旧 `resume_vaddr` 参数和裸对象 ID 接口，不增加兼容 wrapper。

## File Map

- Modify: `cosir/src/state.rs` — Capability、Segment bytes、mapping range、wire frame、Audit。
- Modify: `cosir/src/ops.rs` — 完整授权、验证和原子提交。
- Modify: `cosir/src/main.rs` — fixture、mutation tests 和 smoke harness。
- Modify: `COSIR 原语规范.md` — §12.9/§12.10。
- Modify: `docs/superpowers/specs/2026-08-07-exc-resume-design.md` — 历史设计 amendment。
- Modify: `Prima OS 实现阶段.md` — ISSUE-EXC 状态与证据。

---

### Task 1: 登记阶段二 ISSUE 并建立可比较状态

**Files:**
- Modify: `Prima OS 实现阶段.md`
- Modify: `cosir/src/state.rs:3-111`
- Modify: `cosir/src/main.rs:17-64`

**Interfaces:**
```rust
pub type CapId = u32;
pub const CAP_EXC_SUBSCRIBE: u8 = 0b0001;
pub const CAP_SEG_WRITE: u8 = 0b0010;
pub enum CapObject { ExceptionSource(ExcSourceId), Segment(SegId) }
pub struct Capability { pub owner: TaskId, pub object: CapObject,
                        pub rights: u8, pub active: bool }
pub struct Segment { pub bytes: Vec<u8>, pub kernel_readable: bool }
pub struct PageMapping { pub base: VirtAddr, pub len: u64,
                         pub access: u8, pub user: bool }
pub enum AuditEvent { TaskResumed { actor: TaskId, source_id: ExcSourceId,
    source_cap: CapId, frame_cap: CapId, frame_seg_id: SegId,
    frame_offset: usize, rip: VirtAddr } }
```

- [ ] **Step 1: 在 tracker 登记 ISSUE-EXC**

加入：

```markdown
| ISSUE-EXC-001 | exc_resume 未检查 Capability、持有者、活跃状态和订阅 | MODEL-TEST 缺口 | 阶段二 | 每项授权 mutation 原子失败 | Open |
| ISSUE-EXC-002 | 恢复帧没有实际字节编码，RIP/RSP/CS/SS/RFLAGS 未完整验证 | MODEL-TEST 缺口 | 阶段二 | wire-format 与字段 mutation tests 通过 | Open |
| ISSUE-EXC-003 | 成功恢复未清零用户帧、保存 kernel frame 或追加 Audit | MODEL-TEST 缺口 | 阶段二 | 成功提交顺序和重复消费测试通过 | Open |
| ISSUE-EXC-004 | §12.9/§12.10、旧设计与实现接口矛盾 | REFINEMENT-DESIGN 不一致 | 阶段二 | 三份文档与最终接口一致 | Open |
```

提交：

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: register exc_resume security remediation issues"
```

- [ ] **Step 2: 写状态深比较测试**

```rust
#[test]
fn exc_resume_state_is_deeply_comparable() {
    let st = build_state();
    assert_eq!(st, st.clone());
}

#[test]
fn page_mapping_contains_only_its_half_open_range() {
    let m = PageMapping { base: 0x5000_0000, len: 0x1000,
                          access: ACCESS_X, user: true };
    assert!(m.contains(0x5000_0000));
    assert!(m.contains(0x5000_0fff));
    assert!(!m.contains(0x5000_1000));
}
```

Run: `cargo test -p cosir exc_resume_state_is_deeply_comparable -- --nocapture`
Expected: FAIL。

- [ ] **Step 3: 实现最小状态类型**

替换 `SegRange` 为 `Segment { bytes, kernel_readable }`；`Task.vspace` 使用 `Vec<PageMapping>` 并增加 `kernel_resume_frame: Option<SretFrame>`；`State` 增加 `capabilities: HashMap<CapId, Capability>`、`subscriptions: HashSet<(TaskId, ExcSourceId)>`、`audit: Vec<AuditEvent>`。所有递归类型派生 `Clone, Debug, PartialEq, Eq`。

`PageMapping::contains` 必须使用 checked end：

```rust
pub fn contains(self, addr: u64) -> bool {
    self.base.checked_add(self.len)
        .is_some_and(|end| self.base <= addr && addr < end)
}
```

- [ ] **Step 4: 迁移 fixture 并提交**

`build_state` 建立 source/frame capabilities、订阅、实际 Segment bytes、X 和 W 用户映射。

Run:
```bash
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS。

```bash
git add cosir/src/state.rs cosir/src/main.rs
git commit -m "refactor: model exc_resume authorization state"
```

---

### Task 2: 定义固定 SretFrame wire format

**Files:**
- Modify: `cosir/src/state.rs:14-37`
- Modify: `cosir/src/main.rs`

**Interfaces:**
```rust
pub const SRET_FRAME_LEN: usize = 0xA0;
pub const SRET_FRAME_ALIGN: usize = 16;
pub const USER_CS: u64 = 0x1b;
pub const USER_SS: u64 = 0x23;
impl SretFrame {
    pub fn decode(bytes: &[u8]) -> Option<Self>;
    pub fn encode(self) -> [u8; SRET_FRAME_LEN];
}
```

- [ ] **Step 1: 写 layout/roundtrip 红灯测试**

```rust
#[test]
fn sret_frame_wire_layout_is_fixed_little_endian() {
    let f = valid_frame();
    let b = f.encode();
    assert_eq!(b.len(), 0xa0);
    assert_eq!(&b[0x38..0x40], &f.rsp.to_le_bytes());
    assert_eq!(&b[0x80..0x88], &f.rflags.to_le_bytes());
    assert_eq!(&b[0x88..0x90], &f.rip.to_le_bytes());
    assert_eq!(&b[0x90..0x98], &f.cs.to_le_bytes());
    assert_eq!(&b[0x98..0xa0], &f.ss.to_le_bytes());
}

#[test]
fn sret_frame_round_trips() {
    let f = valid_frame();
    assert_eq!(SretFrame::decode(&f.encode()), Some(f));
}

#[test]
fn sret_frame_rejects_wrong_length() {
    assert_eq!(SretFrame::decode(&[0; SRET_FRAME_LEN - 1]), None);
}
```

Run: `cargo test -p cosir sret_frame -- --nocapture`
Expected: FAIL。

- [ ] **Step 2: 实现显式 little-endian 编解码**

给 `SretFrame` 追加 `rip/cs/ss`。使用 `u64::from_le_bytes` 和 `to_le_bytes` 操作固定 offset；禁止 `unsafe`、`transmute`、`repr(C)`/`size_of` 作为 wire ABI。`decode` 要求输入长度恰为 `0xA0`。

- [ ] **Step 3: 验证并提交**

Run:
```bash
cargo test -p cosir sret_frame -- --nocapture
cargo test -p cosir
```
Expected: PASS。

```bash
git add cosir/src/state.rs cosir/src/main.rs
git commit -m "feat: define explicit x86_64 sret frame encoding"
```

---

### Task 3: 通过 Capability 授权 `exc_resume`

**Files:**
- Modify: `cosir/src/ops.rs:6-111`
- Modify: `cosir/src/main.rs:41-270`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
```rust
pub fn exc_resume(st: &mut State, caller: TaskId,
                  exc_source_cap: CapId, sret_frame_seg_cap: CapId,
                  frame_offset: usize) -> Result<(), ResumeError>;
```

`ResumeError` 最终集合：
```rust
InvalidExcSource, MissingRight, NotSubscribed, InvalidFrameAddr,
InvalidFrame, NoPendingException, UnrecoverableFault
```

- [ ] **Step 1: 建立统一原子失败 helper**

```rust
fn assert_resume_failure_unchanged(mut st: State, expected: ResumeError) {
    let before = st.clone();
    let err = exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, FRAME_OFF)
        .unwrap_err();
    assert_eq!(err, expected);
    assert_eq!(st, before);
}
```

fixture 常量固定为 CALLER=1、SOURCE=1、SOURCE_CAP=10、FRAME_SEG=1、FRAME_CAP=11。

- [ ] **Step 2: 写 source 授权 mutation tests**

分别测试：cap 缺失、inactive、foreign owner、错误 object type、缺 `CAP_EXC_SUBSCRIBE`、source inactive、subscription 缺失。期望分别为 `InvalidExcSource`、`MissingRight` 或 `NotSubscribed`，并调用完整状态 helper。

- [ ] **Step 3: 写 frame 授权 mutation tests**

分别测试：frame cap 缺失、inactive、foreign、错误 type、缺 `CAP_SEG_WRITE`、Segment `kernel_readable=false`。所有失败状态不变。

- [ ] **Step 4: 确认红灯并实现纯授权阶段**

Run: `cargo test -p cosir exc_resume_rejects -- --nocapture`
Expected: FAIL。

实现顺序固定：source cap → source object → subscription → frame cap → Segment。检查阶段只用不可变借用，不修改状态。删除旧 `source_id/frame_seg_id/resume_vaddr` 接口并迁移所有调用；不保留 wrapper。

- [ ] **Step 5: 验证、更新和提交**

Run:
```bash
cargo test -p cosir exc_resume_rejects -- --nocapture
cargo test -p cosir
```
Expected: PASS。

将 `ISSUE-EXC-001` 标为 Fixed。

```bash
git add cosir/src/ops.rs cosir/src/main.rs "Prima OS 实现阶段.md"
git commit -m "feat: authorize exc_resume through capabilities"
```

---

### Task 4: 解码并验证恢复帧

**Files:**
- Modify: `cosir/src/state.rs`
- Modify: `cosir/src/ops.rs:49-111`
- Modify: `cosir/src/main.rs`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Private helpers: `is_user_canonical`, `has_user_access`, `valid_user_rflags`。
- Uses `SRET_FRAME_ALIGN == 16`，不是 frame 长度倍数。

- [ ] **Step 1: 写 frame 边界测试**

覆盖：16-byte 非零 offset 成功；offset=1；`checked_add` 溢出；Segment 长度不足。失败均比较完整状态。

```rust
#[test]
fn exc_resume_accepts_sixteen_byte_aligned_offset() {
    let mut st = faulted_state_with_frame_at(16, valid_frame());
    exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, 16).unwrap();
}
```

- [ ] **Step 2: 写 RIP/RSP 映射测试**

覆盖：noncanonical、kernel-half、映射外、权限错误和映射内部非 base 地址成功。RIP 要 X+user，RSP 要 W+user。

- [ ] **Step 3: 写 CS/SS/RFLAGS mutation tests**

CS 必须 `0x1b`，SS 必须 `0x23`。RFLAGS 至少逐项拒绝：bit1 未置、IOPL、NT、RF、VM、VIF/VIP 和高位保留位。

```rust
fn valid_user_rflags(flags: u64) -> bool {
    const FIXED: u64 = 1 << 1;
    const FORBIDDEN: u64 = (0b11 << 12) | (1 << 14) | (1 << 16)
        | (1 << 17) | (1 << 19) | (1 << 20);
    const ALLOWED: u64 = (1 << 0) | FIXED | (1 << 2) | (1 << 4)
        | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9)
        | (1 << 10) | (1 << 11) | (1 << 18) | (1 << 21);
    flags & FIXED != 0 && flags & FORBIDDEN == 0 && flags & !ALLOWED == 0
}
```

- [ ] **Step 4: 写 pending/source/kind tests**

覆盖 pending source 不匹配、source supported kind 不匹配、DoubleFault 不可恢复、Task 非 Faulted。kind 比较完整枚举值。

- [ ] **Step 5: 确认红灯并实现纯验证阶段**

Run: `cargo test -p cosir exc_resume -- --nocapture`
Expected: 新 mutation tests FAIL。

在任何 `get_mut/remove/push/fill` 之前完成：offset、bounds、decode、Task/pending、source kind、recoverability、RIP/RSP/CS/SS/RFLAGS。所有结果保存在局部变量。

- [ ] **Step 6: 验证、更新和提交**

Run:
```bash
cargo test -p cosir exc_resume -- --nocapture
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS。

将 `ISSUE-EXC-002` 标为 Fixed。

```bash
git add cosir/src/state.rs cosir/src/ops.rs cosir/src/main.rs "Prima OS 实现阶段.md"
git commit -m "feat: validate decoded exc_resume frames"
```

---

### Task 5: 原子消费、kernel frame 和 Audit

**Files:**
- Modify: `cosir/src/ops.rs:100-110`
- Modify: `cosir/src/main.rs`
- Modify: `Prima OS 实现阶段.md`

- [ ] **Step 1: 写成功提交测试**

```rust
#[test]
fn exc_resume_consumes_frame_and_commits_resume_atomically() {
    let frame = valid_frame();
    let mut st = faulted_state_with_frame_at(FRAME_OFF, frame);
    exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, FRAME_OFF).unwrap();
    assert_eq!(st.tasks[&CALLER].status, TaskStatus::Ready);
    assert!(!st.pending_exc.contains_key(&CALLER));
    assert_eq!(st.tasks[&CALLER].kernel_resume_frame, Some(frame));
    assert!(st.segments[&FRAME_SEG].bytes[FRAME_OFF..FRAME_OFF + SRET_FRAME_LEN]
            .iter().all(|byte| *byte == 0));
    assert_eq!(st.audit.len(), 1);
}
```

再测试只清零帧区间、失败不追加 Audit、成功后二次调用返回 `NoPendingException` 且状态不变、schedule 只做 Ready→Running 且不消费 kernel frame。

- [ ] **Step 2: 确认红灯并实现不可失败提交段**

Run: `cargo test -p cosir exc_resume_consumes -- --nocapture`
Expected: FAIL。

在所有验证通过后严格执行：

```text
Segment frame bytes fill(0)
→ pending_exc.remove
→ task.kernel_resume_frame = Some(frame)
→ audit.push(TaskResumed)
→ task.status = Ready
```

提交段只使用已验证索引和 `expect("validated ...")`，不返回新错误。

- [ ] **Step 3: 验证、更新和提交**

Run:
```bash
cargo test -p cosir
cargo clippy -p cosir --all-targets --no-deps -- -D warnings
```
Expected: PASS。

将 `ISSUE-EXC-003` 标为 Fixed。

```bash
git add cosir/src/ops.rs cosir/src/main.rs "Prima OS 实现阶段.md"
git commit -m "feat: atomically commit audited exception resumes"
```

---

### Task 6: 同步规范和证据边界

**Files:**
- Modify: `COSIR 原语规范.md:1390-1451`
- Modify: `docs/superpowers/specs/2026-08-07-exc-resume-design.md:33-103`
- Modify: `Prima OS 实现阶段.md:42,59`

- [ ] **Step 1: 修正 §12.9/§12.10**

删除“不定义 exc_resume 参数和语义”的矛盾文本。最终抽象签名为：

```text
exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset) -> ()
```

明确 RIP/RSP/CS/SS/RFLAGS 来自 Segment frame、x86_64 frame 为 16-byte aligned `0xA0` little-endian 编码、全部授权和失败原子性。

- [ ] **Step 2: 修订旧设计**

在 `2026-08-07-exc-resume-design.md` 顶部注明被 2026-08-12 纠偏设计修订；同步 `0x88→0xA0`、新增 RIP/CS/SS、删除 `resume_vaddr`、实际 Segment bytes、kernel frame、清零和 Audit。保留历史文件，不删除。

- [ ] **Step 3: 校准 SPEC-012**

标为 `MODEL-TEST`，不硬编码旧 12 tests，不引用 xtask 作为功能证据。将 `ISSUE-EXC-004` 标为 Fixed。

- [ ] **Step 4: 检查并提交**

Run:
```bash
git diff --check
cargo test -p cosir
```
Expected: PASS。

```bash
git add "COSIR 原语规范.md" "docs/superpowers/specs/2026-08-07-exc-resume-design.md" "Prima OS 实现阶段.md"
git commit -m "docs: align exc_resume specification with safe frame semantics"
```

---

### Task 7: 阶段二总验收与独立审查

**Files:**
- Modify only if recording evidence: `Prima OS 实现阶段.md`

- [ ] **Step 1: 运行总验收**

```bash
cargo fmt --all --check
cargo test -p cosir
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

Expected: 全部退出码 0；xtask 只支持启动闭环无回归。

- [ ] **Step 2: 独立验证**

调用 verification agent，传入阶段二原始目标、改动文件、接口和全部 ISSUE；要求尝试 capability/source/frame/pending/register mutation，并检查所有错误路径完整 State 相等。Expected verdict: PASS。

- [ ] **Step 3: 记录实际证据**

仅在 PASS 后记录实际测试数量、命令和直接支持命题。不得标记 `KERNEL-TRACE` 或 `REFINEMENT-PROOF`。

- [ ] **Step 4: 提交记录（若有变化）**

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: record verified exc_resume model evidence"
```

---

## Explicit Non-Goals

- 不修改 `kernel/src/main.rs`，不接入 IDT/trap/`iretq`。
- 不实现完整 Capability 派生、移动、撤销系统。
- 不实现通用 subscription 生命周期或 Audit 查询。
- 不增加 ARM/RISC-V frame。
- 不让 schedule 模拟寄存器加载或清空 kernel frame。
- 不把 host 模型测试提升为真实内核或形式化证据。
