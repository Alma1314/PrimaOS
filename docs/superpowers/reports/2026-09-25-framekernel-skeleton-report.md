# 切片 AE 报告：framekernel 分层与 unsafe 收敛（SPEC-076）

> 文档类型：真实内核实现报告（framekernel 分层序列第 1 片）
>
> 日期：2026-09-25
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 设计：`docs/superpowers/specs/2026-09-25-framekernel-skeleton-design.md`

## 1. 定位

落实差距分析报告（`2026-08-16-framekernel-gap-analysis.md`）的 **G2**（`kernel/` 无 unsafe 收敛纪律）与 **G8**（审计与 crate 重组待代码增长后执行），参考 Asterinas framekernel 划出 safe/unsafe 的 crate 边界。本片只做地基，不做调用方逻辑迁移。

## 2. 交付物

| 工件 | 内容 |
|---|---|
| `prima-ostd/`（新 crate，363 行） | 唯一允许 `unsafe` 的框架层。`arch`（port I/O / msr / cr3 / cr0 / invlpg / hlt / cli / sti；每个原语 `#[inline]`）、`sync`（`StaticCell<T>` 闭包式安全读写、`Once<T>`）。`#![cfg_attr(not(test), no_std)]` + `#![deny(unsafe_op_in_unsafe_fn)]` |
| `kernel/src/lib.rs` | 加 `#![deny(unsafe_code)]`；14 个未收敛模块临时 `#[allow(unsafe_code)]` |
| `kernel/src/main.rs` | 加 `#![deny(unsafe_code)]`；`resume` 临时 `#[allow(unsafe_code)]` |
| 调用点迁移 | `msr.rs`（`rdmsr`/`wrmsr` 委托框架，`asm!` 删除）、`cpu.rs`（`cr3`/`write_cr3`/`read_cr0_wp` 委托框架）、`vspace.rs`（`invlpg` 删除）、`serial.rs`/`qemu.rs`（port I/O 与 `hlt` 改框架调用）、`sched.rs`/`cow.rs`/`bootstrap.rs`/`syscall.rs` 相应调用点去 `unsafe` |
| 构建 | `Cargo.toml` 增 `prima-ostd` member、`kernel/Cargo.toml` 增 path 依赖（bindeps 无需改）；`Cargo.lock` 更新 |

`entry_point!` / `#[panic_handler]` / `_start` **留在 bin**（设计规则 1）；内核 bin 在 `#![deny(unsafe_code)]` 下编到 `x86_64-unknown-none` 通过，证实 `entry_point!` 宏不触发该 lint。

## 3. 收敛进度（可核验指标）

| 指标 | HEAD | 本切片后 |
|---|---|---|
| `kernel/src` 行数 | 20,496 | 20,439 |
| kernel 真实 unsafe 操作（块/fn/impl/extern/naked） | 220 + 已迁出项 | **220**（注：迁移把 11 处操作搬入框架；下表口径见备注） |
| 框架 crate unsafe 操作 | — | 16 块 + 2 `unsafe impl`（`Sync`） |
| 临时 `#[allow(unsafe_code)]` 模块数 | — | lib **14** + bin **1** |

**本片真正转为零 unsafe 的模块**：`kernel::msr`（lib）、bin 的 `serial`、bin 的 `qemu`。其余模块的 unsafe 由 AF/AG/AH 迁出，AI 收口时删除全部 `#[allow]`。

备注：kernel 的 unsafe 操作计数在本片后仍为 220，是因为宿主 14 个模块的 `#[allow]` 尚未收缩；**本片的价值在于建立 crate 边界与强制门**，不是减少 kernel 侧计数。框架侧新增 18 处是**信任面的显式化**（原先散落各处的同一批 `asm!`）。

## 4. 回归面实测（完整事件流 diff）

方法：用 `git worktree` 在 HEAD 独立构建基线镜像，与本切片镜像各跑一次 QEMU（TCG，参数同 `xtask`），对全部 PRIMA 事件逐字节 diff。

