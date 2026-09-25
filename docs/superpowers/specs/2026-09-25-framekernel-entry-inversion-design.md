# 切片 AH 设计：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079）

> 文档类型：真实内核实现设计（framekernel 分层序列第 4 片）
>
> 日期：2026-09-25
>
> 状态：已获用户确认（2026-09-25，方案批准：独立底层 crate / 完整序列 / 纳入 G3；AH 不拆分，一次做完）
>
> 前置：AE（SPEC-076）建 `arch`/`sync`；AF（SPEC-077）建 `mm`；AG（SPEC-078）收敛非 asm 全局状态。

## 1. 目标与范围

把**与硬件 ABI 逐字节耦合**的全部内核代码下沉到 `prima_ostd::entry`，使内核 crate（lib + bin）非测试代码 **零 `unsafe`**，并删除内核的 `#![feature(abi_x86_interrupt)]`：

1. 四个裸入口下沉：`page_fault_entry`、`syscall_entry`、`syscall_fast_entry`、`timer_entry`；bin 的 `resume_landing`。
2. 与 asm 同 crate 的静态下沉：`FAULT_FRAME`/`FAULT_ADDR`/`FAULT_ERROR`/`USER_FRAME`/`SYSCALL_STACK`/`USER_RSP`/`CONTEXT_BASE`（AG 已登记留 AH）。
3. 入口内的 `call {sym}` 改为**间接调用注册的函数指针**（`call qword ptr [rip + {handler}]`）：内核把 `extern "C" fn` 注册进框架，不再用 `sym` 直接引用上层符号。
4. IDT/GDT 的特权装载指令与 `extern "x86-interrupt"` handler（`breakpoint_handler`）下沉框架；内核不再需要 `abi_x86_interrupt` feature。

**不属于本片**（留 AI）：`#[cfg(test)]` 下 `cpu.rs`/`vspace.rs` 用 `std::alloc` 造页表页的测试基建 allow；移除残留 allow 并加 xtask 零 unsafe 门禁；TCB 量化指标（G3）。

## 2. ABI 不变论证（关键）

`deny(unsafe_code)` 同时禁 `unsafe fn`，故回调必须写成**安全** `extern "C" fn`。为保持寄存器/栈布局逐字节不变，帧指针用 `#[repr(transparent)]` newtype **按值传递**（仍是 8 字节、仍走 `rdi`）：

| 回调 | 签名（框架类型） | asm 侧实际传入 |
|---|---|---|
| `PageFaultHandler` | `extern "C" fn(&SretFrame) -> u64` | `rdi = &FAULT_FRAME`（原 `rdi = lea frame`） |
| `SyscallHandler` | `extern "C" fn(AbiFrame) -> u64` | `rdi = rsp`（8-qword ABI 现场帧） |
| `TimerHandler` | `extern "C" fn(SaveBlock, u64)` | `rdi = rbp`（20-qword 保存块）、`rsi = 进入时 rsp` |
| `ExitHandler`（用户 int3 / 恢复落点） | `extern "C" fn() -> !` | 无参 |

`AbiFrame`/`SaveBlock` 是 `#[repr(transparent)] struct X(*const u64)`，**值传递即指针传递**。安全访问器（`num`/`arg`/`cs`/`to_sret_frame`）在框架内实现（框架允许 `unsafe`，内部走 `mm::read_u64_at`），内核侧因此不再解引用裸指针。

**handler 寄存**用 `AtomicU64`（存 `fn` 地址）；naked asm 读之。注册前被触发＝跳到地址 0（逻辑问题，非 UB）；单核 boot 的调用次序保证不会发生（见 §3 注册点）。

## 3. 逐文件改造

