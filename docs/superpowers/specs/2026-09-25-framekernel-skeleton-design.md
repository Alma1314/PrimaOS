# 切片 AE 设计：framekernel 分层与 unsafe 收敛（SPEC-076）

> 文档类型：真实内核实现设计（framekernel 分层序列第 1 片）
>
> 日期：2026-09-25
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-25，方案批准：独立底层 crate / 完整序列 / 纳入 G3）

## 0. 定位与前置

差距分析报告 `docs/superpowers/reports/2026-08-16-framekernel-gap-analysis.md` 的两项**未执行**行动项：

- **G2**（可采纳·约束）：`kernel/` 无 unsafe 收敛纪律，应"按唯一允许 unsafe 的细化 crate + safe 机制 crate 组织"；
- **G8**（后续工作）：unsafe 审计与 crate 重组"当前代码量小，待 kernel 代码增长后执行"。

现状（2026-09-25 实测）：`kernel/src/` **20,496 行**，261 行含 `unsafe`，分布在 **20 个文件**（bootstrap 64、syscall 41、cpu 20、idt 19、sched 16、frames 14、user 13、preempt 13、cow 11、vspace 9、serial 8、gdt 8、safety 5、exc 5、msr 4、mem 4、resume 3、qemu 2、main 1、lib 1）；**38 处 `static mut`**（全部经 `addr_of!` 访问）；5 个裸入口以 `sym` 直接引用上层符号。已到 G8 的启动条件。

参考对象：Asterinas framekernel——`unsafe` 全部收敛进 `ostd/`，`kernel/` 为 100% safe Rust，TCB 可度量。

## 1. 目标架构与边界规则

**两个 crate**：

| 层 | 内容 | unsafe |
|---|---|---|
| `prima-ostd`（新，唯一信任面） | `arch`、`sync`、`mm`、`entry`、`boot` | 允许 |
| `kernel`（上层机制，不可信） | syscall 派发 / sched / user / cow / frames / mem 纯逻辑 / handle / rights / bootstrap 组合 / main | `#![deny(unsafe_code)]` |

**边界规则**（对抗性校验得出，逐条写死）：

1. `entry_point!` / `_start` / `.bootloader-config` 段 / `#[panic_handler]` **留在 bin**（`kernel/src/main.rs:22,345`）。搬入 rlib 会与 std 的 `panic_impl` 在 host 测试链接期冲突，并有 `--gc-sections` 丢弃 config 段之虞。
2. `deny(unsafe_code)` 同时禁 `unsafe fn` → 所有回调必须为**安全** `extern "C" fn`；syscall 帧指针用 newtype（`AbiFrame(*const u64)` + 安全 `num()/arg()/cs()`），值传递仍 8 字节、ABI 不变。
3. **凡被 asm 触碰的静态必须与 asm 同 crate（框架）**：`FAULT_FRAME/FAULT_ADDR/FAULT_ERROR/USER_FRAME`、`SYSCALL_STACK/USER_RSP`、`CONTEXT_BASE`。间接调用用 `AtomicU64` 存 handler 地址，naked asm 以 `call qword ptr [rip + {h}]` 取用。
4. **框架 API 一律 safe-by-construction**：前置条件能用类型承载的用类型（`PhysMap{base,len}`、`PageTableRoot` 等），不能的用 proof-carrying 类型，**不向内核暴露 `unsafe fn`**。
5. IDT/GDT 构造与全部 handler 下沉框架，使内核不再需要 `#![feature(abi_x86_interrupt)]`。
6. `CONTEXT_BASE`/`USER_RSP` 改 `AtomicU64`（与 asm `mov qword ptr[rip+x]` 布局兼容）。无 `std::sync::Once` → 自卷 `AtomicU8 + UnsafeCell<MaybeUninit<T>>`。
7. 框架 crate：`#![cfg_attr(not(test), no_std)]` + `#![deny(unsafe_op_in_unsafe_fn)]`；加入 workspace members 并提交 `Cargo.lock`；bindeps 无需改。
8. `SretFrame`/`UserFrame` 下沉后加**跨 crate 编译期布局 pin**（`cosir/src/state.rs` 有逐字节镜像，字段改动会静默使细化声明失效）。

