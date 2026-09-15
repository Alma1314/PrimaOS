# Stage 1 closure: frame ownership + initial Authority Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.
>
> 本文件是 2026-09-16 已执行计划的记录（与实现同批落盘）；所有步骤均已实际运行。

**Goal:** 为阶段 1 的两个未勾选退出条件补齐真实内核实现与可复查证据：S1-X3（活动 Segment
与空闲页帧唯一归属，COSIR §16 W6）与 S1-X2（init 只获得 §14 定义的初始 Authority）。

**Architecture:** 新增两个纯逻辑模块——`kernel/src/rights.rs`（§4.1 权限位集与类型匹配）与
`kernel/src/frames.rs`（W6 帧注册表：free ⊎ Segment 帧 ⊎ 保留页表帧，单一带标签取帧入口）。
`bootstrap.rs` 按 §14 步骤 1-9 重写 `create_state` 并物化 image Segment / 空 Namespace /
Ready 初始状态 / 初始 Audit / `booted`。协议新增 `authority_ready`、`frames_ready` 两条
阶段末见证（追加在既有 21 条之后，保持其 seq 与 data 不变）。**不替换 bootloader CR3**。

**Tech Stack:** Rust `no_std` 内核（nightly 2026-07-30）、`cargo xtask`、QEMU BIOS run=1/2。

## Global Constraints

Copy verbatim from `docs/superpowers/specs/2026-09-16-stage1-closure-design.md`:

- **既有事件的一处数据变化**：`bootstrap_ready.namespace_rights` 由通配符 `all` 改为规范有限集 `Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy`。这是去通配符的**有意修正**（§4.1 要求 `FiniteSet<Right>`），按维护规则 5 显式记录，**不隐藏**；其余既有事件的 data 不变。
- image Segment 的 `frames` 在本实现中以 `(base, count)` 连续区间表达，规范是 `Vector<FrameId>`；本实现另有 `build_inventory` 校验 image 与 Usable 不相交，故区间表达不损失本切片所需语义，但**不是** `Vector<FrameId>` 的完整细化。
- §14 步骤 4 的逐页 vspace 映射**未物化到真实 CR3**：真实内核仍运行在 bootloader 提供的页表上；本切片只建立对象与归属，不替换映射。
- bring-up 的 Segment（code/stack/X/Y/spin/COW/vspace 见证）以 ID 标签 + 帧计数表达，待阶段 2 的 `memseg_create` 接管为真实用户可见 Segment。
- **不交性由构造保证、守恒性由计数校验**：本切片不保存 frame→owner 的逐帧映射，因此无法检测"同一帧被同时计入两个 Segment"这类构造外错误（该路径在本实现中不可达）。
- 未实现：`memseg_create/map/unmap/destroy`、`ns_*` 作为用户可调用原语（阶段 2）；`TaskStatus::Faulted`；`REFINEMENT-PROOF`。
- 本切片产出可复查证据，但**不勾选 S1-X2/S1-X3、不宣告阶段 1 关闭**：沿用与阶段 0 / S0-X1 相同的组织级独立性门槛。
- 保持原分配顺序，既有事件中的帧号必须逐字节不变。

## 文件结构

- Create `kernel/src/rights.rs` — `Right`/`Rights`/`ObjectKind` + host 单测。
- Create `kernel/src/frames.rs` — `FrameRegistry`/`FrameWitness`/带标签取帧入口 + host 单测。
- Modify `kernel/src/bootstrap.rs` — 权限 bitset 化、§14 物化、`activate_init`、`BootstrapRecord`、`validate_authority`。
- Modify `kernel/src/{main,resume,protocol,cow,user,vspace}.rs`、`kernel/src/lib.rs` — 取帧路径迁移与事件发射。

---

### Task 1: rights bitset — `kernel/src/rights.rs`

- [x] **Step 1:** `Right`(20 项)/`Rights`(u32 位集)/`ObjectKind` + `parse`/`render`/`subset_of`/`well_typed`。
- [x] **Step 2:** `bootstrap.rs` 权限常量改 `Rights`，`NAMESPACE_RIGHTS` 去 `"all"`；`derive/revoke/validate/authorize` 改用位集；`syscall::right_name` → `right_of -> Option<Right>`。
- [x] **Step 3:** `cargo test -p kernel`。Expected: 全绿（原 206 → 新增 rights 9 + 类型匹配 1）。

### Task 2: 帧注册表 — `kernel/src/frames.rs`

- [x] **Step 1:** `FrameRegistry::new` 按 §14 定 universe（含 image）；`open_segment`/`allocate_for_segment`/`allocate_table_frame`/`check_conservation`/`witness`。
- [x] **Step 2:** 全局 `install`/`allocate_*` 入口（`unsafe` 收敛于此）。
- [x] **Step 3:** 迁移 `main.rs`（install）、`vspace::run_witness`、`cow.rs`（删分配器副本）、`user.rs`（每区域先 `open_segment`，页表帧走 `allocate_table_frame`）。
- [x] **Step 4:** `cargo test -p kernel`（frames 13 条）+ `cargo xtask test`。Expected: 既有帧号逐字节不变。

### Task 3: 初始 Authority 物化 — `kernel/src/bootstrap.rs`

- [x] **Step 1:** `SegmentObject`/`NamespaceObject` 表 + `booted` + `BootstrapRecord`。
- [x] **Step 2:** `create_state(image_base, image_len, entry, inventory)`：入口范围前置、image Segment、空 Namespace、初始 Audit、`booted`、`AlreadyBooted`。
- [x] **Step 3:** `TaskRecord.root_ns_cap`；init 初始 `Ready`；`activate_init()` 迁 `Running` + `TaskStateChanged`；main 进入用户态前调用。
- [x] **Step 4:** `validate_authority` 增类型匹配、对象存在性、W5 结构性根。
- [x] **Step 5:** `cargo test -p kernel` + `cargo xtask test`。

### Task 4: 见证事件与 xtask 校验

- [x] **Step 1:** `authority_ready`/`frames_ready` 发射函数；在 `resume::after_resume` 于 `exc_resume_ready` 之后、`boot_complete` 之前发射。
- [x] **Step 2:** `xtask/src/protocol.rs`：`EventKind`、`EXPECTED_EVENTS`(22→24)、`parse_line`、data 允许表、`validate_authority_data`/`validate_frames_data`（含 W6 等式与 `conserved`）、golden 全文更新。
- [x] **Step 3:** 反例单测 4 条（initial_status、booted、W6 失衡、conserved=0、table=0）。
- [x] **Step 4:** `cargo xtask test`。Expected: 六条 PASS。

### Task 5: 文档与 tracker

- [x] **Step 1:** design/plan/report 三件套。
- [x] **Step 2:** tracker 增 SPEC-061/TEST-008，登记边界的 ISSUE，**不勾选** S1-X2/S1-X3。

### Task 6: 全量验证

- [x] `cargo test --workspace`（cosir 145、cosir_state 40、lean_backtrace 2、kernel 225、xtask 125）。
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- [x] `cargo xtask test` BIOS run=1/2 六条 PASS；`cargo xtask compat` PASS。