| 文件 | 变化 |
|---|---|
| `prima-ostd/src/entry.rs` | 新增：`UserFrame`、`AbiFrame`、`SaveBlock`、5 个 handler 类型 + 注册 setter、5 个裸入口（含 `page_fault_entry` 150 行 asm 原样搬移）、`InterruptStackFrame` + `breakpoint_handler`（需 `#![feature(abi_x86_interrupt)]`）、`load_idt`、`trigger_int3`/`trigger_page_fault(_write)`、全部静态与只读访问器、各入口地址 getter |
| `prima-ostd/src/lib.rs` | 加 `#![feature(abi_x86_interrupt)]` |
| `prima-ostd/src/arch.rs` | 新增 `lgdt(base,limit)`/`ltr(sel)`/`reload_data_segments(sel)`（GDT 下沉），`lidt`/`int3`/`hlt` 已具备或补 |
| `kernel/src/idt.rs` | 删裸入口/asm 静态/`transmute`；`HandlerFunc` 改 `u64` 地址；`load_idt` 委派框架；`set_user_trap_handler`/`trap_seen`/`fault_*` 变薄包装；`init_idt` 注册 4 个 handler |
| `kernel/src/syscall.rs` | `CONTEXT_BASE`/`save_block`/`frame_from_save_block` 删（改用框架 `context_base()`/`SaveBlock::to_sret_frame()`）；`handle`/`handle_fast` 变**安全** `extern "C" fn(AbiFrame) -> u64`；`configure_fast_path` 用 `syscall_fast_entry_addr()` |
| `kernel/src/preempt.rs` | 删 `timer_entry`/本地 `outb`；`outb`→`arch::outb`、`hlt`→`arch::hlt`；`preempt_tick(SaveBlock, u64)`（安全）；`finish` 用 `StaticCell<Option<fn() -> !>>` 去 `transmute`；`run()` 注册 `TimerHandler` |
| `kernel/src/gdt.rs` | `init` 去 asm（调 `arch::lgdt/ltr/reload_data_segments`），变安全 `fn`；`init_kernel_gdt` 去 `unsafe` |
| `kernel/src/resume.rs`(bin) | 删裸 `resume_landing`/`SEEN_*`；落点地址取 `entry::resume_landing_addr()`、实测值取 `entry::seen_*()`；`run_and_exit` 注册 `ExitHandler` |
| `kernel/src/lib.rs` | 删 4 个模块 allow 与 `#![feature(abi_x86_interrupt)]` |
| `kernel/src/main.rs` | 删 `mod resume` 的 allow |

**注册点**（保证注册先于触发）：`init_idt()` 注册 page-fault/syscall/syscall-fast（此时尚未触发任何 #PF/`int 0x80`）；`preempt::run()` 注册 timer（PIC 此前全屏蔽）；bin `run_and_exit()` 注册恢复落点（紧接 resume 之前）。

`FAULT_SKIP_LEN`、`KERNEL_CS/SS`、`SYSCALL_VECTOR`、`TIMER_VECTOR`、`USER_CS` 等**纯常量**留在内核（无 unsafe）；`FAULT_FRAME` 改用 `StaticCell<SretFrame>` 承载，asm 只取其地址。

## 4. 收敛目标

AH 后：lib 模块 allow **4 → 0**、bin allow **1 → 0**、`static mut` **5 → 0**、内核 `#![feature(abi_x86_interrupt)]` 删除。lib/bin 的 `#![deny(unsafe_code)]` 由编译期强制；仅 `#[cfg(test)]` 测试基建保留两处局部 allow（AI 处置）。

## 5. 回归面

预期**事件序列与 seq 不变**（仍 35 条）。已知 AH 前置子步（帧类型 + 恢复跳板下沉）已产生并按维护规则 5 登记的变化：`memory_ready.image 878→881`、`bootstrap_ready.image_len 3593304→3605336`、`preempt_ready` rsp/kstack `+0x2b0`、`frames_ready.universe 31473→31474 / free 30485→30483`。本片把大量代码跨 crate 搬移并改间接调用，**必然**扰动镜像尺寸与栈/内核镜像边界数值，故需对全部 events 逐条 diff 并登记；序列、顺序、每事件校验式不得变化。

## 6. 验证

`cargo test --workspace`（cosir 145 / cosir_state 40 / lean_backtrace 2 / kernel 305 / xtask 167 / prima-ostd 13，只增不减）、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`、`cargo xtask test`（六条 PASS）、`cargo xtask compat`、`cargo build -p kernel --target x86_64-unknown-none`；QEMU 事件流与上一片终点逐条 diff。
