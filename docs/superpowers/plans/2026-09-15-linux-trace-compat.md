# Linux trace + bounded COMPAT-TEST (cow-write-fault) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.
>
> 本文件是 2026-09-15 已执行计划的记录（与实现同批落盘）；所有步骤已实际运行并记录结果。

**Goal:** 为已冻结切片的最后节点建立"真实内核 trace ↔ 参考 Linux trace"的可审查对应：在
`cow-write-fault` 场景契约下，把 Prima 的 `user_cow_ready` 观测与 pinned 参考 Linux guest 的
`exceptions:page_fault_user` trace + 程序自证结果做语义化差分。

**Architecture:** Prima 侧零内核改动（既有事件字段已覆盖契约）。新增 `xtask/src/linux.rs`
（参考 guest 构建/运行/归一化）与 `xtask/src/compat.rs`（schema + 解析 + 判定 + host 单测）；
`xtask/src/qemu.rs` 抽出通用捕获运行器；`xtask/src/main.rs` 增 `compat`/`linux-trace` 子命令。
归一化基线入库，差分离线。

**Tech Stack:** Rust（xtask，edition 2024）、`cargo xtask`、QEMU 8.2.2（KVM/TCG）、pinned
`linux-image-6.8.0-31-generic` + busybox static initramfs、gcc 静态编译 guest 程序。

## Global Constraints

Copy verbatim from the spec `docs/superpowers/specs/2026-09-15-linux-trace-compat-design.md`:

- 本切片是**有界差分**，不是 [[Prima OS]] §8.3/§13.4 的完整 `COMPAT-TEST`：后者要求 ELF 加载器、Linux 初始栈与 Prima LibOS 上运行**同一二进制**，当前阶段 5/6 均未开始。故证据名目为 `KERNEL-TRACE` + **`COMPAT-TEST (bounded: cow-write-fault)`**，**不勾选 S6-X2**。
- Prima 侧的比较输入是**后置见证事件**（单条 `user_cow_ready`），无法观测故障重数；判定 5 因此只要求"存在故障记录"，不宣称"目标页恰故障一次"。Linux 侧基线额外要求目标页恰一次（`normalize` 在 >1 时报错），避免把非冻结场景混入基线。
- 不比较：页表级结构、CR3/PCID、TLB shootdown、文件后端、绝对地址、进程/时间维度。TLB 节点由判定 5 间接覆盖——重试成功（两侧 `receipt`/`write_retried_ok`）要求重试时观测到已刷新的翻译，否则会再次 fault 而无法完成。
- 不修改 `COSIR 原语规范.md` 与 `Prima OS.md` 的语义；不宣称 `REFINEMENT-PROOF`。
- 若差分要求修改已冻结的 POSIR observable semantics 或新增架构专属原语，按 tracker 既有停止条件停止并记为 H1/H3 反例。
- 只做单场景 `cow-write-fault`；不追加 `not_present` 缺页场景，也不做 `tlb_flush` tracepoint 对比。
- 提交归一化基线 + 离线比对；内核 deb 不入库，只入库归一化 trace 与来源声明（含 deb SHA512）。
- 差分作为独立命令，不并入 `cargo xtask test`；标准套件保持 30s、无网络、既有 22 条事件的精确序列断言不变。
- Prima 侧零内核改动；既有证据逐字节不变。
- 参考 guest 的 tracepoint 缺失时必须判失败（`LINUX|error=no_page_fault_tracepoint`），不得降级为通过。

## 文件结构

- Create `xtask/src/compat.rs` — 观测 schema、两侧解析、差分判定、host 单测。
- Create `xtask/src/linux.rs` — pinned 内核拉取/校验、initramfs 构建、guest 运行、归一化、来源声明。
- Create `xtask/assets/linux/cow_write_fault.c` — Linux 侧场景契约实现。
- Create `xtask/assets/linux/init` — guest init（挂 tracefs、开 tracepoint、跑程序、dump、poweroff）。
- Create `xtask/tests/fixtures/linux-trace/cow-write-fault.trace` — 提交入库的归一化基线。
- Create `xtask/tests/fixtures/linux-trace/PROVENANCE.toml` — 基线来源声明。
- Modify `xtask/src/main.rs` — 子命令与参数解析。
- Modify `xtask/src/qemu.rs` — `run_captured_with`、`run_bios_capture`、`locate_qemu` 可见性。
- Modify `xtask/src/protocol.rs` — `cow_pattern`/`USER_STORE_VALUE` 改 `pub(crate)` 供比较器镜像。