**事件序列与 seq 全部不变（35 条）**。实测先有 4 条事件 data 变化；给框架原语加 `#[inline]`（"零开销 safe API"）后收敛为 **2 条**：

| 事件(seq) | HEAD 基线 | 本切片 | 变化字段 | 原因 |
|---|---|---|---|---|
| `bootstrap_ready` (9) | `image_len=3379568` | `image_len=3382336` | `image_len` **+2768 B** | 内核镜像跨 crate 后代码布局变化；`task/caps/*_cap/image_base/image_frames/free_frames/image_rights/...` 全部不变 |
| `preempt_ready` (25) | `rsp1=0x1000008f508`,`rsp2=0x10000093508`,`kstack1_top=0x1000008f530`,`kstack2_top=0x10000093530` | `...90688`/`...94688`/`...906b0`/`...946b0` | 启动栈地址随镜像后移（+0x1180） | 同上；`ticks/pre1/pre2/root1/root2/cr3*/data*/mark*` 不变 |

其余 **33 条事件逐字段不变**。中间版本（未加 `#[inline]`）曾额外扰动 `memory_ready`（`usable/free` 30648→30649）与 `frames_ready`（`universe` 31474→31475、`free` 30538→30539）——这两条按 §4 的守恒/非零断言本不会被测试拦住，靠手工事件流 diff 发现；`#[inline]` 使镜像回到该帧边界之下，二者恢复基线。**教训**：`xtask test` 的断言对镜像尺寸类字段只做守恒/非零检查，跨 crate 改动的实际事件流必须手工 diff。

确定性复核：同一镜像连跑两次，事件流逐字节相同。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、**prima-ostd 5**、xtask 167 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 零告警 |
| `cargo fmt --all --check` | 无差异 |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（35 条事件序列断言通过） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| `cargo build -p kernel --target x86_64-unknown-none` | 通过（`deny(unsafe_code)` 下 bin+lib 均编译） |

## 6. 口径偏差登记

1. **safe-by-construction ≠ sound**：框架把不可静态验证的前置条件（如 `write_cr3` 要求"新根映射当前指令流"、`wrmsr` 要求"值合法"）降级为**文档契约**，即信任调用方。误用可产生错误行为，设计目标是不产生 UB。**不宣称已消除全部 UB 风险**，也不构成 soundness 证明。此取舍由用户批准（方案"独立底层 crate / 完整序列 / 纳入 G3"）。
2. **`#[allow(unsafe_code)]` 是临时的**：本片后 kernel 仍有 14 + 1 个模块带 allow，unsafe 未真正减少；收敛以 AF→AG→AH→AI 的 allow 收缩为进度指标。
3. **`invlpg` 由 `unsafe fn` 改为安全 `fn`**：原实现带 "仅 CPL0" 的 `# Safety`；框架版把 CPL0 作为文档契约（非法 `va` 本身不构成 UB）。调用点相应去 `unsafe`。
4. **既有事件 data 2 条变化照实登记**（维护规则 5）：见 §4，均为镜像尺寸/布局的间接后果，无语义变化、无 seq 顺移。

## 7. 遗留与后续

- **AF（SPEC-077）**：物理内存/页表 POD 收敛（`safety.rs`、`vspace.rs::read_raw/write_raw`、`cpu.rs::read_entry`、`cow.rs`/`user.rs`/`syscall.rs`/`bootstrap.rs` 的裸 volatile 访问）。
- **AG（SPEC-078）**：38 处 `static mut` → `StaticCell`/`Once`。
- **AH（SPEC-079）**：5 个裸入口 + IDT/GDT 下沉，回调反转；删内核 `abi_x86_interrupt`。
- **AI（SPEC-080）**：删全部 `#[allow]`、xtask 加 lint/零-unsafe 门禁、TCB 量化指标（G3）。
- **本片不勾选任何阶段状态**；闭合判断交复核者。
