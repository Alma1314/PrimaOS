# 切片 AH 报告：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079）

> 文档类型：真实内核实现报告（framekernel 分层序列第 4 片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-framekernel-entry-inversion-design.md`

## 1. 交付物

把与硬件 ABI 逐字节耦合的部分全部下沉 `prima_ostd::entry`，内核改为**注册安全回调**：

| 下沉项 | 去向 | 内核侧新形态 |
|---|---|---|
| `page_fault_entry`（150 行裸 asm）+ `FAULT_FRAME`/`FAULT_ADDR`/`FAULT_ERROR`/`USER_FRAME` | `prima_ostd::entry` | `set_page_fault_handler(cow::dispatch)`；`fault_addr()`/`last_fault_err()`/`fault_frame()` 变薄转发 |
| `syscall_entry` + `CONTEXT_BASE` | 同上 | `set_syscall_handler(syscall::handle)`；`context_base() -> Option<SaveBlock>` |
| `syscall_fast_entry` + `SYSCALL_STACK`/`USER_RSP` | 同上 | `set_fast_syscall_handler(syscall::handle_fast)`；LSTAR 取 `syscall_fast_entry_addr()` |
| `timer_entry`（`preempt.rs`） | 同上 | `set_timer_handler(preempt_tick)`；`run()` 内注册 |
| `resume_landing`（bin `resume.rs`）+ `SEEN_*` | 同上 | `set_resume_handler(after_resume)`；落点/实测值经 `resume_landing_addr()`/`seen_*()` |
| `breakpoint_handler` + `InterruptStackFrame` + 用户 `int3` 收尾注册 | 同上（含 `#![feature(abi_x86_interrupt)]`） | `breakpoint_handler_addr()`；`set_user_trap_handler` 转发 |
| `lidt` / `int3` / `trigger_page_fault(_write)` | 同上 | 薄转发 |
| `lgdt` / `ltr` / `mov ds,es` | `prima_ostd::arch` | `gdt::init` 变**安全** `fn` |
| PIC/PIT `outb`、`hlt`、`transmute` 回调 | `arch` / `StaticCell<Option<fn() -> !>>` | `preempt` 局部 `unsafe fn outb` 与 `transmute` 删除 |

**ABI 不变**：四个入口改为 `call qword ptr [rip + {handler}]`（间接调用注册的函数指针）；帧指针参数用 `#[repr(transparent)]` newtype **按值**传递——`AbiFrame`（8-qword ABI 帧：`num/arg/cs`）、`SaveBlock`（20-qword 保存块：`cs/to_sret_frame`）——仍是 8 字节、仍走 `rdi`，与原先 `*const u64` 参数的寄存器/栈布局逐字节相同。`cow::dispatch` 仍收 `&SretFrame`（`rdi = &FAULT_FRAME`）。

新增注册点均先于触发：`init_idt()` 注册 #PF/syscall/fast；`preempt::run()` 注册 timer（此前 PIC 全屏蔽）；bin `run_and_exit()` 注册恢复落点。

## 2. 收敛进度

| 指标 | AG 后 | AH 后 |
|---|---|---|
| `static mut`（kernel src） | 5 | **0** |
| 模块级 `#[allow(unsafe_code)]` | lib 6 + bin 1 | **lib 0 + bin 0** |
| 内核 `#![feature(abi_x86_interrupt)]` | 有 | **删除** |
| kernel 非测试 `unsafe`（token 级） | 多处 | **0**（grep 仅命中文档注释；由 `#![deny(unsafe_code)]` 编译期强制） |
| `prima-ostd` host 单测 | 13 | **21** |

仍存的两处 `#[allow(unsafe_code)]` 都在 `#[cfg(test)] mod tests` 内（`cpu.rs:190`、`vspace.rs:502`，用 `std::alloc` 造 host 页表页的测试基建），**不属内核代码**；其处置（迁移或门禁豁免口径）留 AI。

## 3. 回归面实测

方法同 AE/AF/AG：与本片终点的上一检查点（AH 前置子步「帧类型 + 恢复跳板下沉」）逐条 diff。

**事件序列与 seq 不变（35 条）**，**5 条 data 变化**：

| 事件(seq) | AH1 | AH | 变化 |
|---|---|---|---|
| `memory_ready` (3) | `usable=30593;image=881;kend=0x1371000` | `usable=30587;image=886;kend=0x1376000` | `usable` −6、`image` +5、`kend` +5 帧 |
| `bootstrap_ready` (9) | `image_len=3605336;image_frames=881;free_frames=30593` | `image_len=3628080;image_frames=886;free_frames=30587` | `image_len` **+22,744 B**（≈22.2 KiB） |
| `preempt_ready` (25) | `rsp1=0x10000094498;rsp2=0x10000098498;kstack*_top=…44c0/…84c0` | `rsp1=0x10000090f08;rsp2=0x10000094f08;kstack*_top=…0f30/…4f30` | per-task 内核栈地址 −0x3590；`root1/root2/cr31/cr32/data1/data2/mark/ticks` **不变** |
| `frames_ready` (29) | `universe=31474;free=30483;image=881` | `universe=31473;free=30477;image=886` | `universe` −1、`free` −6、`image` +5；`segment=19`/`table=91` **不变** |
| `self_root_ready` (30) | `pages=177` | `pages=178` | 镜像 VA 页数 +1 |