---

### Task 1: 差分比较器 — `xtask/src/compat.rs`

**Files:** Create `xtask/src/compat.rs`

**Interfaces:**
- Produces: `CowObservation`、`Platform`、`Divergence`、`CompatVerdict`、
  `classify(err) -> &'static str`、`compare_cow_write_fault(&CowObservation, &CowObservation) -> CompatVerdict`、
  `parse_prima(&str) -> Result<CowObservation, String>`、`parse_linux_trace(&str) -> Result<CowObservation, String>`。

- [x] **Step 1: 写正例与反例单测，先红。**
- [x] **Step 2: 实现 schema/解析/判定，使单测转绿。**
- [x] **Step 3: 运行 `cargo test -p xtask`。** Expected: 120 passed（含 14 条新测）。

### Task 2: 参考 Linux guest — `xtask/src/linux.rs` + assets

**Files:** Create `xtask/src/linux.rs`、`xtask/assets/linux/cow_write_fault.c`、`xtask/assets/linux/init`

**Interfaces:**
- Produces: `regenerate(force_tcg: bool) -> Result<(), String>`、`load_baseline() -> Result<String, String>`、
  `normalize(serial: &str) -> Result<String, String>`。

- [x] **Step 1: 端到端 spike 验证 tracepoint 可用。** 结果：`exceptions:page_fault_user` 可用；
  `mprotect(PROT_READ)` 方案子进程收到 `SIGSEGV`(0xb)，改 `fork`-COW 后目标页单次 `error_code=0x7`。
- [x] **Step 2: 实现 pinned 内核拉取 + SHA512 校验 + initramfs 构建 + guest 运行。**
- [x] **Step 3: 实现归一化（要求目标页恰一次故障、child 正常退出），并自检生成基线可被比较器解析。**
- [x] **Step 4: 运行 `cargo test -p xtask`。** Expected: 归一化 4 条单测通过。

### Task 3: CLI 接线 — `xtask/src/main.rs` + `qemu.rs`

**Files:** Modify `xtask/src/main.rs`、`xtask/src/qemu.rs`、`xtask/src/protocol.rs`

- [x] **Step 1: 抽出 `run_captured_with`/`run_bios_capture`，保持 BIOS 常量与 30s 超时不变。**
- [x] **Step 2: 增 `compat [--baseline <path>]` 与 `linux-trace [--regenerate [--tcg]]`。**
- [x] **Step 3: 运行 `cargo xtask linux-trace --regenerate`。** Expected: `PASS linux-trace regenerate (kvm)`
  （实测 kvm；deb SHA512 校验通过）。
- [x] **Step 4: 运行 `cargo xtask compat`。** Expected: `PASS compat cow-write-fault`。

### Task 4: 文档与 tracker

**Files:** Create design/plan/report；Modify `Prima OS 实现阶段.md`

- [x] **Step 1: 写 design（本文件同目录）+ 本 plan + report。**
- [x] **Step 2: tracker 增 SPEC-060（比较器 + 参考 harness）与 TEST-007（端到端差分 + 负例），
  如实改写冻结切片段与"当前首要缺口"，不勾选 S6-X2、不动阶段 0 门槛。**

### Task 5: 全量验证与负例保留

- [x] **Step 1: `cargo test --workspace`。** Expected: cosir 145、kernel 206、xtask 120 全绿。
- [x] **Step 2: `cargo clippy --workspace --all-targets -- -D warnings`。** Expected: 零告警。
- [x] **Step 3: `cargo xtask test`。** Expected: 六条 PASS、既有 22 事件不变。
- [x] **Step 4: 负例注入（`--baseline` 指向篡改副本）。**
  `error_code=0x7→0x3` ⇒ `ErrBitsMismatch`；`count=1→0` ⇒ `MissingFault`；`order_ok=1→0` ⇒
  `OrderViolation`。