## 2. 本切片（AE / SPEC-076）范围

只做**地基**，**零调用方逻辑迁移、零行为变化**：

1. 新建 `prima-ostd` crate（暂定名，最终命名以本设计文档为准）、加入 workspace、编译到 `x86_64-unknown-none`。
2. `sync`：`StaticCell<T>`（`const new` / `get` / `get_mut` / `unsafe impl Sync`，安全用法以"单核顺序访问"文档契约承载）、`Once<T>`（`AtomicU8 + UnsafeCell<MaybeUninit<T>>`，`get_or_init`）。
3. `arch`：port I/O（`outb`/`inb`）、`rdmsr`/`wrmsr`、`read_cr3`/`write_cr3`、`read_cr2`、`invlpg`、`hlt`、`cli`/`sti`。除带不可静态验证前置条件者外均为安全 `fn`。
4. 内核 lib 与 bin 加 `#![deny(unsafe_code)]`，未收敛模块逐个 `#[allow(unsafe_code)]`（**临时**，AE–AI 逐片收缩至零）。
5. 把 `cpu.rs`/`msr.rs` 的 asm 原语实现替换为 `prima-ostd` 调用；`serial.rs`/`qemu.rs` 的 port I/O 改为框架调用（模块位置暂不动，留待后续切片）。
6. 新增 host 单测：`StaticCell`/`Once` 基本语义；框架 crate 的 `cfg_attr(not(test), no_std)` 在 host 下可测。

**不在本切片**：物理内存/页表 POD（AF/077）、全局状态 `static mut` 替换（AG/078）、裸入口与 IDT/GDT 下沉（AH/079）、强制门与 TCB 度量（AI/080）。

## 3. 回归面

预期 **QEMU 事件序列与各事件 data 逐字段不变**（本切片不改逻辑，只搬原语）。`arch` 原语实现逐指令等价（同一 `asm!` 语句）。镜像尺寸可能因新增 crate 的代码布局轻微变化——若既有事件中依赖镜像大小的字段（`memory_ready.image/kend`、`bootstrap_ready.image_len`、`frames_ready.*`、`preempt_ready.rsp*`）出现变化，**按维护规则 5 逐条登记**，不静默改写。

host 单测基线：cosir 145 / cosir_state 40 / lean_backtrace 2 / kernel 305 / xtask 167，只增不减。

## 4. 口径偏差登记

- **safe-by-construction ≠ sound**：框架把不可静态验证的前置条件（如"该物理地址是可安全读取的直映射区"）降级为**文档契约 + 类型/边界检查**，即**信任调用方**。误用可产生错误行为，但设计目标是不产生 UB。本设计**不宣称已消除全部 UB 风险**，也不宣称完成机器证明；该口径与"不变量检查不得放宽"的取舍已在此显式登记。
- 切片 AE 不减少既有 unsafe 行数（只新增信任面 crate 骨架并为后续切片铺路）；除 `cpu.rs`/`msr.rs`/`serial.rs`/`qemu.rs` 的少量调用点外，`#[allow(unsafe_code)]` 覆盖的模块在 AE 结束时仍含 unsafe。**收敛进度以 `#[allow]` 数量与 unsafe 行数为可核验指标，逐片报告。**

## 5. 验证

- `cargo test --workspace`（五组计数不下滑）
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `cargo xtask test`（QEMU BIOS run=1/2 六条 PASS；事件序列与 seq 不变）
- `cargo xtask compat`（`PASS compat cow-write-fault`）
- 事件流逐条 diff 复核改动面；`prima-ostd` 单测在 host x86-64 可跑（特权指令的真实性由 QEMU 验证）