其余 **30 条逐字段不变**。

**根因**：handler、四条裸入口、asm 触点与新增安全访问器全部随 crate 搬移，链接进镜像的 `prima-ostd` 代码增加；`image_len` 上报整份 ELF（debug 构建含 `.debug_*`），故 +22.2 KiB。派生量守恒自洽：

- `universe = usable_frames + image_frames` → `30593+881 = 31474`、`30587+886 = 31473`，两次都对上（−1 只因 `usable` 多降 1 帧）；
- `usable − free = 110`（= `segment 19 + table 91`）在 AH1/AH 两侧**都恰为 110**，段/页表帧数未变；
- `frames_ready.conserved=1`、`disjoint=1`、`self_root_ready.owned=1` 等校验式在事件内通过（`xtask test` 逐事件校验）。

`self_root_ready.pages` +1 是内核 ELF 的**已分配 VA 跨度**多跨一页（`.bss`/debug 段），与文件字节数增长无固定比例——故 AH1 时 +3 帧未跨页、本片跨了。**无语义变化**，按维护规则 5 登记。

**确定性**：同一镜像连跑两次，全部输出（含非 PRIMA 行）逐字节相同。

## 4. 实施中的失败与反例

1. **`sy m` 直接引用上层的编译期破坏**：删掉内核侧的 `syscall_fast_entry` 符号后，`syscall.rs:1742` 的 `lstar_ok` 仍引用 `crate::idt::syscall_fast_entry as *const () as u64`，裸 metal 构建才报 `E0425`（host 单测用的是另一套代码路径）。已改取 `prima_ostd::entry::syscall_fast_entry_addr()`。**教训**：把符号搬走后必须扫全部 `sym`/取址点——`cargo test`（host）不覆盖 `--target x86_64-unknown-none` 的 bin。
2. **快路径 CS 的隐性前置**：原 `syscall_fast_entry` 把用户 CS 硬编码为 `0x33`。下沉后改为框架静态 `FAST_SYSCALL_CS`，由 `configure_fast_path()` 写入 `gdt::USER_CS`。这引入了"注册先于 `syscall`"的新前置；本次由 `syscall_fast_ready.lstar_ok=1`/`cs=0x33` 在 QEMU 端证伪，**若漏注册则 cs 槽为 0 而非报错**——该风险已写进框架文档契约。
3. **测试计数守恒的约束**：IDT 门编码的 `HandlerFunc`（`extern "x86-interrupt" fn`）类型必须随 feature 一起下沉，故内核侧 `set_handler`/`set_handler_dpl3` 合并为地址版 `set_handler_addr`/`set_handler_addr_dpl3`。为不改动基线测试计数（kernel 305），11 个 idt 单测逐条改址保留（改名不删测）。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 305、prima-ostd **21**、xtask 167 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 零告警 |
| `cargo fmt --all --check` | 无差异 |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（35 条事件序列） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| `cargo build -p kernel --target x86_64-unknown-none` | 通过（`deny(unsafe_code)` 生效） |

QEMU 端逐入口实证：`trap_ready`（框架 breakpoint handler）、`page_fault_ready` + `vspace_ready`/`cow_ready`（框架 #PF 入口 + 注册 dispatch）、`syscall_ready`/`syscall_auth_ready`/`exc_ready`（框架 `int 0x80` 入口 + `AbiFrame` 处理器）、`syscall_fast_ready.lstar_ok=1`（框架 `syscall` 入口）、`preempt_ready`（框架 timer 入口）、`exc_resume_ready`（框架恢复落点 + 注册收尾）——**7 条入口路径全部被测**。

## 6. 口径偏差登记

1. safe-by-construction ≠ sound（同 AE/AF/AG）：回调注册是"信任调用方 + 调用次序"，框架**不**类型化地保证"注册先于触发"；未注册即触发会跳到地址 0。
2. `AbiFrame`/`SaveBlock` 的访问器（`arg(i)`）不校验 `i < 6`，索引越界会读到帧外内存——仍是文档契约。
3. `#[cfg(test)]` 的两处局部 `#[allow(unsafe_code)]` 未处置，故"内核零 unsafe"目前的严格含义是**非测试代码零 unsafe**。
4. 本片使 debug 镜像再增大 22.2 KiB，属工具链产物、非运行时膨胀；已按维护规则 5 登记。

## 7. 遗留与后续

- **AI（SPEC-080）**：处置两处测试 allow、加 xtask 零-unsafe 门禁与 `clippy -D warnings`/`fmt --check` 子命令、实现 TCB 量化指标（G3）。
- 本片不勾选任何阶段状态；闭合判断交复核者。
